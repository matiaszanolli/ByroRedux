# AUDIT — Save / Load Subsystem (M45 + M45.1)

- **Date**: 2026-07-02
- **Scope**: `crates/save/src/` (snapshot / registry / disk / validate / driver / lib)
  + the sole engine-side consumer `byroredux/src/save_io.rs` + cross-cut ground
  truth (`crates/core/src/ecs/world.rs`, `crates/core/src/string/mod.rs`,
  `crates/physics/src/sync.rs`, `byroredux/src/main.rs` run-loop ordering,
  `byroredux/src/cell_loader/transition.rs`).
- **Depth**: deep (full capture → encode → disk → decode → reload → delta-apply
  flow + frame-boundary / off-frame drain ordering traced).
- **Dedup baseline**: `gh issue list` (200 issues) — **no** existing OPEN or
  CLOSED issue mentions save / load / snapshot / FormId-save. No prior
  `AUDIT_SAVE_*` report. All findings below are NEW.

## Executive Summary

The M45 crate and M45.1 live-load consumer are **among the most defensively
engineered subsystems in the tree**. Every design claim in the
`crates/save/src/lib.rs` docstring is CODE-CONFIRMED, and a large share of the
audit checklist was pre-empted by prior in-code hardening (traceable via the
`SAVE-D1-01 … SAVE-D6-02` markers and `#1696` / `#1714`). Verification results
against the docstring's promised design:

| Claimed design property | Verdict | Evidence |
|---|---|---|
| Full ECS snapshot, size scales with loaded-cell count not playtime | CODE-CONFIRMED | `save_world` walks the registry, omits empty columns (`driver.rs:38-53`) |
| Atomic disk write (tmp → fsync → read-back → rename → dir-fsync) | CODE-CONFIRMED | `write_slot` (`disk.rs:34-69`); read-back is byte-exact (`readback != bytes`), dir fsync present (SAVE-D3-01) |
| Slot ring never clobbers the last good save; survives restart | CODE-CONFIRMED | `SaveRing::resume` + `cursor_after_newest` (`disk.rs:96-160`) — the session-reset clobber the SKILL predicts is already fixed (SAVE-D3-02) |
| Header gates precede any JSON parse | CODE-CONFIRMED | `decode` order: len → magic → major → schema_fpr → payload_len bounds (`checked_add`) → CRC → `from_slice` (`snapshot.rs:120-166`) |
| Pre-save validation gate blocks the write | CODE-CONFIRMED | `SaveCommand::execute` aborts before `save_world` on non-empty `validate_world` + `validate_form_ids` (`save_io.rs:384-398`) |
| Load is off-frame, drained between ticks with `&mut World` | CODE-CONFIRMED | `execute_pending_save_loads` drains in `step_save_loads` post-scheduler (`main.rs:2362`); command layer only decodes + queues |
| Capture is read-only + consistent (no torn frame) | CODE-CONFIRMED | `SaveCommand` runs through the **Late-stage exclusive** `DebugDrainSystem` (`debug-server/src/system.rs:1-3`); no system holds a storage write lock during the drain |
| `FixedString` symbol round-trip via `StringPool::dump`/`from_dump` | CODE-CONFIRMED | symbol-indexed dump with gap panic (`string/mod.rs:102-131`); re-intern is idempotent + lowercased-canonical |
| `next_entity` high-water restored before inserts | CODE-CONFIRMED | `restore_world` sets it before `load` (`driver.rs:82-84`); guard is `entity < next_entity` |
| Stable `FormIdPair` saved, not session-local handle | CODE-CONFIRMED | `register_form_id_component` resolves through pool, re-interns on load, fails cleanly without a pool (`registry.rs:183-239`, SAVE-D2-02) |
| Two divergent restore paths; live never calls `restore_world` | CODE-CONFIRMED | `restore_world` is only reached from tests (`save_io.rs:698/759`, `round_trip.rs`); the live drain calls only `restore_resources` + `apply_deltas` |
| Deterministic CRC at equal state (row order stable) | CODE-CONFIRMED | both save closures `sort_by_key(entity)` (`registry.rs:92`, `registry.rs:206`, SAVE-D1-01) |

**No CRITICAL or HIGH findings.** The 6 findings are all MEDIUM / LOW and cluster
into two roots: (a) the live overlay is **additive-only and form-id-keyed**, so
any mutable state that is *not* form-id-addressable, or that represents a
*removal*, has no live-load replay path; (b) a couple of latent
maintenance/hardening traps. The `docs/feature-matrix.md` "M45 (unstarted)"
doc-rot the SKILL instructs me to report is **already fixed** — see SAVE-06.

### Findings by severity

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 3 |
| LOW | 3 |

### Findings by Data-Loss Class

| Data-Loss Class | Findings |
|---|---|
| silent-drop | SAVE-03 (latent) |
| corruption-on-load | — |
| irrecoverable-write | — |
| reference-break | SAVE-02 (latent), SAVE-04 (latent) |
| none (hardening / DiD / doc) | SAVE-01, SAVE-05, SAVE-06 |

## Data-Loss Class Matrix

| Finding | Dimension | Severity | Data-Loss Class | Live today? |
|---|---|---|---|---|
| SAVE-01 | Validation Gates | MEDIUM | none (defense-in-depth) | yes (a hand-edited/old save loads unvalidated) |
| SAVE-02 | Registry & (De)serialization | MEDIUM | reference-break | latent (future `apply:None` component) |
| SAVE-03 | Snapshot Completeness / Live-Apply | MEDIUM | silent-drop | latent (player has no Inventory today) |
| SAVE-04 | M45.1 Live Load-Apply | LOW | reference-break | latent (no delete/disable mechanism today) |
| SAVE-05 | Frame-Boundary / Off-Frame Apply | LOW | none | yes (rare: two `load`s same frame) |
| SAVE-06 | Doc-rot | LOW | none | n/a (already fixed) |

## Completeness Ledger

`build_save_registry` (`save_io.rs:155-186`) × `MUTABLE_DELTA_COLUMNS`
(`save_io.rs:81-88`). The two lists are the twin sources of truth; a
registered-mutable column absent from the overlay set would be *saved to disk yet
never replayed on a live load*.

| Registered column | Overlaid? | Classification |
|---|---|---|
| `Transform` | ✓ overlaid | mutable game-state |
| `Inventory` | ✓ overlaid | mutable game-state |
| `EquipmentSlots` | ✓ overlaid | mutable game-state |
| `LightSource` | ✓ overlaid | mutable game-state |
| `LightFlicker` | ✓ overlaid | mutable game-state |
| `ScriptTimer` | ✓ overlaid | mutable game-state |
| `Name` | ✗ | structural/identity — reloaded cell owns it (FixedString unsafe to overlay) |
| `Parent` | ✗ | structural/identity — reloaded cell owns it |
| `Children` | ✗ | structural/identity — reloaded cell owns it (EntityId list) |
| `FormIdComponent` | ✗ (`apply:None`) | the remap KEY itself, never a delta |
| `AnimationPlayer` | ✗ | deliberately excluded (#1696): `root_entity: Option<EntityId>` + `clip_handle` are session-local |
| `AnimationStack` | ✗ | deliberately excluded (#1696): layer `root_entity` session-local |
| `ItemInstancePool` (resource) | wholesale (`restore_resources`) | pool that `ItemStack.instance` indexes — restored before deltas |
| `CurrentCellContext` (resource) | wholesale | cell identity that drives which cell to reload |
| `PlayerPose` (resource) | wholesale → `apply_player_pose` | player standing pos + look angles |

**Ledger verdict**: NO save-but-never-replay drift among the six mutable columns
— every mutable column appears in both lists, pinned by the
`delta_columns_carry_only_session_stable_fields` tripwire test (`save_io.rs:715`).
Every non-overlaid registered column has a documented structural/identity/session
reason. All six overlaid columns were spot-verified free of `FixedString` /
`EntityId` / session-handle fields (`light.rs`, `inventory.rs`, `timer.rs`,
`packed.rs` Transform). The delta apply's field-safety invariant holds.

The gap is not in the ledger's *columns* — it is that the ledger is **entirely
form-id-keyed**, which SAVE-03 / SAVE-04 address.

## Findings

### SAVE-01: Load path performs no referential-integrity re-validation
- **Severity**: MEDIUM
- **Dimension**: Validation Gates
- **Data-Loss Class**: none (defense-in-depth)
- **Location**: `crates/save/src/driver.rs:77-118` (`restore_world` /
  `restore_resources` / `apply_deltas`); `byroredux/src/save_io.rs:566-689`
  (`execute_pending_save_loads`)
- **Status**: NEW
- **Description**: `validate_world` + `validate_form_ids` run only on the SAVE
  path (`SaveCommand::execute`). `decode` validates the *container*
  (magic/version/schema/CRC) but the load drivers never re-run the referential
  gate on the decoded data. A save written by an OLDER engine (before a given
  validation rule existed), or a file hand-edited to keep a valid CRC, loads a
  referentially broken world unchecked — re-introducing the very slow-corruption
  tail the format's thesis exists to prevent. The thesis is symmetric (persist no
  inconsistent state ⇒ *ingest* no inconsistent state); only half is enforced.
- **Evidence**: `execute_pending_save_loads` goes
  `restore_resources` → `build_form_id_remap` → `apply_deltas` → `apply_player_pose`
  with no `validate_world` call anywhere on the drain. `restore_world`
  (`driver.rs:77`) likewise clears + repopulates with no post-load check.
- **Impact**: A corrupt-but-CRC-valid save (older-engine save, or manual edit)
  loads a broken world silently. Blast radius bounded to the loaded cell; not a
  write-side corruption, so no compounding tail on disk — but the in-memory world
  is inconsistent with no diagnostic.
- **Related**: The SKILL's Dim-4 "validation runs on SAVE only" checklist item.
- **Suggested Fix**: After `apply_deltas` (live) and after `restore_world`
  (loose), run `validate_world` and log the issues at WARN (do not abort — a load
  can't fall back to the previous world cleanly, but a diagnostic is the minimum).

### SAVE-02: `form_id_column()` heuristic mis-keys the entire remap if any future component registers with `apply: None`
- **Severity**: MEDIUM
- **Dimension**: Registry & (De)serialization Fidelity
- **Data-Loss Class**: reference-break (latent)
- **Location**: `crates/save/src/registry.rs:289-295`
- **Status**: NEW
- **Description**: `form_id_column()` identifies the remap key column as *"the one
  component with `apply.is_none()`"*. Today only `register_form_id_component` sets
  `apply: None` on a component, so the heuristic is correct. But it is
  **structural coincidence, not an assertion**: if any future `register_component`
  variant (or a second special column) ships with `apply: None`,
  `.find(|e| e.apply.is_none())` returns whichever comes first in registration
  order — potentially the WRONG column — and the entire live-load remap is built
  from non-form-id data → every delta drops or lands on the wrong entity.
- **Evidence**:
  ```rust
  self.components.iter().find(|e| e.apply.is_none()).map(|e| e.name)
  ```
  There is no `is_form_id` flag on `Entry`; the discriminator is the absence of a
  closure.
- **Impact**: Latent. Silent mass reference-break on the live-load path the day a
  second `apply: None` component is registered — with no compile-time or test
  guard (the `round_trip` tests register exactly one form-id column).
- **Related**: SAVE-03 (both concern the form-id remap being the single keying
  mechanism).
- **Suggested Fix**: Add an explicit `is_form_id: bool` (or a dedicated
  `form_id: Option<Entry>` slot) to `SaveRegistry` and key `form_id_column()` off
  it; assert at most one is set. Removes the fragile "no apply ⇒ it's the key"
  inference.

### SAVE-03: No live-load replay path for player-body-owned mutable state (player body carries no form-id key)
- **Severity**: MEDIUM
- **Dimension**: Snapshot Completeness & Determinism / M45.1 Live Load-Apply
- **Data-Loss Class**: silent-drop (latent)
- **Location**: `byroredux/src/scene.rs:677-711` (player-body spawn — no
  `FormIdComponent`, no `Inventory`); `crates/save/src/driver.rs:132-167`
  (`build_form_id_remap`)
- **Status**: NEW
- **Description**: The live-load overlay is **exclusively form-id-keyed**:
  `build_form_id_remap` matches saved→live entities by `FormIdPair`, and
  `apply_deltas` `filter_map`s out any saved row whose id isn't in that map. The
  player character body (`scene.rs:677`) is spawned with `Transform` /
  `GlobalTransform` / `CollisionShape` / `RigidBodyData` but **no
  `FormIdComponent`** — so it is absent from the remap by construction. Player
  *pose* is rescued out-of-band by `PlayerPose` + `apply_player_pose`, but any
  other mutable component that lands on the player body has **no live-load replay
  path**: it is captured to disk (if registered) yet silently dropped on load
  because its saved id can't be remapped. NPCs are unaffected — they DO get a
  `FormIdComponent` at cell spawn (`cell_loader/spawn.rs:263`) and an `Inventory`
  (`npc_spawn.rs:424`), so their deltas remap correctly.
- **Evidence**: `grep` confirms `Inventory::new()` is attached only in
  `npc_spawn.rs` (lines 424, 1120), never to `PlayerEntity`. The player body has
  no form id. `apply_deltas` → `ApplyFn` drops non-remapped rows
  (`registry.rs:120-124`).
- **Impact**: Latent today (the player owns no persistable mutable component
  besides `Transform`, which is pose-restored separately). The day a player
  inventory / equipment / actor-value system lands and attaches those components
  to the player body, **the player's inventory and equipment changes are silently
  lost on every live `load`** — the single worst data-loss class for a save
  system, arriving invisibly. (A full `restore_world` loose-mode load would
  preserve them via saved ids, but the LIVE overlay path — the one players use —
  cannot.)
- **Related**: SAVE-02 (form-id keying is the single mechanism).
- **Suggested Fix**: Before the player system grows persistable state, give the
  player body a stable identity the remap can key on (a reserved sentinel
  `FormIdPair`, or a dedicated `PlayerTag` remap entry that
  `build_form_id_remap` seeds `saved-player-id → live-player-id` from
  `PlayerEntity`). Add a regression test that a player-body `Inventory` survives a
  live load.

### SAVE-04: Live overlay is additive-only — a removed/disabled object reappears on live-load
- **Severity**: LOW
- **Dimension**: M45.1 Live Load-Apply
- **Data-Loss Class**: reference-break (latent)
- **Location**: `crates/save/src/driver.rs:182-199` (`apply_deltas`);
  `crates/save/src/registry.rs:111-128` (`ApplyFn`)
- **Status**: NEW
- **Description**: `apply_deltas` / `ApplyFn` only *insert* remapped rows onto the
  freshly reloaded cell; there is no removal/despawn form. The reloaded cell
  respawns every authored REFR from the ESM. If, during the saved session, the
  player deleted / disabled / picked-up a world object, the reload respawns it and
  the overlay has no way to re-remove it → the object reappears after a live load.
  (`restore_world`'s clear-then-repopulate loose path doesn't have this issue, but
  it isn't the live path.)
- **Evidence**: No `Disabled`/`Deleted`/`Enabled` marker component exists in
  `crates/core/src/ecs/components/` (grep empty); `apply_deltas` has no
  `remove`/`despawn` (grep of `driver.rs`/`registry.rs` finds only doc-comment
  mentions).
- **Impact**: Latent — the engine currently has no enable/disable/delete
  persistence mechanism to save, so nothing regresses today. Becomes a real
  reference-break the moment object enable-state or a "deleted refs" set is
  persisted.
- **Related**: SAVE-03 (both are consequences of the additive form-id overlay
  model).
- **Suggested Fix**: When object enable-state lands, persist a per-cell
  disabled/deleted form-id set and have the drain apply it (despawn / hide the
  matching reloaded entities) after `apply_deltas`.

### SAVE-05: A second `load` before the drain silently discards the first queued snapshot
- **Severity**: LOW
- **Dimension**: Frame-Boundary Capture & Off-Frame Apply
- **Location**: `byroredux/src/save_io.rs:145` (`PendingSaveLoadSlot(pub Option<Snapshot>)`),
  `save_io.rs:546-552` (`LoadCommand` overwrites `pending.0`)
- **Status**: NEW
- **Description**: `PendingSaveLoadSlot` is a single `Option`. If two `load`
  commands are issued in the same frame (before `step_save_loads` drains), the
  second overwrites the first with no warning. Idempotency of a single load is
  correct (the drain `.take()`s and teardown is unconditional), but the
  drop-the-earlier-request behaviour is silent.
- **Evidence**: `LoadCommand::execute` does `pending.0 = Some(snapshot)`
  unconditionally; no check for an already-populated slot.
- **Impact**: Cosmetic / astonishment only — the *last* `load` wins, which is
  arguably the intent, but the discarded request is invisible. No data loss (the
  on-disk saves are untouched).
- **Suggested Fix**: Log at INFO when overwriting a non-empty pending slot ("load
  slot N superseded by slot M before drain").

### SAVE-06: SKILL's flagged feature-matrix doc-rot is already fixed (stale audit instruction)
- **Severity**: LOW
- **Dimension**: Documentation
- **Location**: `docs/feature-matrix.md:169`
- **Status**: NEW (reporting the SKILL premise as stale)
- **Description**: The `audit-save` SKILL instructs reporting
  `docs/feature-matrix.md` line ~176 "Save / load (M45) | … | M45 (unstarted)" as
  a LOW doc-rot finding. That row was **already removed** on 2026-06-21 — line 169
  now carries the removal note: *"TD3-002: Save/load (M45/M45.1) removed — shipped
  2026-06-21."* There is no live "M45 (unstarted)" claim in the matrix. The
  actionable doc-rot is now in the SKILL itself (`.claude/commands/audit-save/SKILL.md`
  lines 82-85 and 94), which still describes the fixed row as a KNOWN finding.
- **Evidence**: `grep -n "M45\|unstarted" docs/feature-matrix.md` → only the
  removal note at line 169; no "unstarted" row.
- **Impact**: None on the engine. Future save audits will chase a phantom.
- **Suggested Fix**: Update the `audit-save` SKILL to drop the "Doc-rot to flag"
  block (Phase-1 step 5 and the Scope note), since the matrix row is gone.

## Regression Guards Discovered

The subsystem is unusually well pinned. A future change that breaks any of the
following trips an existing test:

| Test | File | Invariant pinned |
|---|---|---|
| `full_world_round_trips_through_container` | `crates/save/tests/round_trip.rs:93` | sparse ids + hierarchy + inventory + equip + stable form id round-trip through encode→decode→restore |
| `delta_apply_reroutes_by_form_id_after_cell_reload` | `round_trip.rs:229` | M45.1 form-id-keyed delta re-routing (saved id ≠ live id) |
| `anim_player_root_entity_not_clobbered_by_delta_apply` | `round_trip.rs:313` | #1696 — proves overlaying `AnimationPlayer` leaks a stale `root_entity`, and excluding it is the fix |
| `form_id_restore_without_pool_errors_cleanly` | `round_trip.rs:400` | SAVE-D2-02 — missing `FormIdPool` on restore errors, never panics |
| `validation_catches_equipment_out_of_bounds` / `_dangling_parent` | `round_trip.rs:187/203` | equipment-occupant bounds + dangling-parent gate |
| `dangling_item_instance_is_rejected` / `item_instance_without_pool_is_rejected` | `validate.rs:293/313` | SAVE-D4-01 — dangling `ItemInstanceId` gate |
| `round_trips_through_container` + `rejects_bad_magic` / `_truncated` / `_payload_truncation` / `detects_crc_corruption` / `rejects_schema_mismatch` / `rejects_major_version_skew` | `snapshot.rs:183-252` | every container header gate + gate ordering |
| `cursor_after_newest_points_past_latest_mtime` / `resume_on_empty_dir_starts_at_zero` / `parse_slot_names` / `write_read_round_trip_and_atomic_rename` / `ring_wraps` / `ring_size_floored_to_one` | `disk.rs:183-254` | SAVE-D3-02 resume policy, strict slot-filename parse, atomic write, ring wrap/floor |
| `delta_columns_carry_only_session_stable_fields` | `save_io.rs:715` | tripwire: `MUTABLE_DELTA_COLUMNS` frozen against an audited set — any addition forces the FixedString/EntityId/handle review |
| `serde_default_on_saved_struct_requires_format_major_bump` | `save_io.rs:1034` | SAVE-D2-01 / #1714 — static scan bans `#[serde(default)]` on any save-participating struct without a `FORMAT_MAJOR` bump |
| `binary_registry_round_trips_including_scripttimer` / `player_pose_survives_snapshot_round_trip` / `player_pose_round_trips_flycam` / `player_pose_character_tracks_body` | `save_io.rs:741-965` | binary registry + `PlayerPose` capture/restore in both camera modes; Rapier no-op-without-handle |
| `unresolvable_form_id_is_rejected` / `resolvable_form_id_passes` / `fresh_world_validates_clean` / `save_then_load_command_queues_with_cell_context` | `save_io.rs:774-879` | binary-side FormId-pool gate + end-to-end command plumbing |

**Coverage gaps to add** (LOW, not separately filed): a live-load test that a
player-body-attached `Inventory` survives (would surface SAVE-03 as a failing
test); a `validate_world`-after-load assertion (SAVE-01); a two-`load`-same-frame
supersede test (SAVE-05).

## Next Step

Report ready. To file the findings as GitHub issues:

```
/audit-publish docs/audits/AUDIT_SAVE_2026-07-02.md
```
