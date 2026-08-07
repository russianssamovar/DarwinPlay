# Development

On macOS, select full Xcode before using the command-line build:

```bash
sudo xcode-select --switch "/Applications/Xcode.app/Contents/Developer"
sudo xcodebuild -runFirstLaunch
```

Run the preflight explicitly when diagnosing a machine:

```bash
make preflight
```

It verifies the selected Xcode developer directory, macOS SDK, Swift/XCTest toolchain and Rust availability. DarwinPlay does not use an arbitrary `swift` from `PATH` for macOS builds.

Run available tests:

```bash
make test
```

Build the macOS app:

```bash
make app
```

Build the D3D11 fixture:

```bash
make fixtures
```

## Runtime modules

- `wine.rs`: Wine discovery, Homebrew management, readiness, execution and process lifecycle
- `prefix.rs`: prefix creation and filesystem mappings
- `pe.rs`: PE inspection
- `graphics.rs`: DXMT component state and graphics launch environment
- `steam.rs`: Steam install, library discovery and launch orchestration
- `compatibility.rs`: per-AppID analysis, ranking, persistence and launch configuration
- `vdf.rs`: minimal Valve KeyValues parser

Keep compatibility and runtime policy in Rust. Swift models mirror the serialized contract and should not reproduce candidate scoring, backend resolution or Wine package management.

Do not persist generated PE analysis. Do not follow symlinks while scanning installed games. Do not interpret launch arguments through a shell. Do not replace Steam content delivery or authentication. Do not add app-specific hardcoded rules until a declarative compatibility database has a concrete requirement and test coverage.
