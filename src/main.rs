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

        Some("start") => {
            let mut client = ensure_daemon().await?;
            let opts = parse_start(&args[2..])?;
            let res = client
                .send(DaemonCommand::Start {
                    name: opts.name,
                    cmd: opts.cmd,
                    args: opts.args,
                    watching: opts.watch,
                    interpreter: opts.interpreter,
                    attach: opts.attach,
                })
                .await?;
            handle_response(res);
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
    if processes.is_empty() {
        println!("no processes running.");
        return;
    }
    println!(
        "\n{:<4} {:<16} {:<7} {:<6} {:<8} {:<10} {:<10} {:<6} {:<4}",
        "id", "name", "pid", "cpu%", "mem", "uptime", "status", "watch", "↺"
    );
    println!("{}", "─".repeat(80));
    for p in processes {
        let status = match p.status {
            process::ProcessStatus::Online => "● online",
            process::ProcessStatus::Stopped => "○ stopped",
        };
        println!(
            "{:<4} {:<16} {:<7} {:<6} {:<8} {:<10} {:<10} {:<6} {:<4}",
            p.id,
            p.name,
            p.pid.map(|p| p.to_string()).unwrap_or("-".into()),
            format!("{:.1}", p.cpu),
            p.format_mem(),
            p.format_uptime(),
            status,
            if p.watching { "yes" } else { "no" },
            p.restarts,
        );
    }
    println!();
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

