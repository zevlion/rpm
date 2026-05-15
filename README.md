# rpm2: Process Manager

## Project Overview

**rpm2** (Rust Process Manager 2) is a lightweight process orchestrator provides a native alternative to Node.js-based managers like PM2. Built entirely in Rust, it uses asynchronous system calls to manage background services with near-zero overhead.

---

## Architecture

### 1. The Core Philosophy

The primary objective of `rpm2` is **minimalism and reliability**. Traditional process managers often introduce significant memory overhead because they require a runtime (like V8) to stay resident in memory. `rpm2` compiles to a single native binary, utilizing the operating system's native process management capabilities directly.

### 2. Client-Daemon Model

`rpm2` operates as two distinct logical entities within the same binary:

#### A. The Background Daemon (`rpm2 daemon`)

The daemon is the "Source of Truth." It:

- Creates and listens on a Unix Domain Socket (`/tmp/rpm2.sock`).
- Manages an internal Registry of all child processes.
- Monitors the health and exit codes of managed applications.
- Routes standard I/O to defined log locations (Planned).

#### B. The CLI Client (`rpm2 start`, `rpm2 list`, etc.)

The CLI is a transient interface. It:

- Parses user input using the `clap` crate.
- Serializes commands into a shared IPC (Inter-Process Communication) format.
- Communicates with the daemon over the socket.
- Renders the daemon's response in a human-readable format.

---

## Technical Components

### Asynchronous Runtime

We use **Tokio** to handle concurrency. This allows the daemon to handle multiple CLI requests and monitor multiple child processes simultaneously without blocking the main execution thread.

### IPC Protocol (JSON over Unix Sockets)

Communication is strictly typed using Rust enums. We define an `IpcCommand` enum for requests and an `IpcResponse` enum for replies. This ensures that the client and daemon are always in sync regarding the data format, preventing crashes due to malformed messages.

### Process Tracking

Each managed process is assigned:

- **ID:** A unique integer for easy CLI interaction.
- **Name:** A user-defined string for identification.
- **PID:** The actual OS Process ID used for signaling (SIGTERM/SIGKILL).
- **Status:** A state indicator (e.g., Online, Stopped, Errored).

---

## Design Goals

1. **Low Footprint:** Aim for < 10MB RAM usage for the daemon.
2. **Speed:** Instantaneous command execution via Unix Sockets.
3. **Safety:** Use Rust's ownership model to prevent race conditions in process state management.
4. **Portability:** Initially targeting Linux/macOS with future Windows support via Named Pipes.

---

## Development Environment

The project includes a specialized `.devcontainer` configuration based on `Debian Bookworm`. This environment comes pre-configured with:

- The Rust toolchain (Cargo, Rustc).
- `rust-analyzer` for IDE intelligence.
- Common system utilities (`curl`, `python3`, `bun`) for testing various process types.
