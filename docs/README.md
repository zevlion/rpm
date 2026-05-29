# Process Manager

## Installation

#### Linux

```bash
curl -fsSL "https://raw.githubusercontent.com/zevlion/rpm/refs/heads/master/scripts/linux-installer.sh?$(date +%s)" | bash
```

#### macOS

```bash
curl -fsSL "https://raw.githubusercontent.com/zevlion/rpm/refs/heads/master/scripts/macos-installer.sh?$(date +%s)" | bash
```

#### Windows

```powershell
irm "https://raw.githubusercontent.com/zevlion/rpm/refs/heads/master/scripts/windows-installer.ps1" | iex
```

Or with a custom install directory:

```powershell
.\windows-installer.ps1 -InstallDir "C:\Program Files\rpm"
```

## Platform Support

| Platform | IPC Transport        | Architecture        |
| -------- | -------------------- | ------------------- |
| Linux    | Unix socket          | x86_64, arm64       |
| macOS    | Unix socket          | x86_64, arm64 (M1+) |
| Windows  | Named pipe           | x86_64              |
| Android  | Abstract Unix socket | arm64               |


## Commands

```bash
rpm start <cmd> [flags]   # start a process
rpm stop <id|name>        # stop a process
rpm restart <id|name>     # restart a process
rpm delete <id|name|all>  # delete a process
rpm ls                    # list all processes
rpm tui                   # open the terminal UI
rpm kill                  # stop the daemon
rpm --update              # update to the latest release
rpm --uninstall           # remove rpm from the system
rpm --version             # print version
```

## Start Flags

```bash
-n, --name <name>          # process name (default: binary name)
-w, --watch                # auto-restart on crash
-a, --attach               # attach stdout to terminal
-i, --interpreter <bin>    # interpreter e.g. node, python3
    --force                # restart if already running
```

## Examples

```bash
rpm start ./server --name api --watch
rpm start app.js --interpreter node --name frontend
rpm start script.py --interpreter python3 -w
rpm stop api
rpm restart 0
rpm delete all
rpm ls
rpm tui
rpm --update
rpm --uninstall
```

## How It Works

On first use, rpm spawns a background daemon and connects to it via a platform-native IPC channel (Unix socket on Linux/macOS, named pipe on Windows). Subsequent CLI invocations connect to the already-running daemon — startup is instant. Process metadata is persisted to a local SQLite database so the process list survives daemon restarts.

## Building from Source

Requires [Rust](https://rustup.rs) 1.96.0 or later.

```bash
git clone https://github.com/zevlion/rpm
cd rpm
cargo build --release
./target/release/rpm --version
```
