use crate::app_dirs::application_support;
use crate::error::{AppError, Result};
use crate::wine::WineRuntime;
use std::fs;
use std::path::{Path, PathBuf};

pub struct PrefixManager {
    root: PathBuf,
}

impl PrefixManager {
    pub fn new() -> Result<Self> {
        let root = application_support()?.join("prefixes");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn path(&self, game_id: &str) -> Result<PathBuf> {
        validate_game_id(game_id)?;
        Ok(self.root.join(game_id))
    }

    pub fn ensure(&self, runtime: &WineRuntime, game_id: &str) -> Result<PathBuf> {
        let path = self.path(game_id)?;
        let marker = path.join(".darwinplay-initialized");
        if marker.is_file() {
            if prefix_payload_ready(&path) {
                return Ok(path);
            }
            return Err(AppError::CorruptPrefix(path.display().to_string()));
        }

        if path.exists() {
            fs::remove_dir_all(&path)?;
        }

        let staging = self
            .root
            .join(format!(".{game_id}.creating-{}", std::process::id()));
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir_all(&staging)?;

        let result = (|| {
            runtime.initialize_prefix(&staging)?;
            if !prefix_payload_ready(&staging) {
                return Err(AppError::CorruptPrefix(staging.display().to_string()));
            }
            remove_mapping(&staging.join("dosdevices").join("z:"))?;
            fs::write(
                staging.join(".darwinplay-initialized"),
                runtime.version().as_bytes(),
            )?;
            Ok(())
        })();

        if let Err(error) = result {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }

        fs::rename(&staging, &path)?;
        Ok(path)
    }

    pub fn bind_game_drive(&self, prefix: &Path, executable: &Path) -> Result<()> {
        let parent = executable
            .parent()
            .ok_or_else(|| AppError::MissingParent(executable.display().to_string()))?;
        self.bind_drive(prefix, 'g', parent)
    }

    pub fn bind_drive(&self, prefix: &Path, drive: char, directory: &Path) -> Result<()> {
        let drive = validate_drive_letter(drive)?;
        if !directory.is_dir() {
            return Err(AppError::InvalidDirectory(directory.display().to_string()));
        }
        let mapping = prefix.join("dosdevices").join(format!("{drive}:"));
        replace_symlink(&mapping, directory)
    }

    pub fn unbind_drive(&self, prefix: &Path, drive: char) -> Result<()> {
        let drive = validate_drive_letter(drive)?;
        remove_mapping(&prefix.join("dosdevices").join(format!("{drive}:")))
    }

    pub fn reset(&self, game_id: &str) -> Result<()> {
        let path = self.path(game_id)?;
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}

fn prefix_payload_ready(path: &Path) -> bool {
    path.join("drive_c/windows/system32/kernel32.dll").is_file()
        && path.join("system.reg").is_file()
        && path.join("user.reg").is_file()
}

fn validate_game_id(value: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(AppError::InvalidGameId(value.to_string()))
    }
}

fn validate_drive_letter(value: char) -> Result<char> {
    let value = value.to_ascii_lowercase();
    if value.is_ascii_lowercase() && value != 'c' && value != 'z' {
        Ok(value)
    } else {
        Err(AppError::ProcessFailed(format!(
            "invalid Wine drive letter: {value}"
        )))
    }
}

fn remove_mapping(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            fs::remove_file(path)?;
        }
        Ok(metadata) if metadata.is_dir() => {
            fs::remove_dir_all(path)?;
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(unix)]
fn replace_symlink(link: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;
    remove_mapping(link)?;
    symlink(target, link)?;
    Ok(())
}

#[cfg(not(unix))]
fn replace_symlink(_link: &Path, _target: &Path) -> Result<()> {
    Err(AppError::ProcessFailed(
        "DarwinPlay prefix mappings require a Unix host".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_safe_game_ids() {
        assert!(validate_game_id("8F73CA87-55A1-4A11").is_ok());
        assert!(validate_game_id("game_01").is_ok());
        assert!(validate_game_id("../escape").is_err());
        assert!(validate_game_id("").is_err());
    }

    #[test]
    fn validates_drive_letters() {
        assert_eq!(validate_drive_letter('G').unwrap(), 'g');
        assert_eq!(validate_drive_letter('i').unwrap(), 'i');
        assert!(validate_drive_letter('c').is_err());
        assert!(validate_drive_letter('z').is_err());
        assert!(validate_drive_letter('1').is_err());
    }

    #[test]
    fn validates_prefix_payload() {
        let root = std::env::temp_dir().join(format!(
            "darwinplay-prefix-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("drive_c/windows/system32")).unwrap();
        assert!(!prefix_payload_ready(&root));
        fs::write(root.join("drive_c/windows/system32/kernel32.dll"), b"x").unwrap();
        fs::write(root.join("system.reg"), b"x").unwrap();
        fs::write(root.join("user.reg"), b"x").unwrap();
        assert!(prefix_payload_ready(&root));
        fs::remove_dir_all(root).unwrap();
    }
}
