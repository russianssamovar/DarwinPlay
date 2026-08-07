# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

DarwinPlay is a macOS-first (Apple Silicon, macOS 15+) compatibility launcher for Windows games. Two halves:

- `runtime/` — Rust CLI `darwinplay-runtime` (edition 2024, rust 1.85+). Owns Wine, Steam, graphics backends, PE inspection and compatibility policy.
- `app/` — SwiftPM package (swift-tools 6.2): `DarwinPlayCore` library (models, `RuntimeClient`, actor-based JSON stores) + `DarwinPlay` SwiftUI executable (macOS-only, guarded by `#if os(macOS)` in `Package.swift`).

Swift never reimplements runtime policy — it shells out to the Rust binary and decodes typed JSON.

## Commands

```bash
make test        # cargo test + swift test (runs preflight first)
make build       # release build of both halves
make app         # scripts/build-macos.sh → dist/DarwinPlay.app (ad-hoc codesigned)
make preflight   # diagnose the toolchain only
make fixtures    # build fixtures/d3d11-triangle with MinGW-w64 (optional)
make clean
```

Single tests:

```bash
cargo test --manifest-path runtime/Cargo.toml compatibility::tests::auto_backend_prefers_dxmt_for_d3d11
```

```bash
xcrun --sdk macosx swift test --package-path app --filter SteamModelsTests
```

macOS builds deliberately use `xcrun --sdk macosx swift`, never a bare `swift` from `PATH`. `scripts/preflight-macos.sh` rejects `/Library/Developer/CommandLineTools` and fails the build if full Xcode is not selected. XCTest availability is proven by the real SwiftPM test build, not by a standalone interpreter import.

Exercise the runtime directly while developing:

```bash
cargo run --manifest-path runtime/Cargo.toml -- steam status --json
```

Every read command has a `--json` flag; long-running ones stream JSONL.

## Runtime modules

- `wine.rs` — Wine discovery, Homebrew-managed install/reinstall/remove, readiness probing, process launch, `wineserver` lifecycle, Windows process listing
- `prefix.rs` — prefix creation and drive mappings
- `pe.rs` — PE32/PE32+ parsing: architecture, subsystem, imports, graphics API detection
- `graphics.rs` — DXMT component install/status, backend resolution, per-launch env (`WINEDLLOVERRIDES`, `DXMT_LOG_*`, `DXMT_SHADER_CACHE_PATH`), managed-DLL backup/restore inside the prefix
- `steam.rs` — Steam install from CDN, library discovery, single-instance lifecycle, `-applaunch` orchestration, UI policy version
- `compatibility.rs` — per-AppID executable scan, ranking, backend recommendation, profile persistence, launch configuration
- `vdf.rs` — minimal Valve KeyValues parser for `libraryfolders.vdf` / `appmanifest_*.acf`
- `cli.rs` / `main.rs` — clap command surface; `main.rs` is the only place that maps CLI enums onto domain enums and prints human-readable output

## Architecture invariants

These are load-bearing decisions, not style preferences. Violating them regresses fixed bugs.

**Policy lives in Rust.** Candidate scoring, backend resolution, and Wine package management belong in `compatibility.rs` / `graphics.rs` / `wine.rs`. Swift models mirror the serialized contract only.

**Generated compatibility facts are never persisted.** Executable discovery, PE imports, ranking and recommendations are recomputed on every request so a Steam update cannot leave stale generated state authoritative. Only user overrides are written, to `~/Library/Application Support/DarwinPlay/compatibility/steam/<AppID>.json`: schema version, backend override (`inherit` | `auto` | `dxmt` | `wined3d`), optional analysis target, launch arguments. A saved analysis target that disappears is ignored and ranking resumes.

**Backend precedence:** per-game override → global preference (`inherit`) → current executable analysis (`auto`). `auto` picks DXMT only for D3D10/11 imports *and* only when DXMT is installed. D3D9 / Vulkan / OpenGL take the Wine path. D3D12 is reported unsupported rather than routed through WineD3D. Unknown imports report unknown instead of guessing.

**No app-specific hardcoded game rules** until a declarative compatibility database exists with test coverage. Likely redistributables, launchers, tools and servers are classified and ranked *below* game executables, never filtered out — the analysis must stay inspectable.

**Scanning is bounded and deterministic.** `MAX_SCAN_DEPTH = 8`, `MAX_EXECUTABLES = 512`, symlinks are not followed. Launch arguments are bounded too (`MAX_LAUNCH_ARGUMENTS = 64`, 1024 chars each, 8192 total).

**Launch arguments are an array, never a shell string.** They are appended as individual process arguments after `steam.exe -applaunch <AppID>`. No shell interpretation anywhere.

**Steam owns launch, DRM, downloads and authentication.** DarwinPlay never accepts Steam credentials, Guard codes or tokens. A profile's selected executable is DarwinPlay's *analysis target*, not a replacement for Steam's configured launch entry.

**Filesystem isolation.** Prefix creation removes Wine's default `Z:` mapping. Imported games get only a `G:` mapping to their executable directory. The Steam installer is exposed through a temporary `I:` drive over the DarwinPlay downloads directory, which is removed after the installer returns; installed Steam is launched from its native `C:` path. The downloaded installer is validated as a PE before Wine runs it.

**One shared Steam prefix (`prefixes/steam`), per-AppID graphics.** Only one graphics environment can be live in that prefix at a time, so a launch stops the prefix, prepares the AppID's backend, then restarts Steam. DXMT logs and shader cache use a `steam-<AppID>` runtime id despite the shared prefix.

**Steam is single-instance.** `launch_game` checks `steam.exe` in the prefix first and dispatches into the running client instead of starting a second one. `STEAM_UI_POLICY_VERSION` in `steam.rs` records the UI flag policy of the current session (`STEAM_UI_ARGUMENTS` — currently `-cef-disable-gpu`, `-cef-disable-gpu-compositing`, `-cef-disable-occlusion`, `-opengl`, `-system-composer`); a running client from an older policy reports *UI restart required* rather than being accepted merely because `steam.exe` exists. Bump that constant whenever `STEAM_UI_ARGUMENTS` changes, and record the reasoning — these flags are the black-window/CEF compatibility surface, so a change with no rationale is unreviewable.

**Prefixes are created transactionally.** A prefix is initialized in staging and promoted only once `kernel32.dll`, `system.reg` and `user.reg` exist. Incomplete prefixes are discarded and recreated; a prefix marked initialized that later loses runtime files is reported corrupted, not silently reused.

`steam diagnostics [--json]` reads Steam's own `webhelper.txt` / `cef_log.txt` from inside the prefix and reports the WebHelper command line actually observed plus whether the CEF GPU switches took effect. Use it before changing flags — it shows what Steam did, not what DarwinPlay intended.

## IPC contract

Short operations write one JSON document to stdout. Long-running Wine operations emit JSON Lines (`RuntimeEvent` in `events.rs`, camelCase, `None` fields skipped). `RuntimeClient` drains stdout and stderr on separate tasks — this avoids pipe backpressure deadlocks with large compatibility payloads or chatty Wine diagnostics; keep it that way when adding commands.

`RuntimeClient` locates the binary via `DARWINPLAY_RUNTIME`, else a `darwinplay-runtime` sibling of the app executable. Adding a runtime command means: `cli.rs` variant → `main.rs` dispatch → `RuntimeClient` method → `Models.swift` decodable.

## State and env

`~/Library/Application Support/DarwinPlay/` (override with `DARWINPLAY_HOME`) holds `prefixes/`, `compatibility/`, `library.json`, `settings.json`, `activity.json`, downloads and the DXMT component. Swift stores are actors writing atomically; `LibraryStore` keeps a legacy ISO-8601 decoder fallback for old `library.json` files.

Other env: `DARWINPLAY_WINE` (Wine path, same as `--wine`), `DARWINPLAY_WINEDEBUG` (defaults to `-all`).

Favorites and last-played live in `activity.json`, independent of Steam manifests — Steam stays the source of truth for what is installed, DarwinPlay activity survives manifest refreshes.

## UI

Top-level destinations are Home, Games, Console (`ContentView.swift`). Initial Wine/Steam setup appears only on Home as game-like cards; after setup, runtime maintenance moves to Settings, and diagnostics stay in Console. `DesignSystem.swift` (`DarwinPalette`) is the authoritative palette: cold blue-gray surfaces with a muted green accent. `design-system/` is a generated web-oriented document (CSS variables, GSAP, purple palette) that does **not** describe this SwiftUI app — do not treat it as the design source of truth.

## Notes

- This working copy is not a git repository, so there is no history to consult and no commits to make.
- Docs to keep in sync with behavior changes: `docs/ARCHITECTURE.md`, `docs/COMPATIBILITY.md`, `docs/DEVELOPMENT.md`, `docs/SECURITY.md`, plus the versioned changelog sections at the end of `README.md`. The version lives in `runtime/Cargo.toml`.
