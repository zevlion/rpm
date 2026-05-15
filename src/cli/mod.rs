use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "rpm2", version, about = "Native process manager written in Rust")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Start {
        program: String,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(last = true)]
        args: Vec<String>,
    },
    List,
    Stop {
        id: usize,
    },
    Daemon,
}