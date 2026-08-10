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
| [`p0-door-interaction.sh`](p0-door-interaction.sh) | Playable slice P0 close-out | Loads Skyrim's Bannered Mare at a deterministic camera pose, selects the authored XTEL exit, observes the native `[E] Open` prompt, injects one physical `KeyE` pulse through `ActionBindings`/`ActionState`, and requires exactly one canonical `ActivateEvent` plus a completed deferred transition to `WhiterunWorld (6,-2)`. The 2026-08-10 close-out run passed with 5,183 source-cell entities. |
| [`m-exteriors.sh`](m-exteriors.sh) | Exterior readiness EX-01 / EX-05 / EX-06 / EX-07 / EX-13 | Cross-game matrix for FNV WastelandNV, FO3 MegatonWorld, Oblivion/Skyrim Tamriel, and FO4 Commonwealth. `static` mode gates populated exterior captures; `boundary` mode drives the deterministic three-cell `grid-cross` path and additionally requires every full-detail and LOD handoff to settle without supersession. Debug artifacts include `water.dump` and `water.contacts` so plane/material/volume coverage and solver contact can be correlated with each retained image; leaked `#INT_MIN#`/FLT_MAX no-water sentinels are hard failures. |
| `cargo run --release -p byroredux-scripting --example mq101_conformance` | MQ101 intro vertical-slice preflight | Production ESM/BSA/PEX paths recover the `MQ101` quest plus its typed `SCEN` timelines, aliases, phases, dialogue/package/timer actions, stage/scene-fragment bindings, attached properties, critical intro scripts, cart HKX files, and localized FUZ dialogue. Hard checks verify every scene actor, phase range, DIAL/PACK reference, SCEN-bound PEX asset, and construction of the live `SceneRegistry`/`ScenePlayer` shape. Also reports the exact share of bound quest fragments the current effect lowerer understands. This does not need Vulkan; it proves data ingress and orchestration-plan construction, not actor-alias spawning or dialogue/package execution. Pass a data-directory argument or set `BYROREDUX_SKYRIM_DATA` to override the default install path. |
| [`r6a_stale_15_bench.sh`](r6a_stale_15_bench.sh) | R6a-stale-15 bench-of-record refresh | Canonical three-cell benchmark suite: Prospector Saloon (FNV synthesized collision), Whiterun (Skyrim control), MedTek (FO4 precombined). Collects FPS / wall_ms / fence_ms / brd_ms / entities / draws / IsCollisionOnly counts. Formats output for ROADMAP.md copy-paste. Enforces CWD rule (run from each game's `Data/` directory). |
| [`m41-equip.sh`](m41-equip.sh) | M41 Phase 2 close-out | Skyrim+ / FO4 NPCs spawn with their default outfit (LVLI dispatch via OTFT walks resolves to base ARMO refs; `Inventory` + `EquipmentSlots` are populated; armor meshes load without `tex.missing` overflow). |
| [`m-trees.sh`](m-trees.sh) | SpeedTree Phase 1.7 close-out | Pre-Skyrim TREE REFRs round-trip through the SpeedTree pipeline: TREE record → `.spt` parser → SPT importer → cell loader extension switch → `Billboard` ECS entity. FNV / FO3 exterior cells must spawn ≥ 1 / ≥ 5 billboard placeholders respectively. |
| [`m47-triggers.sh`](m47-triggers.sh) | M47.2 compiled-script + trigger close-out | A Skyrim cell loads with `--scripts-bsa "Skyrim - Misc.bsa"`; the engine decompiles each scripted REFR's VMAD-named `.pex` through the recognizer chain and spawns invisible trigger volumes from `XPRM` primitives. Hard gate: the cell loaded (entity floor + bench summary). Soft: the `M47.2 scripts: N REFRs recognized, M trigger volumes spawned` summary line (content / load-order dependent). Point `BYROREDUX_TRIGGER_CELL` at a quest dungeon for trigger-volume coverage. The runtime crossing (enter → quest advance) is unit-tested, not driven here. |
| [`m43-quest-runtime.sh`](m43-quest-runtime.sh) | M43.1 quest observability | Loads a real Skyrim cell and drives `quest.show`, `quest.aliases`, `quest.start`, `quest.setstage`, and `quest.stop` through the embedded debug server. Hard gates cover production QUST installation, lifecycle transitions, alias diagnostics, final canonical state, and a populated-cell entity floor. Defaults to DA10 (`0x00022F08`, stage 37); override the quest/cell/stage for load-order-specific probes. |
| [`m34-day-night.sh`](m34-day-night.sh) | M34 day/night close-out | Loads a real Skyrim Tamriel exterior and drives `time.set`/`time.show` through sunrise, noon, and night. Hard gates pin climate phase selection, full/no-sun intensity, pause/resume rate retention, multi-day rollover, and a populated exterior entity floor. |
| `cargo test -p byroredux --test skinning_e2e -- --ignored` | M29 skinning close-out | FNV `NiSkinInstance` + SSE `BSSkinInstance` full import chain: bones populated, names round-trip `node_by_name`, partition-local → global bone-index remap correct, per-vertex `bone_indices`/`bone_weights` in bounds. Needs `BYROREDUX_FNV_DATA` + `BYROREDUX_SKYRIM_DATA`. |

### Assertion shape

Each script splits checks into **hard** (script exits non-zero on
miss) and **soft** (logs `WARN`, no exit code change). The split
matches the audit-severity model: hard fails point at engine
regressions; soft warnings point at environment / archive-coverage
drift that doesn't indicate a code bug.

For `m-exteriors.sh`, the entity/draw floors are calibrated below the
2026-08-04 five-game radius-1 baseline. PNG health additionally requires RGB
mean `(0.01, 0.98)` and standard deviation `> 0.005`.

Run the two modes explicitly:

```bash
docs/smoke-tests/m-exteriors.sh all static
docs/smoke-tests/m-exteriors.sh all boundary
```

Boundary mode defaults to 900 logical movement/capture frames and crosses three
complete exterior cells. The path pauses at each boundary until both
full-detail and LOD work settle, so a faster renderer cannot accidentally give
streaming less wall time; inserted hold frames are included in the reported
frame distribution and total bench frame count. The run emits bounded
telemetry for dispatch, unload, worker queue/parse, apply, LOD, and frame
p50/p95/max. A profile fails if it reports fewer than three crossings, a
superseded deadline, or an unsettled handoff. The smoke timeout remains the
hard deadlock/runaway bound; measured latency and frame tails are tracked as
EX-07 performance evidence rather than hidden by looser traversal pacing.

| Profile | Grid / WRLD | Hard floor entities / draws | Observed entities / draws | Image mean / stddev |
|---------|-------------|------------------------------|---------------------------|---------------------|
| FNV | `0,0` / `WastelandNV` | 2500 / 700 | 4367 / 1229 | 0.2587 / 0.0160 |
| FO3 | `-1,-7` / `MegatonWorld` | 2000 / 700 | 3201 / 1093 | 0.2725 / 0.0547 |
| Oblivion | `0,0` / `Tamriel` | 3500 / 1300 | 5709 / 2355 | 0.3658 / 0.1364 |
| Skyrim SE | `2,-4` / `Tamriel` | 3500 / 500 | 6160 / 947 | 0.2625 / 0.0250 |
| FO4 | `0,0` / `Commonwealth` | 30000 / 12000 | 57102 / 22706 | 0.3267 / 0.1737 |

Texture misses and failed NIF cache entries remain soft, but their exact lists
are retained in each profile's `debug.log` for classification.

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
| `BYROREDUX_FO3_DATA`        | `/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data`                            |
| `BYROREDUX_OBLIVION_DATA`   | `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data`                                  |
| `BYROREDUX_SKYRIM_DATA`     | `/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data`                    |
| `BYROREDUX_FO4_DATA`        | `/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data`                                 |
| `BYRO_DEBUG_PORT`           | `9876`                                                                                   |
| `BYROREDUX_SMOKE_FRAMES`    | `30` (static exterior and other smoke bench frames before hold)                           |
| `BYROREDUX_BOUNDARY_FRAMES` | `900` (logical movement/capture frames; boundary settle holds are inserted)               |
| `BYROREDUX_SMOKE_TIMEOUT`   | `240` seconds per profile (used by `m-exteriors.sh`)                                     |
| `BYROREDUX_EXTERIOR_ARTIFACT_DIR` | Fresh `/tmp/byro-exterior-smoke.*` directory retained after the run               |
| `BYROREDUX_TRIGGER_CELL`    | `WhiterunBanneredMare` (cell `m47-triggers.sh` loads; override with a quest dungeon for trigger-volume coverage) |
| `BYROREDUX_QUEST_CELL`      | `WhiterunBanneredMare` (cell loaded by `m43-quest-runtime.sh`)                              |
| `BYROREDUX_QUEST_FORM_ID`   | `0x00022F08` (DA10; quest exercised by the M43.1 runtime smoke)                            |
| `BYROREDUX_QUEST_STAGE`     | `37` (stage assigned by the M43.1 runtime smoke)                                           |
| `BYROREDUX_DAY_NIGHT_GRID`  | `2,-4` (Skyrim Tamriel exterior used by `m34-day-night.sh`)                                |
| `BYROREDUX_BENCH_FRAMES`    | `300` (bench frames — used by `r6a_stale_15_bench.sh`; override to e.g. `10` for validation) |
