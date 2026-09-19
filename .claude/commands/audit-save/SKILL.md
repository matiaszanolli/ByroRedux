---
description: "Deep audit of the M45 save/load subsystem — full-ECS-snapshot capture, type-erased registry, format-version discipline, atomic disk write + ring, save-side refusal/validation gates, and the M45.1 live load-apply (cell reload + FormId-keyed deltas + player-pose restore)"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Save / Load Subsystem Audit (M45 + M45.1)

Audit `crates/save` (full-ECS-snapshot format) and its sole live caller `byroredux/src/save_io.rs`
(+ `save_io/*_tests.rs`) for **data loss and save corruption**. The subsystem exists to remove Bethesda's
slow-corruption tail by making the live ECS the single source of truth; verify the CODE delivers that.
A silently dropped column, a stale schema guard, a torn capture, or a botched FormId remap **loses player
progress** — frame as CRITICAL/HIGH per `_audit-severity.md` (data loss is CRITICAL).

**Architecture**: Orchestrator; each dimension is a Task agent (max 3 concurrent). Read
`.claude/commands/_audit-common.md` (layout, methodology, dedup, finding format) and
`.claude/commands/_audit-severity.md` first; do not duplicate them here.

## Scope

**Crate** `crates/save/src/` (~2k LOC — read all): `lib.rs` (design intent docstring, `SaveError`),
`snapshot.rs` (`Snapshot`, container header, `FORMAT_MAJOR` doc = the bump ledger, `encode`/`decode`),
`registry.rs` (`SaveRegistry`; `register_component` / `register_replacing_component` / `register_resource`
/ `register_form_id_component`; `ValidateFn`; `schema_fingerprint`), `driver.rs` (`save_world`,
`restore_world`, `validate_snapshot_types`, `restore_resources`, `restore_resources_subset`,
`build_form_id_remap`, `apply_deltas`), `disk.rs` (`write_slot`, `slots_by_recency`, `SaveRing`),
`validate.rs` (`validate_world` + 7 sub-checks). Tests: `crates/save/tests/round_trip.rs`.

**Engine side** (`byroredux/src/save_io.rs`): `build_save_registry` (the curated type set),
`MUTABLE_DELTA_COLUMNS` (**second** hardcoded list driving the live overlay), `PRE_RELOAD_RESOURCES`,
`SaveCommand`/`SaveInfoCommand`/`LoadCommand`, `PendingPlayerSaveActions` (F5/F9/pause-menu ingress),
`quickload_latest`, `execute_pending_save_loads` (+ `reload_interior_session`/`reload_exterior_session`),
`capture_player_pose`/`apply_player_pose`, `validate_form_ids`, `validate_cinematic_entity_refs`.
Cross-cut: `byroredux/src/boot/registries.rs` (installs registry/state), `app_events.rs` (per-frame
order: `capture_player_pose` → `step_player_save_actions` → `step_save_loads`), `app_step.rs`,
`byroredux/src/notifications.rs` (`PlayerNotifications`: bounded, transient, unsaved; drained in
`app_frame.rs::render_one_frame`), `byroredux/src/extensions/` (SDK extension-state capture/preflight/
restore layered around the save/reload — the extension *API* is `/audit-tooling`),
`byroredux/src/cell_loader/reference_state.rs` (`PersistentReferenceStates`, `without_parked_state`),
`cell_loader/{transition,spawn}.rs`, `crates/core/src/{atomic_file.rs,string/mod.rs,ecs/world.rs}`,
`crates/physics/src/sync.rs`. Companion doc: `docs/engine/save-load-roundtrip.md` (does not yet name the
extension preflight/restore steps — #4145).

**Ownership split**: this skill owns schema, atomicity, validation, and load-apply *mechanics*. What loot /
inventory / consumable state *should* persist (gameplay semantics) → `/audit-gameplay`; reconcile-after-
overlay code lives in gameplay files but its **existence per removable column** is Dim 1's.

## Guard ledger (E2) — run these first; aim dimensions at what they cannot see

`cargo test -p byroredux-save` and `cargo test -p byroredux --bin byroredux save_io` (plus
`app_step` / `boot::schedule` sources named below). Only the four real-master tests in
`save_io/consumable_tests.rs` are `#[ignore]`d. Confirm each guard is live: not `#[ignore]`d, its scan
finds >0 items, and it fails when the invariant is broken.

| Guard | Pins |
|---|---|
| `registry_completeness_tests::every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` | every `Component`/`Resource` impl in any `crates/*/src` (roots **discovered**, not listed) + `byroredux/src` is registered XOR in `NOT_SAVED_BY_DESIGN` with a reason; `PlayerNotifications`, `PickedUp` etc. are classified there |
| `serde_default_guard_tests::serde_default_on_saved_struct_requires_format_major_bump` | any `#[serde(default)]` on a save-participating type (skip-only fields exempt) |
| `serde_default_guard_tests::saved_type_shape_changes_require_format_major_bump` | hash of every `#[derive(..Serialize..)]` struct/enum body in the discovered save-source files vs `BASELINE_SHAPE_FINGERPRINT`/`BASELINE_MAJOR` |
| `serde_default_guard_tests::set_in_chargen_renames_still_decode_v23_keys` | `serde(alias)` rename path stays decodable |
| `round_trip_tests::delta_columns_carry_only_session_stable_fields` | no `FixedString`/`EntityId`/session handle in a `MUTABLE_DELTA_COLUMNS` type |
| `round_trip_tests::delta_columns_removed_at_runtime_have_a_load_reconciler` | every delta column with a production removal site has a declared reconciler or "NoReconcilerNeeded" reason |
| `round_trip_tests::npc_spawn_stamped_components_are_saved_or_intentionally_rederived` | spawn-stamped components registered or in `REDERIVED_NOT_SAVED` |
| `crates/save` `snapshot.rs`/`disk.rs`/`registry.rs` unit tests | header gates (`rejects_bad_magic`/`_truncated`/`_payload_truncation`/`detects_crc_corruption`/`rejects_schema_mismatch`/`rejects_major_version_skew`), atomic write, slot parsing, ring resume, recency tie-break, `form_id_column_resolves_the_flagged_entry`, replacing-column semantics |
| `tests/round_trip.rs::typed_snapshot_preflight_rejects_bad_column_without_world_mutation` | typed preflight precedes `clear_entities` |
| `live_reload_tests::{saved_resources_are_restored_before_the_cell_reload, pre_reload_restore_must_not_install_saved_item_instance_pool_early}` | pre-reload resource subset, #4135 |
| `command_queue_tests::{a_save_taken_mid_cell_transition_is_refused_not_written, a_save_taken_while_chargen_disables_saving_is_refused_not_written, quicksave_ring_cursor_does_not_advance_on_validation_abort, player_save_actions_wait_for_the_quiescent_fifo_drain, quickload_empty_errors_and_corrupt_newest_falls_back}` | save-side refusal gates, ring, quiescent drain |
| `app_step.rs::the_save_drain_publishes_the_transition_flag_before_draining` | `CellTransitionInFlight` published before the drain |
| `*_survives_save_load_round_trip` (`round_trip_tests.rs`, `consumable_tests.rs`) | per-type round trips incl. lock/enable ledgers, fragment/provider queues, cinematic trio, `Perks`/timed restorations |

Not covered by any guard (the audit's real work): the **two-list drift** between registry and
`MUTABLE_DELTA_COLUMNS`; staleness of `NOT_SAVED_BY_DESIGN` *reasons*; whether a baseline refresh
without a bump was *justified*; manual `impl Serialize` types (invisible to both serde guards);
the extension-state payload's own versioning (Dim 2); semantic correctness of load-apply ordering beyond
the two pinned pre-reload cases.

## Parameters / Extra Fields

`--focus <dims>` (default all 5) · `--depth shallow|deep` (`shallow` = container/API contracts; `deep` =
trace capture → encode → disk → decode → reload → delta-apply). Finding fields: **Dimension**: Snapshot
Completeness | Format & Schema Discipline | Container & Disk | Save-Side Gates | Live Load-Apply ·
**Data-Loss Class**: silent-drop | corruption-on-load | irrecoverable-write | reference-break | none
(required for any finding that can lose progress).

## Phase 1: Setup

`mkdir -p /tmp/audit/save`; dedup: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/save/issues.json`;
read the newest `docs/audits/AUDIT_SAVE_*.md` (fixed findings are regression checks — verify the fix is
in place); read the `crates/save/src/lib.rs` docstring and `snapshot.rs` container doc — each design claim
must be CODE-CONFIRMED or DRIFTED; run the guard ledger. Delta-first: for each dimension
`git log --since=<last report> --format='%h %cs %s' -- <Paths>`, skim unchanged ones.

## Phase 2: Dimensions

### Dimension 1: Snapshot Completeness & the Two Lists (highest risk — silent-drop)
Paths: `byroredux/src/save_io.rs` (`build_save_registry`, `MUTABLE_DELTA_COLUMNS`, `PRE_RELOAD_RESOURCES`), `save_io/registry_completeness_tests.rs`, `crates/save/src/{registry,driver}.rs`
First step: `git log -p -S'register_' --since=<last report> -- byroredux/src/save_io.rs`; then `git log --since=<last report> --format='%h %cs %s' -- 'crates/*/src' byroredux/src | grep -i 'component\|resource\|state'` to find new persistent state.
- **The registry IS the completeness contract**; start from the guard's `NOT_SAVED_BY_DESIGN` list, not from a
  hand enumeration. A persistent, player-mutable component/resource that is neither registered nor
  allowlisted is a HIGH silent-drop; a *stale allowlist reason* (the guard checks a reason exists, not that
  it is true) is the auditable residue — spot-check a sample every run, and re-check any reason that names a
  "no production mutator" claim after new gameplay commits land (P3 loot/consumables/pickups, perks, timed
  restorations, hardcore mode, persistent reference state were all new saved state; `PlayerNotifications`
  and `PickedUp` are correctly transient — the durable half of a pickup is the `PersistentReferenceStates`
  tombstone).
- **Removed-from-allowlist stays registered**: `CharacterController` (fractional breath/drowning carry),
  `RigidBodyData`, `Material`, `RumbleOnActivate`, `FragmentExecutionQueue`, the cinematic trio — verify none
  regressed out of `build_save_registry`, and no re-added `NOT_SAVED_BY_DESIGN` entry re-excludes them.
- **Two lists, one truth**: the live load overlays only columns in BOTH the registry and
  `MUTABLE_DELTA_COLUMNS`. A registered mutable column absent from the overlay is captured but never
  replayed → HIGH silent-drop, unless deliberately excluded and documented at its `register_component`
  site: `Material` (blast radius), `AnimationPlayer`/`AnimationStack` (stale `root_entity`/`clip_handle`),
  `FollowState`/`EscortState`/`Seated`/cinematic pair (`EntityId`, registry handles), `ActorVitals`
  (write-once FormID key). A column with an `EntityId`/handle must also be covered by
  `validate_saved_entity_references` (Dim 4).
- **Replacing columns**: `Perks`, `TimedRestorations` use `register_replacing_component` — saved absence is
  authoritative for FormID-matched entities (empty column kept in the snapshot; a *missing* column is not a
  tombstone). Ordinary columns are additive-only, so **every runtime removal of an overlaid component needs a
  reconciler** (`reconcile_dead_actor_runtime_state`, `reconcile_player_equipped_weapon`); the guard enforces
  disposition, this dimension judges whether "NoReconcilerNeeded" reasons are true (carrier destroyed by
  the cell reload?) and whether a new persisted fact (disable, pickup, lock) chose the marker-plus-
  reconciler or FormID-keyed-ledger model (`ReferenceEnableState`, `ReferenceLockState`,
  `PersistentReferenceStates` — resources keyed by FormID survive cell unload; a component would not).
- **Determinism** (the `Snapshot` doc claims reproducible CRCs at equal state): `Snapshot` maps are
  `BTreeMap` and component rows are sorted by entity id (`registry.rs`), but saved **resources** serialize
  as-is — at 2026-09-19 `Globals(HashMap<u32, f32>)`, `QuestStageState`, `ReferenceEnableState` (`HashSet`),
  `ReferenceLockState` and `PersistentReferenceStates::pair_rows` all emit hash-iteration order, so two saves
  of equal state can differ in bytes/CRC. Re-verify; if still true it is a MEDIUM doc/contract mismatch
  (not data loss) unless something diffs or hashes saves — narrow the doc claim to component columns or
  sort at the resource boundary. Do not claim determinism at the row level without checking this.
- **`next_entity`** is saved verbatim and replayed via `set_next_entity` before inserts; `insert_batch`'s
  `entity < next_entity` is a `debug_assert` only (release inserts at an unspawned id silently — MEDIUM);
  `StringPool::dump`/`from_dump` preserves symbol order (a reordered dump = every `Name` wrong = CRITICAL).
**Output**: `/tmp/audit/save/dim_1.md`

### Dimension 2: Format & Schema Discipline (registry fidelity + `FORMAT_MAJOR`)
Paths: `crates/save/src/{snapshot,registry}.rs`, `save_io/serde_default_guard_tests.rs`, `crates/save/Cargo.toml`, `crates/core/Cargo.toml`
First step: `cargo test -p byroredux --bin byroredux serde_default_guard` ; `git log --since=<last report> --format='%h %cs %s' -- crates/save/src/snapshot.rs`
- **Bump rule** (read the `FORMAT_MAJOR` doc comment in `snapshot.rs`, 25 as of 2026-09-19 — do not
  hardcode the number elsewhere): intra-type shape changes need a bump because `schema_fingerprint` hashes
  only column keys (+ replacing policy). A new required field, retyped field, or new `Option` in a saved
  type bumps; `#[serde(default)]` is forbidden as a compatibility mechanism (guard) — even where the default
  would be correct for every old save. Read-compatible changes move the shape baseline **without** a bump:
  adding an enum variant to an externally-tagged `serde_json` enum (keyed by variant *name*, never ordinal —
  #4140 corrected the old "discriminant shift" rationale), removing a field (no `deny_unknown_fields` on any
  saved type), renaming with `serde(alias)`. Changing an existing variant's field shape is not compatible.
  Adding/removing a registered column changes the fingerprint and rejects old saves via `SchemaMismatch`
  with no bump. An audit proposing to relax the blanket rule for a "safe" default reopens #1714.
- **Baseline refresh triage** (the guard cannot judge this): every `BASELINE_SHAPE_FINGERPRINT` change must
  carry a justification comment stating whether a registered column's shape changed. Known false-positive
  classes: (a) the guard hashes *every* serialized-derive type in the discovered files, including types on
  `NOT_SAVED_BY_DESIGN` (e.g. `AnimatedTextureFlip`, `WaterMaterial`) — those move the hash with no snapshot
  impact; (b) a **tuple struct** sweeps the following `impl` block into its span (`VisibilityMask`) — method
  edits move the hash; (c) the hash includes the file-relative path, so moving a save-participating type
  between files moves it. Verify baseline and `FORMAT_MAJOR` were updated in the same commit, and that a
  refresh-without-bump was not hiding a real field change to a registered type.
- **Guard blind spots**: manual `impl Serialize` (no derive) types; a new `Option<T>` field is hashed but
  looks like a routine baseline bump — confirm it got the bump; `serde(alias)` must never coexist with a
  dropped value semantic change.
- **Second payload — extension state** (`Snapshot.resources["ByroExtensionState"]`, written by
  `extensions::capture_extension_state`, type `ExtensionStateSnapshot` in `crates/sdk/src/component.rs`):
  it is outside the registry, the completeness guard, and both serde guards (sdk types carry a plain
  `derive`, so `save_type_sources()` drops them), and follows its **own** policy — a `format_version` range
  (`MIN_EXTENSION_STATE_FORMAT_VERSION..=EXTENSION_STATE_FORMAT_VERSION`) with `#[serde(default)]` fields, which
  the blanket rule forbids for registry types. Verify: the range gate runs in `preflight_extension_state`
  (before teardown); every shape change to its rows bumps `EXTENSION_STATE_FORMAT_VERSION` (nothing enforces
  it — no shape baseline); rows without stable authored identity refuse the save rather than dropping;
  `decode_saved_state` treats a missing key as empty but a present-and-malformed one as an abort.
- **`ValidateFn` parity**: every `register_*` variant builds a `validate` closure that decodes the SAME type
  `load` does (`Vec<(u32, T)>` for columns, bare `T` for resources) — a missing/wrong one exempts a column
  from the preflight so it fails only after teardown.
- **Feature chain**: `crates/save` → `byroredux-core` `features=["save"]` (→ `inspect`) must still hold, or a
  non-default build compiles away serde impls and columns serialize to `null`.
- **FormId handle vs pair**: `register_form_id_component` saves the stable `FormIdPair` (skips unresolvable
  handles with WARN, never panics); load re-interns to fresh handles and errors cleanly without a pool.
  `form_id_column()` is keyed by the explicit `is_form_id` flag, never "first `apply: None` entry".
- `FnvHasher` uses canonical FNV-1a 64 constants and hashes only names/order/policy (no TypeId/address).
**Output**: `/tmp/audit/save/dim_2.md`

### Dimension 3: Container & Disk Durability
Paths: `crates/save/src/{snapshot,disk}.rs`, `crates/core/src/atomic_file.rs`, `crates/save/tests/round_trip.rs`
First step: `cargo test -p byroredux-save disk:: snapshot::`
Guarded (see ledger): header gate order (length → magic → major → schema fingerprint → `payload_len`
checked bounds → CRC over the payload only → `serde_json::from_slice`), advisory `minor`, slot-name strictness
(`save_42.ess.tmp` rejected), `slots_by_recency` newest-first with slot-number tie-break, ring wrap/resume.
Residual checks:
- **`atomic_write` (shared with `settings_io`)** order is exactly: create tmp → `write_all` → `flush` →
  `sync_all` → byte-exact read-back (mismatch deletes the tmp and errors) → `rename` → parent-directory
  fsync. Rename-before-fsync or a length-only read-back is a HIGH durability hole. `write_slot` only adds
  `create_dir_all`; both callers must still use the one helper.
- **Ring never clobbers the last good save**: `SaveState::new` must build the ring via `SaveRing::resume`
  (not `new`), and the cursor advances only after a committed write (`quicksave_ring_cursor_does_not_advance_on_validation_abort`).
- **Minor-version + default-fill**: `decode` accepts newer minors and serde default-fills; that is the exact
  path `#[serde(default)]` would exploit — cross-check Dim 2's guard is live.
- `SAVE_DIR_ENV` (`BYROREDUX_SAVE_DIR`) redirects the ring; smoke gates depend on the spelling.
**Output**: `/tmp/audit/save/dim_3.md`

### Dimension 4: Save-Side Gates (refusal + validation)
Paths: `byroredux/src/save_io.rs` (`SaveCommand::execute`, `validate_form_ids`, `validate_cinematic_entity_refs`), `crates/save/src/validate.rs`, `byroredux/src/app_step.rs` (`step_player_save_actions`)
First step: `grep -n 'fn validate_\|validate_[a-z_]*(world' crates/save/src/validate.rs byroredux/src/save_io.rs`
- **Order in `SaveCommand::execute`**: refusals first (`CellTransitionInFlight` — a mid-transition save would be
  written and permanently unloadable; `CinematicPresentationState.disable_saving` — `Game.SetInChargen`
  `abDisableSaving`), then `validate_world` + `validate_form_ids` + `validate_cinematic_entity_refs`
  (abort, up to 20 lines, nothing written), then `save_world` → `capture_extension_state` → `encode` →
  `write_slot`; ring advance and the `SaveComplete` session event only after the commit. Any alternate save
  path that bypasses the gates (console, player action, SDK) is HIGH; input adapters must enqueue through
  `queue_player_save_action` (post-scheduler drain — `save_world` takes ~30 storage + resource read locks, so
  a live scheduler lane needs the ABBA analysis re-derived; `/audit-concurrency`).
- **Coverage**: `validate_world` currently runs hierarchy, equipment, saved-entity references (session-local
  `EntityId` fields of columns excluded from the overlay — grep the function for the live column list),
  animation (`AnimationPlayer`, `AnimationStack` root + layer clips, `Seated.animation_restore`),
  inventory instances vs `ItemInstancePool`, progression (`CharacterLevel.xp != 0` aborts because
  `CharacterLevel` is unsaved; the player has no `CharacterLevel` — if a leveling runtime lands, register
  it), and `Material` finiteness; the binary adds FormId resolvability and cinematic `EntityId` refs. Do
  not restate the list elsewhere — enumerate **inter-entity reference fields in any newly registered type
  not covered** (MEDIUM defense-in-depth gap) and any check that flags a legitimately sparse-but-spawned id
  (dangling = `>= next_entity`, not "no components").
- **Post-load validation is diagnostic-only** (`log_validation_warnings`, WARN, no abort) in both
  `restore_world` and `execute_pending_save_loads`; the **typed preflight aborts** (before teardown).
- **Transient state**: saves must not depend on transients — `PlayerNotifications`, pending action queues,
  `CellTransitionInFlight`, parked `StreamStateSnapshots` are unsaved by design; a save taken while one is
  non-empty must still round-trip (check new transient resources land on the allowlist, not the registry).
**Output**: `/tmp/audit/save/dim_4.md`

### Dimension 5: Live Load-Apply & Frame Boundary
Paths: `byroredux/src/save_io.rs` (`execute_pending_save_loads`, `reload_*_session`, `apply_player_pose`), `crates/save/src/driver.rs`, `byroredux/src/{app_events,app_step,app_frame}.rs`, `cell_loader/{transition,reference_state}.rs`
First step: read `execute_pending_save_loads` top to bottom against the sequence below; `git log --since=<last report> --format='%h %cs %s' -- byroredux/src/save_io.rs`
- **Strict apply sequence**: drain slot → `validate_snapshot_types` (abort, session kept) →
  `preflight_extension_state` (abort) → `restore_resources_subset(PRE_RELOAD_RESOURCES)` (only resources
  read by spawn-time code during the reload: `ReferenceEnableState`, `ReferenceLockState`; **never**
  `ItemInstancePool` — teardown's `release_victim_item_instances` needs the live pool, #4135) →
  `without_parked_state` wraps the reload (outgoing session's `PersistentReferenceStates` and
  `StreamStateSnapshots` are set aside and restored on failure) → teardown + reload (`validate_cell_loadable`
  preflight first, so a missing ESM/cell keeps the live session) → `restore_extension_state` → wholesale
  `restore_resources` **again** (idempotent; re-asserts `CurrentCellContext`/`PlayerPose`; the second call is
  not redundant) → `drop_entity_bound_continuations` on `PapyrusProviderContinuationQueue` (session-local
  `EntityRef` handles must not resume, #4139) → `build_form_id_remap` → `apply_deltas(MUTABLE_DELTA_COLUMNS)`
  → dead/equipped-weapon reconcilers → diagnostic `validate_world` → `apply_player_pose` LAST (after the
  overlay of `CharacterController`, whose motion fields pose-restore then zeroes; a new field on either
  side needs an explicit decision). Any failure after the reload returns immediately, never falling through
  into pose-restore on a partial overlay. Idempotency: teardown is unconditional, so a second load of the same
  slot does not stack deltas.
- **Two restore paths**: live load uses `restore_resources` + `apply_deltas` (saved ids remapped to the
  reloaded cell's fresh ids); `restore_world` (clear + repopulate at saved ids) is test/loose only and must
  never be reachable from `execute_pending_save_loads` (id collision = CRITICAL). `clear_entities` does not
  free GPU/physics handles — the live path's `unload_current_interior`/`drain_streaming_state` must.
- **Remap**: `build_form_id_remap` matches saved `FormIdPair` → live entity; rows without a form id are
  skipped (respawned by the loader); unresolved pairs are logged (WARN, first 20) — a moved object vanishing
  must stay diagnosable; the player body carries `PLAYER_FORM_ID_PAIR` from spawn so it participates.
  Persistent-reference rows: resident rows arrive via the component overlay, nonresident via the resource
  restore; rows are consumed on respawn (semantics → `/audit-gameplay`).
- **Frame boundary**: capture runs post-scheduler (`step_player_save_actions` drains a FIFO after
  `Scheduler::run`; remote console runs in the exclusive `DebugDrainSystem`); `capture_player_pose` reads
  post-propagation `Transform` before the drain; `PlayerNotifications` drain in `render_one_frame` is
  unconditional (bounded ring of 8, oldest dropped). The load drain lives in `step_save_loads` where the
  App owns `&mut World` + `&mut VulkanContext` and is skipped while an interior transition or foreign
  loading screen is active; the single `PendingSaveLoadSlot` is last-writer-wins and reports supersession.
- **Pose**: yaw/pitch always to `InputState`; Character mode + live body → body `Transform` +
  `GlobalTransform` + `set_kinematic_translation` (returns `false` without a Rapier handle) and clears
  `vertical_velocity`/`is_grounded`/`wants_jump`; restore branches on the *live* mode (#2018), so
  FlyCam-saved/Character-loaded relocates the body and the reverse repositions the camera.
- **Context guards**: `LoadCommand` accepts interior OR exterior context and rejects only neither;
  `SaveInfoCommand` mirrors the three arms (#3500); `quickload_latest` walks `slots_by_recency` and falls back
  past a corrupt newest slot; `command_output_is_failure` scans every output line.
**Output**: `/tmp/audit/save/dim_5.md`

## Phase 3: Merge

Combine `/tmp/audit/save/dim_*.md` into `docs/audits/AUDIT_SAVE_<TODAY>.md`: **Executive Summary** (each
`lib.rs` design claim CODE-CONFIRMED/DRIFTED; findings by severity and Data-Loss Class) · **Data-Loss
Class Matrix** · **Completeness Ledger** (registry × `MUTABLE_DELTA_COLUMNS`: SAVED-only / SAVED+OVERLAID /
structural; cross-check against the guard's allowlist rather than re-deriving) · **Findings** (CRITICAL
first, deduplicated; two-list drift owned by Dim 1, GPU/physics teardown by Dim 5) · **Guards
Verified** (which ledger guards were confirmed live, which reasons were spot-checked).

## Phase 4: Cleanup

`rm -rf /tmp/audit/save`; tell the user the report is ready; suggest
`/audit-publish docs/audits/AUDIT_SAVE_<TODAY>.md` (domain label `save-load`; `test-gap` for coverage
findings, `doc-rot` for drifted save/load docs).
