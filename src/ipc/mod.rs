pub mod messages;

use anyhow::Result;
use messages::{DaemonCommand, DaemonResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub const SOCKET_PATH: &str = "/tmp/rpm2.sock";

pub struct IpcClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl IpcClient {
    pub async fn connect() -> Result<Self> {
        let stream = UnixStream::connect(SOCKET_PATH).await?;
        let (read_half, write_half) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(read_half),
            writer: write_half,
        })
    }

    pub async fn send(&mut self, cmd: DaemonCommand) -> Result<DaemonResponse> {
        let mut line = serde_json::to_string(&cmd)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        self.recv().await
    }

    pub async fn recv(&mut self) -> Result<DaemonResponse> {
        let mut response = String::new();
        self.reader.read_line(&mut response).await?;
        if response.is_empty() {
            anyhow::bail!("daemon closed connection unexpectedly");
        }
        Ok(serde_json::from_str(response.trim())?)
    }
}

