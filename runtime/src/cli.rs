use clap::{Parser, Subcommand, ValueEnum};
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
    Graphics {
        #[command(subcommand)]
        command: GraphicsCommand,
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
        #[arg(long, value_enum, default_value_t = GraphicsBackendArg::Auto)]
        backend: GraphicsBackendArg,
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
pub enum GraphicsCommand {
    Dxmt {
        #[command(subcommand)]
        command: DxmtCommand,
    },
}

#[derive(Subcommand)]
pub enum DxmtCommand {
    Status {
        #[arg(long)]
        json: bool,
    },
    Install {
        #[arg(long)]
        source: PathBuf,
        #[arg(long, value_enum, default_value_t = DxmtModeArg::Builtin)]
        mode: DxmtModeArg,
        #[arg(long)]
        json: bool,
    },
    InstallLatest {
        #[arg(long)]
        json: bool,
    },
    Update {
        #[arg(long)]
        json: bool,
    },
    Remove,
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
        #[arg(long, value_enum, default_value_t = GraphicsBackendArg::Auto)]
        backend: GraphicsBackendArg,
        #[arg(long)]
        json: bool,
    },
    Restart {
        #[arg(long, value_enum, default_value_t = GraphicsBackendArg::Auto)]
        backend: GraphicsBackendArg,
        #[arg(long)]
        json: bool,
    },
    Run {
        #[arg(long)]
        app_id: u32,
        #[arg(long, value_enum, default_value_t = GraphicsBackendArg::Auto)]
        backend: GraphicsBackendArg,
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
        #[arg(long, value_enum, default_value_t = GraphicsBackendArg::Auto)]
        fallback_backend: GraphicsBackendArg,
        #[arg(long)]
        json: bool,
    },
    Set {
        #[arg(long)]
        app_id: u32,
        #[arg(long, value_enum, default_value_t = BackendOverrideArg::Inherit)]
        backend: BackendOverrideArg,
        #[arg(long)]
        executable: Option<String>,
        #[arg(long = "launch-argument", allow_hyphen_values = true)]
        launch_arguments: Vec<String>,
        #[arg(long, value_enum, default_value_t = GraphicsBackendArg::Auto)]
        fallback_backend: GraphicsBackendArg,
        #[arg(long)]
        json: bool,
    },
    Reset {
        #[arg(long)]
        app_id: u32,
        #[arg(long, value_enum, default_value_t = GraphicsBackendArg::Auto)]
        fallback_backend: GraphicsBackendArg,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum GraphicsBackendArg {
    Auto,
    Wined3d,
    Dxmt,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum BackendOverrideArg {
    Inherit,
    Auto,
    Wined3d,
    Dxmt,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DxmtModeArg {
    Builtin,
    Native,
}
