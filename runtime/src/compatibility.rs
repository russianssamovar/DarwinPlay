use crate::app_dirs::application_support;
use crate::error::{AppError, Result};
use crate::pe::{PeReport, inspect_pe};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::fs;
use std::path::{Path, PathBuf};

const PROFILE_SCHEMA_VERSION: u32 = 1;
const MAX_EXECUTABLES: usize = 512;
const MAX_SCAN_DEPTH: usize = 8;
const MAX_LAUNCH_ARGUMENTS: usize = 64;
const MAX_LAUNCH_ARGUMENT_LENGTH: usize = 1024;
const MAX_TOTAL_LAUNCH_ARGUMENT_LENGTH: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompatibilityLevel {
    Promising,
    Fallback,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutableKind {
    Game,
    Launcher,
    Tool,
    Redistributable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableCandidate {
    pub relative_path: String,
    pub architecture: String,
    pub subsystem: String,
    pub graphics_apis: Vec<String>,
    #[serde(skip_serializing)]
    pub imports: Vec<String>,
    pub score: i32,
    pub kind: ExecutableKind,
    pub compatibility: CompatibilityLevel,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamCompatibilityProfile {
    pub app_id: u32,
    pub name: String,
    pub selected_executable: Option<String>,
    pub launch_arguments: Vec<String>,
    pub recommended_executable: Option<String>,
    pub compatibility: CompatibilityLevel,
    pub reasons: Vec<String>,
    pub candidates: Vec<ExecutableCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProfile {
    schema_version: u32,
    app_id: u32,
    selected_executable: Option<String>,
    launch_arguments: Vec<String>,
}

impl StoredProfile {
    fn defaults(app_id: u32) -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            app_id,
            selected_executable: None,
            launch_arguments: Vec::new(),
        }
    }
}

pub struct CompatibilityManager {
    root: PathBuf,
}

impl CompatibilityManager {
    pub fn new() -> Result<Self> {
        Self::with_root(application_support()?.join("compatibility/steam"))
    }

    fn with_root(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn analyze(
        &self,
        app_id: u32,
        name: &str,
        install_path: &Path,
    ) -> Result<SteamCompatibilityProfile> {
        if !install_path.is_dir() {
            return Err(AppError::InvalidDirectory(install_path.display().to_string()));
        }

        let stored = self.load(app_id)?;
        let mut candidates = scan_candidates(install_path, name);
        candidates.sort_by_key(|candidate| {
            (
                Reverse(candidate.score),
                candidate.relative_path.to_ascii_lowercase(),
            )
        });

        let saved_selection = stored.selected_executable.as_deref();
        let saved_candidate = saved_selection
            .and_then(|path| candidates.iter().find(|candidate| candidate.relative_path == path));
        let selected = saved_candidate.or_else(|| candidates.first());
        let recommended = candidates.first();
        let compatibility = selected
            .map(|candidate| candidate.compatibility)
            .unwrap_or(CompatibilityLevel::Unknown);
        let mut reasons = selected
            .map(|candidate| candidate.reasons.clone())
            .unwrap_or_else(|| vec!["No Windows executables were detected in the installed game directory".into()]);

        if let Some(path) = saved_selection
            && saved_candidate.is_none()
        {
            reasons.push(format!("Saved analysis target is no longer installed: {path}"));
        }

        Ok(SteamCompatibilityProfile {
            app_id,
            name: name.to_string(),
            selected_executable: saved_candidate.map(|candidate| candidate.relative_path.clone()),
            launch_arguments: stored.launch_arguments,
            recommended_executable: recommended.map(|candidate| candidate.relative_path.clone()),
            compatibility,
            reasons,
            candidates,
        })
    }

    pub fn save(
        &self,
        app_id: u32,
        selected_executable: Option<&str>,
        launch_arguments: Vec<String>,
        allowed_executables: &[String],
    ) -> Result<()> {
        validate_launch_arguments(&launch_arguments)?;
        let selected_executable = match selected_executable {
            Some(value) => {
                validate_relative_executable(value)?;
                if !allowed_executables.iter().any(|candidate| candidate == value) {
                    return Err(AppError::InvalidCompatibilityProfile(format!(
                        "executable is not part of the current Steam installation: {value}"
                    )));
                }
                Some(value.to_string())
            }
            None => None,
        };
        let profile = StoredProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            app_id,
            selected_executable,
            launch_arguments,
        };
        atomic_write_json(&self.path(app_id), &profile)
    }

    pub fn reset(&self, app_id: u32) -> Result<()> {
        match fs::remove_file(self.path(app_id)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn launch_configuration(
        &self,
        profile: &SteamCompatibilityProfile,
    ) -> (Vec<String>, Vec<String>) {
        let selected = profile
            .selected_executable
            .as_deref()
            .and_then(|path| profile.candidates.iter().find(|candidate| candidate.relative_path == path))
            .or_else(|| profile.candidates.first());
        let imports = selected
            .map(|candidate| candidate.imports.clone())
            .unwrap_or_default();
        (imports, profile.launch_arguments.clone())
    }

    fn load(&self, app_id: u32) -> Result<StoredProfile> {
        let path = self.path(app_id);
        if !path.is_file() {
            return Ok(StoredProfile::defaults(app_id));
        }
        let profile: StoredProfile = serde_json::from_slice(&fs::read(path)?)?;
        if profile.schema_version != PROFILE_SCHEMA_VERSION || profile.app_id != app_id {
            return Err(AppError::InvalidCompatibilityProfile(format!(
                "invalid profile metadata for Steam app {app_id}"
            )));
        }
        validate_launch_arguments(&profile.launch_arguments)?;
        if let Some(executable) = profile.selected_executable.as_deref() {
            validate_relative_executable(executable)?;
        }
        Ok(profile)
    }

    fn path(&self, app_id: u32) -> PathBuf {
        self.root.join(format!("{app_id}.json"))
    }
}

fn scan_candidates(root: &Path, game_name: &str) -> Vec<ExecutableCandidate> {
    let mut files = Vec::new();
    collect_executables(root, root, 0, &mut files);
    files.truncate(MAX_EXECUTABLES);
    files
        .into_iter()
        .filter_map(|path| build_candidate(root, &path, game_name).ok())
        .collect()
}

fn collect_executables(root: &Path, directory: &Path, depth: usize, result: &mut Vec<PathBuf>) {
    if depth > MAX_SCAN_DEPTH || result.len() >= MAX_EXECUTABLES {
        return;
    }
    let Ok(read_dir) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = read_dir.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
    for entry in entries {
        if result.len() >= MAX_EXECUTABLES {
            return;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_executables(root, &path, depth + 1, result);
            continue;
        }
        if !file_type.is_file() || !is_executable_path(&path) {
            continue;
        }
        if path.strip_prefix(root).is_ok() {
            result.push(path);
        }
    }
}

fn build_candidate(
    root: &Path,
    path: &Path,
    game_name: &str,
) -> Result<ExecutableCandidate> {
    let report = inspect_pe(path)?;
    let relative = path
        .strip_prefix(root)
        .map_err(|_| AppError::InvalidFile(path.display().to_string()))?;
    let relative_path = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    validate_relative_executable(&relative_path)?;
    let kind = classify(&relative_path);
    let (compatibility, mut reasons) = assess(&report);
    let score = candidate_score(&relative_path, game_name, &report, kind);
    reasons.push(format!("Candidate score: {score}"));
    reasons.push(format!("Candidate type: {}", kind_name(kind)));

    Ok(ExecutableCandidate {
        relative_path,
        architecture: report.architecture,
        subsystem: report.subsystem,
        graphics_apis: report.graphics_apis,
        imports: report.imports,
        score,
        kind,
        compatibility,
        reasons,
    })
}

fn assess(report: &PeReport) -> (CompatibilityLevel, Vec<String>) {
    let imports = import_set(report);
    let has_d3d12 = imports.contains("d3d12.dll");
    let has_d3d11 = imports.contains("d3d11.dll");
    let has_d3d10 = imports.contains("d3d10.dll")
        || imports.contains("d3d10_1.dll")
        || imports.contains("d3d10core.dll");
    let has_d3d9 = imports.contains("d3d9.dll");
    let has_vulkan = imports.contains("vulkan-1.dll");
    let has_opengl = imports.contains("opengl32.dll");

    if has_d3d11 || has_d3d10 {
        let api = if has_d3d11 { "Direct3D 11" } else { "Direct3D 10" };
        // The runtime translates Direct3D 10/11 to Metal itself; there is no
        // component for the user to install and nothing to choose between.
        return (
            CompatibilityLevel::Promising,
            vec![
                format!("{api} imports detected"),
                "The DarwinWine runtime translates Direct3D 10/11 to Metal".into(),
            ],
        );
    }

    if has_d3d12 {
        return (
            CompatibilityLevel::Unsupported,
            vec![
                "Direct3D 12 imports detected".into(),
                "This DarwinPlay build has no Direct3D 12 translation backend".into(),
            ],
        );
    }

    if has_vulkan {
        return (
            CompatibilityLevel::Fallback,
            vec![
                "Vulkan imports detected".into(),
                "Vulkan runs through Wine's own Vulkan path".into(),
            ],
        );
    }

    if has_d3d9 {
        return (
            CompatibilityLevel::Fallback,
            vec![
                "Direct3D 9 imports detected".into(),
                "Direct3D 9 runs through Wine's own Direct3D path".into(),
            ],
        );
    }

    if has_opengl {
        return (
            CompatibilityLevel::Fallback,
            vec!["OpenGL imports detected".into()],
        );
    }

    (
        CompatibilityLevel::Unknown,
        vec!["No supported graphics API was detected from the PE import table".into()],
    )
}

fn candidate_score(
    relative_path: &str,
    game_name: &str,
    report: &PeReport,
    kind: ExecutableKind,
) -> i32 {
    let imports = import_set(report);
    let lower = relative_path.to_ascii_lowercase();
    let file_name = Path::new(relative_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let depth = relative_path.matches('/').count() as i32;
    let mut score = 0;

    score += match report.architecture.as_str() {
        "x86_64" => 35,
        "x86" => 5,
        _ => -80,
    };
    if report.subsystem == "windows-gui" {
        score += 15;
    }
    if imports.contains("d3d11.dll") {
        score += 260;
    } else if imports.contains("d3d10.dll")
        || imports.contains("d3d10_1.dll")
        || imports.contains("d3d10core.dll")
    {
        score += 220;
    } else if imports.contains("vulkan-1.dll") {
        score += 190;
    } else if imports.contains("d3d9.dll") {
        score += 160;
    } else if imports.contains("opengl32.dll") {
        score += 120;
    } else if imports.contains("d3d12.dll") {
        score += 90;
    }
    if imports.contains("steam_api64.dll") || imports.contains("steam_api.dll") {
        score += 35;
    }
    if lower.contains("dx11") || lower.contains("d3d11") {
        score += 60;
    }
    if lower.contains("dx10") || lower.contains("d3d10") {
        score += 35;
    }
    if lower.contains("dx9") || lower.contains("d3d9") {
        score += 20;
    }
    if lower.contains("dx12") || lower.contains("d3d12") {
        score += 15;
    }
    score += name_similarity_score(&file_name, game_name);
    score -= depth * 3;
    score += match kind {
        ExecutableKind::Game => 50,
        ExecutableKind::Launcher => -20,
        ExecutableKind::Tool => -140,
        ExecutableKind::Redistributable => -400,
    };
    score
}

fn classify(relative_path: &str) -> ExecutableKind {
    let lower = relative_path.to_ascii_lowercase();
    if contains_any(
        &lower,
        &[
            "_commonredist",
            "commonredist",
            "redist/",
            "redistributable",
            "vcredist",
            "directx/",
            "dotnet/",
            "prereq",
            "prerequisite",
        ],
    ) {
        return ExecutableKind::Redistributable;
    }
    let file_name = Path::new(relative_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if contains_any(
        &file_name,
        &[
            "crashreport",
            "crashpad",
            "unins",
            "uninstall",
            "benchmark",
            "editor",
            "server",
            "configtool",
            "configurator",
            "setup",
        ],
    ) {
        return ExecutableKind::Tool;
    }
    if contains_any(&file_name, &["launcher", "bootstrap", "start_protected_game"]) {
        return ExecutableKind::Launcher;
    }
    ExecutableKind::Game
}

fn import_set(report: &PeReport) -> std::collections::BTreeSet<&str> {
    report.imports.iter().map(String::as_str).collect()
}

fn name_similarity_score(file_name: &str, game_name: &str) -> i32 {
    let game_tokens = tokens(game_name);
    let file_tokens = tokens(file_name);
    let matches = game_tokens
        .iter()
        .filter(|token| file_tokens.iter().any(|candidate| candidate == *token))
        .count();
    i32::try_from(matches.min(4)).unwrap_or(0) * 12
}

fn tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|token| token.len() >= 3)
        .collect()
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn is_executable_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
}

pub(crate) fn validate_relative_executable(value: &str) -> Result<()> {
    if value.is_empty()
        || value.starts_with('/')
        || value.starts_with('\\')
        || value.contains(':')
        || value.contains('\\')
        || value.split('/').any(|component| component.is_empty() || component == "." || component == "..")
        || !value.to_ascii_lowercase().ends_with(".exe")
    {
        return Err(AppError::InvalidCompatibilityProfile(format!(
            "invalid relative executable path: {value}"
        )));
    }
    Ok(())
}

fn validate_launch_arguments(arguments: &[String]) -> Result<()> {
    if arguments.len() > MAX_LAUNCH_ARGUMENTS {
        return Err(AppError::InvalidCompatibilityProfile(format!(
            "at most {MAX_LAUNCH_ARGUMENTS} launch arguments are allowed"
        )));
    }
    let mut total = 0usize;
    for argument in arguments {
        if argument.contains('\0') || argument.len() > MAX_LAUNCH_ARGUMENT_LENGTH {
            return Err(AppError::InvalidCompatibilityProfile(
                "launch argument is invalid or too long".into(),
            ));
        }
        total = total.saturating_add(argument.len());
    }
    if total > MAX_TOTAL_LAUNCH_ARGUMENT_LENGTH {
        return Err(AppError::InvalidCompatibilityProfile(
            "launch arguments exceed the total size limit".into(),
        ));
    }
    Ok(())
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::MissingParent(path.display().to_string()))?;
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("profile"),
        std::process::id()
    ));
    let data = serde_json::to_vec_pretty(value)?;
    fs::write(&staging, data)?;
    match fs::rename(&staging, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&staging);
            Err(error.into())
        }
    }
}

fn kind_name(kind: ExecutableKind) -> &'static str {
    match kind {
        ExecutableKind::Game => "game",
        ExecutableKind::Launcher => "launcher",
        ExecutableKind::Tool => "tool",
        ExecutableKind::Redistributable => "redistributable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("darwinplay-{name}-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn persists_only_overrides() {
        let root = temp_root("profiles");
        let manager = CompatibilityManager::with_root(root.clone()).unwrap();
        manager
            .save(
                570,
                Some("bin/game.exe"),
                vec!["-dx11".into()],
                &["bin/game.exe".into()],
            )
            .unwrap();
        let stored = manager.load(570).unwrap();
        assert_eq!(stored.selected_executable.as_deref(), Some("bin/game.exe"));
        assert_eq!(stored.launch_arguments, vec!["-dx11"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_executable_outside_installation() {
        let root = temp_root("profiles-invalid");
        let manager = CompatibilityManager::with_root(root.clone()).unwrap();
        let error = manager
            .save(570, Some("other.exe"), vec![], &["game.exe".into()])
            .unwrap_err();
        assert!(matches!(error, AppError::InvalidCompatibilityProfile(_)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn classifies_redistributables() {
        assert_eq!(
            classify("_CommonRedist/vcredist/2022/vc_redist.x64.exe"),
            ExecutableKind::Redistributable
        );
    }

    #[test]
    fn d3d11_imports_are_promising_without_any_extra_component() {
        let report = PeReport {
            path: "game.exe".into(),
            architecture: "x86_64".into(),
            subsystem: "windows-gui".into(),
            entry_point: 0,
            image_base: 0,
            imports: vec!["d3d11.dll".into()],
            graphics_apis: vec!["Direct3D 11 / DXGI".into()],
        };
        let (level, reasons) = assess(&report);
        assert_eq!(level, CompatibilityLevel::Promising);
        assert!(reasons.iter().any(|reason| reason.contains("Direct3D 11")));
    }
}
