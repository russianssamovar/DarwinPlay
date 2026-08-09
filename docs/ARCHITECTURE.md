# Architecture

```text
SwiftUI
  |
  | JSON + JSONL
  v
Rust runtime
  |-- DarwinWine runtime lifecycle
  |-- transactional Wine prefixes
  |-- Steam integration
  |-- PE inspection
  |-- compatibility analysis
  `-- graphics component management
        |-- WineD3D
        `-- DXMT -> Metal
```

## DarwinWine boundary

DarwinPlay supports exactly one Wine runtime family: DarwinWine.

The DarwinWine repository owns Wine source selection, patching, build dependencies, dylib bundling, networking/TLS validation, Steam validation, packaging and release artifacts. DarwinPlay owns only installation and use of the resulting runtime artifact.

DarwinPlay never:

- discovers system Wine;
- installs Homebrew/WineHQ Wine;
- invokes Sikarugir;
- accepts an arbitrary Wine executable override;
- switches between Wine engines;
- vendors DarwinWine source code.

A DarwinWine installation is transactional: validate archive paths, extract to staging, validate `runtime.json`, validate declared entrypoints and version, create a disposable prefix, then atomically replace the active runtime.

## Prefixes

Imported executables receive one prefix per game. Steam uses the shared `steam` prefix. Prefix creation is transactional and promoted only after required Wine files and registry state exist.

Each prefix records the Wine version used to initialize it. Runtime compatibility is checked against the single active DarwinWine runtime. A change in Wine major version requires an explicit reset.

The default `Z:` mapping is removed. Imported games receive only explicit mappings needed for launch. Steam setup receives a temporary installer-directory mapping.

## Graphics boundary

Graphics translation is independent from runtime selection because runtime selection does not exist.

WineD3D is the baseline path. DXMT is an application-managed, versioned graphics component. Per-game profiles may choose WineD3D or DXMT without replacing DarwinWine.

## Steam UI and games

Steam uses one shared prefix, but Steam UI graphics and game compatibility are separate decisions. Only one graphics environment can be active in the shared Steam process tree. When the requested game backend differs from the active Steam session backend, DarwinPlay performs a controlled restart.

## Persisted compatibility state

Generated executable analysis is recomputed. Only explicit user choices are persisted: backend override, optional analysis target, and launch arguments.
