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
