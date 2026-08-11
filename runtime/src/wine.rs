use crate::app_dirs::application_support;
use crate::error::{AppError, Result};
use crate::events::{write_json, write_progress, RuntimeEvent};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const DARWINWINE_DIRECTORY: &str = "darwinwine";
const DARWINWINE_MANIFEST: &str = "runtime.json";
const INSTALL_PROBE_TIMEOUT: Duration = Duration::from_secs(90);
const EXTRACTION_STALL_TIMEOUT: Duration = Duration::from_secs(300);
const EXTRACTION_TIMEOUT: Duration = Duration::from_secs(1800);
const DARWINWINE_MANIFEST_SCHEMA: u32 = 2;
const MIN_DARWINWINE_CX_MAJOR: u32 = 26;
const MIN_DARWINWINE_CX_MINOR: u32 = 3;
/// dp9 is the first runtime whose kernelbase injects --in-process-gpu into
/// Steam CEF processes; older runtimes need the retired webhelper shim that
/// this DarwinPlay no longer installs, so they would render Steam black.
const MIN_DARWINWINE_DP_REVISION: u32 = 9;

#[derive(Clone)]
pub struct WineRuntime {
    wine: PathBuf,
    wineserver: PathBuf,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DarwinWineManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub wine_version: String,
    pub darwin_wine_version: String,
    pub architecture: String,
    #[serde(rename = "minimumMacOS", alias = "minimumMacOs")]
    pub minimum_mac_os: String,
    pub channel: String,
    pub entrypoint: String,
    pub wineserver: String,
    pub steam_validated: bool,
    pub steam_login_validated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DarwinWineStatus {
    pub installed: bool,
    pub ready: bool,
    pub runtime_id: Option<String>,
    pub runtime_name: Option<String>,
    pub wine_path: Option<String>,
    pub wine_version: Option<String>,
    pub darwin_wine_version: Option<String>,
    pub architecture: Option<String>,
    pub channel: Option<String>,
    pub steam_validated: bool,
    pub steam_login_validated: bool,
    pub probe_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub wine_path: String,
    pub wine_version: String,
    pub host_architecture: String,
    pub wine_architecture: String,
}

pub fn runtime_status() -> DarwinWineStatus {
    let root = match darwinwine_root() {
        Ok(root) => root,
        Err(error) => return missing_status(Some(error.to_string())),
    };
    if !root.is_dir() {
        return missing_status(None);
    }
    let manifest = match load_manifest(&root) {
        Ok(manifest) => manifest,
        Err(error) => {
            return DarwinWineStatus {
                installed: true,
                ready: false,
                runtime_id: None,
                runtime_name: Some("DarwinWine".into()),
                wine_path: None,
                wine_version: None,
                darwin_wine_version: None,
                architecture: None,
                channel: None,
                steam_validated: false,
                steam_login_validated: false,
                probe_error: Some(error.to_string()),
            };
        }
    };
    let wine = root.join(&manifest.entrypoint);
    match WineRuntime::discover() {
        Ok(runtime) => DarwinWineStatus {
            installed: true,
            ready: true,
            runtime_id: Some(manifest.id),
            runtime_name: Some(manifest.name),
            wine_path: Some(wine.display().to_string()),
            wine_version: Some(runtime.version.clone()),
            darwin_wine_version: Some(manifest.darwin_wine_version),
            architecture: Some(manifest.architecture),
            channel: Some(manifest.channel),
            steam_validated: manifest.steam_validated,
            steam_login_validated: manifest.steam_login_validated,
            probe_error: None,
        },
        Err(error) => DarwinWineStatus {
            installed: true,
            ready: false,
            runtime_id: Some(manifest.id),
            runtime_name: Some(manifest.name),
            wine_path: Some(wine.display().to_string()),
            wine_version: None,
            darwin_wine_version: Some(manifest.darwin_wine_version),
            architecture: Some(manifest.architecture),
            channel: Some(manifest.channel),
            steam_validated: manifest.steam_validated,
            steam_login_validated: manifest.steam_login_validated,
            probe_error: Some(error.to_string()),
        },
    }
}

pub fn install_darwinwine(archive: &Path, json: bool) -> Result<DarwinWineStatus> {
    if !archive.is_file() {
        return Err(AppError::InvalidFile(archive.display().to_string()));
    }
    emit_runtime_progress(json, "Preparing", "Preparing DarwinWine runtime", None, Some(0.03))?;
    let archive_entries = validate_archive_paths(archive)?;

    let support = application_support()?;
    let runtimes = support.join("runtime");
    fs::create_dir_all(&runtimes)?;
    sweep_stale_install_dirs(&runtimes);
    let staging = runtimes.join(format!(".darwinwine-install-{}", std::process::id()));
    remove_path_if_exists(&staging)?;
    fs::create_dir_all(&staging)?;

    emit_runtime_progress(json, "Extracting", "Extracting DarwinWine", None, Some(0.18))?;
    extract_archive(archive, &staging, json, &archive_entries)?;
    let extracted_root = find_runtime_root(&staging, 3).ok_or_else(|| {
        AppError::Runtime("DarwinWine archive does not contain runtime.json".into())
    })?;
    let manifest = load_manifest(&extracted_root)?;
    validate_manifest(&manifest)?;

    emit_runtime_progress(json, "Validating", "Validating DarwinWine runtime", None, Some(0.42))?;
    let runtime = WineRuntime::from_root(&extracted_root, &manifest)?;
    if !runtime.version.contains(&manifest.wine_version) {
        return Err(AppError::Runtime(format!(
            "runtime reports {}, manifest expects Wine {}",
            runtime.version, manifest.wine_version
        )));
    }

    let probe_prefix = staging.join(".probe-prefix");
    emit_runtime_progress(json, "Testing", "Creating a temporary Wine prefix", None, Some(0.62))?;
    probe_runtime(&runtime, &probe_prefix)?;
    remove_path_if_exists(&probe_prefix)?;

    let target = darwinwine_root()?;
    let backup = runtimes.join(".darwinwine-backup");
    remove_path_if_exists(&backup)?;
    if target.exists() {
        fs::rename(&target, &backup)?;
    }
    emit_runtime_progress(json, "Activating", "Activating DarwinWine", None, Some(0.88))?;
    if let Err(error) = fs::rename(&extracted_root, &target) {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(error.into());
    }
    // The runtime is active from here on; cleanup problems must not be
    // reported as an installation failure. Anything left behind is swept by
    // the next install.
    let _ = remove_path_if_exists(&backup);
    let _ = remove_path_if_exists(&staging);

    let status = runtime_status();
    if !status.ready {
        return Err(AppError::Runtime(
            status.probe_error.clone().unwrap_or_else(|| "DarwinWine activation failed".into()),
        ));
    }
    remove_legacy_managed_wine_state(&support)?;
    emit_runtime_progress(json, "Ready", "DarwinWine is ready", Some(1.0), Some(1.0))?;
    Ok(status)
}

pub fn remove_darwinwine() -> Result<()> {
    remove_path_if_exists(&darwinwine_root()?)
}

fn missing_status(error: Option<String>) -> DarwinWineStatus {
    DarwinWineStatus {
        installed: false,
        ready: false,
        runtime_id: None,
        runtime_name: Some("DarwinWine".into()),
        wine_path: None,
        wine_version: None,
        darwin_wine_version: None,
        architecture: None,
        channel: None,
        steam_validated: false,
        steam_login_validated: false,
        probe_error: error,
    }
}

fn darwinwine_root() -> Result<PathBuf> {
    Ok(application_support()?.join("runtime").join(DARWINWINE_DIRECTORY))
}

fn load_manifest(root: &Path) -> Result<DarwinWineManifest> {
    let path = root.join(DARWINWINE_MANIFEST);
    let data = fs::read(&path).map_err(|_| {
        AppError::Runtime(format!("DarwinWine manifest not found: {}", path.display()))
    })?;
    Ok(serde_json::from_slice(&data)?)
}

fn validate_manifest(manifest: &DarwinWineManifest) -> Result<()> {
    if manifest.schema_version != DARWINWINE_MANIFEST_SCHEMA {
        return Err(AppError::Runtime(format!(
            "unsupported DarwinWine manifest schema {}; DarwinPlay requires schema {}",
            manifest.schema_version, DARWINWINE_MANIFEST_SCHEMA
        )));
    }
    if manifest.name != "DarwinWine" || !manifest.id.starts_with("darwinwine-") {
        return Err(AppError::Runtime("archive is not a DarwinWine runtime".into()));
    }
    if manifest.architecture != "x86_64" {
        return Err(AppError::Runtime(format!("unsupported DarwinWine architecture {}", manifest.architecture)));
    }
    validate_supported_darwinwine_version(&manifest.darwin_wine_version)?;
    validate_relative_runtime_path(&manifest.entrypoint)?;
    validate_relative_runtime_path(&manifest.wineserver)?;
    Ok(())
}

fn validate_supported_darwinwine_version(value: &str) -> Result<()> {
    let value_without_prefix = value
        .strip_prefix("cx")
        .ok_or_else(|| AppError::Runtime(format!(
            "unsupported DarwinWine version {value}; DarwinPlay requires CrossOver-derived cx26.3-dp9 or newer"
        )))?;
    let (crossover, revision) = value_without_prefix
        .split_once("-dp")
        .ok_or_else(|| AppError::Runtime(format!("invalid DarwinWine version {value}")))?;
    let (major, minor) = crossover
        .split_once('.')
        .ok_or_else(|| AppError::Runtime(format!("invalid DarwinWine version {value}")))?;
    let major = major
        .parse::<u32>()
        .map_err(|_| AppError::Runtime(format!("invalid DarwinWine version {value}")))?;
    let minor = minor
        .parse::<u32>()
        .map_err(|_| AppError::Runtime(format!("invalid DarwinWine version {value}")))?;
    let revision = revision
        .parse::<u32>()
        .map_err(|_| AppError::Runtime(format!("invalid DarwinWine version {value}")))?;

    let crossover_version = (major, minor);
    let minimum_crossover = (MIN_DARWINWINE_CX_MAJOR, MIN_DARWINWINE_CX_MINOR);
    if crossover_version < minimum_crossover
        || (crossover_version == minimum_crossover && revision < MIN_DARWINWINE_DP_REVISION)
    {
        return Err(AppError::Runtime(format!(
            "DarwinWine {value} is too old; DarwinPlay requires cx26.3-dp9 or newer"
        )));
    }
    Ok(())
}

fn validate_relative_runtime_path(value: &str) -> Result<()> {
    let path = Path::new(value);
    if path.is_absolute() || path.components().any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
        return Err(AppError::Runtime(format!("invalid runtime path in manifest: {value}")));
    }
    Ok(())
}

fn find_runtime_root(root: &Path, depth: usize) -> Option<PathBuf> {
    if root.join(DARWINWINE_MANIFEST).is_file() {
        return Some(root.to_path_buf());
    }
    if depth == 0 { return None; }
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_runtime_root(&path, depth - 1) { return Some(found); }
        }
    }
    None
}

/// Opens the archive for reading, decompressing zstd in-process. The system
/// bsdtar handles `.tar.zst` by spawning an external `zstd` from PATH, which
/// does not exist in a GUI app's environment (and may not be installed at
/// all), so tar must only ever see a plain tar stream on stdin.
fn open_archive_stream(archive: &Path) -> Result<Box<dyn std::io::Read + Send>> {
    let file = fs::File::open(archive)?;
    if archive.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("zst")) {
        let decoder = zstd::stream::read::Decoder::new(file)
            .map_err(|error| AppError::Runtime(format!("failed to open zstd archive: {error}")))?;
        Ok(Box::new(decoder))
    } else {
        Ok(Box::new(file))
    }
}

/// Spawns a thread feeding the (decompressed) archive into a child's stdin.
/// A broken pipe is expected when tar stops reading early; real read errors
/// surface through tar's own failure.
fn feed_archive(
    mut reader: Box<dyn std::io::Read + Send>,
    mut stdin: std::process::ChildStdin,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let _ = std::io::copy(&mut reader, &mut stdin);
    })
}

fn validate_archive_paths(archive: &Path) -> Result<HashSet<String>> {
    let mut child = Command::new("/usr/bin/tar")
        .arg("-tf").arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdin = child.stdin.take()
        .ok_or_else(|| AppError::Runtime("failed to open tar stdin".into()))?;
    let feeder = feed_archive(open_archive_stream(archive)?, stdin);
    let output = child.wait_with_output()?;
    let _ = feeder.join();
    if !output.status.success() {
        return Err(command_failure("tar -tf", &output));
    }
    let mut entries = HashSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let entry = line.trim();
        if entry.is_empty() { continue; }
        let path = Path::new(entry);
        if path.is_absolute() || path.components().any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
            return Err(AppError::Runtime(format!("unsafe path in DarwinWine archive: {line}")));
        }
        entries.insert(entry.to_string());
    }
    if entries.is_empty() {
        return Err(AppError::Runtime("DarwinWine archive is empty".into()));
    }
    Ok(entries)
}

fn extract_archive(
    archive: &Path,
    destination: &Path,
    json: bool,
    archive_entries: &HashSet<String>,
) -> Result<()> {
    enum TarLine { Stdout(String), Stderr(String) }

    let mut child = Command::new("/usr/bin/tar")
        .arg("-xvf").arg("-")
        .arg("-C").arg(destination)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let archive_stdin = child.stdin.take()
        .ok_or_else(|| AppError::Runtime("failed to open tar stdin".into()))?;
    let _feeder = feed_archive(open_archive_stream(archive)?, archive_stdin);
    let stdout = child.stdout.take().ok_or_else(|| AppError::Runtime("failed to capture tar stdout".into()))?;
    let stderr = child.stderr.take().ok_or_else(|| AppError::Runtime("failed to capture tar stderr".into()))?;
    let (tx, rx) = mpsc::channel();
    let stdout_tx = tx.clone();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
            let _ = stdout_tx.send(TarLine::Stdout(line));
        }
    });
    let stderr_tx = tx.clone();
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(std::result::Result::ok) {
            let _ = stderr_tx.send(TarLine::Stderr(line));
        }
    });
    drop(tx);

    let total = archive_entries.len();
    let mut extracted = HashSet::new();
    let mut stderr_tail = VecDeque::with_capacity(80);
    let started = Instant::now();
    let mut last_activity = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_secs(2);
    let mut last_percent = usize::MAX;

    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(TarLine::Stdout(line)) | Ok(TarLine::Stderr(line)) => {
                last_activity = Instant::now();
                let normalized = line.strip_prefix("x ").unwrap_or(&line).trim();
                if archive_entries.contains(normalized) {
                    extracted.insert(normalized.to_string());
                } else if !line.trim().is_empty() {
                    if stderr_tail.len() == 80 { stderr_tail.pop_front(); }
                    stderr_tail.push_back(line);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }

        let count = extracted.len().min(total);
        let phase_progress = count as f64 / total as f64;
        let percent = (phase_progress * 100.0).floor() as usize;
        if json && (percent != last_percent || last_emit.elapsed() >= Duration::from_secs(1)) {
            emit_runtime_progress(
                true,
                "Extracting",
                &format!("Extracting DarwinWine · {count}/{total} files"),
                Some(phase_progress),
                Some(0.18 + phase_progress * 0.22),
            )?;
            last_percent = percent;
            last_emit = Instant::now();
        }

        if let Some(status) = child.try_wait()? {
            if !status.success() {
                let detail = stderr_tail.into_iter().collect::<Vec<_>>().join("\n");
                let status_detail = status.code()
                    .map(|code| format!("exit code {code}"))
                    .unwrap_or_else(|| "terminated by signal".into());
                return Err(AppError::Runtime(format!(
                    "DarwinWine extraction failed ({status_detail}): {}",
                    detail.trim()
                )));
            }
            if json {
                emit_runtime_progress(true, "Extracting", "DarwinWine extracted", Some(1.0), Some(0.40))?;
            }
            return Ok(());
        }

        if started.elapsed() >= EXTRACTION_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::Runtime(format!(
                "DarwinWine extraction timed out after {} minutes",
                EXTRACTION_TIMEOUT.as_secs() / 60
            )));
        }
        if last_activity.elapsed() >= EXTRACTION_STALL_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::Runtime(format!(
                "DarwinWine extraction produced no progress for {} minutes",
                EXTRACTION_STALL_TIMEOUT.as_secs() / 60
            )));
        }
    }
}

fn probe_runtime(runtime: &WineRuntime, prefix: &Path) -> Result<()> {
    // First-run validation must mirror DarwinWine itself: pass Wine a path that does
    // not exist yet and let wineboot create the prefix. Pre-creating an empty prefix
    // changes Wine's initialization path and can result in a successful exit without
    // the registry files being materialized.
    remove_path_if_exists(prefix)?;
    let parent = prefix
        .parent()
        .ok_or_else(|| AppError::Runtime("DarwinWine probe prefix has no parent directory".into()))?;
    fs::create_dir_all(parent)?;
    let log_path = parent.join("darwinwine-install-probe.log");
    let _ = fs::remove_file(&log_path);

    let wineboot_status = run_probe_command(
        &runtime.wine,
        &["wineboot", "-u"],
        prefix,
        &runtime.wine,
        &log_path,
        INSTALL_PROBE_TIMEOUT,
        true,
        None,
    )?;

    if !wineboot_status.success() {
        return Err(probe_failure(
            "DarwinWine wineboot probe failed",
            &log_path,
            Some(&wineboot_status),
        ));
    }

    // wineboot can return after handing asynchronous prefix work to wineserver. Wait
    // until the server drains before inspecting system.reg, exactly like DarwinWine's
    // own runtime validator.
    let wineserver_status = run_probe_command(
        &runtime.wineserver,
        &["-w"],
        prefix,
        &runtime.wine,
        &log_path,
        Duration::from_secs(30),
        false,
        None,
    )?;
    if !wineserver_status.success() {
        return Err(probe_failure(
            "DarwinWine wineserver -w probe failed",
            &log_path,
            Some(&wineserver_status),
        ));
    }

    for _ in 0..50 {
        if prefix.join("system.reg").is_file() {
            let _ = fs::remove_file(&log_path);
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    // A quiet probe can legitimately produce no output. Run one bounded diagnostic
    // pass before reporting failure so the UI gets actionable Wine loader/process
    // information rather than only "system.reg missing".
    if fs::metadata(&log_path).map(|metadata| metadata.len() == 0).unwrap_or(true) {
        let _ = run_probe_command(
            &runtime.wine,
            &["wineboot", "-u"],
            prefix,
            &runtime.wine,
            &log_path,
            Duration::from_secs(30),
            true,
            Some("+process,+module,+loaddll,+seh"),
        );
        let _ = run_probe_command(
            &runtime.wineserver,
            &["-w"],
            prefix,
            &runtime.wine,
            &log_path,
            Duration::from_secs(10),
            false,
            None,
        );
    }

    Err(probe_failure(
        "DarwinWine wineboot probe completed but did not create system.reg",
        &log_path,
        None,
    ))
}

fn run_probe_command(
    executable: &Path,
    arguments: &[&str],
    prefix: &Path,
    wine: &Path,
    log_path: &Path,
    timeout: Duration,
    prefix_bootstrap: bool,
    wine_debug_override: Option<&str>,
) -> Result<std::process::ExitStatus> {
    let mut command = Command::new(executable);
    command.args(arguments);
    if prefix_bootstrap {
        configure_command(&mut command, prefix, wine);
        configure_prefix_bootstrap(&mut command);
    } else {
        command.env("WINEPREFIX", prefix);
        configure_runtime_library_environment(&mut command, wine);
    }
    if let Some(debug) = wine_debug_override {
        command.env("WINEDEBUG", debug);
    }

    let log = OpenOptions::new().create(true).append(true).open(log_path)?;
    let stderr = log.try_clone()?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::Runtime(format!(
                "DarwinWine probe command timed out after {} seconds: {}",
                timeout.as_secs(),
                probe_log_tail(log_path)
            )));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn probe_failure(prefix: &str, log_path: &Path, status: Option<&std::process::ExitStatus>) -> AppError {
    let status = status
        .map(process_status_description)
        .map(|value| format!(" ({value})"))
        .unwrap_or_default();
    let detail = probe_log_tail(log_path);
    if detail.is_empty() {
        AppError::Runtime(format!("{prefix}{status}"))
    } else {
        AppError::Runtime(format!("{prefix}{status}: {detail}"))
    }
}

fn probe_log_tail(log_path: &Path) -> String {
    let content = fs::read_to_string(log_path).unwrap_or_default();
    let mut lines = content.lines().rev().take(40).collect::<Vec<_>>();
    lines.reverse();
    lines.join("\n")
}

fn emit_runtime_progress(json: bool, phase: &str, message: &str, progress: Option<f64>, overall: Option<f64>) -> Result<()> {
    if json {
        write_progress("wine_runtime_progress", phase, message, progress, overall, None, None)
    } else {
        println!("[{phase}] {message}");
        Ok(())
    }
}

fn remove_legacy_managed_wine_state(support: &Path) -> Result<()> {
    // v0.8 and earlier stored app-managed Wine providers under `engines` and
    // cached their source packages under `downloads/wine`. DarwinPlay 0.9
    // supports DarwinWine only, so those private app-owned paths are removed
    // after a new DarwinWine runtime has been validated and activated.
    remove_path_if_exists(&support.join("engines"))?;
    remove_path_if_exists(&support.join("downloads").join("wine"))?;
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    if !path.exists() { return Ok(()); }
    if !path.is_dir() {
        fs::remove_file(path)?;
        return Ok(());
    }

    // Rename the tree away first so the original name frees up atomically,
    // then delete. Finder and Spotlight materialize .DS_Store entries inside
    // trees that are being deleted, which makes a plain remove_dir_all fail
    // with ENOTEMPTY (os error 66) on large runtimes.
    static DOOM_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let doomed = path.with_file_name(format!(
        ".doomed-{}-{}",
        std::process::id(),
        DOOM_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let victim = if fs::rename(path, &doomed).is_ok() { doomed } else { path.to_path_buf() };

    let mut last_error = None;
    for attempt in 0..3 {
        if attempt > 0 { thread::sleep(Duration::from_millis(150)); }
        match fs::remove_dir_all(&victim) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.expect("remove_dir_all retried without recording an error").into())
}

/// Best-effort removal of leftovers from crashed or interrupted installs:
/// stale staging trees, an orphaned backup, and rename-parked `.doomed-*`
/// trees whose deletion lost a race with Finder.
fn sweep_stale_install_dirs(runtimes: &Path) {
    let Ok(entries) = fs::read_dir(runtimes) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with(".darwinwine-install-")
            || name.starts_with(".doomed-")
            || name == ".darwinwine-backup"
        {
            let _ = remove_path_if_exists(&entry.path());
        }
    }
}
impl WineRuntime {
    pub fn discover() -> Result<Self> {
        let root = darwinwine_root()?;
        if !root.is_dir() {
            return Err(AppError::RuntimeNotFound);
        }
        let manifest = load_manifest(&root).map_err(|_| AppError::RuntimeNotFound)?;
        validate_manifest(&manifest)?;
        Self::from_root(&root, &manifest)
    }

    fn from_root(root: &Path, manifest: &DarwinWineManifest) -> Result<Self> {
        let wine = root.join(&manifest.entrypoint);
        let wineserver = root.join(&manifest.wineserver);
        if !wine.is_file() || !wineserver.is_file() {
            return Err(AppError::Runtime("DarwinWine entrypoints are missing".into()));
        }
        let version = command_output(Command::new(&wine).arg("--version"))?;
        Ok(Self { wine, wineserver, version })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn doctor(&self) -> Result<DoctorReport> {
        let host_architecture = command_output(Command::new("/usr/bin/uname").arg("-m"))
            .unwrap_or_else(|_| env::consts::ARCH.to_string());
        let wine_architecture = command_output(Command::new("/usr/bin/file").arg(&self.wine))
            .unwrap_or_else(|_| "unknown".to_string());

        Ok(DoctorReport {
            wine_path: self.wine.display().to_string(),
            wine_version: self.version.clone(),
            host_architecture,
            wine_architecture,
        })
    }

    pub fn initialize_prefix(&self, prefix: &Path) -> Result<()> {
        let parent = prefix
            .parent()
            .ok_or_else(|| AppError::Runtime("Wine prefix has no parent directory".into()))?;
        fs::create_dir_all(parent)?;
        let file_name = prefix
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("prefix");
        let log_path = parent.join(format!(".{file_name}.wineboot.log"));
        let _ = fs::remove_file(&log_path);

        let wineboot_status = run_probe_command(
            &self.wine,
            &["wineboot", "-u"],
            prefix,
            &self.wine,
            &log_path,
            INSTALL_PROBE_TIMEOUT,
            true,
            None,
        )?;
        if !wineboot_status.success() {
            return Err(probe_failure("wineboot failed", &log_path, Some(&wineboot_status)));
        }

        let wineserver_status = run_probe_command(
            &self.wineserver,
            &["-w"],
            prefix,
            &self.wine,
            &log_path,
            Duration::from_secs(30),
            false,
            None,
        )?;
        if !wineserver_status.success() {
            return Err(probe_failure(
                "wineserver -w failed",
                &log_path,
                Some(&wineserver_status),
            ));
        }

        for _ in 0..50 {
            if prefix.join("system.reg").is_file() {
                let _ = fs::remove_file(&log_path);
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(probe_failure(
            "wineboot completed but did not create system.reg",
            &log_path,
            None,
        ))
    }

    pub fn launch(
        &self,
        prefix: &Path,
        executable: &Path,
        json: bool,
    ) -> Result<()> {
        let file_name = executable
            .file_name()
            .ok_or_else(|| AppError::MissingFileName(executable.display().to_string()))?;
        let file_name = file_name.to_string_lossy();
        if file_name.chars().any(|character| matches!(character, '\\' | ':')) {
            return Err(AppError::InvalidFile(executable.display().to_string()));
        }
        let parent = executable
            .parent()
            .ok_or_else(|| AppError::MissingParent(executable.display().to_string()))?;
        let windows_path = format!("G:\\{file_name}");
        let mut command = Command::new(&self.wine);
        command.arg(windows_path).current_dir(parent);
        configure_command(&mut command, prefix, &self.wine);
        self.stream_command(command, prefix, json)
            .map(|_| ())
    }

    pub fn launch_windows(
        &self,
        prefix: &Path,
        executable: &str,
        arguments: &[String],
        json: bool,
    ) -> Result<i32> {
        self.launch_windows_in(prefix, executable, arguments, json, None)
    }

    /// Games routinely resolve their data relative to the executable, so a game
    /// started outside its own directory fails to find its assets.
    pub fn launch_windows_in(
        &self,
        prefix: &Path,
        executable: &str,
        arguments: &[String],
        json: bool,
        working_directory: Option<&Path>,
    ) -> Result<i32> {
        validate_windows_executable(executable)?;
        let mut command = Command::new(&self.wine);
        command.arg(executable).args(arguments);
        if let Some(directory) = working_directory {
            command.current_dir(directory);
        }
        configure_command(&mut command, prefix, &self.wine);
        self.stream_command(command, prefix, json)
    }

    pub fn dispatch_windows(
        &self,
        prefix: &Path,
        executable: &str,
        arguments: &[String],
    ) -> Result<i32> {
        validate_windows_executable(executable)?;
        let mut command = Command::new(&self.wine);
        command.arg(executable).args(arguments);
        configure_command(&mut command, prefix, &self.wine);
        let output = command.stdin(Stdio::null()).output()?;
        if output.status.success() {
            Ok(output.status.code().unwrap_or(0))
        } else {
            Err(command_failure("Wine dispatch", &output))
        }
    }

    pub fn run_windows_blocking(
        &self,
        prefix: &Path,
        executable: &str,
        arguments: &[String],
        timeout: Duration,
    ) -> Result<()> {
        validate_windows_executable(executable)?;
        let mut command = Command::new(&self.wine);
        command.arg(executable).args(arguments);
        configure_command(&mut command, prefix, &self.wine);
        let log_path = prefix.join(".darwinplay-wine-command.log");
        let log = File::create(&log_path)?;
        let stderr = log.try_clone()?;
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr))
            .spawn()?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                let output = fs::read_to_string(&log_path).unwrap_or_default();
                let _ = fs::remove_file(&log_path);
                let detail = output.trim();
                let suffix = if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                };
                return Err(AppError::ProcessFailed(format!(
                    "Wine command timed out after {} seconds{suffix}",
                    timeout.as_secs()
                )));
            }
            thread::sleep(Duration::from_millis(100));
        };
        if status.success() {
            let _ = fs::remove_file(&log_path);
            Ok(())
        } else {
            let output = fs::read_to_string(&log_path).unwrap_or_default();
            let _ = fs::remove_file(&log_path);
            let detail = output.trim();
            let detail = if detail.is_empty() {
                process_status_description(&status)
            } else {
                detail.to_string()
            };
            Err(AppError::ProcessFailed(format!("Wine: {detail}")))
        }
    }

    pub fn is_windows_process_running(&self, prefix: &Path, image_name: &str) -> Result<bool> {
        let image_name = image_name.trim();
        if image_name.is_empty() || image_name.chars().any(|character| matches!(character, '\\' | '/' | '\0')) {
            return Err(AppError::InvalidFile(image_name.to_string()));
        }
        let mut command = Command::new(&self.wine);
        command.args(["tasklist.exe", "/fo", "csv", "/nh"]);
        configure_command(&mut command, prefix, &self.wine);
        let output = command.stdin(Stdio::null()).output()?;
        if !output.status.success() {
            return Err(command_failure("tasklist", &output));
        }
        Ok(tasklist_contains_image(
            &String::from_utf8_lossy(&output.stdout),
            image_name,
        ))
    }

    pub fn stop_prefix(&self, prefix: &Path) -> Result<()> {
        if !prefix.exists() {
            return Ok(());
        }
        let mut kill = Command::new(&self.wineserver);
        kill.arg("-k").env("WINEPREFIX", prefix);
        configure_runtime_library_environment(&mut kill, &self.wine);
        let output = kill
            .stdin(Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err(command_failure("wineserver -k", &output));
        }
        let mut wait_command = Command::new(&self.wineserver);
        wait_command.arg("-w").env("WINEPREFIX", prefix);
        configure_runtime_library_environment(&mut wait_command, &self.wine);
        let wait = wait_command
            .stdin(Stdio::null())
            .output()?;
        if wait.status.success() {
            Ok(())
        } else {
            Err(command_failure("wineserver -w", &wait))
        }
    }

    fn stream_command(
        &self,
        mut command: Command,
        prefix: &Path,
        json: bool,
    ) -> Result<i32> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;

        if json {
            let prefix_text = prefix.display().to_string();
            write_json(&RuntimeEvent {
                kind: "started",
                stream: None,
                message: None,
                pid: Some(child.id()),
                exit_code: None,
                prefix: Some(&prefix_text),
            })?;
        } else {
            println!("Started Wine process {}", child.id());
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::ProcessFailed("Wine stdout was not captured".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::ProcessFailed("Wine stderr was not captured".into()))?;
        let (sender, receiver) = mpsc::channel::<(String, String)>();

        let stdout_sender = sender.clone();
        let stdout_thread = thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
                if stdout_sender.send(("stdout".into(), line)).is_err() {
                    break;
                }
            }
        });

        let stderr_sender = sender.clone();
        let stderr_thread = thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(|line| line.ok()) {
                if stderr_sender.send(("stderr".into(), line)).is_err() {
                    break;
                }
            }
        });
        drop(sender);

        let status = loop {
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok((stream, message)) => write_log(json, &stream, &message)?,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break child.wait()?,
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
        };

        drop(stdout_thread);
        drop(stderr_thread);
        for (stream, message) in receiver.try_iter() {
            write_log(json, &stream, &message)?;
        }
        let exit_code = status.code().unwrap_or(-1);

        if json {
            write_json(&RuntimeEvent {
                kind: "exited",
                stream: None,
                message: None,
                pid: None,
                exit_code: Some(exit_code),
                prefix: None,
            })?;
        } else {
            println!("Wine process exited with {exit_code}");
        }
        Ok(exit_code)
    }
}

fn tasklist_contains_image(output: &str, image_name: &str) -> bool {
    output.lines().any(|line| {
        tasklist_image_name(line)
            .is_some_and(|value| value.eq_ignore_ascii_case(image_name))
    })
}

fn tasklist_image_name(line: &str) -> Option<&str> {
    let line = line.trim().trim_start_matches('\u{feff}');
    if line.is_empty() {
        return None;
    }

    if let Some(rest) = line.strip_prefix('"') {
        return rest.find('"').map(|end| &rest[..end]);
    }

    line.split(',')
        .next()
        .map(|value| value.trim().trim_matches('"'))
        .filter(|value| !value.is_empty())
}

fn validate_windows_executable(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let valid = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\'
        && !value.contains("..\\")
        && !value.ends_with("..")
        && !value.contains('/')
        && !value.contains('\0');
    if valid {
        Ok(())
    } else {
        Err(AppError::InvalidFile(value.to_string()))
    }
}

fn configure_command(command: &mut Command, prefix: &Path, wine: &Path) {
    command
        .env("WINEPREFIX", prefix)
        .env("WINEDEBUG", wine_debug())
        .env_remove("WINEDLLPATH")
        .env_remove("WINEDLLPATH_PREPEND")
        .env_remove("WINEDLLOVERRIDES");
    configure_runtime_library_environment(command, wine);
}

fn configure_prefix_bootstrap(command: &mut Command) {
    command
        .env("WINEARCH", "win64")
        .env("WINEDLLOVERRIDES", "winebus.sys=d");
}

fn configure_runtime_library_environment(command: &mut Command, wine: &Path) {
    let Some(lib) = managed_runtime_library_directory(wine) else {
        return;
    };
    if !lib.is_dir() {
        return;
    }

    let mut paths = vec![lib];
    if let Some(existing) = env::var_os("DYLD_FALLBACK_LIBRARY_PATH") {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(joined) = env::join_paths(paths) {
        command.env("DYLD_FALLBACK_LIBRARY_PATH", joined);
    }
}

fn write_log(json: bool, stream: &str, message: &str) -> Result<()> {
    if json {
        write_json(&RuntimeEvent {
            kind: "log",
            stream: Some(stream),
            message: Some(message),
            pid: None,
            exit_code: None,
            prefix: None,
        })?;
    } else {
        println!("[{stream}] {message}");
    }
    Ok(())
}

fn command_failure(name: &str, output: &Output) -> AppError {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        process_status_description(&output.status)
    };
    AppError::ProcessFailed(format!("{name}: {detail}"))
}

fn process_status_description(status: &std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exited with code {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("terminated by signal {signal}");
        }
    }
    "terminated without an exit code".to_string()
}

fn managed_runtime_library_directory(wine: &Path) -> Option<PathBuf> {
    wine.parent()?.parent().map(|root| root.join("lib"))
}


fn command_output(command: &mut Command) -> Result<String> {
    let output = command.stdin(Stdio::null()).output()?;
    if !output.status.success() { return Err(command_failure("command", &output)); }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn wine_debug() -> OsString {
    env::var_os("DARWINPLAY_WINEDEBUG").unwrap_or_else(|| OsString::from("-all"))
}

#[cfg(test)]
mod tests {
    use super::{tasklist_contains_image, validate_manifest, validate_relative_runtime_path, validate_supported_darwinwine_version, DarwinWineManifest};

    #[test]
    fn detects_steam_in_csv_tasklist() {
        let output = "\"steam.exe\",\"00000020\",\"Console\",\"1\",\"120,000 K\"\r\n";
        assert!(tasklist_contains_image(output, "steam.exe"));
    }

    #[test]
    fn tasklist_matches_only_image_name_column() {
        let output = "\"notepad.exe\",\"00000020\",\"steam.exe\",\"1\",\"120,000 K\"";
        assert!(!tasklist_contains_image(output, "steam.exe"));
    }

    #[test]
    fn tasklist_accepts_bom_and_case_insensitive_image_name() {
        let output = "\u{feff}\"STEAM.EXE\",\"00000020\",\"Console\",\"1\",\"120,000 K\"";
        assert!(tasklist_contains_image(output, "steam.exe"));
    }

    #[test]
    fn rejects_parent_runtime_path() {
        assert!(validate_relative_runtime_path("../bin/wine").is_err());
    }

    #[test]
    fn rejects_pre_dp5_crossover_darwinwine() {
        assert!(validate_supported_darwinwine_version("cx26.3-dp4").is_err());
    }

    #[test]
    fn rejects_legacy_winehq_darwinwine_version_family() {
        assert!(validate_supported_darwinwine_version("10.20-dp8").is_err());
    }

    #[test]
    fn accepts_cx26_3_dp9_and_newer_darwinwine() {
        assert!(validate_supported_darwinwine_version("cx26.3-dp5").is_err());
        assert!(validate_supported_darwinwine_version("cx26.3-dp8").is_err());
        assert!(validate_supported_darwinwine_version("cx26.3-dp9").is_ok());
        assert!(validate_supported_darwinwine_version("cx26.3-dp10").is_ok());
        assert!(validate_supported_darwinwine_version("cx26.4-dp1").is_ok());
        assert!(validate_supported_darwinwine_version("cx27.0-dp1").is_ok());
    }
    #[test]
    fn decodes_canonical_minimum_macos_manifest_field() {
        let manifest: DarwinWineManifest = serde_json::from_str(r#"{
            "schemaVersion":2,
            "id":"darwinwine-cx26.3-dp9",
            "name":"DarwinWine",
            "wineVersion":"10.0",
            "darwinWineVersion":"cx26.3-dp9",
            "architecture":"x86_64",
            "minimumMacOS":"13.0",
            "channel":"experimental",
            "entrypoint":"bin/wine",
            "wineserver":"bin/wineserver",
            "steamValidated":true,
            "steamLoginValidated":false
        }"#).unwrap();
        assert_eq!(manifest.minimum_mac_os, "13.0");
    }

    #[test]
    fn accepts_legacy_minimum_macos_alias() {
        let manifest: DarwinWineManifest = serde_json::from_str(r#"{
            "schemaVersion":2,
            "id":"darwinwine-cx26.3-dp9",
            "name":"DarwinWine",
            "wineVersion":"10.0",
            "darwinWineVersion":"cx26.3-dp9",
            "architecture":"x86_64",
            "minimumMacOs":"13.0",
            "channel":"experimental",
            "entrypoint":"bin/wine",
            "wineserver":"bin/wineserver",
            "steamValidated":true,
            "steamLoginValidated":false
        }"#).unwrap();
        assert_eq!(manifest.minimum_mac_os, "13.0");
    }

    #[test]
    fn accepts_schema2_crossover_manifest() {
        let manifest: DarwinWineManifest = serde_json::from_str(r#"{
            "schemaVersion":2,
            "id":"darwinwine-cx26.3-dp9",
            "name":"DarwinWine",
            "wineVersion":"10.0",
            "darwinWineVersion":"cx26.3-dp9",
            "architecture":"x86_64",
            "minimumMacOS":"13.0",
            "channel":"experimental",
            "entrypoint":"bin/wine",
            "wineserver":"bin/wineserver",
            "steamValidated":true,
            "steamLoginValidated":false,
            "wow64":true,
            "inputBackend":"native-no-sdl",
            "upstream":"CodeWeavers CrossOver FOSS",
            "upstreamVersion":"26.3.0",
            "moltenVKVersion":"1.4.2"
        }"#).unwrap();
        assert!(validate_manifest(&manifest).is_ok());
    }

    #[test]
    fn rejects_schema1_manifest() {
        let manifest: DarwinWineManifest = serde_json::from_str(r#"{
            "schemaVersion":1,
            "id":"darwinwine-cx26.3-dp9",
            "name":"DarwinWine",
            "wineVersion":"10.0",
            "darwinWineVersion":"cx26.3-dp9",
            "architecture":"x86_64",
            "minimumMacOS":"13.0",
            "channel":"experimental",
            "entrypoint":"bin/wine",
            "wineserver":"bin/wineserver",
            "steamValidated":true,
            "steamLoginValidated":false
        }"#).unwrap();
        assert!(validate_manifest(&manifest).is_err());
    }

}
