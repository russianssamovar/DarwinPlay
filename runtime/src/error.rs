use std::io;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("DarwinWine runtime is not installed. Install a DarwinWine runtime artifact first")]
    RuntimeNotFound,
    #[error("DarwinWine runtime error: {0}")]
    Runtime(String),
    #[error("invalid game id: {0}")]
    InvalidGameId(String),
    #[error("invalid PE file: {0}")]
    InvalidPe(String),
    #[error("DXMT is not installed")]
    DxmtNotInstalled,
    #[error("Wine prefix is incomplete or corrupted: {0}. Reset the affected prefix and try again")]
    CorruptPrefix(String),
    #[error("Wine prefix was created with {0}, but DarwinWine is now {1}. Reset the affected prefix after a runtime-incompatible update")]
    PrefixRuntimeMismatch(String, String),
    #[error("Steam is not installed in the DarwinPlay Wine prefix")]
    SteamNotInstalled,
    #[error("Steam installer completed but steam.exe was not found")]
    SteamInstallationMissing,
    #[error("Steam installer download failed")]
    SteamInstallerDownloadFailed,
    #[error("Steam app {0} is not installed")]
    SteamGameNotInstalled(u32),
    #[error("invalid Valve KeyValues data: {0}")]
    InvalidVdf(String),
    #[error("invalid compatibility profile: {0}")]
    InvalidCompatibilityProfile(String),
    #[error("DXMT package is missing required file: {0}")]
    DxmtPackageMissing(String),
    #[error("DXMT release error: {0}")]
    DxmtRelease(String),
    #[error("path is not a directory: {0}")]
    InvalidDirectory(String),
    #[error("process failed: {0}")]
    ProcessFailed(String),
    #[error("path is not a regular file: {0}")]
    InvalidFile(String),
    #[error("path has no parent directory: {0}")]
    MissingParent(String),
    #[error("path has no file name: {0}")]
    MissingFileName(String),
    #[error("HOME is not set")]
    HomeNotSet,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
