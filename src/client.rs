// src/client.rs
use crate::ipc::{IpcCommand, IpcResponse, SOCKET_PATH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

pub async fn send_command(cmd: IpcCommand) -> IpcResponse {
    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .expect("Could not connect to daemon. Is rpm2 daemon running?");

    let encoded = serde_json::to_vec(&cmd).unwrap();
    stream.write_all(&encoded).await.unwrap();
    stream.shutdown().await.unwrap(); // signal EOF so daemon knows we're done writing

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();

    serde_json::from_slice(&buf).expect("Invalid response from daemon")
}
