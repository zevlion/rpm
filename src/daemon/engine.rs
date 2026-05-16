use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::ipc::{IpcCommand, IpcResponse, ProcessInfo, ProcessStatus, StartOptions};
use crate::store::Store;

pub struct ManagedProcess {
    pub info: ProcessInfo,
}

pub struct Engine {
    pub processes: HashMap<usize, ManagedProcess>,
    pub next_id: usize,
    pub store: Store,
}

impl Engine {
    pub fn new(store: Store) -> Self {
        Self {
            processes: HashMap::new(),
            next_id: 0,
            store,
        }
    }

    pub fn restore(&mut self, _engine: Arc<Mutex<Engine>>) {
        let saved = self.store.load_all().unwrap_or_default();
        for mut info in saved {
            let id = info.id;
            if id >= self.next_id {
                self.next_id = id + 1;
            }
            if info.status == ProcessStatus::Running {
                info.status = ProcessStatus::Stopped;
                info.pid = None;
            }
            self.processes
                .insert(id, ManagedProcess { info: info.clone() });
            self.store.upsert(&info).ok();
        }
        println!(
            "Restored {} process(es) from database.",
            self.processes.len()
        );
    }

    pub async fn handle(&mut self, cmd: IpcCommand, engine: Arc<Mutex<Engine>>) -> IpcResponse {
        match cmd {
            IpcCommand::Start(opts) => self.start_process(opts, engine).await,
            IpcCommand::Stop { id } => self.stop_process(id).await,
            IpcCommand::Restart { id } => self.restart_process(id, engine).await,
            IpcCommand::List => self.list_processes(),
            IpcCommand::Kill => {
                std::process::exit(0);
            }
        }
    }

    async fn start_process(
        &mut self,
        opts: StartOptions,
        engine: Arc<Mutex<Engine>>,
    ) -> IpcResponse {
        let id = self.next_id;
        self.next_id += 1;

        let info = ProcessInfo {
            id,
            name: opts.name.clone(),
            program: opts.program.clone(),
            args: opts.args.clone(),
            cwd: opts.cwd.clone(),
            interpreter: opts.interpreter.clone(),
            interpreter_args: opts.interpreter_args.clone(),
            status: ProcessStatus::Stopped,
            pid: None,
            restarts: 0,
            max_restarts: opts.max_restarts,
            no_autorestart: opts.no_autorestart,
            restart_delay: opts.restart_delay,
            kill_timeout: opts.kill_timeout,
            started_at: None,
        };

        self.processes.insert(id, ManagedProcess { info });
        self.store.upsert(&self.processes[&id].info).ok();

        match Self::spawn_process(&mut self.processes.get_mut(&id).unwrap().info).await {
            Ok(child) => {
                let info_snapshot = self.processes[&id].info.clone();
                self.store.upsert(&info_snapshot).ok();
                let name = opts.name.clone();
                Self::watch_process(id, child, engine);
                IpcResponse::Success(format!("Started '{}' → id={}", name, id))
            }
            Err(e) => {
                self.processes.get_mut(&id).unwrap().info.status = ProcessStatus::Errored;
                let info_snapshot = self.processes[&id].info.clone();
                self.store.upsert(&info_snapshot).ok();
                IpcResponse::Error(format!("Failed to start: {}", e))
            }
        }
    }

    async fn spawn_process(info: &mut ProcessInfo) -> Result<tokio::process::Child, String> {
        let (bin, leading_args) = if let Some(ref interp) = info.interpreter {
            let mut iargs: Vec<String> = info
                .interpreter_args
                .as_deref()
                .unwrap_or("")
                .split_whitespace()
                .map(String::from)
                .collect();
            iargs.push(info.program.clone());
            (interp.clone(), iargs)
        } else {
            (info.program.clone(), vec![])
        };

        let mut cmd = Command::new(&bin);
        cmd.args(&leading_args);
        cmd.args(&info.args);

        if let Some(ref cwd) = info.cwd {
            cmd.current_dir(cwd);
        }

        match cmd.spawn() {
            Ok(child) => {
                info.pid = child.id();
                info.status = ProcessStatus::Running;
                info.started_at = Some(Utc::now());
                Ok(child)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn watch_process(id: usize, mut child: tokio::process::Child, engine: Arc<Mutex<Engine>>) {
        tokio::spawn(async move {
            let _ = child.wait().await;

            let mut eng = engine.lock().await;

            let proc = match eng.processes.get_mut(&id) {
                Some(p) => p,
                None => return,
            };

            if proc.info.status == ProcessStatus::Stopped {
                return;
            }

            let no_autorestart = proc.info.no_autorestart;
            let max_restarts = proc.info.max_restarts;
            let restarts = proc.info.restarts;
            let restart_delay = proc.info.restart_delay;

            let should_restart = !no_autorestart && max_restarts.map_or(true, |max| restarts < max);

            if !should_restart {
                proc.info.status = ProcessStatus::Stopped;
                proc.info.pid = None;
                let info_snapshot = proc.info.clone();
                eng.store.upsert(&info_snapshot).ok();
                return;
            }

            proc.info.status = ProcessStatus::Restarting;
            proc.info.restarts += 1;
            proc.info.pid = None;
            let info_snapshot = proc.info.clone();
            eng.store.upsert(&info_snapshot).ok();

            drop(eng);

            if let Some(delay) = restart_delay {
                sleep(Duration::from_millis(delay)).await;
            }

            let mut eng = engine.lock().await;
            let proc = match eng.processes.get_mut(&id) {
                Some(p) => p,
                None => return,
            };

            match Self::spawn_process(&mut proc.info).await {
                Ok(new_child) => {
                    let info_snapshot = proc.info.clone();
                    eng.store.upsert(&info_snapshot).ok();
                    let engine_clone = Arc::clone(&engine);
                    drop(eng);
                    Self::watch_process(id, new_child, engine_clone);
                }
                Err(_) => {
                    if let Some(p) = eng.processes.get_mut(&id) {
                        p.info.status = ProcessStatus::Errored;
                        let info_snapshot = p.info.clone();
                        eng.store.upsert(&info_snapshot).ok();
                    }
                }
            }
        });
    }

    pub async fn stop_process(&mut self, id: usize) -> IpcResponse {
        match self.processes.get_mut(&id) {
            None => IpcResponse::Error(format!("No process with id={}", id)),
            Some(proc) => {
                let kill_timeout = proc.info.kill_timeout.unwrap_or(1600);
                let pid = proc.info.pid;
                proc.info.status = ProcessStatus::Stopped;
                proc.info.pid = None;
                let info_snapshot = proc.info.clone();
                self.store.upsert(&info_snapshot).ok();

                if let Some(pid) = pid {
                    unsafe {
                        libc::kill(pid as i32, libc::SIGTERM);
                    }
                    sleep(Duration::from_millis(kill_timeout)).await;
                    unsafe {
                        libc::kill(pid as i32, libc::SIGKILL);
                    }
                }

                IpcResponse::Success(format!("Stopped process id={}", id))
            }
        }
    }

    pub async fn restart_process(&mut self, id: usize, engine: Arc<Mutex<Engine>>) -> IpcResponse {
        self.stop_process(id).await;

        let proc = match self.processes.get_mut(&id) {
            Some(p) => p,
            None => return IpcResponse::Error(format!("No process with id={}", id)),
        };

        match Self::spawn_process(&mut proc.info).await {
            Ok(child) => {
                let info_snapshot = proc.info.clone();
                let name = info_snapshot.name.clone();
                self.store.upsert(&info_snapshot).ok();
                Self::watch_process(id, child, engine);
                IpcResponse::Success(format!("Restarted '{}' → id={}", name, id))
            }
            Err(e) => IpcResponse::Error(format!("Restart failed: {}", e)),
        }
    }

    pub fn list_processes(&self) -> IpcResponse {
        let list = self.processes.values().map(|p| p.info.clone()).collect();
        IpcResponse::StatusList(list)
    }
}
