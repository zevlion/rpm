# rpm2 — Rust Process Manager

A lightweight native alternative to PM2. Single binary, zero runtime overhead, no Node.js required.

The daemon starts automatically on first use and runs in the background. The CLI connects to it over a Unix socket, sends a command, and exits. All process state lives in the daemon.

---

## Installation

```bash
curl -L https://github.com/zevlion/rpm2/releases/download/latest/rpm2 -o rpm2 && \
chmod +x rpm2 && \
sudo mv rpm2 /usr/local/bin/rpm2
```

## Commands

```bash
rpm2 start <cmd> [flags]   # start a process
rpm2 stop <id|name>        # stop a process
rpm2 restart <id|name>     # restart a process
rpm2 delete <id|name|all>  # delete a process
rpm2 ls                    # list all processes
rpm2 tui                   # open the terminal UI
rpm2 kill                  # stop the daemon
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
rpm2 start ./server --name api --watch
rpm2 start app.js --interpreter node --name frontend
rpm2 start script.py --interpreter python3 -w
rpm2 stop api
rpm2 restart 0
rpm2 delete all
rpm2 ls
rpm2 tui
```
