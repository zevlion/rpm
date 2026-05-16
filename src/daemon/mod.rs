pub mod manager;
pub mod monitor;

use crate::ipc::messages::{DaemonCommand, DaemonResponse};
use crate::ipc::SOCKET_PATH;
use anyhow::Result;
use manager::{new_process_map, ProcessMap};
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

static NEXT_ID: AtomicU32 = AtomicU32::new(0);

pub async fn run() -> Result<()> {
    let _ = std::fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH)?;
    let map = new_process_map();

    tokio::spawn(monitor::run(map.clone()));

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let map = map.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, map).await {
                        eprintln!("[daemon] client error: {}", e);
                    }
                });
            }
            Err(e) => eprintln!("[daemon] accept error: {}", e),
        }
    }
}

async fn handle_client(stream: UnixStream, map: ProcessMap) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 { break; }

        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        let cmd: DaemonCommand = match serde_json::from_str(trimmed) {
            Ok(c) => c,
            Err(e) => {
                write_response(&mut writer, DaemonResponse::Err(e.to_string())).await?;
                continue;
            }
        };

        // special case: start with attach streams lines back
        if let DaemonCommand::Start { ref name, ref cmd, ref args, watching, ref interpreter, attach: true } = cmd {
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            match manager::start(
                &map, id,
                name.clone(), cmd.clone(), args.clone(),
                watching, interpreter.clone(), true,
            ).await {
                Ok(Some(mut rx)) => {
                    write_response(&mut writer, DaemonResponse::Ok).await?;
                    // stream lines until channel closes
                    while let Ok(line) = rx.recv().await {
                        write_response(&mut writer, DaemonResponse::Line(line)).await?;
                    }
                    write_response(&mut writer, DaemonResponse::Eof).await?;
                }
                Ok(None) => {
                    write_response(&mut writer, DaemonResponse::Ok).await?;
                }
                Err(e) => {
                    write_response(&mut writer, DaemonResponse::Err(e.to_string())).await?;
                }
            }
            continue;
        }

        let response = dispatch(cmd, &map).await;
        write_response(&mut writer, response).await?;
    }

    Ok(())
}

async fn dispatch(cmd: DaemonCommand, map: &ProcessMap) -> DaemonResponse {
    match cmd {
        DaemonCommand::List => {
            DaemonResponse::ProcessList(manager::list(map).await)
        }

        DaemonCommand::Start { name, cmd, args, watching, interpreter, attach } => {
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            match manager::start(map, id, name, cmd, args, watching, interpreter, attach).await {
                Ok(_)  => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Err(e.to_string()),
            }
        }

        DaemonCommand::Stop { target } => {
            match manager::stop(map, &target).await {
                Ok(_)  => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Err(e.to_string()),
            }
        }

        DaemonCommand::Restart { target } => {
            match manager::restart_by_target(map, &target).await {
                Ok(_)  => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Err(e.to_string()),
            }
        }

        DaemonCommand::Delete { target } => {
            match manager::delete(map, &target).await {
                Ok(_)  => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Err(e.to_string()),
            }
        }

        DaemonCommand::Watch { target, enable } => {
            match manager::set_watch(map, &target, enable).await {
                Ok(_)  => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Err(e.to_string()),
            }
        }

        DaemonCommand::Shutdown => {
            let _ = std::fs::remove_file(SOCKET_PATH);
            std::process::exit(0);
        }
    }
}

async fn write_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    res: DaemonResponse,
) -> Result<()> {
    let mut line = serde_json::to_string(&res)?;
    line.push('\n');
    writer.write_all(line.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}
