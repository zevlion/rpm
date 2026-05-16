mod cli;
mod client;
mod daemon;
mod ipc;

use clap::Parser;
use cli::{Cli, Commands};
use ipc::IpcCommand;

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
        } => {
            let name = name.unwrap_or_else(|| program.clone());
            let resp = client::send_command(IpcCommand::Start {
                program,
                name,
                args,
            })
            .await;
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

