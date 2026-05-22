//! # Daemon
//!
//! The long-running background process that owns all managed child processes.
//!
//! ## Architecture
//!
//! ```text
//!  ┌─────────────────────────────────────────────────────────┐
//!  │                        daemon                           │
//!  │                                                         │
//!  │  IpcServer::accept()                                    │
//!  │        │                                                │
//!  │        ▼  (tokio::spawn per client)                     │
//!  │  handle_client() ──► dispatch()                        │
//!  │        │                  │                             │
//!  │        │           manager::{start,stop,restart,delete} │
//!  │        │                  │                             │
//!  │        │            ProcessMap (Arc<Mutex<HashMap>>)    │
//!  │        │                  │                             │
//!  │  ◄─────┘          monitor::run()  (watch / OOM)        │
//!  │                   metrics::start() (CPU / mem)         │
//!  └─────────────────────────────────────────────────────────┘
//! ```
//!
//! The daemon is started via the hidden `__daemon` CLI argument (see
//! `main.rs::ensure_daemon`).  It binds the platform IPC server, restores
//! any processes that were persisted to SQLite, then loops accepting client
//! connections and spawning a task for each one.
//!
//! Sub-modules:
//! - [`db`]      — SQLite persistence (save / load / remove processes)
//! - [`manager`] — spawn, stop, restart, delete, cluster load-balancer
//! - [`metrics`] — background task that refreshes CPU/mem via `sysinfo`
//! - [`monitor`] — background task that detects crashes and enforces OOM limits
pub mod db;
pub mod manager;
pub mod metrics;
pub mod monitor;

use crate::ipc::messages::{DaemonCommand, DaemonResponse};
use crate::os::ipc::{IpcConn, IpcServer};
use anyhow::Result;
use manager::{ProcessMap, LoadBalancerMap, new_process_map};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::collections::HashMap;
use tokio::sync::Mutex;

static NEXT_ID: AtomicU32 = AtomicU32::new(0);

pub fn next_id() -> u32 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

pub fn reset_id_counter() {
    NEXT_ID.store(0, Ordering::SeqCst);
}

pub async fn run() -> Result<()> {
    let server = IpcServer::bind()?;
    let map = new_process_map();
    let lb_map: LoadBalancerMap = Arc::new(Mutex::new(HashMap::new()));

    let conn = db::init_db()?;
    let saved_processes = db::load_processes(&conn)?;
    let mut max_id = 0;

    {
        let mut locked_map = map.lock().await;
        for proc in saved_processes {
            if proc.id >= max_id {
                max_id = proc.id + 1;
            }
            locked_map.insert(
                proc.id,
                manager::ManagedProcess {
                    app_name: proc.name.clone(),
                    process: proc,
                    child: None,
                    started_at: None,
                    output_tx: None,
                    internal_port: None,
                    max_memory: None,
                    max_cpu: None,
                },
            );
        }
    }
    NEXT_ID.store(max_id, Ordering::SeqCst);

    let db_conn = Arc::new(Mutex::new(conn));

    tokio::spawn(monitor::run(map.clone(), lb_map.clone()));
    metrics::start(map.clone());

    loop {
        match server.accept().await {
            Ok(conn) => {
                let map = map.clone();
                let db_conn = db_conn.clone();
                let lb_map = lb_map.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(conn, map, db_conn, lb_map).await {
                        eprintln!("[daemon] client error: {}", e);
                    }
                });
            }
            Err(e) => eprintln!("[daemon] accept error: {}", e),
        }
    }
}

async fn handle_client(
    mut conn: IpcConn,
    map: ProcessMap,
    db_conn: Arc<Mutex<rusqlite::Connection>>,
    lb_map: LoadBalancerMap,
) -> Result<()> {
    loop {
        let cmd = match conn.read_command().await {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(e) => {
                conn.write_response(DaemonResponse::Err(e.to_string()))
                    .await?;
                continue;
            }
        };

        if let DaemonCommand::Start {
            ref name,
            ref cmd,
            ref args,
            watching,
            ref interpreter,
            attach,
            force,
            ref mode,
            instances,
            port,
            ref lb_strategy,
            max_memory,
            max_cpu,
        } = cmd
        {
            let existing_id = {
                let locked = map.lock().await;
                locked.values()
                    .find(|e| e.app_name == *name || e.process.name == *name)
                    .map(|e| e.process.id)
            };

            if let Some(id) = existing_id {
                let is_online = {
                    let locked = map.lock().await;
                    locked.get(&id).map(|e| e.process.status == crate::process::ProcessStatus::Online).unwrap_or(false)
                };
                if is_online && !force {
                    conn.write_response(DaemonResponse::Err(format!("Process '{}' is already running. Use --force to restart.", name)))
                        .await?;
                    continue;
                }
                // Completely delete the old process from memory map so we don't leak cluster workers
                let _ = manager::delete(&map, &lb_map, name).await;
            }

            let id = existing_id.unwrap_or_else(next_id);
            let config = manager::ProcessConfig {
                id,
                name: name.clone(),
                cmd: cmd.clone(),
                args: args.clone(),
                watching,
                interpreter: interpreter.clone(),
                attach,
                mode: mode.clone().unwrap_or_else(|| "fork".to_string()),
                instances: instances.unwrap_or(1),
                port,
                lb_strategy: lb_strategy.clone().unwrap_or_else(|| "round-robin".to_string()),
                max_memory,
                max_cpu,
            };

            {
                let db = db_conn.lock().await;
                let _ = db::save_process(&db, &config.to_process());
            }

            if attach {
                match manager::start(&map, config, &lb_map).await {
                    Ok(Some(mut rx)) => {
                        conn.write_response(DaemonResponse::Ok).await?;
                        while let Ok(line) = rx.recv().await {
                            conn.write_response(DaemonResponse::Line(line)).await?;
                        }
                        conn.write_response(DaemonResponse::Eof).await?;
                    }
                    Ok(None) => conn.write_response(DaemonResponse::Ok).await?,
                    Err(e) => {
                        conn.write_response(DaemonResponse::Err(e.to_string()))
                            .await?
                    }
                }
            } else {
                match manager::start(&map, config, &lb_map).await {
                    Ok(_) => conn.write_response(DaemonResponse::Ok).await?,
                    Err(e) => conn.write_response(DaemonResponse::Err(e.to_string())).await?,
                }
            }
            continue;
        }

        let response = dispatch(cmd, &map, &db_conn, &lb_map).await;
        conn.write_response(response).await?;
    }

    Ok(())
}

async fn dispatch(
    cmd: DaemonCommand,
    map: &ProcessMap,
    db_conn: &Arc<Mutex<rusqlite::Connection>>,
    lb_map: &LoadBalancerMap,
) -> DaemonResponse {
    match cmd {
        DaemonCommand::List => DaemonResponse::ProcessList(manager::list(map).await),

        DaemonCommand::Start { .. } => {
            DaemonResponse::Err("Start command should be handled by handle_client".to_string())
        }

        DaemonCommand::Stop { target } => match manager::stop(map, lb_map, &target).await {
            Ok(_) => DaemonResponse::Ok,
            Err(e) => DaemonResponse::Err(e.to_string()),
        },

        DaemonCommand::Restart { target } => match manager::restart_by_target(map, lb_map, &target).await {
            Ok(_) => DaemonResponse::Ok,
            Err(e) => DaemonResponse::Err(e.to_string()),
        },

        DaemonCommand::Delete { target } => {
            if target == "all" {
                let db = db_conn.lock().await;
                let _ = db::clear_all(&db);
            } else {
                let id = {
                    let locked = map.lock().await;
                    locked
                        .values()
                        .find(|e| e.app_name == target || e.process.name == target || e.process.id.to_string() == target)
                        .map(|e| e.process.id)
                };
                if let Some(process_id) = id {
                    let db = db_conn.lock().await;
                    let _ = db::remove_process(&db, process_id);
                }
            }
            match manager::delete(map, lb_map, &target).await {
                Ok(_) => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Err(e.to_string()),
            }
        }

        DaemonCommand::Watch { target, enable } => {
            match manager::set_watch(map, &target, enable).await {
                Ok(process) => {
                    let db = db_conn.lock().await;
                    let _ = db::save_process(&db, &process);
                    DaemonResponse::Ok
                }
                Err(e) => DaemonResponse::Err(e.to_string()),
            }
        }

        DaemonCommand::Shutdown => {
            std::process::exit(0);
        }
    }
}
