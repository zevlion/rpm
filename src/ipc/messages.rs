use crate::process::Process;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DaemonCommand {
    List,
    Start {
        name: String,
        cmd: String,
        args: Vec<String>,
        watching: bool,
        interpreter: Option<String>,
        attach: bool,
        force: bool,
        #[serde(default)]
        mode: Option<String>,
        #[serde(default)]
        instances: Option<u32>,
        #[serde(default)]
        port: Option<u16>,
        #[serde(default)]
        lb_strategy: Option<String>,
        #[serde(default)]
        max_memory: Option<u64>,
        #[serde(default)]
        max_cpu: Option<f32>,
    },
    Stop {
        target: String,
    },
    Restart {
        target: String,
    },
    Delete {
        target: String,
    },
    Watch {
        target: String,
        enable: bool,
    },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DaemonResponse {
    ProcessList(Vec<Process>),
    Ok,
    Err(String),
    Line(String), // streaming stdout/stderr line
    Eof,          // process exited or detached
}
