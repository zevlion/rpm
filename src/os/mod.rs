//! # OS Abstraction
//!
//! Platform-specific IPC implementations selected at compile time via
//! `cfg(target_os)`.
//!
//! Each platform sub-module exposes the same three types under the `ipc`
//! path so the rest of the codebase can import from `crate::os::ipc` without
//! knowing which transport is in use:
//!
//! | Type | Role |
//! |------|------|
//! | `IpcClient` | CLI side — connects to the daemon and sends commands |
//! | `IpcServer` | Daemon side — binds the socket/pipe and accepts connections |
//! | `IpcConn` | Per-connection handle returned by `IpcServer::accept` |
//!
//! | Platform | Module | Transport |
//! |----------|--------|-----------|
//! | Linux | [`linux`] | Unix socket `/tmp/rpm.sock` |
//! | macOS | `macos` | Unix socket `/tmp/rpm.sock` |
//! | Windows | `windows` | Named pipe `\\.\pipe\rpm` |
//! | Android | `android` | Abstract Unix socket |
#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "android")]
pub mod android;

#[cfg(target_os = "linux")]
pub use linux::ipc;

#[cfg(target_os = "macos")]
pub use macos::ipc;

#[cfg(target_os = "windows")]
pub use windows::ipc;

#[cfg(target_os = "android")]
pub use android::ipc;
