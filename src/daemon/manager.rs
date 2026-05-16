use crate::process::{Process, ProcessStatus};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, broadcast};

pub struct ManagedProcess {
    pub process: Process,
    pub child: Option<Child>,
    pub started_at: Option<Instant>,
    #[allow(dead_code)]
    pub output_tx: Option<broadcast::Sender<String>>,
}

pub type ProcessMap = Arc<Mutex<HashMap<u32, ManagedProcess>>>;

pub fn new_process_map() -> ProcessMap {
    Arc::new(Mutex::new(HashMap::new()))
}

fn resolve(map: &HashMap<u32, ManagedProcess>, target: &str) -> Option<u32> {
    if let Ok(id) = target.parse::<u32>() {
        if map.contains_key(&id) {
            return Some(id);
        }
    }
    map.values()
        .find(|e| e.process.name == target)
        .map(|e| e.process.id)
}

pub async fn start(
    map: &ProcessMap,
    id: u32,
    name: String,
    cmd: String,
    args: Vec<String>,
    watching: bool,
    interpreter: Option<String>,
    attach: bool,
) -> Result<Option<broadcast::Receiver<String>>> {
    let mut command = match &interpreter {
        Some(interp) => {
            let mut c = Command::new(interp);
            c.arg(&cmd);
            c
        }
        None => Command::new(&cmd),
    };

    command.args(&args);

    let (output_tx, output_rx, child) = if attach {
        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = command.spawn()?;
        let (tx, rx) = broadcast::channel(256);

        // stream stdout
        if let Some(stdout) = child.stdout.take() {
            let tx2 = tx.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let _ = tx2.send(line);
                }
            });
        }

        // stream stderr
        if let Some(stderr) = child.stderr.take() {
            let tx2 = tx.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let _ = tx2.send(format!("[err] {}", line));
                }
            });
        }

        (Some(tx), Some(rx), child)
    } else {
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        let child = command.spawn()?;
        (None, None, child)
    };

    let pid = child.id();

    let process = Process {
        id,
        name,
        cmd,
        args,
        interpreter,
        pid,
        uptime: Duration::ZERO,
        status: ProcessStatus::Online,
        cpu: 0.0,
        mem: 0,
        watching,
        restarts: 0,
    };

    map.lock().await.insert(
        id,
        ManagedProcess {
            process,
            child: Some(child),
            started_at: Some(Instant::now()),
            output_tx,
        },
    );

    Ok(output_rx)
}

pub async fn stop(map: &ProcessMap, target: &str) -> Result<()> {
    let mut locked = map.lock().await;
    let id = resolve(&locked, target).ok_or(anyhow::anyhow!("process '{}' not found", target))?;
    let entry = locked.get_mut(&id).unwrap();
    if let Some(child) = entry.child.as_mut() {
        child.kill().await?;
    }
    entry.child = None;
    entry.process.status = ProcessStatus::Stopped;
    entry.process.pid = None;
    entry.started_at = None;
    Ok(())
}

pub async fn restart(map: &ProcessMap, id: u32) -> Result<()> {
    let (_name, cmd, args, _watching, interpreter, restarts) = {
        let mut locked = map.lock().await;
        let entry = locked
            .get_mut(&id)
            .ok_or(anyhow::anyhow!("process not found"))?;
        if let Some(child) = entry.child.as_mut() {
            child.kill().await?;
        }
        entry.child = None;
        (
            entry.process.name.clone(),
            entry.process.cmd.clone(),
            entry.process.args.clone(),
            entry.process.watching,
            entry.process.interpreter.clone(),
            entry.process.restarts,
        )
    };

    let mut command = match &interpreter {
        Some(interp) => {
            let mut c = Command::new(interp);
            c.arg(&cmd);
            c
        }
        None => Command::new(&cmd),
    };

    let child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .args(&args)
        .spawn()?;

    let pid = child.id();

    let mut locked = map.lock().await;
    if let Some(entry) = locked.get_mut(&id) {
        entry.child = Some(child);
        entry.started_at = Some(Instant::now());
        entry.process.pid = pid;
        entry.process.status = ProcessStatus::Online;
        entry.process.restarts = restarts + 1;
        entry.process.uptime = Duration::ZERO;
    }

    Ok(())
}

pub async fn restart_by_target(map: &ProcessMap, target: &str) -> Result<()> {
    let id = {
        let locked = map.lock().await;
        resolve(&locked, target).ok_or(anyhow::anyhow!("process '{}' not found", target))?
    };
    restart(map, id).await
}

pub async fn delete(map: &ProcessMap, target: &str) -> Result<()> {
    if target == "all" {
        let ids: Vec<u32> = {
            let locked = map.lock().await;
            locked.keys().cloned().collect()
        };
        for id in ids {
            let id_str = id.to_string();
            let _ = stop(map, &id_str).await;
            map.lock().await.remove(&id);
        }
        return Ok(());
    }

    let id = {
        let locked = map.lock().await;
        resolve(&locked, target).ok_or(anyhow::anyhow!("process '{}' not found", target))?
    };
    stop(map, target).await?;
    map.lock().await.remove(&id);
    Ok(())
}

pub async fn set_watch(map: &ProcessMap, target: &str, enable: bool) -> Result<()> {
    let mut locked = map.lock().await;
    let id = resolve(&locked, target).ok_or(anyhow::anyhow!("process '{}' not found", target))?;
    if let Some(entry) = locked.get_mut(&id) {
        entry.process.watching = enable;
    }
    Ok(())
}

pub async fn list(map: &ProcessMap) -> Vec<Process> {
    let locked = map.lock().await;
    let mut processes: Vec<Process> = locked.values().map(|e| e.process.clone()).collect();
    processes.sort_by_key(|p| p.id);
    processes
}
