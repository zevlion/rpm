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
    },
    Stop {
        target: String,
    }, // id or name
    Restart {
        target: String,
    },
    Delete {
        target: String,
    }, // id, name, or "all"
    Watch {
        target: String,
        enable: bool,
    },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DaemonResponse {
    ProcessList(Vec<Process>),
    Ok,
    Err(String),
}
