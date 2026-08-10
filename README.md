# DarwinPlay

DarwinPlay is a macOS-first launcher for Windows games on Apple Silicon. SwiftUI owns presentation; the Rust runtime owns DarwinWine, prefixes, Steam, graphics translation, PE inspection and compatibility decisions.

## Runtime policy

**DarwinPlay supports DarwinWine only.**

There is no System Wine discovery, Homebrew/WineHQ Wine installation, Sikarugir integration, arbitrary Wine path, or engine selector. DarwinWine is built and released from its own repository; DarwinPlay consumes only the packaged runtime artifact.

Current minimum runtime: **DarwinWine cx26.3-dp5** (`x86_64`). Legacy WineHQ-based runtimes are rejected; cx26.3-dp5 is the current CrossOver-derived DarwinWine baseline for DarwinPlay installation probes.

```text
DarwinWine repository
        |
        | build / validate / package
        v
DarwinWine-cx26.3-dp5-macos-x86_64.tar.zst
        |
        | Install Runtime
        v
~/Library/Application Support/DarwinPlay/runtime/darwinwine/
        |
        +-- Steam prefix
        +-- game prefixes
        `-- DarwinPlay launches
```

DarwinWine source code is never copied or vendored into this repository.

## Current scope

- DarwinWine-only runtime lifecycle with staged validation and atomic activation
- Official Windows Steam client in a dedicated shared prefix
- Steam bootstrapper download and install progress
- Steam library discovery from `libraryfolders.vdf` and `appmanifest_*.acf`
- Steam launch by AppID through `steam.exe -applaunch`
- Managed DXMT installation/update as a graphics component independent from DarwinWine
- WineD3D baseline and DXMT per-game/Steam UI policies
- PE32/PE32+ inspection and graphics API detection
- Per-game compatibility analysis and persistent overrides
- Recent games, favorites and last-played activity
- Runtime console with Wine, Steam and graphics events
- Structured JSON/JSONL protocol between SwiftUI and Rust

## Requirements

- Apple Silicon Mac
- macOS 15 or newer
- Rosetta 2 (DarwinWine CrossOver runtimes are x86_64)
- Full Xcode selected with `xcode-select`
- Rust 1.85 or newer to build DarwinPlay itself
- A **DarwinWine cx26.3-dp5 or newer** runtime (downloaded by the app, or a packaged artifact)
- Internet access for Steam and runtime downloads

Homebrew and Sikarugir are **not DarwinPlay runtime dependencies**.

## Install with Homebrew

```bash
brew tap russianssamovar/tap
brew trust russianssamovar/tap
brew install --cask --no-quarantine darwinplay
```

The app is ad-hoc signed, not notarized — `--no-quarantine` keeps Gatekeeper
from killing it on launch (or clear the flag later with
`xattr -dr com.apple.quarantine /Applications/DarwinPlay.app`).

## Build DarwinPlay

```bash
make preflight
make test
make app
open dist/DarwinPlay.app
```

If full Xcode is installed but not selected:

```bash
sudo xcode-select --switch "/Applications/Xcode.app/Contents/Developer"
sudo xcodebuild -runFirstLaunch
```

## Install DarwinWine

From the UI, use **Download & Install** — the app fetches the newest published
DarwinWine release from GitHub, verifies its checksum and installs it. The same
operation is available through the runtime CLI:

```bash
cargo run --manifest-path runtime/Cargo.toml -- runtime install-latest
```

To install a locally built artifact instead (it must contain a schema-2
`runtime.json` and the declared `wine` / `wineserver` entrypoints), use
**From File…** in Settings, or:

```bash
cargo run --manifest-path runtime/Cargo.toml -- \
  runtime install --archive /path/to/DarwinWine-cx26.3-dp8-macos-x86_64.tar.zst --json
```

Inspect status:

```bash
cargo run --manifest-path runtime/Cargo.toml -- runtime status --json
cargo run --manifest-path runtime/Cargo.toml -- doctor --json
```

Installation is transactional. DarwinPlay validates archive paths and `runtime.json`, rejects unsupported/old DarwinWine builds, probes `wine --version`, creates a disposable prefix using `wineboot`, then atomically activates the runtime. Existing DarwinPlay-owned v0.8 Wine-engine caches are removed after successful DarwinWine activation.

## Steam

Steam lives in:

```text
~/Library/Application Support/DarwinPlay/prefixes/steam
```

Install:

```bash
cargo run --manifest-path runtime/Cargo.toml -- steam install --json
```

Start:

```bash
cargo run --manifest-path runtime/Cargo.toml -- steam start --backend auto --json
```

List games:

```bash
cargo run --manifest-path runtime/Cargo.toml -- steam games --json
```

Launch an AppID:

```bash
cargo run --manifest-path runtime/Cargo.toml -- \
  steam run --app-id 292030 --backend auto --json
```

DarwinPlay does not handle Steam credentials. Authentication stays inside the Windows Steam client.

## Prefix compatibility

Each managed prefix records the DarwinWine/Wine version used to initialize it. A runtime-incompatible Wine major change requires an explicit prefix reset. There is no fallback to another Wine runtime.

## Graphics

DarwinWine is the Wine runtime; graphics translation remains a separate component boundary.

- WineD3D is always the baseline path.
- DXMT can be installed/updated independently and selected for Steam UI or games.
- Installing DXMT never mutates the DarwinWine runtime artifact.

```bash
cargo run --manifest-path runtime/Cargo.toml -- graphics dxmt install-latest --json
cargo run --manifest-path runtime/Cargo.toml -- graphics dxmt status --json
```

## Repository boundary

`DarwinPlay` owns launcher UX, runtime installation, prefixes, Steam and compatibility orchestration.

`DarwinWine` owns Wine source version, patches, build toolchain, bundled dylib closure, FreeType/GnuTLS/Schannel validation, packaging and release artifacts.

Keep that boundary strict: **DarwinWine source and build dependencies do not belong in DarwinPlay.**


## Runtime installation progress

DarwinPlay streams DarwinWine archive extraction progress instead of holding the setup UI at a fixed percentage. Extraction reports files processed, emits a heartbeat while active, and fails with a diagnostic if the archive extractor stops making progress. DarwinWine remains the only supported runtime.


## Steam CEF safe mode

DarwinPlay launches the Windows Steam client with GPU-accelerated CEF web views disabled by default. It sets `GPUAccelWebViewsV3=0` and passes `-cef-disable-gpu`, then records policy version 9 so an older running Steam UI is restarted once to pick up the policy. This affects Steam's embedded web UI only; per-game graphics backends remain independent.

## Steam UI composition policy

DarwinPlay keeps the Steam client UI on WineD3D/system composition even when DXMT is installed for games. Steam is launched with `-cef-disable-gpu -system-composer`, while `GPUAccelWebViewsV3` remains disabled. DXMT remains a per-game graphics backend.
