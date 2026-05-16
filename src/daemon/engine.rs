use crate::ipc::{IpcCommand, IpcResponse, ProcessInfo};
use std::collections::HashMap;
use tokio::process::Command;

pub struct Engine {
    pub processes: HashMap<usize, ManagedProcess>,
    pub next_id: usize,
}

pub struct ManagedProcess {
    pub info: ProcessInfo,
    // Store the child handle so we can kill it later
    pub child: Option<tokio::process::Child>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
            next_id: 0,
        }
    }

    pub async fn handle(&mut self, cmd: IpcCommand) -> IpcResponse {
        match cmd {
            IpcCommand::Start {
                program,
                name,
                args,
            } => self.start_process(program, name, args).await,
            IpcCommand::List => self.list_processes(),
            IpcCommand::Stop { id } => self.stop_process(id).await,
        }
    }

    async fn start_process(
        &mut self,
        program: String,
        name: String,
        args: Vec<String>,
    ) -> IpcResponse {
        let id = self.next_id;
        self.next_id += 1;

        match Command::new(&program).args(&args).spawn() {
            Ok(child) => {
                let pid = child.id();
                let info = ProcessInfo {
                    id,
                    name: name.clone(),
                    pid,
                    status: "running".into(),
                    uptime: None, // add chrono::Utc::now() tracking here later
                };
                self.processes.insert(
                    id,
                    ManagedProcess {
                        info,
                        child: Some(child),
                    },
                );
                IpcResponse::Success(format!("Started '{}' with id={}", name, id))
            }
            Err(e) => IpcResponse::Error(format!("Failed to start '{}': {}", program, e)),
        }
    }

    fn list_processes(&self) -> IpcResponse {
        let list = self.processes.values().map(|p| p.info.clone()).collect();
        IpcResponse::StatusList(list)
    }

    async fn stop_process(&mut self, id: usize) -> IpcResponse {
        match self.processes.get_mut(&id) {
            Some(proc) => {
                if let Some(child) = proc.child.as_mut() {
                    let _ = child.kill().await;
                }
                proc.info.status = "stopped".into();
                proc.info.pid = None;
                IpcResponse::Success(format!("Stopped process id={}", id))
            }
            None => IpcResponse::Error(format!("No process with id={}", id)),
        }
    }
}
