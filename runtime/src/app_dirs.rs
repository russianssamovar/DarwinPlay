use crate::error::{AppError, Result};
use std::env;
use std::path::PathBuf;

pub fn application_support() -> Result<PathBuf> {
    if let Some(path) = env::var_os("DARWINPLAY_HOME") {
        return Ok(PathBuf::from(path));
    }

    let home = env::var_os("HOME").ok_or(AppError::HomeNotSet)?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("DarwinPlay"))
}
