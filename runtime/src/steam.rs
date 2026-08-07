use crate::app_dirs::application_support;
use crate::compatibility::{
    BackendOverride, CompatibilityManager, SteamCompatibilityProfile,
};
use crate::error::{AppError, Result};
use crate::events::{write_json, RuntimeEvent};
use crate::graphics::{GraphicsBackend, GraphicsManager};
use crate::pe::inspect_pe;
use crate::prefix::PrefixManager;
use crate::vdf::{self, VdfValue};
use crate::wine::WineRuntime;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const STEAM_PREFIX_ID: &str = "steam";
const STEAM_INSTALLER_URL: &str = "https://cdn.akamai.steamstatic.com/client/installer/SteamSetup.exe";
const STEAM_RESTART_EXIT_CODE: i32 = 42;
const STEAM_RESTART_LIMIT: usize = 3;
const STEAM_INSTALL_TIMEOUT_SECS: u64 = 90;
const STEAM_UI_POLICY_VERSION: u32 = 3;
const STEAM_UI_ARGUMENTS: [&str; 5] = [
    "-cef-disable-gpu",
    "-cef-disable-gpu-compositing",
    "-cef-disable-occlusion",
    "-opengl",
    "-system-composer",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamStatus {
    pub installed: bool,
    pub running: bool,
    pub prefix: String,
    pub steam_path: Option<String>,
    pub games_installed: usize,
    pub ui_policy_current: bool,
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
    pub vulkan_observed: bool,
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
        let running = if steam_path.is_some() {
            runtime
                .and_then(|runtime| runtime.is_windows_process_running(&prefix, "steam.exe").ok())
                .unwrap_or(false)
        } else {
            false
        };
        let ui_policy_current = !running || steam_ui_policy_current().unwrap_or(false);
        Ok(SteamStatus {
            installed: steam_path.is_some(),
            running,
            prefix: prefix.display().to_string(),
            steam_path: steam_path.map(|steam| steam.host_path.display().to_string()),
            games_installed,
            ui_policy_current,
        })
    }

    pub fn install(
        &self,
        runtime: &WineRuntime,
        graphics: &GraphicsManager,
        installer: Option<&Path>,
    ) -> Result<SteamStatus> {
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
            return self.status(Some(runtime));
        }
        let installer_path = match installer {
            Some(path) => {
                if !path.is_file() {
                    return Err(AppError::InvalidFile(path.display().to_string()));
                }
                path.to_path_buf()
            }
            None => download_installer()?,
        };
        inspect_pe(&installer_path)?;
        let launch_graphics = graphics.prepare_launch(
            GraphicsBackend::WineD3d,
            &[],
            &prefix,
            STEAM_PREFIX_ID,
        )?;
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
        self.prefixes.bind_drive(&prefix, 'i', installer_directory)?;
        let windows_installer = format!("I:\\{installer_name}");
        let installer_arguments = vec!["/S".to_string()];
        let install_result = runtime.run_windows_blocking(
            &prefix,
            &windows_installer,
            &installer_arguments,
            &launch_graphics,
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
        let _ = clear_steam_ui_session();
        self.status(Some(runtime))
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
            vulkan_observed: normalized.contains("vulkan") || normalized.contains("angle_platform_vulkan"),
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

    pub fn profile(
        &self,
        graphics: &GraphicsManager,
        app_id: u32,
        fallback_backend: GraphicsBackend,
    ) -> Result<SteamCompatibilityProfile> {
        let game = self.game(app_id)?;
        let dxmt_installed = graphics.dxmt_status()?.installed;
        self.compatibility.analyze(
            app_id,
            &game.name,
            Path::new(&game.install_path),
            dxmt_installed,
            fallback_backend,
        )
    }

    pub fn set_profile(
        &self,
        graphics: &GraphicsManager,
        app_id: u32,
        backend: BackendOverride,
        executable: Option<&str>,
        launch_arguments: Vec<String>,
        fallback_backend: GraphicsBackend,
    ) -> Result<SteamCompatibilityProfile> {
        let current = self.profile(graphics, app_id, fallback_backend)?;
        let allowed = current
            .candidates
            .iter()
            .map(|candidate| candidate.relative_path.clone())
            .collect::<Vec<_>>();
        self.compatibility
            .save(app_id, backend, executable, launch_arguments, &allowed)?;
        self.profile(graphics, app_id, fallback_backend)
    }

    pub fn reset_profile(
        &self,
        graphics: &GraphicsManager,
        app_id: u32,
        fallback_backend: GraphicsBackend,
    ) -> Result<SteamCompatibilityProfile> {
        self.compatibility.reset(app_id)?;
        self.profile(graphics, app_id, fallback_backend)
    }

    pub fn start(
        &self,
        runtime: &WineRuntime,
        graphics: &GraphicsManager,
        backend: GraphicsBackend,
        json: bool,
    ) -> Result<()> {
        let prefix = self.prefix_path()?;
        let steam = find_steam_executable(&prefix).ok_or(AppError::SteamNotInstalled)?;
        if runtime.is_windows_process_running(&prefix, "steam.exe")? {
            if steam_ui_policy_current().unwrap_or(false) {
                emit_steam_state(json, "already_running", "Steam is already running")?;
            } else {
                emit_steam_state(
                    json,
                    "ui_restart_required",
                    "Steam is running with an older UI compatibility policy; restart the UI to apply the current renderer settings",
                )?;
            }
            return Ok(());
        }
        let launch_graphics = graphics.prepare_launch(backend, &[], &prefix, STEAM_PREFIX_ID)?;
        let _ = runtime.stop_prefix(&prefix);
        thread::sleep(Duration::from_millis(250));
        launch_steam_client(
            runtime,
            &prefix,
            steam.windows_path,
            &[],
            json,
            &launch_graphics,
        )
    }

    pub fn restart(
        &self,
        runtime: &WineRuntime,
        graphics: &GraphicsManager,
        backend: GraphicsBackend,
        json: bool,
    ) -> Result<()> {
        let prefix = self.prefix_path()?;
        let steam = find_steam_executable(&prefix).ok_or(AppError::SteamNotInstalled)?;
        let launch_graphics = graphics.prepare_launch(backend, &[], &prefix, STEAM_PREFIX_ID)?;
        runtime.stop_prefix(&prefix)?;
        clear_steam_ui_session()?;
        thread::sleep(Duration::from_millis(350));
        emit_steam_state(json, "restarting_ui", "Restarting Steam UI in compatibility mode")?;
        launch_steam_client(
            runtime,
            &prefix,
            steam.windows_path,
            &[],
            json,
            &launch_graphics,
        )
    }

    pub fn launch_game(
        &self,
        runtime: &WineRuntime,
        graphics: &GraphicsManager,
        app_id: u32,
        fallback_backend: GraphicsBackend,
        json: bool,
    ) -> Result<()> {
        let prefix = self.prefix_path()?;
        let steam = find_steam_executable(&prefix).ok_or(AppError::SteamNotInstalled)?;
        let profile = self.profile(graphics, app_id, fallback_backend)?;
        let (backend, imports, launch_arguments) = self
            .compatibility
            .launch_configuration(&profile, fallback_backend);
        let mut arguments = vec!["-applaunch".to_string(), app_id.to_string()];
        arguments.extend(launch_arguments);
        if runtime.is_windows_process_running(&prefix, "steam.exe")? {
            emit_steam_state(
                json,
                "reusing_running",
                "Using the running Steam client and its current graphics environment",
            )?;
            let _ = runtime.dispatch_windows(&prefix, steam.windows_path, &arguments)?;
            return Ok(());
        }
        let game_runtime_id = format!("steam-{app_id}");
        let launch_graphics = graphics.prepare_launch(backend, &imports, &prefix, &game_runtime_id)?;
        let _ = runtime.stop_prefix(&prefix);
        thread::sleep(Duration::from_millis(250));
        launch_steam_client(
            runtime,
            &prefix,
            steam.windows_path,
            &arguments,
            json,
            &launch_graphics,
        )
    }

    pub fn stop(&self, runtime: &WineRuntime) -> Result<()> {
        let result = runtime.stop_prefix(&self.prefix_path()?);
        let clear_result = clear_steam_ui_session();
        result?;
        clear_result
    }

    pub fn reset(&self, runtime: &WineRuntime) -> Result<()> {
        let prefix = self.prefix_path()?;
        let _ = runtime.stop_prefix(&prefix);
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

fn download_installer() -> Result<PathBuf> {
    let downloads = application_support()?.join("downloads");
    fs::create_dir_all(&downloads)?;
    let target = downloads.join("SteamSetup.exe");
    let staging = downloads.join(format!(".SteamSetup-{}.tmp", std::process::id()));
    if target.is_file() && inspect_pe(&target).is_ok() {
        return Ok(target);
    }
    let _ = fs::remove_file(&staging);
    let status = Command::new("/usr/bin/curl")
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
        .status()?;
    if !status.success() {
        let _ = fs::remove_file(&staging);
        return Err(AppError::SteamInstallerDownloadFailed);
    }
    inspect_pe(&staging)?;
    fs::rename(&staging, &target)?;
    Ok(target)
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
            backend: None,
            pid: None,
            exit_code: None,
            prefix: None,
        })
    } else {
        println!("{message}");
        Ok(())
    }
}

fn launch_steam_client(
    runtime: &WineRuntime,
    prefix: &Path,
    executable: &str,
    arguments: &[String],
    json: bool,
    graphics: &crate::graphics::LaunchGraphics,
) -> Result<()> {
    let client_arguments = steam_client_arguments(arguments);
    write_steam_ui_session()?;
    emit_steam_state(
        json,
        "ui_compatibility",
        "Steam Web UI: OpenGL + system composer, CEF GPU and occlusion disabled",
    )?;
    for attempt in 0..STEAM_RESTART_LIMIT {
        let exit_code = match runtime.launch_windows(
            prefix,
            executable,
            &client_arguments,
            json,
            graphics,
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

fn steam_client_arguments(arguments: &[String]) -> Vec<String> {
    STEAM_UI_ARGUMENTS
        .iter()
        .map(|value| (*value).to_string())
        .chain(arguments.iter().cloned())
        .collect()
}

#[derive(Debug, Serialize, Deserialize)]
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

fn clear_steam_ui_session() -> Result<()> {
    let path = steam_ui_session_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn steam_ui_policy_current() -> Result<bool> {
    let path = steam_ui_session_path()?;
    let Ok(bytes) = fs::read(path) else {
        return Ok(false);
    };
    let Ok(session) = serde_json::from_slice::<SteamUiSession>(&bytes) else {
        return Ok(false);
    };
    Ok(session.ui_policy_version == STEAM_UI_POLICY_VERSION)
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
    fn steam_ui_compatibility_flags_precede_launch_arguments() {
        let arguments = vec!["-applaunch".to_string(), "292030".to_string()];
        assert_eq!(
            steam_client_arguments(&arguments),
            vec![
                "-cef-disable-gpu",
                "-cef-disable-gpu-compositing",
                "-cef-disable-occlusion",
                "-opengl",
                "-system-composer",
                "-applaunch",
                "292030",
            ]
        );
    }
}
