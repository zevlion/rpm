use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const SOCKET_PATH: &str = "/tmp/rpm2.sock";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StartOptions {
    pub program: String,
    pub name: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub interpreter: Option<String>,
    pub interpreter_args: Option<String>,
    pub max_restarts: Option<u32>,
    pub restart_delay: Option<u64>,
    pub no_autorestart: bool,
    pub kill_timeout: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum IpcCommand {
    Start(StartOptions),
    Stop { id: usize },
    Restart { id: usize },
    List,
    Kill,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum IpcResponse {
    Success(String),
    StatusList(Vec<ProcessInfo>),
    Error(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProcessInfo {
    pub id: usize,
    pub name: String,
    pub pid: Option<u32>,
    pub status: ProcessStatus,
    pub restarts: u32,
    pub max_restarts: Option<u32>,
    pub no_autorestart: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub interpreter: Option<String>,
    pub interpreter_args: Option<String>,
    pub restart_delay: Option<u64>,
    pub kill_timeout: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ProcessStatus {
    Running,
    Stopped,
    Errored,
    Restarting,
}

impl std::fmt::Display for ProcessStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessStatus::Running => write!(f, "running"),
            ProcessStatus::Stopped => write!(f, "stopped"),
            ProcessStatus::Errored => write!(f, "errored"),
            ProcessStatus::Restarting => write!(f, "restarting"),
        }
    }
}
