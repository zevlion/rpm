//! # IPC
//!
//! Inter-process communication between the `rpm` CLI and the background daemon.
//!
//! This module re-exports the platform-specific [`IpcClient`] so callers never
//! need to import from `os::*` directly.  The underlying transport is chosen at
//! compile time based on `cfg(target_os)`:
//!
//! | Platform | Transport |
//! |----------|-----------|
//! | Linux / macOS | Unix domain socket (`/tmp/rpm.sock`) |
//! | Windows | Named pipe (`\\.\pipe\rpm`) |
//! | Android | Abstract Unix socket |
//!
//! See [`messages`] for the JSON-encoded command/response types.
pub mod messages;

pub use crate::os::ipc::IpcClient;
