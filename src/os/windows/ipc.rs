use crate::ipc::messages::{DaemonCommand, DaemonResponse};
use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, PipeMode, ServerOptions};

pub const PIPE_NAME: &str = r"\\.\pipe\rpm2";

// ── client ────────────────────────────────────────────────────────────────────

type PipeClient = tokio::net::windows::named_pipe::NamedPipeClient;

pub struct IpcClient {
    reader: BufReader<ReadHalf<PipeClient>>,
    writer: WriteHalf<PipeClient>,
}

impl IpcClient {
    pub async fn connect() -> Result<Self> {
        let pipe = loop {
            match ClientOptions::new().open(PIPE_NAME) {
                Ok(p) => break p,
                Err(e) if e.raw_os_error() == Some(231) => {
                    // ERROR_PIPE_BUSY — another client has the pipe, retry
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Err(e) => return Err(e.into()),
            }
        };
        let (r, w) = tokio::io::split(pipe);
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

pub struct IpcServer;

impl IpcServer {
    pub fn bind() -> Result<Self> {
        Ok(Self)
    }

    pub async fn accept(&self) -> Result<IpcConn> {
        // Named pipes: create a new server instance for each connection.
        let server = ServerOptions::new()
            .pipe_mode(PipeMode::Byte)
            .first_pipe_instance(false)
            .create(PIPE_NAME)?;
        server.connect().await?;
        let (r, w) = tokio::io::split(server);
        Ok(IpcConn {
            reader: BufReader::new(r),
            writer: w,
        })
    }

    pub fn cleanup(&self) {
        // Named pipes are cleaned up automatically by Windows when handles close.
    }
}

pub struct IpcConn {
    reader: BufReader<ReadHalf<NamedPipeServer>>,
    writer: WriteHalf<NamedPipeServer>,
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
