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
| [`m-trees.sh`](m-trees.sh) | SpeedTree Phase 1.7 close-out | Pre-Skyrim TREE REFRs round-trip through the SpeedTree pipeline: TREE record → `.spt` parser → SPT importer → cell loader extension switch → `Billboard` ECS entity. FNV / FO3 exterior cells must spawn ≥ 1 / ≥ 5 billboard placeholders respectively. |

### Assertion shape

Each script splits checks into **hard** (script exits non-zero on
miss) and **soft** (logs `WARN`, no exit code change). The split
matches the audit-severity model: hard fails point at engine
regressions; soft warnings point at environment / archive-coverage
drift that doesn't indicate a code bug.

For `m41-equip.sh`:

| Check | Class | Threshold (FO4 / Skyrim) | Source |
|-------|-------|--------------------------|--------|
| `bench: entities=N` | hard | 5000 / 1200 | engine `bench:` summary line |
| `bench: draws=N` | hard | 4000 / 700 | engine `bench:` summary line |
| `entities Inventory` count | soft | > 0 | byro-dbg `(N entities)` line |
| `entities EquipmentSlots` count | soft | > 0 | byro-dbg `(N entities)` line |
| `tex.missing` unique count | soft | ≤ 20 / 30 | byro-dbg JSON header |

Thresholds are intentionally below observed values (the 2026-05-08
FO4 baseline saw 10809 entities / 8162 draws) so vanilla mod-load-
order drift doesn't trip false positives.

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
