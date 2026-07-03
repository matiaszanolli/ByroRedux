# #1862: SAVE-07: QuestStageState/QuestObjectiveState absent from save registry — quest progress silently wiped on save/load

**Severity**: HIGH
**Location**: `crates/scripting/src/quest_stages.rs`, `byroredux/src/save_io.rs`

`QuestStageState` / `QuestObjectiveState` — the live Papyrus `Quest.SetStage()`/
`GetStage()`/`GetStageDone()`/objective-completion runtime — carried no
`Serialize`/`Deserialize` derive and were absent from `build_save_registry`.
Both are live-wired `World` resources mutated every frame by real
recognizer-emitted scripts (`quest_advance`, `dlc2_ttr4a`, `mg07_door`), so
every quest's stage/objective progress silently reverted to default on every
save→load cycle.

## Fix
Added `#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]`
to `QuestFormId`, `QuestStageState`, `QuestStageData`, `QuestObjectiveState`,
and `ObjectiveStatus` — the same feature-gated pattern already used by
`ScriptTimer` in the same crate. Registered both resources in
`build_save_registry` (`"QuestStageState"`, `"QuestObjectiveState"`). No
bespoke restore logic was needed (unlike `PlayerPose`/`CurrentCellContext`):
`byroredux_save::restore_resources` / `restore_world` walk every registered
resource generically, so registration alone wires both the live M45.1 overlay
load and the full `restore_world` path.

Verified empirically that `serde_json` round-trips a `HashMap<QuestFormId, _>`
correctly (the newtype-u32 key stringifies/destringifies transparently) before
implementing, since the struct's own doc comment ("the save system sorts by
FormID before writing") hinted at a bespoke serialization path that turned out
to be unnecessary.

## Completeness Checks
- [x] **TESTS**: Added `quest_stage_and_objective_state_survive_snapshot_round_trip`
      mirroring `player_pose_survives_snapshot_round_trip`, exercising both the
      full `restore_world` path and the live-overlay `restore_resources` path.
      Also added `quest_stages.rs` to the `SAVE_TYPE_SOURCES` list so the
      SAVE-D2-01 `#[serde(default)]` guard scans it.
- [x] **SIBLING**: Confirmed #1834 (`ActorValues`) and #1835 (`PerkList`/
      `FactionRanks`) are separately-filed issues with a different root cause
      (Component-registry gaps, not Resource-derive gaps) — out of scope for
      this fix, not silently duplicated.

---

# #1863: RT-1: Oblivion runtime baseline bench_draws_gpu_calls is stale (3 vs live 4)

**Severity**: LOW (baseline housekeeping, not a code bug)
**Location**: `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv`

Four independent sweeps (06-23, 06-26, 07-02, 07-03) read `bench_draws_gpu_calls
= 4` on this cell; the baseline (created 2026-06-14) still recorded `3`.

## Verification
Reproduced the live bench per `audit-runtime` skill's exact procedure: built
release binaries, launched `xvfb-run -a ./target/release/byroredux --game
oblivion --cell ICMarketDistrictTheGildedCarafe --bench-frames 240
--bench-hold`, polled `byro-dbg` for readiness, then read the authoritative
`bench:` summary line from the engine log (NOT the `stats` console command's
live snapshot, which reads a different in-flight frame and is not the gating
metric per the skill's own warning):
```
bench: ... entities=701 meshes=156 textures=159 draws=324/27b/4c
```
`draws=324/27b/4c` → `bench_draws_cmds=324` (unchanged), `bench_draws_batches=27`
(improved from 30), `bench_draws_gpu_calls=4` (confirmed stale at 3) — exactly
matching the issue's cited live values. Every other metric (entities, tex,
mesh-cache, skin pool) was unchanged from the existing baseline.

## Fix
Regenerated the baseline TSV with the current, verified values (`--regen`
equivalent — this fixes real drift, not a code bug).

## Completeness Checks
- [x] **TESTS**: N/A — baseline TSV housekeeping.
- Left #1833 (the identical FNV `skin_pool_max` staleness pattern) untouched —
  a separately-filed issue, not requested in this pass.
