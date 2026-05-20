# rpm2 — Process Manager

A lightweight native process manager. Single binary, zero runtime overhead, no Node.js required.

---

## Installation

#### Linux
```bash
curl -fsSL "https://raw.githubusercontent.com/zevlion/rpm2/refs/heads/master/scripts/linux-installer.sh?$(date +%s)" | bash
```

#### macOS
```bash
curl -fsSL "https://raw.githubusercontent.com/zevlion/rpm2/refs/heads/master/scripts/macos-installer.sh?$(date +%s)" | bash
```

#### Windows
```powershell
irm "https://raw.githubusercontent.com/zevlion/rpm2/refs/heads/master/scripts/windows-installer.ps1" | iex
```

Or with a custom install directory:
```powershell
.\windows-installer.ps1 -InstallDir "C:\Program Files\rpm2"
```

---

## Platform Support

| Platform       | IPC Transport         | Architecture        |
|----------------|-----------------------|---------------------|
| Linux          | Unix socket           | x86_64, arm64       |
| macOS          | Unix socket           | x86_64, arm64 (M1+) |
| Windows        | Named pipe            | x86_64              |
| Android        | Abstract Unix socket  | arm64               |

---

## Commands

```bash
rpm2 start <cmd> [flags]   # start a process
rpm2 stop <id|name>        # stop a process
rpm2 restart <id|name>     # restart a process
rpm2 delete <id|name|all>  # delete a process
rpm2 ls                    # list all processes
rpm2 tui                   # open the terminal UI
rpm2 kill                  # stop the daemon
rpm2 --update              # update to the latest release
rpm2 --uninstall           # remove rpm2 from the system
rpm2 --version             # print version
```

---

## Start Flags

```bash
-n, --name <name>          # process name (default: binary name)
-w, --watch                # auto-restart on crash
-a, --attach               # attach stdout to terminal
-i, --interpreter <bin>    # interpreter e.g. node, python3
    --force                # restart if already running
```

---

## Examples

```bash
rpm2 start ./server --name api --watch
rpm2 start app.js --interpreter node --name frontend
rpm2 start script.py --interpreter python3 -w
rpm2 stop api
rpm2 restart 0
rpm2 delete all
rpm2 ls
rpm2 tui
rpm2 --update
rpm2 --uninstall
```

---

## How It Works

On first use, rpm2 spawns a background daemon and connects to it via a platform-native IPC channel (Unix socket on Linux/macOS, named pipe on Windows). Subsequent CLI invocations connect to the already-running daemon — startup is instant. Process metadata is persisted to a local SQLite database so the process list survives daemon restarts.

---

## Building from Source

Requires [Rust](https://rustup.rs) 1.85 or later.

```bash
git clone https://github.com/zevlion/rpm2
cd rpm2
cargo build --release
./target/release/rpm2 --version
```