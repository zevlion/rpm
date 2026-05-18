mod daemon;
mod ipc;
mod process;
mod tui;

use anyhow::Result;
use ipc::IpcClient;
use ipc::messages::DaemonCommand;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        // hidden arg — invoked by ensure_daemon() to start the background daemon
        Some("__daemon") => {
            daemon::run().await?;
        }

        Some("--version") | Some("-V") => {
            println!("rpm2 v{}", env!("CARGO_PKG_VERSION"));
        }

        Some("--uninstall") => {
            uninstall().await?;
        }

        Some("--update") => {
            update().await?;
        }

        Some("start") => {
            let mut client = ensure_daemon().await?;
            let opts = parse_start(&args[2..])?;
            let attach = opts.attach;

            let res = client
                .send(DaemonCommand::Start {
                    name: opts.name,
                    cmd: opts.cmd,
                    args: opts.args,
                    watching: opts.watch,
                    interpreter: opts.interpreter,
                    attach,
                })
                .await?;

            match res {
                ipc::messages::DaemonResponse::Ok if attach => {
                    // Intercept Ctrl+C so it detaches instead of killing everything
                    let ctrlc_hit = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let flag = ctrlc_hit.clone();
                    tokio::spawn(async move {
                        tokio::signal::ctrl_c().await.ok();
                        flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    });

                    loop {
                        if ctrlc_hit.load(std::sync::atomic::Ordering::SeqCst) {
                            eprintln!(
                                "\n[rpm2] detached — process still running. use `rpm2 stop` to stop it."
                            );
                            break;
                        }
                        match client.recv().await? {
                            ipc::messages::DaemonResponse::Line(line) => println!("{}", line),
                            ipc::messages::DaemonResponse::Eof => break,
                            ipc::messages::DaemonResponse::Err(e) => {
                                eprintln!("✗ {}", e);
                                break;
                            }
                            _ => break,
                        }
                    }
                }
                other => handle_response(other),
            }
        }

        Some("stop") => {
            let target = args
                .get(2)
                .cloned()
                .ok_or(anyhow::anyhow!("usage: rpm2 stop <id|name>"))?;
            let mut client = ensure_daemon().await?;
            let res = client.send(DaemonCommand::Stop { target }).await?;
            handle_response(res);
        }

        Some("restart") => {
            let target = args
                .get(2)
                .cloned()
                .ok_or(anyhow::anyhow!("usage: rpm2 restart <id|name>"))?;
            let mut client = ensure_daemon().await?;
            let res = client.send(DaemonCommand::Restart { target }).await?;
            handle_response(res);
        }

        Some("delete") | Some("del") => {
            let target = args
                .get(2)
                .cloned()
                .ok_or(anyhow::anyhow!("usage: rpm2 delete <id|name|all>"))?;
            let mut client = ensure_daemon().await?;
            let res = client.send(DaemonCommand::Delete { target }).await?;
            handle_response(res);
        }

        Some("list") | Some("ls") => {
            let mut client = ensure_daemon().await?;
            let res = client.send(DaemonCommand::List).await?;
            print_list(res);
        }

        Some("tui") => {
            ensure_daemon().await?;
            run_tui().await?;
        }

        Some("kill") => {
            if let Ok(mut client) = IpcClient::connect().await {
                let _ = client.send(DaemonCommand::Shutdown).await;
                println!("rpm2 daemon stopped.");
            } else {
                println!("daemon is not running.");
            }
        }

        _ => {
            print_help();
        }
    }

    Ok(())
}

// --- ensure daemon is running, start it silently if not ---

async fn ensure_daemon() -> Result<IpcClient> {
    if let Ok(client) = IpcClient::connect().await {
        return Ok(client);
    }

    tokio::process::Command::new(std::env::current_exe()?)
        .arg("__daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if let Ok(client) = IpcClient::connect().await {
            eprintln!("[rpm2] daemon started");
            return Ok(client);
        }
    }

    anyhow::bail!("daemon failed to start within 2s");
}

// --- start options ---

struct StartOpts {
    name: String,
    cmd: String,
    args: Vec<String>,
    watch: bool,
    interpreter: Option<String>,
    attach: bool,
}

fn parse_start(args: &[String]) -> Result<StartOpts> {
    if args.is_empty() {
        anyhow::bail!(
            "usage: rpm2 start <cmd> [args..] [--name <n>] [--watch] [--interpreter <i>] [--attach]"
        );
    }

    let mut cmd = String::new();
    let mut name = String::new();
    let mut extra_args = vec![];
    let mut watch = false;
    let mut interpreter: Option<String> = None;
    let mut attach = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--name" | "-n" => {
                i += 1;
                name = args
                    .get(i)
                    .cloned()
                    .ok_or(anyhow::anyhow!("--name requires a value"))?;
            }
            "--watch" | "-w" => {
                watch = true;
            }
            "--attach" | "-a" => {
                attach = true;
            }
            "--force" => { /* TODO: kill existing if same name */ }
            "--interpreter" | "-i" => {
                i += 1;
                interpreter = Some(
                    args.get(i)
                        .cloned()
                        .ok_or(anyhow::anyhow!("--interpreter requires a value"))?,
                );
            }
            arg => {
                if cmd.is_empty() {
                    cmd = arg.to_string();
                    if name.is_empty() {
                        name = std::path::Path::new(arg)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                    }
                } else {
                    extra_args.push(arg.to_string());
                }
            }
        }
        i += 1;
    }

    if cmd.is_empty() {
        anyhow::bail!("no command provided");
    }

    Ok(StartOpts {
        name,
        cmd,
        args: extra_args,
        watch,
        interpreter,
        attach,
    })
}

// --- output helpers ---

fn handle_response(res: ipc::messages::DaemonResponse) {
    match res {
        ipc::messages::DaemonResponse::Ok => println!("✓ ok"),
        ipc::messages::DaemonResponse::Err(e) => eprintln!("✗ {}", e),
        ipc::messages::DaemonResponse::ProcessList(list) => print_table(&list),
        ipc::messages::DaemonResponse::Line(line) => println!("{}", line),
        ipc::messages::DaemonResponse::Eof => {}
    }
}

fn print_list(res: ipc::messages::DaemonResponse) {
    match res {
        ipc::messages::DaemonResponse::ProcessList(list) => print_table(&list),
        ipc::messages::DaemonResponse::Err(e) => eprintln!("✗ {}", e),
        _ => {}
    }
}

fn print_table(processes: &[process::Process]) {
    // column widths
    let col = [4, 16, 7, 6, 8, 10, 10, 6, 4];
    let headers = [
        "id", "name", "pid", "cpu%", "mem", "uptime", "status", "watch", "↺",
    ];

    let total: usize = col.iter().sum::<usize>() + col.len() * 3 + 1;

    // top border
    print!("┌");
    for (i, w) in col.iter().enumerate() {
        print!("{}", "─".repeat(w + 2));
        if i < col.len() - 1 {
            print!("┬");
        }
    }
    println!("┐");

    // header row
    print!("│");
    for (i, h) in headers.iter().enumerate() {
        print!(" {:<width$} │", h, width = col[i]);
    }
    println!();

    // header separator
    print!("├");
    for (i, w) in col.iter().enumerate() {
        print!("{}", "─".repeat(w + 2));
        if i < col.len() - 1 {
            print!("┼");
        }
    }
    println!("┤");

    if processes.is_empty() {
        print!("│");
        let inner = total - 2;
        let msg = "no processes running";
        let pad = (inner - msg.len()) / 2;
        print!(
            "{}{}{}",
            " ".repeat(pad),
            msg,
            " ".repeat(inner - pad - msg.len())
        );
        println!("│");
    } else {
        for p in processes {
            let status = match p.status {
                process::ProcessStatus::Online => "● online",
                process::ProcessStatus::Stopped => "○ stopped",
            };
            print!("│");
            print!(" {:<width$} │", p.id, width = col[0]);
            print!(" {:<width$} │", p.name, width = col[1]);
            print!(
                " {:<width$} │",
                p.pid.map(|p| p.to_string()).unwrap_or("-".into()),
                width = col[2]
            );
            print!(" {:<width$} │", format!("{:.1}", p.cpu), width = col[3]);
            print!(" {:<width$} │", p.format_mem(), width = col[4]);
            print!(" {:<width$} │", p.format_uptime(), width = col[5]);
            print!(" {:<width$} │", status, width = col[6]);
            print!(
                " {:<width$} │",
                if p.watching { "yes" } else { "no" },
                width = col[7]
            );
            print!(" {:<width$} │", p.restarts, width = col[8]);
            println!();
        }
    }

    // bottom border
    print!("└");
    for (i, w) in col.iter().enumerate() {
        print!("{}", "─".repeat(w + 2));
        if i < col.len() - 1 {
            print!("┴");
        }
    }
    println!("┘");
    println!();
}

// --- install / uninstall / update ---

async fn uninstall() -> Result<()> {
    // 1. Kill the daemon gracefully if it's running
    if let Ok(mut client) = IpcClient::connect().await {
        println!("Stopping rpm2 daemon...");
        let _ = client.send(DaemonCommand::Shutdown).await;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }

    // 2. Remove the binary
    let bin = std::path::Path::new("/usr/local/bin/rpm2");
    if bin.exists() {
        match std::fs::remove_file(bin) {
            Ok(_) => {
                println!("✓ Removed /usr/local/bin/rpm2");
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                let status = std::process::Command::new("sudo")
                    .args(["rm", "/usr/local/bin/rpm2"])
                    .status()?;
                if status.success() {
                    println!("✓ Removed /usr/local/bin/rpm2");
                } else {
                    anyhow::bail!("Failed to remove binary (sudo rm exited non-zero)");
                }
            }
            Err(e) => return Err(e.into()),
        }
    } else {
        println!("rpm2 is not installed at /usr/local/bin/rpm2 — nothing to remove.");
    }

    println!("rpm2 uninstalled.");
    Ok(())
}

async fn update() -> Result<()> {
    const DOWNLOAD_URL: &str = "https://github.com/zevlion/rpm2/releases/download/latest/rpm2";
    const TMP_PATH: &str = "/tmp/rpm2_bin";
    const INSTALL_PATH: &str = "/usr/local/bin/rpm2";

    println!("Downloading latest rpm2...");

    let status = std::process::Command::new("curl")
        .args(["-fsSL", DOWNLOAD_URL, "-o", TMP_PATH])
        .status()?;

    if !status.success() {
        anyhow::bail!("curl failed — check your internet connection or the release URL");
    }

    std::process::Command::new("chmod")
        .args(["+x", TMP_PATH])
        .status()?;

    let mv_status = std::process::Command::new("mv")
        .args([TMP_PATH, INSTALL_PATH])
        .status()?;

    if !mv_status.success() {
        let sudo_status = std::process::Command::new("sudo")
            .args(["mv", TMP_PATH, INSTALL_PATH])
            .status()?;
        if !sudo_status.success() {
            anyhow::bail!("Failed to move binary to {INSTALL_PATH}");
        }
    }

    let ver = std::process::Command::new(INSTALL_PATH)
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "(unknown)".into());

    println!("✓ Updated to {ver}");
    Ok(())
}

fn print_help() {
    println!(
        r#"
rpm2 — process manager

USAGE:
  rpm2 <command> [options]

COMMANDS:
  start <cmd>             Start a process
    -n, --name <name>       Process name (default: binary name)
    -w, --watch             Auto-restart on crash
    -a, --attach            Attach stdout to terminal
    -i, --interpreter <i>   e.g. node, python3
        --force             Restart if already running

  stop    <id|name>       Stop a process
  restart <id|name>       Restart a process
  delete  <id|name|all>   Delete a process
  list, ls                List all processes
  tui                     Open the terminal UI
  kill                    Stop the rpm2 daemon

  --version, -V           Print version
  --update                Update rpm2 to the latest release
  --uninstall             Remove rpm2 from the system

EXAMPLES:
  rpm2 start ./server --name api --watch
  rpm2 start app.js --interpreter node --name frontend
  rpm2 stop api
  rpm2 restart 0
  rpm2 delete all
  rpm2 ls
"#
    );
}

// --- tui runner ---

async fn run_tui() -> Result<()> {
    use crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    };
    use ratatui::{Terminal, backend::CrosstermBackend};

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = tui::run(&mut terminal).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}
