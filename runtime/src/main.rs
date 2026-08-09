mod app_dirs;
mod cli;
mod compatibility;
mod error;
mod events;
mod pe;
mod prefix;
mod release;
mod steam;
mod vdf;
mod wine;

use clap::Parser;
use cli::{
    Cli, Command, PrefixCommand, RuntimeCommand, SteamCommand, SteamProfileCommand,
};
use error::Result;
use events::write_json;
use pe::inspect_pe;
use prefix::PrefixManager;
use release::install_latest_darwinwine;
use steam::SteamManager;
use wine::{install_darwinwine, remove_darwinwine, runtime_status, WineRuntime};

fn run() -> Result<()> {
    let cli = Cli::parse();
    let prefixes = PrefixManager::new()?;
    let steam = SteamManager::new()?;

    match cli.command {
        Command::Doctor { json } => {
            let runtime = WineRuntime::discover()?;
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
        Command::Runtime { command } => match command {
            RuntimeCommand::Status { json } => {
                let status = runtime_status();
                if json {
                    write_json(&status)?;
                } else if status.ready {
                    println!("DarwinWine: {}", status.darwin_wine_version.as_deref().unwrap_or("unknown"));
                    println!("Wine: {}", status.wine_version.as_deref().unwrap_or("unknown"));
                } else if status.installed {
                    println!("DarwinWine is installed but not ready");
                    if let Some(error) = status.probe_error { println!("{error}"); }
                } else {
                    println!("DarwinWine is not installed");
                }
            }
            RuntimeCommand::InstallLatest { json } => {
                let status = install_latest_darwinwine(json)?;
                if !json {
                    println!("DarwinWine: {}", status.darwin_wine_version.as_deref().unwrap_or("unknown"));
                }
            }
            RuntimeCommand::Install { archive, json } => {
                let status = install_darwinwine(&archive, json)?;
                if !json {
                    println!("DarwinWine: {}", status.darwin_wine_version.as_deref().unwrap_or("unknown"));
                    println!("Wine: {}", status.wine_version.as_deref().unwrap_or("unknown"));
                }
            }
            RuntimeCommand::Remove { json } => {
                remove_darwinwine()?;
                if json { write_json(&runtime_status())?; }
            }
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
                let runtime = WineRuntime::discover()?;
                let path = prefixes.ensure(&runtime, &game_id)?;
                println!("{}", path.display());
            }
            PrefixCommand::Reset { game_id } => {
                if let Ok(runtime) = WineRuntime::discover() {
                    if prefixes.runtime_compatible(&runtime, &game_id).unwrap_or(false) {
                        let _ = runtime.stop_prefix(&prefixes.path(&game_id)?);
                    }
                }
                prefixes.reset(&game_id)?;
            }
        },
        Command::Steam { command } => match command {
            SteamCommand::Status { json } => {
                let runtime = WineRuntime::discover().ok();
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
                let runtime = WineRuntime::discover()?;
                let status = steam.install(&runtime, installer.as_deref(), json)?;
                if !json {
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
                    println!("Steam GPU acceleration registry setting disabled: {}", diagnostics.registry_gpu_acceleration_disabled);
                    println!("Vulkan mentioned in CEF/WebHelper logs: {}", diagnostics.vulkan_observed);
                    println!("CEF --in-process-gpu observed: {}", diagnostics.in_process_gpu_observed);
                    if let Some(command_line) = diagnostics.webhelper_command_line {
                        println!("WebHelper command line: {command_line}");
                    }
                }
            }
            SteamCommand::Start { json } => {
                let runtime = WineRuntime::discover()?;
                steam.start(&runtime, json)?;
            }
            SteamCommand::Restart { json } => {
                let runtime = WineRuntime::discover()?;
                steam.restart(&runtime, json)?;
            }
            SteamCommand::Run { app_id, json } => {
                let runtime = WineRuntime::discover()?;
                steam.launch_game(&runtime, app_id, json)?;
            }
            SteamCommand::Profile { command } => match command {
                SteamProfileCommand::Show { app_id, json } => {
                    let profile = steam.profile(app_id)?;
                    if json {
                        write_json(&profile)?;
                    } else {
                        print_profile(&profile);
                    }
                }
                SteamProfileCommand::Set {
                    app_id,
                    executable,
                    launch_arguments,
                    json,
                } => {
                    let profile =
                        steam.set_profile(app_id, executable.as_deref(), launch_arguments)?;
                    if json {
                        write_json(&profile)?;
                    } else {
                        print_profile(&profile);
                    }
                }
                SteamProfileCommand::Reset { app_id, json } => {
                    let profile = steam.reset_profile(app_id)?;
                    if json {
                        write_json(&profile)?;
                    } else {
                        print_profile(&profile);
                    }
                }
            },
            SteamCommand::Stop => {
                let runtime = WineRuntime::discover()?;
                steam.stop(&runtime)?;
            }
            SteamCommand::Reset => {
                let runtime = WineRuntime::discover()?;
                steam.reset(&runtime)?;
            }
        },
        Command::Launch {
            game_id,
            executable,
            json,
        } => {
            let runtime = WineRuntime::discover()?;
            inspect_pe(&executable)?;
            let prefix = prefixes.ensure(&runtime, &game_id)?;
            prefixes.bind_game_drive(&prefix, &executable)?;
            runtime.launch(&prefix, &executable, json)?;
        }
        Command::Stop { game_id } => {
            let runtime = WineRuntime::discover()?;
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
