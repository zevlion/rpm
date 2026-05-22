//! # Process Manager
//!
//! Owns all runtime state of managed processes and exposes async functions
//! to start, stop, restart, and delete them.
//!
//! ## Key types
//!
//! | Type | Description |
//! |------|-------------|
//! | [`ManagedProcess`] | Live entry in the in-memory map — holds the [`tokio::process::Child`] handle, timing info, and a broadcast channel for attached stdout. |
//! | [`ProcessConfig`] | Snapshot of user-supplied options used to (re-)spawn a process. Converted to [`crate::process::Process`] via [`ProcessConfig::to_process`]. |
//! | [`ProcessMap`] | `Arc<Mutex<HashMap<u32, ManagedProcess>>>` — the central registry shared across every async task. |
//! | [`LoadBalancerMap`] | `Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>` — one entry per cluster app; sending on the channel shuts the load-balancer task down. |
//!
//! ## Cluster mode
//!
//! When `mode == "cluster"` and a `port` is given, `start()` spawns *N*
//! worker instances each listening on a private ephemeral port (`PORT` env
//! var), then launches [`start_load_balancer`] which accepts public TCP
//! connections and proxies them to workers using either **round-robin**
//! (default) or **least-loaded** (by CPU + memory score) strategy.
//!
//! ```text
//!  Internet ──► :public_port (load balancer)
//!                    │ round-robin / least-loaded
//!          ┌─────────┼─────────┐
//!          ▼         ▼         ▼
//!       worker-0  worker-1  worker-2
//!      :ephemeral :ephemeral :ephemeral
//! ```
use crate::process::{Process, ProcessStatus};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, broadcast};
use tokio::net::{TcpListener, TcpStream};

pub struct ManagedProcess {
    pub process: Process,
    pub child: Option<Child>,
    pub started_at: Option<Instant>,
    #[allow(dead_code)]
    pub output_tx: Option<broadcast::Sender<String>>,
    pub app_name: String,
    pub internal_port: Option<u16>,
    pub max_memory: Option<u64>,
    #[allow(dead_code)]
    pub max_cpu: Option<f32>,
}

pub struct ProcessConfig {
    pub id: u32,
    pub name: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub watching: bool,
    pub interpreter: Option<String>,
    pub attach: bool,
    pub mode: String,
    pub instances: u32,
    pub port: Option<u16>,
    pub lb_strategy: String,
    pub max_memory: Option<u64>,
    pub max_cpu: Option<f32>,
}

impl ProcessConfig {
    pub fn to_process(&self) -> Process {
        Process {
            id: self.id,
            name: self.name.clone(),
            cmd: self.cmd.clone(),
            args: self.args.clone(),
            interpreter: self.interpreter.clone(),
            pid: None,
            uptime: Duration::ZERO,
            status: ProcessStatus::Stopped,
            cpu: 0.0,
            mem: 0,
            watching: self.watching,
            restarts: 0,
            mode: self.mode.clone(),
            instances: self.instances,
            port: self.port,
            lb_strategy: Some(self.lb_strategy.clone()),
            max_memory: self.max_memory,
            max_cpu: self.max_cpu,
        }
    }
}

pub type ProcessMap = Arc<Mutex<HashMap<u32, ManagedProcess>>>;
pub type LoadBalancerMap = Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>>;

pub fn new_process_map() -> ProcessMap {
    Arc::new(Mutex::new(HashMap::new()))
}

fn resolve(map: &HashMap<u32, ManagedProcess>, target: &str) -> Option<u32> {
    if let Ok(id) = target.parse::<u32>()
        && map.contains_key(&id)
    {
        return Some(id);
    }
    map.values()
        .find(|e| e.app_name == target || e.process.name == target)
        .map(|e| e.process.id)
}

fn find_free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|listener| listener.local_addr().ok())
        .map(|addr| addr.port())
}

pub async fn start(
    map: &ProcessMap,
    config: ProcessConfig,
    lb_map: &LoadBalancerMap,
) -> Result<Option<broadcast::Receiver<String>>> {
    let (output_tx, output_rx) = if config.attach {
        let (tx, rx) = broadcast::channel(256);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    let instances = config.instances.max(1);

    for i in 0..instances {
        let worker_id = if i == 0 {
            config.id
        } else {
            super::next_id()
        };

        let worker_name = if instances == 1 {
            config.name.clone()
        } else {
            format!("{}-{}", config.name, i)
        };

        let internal_port = if config.mode == "cluster" && config.port.is_some() {
            find_free_port()
        } else {
            None
        };

        let mut command = match &config.interpreter {
            Some(interp) => {
                let mut c = Command::new(interp);
                c.arg(&config.cmd);
                c
            }
            None => Command::new(&config.cmd),
        };

        command.args(&config.args);

        if config.attach {
            command
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
        } else {
            command
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
        }

        #[cfg(unix)]
        command.process_group(0);

        if let Some(port) = internal_port {
            command.env("PORT", port.to_string());
        }

        let mut child = command.spawn()?;
        let pid = child.id();

        if config.attach {
            if let Some(stdout) = child.stdout.take() {
                let tx2 = output_tx.as_ref().unwrap().clone();
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        let _ = tx2.send(line);
                    }
                });
            }

            if let Some(stderr) = child.stderr.take() {
                let tx2 = output_tx.as_ref().unwrap().clone();
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        let _ = tx2.send(format!("[err] {}", line));
                    }
                });
            }
        }

        let process = Process {
            id: worker_id,
            name: worker_name,
            cmd: config.cmd.clone(),
            args: config.args.clone(),
            interpreter: config.interpreter.clone(),
            pid,
            uptime: Duration::ZERO,
            status: ProcessStatus::Online,
            cpu: 0.0,
            mem: 0,
            watching: config.watching,
            restarts: 0,
            mode: config.mode.clone(),
            instances: config.instances,
            port: config.port,
            lb_strategy: Some(config.lb_strategy.clone()),
            max_memory: config.max_memory,
            max_cpu: config.max_cpu,
        };

        map.lock().await.insert(
            worker_id,
            ManagedProcess {
                process,
                child: Some(child),
                started_at: Some(Instant::now()),
                output_tx: output_tx.clone(),
                app_name: config.name.clone(),
                internal_port,
                max_memory: config.max_memory,
                max_cpu: config.max_cpu,
            },
        );
    }

    if config.mode == "cluster" && let Some(pub_port) = config.port {
        let mut lb_locked = lb_map.lock().await;
        if let std::collections::hash_map::Entry::Vacant(e) = lb_locked.entry(config.name.clone()) {
            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
            tokio::spawn(start_load_balancer(
                config.name,
                pub_port,
                config.lb_strategy,
                map.clone(),
                shutdown_rx,
            ));
            e.insert(shutdown_tx);
        }
    }

    Ok(output_rx)
}

pub async fn stop(
    map: &ProcessMap,
    lb_map: &LoadBalancerMap,
    target: &str,
) -> Result<()> {
    let app_workers: Vec<u32> = {
        let locked = map.lock().await;
        if target == "all" {
            locked.keys().cloned().collect()
        } else if let Ok(id) = target.parse::<u32>() {
            if locked.contains_key(&id) {
                vec![id]
            } else {
                vec![]
            }
        } else {
            locked
                .values()
                .filter(|e| e.app_name == target || e.process.name == target)
                .map(|e| e.process.id)
                .collect()
        }
    };

    for id in &app_workers {
        let mut locked = map.lock().await;
        if let Some(entry) = locked.get_mut(id) {
            if let Some(child) = entry.child.as_mut() {
                let _ = child.kill().await;
            }
            entry.child = None;
            entry.process.status = ProcessStatus::Stopped;
            entry.process.pid = None;
            entry.process.cpu = 0.0;
            entry.process.mem = 0;
            entry.started_at = None;
        }
    }

    let app_names: Vec<String> = {
        let locked = map.lock().await;
        app_workers
            .iter()
            .filter_map(|id| locked.get(id).map(|e| e.app_name.clone()))
            .collect()
    };
    let mut unique_app_names = app_names;
    unique_app_names.sort();
    unique_app_names.dedup();

    for name in unique_app_names {
        let any_running = {
            let locked = map.lock().await;
            locked
                .values()
                .any(|e| e.app_name == name && e.process.status == ProcessStatus::Online)
        };
        if !any_running {
            let mut lb_locked = lb_map.lock().await;
            if let Some(shutdown_tx) = lb_locked.remove(&name) {
                let _ = shutdown_tx.send(());
            }
        }
    }

    Ok(())
}

pub async fn restart(
    map: &ProcessMap,
    lb_map: &LoadBalancerMap,
    id: u32,
) -> Result<()> {
    let (cmd, args, interpreter, restarts, app_name, internal_port, pub_port, lb_strategy, mode) = {
        let mut locked = map.lock().await;
        let entry = locked
            .get_mut(&id)
            .ok_or(anyhow::anyhow!("process not found"))?;
        if let Some(child) = entry.child.as_mut() {
            let _ = child.kill().await;
        }
        entry.child = None;
        (
            entry.process.cmd.clone(),
            entry.process.args.clone(),
            entry.process.interpreter.clone(),
            entry.process.restarts,
            entry.app_name.clone(),
            entry.internal_port,
            entry.process.port,
            entry.process.lb_strategy.clone(),
            entry.process.mode.clone(),
        )
    };

    let port_to_use = if mode == "cluster" && pub_port.is_some() {
        internal_port.or_else(find_free_port)
    } else {
        None
    };

    let mut command = match &interpreter {
        Some(interp) => {
            let mut c = Command::new(interp);
            c.arg(&cmd);
            c
        }
        None => Command::new(&cmd),
    };

    command.args(&args);
    command.stdin(std::process::Stdio::null())
           .stdout(std::process::Stdio::null())
           .stderr(std::process::Stdio::null());

    if let Some(port) = port_to_use {
        command.env("PORT", port.to_string());
    }

    #[cfg(unix)]
    command.process_group(0);

    let child = command.spawn()?;
    let pid = child.id();

    let mut locked = map.lock().await;
    if let Some(entry) = locked.get_mut(&id) {
        entry.child = Some(child);
        entry.started_at = Some(Instant::now());
        entry.process.pid = pid;
        entry.process.status = ProcessStatus::Online;
        entry.process.restarts = restarts + 1;
        entry.process.uptime = Duration::ZERO;
        entry.internal_port = port_to_use;
    }

    if mode == "cluster" && let Some(pub_port_val) = pub_port {
        let mut lb_locked = lb_map.lock().await;
        if let std::collections::hash_map::Entry::Vacant(e) = lb_locked.entry(app_name.clone()) {
            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
            tokio::spawn(start_load_balancer(
                app_name,
                pub_port_val,
                lb_strategy.unwrap_or_else(|| "round-robin".to_string()),
                map.clone(),
                shutdown_rx,
            ));
            e.insert(shutdown_tx);
        }
    }

    Ok(())
}

pub async fn restart_by_target(
    map: &ProcessMap,
    lb_map: &LoadBalancerMap,
    target: &str,
) -> Result<()> {
    let worker_ids: Vec<u32> = {
        let locked = map.lock().await;
        if target == "all" {
            locked.keys().cloned().collect()
        } else if let Ok(id) = target.parse::<u32>() {
            if locked.contains_key(&id) {
                vec![id]
            } else {
                vec![]
            }
        } else {
            locked
                .values()
                .filter(|e| e.app_name == target || e.process.name == target)
                .map(|e| e.process.id)
                .collect()
        }
    };

    if worker_ids.is_empty() {
        anyhow::bail!("process '{}' not found", target);
    }

    for id in worker_ids {
        restart(map, lb_map, id).await?;
    }
    Ok(())
}

pub async fn delete(
    map: &ProcessMap,
    lb_map: &LoadBalancerMap,
    target: &str,
) -> Result<()> {
    if target == "all" {
        stop(map, lb_map, "all").await?;
        map.lock().await.clear();
        super::reset_id_counter();
        return Ok(());
    }

    stop(map, lb_map, target).await?;

    let to_remove: Vec<u32> = {
        let locked = map.lock().await;
        if let Ok(id) = target.parse::<u32>() {
            if locked.contains_key(&id) {
                vec![id]
            } else {
                vec![]
            }
        } else {
            locked
                .values()
                .filter(|e| e.app_name == target || e.process.name == target)
                .map(|e| e.process.id)
                .collect()
        }
    };

    let mut locked = map.lock().await;
    for id in to_remove {
        locked.remove(&id);
    }

    if locked.is_empty() {
        super::reset_id_counter();
    }
    Ok(())
}

pub async fn set_watch(map: &ProcessMap, target: &str, enable: bool) -> Result<Process> {
    let mut locked = map.lock().await;
    let id = resolve(&locked, target).ok_or(anyhow::anyhow!("process '{}' not found", target))?;
    
    let app_name = locked.get(&id).map(|e| e.app_name.clone()).unwrap();
    for entry in locked.values_mut() {
        if entry.app_name == app_name {
            entry.process.watching = enable;
        }
    }

    let entry = locked.get(&id).ok_or(anyhow::anyhow!("process '{}' not found", target))?;
    Ok(entry.process.clone())
}

pub async fn list(map: &ProcessMap) -> Vec<Process> {
    let locked = map.lock().await;
    let mut processes: Vec<Process> = locked.values().map(|e| e.process.clone()).collect();
    processes.sort_by_key(|p| p.id);
    processes
}

pub async fn start_load_balancer(
    app_name: String,
    public_port: u16,
    strategy: String,
    process_map: ProcessMap,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let listener = match TcpListener::bind(format!("0.0.0.0:{}", public_port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[daemon] Failed to bind load balancer for '{}' to port {}: {}", app_name, public_port, e);
            return;
        }
    };
    println!("[daemon] Load balancer for '{}' listening on port {}", app_name, public_port);

    let mut rr_index = 0usize;

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                println!("[daemon] Shutting down load balancer for '{}'", app_name);
                break;
            }
            conn_res = listener.accept() => {
                let (client_socket, _) = match conn_res {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[daemon] Load balancer accept error: {}", e);
                        continue;
                    }
                };

                let workers = get_worker_ports(&process_map, &app_name).await;
                if workers.is_empty() {
                    eprintln!("[daemon] No active workers for '{}' to handle connection", app_name);
                    continue;
                }

                let target_port = if strategy == "least-loaded" {
                    let mut chosen_port = workers[0].1;
                    let mut min_score = f32::MAX;
                    for (_id, port, cpu, mem) in &workers {
                        let mem_mb = (*mem as f32) / 1024.0 / 1024.0;
                        let score = *cpu + mem_mb / 10.0;
                        if score < min_score {
                            min_score = score;
                            chosen_port = *port;
                        }
                    }
                    chosen_port
                } else {
                    let worker = &workers[rr_index % workers.len()];
                    rr_index = rr_index.wrapping_add(1);
                    worker.1
                };

                tokio::spawn(async move {
                    let _ = proxy_connection(client_socket, target_port).await;
                });
            }
        }
    }
}

async fn proxy_connection(mut client_socket: TcpStream, target_port: u16) -> Result<()> {
    let mut server_socket = TcpStream::connect(format!("127.0.0.1:{}", target_port)).await?;
    tokio::io::copy_bidirectional(&mut client_socket, &mut server_socket).await?;
    Ok(())
}

async fn get_worker_ports(map: &ProcessMap, app_name: &str) -> Vec<(u32, u16, f32, u64)> {
    let locked = map.lock().await;
    locked
        .values()
        .filter(|e| e.process.status == ProcessStatus::Online && e.app_name == app_name)
        .filter_map(|e| e.internal_port.map(|port| (e.process.id, port, e.process.cpu, e.process.mem)))
        .collect()
}
