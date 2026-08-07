mod app_dirs;
mod cli;
mod compatibility;
mod error;
mod events;
mod graphics;
mod pe;
mod prefix;
mod steam;
mod vdf;
mod wine;

use clap::Parser;
use cli::{
    BackendOverrideArg, Cli, Command, DxmtCommand, DxmtModeArg, GraphicsBackendArg,
    GraphicsCommand, PrefixCommand, SteamCommand, SteamProfileCommand, WineCommand,
};
use compatibility::BackendOverride;
use error::Result;
use events::write_json;
use graphics::{DxmtMode, GraphicsBackend, GraphicsManager};
use pe::inspect_pe;
use prefix::PrefixManager;
use steam::SteamManager;
use wine::{manage_wine, wine_status, WineManagedAction, WineRuntime};

impl From<GraphicsBackendArg> for GraphicsBackend {
    fn from(value: GraphicsBackendArg) -> Self {
        match value {
            GraphicsBackendArg::Auto => Self::Auto,
            GraphicsBackendArg::Wined3d => Self::WineD3d,
            GraphicsBackendArg::Dxmt => Self::Dxmt,
        }
    }
}

impl From<BackendOverrideArg> for BackendOverride {
    fn from(value: BackendOverrideArg) -> Self {
        match value {
            BackendOverrideArg::Inherit => Self::Inherit,
            BackendOverrideArg::Auto => Self::Auto,
            BackendOverrideArg::Wined3d => Self::Wined3d,
            BackendOverrideArg::Dxmt => Self::Dxmt,
        }
    }
}

impl From<DxmtModeArg> for DxmtMode {
    fn from(value: DxmtModeArg) -> Self {
        match value {
            DxmtModeArg::Builtin => Self::Builtin,
            DxmtModeArg::Native => Self::Native,
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let prefixes = PrefixManager::new()?;
    let graphics = GraphicsManager::new()?;
    let steam = SteamManager::new()?;

    match cli.command {
        Command::Doctor { json } => {
            let runtime = WineRuntime::discover(cli.wine.as_deref())?;
            let report = runtime.doctor()?;
            if json {
                write_json(&report)?;
            } else {
                println!("Wine: {}", report.wine_path);
                println!("Version: {}", report.wine_version);
                println!("Host architecture: {}", report.host_architecture);
                println!("Wine architecture: {}", report.wine_architecture);
            }
        }
        Command::Wine { command } => match command {
            WineCommand::Status { json } => {
                let status = wine_status(cli.wine.as_deref());
                if json {
                    write_json(&status)?;
                } else if status.installed {
                    println!("Wine installed: {}", status.wine_path.as_deref().unwrap_or("unknown"));
                    println!("Version: {}", status.wine_version.as_deref().unwrap_or("unknown"));
                } else {
                    println!("Wine is not installed");
                }
            }
            WineCommand::Install { json } => manage_wine(WineManagedAction::Install, json)?,
            WineCommand::Reinstall { json } => manage_wine(WineManagedAction::Reinstall, json)?,
            WineCommand::Remove { json } => manage_wine(WineManagedAction::Remove, json)?,
        },
        Command::Inspect { executable, json } => {
            let report = inspect_pe(&executable)?;
            if json {
                write_json(&report)?;
            } else {
                println!("Path: {}", report.path);
                println!("Architecture: {}", report.architecture);
                println!("Subsystem: {}", report.subsystem);
                println!("Entry point: 0x{:x}", report.entry_point);
                println!("Image base: 0x{:x}", report.image_base);
                if !report.graphics_apis.is_empty() {
                    println!("Graphics: {}", report.graphics_apis.join(", "));
                }
                if !report.imports.is_empty() {
                    println!("Imports:");
                    for import in report.imports {
                        println!("  {import}");
                    }
                }
            }
        }
        Command::Prefix { command } => match command {
            PrefixCommand::Create { game_id } => {
                let runtime = WineRuntime::discover(cli.wine.as_deref())?;
                let path = prefixes.ensure(&runtime, &game_id)?;
                println!("{}", path.display());
            }
            PrefixCommand::Reset { game_id } => {
                if let Ok(runtime) = WineRuntime::discover(cli.wine.as_deref()) {
                    let _ = runtime.stop_prefix(&prefixes.path(&game_id)?);
                }
                prefixes.reset(&game_id)?;
            }
        },
        Command::Graphics { command } => match command {
            GraphicsCommand::Dxmt { command } => match command {
                DxmtCommand::Status { json } => {
                    let status = graphics.dxmt_status()?;
                    if json {
                        write_json(&status)?;
                    } else if status.installed {
                        println!("DXMT installed: {}", status.root.as_deref().unwrap_or("unknown"));
                        println!("Mode: {:?}", status.mode.unwrap());
                    } else {
                        println!("DXMT is not installed");
                    }
                }
                DxmtCommand::Install { source, mode, json } => {
                    let status = graphics.install_dxmt(&source, mode.into())?;
                    if json {
                        write_json(&status)?;
                    } else {
                        println!("DXMT installed: {}", status.root.as_deref().unwrap_or("unknown"));
                    }
                }
                DxmtCommand::Remove => graphics.remove_dxmt()?,
            },
        },
        Command::Steam { command } => match command {
            SteamCommand::Status { json } => {
                let runtime = WineRuntime::discover(cli.wine.as_deref()).ok();
                let status = steam.status(runtime.as_ref())?;
                if json {
                    write_json(&status)?;
                } else if status.installed {
                    println!("Steam installed: {}", status.steam_path.as_deref().unwrap_or("unknown"));
                    println!("Steam running: {}", status.running);
                    println!("Installed games: {}", status.games_installed);
                } else {
                    println!("Steam is not installed");
                }
            }
            SteamCommand::Install { installer, json } => {
                let runtime = WineRuntime::discover(cli.wine.as_deref())?;
                let status = steam.install(&runtime, &graphics, installer.as_deref())?;
                if json {
                    write_json(&status)?;
                } else {
                    println!("Steam installed: {}", status.steam_path.as_deref().unwrap_or("unknown"));
                }
            }
            SteamCommand::Games { json } => {
                let library = steam.games()?;
                if json {
                    write_json(&library)?;
                } else {
                    for game in library.games {
                        println!("{}\t{}\t{}", game.app_id, game.name, game.install_path);
                    }
                }
            }
            SteamCommand::Diagnostics { json } => {
                let diagnostics = steam.ui_diagnostics()?;
                if json {
                    write_json(&diagnostics)?;
                } else {
                    println!("WebHelper log: {}", diagnostics.webhelper_log_path.as_deref().unwrap_or("missing"));
                    println!("CEF log: {}", diagnostics.cef_log_path.as_deref().unwrap_or("missing"));
                    println!("CEF --disable-gpu observed: {}", diagnostics.disable_gpu_observed);
                    println!("CEF --disable-gpu-compositing observed: {}", diagnostics.disable_gpu_compositing_observed);
                    println!("Vulkan mentioned in CEF/WebHelper logs: {}", diagnostics.vulkan_observed);
                    if let Some(command_line) = diagnostics.webhelper_command_line {
                        println!("WebHelper command line: {command_line}");
                    }
                }
            }
            SteamCommand::Start { backend, json } => {
                let runtime = WineRuntime::discover(cli.wine.as_deref())?;
                steam.start(&runtime, &graphics, backend.into(), json)?;
            }
            SteamCommand::Restart { backend, json } => {
                let runtime = WineRuntime::discover(cli.wine.as_deref())?;
                steam.restart(&runtime, &graphics, backend.into(), json)?;
            }
            SteamCommand::Run { app_id, backend, json } => {
                let runtime = WineRuntime::discover(cli.wine.as_deref())?;
                steam.launch_game(&runtime, &graphics, app_id, backend.into(), json)?;
            }
            SteamCommand::Profile { command } => match command {
                SteamProfileCommand::Show {
                    app_id,
                    fallback_backend,
                    json,
                } => {
                    let profile = steam.profile(&graphics, app_id, fallback_backend.into())?;
                    if json {
                        write_json(&profile)?;
                    } else {
                        print_profile(&profile);
                    }
                }
                SteamProfileCommand::Set {
                    app_id,
                    backend,
                    executable,
                    launch_arguments,
                    fallback_backend,
                    json,
                } => {
                    let profile = steam.set_profile(
                        &graphics,
                        app_id,
                        backend.into(),
                        executable.as_deref(),
                        launch_arguments,
                        fallback_backend.into(),
                    )?;
                    if json {
                        write_json(&profile)?;
                    } else {
                        print_profile(&profile);
                    }
                }
                SteamProfileCommand::Reset {
                    app_id,
                    fallback_backend,
                    json,
                } => {
                    let profile = steam.reset_profile(&graphics, app_id, fallback_backend.into())?;
                    if json {
                        write_json(&profile)?;
                    } else {
                        print_profile(&profile);
                    }
                }
            },
            SteamCommand::Stop => {
                let runtime = WineRuntime::discover(cli.wine.as_deref())?;
                steam.stop(&runtime)?;
            }
            SteamCommand::Reset => {
                let runtime = WineRuntime::discover(cli.wine.as_deref())?;
                steam.reset(&runtime)?;
            }
        },
        Command::Launch {
            game_id,
            executable,
            backend,
            json,
        } => {
            let runtime = WineRuntime::discover(cli.wine.as_deref())?;
            let report = inspect_pe(&executable)?;
            let prefix = prefixes.ensure(&runtime, &game_id)?;
            prefixes.bind_game_drive(&prefix, &executable)?;
            let launch_graphics =
                graphics.prepare_launch(backend.into(), &report.imports, &prefix, &game_id)?;
            runtime.launch(&prefix, &executable, json, &launch_graphics)?;
        }
        Command::Stop { game_id } => {
            let runtime = WineRuntime::discover(cli.wine.as_deref())?;
            let prefix = prefixes.path(&game_id)?;
            runtime.stop_prefix(&prefix)?;
        }
    }

    Ok(())
}

fn print_profile(profile: &compatibility::SteamCompatibilityProfile) {
    println!("Steam AppID: {}", profile.app_id);
    println!("Game: {}", profile.name);
    println!("Compatibility: {:?}", profile.compatibility);
    println!("Effective backend: {}", profile.effective_backend);
    println!(
        "Recommended executable: {}",
        profile.recommended_executable.as_deref().unwrap_or("unknown")
    );
    println!("Candidates: {}", profile.candidates.len());
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
