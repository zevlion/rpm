pub mod manager;
pub mod monitor;

use crate::ipc::SOCKET_PATH;
use crate::ipc::messages::{DaemonCommand, DaemonResponse};
use anyhow::Result;
use manager::{ProcessMap, new_process_map};
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
        let (stream, _) = listener.accept().await?;
        let map = map.clone();
        tokio::spawn(handle_client(stream, map));
    }
}

async fn handle_client(stream: UnixStream, map: ProcessMap) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }

        let cmd: DaemonCommand = match serde_json::from_str(line.trim()) {
            Ok(c) => c,
            Err(e) => {
                let _ = write_response(&mut writer, DaemonResponse::Err(e.to_string())).await;
                continue;
            }
        };

        let response = dispatch(cmd, &map).await;
        if write_response(&mut writer, response).await.is_err() {
            break;
        }
    }
}

async fn dispatch(cmd: DaemonCommand, map: &ProcessMap) -> DaemonResponse {
    match cmd {
        DaemonCommand::List => DaemonResponse::ProcessList(manager::list(map).await),

        DaemonCommand::Start {
            name,
            cmd,
            args,
            watching,
            interpreter,
            attach: _,
        } => {
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            match manager::start(map, id, name, cmd, args, watching, interpreter).await {
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

        DaemonCommand::Delete { target } => match manager::delete(map, &target).await {
            Ok(_) => DaemonResponse::Ok,
            Err(e) => DaemonResponse::Err(e.to_string()),
        },

        DaemonCommand::Watch { target, enable } => {
            match manager::set_watch(map, &target, enable).await {
                Ok(_) => DaemonResponse::Ok,
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
    writer: &mut (impl AsyncWriteExt + Unpin),
    res: DaemonResponse,
) -> anyhow::Result<()> {
    let mut line = serde_json::to_string(&res)?;
    line.push('\n');
    writer.write_all(line.as_bytes()).await?;
    Ok(())
}
