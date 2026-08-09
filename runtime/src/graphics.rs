use crate::app_dirs::application_support;
use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DXMT_COMPONENT: &str = "dxmt";
const DXMT_MANIFEST: &str = "manifest.json";
const DXMT_RELEASE_API: &str = "https://api.github.com/repos/3Shain/dxmt/releases/latest";
const DXMT_MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphicsBackend {
    Auto,
    #[serde(rename = "wined3d")]
    WineD3d,
    Dxmt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DxmtMode {
    Builtin,
    Native,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DxmtManifest {
    pub schema_version: u32,
    pub mode: DxmtMode,
    pub source_name: String,
    pub has_d3d10core: bool,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DxmtStatus {
    pub installed: bool,
    pub root: Option<String>,
    pub mode: Option<DxmtMode>,
    pub source_name: Option<String>,
    pub has_d3d10core: bool,
    pub version: Option<String>,
    pub managed: bool,
}

#[derive(Debug, Clone)]
pub struct LaunchGraphics {
    pub backend: GraphicsBackend,
    pub environment: BTreeMap<String, String>,
}

impl LaunchGraphics {
    pub fn wined3d() -> Self {
        Self {
            backend: GraphicsBackend::WineD3d,
            environment: BTreeMap::new(),
        }
    }
}

pub struct GraphicsManager {
    root: PathBuf,
}

impl GraphicsManager {
    pub fn new() -> Result<Self> {
        let root = application_support()?.join("components");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn dxmt_status(&self) -> Result<DxmtStatus> {
        let root = self.dxmt_root();
        let manifest_path = root.join(DXMT_MANIFEST);
        if !manifest_path.is_file() {
            return Ok(DxmtStatus {
                installed: false,
                root: None,
                mode: None,
                source_name: None,
                has_d3d10core: false,
                version: None,
                managed: false,
            });
        }

        let manifest: DxmtManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
        validate_installed_dxmt(&root)?;

        Ok(DxmtStatus {
            installed: true,
            root: Some(root.display().to_string()),
            mode: Some(manifest.mode),
            source_name: Some(manifest.source_name),
            has_d3d10core: manifest.has_d3d10core,
            version: manifest.version,
            managed: manifest.managed,
        })
    }

    pub fn install_dxmt(&self, source: &Path, mode: DxmtMode) -> Result<DxmtStatus> {
        let source_name = source
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| source.display().to_string());
        self.install_dxmt_package(source, mode, source_name, None, false)
    }

    pub fn install_latest_dxmt(&self) -> Result<DxmtStatus> {
        let release = fetch_latest_dxmt_release()?;
        let asset = select_builtin_asset(&release)?;
        if asset.size == 0 || asset.size > DXMT_MAX_ASSET_BYTES {
            return Err(AppError::DxmtRelease(format!(
                "DXMT asset size is outside the allowed range: {} bytes",
                asset.size
            )));
        }

        let downloads = application_support()?.join("downloads/dxmt");
        fs::create_dir_all(&downloads)?;
        let archive = downloads.join(&asset.name);
        let staging_archive = downloads.join(format!(".{}.{}.tmp", asset.name, std::process::id()));
        remove_file_if_exists(&staging_archive)?;
        download_file(&asset.browser_download_url, &staging_archive)?;
        if fs::metadata(&staging_archive)?.len() != asset.size {
            let _ = fs::remove_file(&staging_archive);
            return Err(AppError::DxmtRelease("downloaded DXMT asset size does not match GitHub metadata".into()));
        }
        if let Some(digest) = asset.digest.as_deref() {
            verify_sha256(&staging_archive, digest)?;
        }
        remove_file_if_exists(&archive)?;
        fs::rename(&staging_archive, &archive)?;

        let extract_root = self.root.join(format!(".dxmt-extract-{}", std::process::id()));
        remove_path(&extract_root)?;
        fs::create_dir_all(&extract_root)?;
        let extract_result = extract_tar_gz(&archive, &extract_root);
        if let Err(error) = extract_result {
            let _ = remove_path(&extract_root);
            return Err(error);
        }
        let package_root = find_dxmt_package_root(&extract_root, 4)?;
        let result = self.install_dxmt_package(
            &package_root,
            DxmtMode::Builtin,
            asset.name.clone(),
            Some(release.tag_name),
            true,
        );
        let _ = remove_path(&extract_root);
        result
    }

    fn install_dxmt_package(
        &self,
        source: &Path,
        mode: DxmtMode,
        source_name: String,
        version: Option<String>,
        managed: bool,
    ) -> Result<DxmtStatus> {
        let package = DxmtPackage::discover(source)?;
        let target = self.dxmt_root();
        let staging = self.root.join(format!(".{DXMT_COMPONENT}-staging-{}", std::process::id()));
        let backup = self.root.join(format!(".{DXMT_COMPONENT}-backup-{}", std::process::id()));

        remove_path(&staging)?;
        remove_path(&backup)?;
        copy_directory(&source.join("x86_64-unix"), &staging.join("x86_64-unix"))?;
        copy_directory(
            &source.join("x86_64-windows"),
            &staging.join("x86_64-windows"),
        )?;
        let i386_windows = source.join("i386-windows");
        if i386_windows.is_dir() {
            copy_directory(&i386_windows, &staging.join("i386-windows"))?;
        }

        let manifest = DxmtManifest {
            schema_version: 2,
            mode,
            source_name,
            has_d3d10core: package.d3d10core.is_some(),
            version,
            managed,
        };
        fs::write(
            staging.join(DXMT_MANIFEST),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        validate_installed_dxmt(&staging)?;

        if target.exists() {
            fs::rename(&target, &backup)?;
        }
        if let Err(error) = fs::rename(&staging, &target) {
            if backup.exists() {
                let _ = fs::rename(&backup, &target);
            }
            return Err(error.into());
        }
        remove_path(&backup)?;
        self.dxmt_status()
    }

    pub fn remove_dxmt(&self) -> Result<()> {
        remove_path(&self.dxmt_root())
    }

    pub fn prepare_launch(
        &self,
        requested: GraphicsBackend,
        imports: &[String],
        prefix: &Path,
        game_id: &str,
    ) -> Result<LaunchGraphics> {
        let backend = self.resolve_backend(requested, imports)?;
        match backend {
            GraphicsBackend::Dxmt => self.prepare_dxmt(prefix, game_id),
            GraphicsBackend::WineD3d | GraphicsBackend::Auto => {
                restore_managed_dlls(prefix)?;
                Ok(LaunchGraphics {
                    backend: GraphicsBackend::WineD3d,
                    environment: BTreeMap::new(),
                })
            }
        }
    }

    pub fn prepare_steam_ui(
        &self,
        _requested: GraphicsBackend,
        prefix: &Path,
    ) -> Result<LaunchGraphics> {
        // Steam's embedded CEF is intentionally isolated from game renderers.
        // Keep the Steam UI on WineD3D/system composition; DXMT remains a game backend.
        restore_managed_dlls(prefix)?;
        Ok(LaunchGraphics::wined3d())
    }

    pub fn resolve_steam_ui_backend(&self, _requested: GraphicsBackend) -> Result<GraphicsBackend> {
        Ok(GraphicsBackend::WineD3d)
    }

    fn resolve_backend(
        &self,
        requested: GraphicsBackend,
        imports: &[String],
    ) -> Result<GraphicsBackend> {
        if requested != GraphicsBackend::Auto {
            if requested == GraphicsBackend::Dxmt && !self.dxmt_status()?.installed {
                return Err(AppError::DxmtNotInstalled);
            }
            return Ok(requested);
        }

        if is_dxmt_candidate(imports) && self.dxmt_status()?.installed {
            Ok(GraphicsBackend::Dxmt)
        } else {
            Ok(GraphicsBackend::WineD3d)
        }
    }

    fn prepare_dxmt(&self, prefix: &Path, game_id: &str) -> Result<LaunchGraphics> {
        let root = self.dxmt_root();
        let manifest_path = root.join(DXMT_MANIFEST);
        if !manifest_path.is_file() {
            return Err(AppError::DxmtNotInstalled);
        }
        let manifest: DxmtManifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
        validate_installed_dxmt(&root)?;

        let system32 = prefix.join("drive_c/windows/system32");
        fs::create_dir_all(&system32)?;
        backup_managed_dlls(prefix)?;

        let mut environment = BTreeMap::new();

        match manifest.mode {
            DxmtMode::Builtin => {
                restore_managed_dlls(prefix)?;
                environment.insert(
                    "WINEDLLPATH_PREPEND".to_string(),
                    root.display().to_string(),
                );
            }
            DxmtMode::Native => {
                install_managed_dll(
                    prefix,
                    "winemetal.dll",
                    &root.join("x86_64-windows/winemetal.dll"),
                )?;
                install_managed_dll(prefix, "d3d11.dll", &root.join("x86_64-windows/d3d11.dll"))?;
                install_managed_dll(prefix, "dxgi.dll", &root.join("x86_64-windows/dxgi.dll"))?;
                if manifest.has_d3d10core {
                    install_managed_dll(
                        prefix,
                        "d3d10core.dll",
                        &root.join("x86_64-windows/d3d10core.dll"),
                    )?;
                } else {
                    restore_managed_dll(prefix, "d3d10core.dll")?;
                }
                environment.insert(
                    "WINEDLLOVERRIDES".to_string(),
                    "dxgi,d3d11,d3d10core=n,b".to_string(),
                );
            }
        }

        let runtime_root = application_support()?;
        let log_path = runtime_root.join("logs").join(game_id).join("dxmt");
        let shader_cache = runtime_root.join("shader-cache").join(game_id).join("dxmt");
        if game_id == "steam-ui" {
            remove_path(&log_path)?;
        }
        fs::create_dir_all(&log_path)?;
        fs::create_dir_all(&shader_cache)?;
        environment.insert("DXMT_LOG_LEVEL".to_string(), "info".to_string());
        environment.insert("DXMT_LOG_PATH".to_string(), log_path.display().to_string());
        environment.insert(
            "DXMT_SHADER_CACHE_PATH".to_string(),
            shader_cache.display().to_string(),
        );

        Ok(LaunchGraphics {
            backend: GraphicsBackend::Dxmt,
            environment,
        })
    }

    fn dxmt_root(&self) -> PathBuf {
        self.root.join(DXMT_COMPONENT)
    }
}

struct DxmtPackage {
    winemetal_so: PathBuf,
    winemetal_dll: PathBuf,
    d3d11: PathBuf,
    dxgi: PathBuf,
    d3d10core: Option<PathBuf>,
}

impl DxmtPackage {
    fn discover(source: &Path) -> Result<Self> {
        if !source.is_dir() {
            return Err(AppError::InvalidDirectory(source.display().to_string()));
        }

        let unix = source.join("x86_64-unix");
        let windows = source.join("x86_64-windows");
        let package = Self {
            winemetal_so: unix.join("winemetal.so"),
            winemetal_dll: windows.join("winemetal.dll"),
            d3d11: windows.join("d3d11.dll"),
            dxgi: windows.join("dxgi.dll"),
            d3d10core: optional_file(windows.join("d3d10core.dll")),
        };
        package.validate()?;
        Ok(package)
    }

    fn validate(&self) -> Result<()> {
        for path in [&self.winemetal_so, &self.winemetal_dll, &self.d3d11, &self.dxgi] {
            if !path.is_file() {
                return Err(AppError::DxmtPackageMissing(path.display().to_string()));
            }
        }
        Ok(())
    }
}

fn validate_installed_dxmt(root: &Path) -> Result<()> {
    let package = DxmtPackage::discover(root)?;
    package.validate()
}


fn is_dxmt_candidate(imports: &[String]) -> bool {
    imports.iter().any(|name| {
        matches!(
            name.as_str(),
            "d3d11.dll" | "d3d10.dll" | "d3d10_1.dll" | "d3d10core.dll"
        )
    })
}

const MANAGED_DXMT_DLLS: [&str; 4] = ["winemetal.dll", "d3d11.dll", "dxgi.dll", "d3d10core.dll"];

fn backup_managed_dlls(prefix: &Path) -> Result<()> {
    for name in MANAGED_DXMT_DLLS {
        backup_managed_dll(prefix, name)?;
    }
    Ok(())
}

fn backup_managed_dll(prefix: &Path, name: &str) -> Result<()> {
    let system32 = prefix.join("drive_c/windows/system32");
    let backup = prefix.join(".darwinplay/graphics-backup/system32");
    let backup_file = backup.join(name);
    let absent_marker = backup.join(format!("{name}.absent"));
    if backup_file.exists() || absent_marker.exists() {
        return Ok(());
    }
    fs::create_dir_all(&backup)?;
    let source = system32.join(name);
    if source.is_file() {
        copy_file(&source, &backup_file)?;
    } else {
        fs::write(absent_marker, b"")?;
    }
    Ok(())
}

fn install_managed_dll(prefix: &Path, name: &str, source: &Path) -> Result<()> {
    backup_managed_dll(prefix, name)?;
    let target = prefix.join("drive_c/windows/system32").join(name);
    copy_file(source, &target)
}

fn restore_managed_dlls(prefix: &Path) -> Result<()> {
    for name in MANAGED_DXMT_DLLS {
        restore_managed_dll(prefix, name)?;
    }
    Ok(())
}

fn restore_managed_dll(prefix: &Path, name: &str) -> Result<()> {
    let backup = prefix.join(".darwinplay/graphics-backup/system32");
    let backup_file = backup.join(name);
    let absent_marker = backup.join(format!("{name}.absent"));
    let target = prefix.join("drive_c/windows/system32").join(name);
    if backup_file.is_file() {
        copy_file(&backup_file, &target)?;
    } else if absent_marker.is_file() {
        remove_file_if_exists(&target)?;
    }
    Ok(())
}

fn copy_directory(source: &Path, target: &Path) -> Result<()> {
    if !source.is_dir() {
        return Err(AppError::InvalidDirectory(source.display().to_string()));
    }
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_directory(&source_path, &target_path)?;
        } else if source_path.is_file() {
            copy_file(&source_path, &target_path)?;
        }
    }
    Ok(())
}

fn copy_file(source: &Path, target: &Path) -> Result<()> {
    if !source.is_file() {
        return Err(AppError::InvalidFile(source.display().to_string()));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, target)?;
    Ok(())
}

fn optional_file(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then_some(path)
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_path(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path).map_err(Into::into),
        Ok(_) => fs::remove_file(path).map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}


fn select_builtin_asset(release: &GithubRelease) -> Result<&GithubAsset> {
    release
        .assets
        .iter()
        .filter(|asset| {
            let name = asset.name.to_ascii_lowercase();
            name.contains("builtin") && name.ends_with(".tar.gz")
        })
        .min_by_key(|asset| asset.name.len())
        .ok_or_else(|| AppError::DxmtRelease("latest DXMT release has no builtin tar.gz asset".into()))
}

fn fetch_latest_dxmt_release() -> Result<GithubRelease> {
    let output = Command::new("/usr/bin/curl")
        .arg("--fail")
        .arg("--location")
        .arg("--silent")
        .arg("--show-error")
        .arg("--proto")
        .arg("=https")
        .arg("--tlsv1.2")
        .arg("--header")
        .arg("Accept: application/vnd.github+json")
        .arg("--header")
        .arg("User-Agent: DarwinPlay/0.8")
        .arg(DXMT_RELEASE_API)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(AppError::DxmtRelease(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }
    serde_json::from_slice(&output.stdout).map_err(Into::into)
}

fn download_file(url: &str, destination: &Path) -> Result<()> {
    if !url.starts_with("https://github.com/") && !url.starts_with("https://objects.githubusercontent.com/") {
        return Err(AppError::DxmtRelease("DXMT asset URL is not an allowed GitHub HTTPS URL".into()));
    }
    let status = Command::new("/usr/bin/curl")
        .arg("--fail")
        .arg("--location")
        .arg("--silent")
        .arg("--show-error")
        .arg("--proto")
        .arg("=https")
        .arg("--tlsv1.2")
        .arg("--output")
        .arg(destination)
        .arg(url)
        .stdin(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(AppError::DxmtRelease("DXMT download failed".into()));
    }
    Ok(())
}

fn verify_sha256(path: &Path, digest: &str) -> Result<()> {
    let expected = digest
        .strip_prefix("sha256:")
        .ok_or_else(|| AppError::DxmtRelease(format!("unsupported DXMT digest: {digest}")))?;
    if expected.len() != 64 || !expected.bytes().all(|value| value.is_ascii_hexdigit()) {
        return Err(AppError::DxmtRelease("invalid DXMT sha256 digest from GitHub".into()));
    }
    let output = Command::new("/usr/bin/shasum")
        .arg("-a")
        .arg("256")
        .arg(path)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(AppError::DxmtRelease("failed to calculate DXMT sha256".into()));
    }
    let actual = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if actual != expected.to_ascii_lowercase() {
        return Err(AppError::DxmtRelease("DXMT sha256 does not match GitHub metadata".into()));
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, destination: &Path) -> Result<()> {
    validate_tar_entries(archive)?;
    let status = Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(destination)
        .stdin(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(AppError::DxmtRelease("failed to extract DXMT archive".into()));
    }
    Ok(())
}

fn validate_tar_entries(archive: &Path) -> Result<()> {
    let output = Command::new("/usr/bin/tar")
        .arg("-tzf")
        .arg(archive)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(AppError::DxmtRelease("DXMT archive could not be inspected".into()));
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let path = Path::new(line);
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(AppError::DxmtRelease("DXMT archive contains an unsafe path".into()));
        }
    }
    Ok(())
}

fn find_dxmt_package_root(root: &Path, max_depth: usize) -> Result<PathBuf> {
    fn visit(path: &Path, depth: usize, max_depth: usize) -> Option<PathBuf> {
        if path.join("x86_64-unix/winemetal.so").is_file()
            && path.join("x86_64-windows/winemetal.dll").is_file()
            && path.join("x86_64-windows/d3d11.dll").is_file()
            && path.join("x86_64-windows/dxgi.dll").is_file()
        {
            return Some(path.to_path_buf());
        }
        if depth >= max_depth {
            return None;
        }
        let mut directories: Vec<_> = fs::read_dir(path).ok()?.flatten()
            .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
            .collect();
        directories.sort_by_key(|entry| entry.file_name());
        for entry in directories {
            if let Some(found) = visit(&entry.path(), depth + 1, max_depth) {
                return Some(found);
            }
        }
        None
    }
    visit(root, 0, max_depth).ok_or_else(|| AppError::DxmtRelease("extracted DXMT archive has an unexpected layout".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("darwinplay-{name}-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn validates_dxmt_layout() {
        let root = temp_dir("dxmt-layout");
        fs::create_dir_all(root.join("x86_64-unix")).unwrap();
        fs::create_dir_all(root.join("x86_64-windows")).unwrap();
        for relative in [
            "x86_64-unix/winemetal.so",
            "x86_64-windows/winemetal.dll",
            "x86_64-windows/d3d11.dll",
            "x86_64-windows/dxgi.dll",
        ] {
            fs::write(root.join(relative), b"fixture").unwrap();
        }

        assert!(DxmtPackage::discover(&root).is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_missing_dxmt_files() {
        let root = temp_dir("dxmt-missing");
        assert!(DxmtPackage::discover(&root).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dxgi_only_is_not_a_dxmt_candidate() {
        assert!(!is_dxmt_candidate(&["dxgi.dll".to_string(), "d3d12.dll".to_string()]));
        assert!(is_dxmt_candidate(&["dxgi.dll".to_string(), "d3d11.dll".to_string()]));
    }

    #[test]
    fn restores_original_prefix_dll() {
        let root = temp_dir("dll-restore");
        let system32 = root.join("drive_c/windows/system32");
        fs::create_dir_all(&system32).unwrap();
        fs::write(system32.join("d3d11.dll"), b"wine").unwrap();
        let replacement = root.join("replacement.dll");
        fs::write(&replacement, b"dxmt").unwrap();

        install_managed_dll(&root, "d3d11.dll", &replacement).unwrap();
        assert_eq!(fs::read(system32.join("d3d11.dll")).unwrap(), b"dxmt".to_vec());
        restore_managed_dll(&root, "d3d11.dll").unwrap();
        assert_eq!(fs::read(system32.join("d3d11.dll")).unwrap(), b"wine".to_vec());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn selects_builtin_release_asset() {
        let release = GithubRelease {
            tag_name: "v0.80".to_string(),
            assets: vec![
                GithubAsset {
                    name: "dxmt-v0.80-native.tar.gz".to_string(),
                    browser_download_url: "https://github.com/example/native".to_string(),
                    size: 1,
                    digest: None,
                },
                GithubAsset {
                    name: "dxmt-v0.80-builtin.tar.gz".to_string(),
                    browser_download_url: "https://github.com/example/builtin".to_string(),
                    size: 1,
                    digest: Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
                },
            ],
        };

        assert_eq!(select_builtin_asset(&release).unwrap().name, "dxmt-v0.80-builtin.tar.gz");
    }

    #[test]
    fn restores_original_absence() {
        let root = temp_dir("dll-absent");
        let replacement = root.join("replacement.dll");
        fs::write(&replacement, b"dxmt").unwrap();

        install_managed_dll(&root, "winemetal.dll", &replacement).unwrap();
        assert!(root.join("drive_c/windows/system32/winemetal.dll").is_file());
        restore_managed_dll(&root, "winemetal.dll").unwrap();
        assert!(!root.join("drive_c/windows/system32/winemetal.dll").exists());

        let _ = fs::remove_dir_all(root);
    }
}
