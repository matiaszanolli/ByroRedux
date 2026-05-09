# Smoke tests

Manual / scripted smoke checks that need a real Vulkan device + game data
on disk — the kind that don't fit `cargo test` because they require a
windowed engine instance and out-of-tree BSA / ESM files.

Each script targets a specific milestone close-out gate. They're
opt-in (run by hand or in a future CI lane that has a GPU runner +
the relevant game-data archives mounted) and self-skip when their
data prerequisites aren't present.

## Procedure shape

All smoke tests follow the same workflow:

1. Spawn the engine in the background under `--bench-frames N --bench-hold`
   so the bench summary lands and the embedded TCP debug server (port
   9876 by default) stays reachable after the bench window closes.
2. Wait for the `bench-hold:` notice in the engine's stderr (signals
   the engine is held open, attach window).
3. Pipe a command sequence into `byro-dbg` (it reads stdin
   line-by-line and exits on EOF):
   ```
   echo -e 'entities\nfind Inventory\ntex.missing\nquit' \
     | cargo run --release -p byro-dbg
   ```
4. Assert on the captured output and SIGTERM the engine.

Both the `--bench-hold` flag and the debug-server's component
registry are the load-bearing infrastructure — pre-`73adffb` (`bench-
hold`) the engine exited too quickly for `byro-dbg` to attach, and
pre-this-patch the equip components weren't registered so `find
Inventory` returned nothing.

## Tests

| Script | Milestone | Verifies |
|--------|-----------|----------|
| [`m41-equip.sh`](m41-equip.sh) | M41 Phase 2 close-out | Skyrim+ / FO4 NPCs spawn with their default outfit (LVLI dispatch via OTFT walks resolves to base ARMO refs; `Inventory` + `EquipmentSlots` are populated; armor meshes load without `tex.missing` overflow). |

## Environment

Each script reads game-data paths from environment variables and
falls back to the canonical Steam install paths:

| Variable                    | Default                                                                                  |
|-----------------------------|------------------------------------------------------------------------------------------|
| `BYROREDUX_FNV_DATA`        | `/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data`                         |
| `BYROREDUX_SKYRIM_DATA`     | `/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data`                    |
| `BYROREDUX_FO4_DATA`        | `/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data`                                 |
| `BYRO_DEBUG_PORT`           | `9876`                                                                                   |
| `BYROREDUX_SMOKE_FRAMES`    | `30` (bench frames before the hold kicks in)                                             |
