use crate::ipc::messages::{DaemonCommand, DaemonResponse};
use anyhow::Result;
use std::os::unix::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub const SOCKET_NAME: &[u8] = b"\x00rpm2.sock";

fn abstract_addr() -> Result<SocketAddr> {
    Ok(SocketAddr::from_abstract_name(SOCKET_NAME)?)
}

// ── client ────────────────────────────────────────────────────────────────────

pub struct IpcClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl IpcClient {
    pub async fn connect() -> Result<Self> {
        let std_stream = std::os::unix::net::UnixStream::connect_addr(&abstract_addr()?)?;
        std_stream.set_nonblocking(true)?;
        let stream = UnixStream::from_std(std_stream)?;
        let (r, w) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(r),
            writer: w,
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

// ── server ────────────────────────────────────────────────────────────────────

pub struct IpcServer {
    listener: tokio::net::UnixListener,
}

impl IpcServer {
    pub fn bind() -> Result<Self> {
        let std_listener = std::os::unix::net::UnixListener::bind_addr(&abstract_addr()?)?;
        std_listener.set_nonblocking(true)?;
        let listener = tokio::net::UnixListener::from_std(std_listener)?;
        Ok(Self { listener })
    }

    pub async fn accept(&self) -> Result<IpcConn> {
        let (stream, _) = self.listener.accept().await?;
        let (r, w) = stream.into_split();
        Ok(IpcConn {
            reader: BufReader::new(r),
            writer: w,
        })
    }

    pub fn cleanup(&self) {
        // Abstract namespace sockets vanish when the last handle closes — nothing to do.
    }
}

pub struct IpcConn {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl IpcConn {
    pub async fn read_command(&mut self) -> Result<Option<DaemonCommand>> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(trimmed)?))
    }

    pub async fn write_response(&mut self, res: DaemonResponse) -> Result<()> {
        let mut line = serde_json::to_string(&res)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }
}
