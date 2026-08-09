//! Fetching a published DarwinWine runtime artifact instead of building one.
//!
//! The runtime is a full Wine build tree and weighs several hundred megabytes
//! compressed, so it is published as a GitHub release asset rather than carried
//! in the repository.

use crate::app_dirs::application_support;
use crate::error::{AppError, Result};
use crate::events::write_progress;
use crate::wine::{install_darwinwine, DarwinWineStatus};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const RELEASE_API: &str = "https://api.github.com/repos/russianssamovar/DarwinWine/releases/latest";
const ARTIFACT_PREFIX: &str = "DarwinWine-";
const ARTIFACT_SUFFIX: &str = "-macos-x86_64.tar.zst";
/// GitHub caps a single release asset at 2 GiB; anything larger is not ours.
const MAX_ARTIFACT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

/// Downloads the newest published runtime and installs it.
pub fn install_latest_darwinwine(json: bool) -> Result<DarwinWineStatus> {
    emit(json, "Resolving", "Looking up the latest DarwinWine release", None)?;
    let release = fetch_latest_release()?;
    let asset = select_runtime_asset(&release)?;
    let expected = expected_digest(&release, asset)?;

    let downloads = application_support()?.join("downloads/darwinwine");
    fs::create_dir_all(&downloads)?;
    let target = downloads.join(&asset.name);
    let staging = downloads.join(format!(".{}-{}.part", asset.name, std::process::id()));
    let _ = fs::remove_file(&staging);

    emit(
        json,
        "Downloading",
        &format!("Downloading DarwinWine {}", release.tag_name),
        Some(0.0),
    )?;
    download(&asset.browser_download_url, &staging, asset.size, json)?;

    emit(json, "Verifying", "Verifying the downloaded artifact", None)?;
    if let Err(error) = verify_sha256(&staging, &expected) {
        let _ = fs::remove_file(&staging);
        return Err(error);
    }

    let _ = fs::remove_file(&target);
    fs::rename(&staging, &target)?;

    let status = install_darwinwine(&target, json)?;
    // The artifact stays on disk only as long as it takes to unpack it.
    let _ = fs::remove_file(&target);
    Ok(status)
}

fn fetch_latest_release() -> Result<GithubRelease> {
    let output = Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--max-time",
            "30",
            "--header",
            "Accept: application/vnd.github+json",
            "--user-agent",
            "DarwinPlay",
            RELEASE_API,
        ])
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(AppError::Release(
            "could not reach the DarwinWine release feed".into(),
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|_| AppError::Release("the DarwinWine release feed was not valid JSON".into()))
}

fn select_runtime_asset(release: &GithubRelease) -> Result<&GithubAsset> {
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name.starts_with(ARTIFACT_PREFIX) && asset.name.ends_with(ARTIFACT_SUFFIX))
        .ok_or_else(|| {
            AppError::Release(format!(
                "release {} has no {ARTIFACT_PREFIX}*{ARTIFACT_SUFFIX} asset",
                release.tag_name
            ))
        })?;
    if asset.size > MAX_ARTIFACT_BYTES {
        return Err(AppError::Release(format!(
            "release asset {} is implausibly large ({} bytes)",
            asset.name, asset.size
        )));
    }
    Ok(asset)
}

/// GitHub reports a digest for newer assets. Older ones only have the sidecar
/// `.sha256` file that `make package` publishes alongside the artifact.
fn expected_digest(release: &GithubRelease, asset: &GithubAsset) -> Result<String> {
    if let Some(digest) = asset.digest.as_deref() {
        let value = digest.strip_prefix("sha256:").unwrap_or(digest);
        if is_sha256_hex(value) {
            return Ok(value.to_ascii_lowercase());
        }
    }

    let sidecar_name = format!("{}.sha256", asset.name);
    let sidecar = release
        .assets
        .iter()
        .find(|candidate| candidate.name == sidecar_name)
        .ok_or_else(|| {
            AppError::Release(format!(
                "release {} publishes no checksum for {}",
                release.tag_name, asset.name
            ))
        })?;

    let output = Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--max-time",
            "30",
            "--user-agent",
            "DarwinPlay",
        ])
        .arg(&sidecar.browser_download_url)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(AppError::Release("could not download the checksum file".into()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let value = text
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !is_sha256_hex(&value) {
        return Err(AppError::Release("the published checksum is malformed".into()));
    }
    Ok(value)
}

fn download(url: &str, destination: &Path, total: u64, json: bool) -> Result<()> {
    if !is_allowed_download_url(url) {
        return Err(AppError::Release(
            "release asset URL is not an allowed GitHub HTTPS URL".into(),
        ));
    }
    let mut child = Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--tlsv1.2",
        ])
        .arg("--output")
        .arg(destination)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut last = u64::MAX;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        let current = fs::metadata(destination).map(|entry| entry.len()).unwrap_or(0);
        if current != last {
            last = current;
            let fraction = (total > 0).then(|| (current as f64 / total as f64).min(1.0));
            emit_bytes(json, current, total, fraction)?;
        }
        thread::sleep(Duration::from_millis(200));
    };

    if !status.success() {
        let _ = fs::remove_file(destination);
        return Err(AppError::Release("the runtime download failed".into()));
    }
    Ok(())
}

fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let output = Command::new("/usr/bin/shasum")
        .arg("-a")
        .arg("256")
        .arg(path)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(AppError::Release("could not hash the downloaded artifact".into()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let actual = text
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if actual != expected {
        return Err(AppError::Release(
            "the downloaded artifact does not match its published checksum".into(),
        ));
    }
    Ok(())
}

fn is_allowed_download_url(url: &str) -> bool {
    url.starts_with("https://github.com/")
        || url.starts_with("https://objects.githubusercontent.com/")
        || url.starts_with("https://release-assets.githubusercontent.com/")
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn emit(json: bool, phase: &str, message: &str, progress: Option<f64>) -> Result<()> {
    if json {
        write_progress("wine_runtime_progress", phase, message, progress, None, None, None)
    } else {
        println!("[{phase}] {message}");
        Ok(())
    }
}

fn emit_bytes(json: bool, current: u64, total: u64, progress: Option<f64>) -> Result<()> {
    if json {
        write_progress(
            "wine_runtime_progress",
            "Downloading",
            "Downloading DarwinWine runtime",
            progress,
            None,
            Some(current),
            (total > 0).then_some(total),
        )
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release_with(assets: Vec<GithubAsset>) -> GithubRelease {
        GithubRelease { tag_name: "cx26.3-dp6".into(), assets }
    }

    fn asset(name: &str, digest: Option<&str>) -> GithubAsset {
        GithubAsset {
            name: name.into(),
            browser_download_url: format!("https://github.com/o/r/releases/download/t/{name}"),
            size: 1024,
            digest: digest.map(str::to_string),
        }
    }

    #[test]
    fn picks_the_macos_artifact_and_ignores_its_sidecar() {
        let release = release_with(vec![
            asset("DarwinWine-cx26.3-dp6-macos-x86_64.tar.zst.sha256", None),
            asset("DarwinWine-cx26.3-dp6-macos-x86_64.tar.zst", None),
        ]);
        let selected = select_runtime_asset(&release).unwrap();
        assert_eq!(selected.name, "DarwinWine-cx26.3-dp6-macos-x86_64.tar.zst");
    }

    #[test]
    fn rejects_a_release_without_a_runtime_artifact() {
        let release = release_with(vec![asset("notes.txt", None)]);
        assert!(matches!(
            select_runtime_asset(&release).unwrap_err(),
            AppError::Release(_)
        ));
    }

    #[test]
    fn rejects_an_implausibly_large_asset() {
        let mut oversized = asset("DarwinWine-x-macos-x86_64.tar.zst", None);
        oversized.size = MAX_ARTIFACT_BYTES + 1;
        let release = release_with(vec![oversized]);
        assert!(select_runtime_asset(&release).is_err());
    }

    #[test]
    fn prefers_the_digest_github_reports() {
        let hex = "a".repeat(64);
        let release = release_with(vec![asset(
            "DarwinWine-cx26.3-dp6-macos-x86_64.tar.zst",
            Some(&format!("sha256:{hex}")),
        )]);
        let selected = select_runtime_asset(&release).unwrap();
        assert_eq!(expected_digest(&release, selected).unwrap(), hex);
    }

    #[test]
    fn refuses_to_download_from_outside_github() {
        assert!(!is_allowed_download_url("https://example.com/DarwinWine.tar.zst"));
        assert!(is_allowed_download_url(
            "https://objects.githubusercontent.com/darwinwine.tar.zst"
        ));
    }
}
