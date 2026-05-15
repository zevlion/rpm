pub mod engine;

use std::path::Path;
use tokio::fs;
use tokio::net::UnixListener;
use crate::ipc::SOCKET_PATH;

pub async fn start_daemon() {
    if Path::new(SOCKET_PATH).exists() {
        let _ = fs::remove_file(SOCKET_PATH).await;
    }

    let _listener = UnixListener::bind(SOCKET_PATH).expect("Failed to bind to IPC socket");
    println!("rpm2 daemon initialized background loop.");
}