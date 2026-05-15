mod ipc;
mod cli;
mod daemon;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon => {
            daemon::start_daemon().await;
        }
        Commands::Start { program, name, args } => {
            println!("Client target: start program '{}' with args {:?}", program, args);
        }
        Commands::List => {
            println!("Client target: list running processes");
        }
        Commands::Stop { id } => {
            println!("Client target: stop process index {}", id);
        }
    }
}