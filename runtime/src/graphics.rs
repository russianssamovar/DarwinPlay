use crate::app_dirs::application_support;
use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const DXMT_COMPONENT: &str = "dxmt";
const DXMT_MANIFEST: &str = "manifest.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphicsBackend {
    Auto,
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DxmtStatus {
    pub installed: bool,
    pub root: Option<String>,
    pub mode: Option<DxmtMode>,
    pub source_name: Option<String>,
    pub has_d3d10core: bool,
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
        })
    }

    pub fn install_dxmt(&self, source: &Path, mode: DxmtMode) -> Result<DxmtStatus> {
        let package = DxmtPackage::discover(source)?;
        let target = self.dxmt_root();
        let staging = self.root.join(format!(".{DXMT_COMPONENT}-staging-{}", std::process::id()));
        let backup = self.root.join(format!(".{DXMT_COMPONENT}-backup-{}", std::process::id()));

        remove_path(&staging)?;
        remove_path(&backup)?;
        fs::create_dir_all(staging.join("x86_64-unix"))?;
        fs::create_dir_all(staging.join("x86_64-windows"))?;

        copy_file(&package.winemetal_so, &staging.join("x86_64-unix/winemetal.so"))?;
        copy_file(&package.winemetal_dll, &staging.join("x86_64-windows/winemetal.dll"))?;
        copy_file(&package.d3d11, &staging.join("x86_64-windows/d3d11.dll"))?;
        copy_file(&package.dxgi, &staging.join("x86_64-windows/dxgi.dll"))?;
        if let Some(d3d10core) = package.d3d10core.as_ref() {
            copy_file(d3d10core, &staging.join("x86_64-windows/d3d10core.dll"))?;
        }

        let manifest = DxmtManifest {
            schema_version: 1,
            mode,
            source_name: source
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| source.display().to_string()),
            has_d3d10core: package.d3d10core.is_some(),
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
        install_managed_dll(
            prefix,
            "winemetal.dll",
            &root.join("x86_64-windows/winemetal.dll"),
        )?;

        let mut environment = BTreeMap::new();
        let dll_path = format!(
            "{}:{}",
            root.join("x86_64-windows").display(),
            root.join("x86_64-unix").display()
        );
        environment.insert("WINEDLLPATH".to_string(), dll_path);

        match manifest.mode {
            DxmtMode::Builtin => {
                restore_managed_dll(prefix, "d3d11.dll")?;
                restore_managed_dll(prefix, "dxgi.dll")?;
                restore_managed_dll(prefix, "d3d10core.dll")?;
            }
            DxmtMode::Native => {
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
