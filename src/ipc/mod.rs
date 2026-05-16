pub mod messages;

use anyhow::Result;
use messages::{DaemonCommand, DaemonResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub const SOCKET_PATH: &str = "/tmp/rpm2.sock";

pub struct IpcClient {
    stream: UnixStream,
}

impl IpcClient {
    pub async fn connect() -> Result<Self> {
        let stream = UnixStream::connect(SOCKET_PATH).await?;
        Ok(Self { stream })
    }

    pub async fn send(&mut self, cmd: DaemonCommand) -> Result<DaemonResponse> {
        let mut line = serde_json::to_string(&cmd)?;
        line.push('\n');
        self.stream.write_all(line.as_bytes()).await?;

        let mut reader = BufReader::new(&mut self.stream);
        let mut response = String::new();
        reader.read_line(&mut response).await?;

        let res = serde_json::from_str(&response)?;
        Ok(res)
    }
}
