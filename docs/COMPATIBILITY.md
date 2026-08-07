# Compatibility profiles

## Goals

A profile must survive normal Steam updates without becoming authoritative stale state. It must remain understandable from the UI and CLI, avoid game-specific hardcoded rules, and keep launch behavior inside Steam.

## Stored state

Each AppID can have one small JSON override file containing schema version, AppID, backend override, optional analysis target and launch arguments.

Backend values:

- `inherit`: use the global DarwinPlay backend preference
- `auto`: choose from the current executable analysis
- `dxmt`: require DXMT
- `wined3d`: force the WineD3D path

## Analysis

The scanner walks the installed game directory deterministically. Symbolic links are not followed. Work is bounded by depth and executable count. Each PE executable is inspected for architecture, subsystem and imported graphics libraries.

Candidate scoring prefers 64-bit GUI game executables and supported graphics APIs. Direct3D 11 and 10 receive the strongest preference because DXMT is the primary backend in this release. Vulkan, Direct3D 9 and OpenGL remain viable fallback paths. Direct3D 12 stays visible but receives a lower compatibility score because no D3D12 translation backend is bundled.

Likely redistributables, uninstallers, crash tools, editors, servers and launchers are not hidden. They are classified and ranked below normal game executables so the analysis remains inspectable rather than relying on irreversible filtering.

## Launch arguments

Arguments are stored as an array, not as a shell command. The CLI accepts repeated `--launch-argument` values and the Swift UI uses one argument per line. Values are length-limited and passed directly after `steam.exe -applaunch <AppID>`.

## Shared Steam prefix

Profiles are independent while the Windows Steam installation remains shared. Only one graphics environment can be active inside that Steam prefix at a time. DarwinPlay therefore stops the existing Steam prefix before a game launch, prepares that AppID's backend, then starts Steam and the game with the new environment.

DXMT logs and shader cache use an AppID-specific runtime identifier even though the prefix is shared.
