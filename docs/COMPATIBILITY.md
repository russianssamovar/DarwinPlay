# Compatibility profiles

A profile must survive Steam updates without turning stale generated data into source of truth.

## Stored state

Each AppID may store only:

- backend override
- optional analysis target
- launch arguments

Backend override values are `inherit`, `auto`, `dxmt` and `wined3d`.

## Analysis

The scanner traverses the installed game directory deterministically without following symbolic links. Work is bounded by depth and executable count. PE files are inspected for architecture, subsystem and imported graphics libraries.

D3D10/11 candidates prefer DXMT when it is available. D3D9, Vulkan and OpenGL remain fallback paths. D3D12 stays visible but is not claimed as supported until a D3D12 translation backend is integrated.

## Steam UI backend

Steam UI is not represented as a game compatibility profile. It has a separate application setting because the client UI itself can require a different translation path from a launched game.

The running Steam session records the active backend. A game can reuse that session only when its effective backend matches; otherwise DarwinPlay performs a controlled backend switch by restarting the shared Steam prefix.

## Launch arguments

Arguments are stored as an array and passed directly after `steam.exe -applaunch <AppID>`. They are never interpreted by a shell.
