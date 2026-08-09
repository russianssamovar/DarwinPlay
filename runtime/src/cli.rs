use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "darwinplay-runtime", version, about = "DarwinWine runtime controller for DarwinPlay")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Doctor {
        #[arg(long)]
        json: bool,
    },
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },
    Inspect {
        executable: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Prefix {
        #[command(subcommand)]
        command: PrefixCommand,
    },
    Steam {
        #[command(subcommand)]
        command: SteamCommand,
    },
    Launch {
        #[arg(long)]
        game_id: String,
        #[arg(long)]
        executable: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Stop {
        #[arg(long)]
        game_id: String,
    },
}

#[derive(Subcommand)]
pub enum RuntimeCommand {
    Status {
        #[arg(long)]
        json: bool,
    },
    Install {
        #[arg(long)]
        archive: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Remove {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum PrefixCommand {
    Create {
        #[arg(long)]
        game_id: String,
    },
    Reset {
        #[arg(long)]
        game_id: String,
    },
}

#[derive(Subcommand)]
pub enum SteamCommand {
    Status {
        #[arg(long)]
        json: bool,
    },
    Install {
        #[arg(long)]
        installer: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    Games {
        #[arg(long)]
        json: bool,
    },
    Diagnostics {
        #[arg(long)]
        json: bool,
    },
    Start {
        #[arg(long)]
        json: bool,
    },
    Restart {
        #[arg(long)]
        json: bool,
    },
    Run {
        #[arg(long)]
        app_id: u32,
        #[arg(long)]
        json: bool,
    },
    Profile {
        #[command(subcommand)]
        command: SteamProfileCommand,
    },
    Stop,
    Reset,
}

#[derive(Subcommand)]
pub enum SteamProfileCommand {
    Show {
        #[arg(long)]
        app_id: u32,
        #[arg(long)]
        json: bool,
    },
    Set {
        #[arg(long)]
        app_id: u32,
        #[arg(long)]
        executable: Option<String>,
        #[arg(long = "launch-argument", allow_hyphen_values = true)]
        launch_arguments: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    Reset {
        #[arg(long)]
        app_id: u32,
        #[arg(long)]
        json: bool,
    },
}
