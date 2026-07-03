# AUDIT — Save / Load Subsystem (M45 + M45.1)

- **Date**: 2026-07-03
- **HEAD**: `8498e559`
- **Scope**: `crates/save/src/` (snapshot / registry / disk / validate / driver / lib)
  + the sole engine-side consumer `byroredux/src/save_io.rs` + cross-cut ground
  truth (`crates/core/src/ecs/world.rs`, `crates/core/src/string/mod.rs`,
  `crates/physics/src/sync.rs`, `byroredux/src/main.rs` run-loop ordering,
  `byroredux/src/cell_loader/transition.rs`, `byroredux/src/scene.rs`).
- **Depth**: deep (full capture → encode → disk → decode → reload → delta-apply
  flow + frame-boundary / off-frame drain ordering re-traced against the two
  prior reports).
- **Dedup baseline**: `gh issue list --state all` (300 issues, open + closed) +
  full read of both prior save reports
  (`docs/audits/AUDIT_SAVE_2026-06-23.md`, `docs/audits/AUDIT_SAVE_2026-07-02.md`).
  **Correction to the SKILL's Phase-1 premise**: the SKILL states "no prior
  save audit exists" — that is now stale; this is the **third** `/audit-save`
  run. All four MEDIUM findings from the 2026-07-02 report (SAVE-01…SAVE-04)
  were fixed the same day (`326fcb44`, `91b8c5df`, `dc89ff68`, `cec3b9ab`) and
  closed as `#1845`, `#1846`, `#1844`, `#1847`. One LOW finding (SAVE-05,
  `#1848`) remains open/unfixed. This audit re-verified every one of those
  fixes against the live code (not just the issue tracker) and swept for new
  regressions/gaps since.

## Executive Summary

The M45 crate and M45.1 live-load consumer remain **among the most
defensively engineered subsystems in the tree**, and the prior audit round's
findings were closed with real, well-tested fixes rather than issue-tracker
theater — every fix was re-verified directly against the code in this pass
(`registry.rs`'s `is_form_id` flag, `scene.rs`'s `PLAYER_FORM_ID_PAIR` stamp,
`driver.rs`/`save_io.rs`'s post-load `validate_world` calls, and
`apply_deltas`'s additive-only doc-comment). `cargo test -p byroredux-save`
(20+10 tests) and the binary's `save_io` test module (10 tests) all pass.

Verification results against the `crates/save/src/lib.rs` docstring's claimed
design (unchanged from the prior round — re-confirmed, not re-derived):

| Claimed design property | Verdict |
|---|---|
| Full ECS snapshot, size scales with loaded-cell count | CODE-CONFIRMED |
| Atomic disk write (tmp → fsync → read-back → rename → dir-fsync) | CODE-CONFIRMED |
| Slot ring never clobbers the last good save; survives restart | CODE-CONFIRMED |
| Header gates precede any JSON parse | CODE-CONFIRMED |
| Pre-save validation gate blocks the write | CODE-CONFIRMED |
| **Post-load validation is diagnostic on both restore paths** | CODE-CONFIRMED (new since 2026-07-02: `#1844`) |
| Load is off-frame, drained between ticks with `&mut World` | CODE-CONFIRMED |
| Capture is read-only + consistent (no torn frame) | CODE-CONFIRMED |
| `FixedString` symbol round-trip via `StringPool::dump`/`from_dump` | CODE-CONFIRMED |
| `next_entity` high-water restored before inserts | CODE-CONFIRMED |
| Stable `FormIdPair` saved, not session-local handle | CODE-CONFIRMED |
| **`form_id_column()` keyed off explicit flag, not heuristic** | CODE-CONFIRMED (new since 2026-07-02: `#1845`) |
| **Player body is remappable (carries a stable `FormIdPair`)** | CODE-CONFIRMED (new since 2026-07-02: `#1846`) |
| Two divergent restore paths; live never calls `restore_world` | CODE-CONFIRMED |
| Deterministic CRC at equal state (row order stable) | CODE-CONFIRMED |

This round's own contribution is **one new HIGH finding**: the Papyrus quest
runtime's two state resources (`QuestStageState` / `QuestObjectiveState` —
`Quest.SetStage`/`GetStageDone`/objective-completion state, the actual
quest-progression data a Fallout/Skyrim-lineage RPG lives on) carry **no
`Serialize`/`Deserialize` derive at all** and are **absent from
`build_save_registry`**. Every quest stage/objective the player advances is
silently wiped on every save→load cycle. This is the same class of bug as the
already-open `#1834` (`ActorValues`) / `#1835` (`PerkList`/`FactionRanks`),
found by a prior ECS audit, but is a distinct root cause (a different crate,
a `Resource` not a `Component`, and arguably higher-impact — quest
progression is core RPG state that already has live consumers wired
end-to-end, per `docs/audits/AUDIT_SCRIPTING_2026-07-02.md`).

### Findings by severity

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 0 |
| LOW | 0 (1 pre-existing, tracked as `#1848`) |

### Findings by Data-Loss Class

| Data-Loss Class | Findings |
|---|---|
| silent-drop | SAVE-07 (NEW) |
| corruption-on-load | — |
| irrecoverable-write | — |
| reference-break | — |
| none | — |

## Data-Loss Class Matrix

| Finding | Dimension | Severity | Data-Loss Class | Status |
|---|---|---|---|---|
| SAVE-07 | Snapshot Completeness & Determinism | HIGH | silent-drop | NEW |
| (SAVE-01…04, prior round) | various | was MEDIUM | various | Fixed (`#1844`–`#1847`) |
| (SAVE-05, prior round) | Frame-Boundary Capture | LOW | none | Existing: `#1848`, still open/unfixed |
| (SAVE-06, prior round) | Documentation | LOW | none | Fixed (feature-matrix already correct as of 2026-06-21) |
| ECS-2026-07-02-01 (`#1834`) | Snapshot Completeness | MEDIUM | silent-drop | Existing, still open — `ActorValues` unregistered |
| ECS-2026-07-02-02 (`#1835`) | Snapshot Completeness | LOW | silent-drop | Existing, still open — `PerkList`/`FactionRanks` unregistered |

## Completeness Ledger

`build_save_registry` (`byroredux/src/save_io.rs:157-187`) ×
`MUTABLE_DELTA_COLUMNS` (`save_io.rs:83-90`):

| Registered column | Overlaid? | Classification |
|---|---|---|
| `Transform` | yes | mutable game-state |
| `Inventory` | yes | mutable game-state |
| `EquipmentSlots` | yes | mutable game-state |
| `LightSource` | yes | mutable game-state |
| `LightFlicker` | yes | mutable game-state |
| `ScriptTimer` | yes | mutable game-state |
| `Name` | no | structural/identity — reloaded cell owns it (FixedString unsafe to overlay) |
| `Parent` | no | structural/identity — reloaded cell owns it |
| `Children` | no | structural/identity — reloaded cell owns it (EntityId list) |
| `FormIdComponent` | no (`is_form_id`) | the remap KEY itself, never a delta |
| `AnimationPlayer` | no | deliberately excluded (`#1696`): session-local `root_entity`/`clip_handle` |
| `AnimationStack` | no | deliberately excluded (`#1696`): layer `root_entity` session-local |
| `ItemInstancePool` (resource) | wholesale (`restore_resources`) | pool `ItemStack.instance` indexes — restored before deltas |
| `CurrentCellContext` (resource) | wholesale | cell identity driving which cell to reload |
| `PlayerPose` (resource) | wholesale → `apply_player_pose` | player standing pos + look angles |

**Ledger verdict, this round**: no NEW save-but-never-replay drift among the
six mutable columns — still pinned by
`delta_columns_carry_only_session_stable_fields`. The gap this round is
**entirely outside the ledger** — it's game-state that never entered the
registry at all: `QuestStageState` / `QuestObjectiveState` (this report,
SAVE-07), plus the still-open `ActorValues` (`#1834`) /
`PerkList`+`FactionRanks` (`#1835`) from the prior ECS audit.

## Findings

### SAVE-07: Quest progression (`QuestStageState`/`QuestObjectiveState`) is entirely absent from the save registry — every `SetStage`/objective change is lost on save→load
- **Severity**: HIGH
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop
- **Location**: `crates/scripting/src/quest_stages.rs:64-69` (`QuestStageState`,
  `impl Resource for QuestStageState {}`), `crates/scripting/src/quest_stages.rs:175-180`
  (`QuestObjectiveState`, `impl Resource for QuestObjectiveState {}`);
  `byroredux/src/save_io.rs:157-187` (`build_save_registry` — neither type
  appears)
- **Status**: NEW (not covered by `#1834`/`#1835`, which are about
  `ActorValues`/`PerkList`/`FactionRanks` in `crates/core`, a different crate
  and a different root cause — those are `Component`s missing a save-registry
  entry; this is a `Resource` pair missing both the `serde` derive *and* the
  registration)
- **Description**: `QuestStageState` is the runtime backing for Papyrus
  `Quest.SetStage()` / `GetStage()` / `GetStageDone()` — "one `QuestStageData`
  per quest the player has interacted with" (its own doc comment), i.e. the
  literal definition of quest-progression save data in a Bethesda-lineage
  RPG. `QuestObjectiveState` is its sibling for `SetObjectiveDisplayed` /
  `SetObjectiveCompleted` / `SetObjectiveFailed`. Both are installed as
  `World` resources (`byroredux/src/scene.rs:748,775`,
  `world.insert_resource(QuestStageState::default())`) and are live-wired:
  `docs/audits/AUDIT_SCRIPTING_2026-07-02.md` confirms
  `quest_advance_dispatch` → `quest_fragment_dispatch` run every frame in
  `Stage::Update`, mutating both resources through real Papyrus-recognizer
  call sites (`quest_advance`, `dlc2_ttr4a`, `mg07_door`, condition eval).
  This is not a stub or a future-work placeholder — it is exercised gameplay
  state today.
  Neither struct carries a `#[derive(Serialize, Deserialize)]` (confirmed:
  `grep -n derive crates/scripting/src/quest_stages.rs` shows only
  `Debug`/`Clone`/`Default`/`PartialEq`/`Eq`/`Hash` derives, never `Serialize`),
  which is a **prerequisite** for `SaveRegistry::register_resource::<R>()`
  (`R: Resource + Serialize + DeserializeOwned`). Consequently neither type
  could be registered even if `build_save_registry` tried, and it doesn't try:
  a full read of `build_save_registry` (`save_io.rs:157-187`) shows the
  registered resource set is exactly `ItemInstancePool`, `CurrentCellContext`,
  `PlayerPose` — no quest state.
- **Evidence**:
  ```rust
  // crates/scripting/src/quest_stages.rs
  #[derive(Debug, Default)]
  pub struct QuestStageState { quests: HashMap<QuestFormId, QuestStageData> }
  impl Resource for QuestStageState {}
  // ... no Serialize/Deserialize anywhere in the file
  ```
  ```rust
  // byroredux/src/save_io.rs:167-186 — build_save_registry, full resource list:
  .register_resource::<ItemInstancePool>("ItemInstancePool")
  .register_resource::<CurrentCellContext>("CurrentCellContext")
  .register_resource::<PlayerPose>("PlayerPose");
  // QuestStageState / QuestObjectiveState absent
  ```
- **Impact**: On every `save` → `load` cycle (both the live M45.1 overlay path
  and the loose/test `restore_world` path — this is a resource-level gap, so
  both paths are affected identically), every quest's `current_stage`,
  `stages_done` history, and every objective's displayed/completed/failed flag
  silently reverts to default (stage 0, no objectives). A player who saves
  mid-quest and reloads finds every quest reset to its starting stage with no
  error, warning, or validation-gate trip (this data isn't hierarchy/
  equipment/animation/item-instance shaped, so none of the four
  `validate_world` sub-checks or the binary's `validate_form_ids` would ever
  catch it — it's an *absence*, not an inconsistency). This is the single
  worst-case outcome the M45 format's docstring exists to prevent ("no
  baseline to drift against... removes the corruption tail by construction")
  — except here the state isn't corrupted, it's simply never captured.
  Quest state isn't yet exposed through a player-facing UI (per the module's
  own "What's deliberately NOT here yet" doc section, no quest-journal
  consumer exists today), so the practical blast radius is currently bounded
  to script-observable behavior (a script's `GetStageDone` check reverting)
  rather than a visible UI regression — but the underlying mechanism
  (Papyrus `SetStage`) is live and already wired to real recognizer-emitted
  scripts (`dlc2_ttr4a`, `mg07_door`), so a save/load taken mid-session
  through those paths loses real, currently-observable state today, not just
  latent future state.
- **Related**: Same silent-drop class as `#1834` (`ActorValues`) and `#1835`
  (`PerkList`/`FactionRanks`) — recommend filing alongside those with a
  cross-reference, since a future save-registry completeness pass should
  address all three (and add a registry-vs-`Resource`-impl completeness
  test so a fourth doesn't slip through the same way).
- **Suggested Fix**: Add `#[derive(serde::Serialize, serde::Deserialize)]`
  (feature-gated the same way every other save-participating type is, per
  the `inspect`/`save` feature chain Dimension 2 already verifies) to
  `QuestStageState`, `QuestStageData`, `QuestObjectiveState`, and
  `ObjectiveStatus`; register both resources in `build_save_registry`
  (`"QuestStageState"`, `"QuestObjectiveState"`); add a round-trip test
  mirroring `player_pose_survives_snapshot_round_trip`. Longer-term: a
  compile-time or test-time completeness check that walks every
  `impl Resource for X` in the tree and asserts it's either registered or an
  explicit documented exception (the same shape Dimension 1's checklist asks
  for component types) would catch this class structurally instead of via
  manual audit sweep.

## Regression Guards Discovered (unchanged from 2026-07-02, re-verified)

All previously-documented guards still pass (`cargo test -p byroredux-save`:
20+10 tests; `cargo test --bin byroredux save_io`: 10 tests; all green at
HEAD `8498e559`). New guards added since the last audit round:

| Test | File | Invariant pinned |
|---|---|---|
| `form_id_column_resolves_the_flagged_entry` | `crates/save/src/registry.rs:359` | `#1845` — `form_id_column()` keyed off `is_form_id`, not `apply.is_none()` |
| `form_id_column_is_none_without_registration` | `crates/save/src/registry.rs:371` | no form-id column registered ⇒ `None`, not a false match on any resource |
| `registering_a_second_form_id_column_panics` | `crates/save/src/registry.rs:383` | `#1845` — a second `register_form_id_component` call asserts loudly at registration time |
| `player_body_inventory_survives_live_load` | `crates/save/tests/round_trip.rs:317` | `#1846` — the reserved `PLAYER_FORM_ID_PAIR` makes the player body remappable; a saved player `Inventory` delta lands on the live (post-reload) player entity |
| `restore_world_does_not_abort_on_referentially_broken_snapshot` | `crates/save/tests/round_trip.rs:496` | `#1844` — post-load `validate_world` is diagnostic-only; a broken-but-decodable snapshot still restores (with a WARN), never aborts |

Guards carried over unchanged from the prior report (still passing, still
relevant): `full_world_round_trips_through_container`,
`delta_apply_reroutes_by_form_id_after_cell_reload`,
`anim_player_root_entity_not_clobbered_by_delta_apply`,
`form_id_restore_without_pool_errors_cleanly`,
`validation_catches_equipment_out_of_bounds` /
`_dangling_parent`, `dangling_item_instance_is_rejected` /
`item_instance_without_pool_is_rejected`, the full `snapshot.rs` header-gate
suite, the full `disk.rs` atomic-write/ring suite,
`delta_columns_carry_only_session_stable_fields`,
`serde_default_on_saved_struct_requires_format_major_bump`,
`binary_registry_round_trips_including_scripttimer`,
`player_pose_survives_snapshot_round_trip` /
`player_pose_round_trips_flycam` / `player_pose_character_tracks_body`,
`unresolvable_form_id_is_rejected` / `resolvable_form_id_passes` /
`fresh_world_validates_clean` / `save_then_load_command_queues_with_cell_context`.

**Coverage gap to add** (would surface SAVE-07 as a failing test): a
round-trip test that a `QuestStageState`/`QuestObjectiveState` mutation
(`SetStage` + `SetObjectiveCompleted`) survives save → load, mirroring
`player_pose_survives_snapshot_round_trip`.

## Prior-Round Findings — Verification Status

| Finding | Verdict this round |
|---|---|
| SAVE-01 (load never re-validates) | **Fixed**, confirmed live: `restore_world` (`driver.rs:105`) and `execute_pending_save_loads` (`save_io.rs:683-688`) both call `validate_world` + `validate_form_ids` post-load via `log_validation_warnings`, diagnostic-only as designed. |
| SAVE-02 (`form_id_column()` heuristic) | **Fixed**, confirmed live: explicit `is_form_id: bool` field on `Entry` (`registry.rs:59`), assert-guarded against a second registration (`registry.rs:207-212`). |
| SAVE-03 (player body unremappable) | **Fixed**, confirmed live: `PLAYER_FORM_ID_PAIR` reserved sentinel (`crates/core/src/form_id.rs:149-152`) attached at player-body spawn (`byroredux/src/scene.rs:711-726`), with a dedicated round-trip test. |
| SAVE-04 (additive-only overlay) | **Addressed via documentation**, confirmed: `apply_deltas`'s doc comment (`driver.rs:194-204`) now states the gap explicitly and names the exact companion-pass shape a future fix needs. Still a latent gap (no enable/disable/delete persistence exists today to trigger it) — this is the correct disposition for an inherently-deferred fix, not a re-finding. |
| SAVE-05 (second `load` overwrites pending slot) | **Still open**, `#1848`, no commit found touching `LoadCommand`/`PendingSaveLoadSlot` since. Not escalating — same LOW severity as before (cosmetic astonishment, no data loss: the on-disk saves are untouched, and only the *in-flight, not-yet-drained* request is discarded). |
| SAVE-06 (feature-matrix doc-rot) | **Already correct** as of 2026-06-21 per the SKILL text itself; re-confirmed `docs/feature-matrix.md:169` still carries the `TD3-002` removal note and no "unstarted" row exists. No action needed. |

## Next Step

Report ready. To file SAVE-07 as a GitHub issue:

```
/audit-publish docs/audits/AUDIT_SAVE_2026-07-03.md
```

Note for `/audit-publish`: `#1834` and `#1835` are pre-existing OPEN issues
from a different audit (`/audit-ecs`) — do not re-file them; SAVE-07 is the
only new issue this report should produce. Suggested labels for SAVE-07:
`bug`, `high` (no `save` label exists in the repo's label set; `scripting`
would be the closest domain label if one existed — none currently maps
cleanly, so `bug` + `high` alone is appropriate per the domain-label list in
`_audit-common.md`).
