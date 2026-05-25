//! # IPC Messages
//!
//! JSON-serialisable command / response envelopes that travel over the
//! platform-native IPC channel between the CLI and the background daemon.
//!
//! Every message is encoded as a **newline-delimited JSON** (`\n`) frame so
//! that a single `BufReader::read_line` call on either side retrieves exactly
//! one message.
//!
//! ## Command flow
//!
//! ```text
//! CLI              IPC channel          Daemon
//!  │── DaemonCommand (JSON\n) ──────────►│
//!  │◄─ DaemonResponse (JSON\n) ──────────│
//! ```
//!
//! For streaming commands (e.g. `Start { attach: true }`) the daemon sends one
//! `DaemonResponse::Ok` followed by zero-or-more `Line` frames and a final
//! `Eof` frame.

//! # IPC Messages
//!
//! Types exchanged over the IPC channel. Both sides communicate via newline‑delimited
//! JSON.  The command side (`DaemonCommand`) represents actions that the CLI can
//! request the daemon to perform.  The response side (`DaemonResponse`) mirrors
//! the outcome, providing success (`Ok`), errors, process listings, and streaming
//! output (`Line` / `Eof`).
//!
//! These types derive `Serialize` / `Deserialize` so they can be turned into JSON
//! with `serde_json`.  Keeping the definitions in a dedicated module isolates the
//! protocol from the transport implementation (`ipc::mod`).
//!

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
