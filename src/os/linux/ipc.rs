//! # Linux IPC — Unix Domain Socket
//!
//! Implements the three IPC types for Linux using a dynamic **Unix domain socket**
//! path scoped to the effective user ID to prevent container and environment collision.
//!
//! ## Protocol
//!
//! Both the client and server exchange **newline-delimited JSON** frames:
//!
//! ```text
//! client ──► "{...command json...}\n" ──► daemon
//! client ◄── "{...response json...}\n" ◄── daemon
//! ```
//!
//! A single connection can carry multiple request/response pairs (keep-alive).
//! The daemon reads until EOF; the client drops the connection when done.
//!
//! ## Types
//!
//! - [`IpcClient`] — used by the CLI to send a [`crate::ipc::messages::DaemonCommand`]
//!   and await a [`crate::ipc::messages::DaemonResponse`].
//! - [`IpcServer`] — used by the daemon; calls `bind()` once, then loops on `accept()`.
//! - [`IpcConn`] — per-accepted-connection handle; reads commands and writes responses.

use crate::ipc::messages::{DaemonCommand, DaemonResponse};
use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

pub fn get_socket_path() -> String {
    let uid = unsafe { libc::getuid() };
    format!("/tmp/rpm_{}.sock", uid)
}

// ── client ────────────────────────────────────────────────────────────────────

pub struct IpcClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl IpcClient {
    pub async fn connect() -> Result<Self> {
        let socket_path = get_socket_path();
        let stream = UnixStream::connect(&socket_path).await?;
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
    listener: UnixListener,
}

impl IpcServer {
    pub fn bind() -> Result<Self> {
        let socket_path = get_socket_path();
        let _ = std::fs::remove_file(&socket_path);
        Ok(Self {
            listener: UnixListener::bind(&socket_path)?,
        })
    }

    pub async fn accept(&self) -> Result<IpcConn> {
        let (stream, _) = self.listener.accept().await?;
        let (r, w) = stream.into_split();
        Ok(IpcConn {
            reader: BufReader::new(r),
            writer: w,
        })
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
