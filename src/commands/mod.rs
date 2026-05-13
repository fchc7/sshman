pub mod handlers;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sshman")]
#[command(about = "SSH connection manager with encrypted password storage")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(short, long, global = true, help = "Configuration directory path")]
    pub config_dir: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Initialize sshman (first-time setup)")]
    Init {
        #[arg(short, long, help = "Master password (will prompt if not provided)")]
        master_password: Option<String>,
    },
    #[command(about = "Reset stored data")]
    Reset {
        #[arg(long, help = "Delete everything (connections + passwords)")]
        all: bool,
    },
    #[command(about = "Add a new SSH connection")]
    Add {
        #[arg(short = 'a', long, help = "Connection alias")]
        alias: String,
        #[arg(short = 'h', long, help = "Hostname or IP address")]
        host: String,
        #[arg(short = 'p', long, default_value_t = 22, help = "SSH port")]
        port: u16,
        #[arg(short = 'u', long, help = "SSH username")]
        user: String,
        #[arg(short = 't', long, value_delimiter = ',', help = "Tags (comma-separated)")]
        tags: Vec<String>,
        #[arg(short = 'c', long, help = "Display color (red/green/yellow/blue/purple/cyan)")]
        color: Option<String>,
        #[arg(long, help = "SSH password (will prompt if not provided)")]
        password: Option<String>,
        #[arg(long, help = "Don't connect after adding")]
        no_connect: bool,
    },
    #[command(visible_alias("c"), about = "Connect to a saved SSH server")]
    Connect {
        #[arg(help = "Connection alias or index number")]
        alias: String,
        #[arg(short, long, help = "Master password (will prompt if not provided)")]
        master_password: Option<String>,
        #[arg(long, help = "Show the command without executing")]
        dry_run: bool,
    },
    #[command(visible_alias("ls"), about = "List saved connections")]
    List {
        #[arg(help = "Search keyword (matches alias/host/user/tags)")]
        keyword: Option<String>,
        #[arg(short, long, help = "Filter by tag")]
        tag: Option<String>,
        #[arg(short, long, help = "Only show connections in the same local network")]
        local: bool,
    },
    #[command(about = "Edit a saved connection")]
    Edit {
        #[arg(help = "Connection alias or index number")]
        alias: String,
        #[arg(short = 'h', long, help = "New hostname or IP address")]
        host: Option<String>,
        #[arg(short = 'p', long, help = "New SSH port")]
        port: Option<u16>,
        #[arg(short = 'u', long, help = "New SSH username")]
        user: Option<String>,
        #[arg(short = 't', long, value_delimiter = ',', help = "New tags (comma-separated)")]
        tags: Option<Vec<String>>,
        #[arg(short = 'c', long, help = "New display color")]
        color: Option<String>,
        #[arg(short = 'a', long, help = "New alias name")]
        rename: Option<String>,
        #[arg(long, help = "Change SSH password (will prompt)")]
        password: bool,
    },
    #[command(visible_alias("rm"), about = "Remove a saved connection")]
    Remove {
        #[arg(help = "Connection alias")]
        alias: String,
        #[arg(long, help = "Skip confirmation prompt")]
        force: bool,
    },
    #[command(about = "Show decrypted SSH password for a connection")]
    Show {
        #[arg(help = "Connection alias or index number")]
        alias: String,
        #[arg(short, long, help = "Master password (will prompt if not provided)")]
        master_password: Option<String>,
    },
    #[command(about = "Swap or reassign connection IDs")]
    Swap {
        #[arg(help = "Source connection ID")]
        id1: u32,
        #[arg(help = "Target ID (swap if occupied, reassign if free)")]
        id2: u32,
    },
    #[command(visible_alias("up"), about = "Upload a file to a remote server")]
    Upload {
        #[arg(help = "Connection alias or index number")]
        alias: String,
        #[arg(help = "Local file path")]
        local: String,
        #[arg(help = "Remote file path")]
        remote: String,
        #[arg(short, long, help = "Master password (will prompt if not provided)")]
        master_password: Option<String>,
    },
    #[command(visible_alias("dl"), about = "Download a file from a remote server")]
    Download {
        #[arg(help = "Connection alias or index number")]
        alias: String,
        #[arg(help = "Remote file path")]
        remote: String,
        #[arg(help = "Local file path")]
        local: String,
        #[arg(short, long, help = "Master password (will prompt if not provided)")]
        master_password: Option<String>,
    },
}
