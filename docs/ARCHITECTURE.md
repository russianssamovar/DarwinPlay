# Architecture

```text
SwiftUI
  |
  | JSON + JSONL
  v
Rust runtime
  |-- PE inspection
  |-- Wine discovery and Homebrew management
  |-- Wine execution
  |-- Prefix management
  |-- Graphics management
  |-- Steam integration
  `-- Compatibility analysis
          |
          v
   Windows Steam client
          |
          v
      Windows game
```

## Prefixes

Manually imported executables receive one prefix per game. Steam uses one shared prefix named `steam`. Game-specific compatibility state is not stored by mutating the prefix permanently. Before each Steam game launch, the prefix is stopped, the effective graphics backend is prepared, and Steam is restarted so the launched child processes inherit a deterministic environment.

## Compatibility facts and overrides

Executable discovery, PE imports, ranking and backend recommendations are generated from the current installation. Generated facts are recomputed and are not written to disk.

Only explicit user choices are persisted:

```text
backend override
analysis target
launch arguments
```

This keeps game updates from invalidating a cached generated model. A saved analysis target that disappears is ignored and automatic ranking resumes.

## Backend precedence

```text
per-game override
    |
    | inherit
    v
global application preference
    |
    | auto
    v
current executable analysis
```

`auto` resolves DXMT only for D3D10/11 candidates and only when DXMT is installed. Other APIs fall back to the Wine path. D3D12 is reported as unsupported by this release instead of being presented as a supported WineD3D route.

## Steam launch

DarwinPlay launches `steam.exe -applaunch <AppID>` and appends saved launch arguments as individual process arguments. Steam owns launch configuration, DRM, Steamworks, downloads, updates and authentication. The optional executable selection in a DarwinPlay profile selects what DarwinPlay analyzes; it does not replace Steam's configured executable.

## IPC

Short operations return one JSON document. Long-running Wine processes emit JSON Lines events. The Swift client drains stdout and stderr concurrently to avoid pipe backpressure when compatibility payloads or Wine diagnostics are large.

## Library activity

Favorites and last-played timestamps are local launcher state stored independently from Steam manifests. Steam remains the source of truth for installed games, while DarwinPlay activity survives Steam manifest refreshes. Generated compatibility facts remain ephemeral and load lazily for visible game cards.

## UI shell

Home, Games and Console are top-level destinations. Initial Wine and Steam installation is presented only on Home. After setup, runtime maintenance moves to Settings. This keeps install actions from competing with normal play actions and keeps diagnostic output in the dedicated Console surface.
