pub mod engine;

use crate::ipc::{IpcCommand, IpcResponse, SOCKET_PATH};
use engine::Engine;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::Mutex;

pub async fn start_daemon() {
    if Path::new(SOCKET_PATH).exists() {
        let _ = fs::remove_file(SOCKET_PATH).await;
    }

    let listener = UnixListener::bind(SOCKET_PATH).expect("Failed to bind to IPC socket");
    let engine = Arc::new(Mutex::new(Engine::new()));

    println!("rpm2 daemon listening on {}", SOCKET_PATH);

    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                let engine = Arc::clone(&engine);
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    // Read until EOF for this connection
                    if stream.read_to_end(&mut buf).await.is_err() {
                        return;
                    }

                    let response = match serde_json::from_slice::<IpcCommand>(&buf) {
                        Ok(cmd) => {
                            let mut eng = engine.lock().await;
                            eng.handle(cmd).await
                        }
                        Err(e) => IpcResponse::Error(format!("Bad command: {e}")),
                    };

                    let encoded = serde_json::to_vec(&response).unwrap();
                    let _ = stream.write_all(&encoded).await;
                });
            }
            Err(e) => eprintln!("Accept error: {e}"),
        }
    }
}

