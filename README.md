# DarwinPlay

DarwinPlay is a macOS-first compatibility launcher for Windows games on Apple Silicon. SwiftUI owns presentation. Rust owns Wine, Steam, graphics selection, PE inspection and compatibility profiles.

## Current scope

- Homebrew-managed Wine install, reinstall and removal
- Wine discovery, first-launch readiness and diagnostics
- Official Windows Steam client inside a dedicated shared Wine prefix
- Steam library discovery from `libraryfolders.vdf` and `appmanifest_*.acf`
- Steam launch by AppID through `steam.exe -applaunch`
- Recent games, favorites and last-played activity
- Steam artwork loaded from Steam CDN with local fallbacks
- Isolated Wine prefixes for manually imported Windows executables
- PE32/PE32+ inspection and graphics API detection
- DXMT managed component with DXMT/WineD3D selection
- Per-game compatibility analysis and persistent user overrides
- Ranked executable candidates with D3D10/11/12, D3D9, Vulkan and OpenGL detection
- Per-game backend override, analysis target and launch arguments
- Per-AppID DXMT logs and shader cache while retaining one Steam prefix
- Dedicated runtime console with Wine, Steam and graphics filters
- Structured JSON and JSONL runtime protocol

## Requirements

- Apple Silicon Mac
- macOS 15 or newer
- Full Xcode selected with `xcode-select`
- Rust 1.85 or newer
- Homebrew for managed Wine installation
- DXMT package only when using the DXMT backend

DarwinPlay can install Wine Stable itself after Homebrew is available. A custom Wine executable can still be configured in Settings.

## Build

```bash
make test
make app
open dist/DarwinPlay.app
```

The macOS preflight rejects `/Library/Developer/CommandLineTools` and uses the selected Xcode toolchain through `xcrun`. If full Xcode is installed but not selected, run:

```bash
sudo xcode-select --switch "/Applications/Xcode.app/Contents/Developer"
sudo xcodebuild -runFirstLaunch
```

## Wine

Inspect Wine and Homebrew state:

```bash
cargo run --manifest-path runtime/Cargo.toml -- wine status --json
```

Install Wine Stable through Homebrew:

```bash
cargo run --manifest-path runtime/Cargo.toml -- wine install --json
```

Reinstall or remove the Homebrew-managed runtime:

```bash
cargo run --manifest-path runtime/Cargo.toml -- wine reinstall --json
cargo run --manifest-path runtime/Cargo.toml -- wine remove --json
```

If macOS requires first-launch approval, DarwinPlay reports Wine as installed but not ready and offers to open Wine once before Steam setup continues.

## Steam

Steam is installed into:

```text
~/Library/Application Support/DarwinPlay/prefixes/steam
```

Install the official Windows Steam client:

```bash
cargo run --manifest-path runtime/Cargo.toml -- steam install --json
```

Start Steam:

```bash
cargo run --manifest-path runtime/Cargo.toml -- steam start --json
```

List installed games:

```bash
cargo run --manifest-path runtime/Cargo.toml -- steam games --json
```

Analyze a game:

```bash
cargo run --manifest-path runtime/Cargo.toml -- \
  steam profile show \
  --app-id 292030 \
  --fallback-backend auto \
  --json
```

Persist a game-specific profile:

```bash
cargo run --manifest-path runtime/Cargo.toml -- \
  steam profile set \
  --app-id 292030 \
  --backend dxmt \
  --executable "bin/x64/witcher3.exe" \
  --launch-argument=-dx11 \
  --fallback-backend auto \
  --json
```

Launch the game:

```bash
cargo run --manifest-path runtime/Cargo.toml -- \
  steam run \
  --app-id 292030 \
  --backend auto \
  --json
```

The selected executable is an analysis target. Steam remains responsible for its configured launch entry. DarwinPlay does not bypass Steam launch configuration, Steamworks, licensing or authentication.

## Compatibility model

DarwinPlay scans Windows executables under the installed game directory with deterministic traversal and bounded work. Redistributables, tools and launchers remain visible but receive lower scores than likely game executables.

The current backend policy is conservative:

- Direct3D 10/11: recommend DXMT
- Direct3D 9: WineD3D fallback
- Vulkan: Wine Vulkan path
- OpenGL: Wine path
- Direct3D 12: report unsupported until a D3D12 translation backend is integrated
- Unknown imports: report unknown instead of guessing

Generated facts are never persisted. Only user overrides are stored, so Steam updates cannot leave stale generated compatibility data as the source of truth.

Profiles are stored under:

```text
~/Library/Application Support/DarwinPlay/compatibility/steam/<AppID>.json
```

## DXMT

Install an extracted DXMT package:

```bash
cargo run --manifest-path runtime/Cargo.toml -- \
  graphics dxmt install \
  --source /path/to/dxmt \
  --mode builtin \
  --json
```

## Launcher UI

DarwinPlay 0.6.8 uses a restrained console-style launcher interface inspired by living-room game systems rather than standard macOS navigation.

The macOS preflight validates full Xcode, the selected macOS SDK, Swift, and Rust. XCTest is validated by the actual SwiftPM test build instead of a standalone Swift interpreter import, which avoids false negatives on valid Xcode installations.

- Centered top navigation: Home, Games and Console
- Large latest-played hero on Home
- Vertical Steam covers with wide artwork on game details
- Recent and favorite game rows
- Cold blue-gray surfaces with muted `#8FBA63` focus and play accents
- Initial Wine and Steam setup shown once as game-like cards
- Runtime management moved to Settings after installation
- Separate Console view for Wine, Steam, graphics and error logs
- Darwin finch compatibility-layer application icon

## Development

The repository follows small vertical slices. Rust is the source of truth for Wine, Steam and compatibility decisions. Swift decodes typed runtime responses and does not duplicate those rules.

The Swift test target uses XCTest and has no external testing dependency.

See `docs/ARCHITECTURE.md`, `docs/COMPATIBILITY.md`, `docs/DEVELOPMENT.md` and `docs/SECURITY.md`.


### 0.6.2 Wine readiness diagnostics

Wine readiness failures are now surfaced as probe diagnostics instead of being classified generically as Gatekeeper blocks. The setup screen provides Open Wine, Privacy & Security, and Try Again actions, and Steam remains visually disabled until the Wine CLI probe succeeds.

### 0.6.3 Steam installer path isolation

DarwinPlay removes Wine's default `Z:` mapping so Windows processes cannot browse the entire macOS filesystem. Steam installation therefore exposes only the DarwinPlay downloads directory as temporary drive `I:` and launches `I:\\SteamSetup.exe`. The mapping is removed after the installer returns. Installed Steam is launched from its native `C:` path inside the dedicated Steam prefix.


### 0.6.5 Steam single-instance lifecycle

DarwinPlay now queries Wine's process list for `steam.exe` in the dedicated Steam prefix before opening the client. `Open Steam` is idempotent: if Steam is already running, no new client is started. Steam status exposes the running state to SwiftUI, where Open is disabled and the UI reports Running. Launching a Steam game reuses the existing client when present; a clean prefix restart is reserved for cases where Steam is not running.

### 0.6.7 Steam CEF compositor policy

Steam Web UI launches with both `-cef-disable-gpu` and `-cef-disable-gpu-compositing`. DarwinPlay records the UI policy version used for the current Steam session. When an older Steam process is still running after a DarwinPlay update, the launcher reports that a UI restart is required instead of treating the old process as compatible merely because `steam.exe` exists. Restarting the UI remains explicit and never creates a second persistent Steam client.

### 0.6.8 Steam UI OpenGL policy

The Steam client UI now starts with `-cef-disable-gpu`, `-cef-disable-gpu-compositing`, `-cef-disable-occlusion`, `-opengl`, and `-system-composer`. These arguments affect the Steam client UI only; DarwinPlay does not disable Wine Vulkan globally because Steam-launched games inherit the client's environment.

To inspect what Steam actually passed to `steamwebhelper.exe`:

```bash
./dist/DarwinPlay.app/Contents/MacOS/darwinplay-runtime steam diagnostics --json
```

The diagnostic reads Steam's `webhelper.txt` and `cef_log.txt`, reports whether the two CEF GPU-disable switches are present on the observed WebHelper command line, and reports whether those logs still mention Vulkan.

## Prefix integrity

DarwinPlay creates Wine prefixes transactionally. A new prefix is initialized in a staging directory and promoted only after `kernel32.dll`, `system.reg`, and `user.reg` are present. Incomplete prefixes from interrupted initialization are discarded and recreated on the next attempt. A prefix that was previously marked initialized but later loses required runtime files is reported as corrupted instead of being silently reused.
