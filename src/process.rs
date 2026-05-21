use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessStatus {
    Online,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Process {
    pub id: u32,
    pub name: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub interpreter: Option<String>,
    pub pid: Option<u32>,
    pub uptime: Duration,
    pub status: ProcessStatus,
    pub cpu: f32,
    pub mem: u64,
    pub watching: bool,
    pub restarts: u32,
    pub mode: String,
    pub instances: u32,
    pub port: Option<u16>,
    pub lb_strategy: Option<String>,
    pub max_memory: Option<u64>,
    pub max_cpu: Option<f32>,
}

impl Process {
    pub fn format_uptime(&self) -> String {
        let s = self.uptime.as_secs();
        let h = s / 3600;
        let m = (s % 3600) / 60;
        let s = s % 60;
        if h > 0 {
            format!("{}h{}m{}s", h, m, s)
        } else if m > 0 {
            format!("{}m{}s", m, s)
        } else {
            format!("{}s", s)
        }
    }

    pub fn format_mem(&self) -> String {
        format!("{}mb", self.mem / 1024 / 1024)
    }
}
