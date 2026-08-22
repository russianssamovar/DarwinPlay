//! Declarative per-game compatibility fixes.
//!
//! This is the compatibility database CLAUDE.md requires before any
//! app-specific rule may exist: every entry is data, not code, and every
//! entry carries the reason it exists. Fixes are applied idempotently on
//! every launch (registry writes overwrite the same values), so a recreated
//! prefix heals itself and no applied-state file can go stale.
//!
//! The only fix kind today is a per-executable DLL override, written to
//! `HKCU\Software\Wine\AppDefaults\<exe>\DllOverrides` through `reg.exe`
//! inside the prefix. Registry writes go through Wine itself — editing
//! `user.reg` directly races a running wineserver.

use crate::error::{AppError, Result};
use crate::wine::WineRuntime;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

const REG_EXE: &str = "C:\\windows\\system32\\reg.exe";
/// A cold prefix boots wineserver (and, since DarwinWine dp11, registers the
/// media stack) before reg.exe runs, so this is deliberately generous.
const REG_TIMEOUT: Duration = Duration::from_secs(180);

// The full Wine DLL-override vocabulary; only Disabled has a database user
// today, but the set is closed and fixed by Wine, not by this database.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideMode {
    /// The DLL never loads (registry value "").
    Disabled,
    Native,
    Builtin,
    NativeThenBuiltin,
    BuiltinThenNative,
}

impl OverrideMode {
    fn registry_value(self) -> &'static str {
        match self {
            OverrideMode::Disabled => "",
            OverrideMode::Native => "native",
            OverrideMode::Builtin => "builtin",
            OverrideMode::NativeThenBuiltin => "native,builtin",
            OverrideMode::BuiltinThenNative => "builtin,native",
        }
    }

    fn label(self) -> &'static str {
        match self {
            OverrideMode::Disabled => "disabled",
            OverrideMode::Native => "native",
            OverrideMode::Builtin => "builtin",
            OverrideMode::NativeThenBuiltin => "native,builtin",
            OverrideMode::BuiltinThenNative => "builtin,native",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DllOverride {
    pub dll: &'static str,
    pub mode: OverrideMode,
}

#[derive(Debug, Clone, Copy)]
pub struct GameFix {
    pub app_id: u32,
    pub title: &'static str,
    /// Image name Wine matches for `AppDefaults`, e.g. "DD2.exe".
    pub executable: &'static str,
    pub dll_overrides: &'static [DllOverride],
    pub reason: &'static str,
}

/// What a fix looks like in the serialized compatibility profile.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameFixReport {
    pub executable: String,
    pub dll_overrides: Vec<String>,
    pub reason: String,
}

pub const KNOWN_FIXES: &[GameFix] = &[GameFix {
    app_id: 2054970,
    title: "Dragon's Dogma 2",
    executable: "DD2.exe",
    dll_overrides: &[DllOverride { dll: "nvapi64", mode: OverrideMode::Disabled }],
    reason: "D3DMetal ships nvapi64.dll, which sends NVIDIA Streamline down the \
             NVAPI path; on D3DMetal it then dereferences an unproxied \
             ID3D12CommandQueue and crashes at launch. Hiding nvapi64 for this \
             executable makes Streamline pass through. DLSS is reported \
             unsupported on this hardware either way.",
}];

pub fn fixes_for(app_id: u32) -> Vec<&'static GameFix> {
    KNOWN_FIXES.iter().filter(|fix| fix.app_id == app_id).collect()
}

pub fn reports_for(app_id: u32) -> Vec<GameFixReport> {
    fixes_for(app_id)
        .into_iter()
        .map(|fix| GameFixReport {
            executable: fix.executable.to_string(),
            dll_overrides: fix
                .dll_overrides
                .iter()
                .map(|entry| format!("{}={}", entry.dll, entry.mode.label()))
                .collect(),
            reason: fix.reason.to_string(),
        })
        .collect()
}

/// Registry command lines for one fix: one `reg.exe add` argument vector per
/// DLL override. Pure so tests can pin the exact arguments; arguments stay an
/// array end to end — nothing here may ever pass through a shell.
fn registry_arguments(fix: &GameFix) -> Vec<Vec<String>> {
    fix.dll_overrides
        .iter()
        .map(|entry| {
            vec![
                "add".to_string(),
                format!(
                    "HKCU\\Software\\Wine\\AppDefaults\\{}\\DllOverrides",
                    fix.executable
                ),
                "/v".to_string(),
                entry.dll.to_string(),
                "/t".to_string(),
                "REG_SZ".to_string(),
                "/d".to_string(),
                entry.mode.registry_value().to_string(),
                "/f".to_string(),
            ]
        })
        .collect()
}

fn validate_fix(fix: &GameFix) -> Result<()> {
    let executable_ok = !fix.executable.is_empty()
        && fix.executable.to_ascii_lowercase().ends_with(".exe")
        && !fix
            .executable
            .chars()
            .any(|character| matches!(character, '/' | '\\' | ':' | '\0'));
    let dlls_ok = !fix.dll_overrides.is_empty()
        && fix.dll_overrides.iter().all(|entry| {
            !entry.dll.is_empty()
                && entry
                    .dll
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-' || character == '.')
        });
    if executable_ok && dlls_ok {
        Ok(())
    } else {
        Err(AppError::InvalidCompatibilityProfile(format!(
            "invalid game fix entry for Steam app {}",
            fix.app_id
        )))
    }
}

/// Apply every known fix for the AppID to the prefix. Returns one
/// human-readable line per applied fix for event reporting.
pub fn apply(runtime: &WineRuntime, prefix: &Path, app_id: u32) -> Result<Vec<String>> {
    let mut applied = Vec::new();
    for fix in fixes_for(app_id) {
        validate_fix(fix)?;
        for arguments in registry_arguments(fix) {
            runtime.run_windows_blocking(prefix, REG_EXE, &arguments, REG_TIMEOUT)?;
        }
        let overrides = fix
            .dll_overrides
            .iter()
            .map(|entry| format!("{}={}", entry.dll, entry.mode.label()))
            .collect::<Vec<_>>()
            .join(", ");
        applied.push(format!(
            "Applied compatibility fix for {} ({}): {}",
            fix.title, fix.executable, overrides
        ));
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_fix_is_valid() {
        for fix in KNOWN_FIXES {
            validate_fix(fix).unwrap();
            assert!(!fix.reason.is_empty(), "fix for {} has no reason", fix.app_id);
            assert!(!fix.title.is_empty());
        }
    }

    #[test]
    fn dragons_dogma_2_disables_nvapi() {
        let fixes = fixes_for(2054970);
        assert_eq!(fixes.len(), 1);
        let arguments = registry_arguments(fixes[0]);
        assert_eq!(
            arguments,
            vec![vec![
                "add".to_string(),
                "HKCU\\Software\\Wine\\AppDefaults\\DD2.exe\\DllOverrides".to_string(),
                "/v".to_string(),
                "nvapi64".to_string(),
                "/t".to_string(),
                "REG_SZ".to_string(),
                "/d".to_string(),
                String::new(),
                "/f".to_string(),
            ]]
        );
    }

    #[test]
    fn unknown_app_has_no_fixes() {
        assert!(fixes_for(570).is_empty());
        assert!(reports_for(570).is_empty());
    }

    #[test]
    fn override_modes_map_to_wine_registry_values() {
        assert_eq!(OverrideMode::Disabled.registry_value(), "");
        assert_eq!(OverrideMode::Native.registry_value(), "native");
        assert_eq!(OverrideMode::Builtin.registry_value(), "builtin");
        assert_eq!(OverrideMode::NativeThenBuiltin.registry_value(), "native,builtin");
        assert_eq!(OverrideMode::BuiltinThenNative.registry_value(), "builtin,native");
    }

    #[test]
    fn reports_render_override_labels() {
        let reports = reports_for(2054970);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].executable, "DD2.exe");
        assert_eq!(reports[0].dll_overrides, vec!["nvapi64=disabled".to_string()]);
        assert!(reports[0].reason.contains("Streamline"));
    }

    #[test]
    fn rejects_malformed_entries() {
        let bad = GameFix {
            app_id: 1,
            title: "Bad",
            executable: "..\\evil.exe",
            dll_overrides: &[DllOverride { dll: "nvapi64", mode: OverrideMode::Disabled }],
            reason: "test",
        };
        assert!(validate_fix(&bad).is_err());
        let empty = GameFix {
            app_id: 1,
            title: "Bad",
            executable: "game.exe",
            dll_overrides: &[],
            reason: "test",
        };
        assert!(validate_fix(&empty).is_err());
    }
}
