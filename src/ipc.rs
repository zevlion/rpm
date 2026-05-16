use serde::{Deserialize, Serialize};

pub const SOCKET_PATH: &str = "/tmp/rpm2.sock";

#[derive(Serialize, Deserialize, Debug)]
pub enum IpcCommand {
    Start {
        program: String,
        name: String,
        args: Vec<String>,
    },
    List,
    Stop {
        id: usize,
    },
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
    pub status: String,
    pub uptime: Option<String>,
}
