use crate::app_dirs::application_support;
use crate::compatibility::{validate_relative_executable, CompatibilityManager, SteamCompatibilityProfile};
use crate::error::{AppError, Result};
use crate::events::{write_json, write_progress, RuntimeEvent};
use crate::pe::inspect_pe;
use crate::prefix::PrefixManager;
use crate::vdf::{self, VdfValue};
use crate::wine::WineRuntime;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const STEAM_PREFIX_ID: &str = "steam";
const STEAM_INSTALLER_URL: &str = "https://cdn.fastly.steamstatic.com/client/installer/SteamSetup.exe";
const STEAM_RESTART_EXIT_CODE: i32 = 42;
const STEAM_RESTART_LIMIT: usize = 3;
const STEAM_INSTALL_TIMEOUT_SECS: u64 = 90;
const STEAM_UI_POLICY_VERSION: u32 = 10;

/// Injects `--in-process-gpu` into every CEF process Steam spawns. Without it
/// Steam's windows paint into nothing and render black: Wine implements
/// cross-process rendering only in winex11.drv, and winemac.drv registers no
/// pGetDC to compensate, so Chromium presenting into an HWND owned by a remote
/// process is discarded. Steam does not forward the flag from its own command
/// line, and it re-runs steamwebhelper.exe for each CEF child, so the flag has
/// to be injected at that binary.
const STEAM_WEBHELPER_SHIM: &[u8] = include_bytes!("../assets/steamwebhelper-shim.exe");
const STEAM_WEBHELPER_SHIM_MARKER: &[u8] = b"DARWINPLAY_SWH_SHIM_V1";
/// The genuine helper is ~7 MiB, so anything this small cannot be it.
const STEAM_WEBHELPER_SHIM_MAX_LEN: u64 = 1_048_576;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamStatus {
    pub installed: bool,
    pub running: bool,
    pub prefix: String,
    pub steam_path: Option<String>,
    pub games_installed: usize,
    pub ui_policy_current: bool,
    pub prefix_runtime_compatible: bool,
    pub prefix_runtime_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamGame {
    pub app_id: u32,
    pub name: String,
    pub install_dir: String,
    pub install_path: String,
    pub manifest_path: String,
    pub state_flags: u64,
    pub size_on_disk: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamLibrary {
    pub games: Vec<SteamGame>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamUiDiagnostics {
    pub webhelper_log_path: Option<String>,
    pub cef_log_path: Option<String>,
    pub webhelper_command_line: Option<String>,
    pub disable_gpu_observed: bool,
    pub disable_gpu_compositing_observed: bool,
    pub registry_gpu_acceleration_disabled: bool,
    pub vulkan_observed: bool,
    /// The shim is what keeps Steam's windows from rendering black.
    pub in_process_gpu_observed: bool,
}

struct SteamExecutable {
    host_path: PathBuf,
    windows_path: &'static str,
}

pub struct SteamManager {
    prefixes: PrefixManager,
    compatibility: CompatibilityManager,
}

impl SteamManager {
    pub fn new() -> Result<Self> {
        Ok(Self {
            prefixes: PrefixManager::new()?,
            compatibility: CompatibilityManager::new()?,
        })
    }

    pub fn prefix_path(&self) -> Result<PathBuf> {
        self.prefixes.path(STEAM_PREFIX_ID)
    }

    pub fn status(&self, runtime: Option<&WineRuntime>) -> Result<SteamStatus> {
        let prefix = self.prefix_path()?;
        let steam_path = find_steam_executable(&prefix);
        let games_installed = if steam_path.is_some() {
            self.games().map(|library| library.games.len()).unwrap_or(0)
        } else {
            0
        };
        let prefix_runtime_version = self.prefixes.recorded_runtime_version(STEAM_PREFIX_ID)?;
        let prefix_runtime_compatible = runtime
            .map(|runtime| self.prefixes.runtime_compatible(runtime, STEAM_PREFIX_ID))
            .transpose()?
            .unwrap_or(true);
        let running = if steam_path.is_some() && prefix_runtime_compatible {
            runtime
                .and_then(|runtime| runtime.is_windows_process_running(&prefix, "steam.exe").ok())
                .unwrap_or(false)
        } else {
            false
        };
        let session = if running { read_steam_ui_session().ok().flatten() } else { None };
        let ui_policy_current = !running
            || session
                .as_ref()
                .map(|value| value.ui_policy_version == STEAM_UI_POLICY_VERSION)
                .unwrap_or(false);
        Ok(SteamStatus {
            installed: steam_path.is_some(),
            running,
            prefix: prefix.display().to_string(),
            steam_path: steam_path.map(|steam| steam.host_path.display().to_string()),
            games_installed,
            ui_policy_current,
            prefix_runtime_compatible,
            prefix_runtime_version,
        })
    }

    pub fn install(
        &self,
        runtime: &WineRuntime,
        installer: Option<&Path>,
        json: bool,
    ) -> Result<SteamStatus> {
        emit_install_progress(json, "Preparing", "Preparing Steam prefix", None, Some(0.02), None, None)?;
        let prefix = match self.prefixes.ensure(runtime, STEAM_PREFIX_ID) {
            Ok(prefix) => prefix,
            Err(error @ AppError::CorruptPrefix(_)) => {
                let prefix = self.prefix_path()?;
                if find_steam_executable(&prefix).is_some() {
                    return Err(error);
                }
                let _ = runtime.stop_prefix(&prefix);
                self.prefixes.reset(STEAM_PREFIX_ID)?;
                self.prefixes.ensure(runtime, STEAM_PREFIX_ID)?
            }
            Err(error) => return Err(error),
        };
        if find_steam_executable(&prefix).is_some() {
            emit_install_progress(json, "Ready", "Steam is already installed", Some(1.0), Some(1.0), None, None)?;
            return self.status(Some(runtime));
        }
        let installer_path = match installer {
            Some(path) => {
                if !path.is_file() {
                    return Err(AppError::InvalidFile(path.display().to_string()));
                }
                path.to_path_buf()
            }
            None => download_installer(json)?,
        };
        emit_install_progress(json, "Verifying", "Verifying Steam installer", None, Some(0.46), None, None)?;
        inspect_pe(&installer_path)?;
        let installer_directory = installer_path
            .parent()
            .ok_or_else(|| AppError::MissingParent(installer_path.display().to_string()))?;
        let installer_name = installer_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::MissingFileName(installer_path.display().to_string()))?;
        if runtime
            .is_windows_process_running(&prefix, "SteamSetup.exe")
            .unwrap_or(false)
        {
            runtime.stop_prefix(&prefix)?;
            thread::sleep(Duration::from_millis(250));
        }
        emit_install_progress(json, "Installing", "Starting Steam installer", None, Some(0.56), None, None)?;
        self.prefixes.bind_drive(&prefix, 'i', installer_directory)?;
        let windows_installer = format!("I:\\{installer_name}");
        let installer_arguments = vec!["/S".to_string()];
        let install_result = runtime.run_windows_blocking(
            &prefix,
            &windows_installer,
            &installer_arguments,
            Duration::from_secs(STEAM_INSTALL_TIMEOUT_SECS),
        );
        let _ = runtime.stop_prefix(&prefix);
        let unbind_result = self.prefixes.unbind_drive(&prefix, 'i');
        unbind_result?;
        if find_steam_executable(&prefix).is_none() {
            install_result?;
            return Err(AppError::SteamInstallationMissing);
        }
        install_result?;
        emit_install_progress(json, "Finalizing", "Finalizing Steam installation", None, Some(0.94), None, None)?;
        let _ = clear_steam_ui_session();
        let status = self.status(Some(runtime))?;
        emit_install_progress(json, "Ready", "Steam is installed", Some(1.0), Some(1.0), None, None)?;
        Ok(status)
    }

    pub fn ui_diagnostics(&self) -> Result<SteamUiDiagnostics> {
        let prefix = self.prefix_path()?;
        let steam = find_steam_executable(&prefix).ok_or(AppError::SteamNotInstalled)?;
        let steam_root = steam
            .host_path
            .parent()
            .ok_or_else(|| AppError::MissingParent(steam.host_path.display().to_string()))?;
        let webhelper_path = steam_root.join("logs/webhelper.txt");
        let cef_path = steam_root.join("logs/cef_log.txt");
        let webhelper = read_log_tail(&webhelper_path, 1024 * 1024);
        let cef = read_log_tail(&cef_path, 1024 * 1024);
        let command_line = webhelper
            .as_deref()
            .and_then(latest_webhelper_command_line);
        let combined = format!("{}\n{}", webhelper.as_deref().unwrap_or(""), cef.as_deref().unwrap_or(""));
        let normalized = combined.to_ascii_lowercase();
        let command = command_line.as_deref().unwrap_or("").to_ascii_lowercase();
        Ok(SteamUiDiagnostics {
            webhelper_log_path: webhelper_path.is_file().then(|| webhelper_path.display().to_string()),
            cef_log_path: cef_path.is_file().then(|| cef_path.display().to_string()),
            webhelper_command_line: command_line,
            disable_gpu_observed: command.contains("--disable-gpu"),
            disable_gpu_compositing_observed: command.contains("--disable-gpu-compositing"),
            registry_gpu_acceleration_disabled: steam_gpu_acceleration_disabled_in_registry(&prefix),
            vulkan_observed: normalized.contains("vulkan") || normalized.contains("angle_platform_vulkan"),
            in_process_gpu_observed: command.contains("--in-process-gpu"),
        })
    }

    pub fn games(&self) -> Result<SteamLibrary> {
        let prefix = self.prefix_path()?;
        let steam = find_steam_executable(&prefix).ok_or(AppError::SteamNotInstalled)?;
        let steam_root = steam
            .host_path
            .parent()
            .ok_or_else(|| AppError::MissingParent(steam.host_path.display().to_string()))?;
        let mut libraries = BTreeSet::new();
        libraries.insert(steam_root.to_path_buf());
        let library_folders = steam_root.join("steamapps/libraryfolders.vdf");
        if library_folders.is_file() {
            for path in parse_library_folders(&prefix, &library_folders)? {
                libraries.insert(path);
            }
        }

        let mut games = BTreeMap::new();
        for library in libraries {
            let steamapps = library.join("steamapps");
            if !steamapps.is_dir() {
                continue;
            }
            let mut entries: Vec<_> = fs::read_dir(&steamapps)?.flatten().collect();
            entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
            for entry in entries {
                let path = entry.path();
                let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                if !file_name.starts_with("appmanifest_") || !file_name.ends_with(".acf") {
                    continue;
                }
                if let Ok(game) = parse_app_manifest(&path, &steamapps) {
                    games.insert(game.app_id, game);
                }
            }
        }

        Ok(SteamLibrary {
            games: games.into_values().collect(),
        })
    }

    pub fn profile(&self, app_id: u32) -> Result<SteamCompatibilityProfile> {
        let game = self.game(app_id)?;
        self.compatibility
            .analyze(app_id, &game.name, Path::new(&game.install_path))
    }

    pub fn set_profile(
        &self,
        app_id: u32,
        executable: Option<&str>,
        launch_arguments: Vec<String>,
    ) -> Result<SteamCompatibilityProfile> {
        let current = self.profile(app_id)?;
        let allowed = current
            .candidates
            .iter()
            .map(|candidate| candidate.relative_path.clone())
            .collect::<Vec<_>>();
        self.compatibility
            .save(app_id, executable, launch_arguments, &allowed)?;
        self.profile(app_id)
    }

    pub fn reset_profile(&self, app_id: u32) -> Result<SteamCompatibilityProfile> {
        self.compatibility.reset(app_id)?;
        self.profile(app_id)
    }

    pub fn start(
        &self,
        runtime: &WineRuntime,
        json: bool,
    ) -> Result<()> {
        let prefix = self.prefix_path()?;
        self.prefixes.verify_runtime(runtime, STEAM_PREFIX_ID)?;
        let steam = find_steam_executable(&prefix).ok_or(AppError::SteamNotInstalled)?;
        if runtime.is_windows_process_running(&prefix, "steam.exe")? {
            let session = read_steam_ui_session().ok().flatten();
            let compatible = session
                .as_ref()
                .map(|value| value.ui_policy_version == STEAM_UI_POLICY_VERSION)
                .unwrap_or(false);
            if compatible {
                emit_steam_state(
                    json,
                    "already_running",
                    "Steam is already running with the current CEF-safe UI policy",
                )?;
                return Ok(());
            }
            emit_steam_state(
                json,
                "ui_policy_restart",
                "Restarting Steam to apply the CEF software-rendering policy",
            )?;
            runtime.stop_prefix(&prefix)?;
            clear_steam_ui_session()?;
            thread::sleep(Duration::from_millis(350));
        } else {
            let _ = runtime.stop_prefix(&prefix);
            thread::sleep(Duration::from_millis(250));
        }
        launch_steam_client(runtime, &prefix, steam.windows_path, &[], json)
    }

    pub fn restart(&self, runtime: &WineRuntime, json: bool) -> Result<()> {
        let prefix = self.prefix_path()?;
        self.prefixes.verify_runtime(runtime, STEAM_PREFIX_ID)?;
        let steam = find_steam_executable(&prefix).ok_or(AppError::SteamNotInstalled)?;
        runtime.stop_prefix(&prefix)?;
        clear_steam_ui_session()?;
        thread::sleep(Duration::from_millis(350));
        emit_steam_state(json, "restarting_ui", "Restarting the Steam UI")?;
        launch_steam_client(runtime, &prefix, steam.windows_path, &[], json)
    }

    pub fn launch_game(
        &self,
        runtime: &WineRuntime,
        app_id: u32,
        json: bool,
    ) -> Result<()> {
        let prefix = self.prefix_path()?;
        self.prefixes.verify_runtime(runtime, STEAM_PREFIX_ID)?;
        let steam = find_steam_executable(&prefix).ok_or(AppError::SteamNotInstalled)?;
        let profile = self.profile(app_id)?;
        let (_imports, launch_arguments) = self.compatibility.launch_configuration(&profile);

        // Some titles cannot be started through Steam at all: Steam resolves the
        // executable from its own app info, and when that record carries no launch
        // block it tries to start an empty path and fails. The profile's selected
        // executable is the escape hatch -- run the binary ourselves, with Steam
        // already up so the client-side DRM is satisfied.
        if let Some(relative) = profile.selected_executable.as_deref() {
            return self.launch_selected_executable(runtime, &prefix, app_id, relative, &launch_arguments, json);
        }

        let mut arguments = vec!["-applaunch".to_string(), app_id.to_string()];
        arguments.extend(launch_arguments);
        let running = runtime.is_windows_process_running(&prefix, "steam.exe")?;
        let session = read_steam_ui_session().ok().flatten();
        let session_matches = session
            .as_ref()
            .map(|value| value.ui_policy_version == STEAM_UI_POLICY_VERSION)
            .unwrap_or(false);
        if running && session_matches {
            emit_steam_state(
                json,
                "reusing_running",
                "Using the running Steam client",
            )?;
            let _ = runtime.dispatch_windows(&prefix, steam.windows_path, &arguments)?;
            return Ok(());
        }
        if running {
            emit_steam_state(
                json,
                "ui_policy_restart",
                "Restarting Steam to apply the current UI policy",
            )?;
            runtime.stop_prefix(&prefix)?;
            thread::sleep(Duration::from_millis(350));
        }
        launch_steam_client(runtime, &prefix, steam.windows_path, &arguments, json)
    }

    fn launch_selected_executable(
        &self,
        runtime: &WineRuntime,
        prefix: &Path,
        app_id: u32,
        relative: &str,
        launch_arguments: &[String],
        json: bool,
    ) -> Result<()> {
        let game = self.game(app_id)?;
        let host = resolve_game_executable(Path::new(&game.install_path), relative)?;
        let windows_path = host_path_to_windows(prefix, &host)
            .ok_or_else(|| AppError::InvalidFile(host.display().to_string()))?;
        let working_directory = host
            .parent()
            .ok_or_else(|| AppError::MissingParent(host.display().to_string()))?;

        if !runtime.is_windows_process_running(prefix, "steam.exe")? {
            return Err(AppError::ProcessFailed(format!(
                "Steam must be running before launching {relative} directly; \
                 start Steam first, then launch the game"
            )));
        }

        emit_steam_state(
            json,
            "launching_executable",
            &format!("Launching {relative} directly"),
        )?;
        runtime
            .launch_windows_in(prefix, &windows_path, launch_arguments, json, Some(working_directory))
            .map(|_| ())
    }

    pub fn stop(&self, runtime: &WineRuntime) -> Result<()> {
        let result = runtime.stop_prefix(&self.prefix_path()?);
        let clear_result = clear_steam_ui_session();
        result?;
        clear_result
    }

    pub fn reset(&self, runtime: &WineRuntime) -> Result<()> {
        let prefix = self.prefix_path()?;
        if self.prefixes.runtime_compatible(runtime, STEAM_PREFIX_ID)? {
            let _ = runtime.stop_prefix(&prefix);
        }
        let _ = clear_steam_ui_session();
        self.prefixes.reset(STEAM_PREFIX_ID)
    }

    fn game(&self, app_id: u32) -> Result<SteamGame> {
        self.games()?
            .games
            .into_iter()
            .find(|game| game.app_id == app_id)
            .ok_or(AppError::SteamGameNotInstalled(app_id))
    }
}

fn emit_install_progress(
    json: bool,
    phase: &str,
    message: &str,
    progress: Option<f64>,
    overall_progress: Option<f64>,
    current_bytes: Option<u64>,
    total_bytes: Option<u64>,
) -> Result<()> {
    if json {
        write_progress(
            "steam_install_progress",
            phase,
            message,
            progress,
            overall_progress,
            current_bytes,
            total_bytes,
        )?;
    }
    Ok(())
}

fn steam_installer_content_length() -> Option<u64> {
    let output = Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--head",
            "--proto",
            "=https",
            "--tlsv1.2",
        ])
        .arg(STEAM_INSTALLER_URL)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case("content-length") {
                value.trim().parse::<u64>().ok()
            } else {
                None
            }
        })
        .last()
}

fn download_installer(json: bool) -> Result<PathBuf> {
    let downloads = application_support()?.join("downloads");
    fs::create_dir_all(&downloads)?;
    let target = downloads.join("SteamSetup.exe");
    let staging = downloads.join(format!(".SteamSetup-{}.tmp", std::process::id()));
    let _ = fs::remove_file(&staging);
    let _ = fs::remove_file(&target);
    let total = steam_installer_content_length();
    emit_install_progress(
        json,
        "Downloading Steam",
        "Downloading SteamSetup.exe",
        total.map(|_| 0.0),
        Some(0.08),
        Some(0),
        total,
    )?;

    let mut child = Command::new("/usr/bin/curl")
        .arg("--fail")
        .arg("--location")
        .arg("--silent")
        .arg("--show-error")
        .arg("--proto")
        .arg("=https")
        .arg("--tlsv1.2")
        .arg("--output")
        .arg(&staging)
        .arg(STEAM_INSTALLER_URL)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut last_emitted = u64::MAX;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        let current = fs::metadata(&staging).map(|metadata| metadata.len()).unwrap_or(0);
        if current != last_emitted {
            let fraction = total.filter(|value| *value > 0).map(|value| {
                (current as f64 / value as f64).clamp(0.0, 1.0)
            });
            let overall = fraction.map(|value| 0.08 + 0.34 * value);
            emit_install_progress(
                json,
                "Downloading Steam",
                "Downloading SteamSetup.exe",
                fraction,
                overall,
                Some(current),
                total,
            )?;
            last_emitted = current;
        }
        thread::sleep(Duration::from_millis(120));
    };

    if !status.success() {
        let mut stderr = String::new();
        if let Some(mut handle) = child.stderr.take() {
            let _ = handle.read_to_string(&mut stderr);
        }
        let _ = fs::remove_file(&staging);
        if !stderr.trim().is_empty() {
            return Err(AppError::ProcessFailed(format!(
                "Steam installer download failed: {}",
                stderr.trim()
            )));
        }
        return Err(AppError::SteamInstallerDownloadFailed);
    }
    let current = fs::metadata(&staging).map(|metadata| metadata.len()).unwrap_or(0);
    emit_install_progress(
        json,
        "Downloading Steam",
        "SteamSetup.exe downloaded",
        Some(1.0),
        Some(0.42),
        Some(current),
        total.or(Some(current)),
    )?;
    inspect_pe(&staging)?;
    fs::rename(&staging, &target)?;
    Ok(target)
}

fn steam_gpu_acceleration_disabled_in_registry(prefix: &Path) -> bool {
    let user_reg = prefix.join("user.reg");
    let Ok(text) = fs::read_to_string(user_reg) else {
        return false;
    };
    text.lines().any(|line| {
        let normalized = line.trim().to_ascii_lowercase();
        normalized == "\"gpuaccelwebviewsv3\"=dword:00000000"
    })
}

fn read_log_tail(path: &Path, max_bytes: usize) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let start = bytes.len().saturating_sub(max_bytes);
    Some(String::from_utf8_lossy(&bytes[start..]).into_owned())
}

fn latest_webhelper_command_line(text: &str) -> Option<String> {
    text.lines()
        .rev()
        .find_map(|line| line.split_once("commandline:").map(|(_, value)| value.trim().to_string()))
}

fn emit_steam_state(json: bool, kind: &str, message: &str) -> Result<()> {
    if json {
        write_json(&RuntimeEvent {
            kind,
            stream: None,
            message: Some(message),
            pid: None,
            exit_code: None,
            prefix: None,
        })
    } else {
        println!("{message}");
        Ok(())
    }
}

fn steam_cef_safe_arguments(arguments: &[String]) -> Vec<String> {
    // -cef-disable-gpu was an attempt to work around the black-window bug. It
    // never fixed it and only costs acceleration, so it is dropped even when a
    // caller asks for it. -noverifyfiles keeps Steam from repairing the shim
    // back to the stock helper mid-session.
    let mut result: Vec<String> = arguments
        .iter()
        .filter(|argument| !argument.eq_ignore_ascii_case("-cef-disable-gpu"))
        .cloned()
        .collect();
    for flag in ["-system-composer", "-noverifyfiles"] {
        if !result.iter().any(|argument| argument.eq_ignore_ascii_case(flag)) {
            result.insert(0, flag.to_string());
        }
    }
    result
}

/// True when `path` is our shim rather than Steam's own helper.
fn is_webhelper_shim(path: &Path) -> Result<bool> {
    if fs::metadata(path)?.len() > STEAM_WEBHELPER_SHIM_MAX_LEN {
        return Ok(false);
    }
    let bytes = fs::read(path)?;
    Ok(bytes
        .windows(STEAM_WEBHELPER_SHIM_MARKER.len())
        .any(|window| window == STEAM_WEBHELPER_SHIM_MARKER))
}

/// Installs the shim into every CEF directory Steam ships, preserving the stock
/// helper as steamwebhelper_real.exe. Idempotent, and re-runs cheaply after
/// Steam restores its own binary on update or file verification. Returns the
/// number of directories that had to be (re)shimmed.
fn install_webhelper_shim(prefix: &Path) -> Result<usize> {
    let cef_root = prefix.join("drive_c/Program Files (x86)/Steam/bin/cef");
    if !cef_root.is_dir() {
        return Ok(0);
    }
    let mut installed = 0;
    for entry in fs::read_dir(&cef_root)? {
        let directory = entry?.path();
        let live = directory.join("steamwebhelper.exe");
        if !live.is_file() {
            continue;
        }
        let stock = directory.join("steamwebhelper_real.exe");
        if is_webhelper_shim(&live)? {
            if stock.is_file() {
                continue;
            }
            return Err(AppError::ProcessFailed(format!(
                "{} is the DarwinPlay shim but {} is missing; \
                 run Steam's own file verification to restore the helper",
                live.display(),
                stock.display()
            )));
        }
        // `live` is Steam's own helper: either a first install, or Steam just
        // replaced the shim. Refresh the preserved copy so an updated helper is
        // what actually runs.
        fs::copy(&live, &stock)?;
        fs::write(&live, STEAM_WEBHELPER_SHIM)?;
        installed += 1;
    }
    Ok(installed)
}

fn launch_steam_client(
    runtime: &WineRuntime,
    prefix: &Path,
    executable: &str,
    arguments: &[String],
    json: bool,
) -> Result<()> {
    let client_arguments = steam_cef_safe_arguments(arguments);
    apply_steam_cef_safe_mode(runtime, prefix)?;
    if install_webhelper_shim(prefix)? > 0 {
        emit_steam_state(
            json,
            "webhelper_shim",
            "Installed CEF in-process-GPU shim (Steam had restored its own helper)",
        )?;
    }
    write_steam_ui_session()?;
    for attempt in 0..STEAM_RESTART_LIMIT {
        let exit_code = match runtime.launch_windows(
            prefix,
            executable,
            &client_arguments,
            json,
        ) {
            Ok(exit_code) => exit_code,
            Err(error) => {
                let _ = clear_steam_ui_session();
                return Err(error);
            }
        };
        if exit_code != STEAM_RESTART_EXIT_CODE {
            return Ok(());
        }
        let _ = runtime.stop_prefix(prefix);
        if attempt + 1 < STEAM_RESTART_LIMIT {
            thread::sleep(Duration::from_millis(750));
        }
    }
    Err(AppError::ProcessFailed(format!(
        "Steam requested more than {STEAM_RESTART_LIMIT} consecutive client restarts"
    )))
}

fn apply_steam_cef_safe_mode(runtime: &WineRuntime, prefix: &Path) -> Result<()> {
    let arguments = vec![
        "add".to_string(),
        "HKCU\\Software\\Valve\\Steam".to_string(),
        "/v".to_string(),
        "GPUAccelWebViewsV3".to_string(),
        "/t".to_string(),
        "REG_DWORD".to_string(),
        "/d".to_string(),
        "0".to_string(),
        "/f".to_string(),
    ];
    runtime.dispatch_windows(prefix, "C:\\windows\\system32\\reg.exe", &arguments)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SteamUiSession {
    ui_policy_version: u32,
}

fn steam_ui_session_path() -> Result<PathBuf> {
    Ok(application_support()?.join("runtime-state/steam-ui-session.json"))
}

fn write_steam_ui_session() -> Result<()> {
    let path = steam_ui_session_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let value = serde_json::to_vec(&SteamUiSession {
        ui_policy_version: STEAM_UI_POLICY_VERSION,
    })?;
    let staging = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&staging, value)?;
    fs::rename(staging, path)?;
    Ok(())
}

fn read_steam_ui_session() -> Result<Option<SteamUiSession>> {
    let path = steam_ui_session_path()?;
    let Ok(bytes) = fs::read(path) else {
        return Ok(None);
    };
    let Ok(session) = serde_json::from_slice::<SteamUiSession>(&bytes) else {
        return Ok(None);
    };
    Ok(Some(session))
}

fn clear_steam_ui_session() -> Result<()> {
    let path = steam_ui_session_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn find_steam_executable(prefix: &Path) -> Option<SteamExecutable> {
    [
        (
            prefix.join("drive_c/Program Files (x86)/Steam/steam.exe"),
            "C:\\Program Files (x86)\\Steam\\steam.exe",
        ),
        (
            prefix.join("drive_c/Program Files/Steam/steam.exe"),
            "C:\\Program Files\\Steam\\steam.exe",
        ),
    ]
    .into_iter()
    .find_map(|(host_path, windows_path)| {
        host_path.is_file().then_some(SteamExecutable {
            host_path,
            windows_path,
        })
    })
}

fn parse_library_folders(prefix: &Path, path: &Path) -> Result<Vec<PathBuf>> {
    let text = fs::read_to_string(path)?;
    let parsed = vdf::parse(&text)?;
    let Some(root) = parsed.get("libraryfolders").and_then(VdfValue::object) else {
        return Err(AppError::InvalidVdf("libraryfolders root is missing".into()));
    };
    let mut result = Vec::new();
    for value in root.values() {
        let windows_path = match value {
            VdfValue::String(path) => Some(path.as_str()),
            VdfValue::Object(object) => object.get("path").and_then(VdfValue::string),
        };
        if let Some(host_path) = windows_path.and_then(|path| windows_path_to_host(prefix, path)) {
            result.push(host_path);
        }
    }
    Ok(result)
}

fn parse_app_manifest(path: &Path, steamapps: &Path) -> Result<SteamGame> {
    let text = fs::read_to_string(path)?;
    let parsed = vdf::parse(&text)?;
    let root = parsed
        .get("AppState")
        .and_then(VdfValue::object)
        .ok_or_else(|| AppError::InvalidVdf("AppState root is missing".into()))?;
    let app_id = required_string(root, "appid")?
        .parse::<u32>()
        .map_err(|_| AppError::InvalidVdf("appid is not an unsigned integer".into()))?;
    let name = required_string(root, "name")?.to_string();
    let install_dir = required_string(root, "installdir")?.to_string();
    let state_flags = optional_u64(root, "StateFlags");
    let size_on_disk = optional_u64(root, "SizeOnDisk");
    let install_path = steamapps.join("common").join(&install_dir);
    if !install_path.is_dir() {
        return Err(AppError::SteamGameNotInstalled(app_id));
    }
    Ok(SteamGame {
        app_id,
        name,
        install_dir,
        install_path: install_path.display().to_string(),
        manifest_path: path.display().to_string(),
        state_flags,
        size_on_disk,
    })
}

fn required_string<'a>(object: &'a BTreeMap<String, VdfValue>, key: &str) -> Result<&'a str> {
    object
        .get(key)
        .and_then(VdfValue::string)
        .ok_or_else(|| AppError::InvalidVdf(format!("missing {key}")))
}

fn optional_u64(object: &BTreeMap<String, VdfValue>, key: &str) -> u64 {
    object
        .get(key)
        .and_then(VdfValue::string)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

/// Resolve a profile-selected executable against the game's install directory,
/// rejecting anything that escapes it.
fn resolve_game_executable(install_path: &Path, relative: &str) -> Result<PathBuf> {
    validate_relative_executable(relative)?;
    let mut host = install_path.to_path_buf();
    for component in relative.split('/').filter(|value| !value.is_empty()) {
        host.push(component);
    }
    let host = normalize_host_path(host);
    if !host.starts_with(normalize_host_path(install_path.to_path_buf())) {
        return Err(AppError::InvalidFile(host.display().to_string()));
    }
    if !host.is_file() {
        return Err(AppError::InvalidFile(host.display().to_string()));
    }
    Ok(host)
}

/// Inverse of windows_path_to_host: express a host path inside the prefix as the
/// DOS path Wine will accept.
fn host_path_to_windows(prefix: &Path, host: &Path) -> Option<String> {
    let host = normalize_host_path(host.to_path_buf());
    let dosdevices = prefix.join("dosdevices");
    let mut entries: Vec<_> = fs::read_dir(&dosdevices).ok()?.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Only drive letters; the ::$ device links are not path roots.
        if name.len() != 2 || !name.ends_with(':') || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        let target = fs::read_link(entry.path()).ok()?;
        let base = if target.is_absolute() {
            target
        } else {
            dosdevices.join(target)
        };
        let base = normalize_host_path(base);
        if let Ok(rest) = host.strip_prefix(&base) {
            let drive = name.chars().next()?.to_ascii_uppercase();
            let tail = rest
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("\\");
            return Some(format!("{drive}:\\{tail}"));
        }
    }
    None
}

fn windows_path_to_host(prefix: &Path, value: &str) -> Option<PathBuf> {
    let normalized = value.replace('/', "\\");
    let bytes = normalized.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return None;
    }
    let drive = (bytes[0] as char).to_ascii_lowercase();
    let link = prefix.join("dosdevices").join(format!("{drive}:"));
    let target = fs::read_link(&link).ok()?;
    let base = if target.is_absolute() {
        target
    } else {
        link.parent()?.join(target)
    };
    let mut result = normalize_host_path(base);
    for component in normalized[3..].split('\\').filter(|value| !value.is_empty()) {
        if component == "." || component == ".." {
            return None;
        }
        result.push(component);
    }
    Some(result)
}

fn normalize_host_path(path: PathBuf) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn expresses_a_host_path_as_a_dos_path() {
        let root = std::env::temp_dir().join(format!("darwinplay-hostpath-{}", std::process::id()));
        let prefix = root.join("prefix");
        fs::create_dir_all(prefix.join("dosdevices")).unwrap();
        fs::create_dir_all(prefix.join("drive_c/games/witcher/bin")).unwrap();
        symlink("../drive_c", prefix.join("dosdevices/c:")).unwrap();

        let host = prefix.join("drive_c/games/witcher/bin/game.exe");
        assert_eq!(
            host_path_to_windows(&prefix, &host).as_deref(),
            Some("C:\\games\\witcher\\bin\\game.exe")
        );
        // A path outside every mapped drive has no DOS equivalent.
        assert!(host_path_to_windows(&prefix, Path::new("/etc/hosts")).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selected_executable_must_stay_inside_the_installation() {
        let root = std::env::temp_dir().join(format!("darwinplay-selected-{}", std::process::id()));
        let install = root.join("The Witcher 3");
        fs::create_dir_all(install.join("bin/x64")).unwrap();
        fs::write(install.join("bin/x64/witcher3.exe"), b"MZ").unwrap();
        fs::write(root.join("outside.exe"), b"MZ").unwrap();

        assert!(resolve_game_executable(&install, "bin/x64/witcher3.exe").is_ok());
        // Escaping the install directory and naming a missing file both fail.
        assert!(resolve_game_executable(&install, "../outside.exe").is_err());
        assert!(resolve_game_executable(&install, "bin/x64/missing.exe").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn maps_windows_drive_to_prefix() {
        let root = std::env::temp_dir().join(format!("darwinplay-steam-test-{}", std::process::id()));
        let prefix = root.join("prefix");
        fs::create_dir_all(prefix.join("dosdevices")).unwrap();
        fs::create_dir_all(prefix.join("drive_c")).unwrap();
        symlink("../drive_c", prefix.join("dosdevices/c:")).unwrap();
        let mapped = windows_path_to_host(&prefix, "C:\\Program Files (x86)\\Steam").unwrap();
        assert_eq!(mapped, prefix.join("drive_c/Program Files (x86)/Steam"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_app_manifest() {
        let root = std::env::temp_dir().join(format!("darwinplay-manifest-test-{}", std::process::id()));
        let steamapps = root.join("steamapps");
        fs::create_dir_all(steamapps.join("common/dota 2 beta")).unwrap();
        let manifest = steamapps.join("appmanifest_570.acf");
        fs::write(
            &manifest,
            r#""AppState" { "appid" "570" "name" "Dota 2" "StateFlags" "4" "installdir" "dota 2 beta" "SizeOnDisk" "100" }"#,
        )
        .unwrap();
        let game = parse_app_manifest(&manifest, &steamapps).unwrap();
        assert_eq!(game.app_id, 570);
        assert_eq!(game.name, "Dota 2");
        assert_eq!(game.state_flags, 4);
        assert_eq!(game.size_on_disk, 100);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn extracts_latest_webhelper_command_line() {
        let text = "[1] Startup - webhelper launched pid: 1 commandline: old\n[2] Startup - webhelper launched pid: 2 commandline: steamwebhelper.exe --disable-gpu --disable-gpu-compositing";
        assert_eq!(
            latest_webhelper_command_line(text).as_deref(),
            Some("steamwebhelper.exe --disable-gpu --disable-gpu-compositing")
        );
    }


    #[test]
    fn steam_cef_safe_mode_adds_composer_and_noverify_once() {
        let arguments = vec!["-silent".to_string()];
        assert_eq!(
            steam_cef_safe_arguments(&arguments),
            vec![
                "-noverifyfiles".to_string(),
                "-system-composer".to_string(),
                "-silent".to_string(),
            ]
        );
        let existing = vec![
            "-NOVERIFYFILES".to_string(),
            "-SYSTEM-COMPOSER".to_string(),
            "-silent".to_string(),
        ];
        assert_eq!(steam_cef_safe_arguments(&existing), existing);
    }

    #[test]
    fn steam_cef_safe_mode_drops_disable_gpu() {
        let arguments = vec!["-cef-disable-gpu".to_string(), "-silent".to_string()];
        let result = steam_cef_safe_arguments(&arguments);
        assert!(!result.iter().any(|a| a.eq_ignore_ascii_case("-cef-disable-gpu")));
        assert!(result.contains(&"-silent".to_string()));
    }

    #[test]
    fn recognises_its_own_shim_but_not_the_stock_helper() {
        let root = std::env::temp_dir().join(format!("dp-shim-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();

        let shim = root.join("steamwebhelper.exe");
        fs::write(&shim, STEAM_WEBHELPER_SHIM).unwrap();
        assert!(is_webhelper_shim(&shim).unwrap());

        // Stock helper: large and without the marker.
        let stock = root.join("steamwebhelper_real.exe");
        fs::write(&stock, vec![0u8; (STEAM_WEBHELPER_SHIM_MAX_LEN + 1) as usize]).unwrap();
        assert!(!is_webhelper_shim(&stock).unwrap());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn shim_install_preserves_stock_helper_and_is_idempotent() {
        let root = std::env::temp_dir().join(format!("dp-shim-inst-{}", std::process::id()));
        let cef = root.join("drive_c/Program Files (x86)/Steam/bin/cef/cef.win64");
        fs::create_dir_all(&cef).unwrap();
        let live = cef.join("steamwebhelper.exe");
        fs::write(&live, b"ORIGINAL-HELPER").unwrap();

        assert_eq!(install_webhelper_shim(&root).unwrap(), 1);
        assert!(is_webhelper_shim(&live).unwrap());
        assert_eq!(
            fs::read(cef.join("steamwebhelper_real.exe")).unwrap(),
            b"ORIGINAL-HELPER"
        );

        // Already shimmed: nothing to do, and the preserved copy is untouched.
        assert_eq!(install_webhelper_shim(&root).unwrap(), 0);

        // Steam repaired its helper: reshim, and pick up the newer binary.
        fs::write(&live, b"UPDATED-HELPER").unwrap();
        assert_eq!(install_webhelper_shim(&root).unwrap(), 1);
        assert_eq!(
            fs::read(cef.join("steamwebhelper_real.exe")).unwrap(),
            b"UPDATED-HELPER"
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn steam_ui_session_records_the_policy_version() {
        let session = SteamUiSession {
            ui_policy_version: STEAM_UI_POLICY_VERSION,
        };
        assert_eq!(session.ui_policy_version, STEAM_UI_POLICY_VERSION);
    }
}
