use clap::{ArgAction, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "rpm2", version, about = "Process Manager")]
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

        #[arg(short = 'i', long, value_name = "NUMBER")]
        instances: Option<u32>,

        #[arg(long, value_name = "NUMBER")]
        parallel: Option<u32>,

        #[arg(short = 'x', long = "execute-command", action = ArgAction::SetTrue)]
        execute_command: bool,

        #[arg(long, value_name = "INTERPRETER")]
        interpreter: Option<String>,

        #[arg(long, value_name = "ARGUMENTS")]
        interpreter_args: Option<String>,

        #[arg(long, value_name = "NODE_ARGS")]
        node_args: Option<String>,

        #[arg(long, value_name = "PATH")]
        cwd: Option<String>,

        #[arg(long, value_name = "COUNT")]
        max_restarts: Option<u32>,

        #[arg(long, value_name = "MEMORY")]
        max_memory_restart: Option<String>,

        #[arg(long, value_name = "DELAY")]
        restart_delay: Option<u64>,

        #[arg(long = "no-autorestart", action = ArgAction::SetTrue)]
        no_autorestart: bool,

        #[arg(long, value_name = "CRON_PATTERN")]
        cron: Option<String>,

        #[arg(long = "cron-restart", value_name = "CRON_PATTERN")]
        cron_restart: Option<String>,

        #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
        log: Option<String>,

        #[arg(long, value_name = "PATH")]
        output: Option<String>,

        #[arg(long, value_name = "PATH")]
        error: Option<String>,

        #[arg(long = "log-type", value_name = "TYPE")]
        log_type: Option<String>,

        #[arg(long = "log-date-format", value_name = "DATE_FORMAT")]
        log_date_format: Option<String>,

        #[arg(long = "merge-logs", action = ArgAction::SetTrue)]
        merge_logs: bool,

        #[arg(long = "disable-logs", action = ArgAction::SetTrue)]
        disable_logs: bool,

        #[arg(long, action = ArgAction::SetTrue)]
        time: bool,

        #[arg(long, action = ArgAction::SetTrue)]
        raw: bool,

        #[arg(long, value_name = "ENV_NAME")]
        env: Option<String>,

        #[arg(long = "update-env", action = ArgAction::SetTrue)]
        update_env: bool,

        #[arg(long, value_name = "PATHS", num_args = 0..=1, default_missing_value = "")]
        watch: Option<String>,

        #[arg(long = "ignore-watch", value_name = "FOLDERS_FILES")]
        ignore_watch: Option<String>,

        #[arg(long = "wait-ready", action = ArgAction::SetTrue)]
        wait_ready: bool,

        #[arg(long = "listen-timeout", value_name = "DELAY")]
        listen_timeout: Option<u64>,

        #[arg(long = "kill-timeout", value_name = "DELAY")]
        kill_timeout: Option<u64>,

        #[arg(long, value_name = "USERNAME")]
        user: Option<String>,

        #[arg(long, value_name = "UID")]
        uid: Option<u32>,

        #[arg(long, value_name = "GID")]
        gid: Option<u32>,

        #[arg(long = "no-daemon", action = ArgAction::SetTrue)]
        no_daemon: bool,

        #[arg(long, action = ArgAction::SetTrue)]
        silent: bool,

        #[arg(long = "mini-list", action = ArgAction::SetTrue)]
        mini_list: bool,

        #[arg(long = "no-color", action = ArgAction::SetTrue)]
        no_color: bool,

        #[arg(last = true)]
        args: Vec<String>,
    },

    Restart {
        /// Process id to restart
        id: usize,
    },

    List,

    Stop {
        /// Process id to stop
        id: usize,
    },

    Daemon,
}
