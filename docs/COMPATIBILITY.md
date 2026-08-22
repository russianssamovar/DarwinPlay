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

## Known game fixes

`gamefix.rs` is the declarative compatibility database. Every entry is data —
AppID, executable image name, DLL overrides, and the reason the fix exists —
and every entry is covered by tests; no game-specific behavior may live as
code anywhere else.

Fixes are applied on every launch, before Steam is asked to start the game:
each DLL override is written to
`HKCU\Software\Wine\AppDefaults\<exe>\DllOverrides` with `reg.exe` inside the
prefix. The write is idempotent, so nothing about applied fixes is persisted
and a recreated prefix heals itself on the next launch. Registry writes go
through Wine rather than editing `user.reg`, which would race a running
wineserver.

A fix failure is reported as a `game_fix_failed` event but does not block the
launch: without the fix the game is no worse off than before.

Applied fixes are visible in two places: the compatibility profile carries a
`gameFixes` array (and a human-readable line in `reasons`), and each launch
emits one `game_fix_applied` event per fix.

Current entries:

- **Dragon's Dogma 2** (2054970): `nvapi64` disabled for `DD2.exe`. D3DMetal
  ships `nvapi64.dll`, which sends NVIDIA Streamline down the NVAPI path; it
  then dereferences an unproxied `ID3D12CommandQueue` and crashes at launch.
