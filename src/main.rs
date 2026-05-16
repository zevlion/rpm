mod cli;
mod client;
mod daemon;
mod ipc;
mod store;

use clap::Parser;
use cli::{Cli, Commands};
use ipc::{IpcCommand, StartOptions};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon => {
            daemon::start_daemon().await;
        }

        Commands::Start {
            program,
            name,
            args,
            cwd,
            interpreter,
            interpreter_args,
            max_restarts,
            restart_delay,
            no_autorestart,
            kill_timeout,
            instances,
            parallel,
            execute_command,
            node_args,
            max_memory_restart,
            cron,
            cron_restart,
            log,
            output,
            error,
            log_type,
            log_date_format,
            merge_logs,
            disable_logs,
            time,
            raw,
            env,
            update_env,
            watch,
            ignore_watch,
            wait_ready,
            listen_timeout,
            user,
            uid,
            gid,
            no_daemon,
            silent,
            mini_list,
            no_color,
        } => {
            let opts = StartOptions {
                name: name.unwrap_or_else(|| program.clone()),
                program,
                args,
                cwd,
                interpreter,
                interpreter_args,
                max_restarts,
                restart_delay,
                no_autorestart,
                kill_timeout,
            };
            let resp = client::send_command(IpcCommand::Start(opts)).await;
            println!("{:?}", resp);
        }

        Commands::Restart { id } => {
            let resp = client::send_command(IpcCommand::Restart { id }).await;
            println!("{:?}", resp);
        }

        Commands::List => {
            let resp = client::send_command(IpcCommand::List).await;
            println!("{:?}", resp);
        }

        Commands::Stop { id } => {
            let resp = client::send_command(IpcCommand::Stop { id }).await;
            println!("{:?}", resp);
        }
    }
}
