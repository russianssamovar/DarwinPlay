# Development

Select full Xcode before command-line builds:

```bash
sudo xcode-select --switch "/Applications/Xcode.app/Contents/Developer"
sudo xcodebuild -runFirstLaunch
```

Run:

```bash
make preflight
make test
make app
```

## Runtime modules

- `wine.rs`: DarwinWine artifact validation, lifecycle and process execution
- `prefix.rs`: transactional prefix creation and runtime compatibility
- `pe.rs`: PE inspection
- `graphics.rs`: WineD3D/DXMT component lifecycle and launch environment
- `steam.rs`: Steam installer, library, session graphics and launch orchestration
- `compatibility.rs`: per-AppID analysis and user overrides
- `vdf.rs`: minimal Valve KeyValues parser

## DarwinWine integration

DarwinWine is a separate repository. Do not copy its source tree, build scripts or dependencies into DarwinPlay.

DarwinPlay accepts only a packaged DarwinWine runtime with schema-2 `runtime.json`. Keep this interface narrow. If DarwinWine changes its internal build layout without changing the declared entrypoints, DarwinPlay should not need to change.

The runtime CLI is:

```bash
cargo run --manifest-path runtime/Cargo.toml -- runtime status --json
cargo run --manifest-path runtime/Cargo.toml -- runtime install --archive /path/to/DarwinWine.tar.zst --json
cargo run --manifest-path runtime/Cargo.toml -- runtime remove --json
```

Do not reintroduce system Wine discovery, Homebrew Wine management, Sikarugir integration, runtime selection or custom Wine path overrides.

## DXMT development

DXMT stays independent from DarwinWine lifecycle:

```bash
cargo run --manifest-path runtime/Cargo.toml -- graphics dxmt install-latest --json
cargo run --manifest-path runtime/Cargo.toml -- graphics dxmt update --json
```

Do not mutate DarwinWine binaries to activate DXMT. Runtime and graphics component updates must remain independently removable.

## Steam development

Steam UI and game backend selection are distinct. Keep one Steam instance per shared prefix. Restart only when the requested graphics environment differs from the running session or when its recorded component version is stale.

Keep compatibility and runtime policy in Rust. Swift mirrors serialized state and should not duplicate lifecycle rules.
