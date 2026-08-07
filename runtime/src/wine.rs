use crate::error::{AppError, Result};
use crate::events::{write_json, RuntimeEvent};
use crate::graphics::{GraphicsBackend, LaunchGraphics};
use serde::Serialize;
use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const HOMEBREW_WINE_CASK: &str = "wine-stable";

#[derive(Clone)]
pub struct WineRuntime {
    wine: PathBuf,
    wineserver: PathBuf,
    version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WineStatus {
    pub installed: bool,
    pub ready: bool,
    pub wine_path: Option<String>,
    pub wine_version: Option<String>,
    pub probe_error: Option<String>,
    pub homebrew_installed: bool,
    pub homebrew_path: Option<String>,
    pub managed_by_homebrew: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum WineManagedAction {
    Install,
    Reinstall,
    Remove,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub wine_path: String,
    pub wine_version: String,
    pub host_architecture: String,
    pub wine_architecture: String,
}

pub fn wine_status(explicit: Option<&Path>) -> WineStatus {
    let brew = discover_homebrew();
    let homebrew_wine_installed = brew
        .as_ref()
        .is_some_and(|path| homebrew_has_wine(path));
    let candidate = discover_wine(explicit).ok();
    let managed_by_homebrew = homebrew_wine_installed
        && candidate
            .as_ref()
            .is_some_and(|path| is_homebrew_managed_wine(path));
    match WineRuntime::discover(explicit) {
        Ok(runtime) => WineStatus {
            installed: true,
            ready: true,
            wine_path: Some(runtime.wine.display().to_string()),
            wine_version: Some(runtime.version.clone()),
            probe_error: None,
            homebrew_installed: brew.is_some(),
            homebrew_path: brew.map(|path| path.display().to_string()),
            managed_by_homebrew,
        },
        Err(error) => WineStatus {
            installed: candidate.is_some(),
            ready: false,
            wine_path: candidate.map(|path| path.display().to_string()),
            wine_version: None,
            probe_error: Some(error.to_string()),
            homebrew_installed: brew.is_some(),
            homebrew_path: brew.map(|path| path.display().to_string()),
            managed_by_homebrew,
        },
    }
}

pub fn manage_wine(action: WineManagedAction, json: bool) -> Result<()> {
    let brew = discover_homebrew().ok_or(AppError::HomebrewNotFound)?;
    let mut command = Command::new(&brew);
    match action {
        WineManagedAction::Install => {
            command.args(["install", "--cask", HOMEBREW_WINE_CASK]);
        }
        WineManagedAction::Reinstall => {
            command.args(["reinstall", "--cask", HOMEBREW_WINE_CASK]);
        }
        WineManagedAction::Remove => {
            command.args(["uninstall", "--cask", HOMEBREW_WINE_CASK]);
        }
    }
    configure_homebrew_command(&mut command);
    stream_managed_command(command, json)?;
    if !matches!(action, WineManagedAction::Remove) && !homebrew_has_wine(&brew) {
        return Err(AppError::WineNotFound);
    }
    Ok(())
}

impl WineRuntime {
    pub fn discover(explicit: Option<&Path>) -> Result<Self> {
        let wine = discover_wine(explicit)?;
        let wineserver = discover_wineserver(&wine)?;
        let version = command_output(Command::new(&wine).arg("--version"))?;
        Ok(Self {
            wine,
            wineserver,
            version,
        })
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
        let mut command = Command::new(&self.wine);
        command.arg("wineboot.exe").arg("-u");
        configure_command(&mut command, prefix, &LaunchGraphics::wined3d());
        let output = command.stdin(Stdio::null()).output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_failure("wineboot", &output))
        }
    }

    pub fn launch(
        &self,
        prefix: &Path,
        executable: &Path,
        json: bool,
        graphics: &LaunchGraphics,
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
        configure_command(&mut command, prefix, graphics);
        self.stream_command(command, prefix, json, graphics.backend)
            .map(|_| ())
    }

    pub fn launch_windows(
        &self,
        prefix: &Path,
        executable: &str,
        arguments: &[String],
        json: bool,
        graphics: &LaunchGraphics,
    ) -> Result<i32> {
        validate_windows_executable(executable)?;
        let mut command = Command::new(&self.wine);
        command.arg(executable).args(arguments);
        configure_command(&mut command, prefix, graphics);
        self.stream_command(command, prefix, json, graphics.backend)
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
        configure_command(&mut command, prefix, &LaunchGraphics::wined3d());
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
        graphics: &LaunchGraphics,
        timeout: Duration,
    ) -> Result<()> {
        validate_windows_executable(executable)?;
        let mut command = Command::new(&self.wine);
        command.arg(executable).args(arguments);
        configure_command(&mut command, prefix, graphics);
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
        configure_command(&mut command, prefix, &LaunchGraphics::wined3d());
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
        let output = Command::new(&self.wineserver)
            .arg("-k")
            .env("WINEPREFIX", prefix)
            .stdin(Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err(command_failure("wineserver -k", &output));
        }
        let wait = Command::new(&self.wineserver)
            .arg("-w")
            .env("WINEPREFIX", prefix)
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
        backend: GraphicsBackend,
    ) -> Result<i32> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let backend = backend_name(backend);

        if json {
            let prefix_text = prefix.display().to_string();
            write_json(&RuntimeEvent {
                kind: "started",
                stream: None,
                message: None,
                backend: Some(backend),
                pid: Some(child.id()),
                exit_code: None,
                prefix: Some(&prefix_text),
            })?;
        } else {
            println!("Started Wine process {} with {backend}", child.id());
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
                backend: Some(backend),
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
        line.split(|character: char| character == ',' || character.is_whitespace())
            .map(|value| value.trim_matches('"'))
            .any(|value| value.eq_ignore_ascii_case(image_name))
    })
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

fn configure_command(command: &mut Command, prefix: &Path, graphics: &LaunchGraphics) {
    command
        .env("WINEPREFIX", prefix)
        .env("WINEDEBUG", wine_debug())
        .env_remove("WINEDLLPATH")
        .env_remove("WINEDLLOVERRIDES")
        .env_remove("DXMT_LOG_LEVEL")
        .env_remove("DXMT_LOG_PATH")
        .env_remove("DXMT_SHADER_CACHE_PATH")
        .envs(&graphics.environment);
}

fn write_log(json: bool, stream: &str, message: &str) -> Result<()> {
    if json {
        write_json(&RuntimeEvent {
            kind: "log",
            stream: Some(stream),
            message: Some(message),
            backend: None,
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

fn backend_name(backend: GraphicsBackend) -> &'static str {
    match backend {
        GraphicsBackend::Auto => "auto",
        GraphicsBackend::WineD3d => "wined3d",
        GraphicsBackend::Dxmt => "dxmt",
    }
}

fn discover_wine(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return if path.is_file() {
            Ok(path.to_path_buf())
        } else {
            Err(AppError::InvalidFile(path.display().to_string()))
        };
    }

    if let Some(path) = env::var_os("DARWINPLAY_WINE") {
        let path = PathBuf::from(path);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(AppError::InvalidFile(path.display().to_string()))
        };
    }

    let mut candidates = Vec::new();
    if let Some(path) = find_in_path("wine") {
        candidates.push(path);
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/wine"),
        PathBuf::from("/usr/local/bin/wine"),
        PathBuf::from("/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine"),
        PathBuf::from("/Applications/Wine Devel.app/Contents/Resources/wine/bin/wine"),
    ]);

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or(AppError::WineNotFound)
}

fn discover_wineserver(wine: &Path) -> Result<PathBuf> {
    if let Some(parent) = wine.parent() {
        let sibling = parent.join("wineserver");
        if sibling.is_file() {
            return Ok(sibling);
        }
    }
    find_in_path("wineserver").ok_or(AppError::WineServerNotFound)
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn command_output(command: &mut Command) -> Result<String> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(command_failure("command", &output));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn discover_homebrew() -> Option<PathBuf> {
    [
        PathBuf::from("/opt/homebrew/bin/brew"),
        PathBuf::from("/usr/local/bin/brew"),
    ]
    .into_iter()
    .chain(find_in_path("brew"))
    .find(|candidate| candidate.is_file())
}

fn is_homebrew_managed_wine(wine: &Path) -> bool {
    wine.starts_with("/Applications/Wine Stable.app")
        || wine == Path::new("/opt/homebrew/bin/wine")
        || wine == Path::new("/usr/local/bin/wine")
}

fn homebrew_has_wine(brew: &Path) -> bool {
    let mut command = Command::new(brew);
    command.args(["list", "--cask", HOMEBREW_WINE_CASK]);
    configure_homebrew_command(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn configure_homebrew_command(command: &mut Command) {
    let current = env::var("PATH").unwrap_or_default();
    let base = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";
    let path = if current.is_empty() {
        base.to_string()
    } else {
        format!("{base}:{current}")
    };
    command.env("PATH", path).env("HOMEBREW_NO_AUTO_UPDATE", "1");
}

fn stream_managed_command(mut command: Command, json: bool) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    if json {
        write_json(&RuntimeEvent {
            kind: "started",
            stream: None,
            message: None,
            backend: None,
            pid: Some(child.id()),
            exit_code: None,
            prefix: None,
        })?;
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::ProcessFailed("Homebrew stdout was not captured".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::ProcessFailed("Homebrew stderr was not captured".into()))?;
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

    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    for (stream, message) in receiver.try_iter() {
        write_log(json, &stream, &message)?;
    }
    let exit_code = status.code().unwrap_or(-1);
    if json {
        write_json(&RuntimeEvent {
            kind: "exited",
            stream: None,
            message: None,
            backend: None,
            pid: None,
            exit_code: Some(exit_code),
            prefix: None,
        })?;
    }
    if status.success() {
        Ok(())
    } else {
        Err(AppError::ProcessFailed(format!(
            "Homebrew exited with {exit_code}"
        )))
    }
}

fn wine_debug() -> OsString {
    env::var_os("DARWINPLAY_WINEDEBUG").unwrap_or_else(|| OsString::from("-all"))
}

#[cfg(test)]
mod process_tests {
    use super::tasklist_contains_image;

    #[test]
    fn detects_steam_in_csv_tasklist() {
        let output = "\"steam.exe\",\"00000020\",\"Console\",\"1\",\"120,000 K\"\n\"steamwebhelper.exe\",\"00000024\",\"Console\",\"1\",\"80,000 K\"";
        assert!(tasklist_contains_image(output, "steam.exe"));
    }

    #[test]
    fn does_not_confuse_steamwebhelper_with_steam() {
        let output = "steamwebhelper.exe 00000024 Console 1";
        assert!(!tasklist_contains_image(output, "steam.exe"));
    }
}
