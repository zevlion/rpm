pub mod db;
pub mod manager;
pub mod metrics;
pub mod monitor;

use crate::ipc::messages::{DaemonCommand, DaemonResponse};
use crate::os::ipc::{IpcConn, IpcServer};
use anyhow::Result;
use manager::{ProcessMap, new_process_map};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::Mutex;

static NEXT_ID: AtomicU32 = AtomicU32::new(0);

pub async fn run() -> Result<()> {
    let server = IpcServer::bind()?;
    let map = new_process_map();

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
                    process: proc,
                    child: None,
                    started_at: None,
                    output_tx: None,
                },
            );
        }
    }
    NEXT_ID.store(max_id, Ordering::SeqCst);

    let db_conn = Arc::new(Mutex::new(conn));

    tokio::spawn(monitor::run(map.clone()));
    metrics::start(map.clone());

    loop {
        match server.accept().await {
            Ok(conn) => {
                let map = map.clone();
                let db_conn = db_conn.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(conn, map, db_conn).await {
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
            attach: true,
            force,
        } = cmd
        {
            if force {
                let _ = manager::stop(&map, name).await;
            }
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            let config = manager::ProcessConfig {
                id,
                name: name.clone(),
                cmd: cmd.clone(),
                args: args.clone(),
                watching,
                interpreter: interpreter.clone(),
                attach: true,
            };

            {
                let db = db_conn.lock().await;
                let _ = db::save_process(
                    &db,
                    id,
                    &config.name,
                    &config.cmd,
                    &config.args,
                    config.watching,
                    config.interpreter.as_deref(),
                );
            }

            match manager::start(&map, config).await {
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
            continue;
        }

        let response = dispatch(cmd, &map, &db_conn).await;
        conn.write_response(response).await?;
    }

    Ok(())
}

async fn dispatch(
    cmd: DaemonCommand,
    map: &ProcessMap,
    db_conn: &Arc<Mutex<rusqlite::Connection>>,
) -> DaemonResponse {
    match cmd {
        DaemonCommand::List => DaemonResponse::ProcessList(manager::list(map).await),

        DaemonCommand::Start {
            name,
            cmd,
            args,
            watching,
            interpreter,
            attach,
            force,
        } => {
            if force {
                let _ = manager::stop(map, &name).await;
            }
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            let config = manager::ProcessConfig {
                id,
                name,
                cmd,
                args,
                watching,
                interpreter,
                attach,
            };

            {
                let db = db_conn.lock().await;
                let _ = db::save_process(
                    &db,
                    id,
                    &config.name,
                    &config.cmd,
                    &config.args,
                    config.watching,
                    config.interpreter.as_deref(),
                );
            }

            match manager::start(map, config).await {
                Ok(_) => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Err(e.to_string()),
            }
        }

        DaemonCommand::Stop { target } => match manager::stop(map, &target).await {
            Ok(_) => DaemonResponse::Ok,
            Err(e) => DaemonResponse::Err(e.to_string()),
        },

        DaemonCommand::Restart { target } => match manager::restart_by_target(map, &target).await {
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
                        .find(|e| e.process.name == target || e.process.id.to_string() == target)
                        .map(|e| e.process.id)
                };
                if let Some(process_id) = id {
                    let db = db_conn.lock().await;
                    let _ = db::remove_process(&db, process_id);
                }
            }
            match manager::delete(map, &target).await {
                Ok(_) => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Err(e.to_string()),
            }
        }

        DaemonCommand::Watch { target, enable } => {
            let id = {
                let locked = map.lock().await;
                locked
                    .values()
                    .find(|e| e.process.name == target || e.process.id.to_string() == target)
                    .map(|e| {
                        (
                            e.process.id,
                            e.process.name.clone(),
                            e.process.cmd.clone(),
                            e.process.args.clone(),
                            e.process.interpreter.clone(),
                        )
                    })
            };
            if let Some((process_id, name, cmd, args, interp)) = id {
                let db = db_conn.lock().await;
                let _ = db::save_process(
                    &db,
                    process_id,
                    &name,
                    &cmd,
                    &args,
                    enable,
                    interp.as_deref(),
                );
            }
            match manager::set_watch(map, &target, enable).await {
                Ok(_) => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Err(e.to_string()),
            }
        }

        DaemonCommand::Shutdown => {
            // cleanup is platform-specific — handled inside IpcServer
            // but we can't call it here without a reference to the server.
            // The process exit will close all handles cleanly on all platforms.
            std::process::exit(0);
        }
    }
}
