---
description: "Deep audit of the M45 save/load subsystem — full-ECS-snapshot capture, type-erased registry, atomic disk write + ring, pre-save validation gates, and the M45.1 live load-apply (cell reload + FormId-keyed deltas + player-pose restore)"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Save / Load Subsystem Audit (M45 + M45.1)

Audit the `byroredux-save` crate (M45 — full-ECS-snapshot save format) and its
sole engine-side consumer (`byroredux/src/save_io.rs`, M45.1 live load-apply) for
**data-loss and save-corruption** correctness. The whole subsystem exists to
remove Bethesda's slow-corruption tail by making the live ECS the single source
of truth; the audit's job is to verify the CODE actually delivers that, not to
take the docstring's word for it. A silently-dropped component column, a stale
schema fingerprint, a torn frame-boundary capture, or a botched FormId remap each
**loses player progress** — frame those as CRITICAL/HIGH per
`.claude/commands/_audit-severity.md` (Data loss is CRITICAL on that scale).

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication, context rules, and finding format. See
`.claude/commands/_audit-severity.md` for the severity scale. Do NOT duplicate
those here.

## Scope

**Crate** (`crates/save/src/`, ~2k LOC — read ALL of it before auditing):
- `crates/save/src/lib.rs` — module docstring (design intent: full snapshot,
  atomic write, ring, validation gate, load-off-frame), `SaveError` enum, public
  re-exports.
- `crates/save/src/snapshot.rs` — `Snapshot` struct, binary container layout
  (`FORMAT_MAGIC` / `FORMAT_MAJOR` / `FORMAT_MINOR` / `HEADER_LEN`), `encode` /
  `decode` (magic / version / schema-fpr / CRC32 / payload-len gates).
- `crates/save/src/registry.rs` — `SaveRegistry`, the type-erased `SaveFn` /
  `LoadFn` / `ApplyFn` closures, `register_component` / `register_resource` /
  `register_form_id_component`, `schema_fingerprint` (FNV-1a), `form_id_column`.
- `crates/save/src/driver.rs` — `save_world`, `restore_world`,
  `restore_resources`, `build_form_id_remap`, `apply_deltas`,
  `validate_snapshot_types` (non-mutating typed-decode preflight, #3163 —
  read every registered column's JSON through its `ValidateFn` without
  touching a `World`; called by both `restore_world`, before
  `clear_entities`, and the live-load drain, before cell teardown).
- `crates/save/src/disk.rs` — `write_slot` (tmp → fsync → read-back-verify →
  rename), `read_slot`, `list_slots`, `SaveRing`.
- `crates/save/src/validate.rs` — `validate_world`, `ValidationError`,
  `ValidationKind`, the three sub-checks (hierarchy / equipment / animation).
- `crates/save/tests/round_trip.rs` — the crate-level integration tests; read to
  learn which invariants are already guarded.

**Engine-side consumer** (`byroredux/src/save_io.rs` — the ONLY live caller of
the crate; the crate audit is incomplete without it):
- `build_save_registry` — the curated type set (the authoritative completeness
  list).
- `MUTABLE_DELTA_COLUMNS` — the **second** hardcoded column list that drives the
  live overlay; must stay in lockstep with `build_save_registry`.
- `SaveCommand` / `SaveInfoCommand` / `LoadCommand` (console commands),
  `SaveState`, `PendingSaveLoadSlot`, `PlayerPose`.
- `capture_player_pose`, `apply_player_pose`, `execute_pending_save_loads`,
  `snapshot_cell_context`, `snapshot_player_pose`.
- `SaveLoadNotifications` (added 2026-08-24) — a `Vec<String>` resource that
  `notify_player` appends to on every *failure* path inside
  `reload_interior_session` / `reload_exterior_session` /
  `execute_pending_save_loads` (aborted preflight, failed cell reload, failed
  resource restore, failed delta apply, lost cell/exterior context). It is
  NOT a general save/load confirmation channel — success has no notification,
  only these six abort/error sites do.
- `queue_startup_load` — parses `--load <slot>` through the same
  `queue_load_slot` → `LoadCommand` primitive the console's explicit `load
  <slot>` command calls directly (F9 and the pause menu instead go through
  `quickload_latest`, which itself calls `queue_load_slot` in its
  newest-first fallback loop — all three converge on the one `LoadCommand`
  path). Replaces the old inline `slot.parse::<u32>()` that used to live in
  `main.rs`'s `App::new`.
- `command_output_is_failure` — now `.iter().any(...)` over every output
  line (was `.first()`-only); a multi-line `CommandOutput` whose failure
  marker isn't the first line no longer reads as success.
- `quickload_latest` — walks `disk::slots_by_recency` newest-first and
  falls back to the next-newest slot when the newest fails
  `command_output_is_failure` (a corrupt/undecodable newest save no longer
  hard-fails quickload).

**Cross-cut ground truth — read before auditing the relevant dimension**:
- `byroredux/src/boot.rs` — registry/state install at boot (~line 1137, also
  installs `SaveLoadNotifications::default()` alongside `PendingSaveLoadSlot`);
  `byroredux/src/app_events.rs` — the per-frame ordering of `capture_player_pose`
  THEN `step_save_loads` (in `about_to_wait`, ~line 684; moved out of *main.rs*
  by the #2731 split); `byroredux/src/app_step.rs` — `step_save_loads`
  body (~line 291); `byroredux/src/app_frame.rs` — `render_one_frame` drains
  `SaveLoadNotifications` via `mem::take` (unconditionally, whether or not a
  debug-UI is installed) and, later in the SAME `about_to_wait` call, feeds the
  first message to `byroredux_debug_ui::DebugUiState::push_player_message` and
  any remaining ones to `push_console_line` — so a load-failure toast from this
  frame's `step_save_loads` renders in this frame's draw, not next frame's.
- `crates/debug-ui/src/lib.rs` — `DebugUiState::push_player_message` (also
  appends to console history, arms a 4-second expiry); `crates/debug-ui/src/panels.rs`
  — `draw_player_message` (the on-screen toast).
- `byroredux/src/cell_loader/transition.rs` — `CurrentCellContext` (the saved
  cell identity), `reposition_camera` (FlyCam restore target).
- `crates/core/src/ecs/world.rs` — `insert_batch` (the `entity < next_entity`
  `debug_assert`, NOT a release-mode guard), `clear_entities`, `set_next_entity`,
  `next_entity_id`.
- `crates/core/src/string/mod.rs` — `StringPool::dump` / `from_dump` (symbol-order
  round-trip contract).
- `crates/physics/src/sync.rs` — `set_kinematic_translation` (returns `false` /
  no-ops when no Rapier handle).

**Confirmed-shipped surface (verify against live code, do not assume)**:
- Container is binary-framed JSON payload: 32-byte header (`magic` 8 / `major` 2 /
  `minor` 2 / `schema_fpr` 8 / `crc32` 4 / `payload_len` 8) + serde_json `Snapshot`.
- `Snapshot { next_entity, strings, components: BTreeMap, resources: BTreeMap }`.
- Disk slots are `<dir>/save_<slot>.ess`; ring is in-memory round-robin.
- Live load = `validate_snapshot_types` preflight → reload saved cell via
  `load_cell_with_masters` → `restore_resources` → `build_form_id_remap` →
  `apply_deltas(MUTABLE_DELTA_COLUMNS)` → `apply_player_pose`.
- `restore_world` (clear + full repopulate) is the **test/loose path**; the LIVE
  load path uses `apply_deltas` overlay, NOT `restore_world` — two divergent
  restore code paths. Both now share the same `validate_snapshot_types` typed
  preflight (`restore_world` runs it before `clear_entities`; the live path
  runs it before any cell/streaming teardown) — added 2026-08-24 (#3163) so a
  malformed column is rejected before either restore path can touch a world.
- `FORMAT_MAJOR` is 5 (was 4 as of the last skill sync) — the bump added
  required `Material.water_shader_flags`/`Material.is_water_shader`,
  `RigidBodyData.collidable`, and the newly-saved `CharacterController`
  column (#3164/#3165). Do not hardcode this number elsewhere in findings —
  re-read `crates/save/src/snapshot.rs`'s `FORMAT_MAJOR` doc comment, it will
  drift again.

**Doc-rot check**: `docs/feature-matrix.md:189` already carries an explicit
`TD3-002` comment noting Save/load (M45/M45.1) shipped 2026-06-21 — the
"unstarted" row is gone. Do not re-flag this as doc-rot; confirm it still reads
correctly before reporting anything here.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,6`). Default: all 6.
- `--depth shallow|deep`: `shallow` = check container/API contracts; `deep` = trace
  full capture → encode → disk → decode → reload → delta-apply data flow + the
  frame-boundary / off-frame drain ordering. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Snapshot Completeness & Determinism | Registry & (De)serialization |
  Disk Format & Durability | Validation Gates | Frame-Boundary Capture & Off-Frame
  Apply | M45.1 Live Load-Apply
- **Data-Loss Class**: silent-drop | corruption-on-load | irrecoverable-write |
  reference-break | none — every finding that can lose progress MUST name its class.

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`.
2. `mkdir -p /tmp/audit/save`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/save/issues.json`
4. **Read the most recent `docs/audits/AUDIT_SAVE_*.md` report** (sort by date —
   do not hardcode a filename here, it rots every cycle). Diff direction against
   it: findings it already closed are regression checks, not new findings — verify
   the fix is still in place before reporting anything as NEW. Also scan
   `docs/audits/` for any save/load mention in other reports and grep
   `issues.json` for `save`, `load`, `snapshot`, `corrupt`, `FormId`.
5. Read the `crates/save/src/lib.rs` module docstring and the `crates/save/src/snapshot.rs`
   container-layout doc-comment. They state the design intent (atomic write,
   ring, validation gate, off-frame load). For each claim, the matching dimension
   must verify the CODE delivers it — a docstring promise the code doesn't keep is
   itself a finding.
6. Run the registry-completeness guard before Dimension 1 starts: `cargo test -p
   byroredux every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`
   (SAVE-D1-12, #2295/#3166, `byroredux/src/save_io/registry_completeness_tests.rs`).
   Its `SCAN_ROOTS` widened 2026-08-24 (#3166) from `crates/core/src/ecs/components/`
   to the WHOLE of `crates/core/src`, and gained `crates/audio/src` +
   `crates/plugin/src` alongside the pre-existing `crates/scripting/src`,
   `crates/physics/src`, `byroredux/src` — do not describe this guard as
   components-only, it now source-scans every `impl Component for` / `impl
   Resource for` line under those six roots. It skips test-only sources
   (any `*_tests.rs` file, anything under a `tests/` path component, and
   everything after the first `#[cfg(test)]` marker in a production file) so
   fixture types declared alongside production code don't pollute the ledger.
   Asserts each surviving type is registered in `build_save_registry` XOR
   listed in the test's own `NOT_SAVED_BY_DESIGN` allowlist with a one-line
   reason. A green run IS the completeness ledger — Dimension 1 should consume
   its `NOT_SAVED_BY_DESIGN` list rather than re-deriving completeness from
   scratch, spot-checking a sample of reasons for staleness (the guard
   enforces a reason exists, not that it's still true).

## Phase 2: Launch Dimension Agents

Ordered by data-loss risk: completeness + registry first (silent-drop is the worst
class), durability + validation next, frame-boundary + live-apply last.

### Dimension 1: Snapshot Completeness & Determinism (highest risk)
**Entry points**: `byroredux/src/save_io.rs` — `build_save_registry`,
`MUTABLE_DELTA_COLUMNS`; `crates/save/src/driver.rs` — `save_world`;
`crates/save/src/snapshot.rs` — `Snapshot`.
**Why highest risk**: a persistent component that nobody registered is silently
absent from every save — invisible until the player notices their progress is
gone. Data-Loss Class = silent-drop.
**Checklist**:
- **The registry IS the completeness contract.** Enumerate every component/resource
  in `build_save_registry` and cross-check against the full game-state component
  set (inventory, equipment, lights, animation, scripting, form id, plus the
  `ItemInstancePool` / `CurrentCellContext` / `PlayerPose` / `GameTimeRes` /
  `QuestAliasInjectionState` resources — the M34 day/night clock and the QUST
  alias inventory-grant ledger, both registered 2026-08-07 — and, registered
  2026-08-24, `Globals` — MQ101's startup fragment writes `GameHour` before
  advancing to stage 10, so GLOB values are mutable game state, not a
  from-ESM constant — and `ReferenceEnableState` — the fragment `Disable()`
  effect's FormId-keyed enable/disable ledger; note it round-trips correctly
  but has **no consumer anywhere in cell_loader/streaming yet** (`is_enabled`
  is called only from its own test module), so a save/reload today correctly
  carries the flag without it having any observable effect live or on
  reload — a scripting-domain completeness gap, not a save-domain one; don't
  raise it as a save finding, but don't claim `Disable()` persists visibly
  either). For EACH persistent
  component type in the codebase that carries player-mutable state, confirm it
  is either registered OR documented as reconstruct-on-load (derived data:
  `GlobalTransform`, `WorldBound`; GPU handles: `MeshHandle`, `TextureHandle`,
  `SkinnedMesh`; transient event markers). An unregistered *mutable* component =
  HIGH silent-drop finding. Don't re-derive this list by hand — run the
  SAVE-D1-12 guard (Phase 1 step 6) and start from its `NOT_SAVED_BY_DESIGN`
  allowlist. Building that allowlist on 2026-08-05 surfaced 7 genuine gaps, all
  now fixed and registered: `RigidBodyData` (#2379), `RumbleOnActivate`
  (#2382), `Material` (#2378), `FragmentExecutionQueue` (#2381), and the MQ101
  cinematic trio `ActorCinematicState`/`HorseTetherState`/
  `CinematicPresentationState` (#2380). Verify none of the seven regressed back
  out of `build_save_registry`. A later gap closed 2026-08-24 (#3164/#3165):
  `byroredux_physics::CharacterController` — previously reconstructed fresh
  every reload with a `NOT_SAVED_BY_DESIGN` entry, now registered because its
  `breath_remaining`/`drowning_damage_accumulator` fields are genuine
  fractional gameplay carry (swim/drowning state), not just per-frame motion
  state. Verify the allowlist entry for `CharacterController` stays REMOVED
  (it would be a silent-drop regression if it reappeared) and that
  `WaterContact`'s own allowlist reason was updated to point at the now-saved
  `CharacterController` column rather than claiming "drowning accumulation is
  not yet wired".
- **Two lists, one truth (drift hazard).** `MUTABLE_DELTA_COLUMNS` in
  `byroredux/src/save_io.rs` is a SEPARATE hardcoded `&[&str]` from the
  `register_component` calls in `build_save_registry`. The live load only overlays
  columns named in BOTH the registry AND `MUTABLE_DELTA_COLUMNS`. A component
  registered (so it's SAVED) but absent from `MUTABLE_DELTA_COLUMNS` is captured
  to disk yet **never replayed on a live load** — its post-spawn changes are
  silently lost. Verify every mutable column in `build_save_registry` appears in
  `MUTABLE_DELTA_COLUMNS` (or is deliberately structural/identity: `Name`,
  `Parent`, `Children`, the form-id key). Flag any registered-but-not-overlaid
  mutable column as HIGH (silent-drop on load). Guard: `delta_columns_carry_only_session_stable_fields`
  (#1720, `47dad578`) tripwires any future addition to `MUTABLE_DELTA_COLUMNS`
  against embedding a `FixedString`/`EntityId`/session-local handle. `Material`
  (#2378) and the `ActorCinematicState`/`HorseTetherState` pair (#2380) are
  current, deliberate instances of this exact pattern — registered but NOT in
  `MUTABLE_DELTA_COLUMNS` (blast-radius and `EntityId`-hazard reasons
  respectively, documented at each `register_component` call site in
  `byroredux/src/save_io.rs`). Verify they stay documented as intentional
  rather than silently drifting into the HIGH bucket above. `CharacterController`
  (#3165, added 2026-08-24) is the opposite deliberate case — registered AND
  present in `MUTABLE_DELTA_COLUMNS`, because unlike `Material`/the cinematic
  pair its whole struct is plain f32/bool/enum fields (no `FixedString`/
  `EntityId`), so the tripwire guard passes it. Its overlay happens BEFORE
  `apply_player_pose`, which unconditionally zeroes `vertical_velocity` /
  `is_grounded` / `wants_jump` on the SAME struct afterward — verify that
  ordering still holds (`execute_pending_save_loads` calls `apply_deltas` then
  later `apply_player_pose`), otherwise a stale grounded/jump flag from the
  saved session would survive into the reloaded cell.
- **Determinism.** `Snapshot.components` / `.resources` are `BTreeMap` (sorted
  keys) and `save_world` skips empty columns / null resources. Confirm the CRC is
  reproducible at equal state: column ROW order comes from `World::query` iteration
  — verify that order is stable across runs (storage iteration order) or that
  determinism is only claimed at the column-key level, not row level. A
  per-run-varying row order breaks the "reproducible CRC" claim in the docstring
  (MEDIUM doc/contract mismatch, not data loss).
- **`next_entity` round-trip.** `save_world` records `world.next_entity_id()`;
  restore replays it via `set_next_entity` BEFORE inserts so original (sparse)
  ids pass `insert_batch`'s `entity < next_entity` guard. Verify the high-water
  mark is saved verbatim (a too-low value silently drops every row at/above it via
  the debug_assert — and in RELEASE the assert is COMPILED OUT, so the row inserts
  at an unspawned id with no diagnostic). Flag the release-mode silence as MEDIUM.
- **StringPool symbol-order contract.** `save_world` dumps via `StringPool::dump`
  (symbol order); restore re-interns via `from_dump`. Every `FixedString` in a
  saved component is a symbol index into this pool. Verify `dump`/`from_dump`
  preserve index identity (re-interning the exact sequence reproduces every
  symbol). A reordered or de-duplicated dump = every `Name` / interned string
  points at the wrong symbol = CRITICAL corruption-on-load. Confirm against
  `crates/core/src/string/mod.rs`.
- **Empty-column omission vs. delete-on-load.** `save_world` omits empty columns;
  `restore_world` only `load`s columns present in the snapshot. The live overlay
  is additive-only (can insert/update, never remove) — documented at
  `apply_deltas` (#1847/SAVE-04, `cec3b9ab`). Confirmed **inert today** (no
  enable/disable/delete persistence exists to leak orphaned rows). Re-flag only
  once such a component lands without the promised companion despawn/hide pass —
  until then this is DEFERRED-DOCUMENTED, not a live finding.
**Output**: `/tmp/audit/save/dim_1.md`

### Dimension 2: Registry & (De)serialization Fidelity
**Entry points**: `crates/save/src/registry.rs` — `register_component`,
`register_resource`, `register_form_id_component`, the `SaveFn`/`LoadFn`/`ApplyFn`/`ValidateFn`
closures, `component_validate`, `resource_validate`, `schema_fingerprint`,
`form_id_column`, `FnvHasher`; `crates/save/src/driver.rs` —
`validate_snapshot_types` (the crate-level entry point the two `*_validate`
accessors feed).
**Checklist**:
- **`ValidateFn` is a third closure per `Entry`, added 2026-08-24 (#3163).**
  Every `register_component` / `register_resource` / `register_form_id_component`
  call now builds a `validate: Box<dyn Fn(serde_json::Value) -> Result<(), SaveError>>`
  alongside `save`/`load` (and `apply` for components) — a non-mutating
  `serde_json::from_value::<T>` decode that never touches a `World`.
  `driver::validate_snapshot_types` walks every snapshot column through it.
  Verify: (a) every `register_*` variant builds a matching `validate` closure
  (a variant that forgets to wire one silently exempts its column from the
  preflight — the column would only fail during the actual `load`, after
  teardown has already happened); (b) the closure decodes the SAME target
  type `load` does (`Vec<(u32, T)>` for components/form-id columns, bare `T`
  for resources) so a preflight pass genuinely guarantees the later `load`
  call succeeds; (c) `component_validate`/`resource_validate` do a linear
  `.find()` over `self.components`/`self.resources` by name — correctness is
  fine at registry size, but confirm no accidental first-match-wins collision
  with `form_id_column`'s dedicated-flag fix (Dimension 2's own note below).
- **Serde availability is feature-gated.** Components serialise only with
  `serde::Serialize + DeserializeOwned`; these derives are behind
  `#[cfg_attr(feature = "inspect", …)]` on the core types, and `crates/save`
  depends on `byroredux-core` with `features = ["save"]` (which pulls `inspect`).
  Confirm the `save` → `inspect` feature chain in `crates/save/Cargo.toml` and
  `crates/core/Cargo.toml` so a non-default build can't compile away the serde
  impls and ship a save crate that round-trips nothing. A registry that builds but
  whose columns serialise to `null` is a silent-drop trap.
- **Schema fingerprint = coarse drift only.** `schema_fingerprint` is FNV-1a over
  ordered, kind-tagged column KEYS — it catches add/remove/rename of a TYPE, NOT
  an intra-type field change. Confirm the doc-comment's stated limitation matches
  reality and that an intra-type field change is caught at load by
  `serde_json::from_value` failing (a `SaveError::Serde`, not a silent
  default-fill). The danger case: a field ADDED with `#[serde(default)]` would
  load OLD saves silently. A guard test, `serde_default_on_saved_struct_requires_format_major_bump`
  (`byroredux/src/save_io/serde_default_guard_tests.rs`, #1714, `806ba7af`),
  source-scans every save-participating type (top-level + nested: `ItemStack`,
  `AnimationLayer`, `FormIdPair`, …) for ANY `#[serde(default)]` attribute —
  it is an unconditional offender list, not gated on the current
  `FORMAT_MAJOR` value (prior skill text describing it as checking "while
  `FORMAT_MAJOR == 1`" was stale — the test has never read `FORMAT_MAJOR` at
  all). The `Option`-widening half is still uncaught statically (legitimate
  `Option`s already exist) — verify this residual is still documented, not
  silently dropped.
- **Shape-fingerprint guard closes the `Option`-widening gap (added
  2026-08-24, #3164/#3165).** `saved_type_shape_changes_require_format_major_bump`
  (same file) is a SECOND, stronger check: it normalizes (whitespace/comment
  stripped) every `#[derive(...Serialize...)] struct`/`enum` body reachable
  from `save_type_sources()`, hashes the sorted set with the same FNV-1a
  scheme as `schema_fingerprint`, and asserts the result against a hardcoded
  `BASELINE_SHAPE_FINGERPRINT` + `BASELINE_MAJOR`. Unlike the `#[serde(default)]`
  scan, this ALSO catches a field simply being added/removed/retyped without
  any `default` attribute (the `Option`-widening case the doc-comment above
  calls out as previously uncaught). Verify: (a) the baseline was regenerated
  in the SAME commit `FORMAT_MAJOR` last changed (a stale baseline masks the
  next real drift as a false failure, training reviewers to bump the
  baseline reflexively without checking whether `FORMAT_MAJOR` should also
  move); (b) `save_type_sources()`'s discovery set (feature-gated files +
  the explicit non-turbofish edges: `crates/core/src/form_id.rs`,
  `crates/core/src/ecs/components/form_id.rs`, `crates/core/src/string/mod.rs`,
  `crates/plugin/src/esm/records/script_instance.rs`) still covers every
  save-participating type — a type reachable ONLY through a manual `impl
  Serialize` (no `#[derive]`) would be invisible to both guards.
- **Fingerprint stability across builds.** `FnvHasher` is hand-rolled
  specifically because `DefaultHasher` is unspecified across std versions. Verify
  the FNV constants (`0xcbf2_9ce4_8422_2325` offset basis, `0x100_0000_01b3`
  prime) are the canonical 64-bit FNV-1a values and that the hash depends ONLY on
  registered names + order — not on any address/TypeId (which would vary per run
  and reject every save).
- **`form_id_column` (regression guard, #1845, `326fcb44`).** `form_id_column()`
  is keyed off an explicit `Entry::is_form_id` flag (not the old `apply.is_none()`
  heuristic), with a registration-time assert against a second form-id column.
  Guard: `form_id_column_resolves_the_flagged_entry`. Verify a future
  `register_*` variant can't silently reintroduce the old
  first-`apply:None`-wins heuristic (that pre-fix behavior would let any future
  `apply: None` component hijack the live-load remap key).
- **FormId handle vs. pair.** `register_form_id_component` saves the stable
  `FormIdPair` (resolved through `FormIdPool`), NOT the session-local `FormId`
  handle. Save skips (with WARN) any handle that doesn't resolve in the pool;
  load re-interns the pair to a fresh handle. Verify: (a) save never panics on an
  unresolvable handle, (b) load's `resource_mut::<FormIdPool>()` can't deadlock /
  panic if the pool resource is absent, (c) the re-interned handle is internally
  consistent with every OTHER re-interned reference in the same load. A handle
  saved verbatim instead of the pair = CRITICAL reference-break across loads.
- **Round-trip fidelity.** `crates/save/tests/round_trip.rs` and the
  `save_io.rs` tests (`binary_registry_round_trips_including_scripttimer`,
  `player_pose_survives_snapshot_round_trip`) are the guards. Verify the cross-crate
  `ScriptTimer` and a stable form id round-trip; flag any registered type with no
  round-trip coverage (LOW test-gap unless the type has tricky serde).
**Output**: `/tmp/audit/save/dim_2.md`

### Dimension 3: Disk Format & Durability
**Entry points**: `crates/save/src/disk.rs` — `write_slot`, `read_slot`,
`list_slots`, `parse_slot_filename`, `SaveRing`; `crates/save/src/snapshot.rs` —
`encode` / `decode` header gates.
**Checklist**:
- **Atomic write dance.** `write_slot` does `create_dir_all` → write `.tmp` →
  `flush` → `sync_all` → READ-BACK-VERIFY (`readback != bytes` → delete tmp +
  error) → `rename`. Verify the ordering is exactly that: the `rename` is the LAST
  step and only runs after a byte-exact read-back. A rename-before-fsync, or a
  read-back that compares lengths only, is a HIGH durability hole (a lying/short
  write can replace a good save). Confirm the failed read-back removes the tmp and
  returns `SaveError::Io` rather than proceeding to rename.
- **Directory durability gap — CLOSED (SAVE-D3-01).** `sync_all` fsyncs the
  FILE; on most filesystems the `rename` itself is not durable until the
  DIRECTORY is fsynced. `write_slot` now opens `dir` as a `File` and calls
  `sync_all()` on it immediately after `rename` (Unix-only capability — skipped
  where opening a directory fails, e.g. Windows, which journals the rename
  instead). Prior skill text framing this as an open "check whether — if not,
  flag as MEDIUM" is stale; verify the fsync-after-rename call is still there
  rather than re-deriving the gap as if unaddressed.
- **`latest_slot` / `slots_by_recency` (rewritten 2026-08-24, #3167 quickload
  fallback work).** `latest_slot` is now `slots_by_recency(dir).into_iter().next()`;
  `slots_by_recency` returns EVERY valid slot newest-first, with mtime ties
  broken by higher slot number (`recency_tie_breaks_by_slot_number`) so
  ordering is deterministic even when two slots share a filesystem-granularity
  mtime. Verify: (a) a `.tmp` sibling is still excluded by `parse_slot_filename`
  (`latest_slot_ignores_newer_tmp_and_empty_directory`); (b) an unreadable
  metadata entry is skipped, not treated as newest-by-default; (c) the binary's
  `quickload_latest` (Dimension 5/6) consumes the FULL ordered list — the whole
  point of exposing `slots_by_recency` beyond a single `Option<u32>` is letting
  the caller fall back past a corrupt newest slot instead of hard-failing.
- **Ring never clobbers the last good save.** `SaveRing::advance` is round-robin
  over `0..size` (size floored to ≥1); `SaveCommand` with no arg calls
  `ring.advance()`. Verify a quicksave spreads across slots so the previous good
  save survives (the explicit design goal vs. Bethesda's "F5 ate my save").
  Regression guard (SAVE-D3-02): the cursor itself is in-memory only (`SaveRing`
  is not persisted), so `SaveState::new` builds it via `SaveRing::resume`, which
  scans on-disk slot mtimes (`cursor_after_newest`) and starts one past the
  newest — not via a bare `SaveRing::new`, which would restart at slot 0 every
  launch and let the first quicksave of a new session clobber whichever slot is
  newest on disk. Verify `SaveState::new` still calls `resume`, not `new`.
- **Header gate ordering in `decode`.** Must be: length ≥ `HEADER_LEN` →
  `Truncated`; magic → `BadMagic`; major mismatch → `UnsupportedVersion`;
  schema_fpr mismatch → `SchemaMismatch`; then `payload_len` bounds
  (`checked_add` overflow → `Truncated`, `bytes.len() < payload_end` → `Truncated`);
  then CRC over the payload → `CrcMismatch`; then `from_slice`. Verify ALL gates
  precede `serde_json::from_slice` so a corrupt/truncated/skewed file fails before
  any parse. A CRC check AFTER parse, or a missing `payload_len` bounds check
  (slice panic), is HIGH.
- **CRC scope.** `encode` CRCs the PAYLOAD only (not the header). Confirm `decode`
  recomputes over the same payload slice `[HEADER_LEN..payload_end]`. A header
  edit (e.g. version bump) deliberately does NOT trip CRC — verify the version
  gate catches it instead (guarded by `rejects_major_version_skew`). A CRC that
  covered the header would make the version-skew error unreachable.
- **`parse_slot_filename` strictness.** Confirm `save_42.ess.tmp` and `save_x.ess`
  are rejected so a stray tmp or garbage file never registers as a slot (guard:
  `parse_slot_names`). A loose parse would surface a half-written tmp as a
  loadable slot.
- **`minor` version is advisory.** A newer MINOR still loads (serde default-fills
  missing fields). Confirm `decode` does NOT reject on minor skew — but cross-check
  Dimension 2's `#[serde(default)]` concern: advisory-minor + default-fill is the
  exact path that can silently load a downgraded save.
**Output**: `/tmp/audit/save/dim_3.md`

### Dimension 4: Validation Gates (the slow-corruption-tail defense)
**Entry points**: `crates/save/src/validate.rs` — `validate_world`,
`validate_hierarchy`, `validate_equipment`, `validate_animation`,
`ValidationKind`; `crates/save/src/driver.rs` — `validate_snapshot_types` (the
typed-decode preflight, #3163, added 2026-08-24); `byroredux/src/save_io.rs` —
`SaveCommand::execute` (the write-path gate caller).
**Why this dimension**: the whole format's thesis is "refuse to persist an
inconsistent world rather than seed a corruption tail." This dimension verifies
the gate actually exists, runs before write, and covers the references that matter.
**Checklist**:
- **Gate is enforced on the write path.** `SaveCommand::execute` calls
  `validate_world` and, on a non-empty result, ABORTS the save (prints up to 20
  issues, never writes). Verify the abort precedes `save_world`/`encode`/`write_slot`
  — a validation that runs but doesn't block the write is theatre (HIGH: the
  corruption-tail defense is a no-op). Confirm there is NO alternate save path that
  bypasses the gate.
- **Coverage vs. claim (regression guard, #1700, `380ea4c4`).** `validate_world`
  now checks FOUR reference classes — Hierarchy (`Parent`⇄`Children` bidirectional
  agreement + dangling-id), Equipment (`EquipmentSlots` occupant indexes a live
  `Inventory` row), Animation (`AnimationPlayer.clip_handle` resolves in
  `AnimationClipRegistry`, `root_entity` is spawned), and ItemInstance
  (`validate_inventory_instances` — `Inventory` rows resolve against
  `ItemInstancePool`) — plus a FIFTH the binary layers on top:
  `validate_form_ids` (`byroredux/src/save_io.rs`) checks cross-plugin FormId
  resolvability, run in `SaveCommand::execute` before every save. Verify all
  five still run pre-write. Regression: 6 tests split core/binary (dangling/
  no-pool rejected, resolvable passes). Enumerate any newly-added inter-entity
  reference type not yet covered by one of these five as a MEDIUM
  defense-in-depth gap.
- **Dangling-id semantics.** `validate_hierarchy` / `validate_animation` flag any
  referenced id `>= next_entity` as `DanglingEntity`. Verify this catches
  never-spawned ids but does NOT false-positive on legitimately sparse-but-spawned
  ids (an id `< next_entity` that has no live components is still "spawned" by the
  high-water-mark model). Confirm the check is `>= next_entity`, not "id has no
  components."
- **Equipment occupant bounds.** `validate_equipment` resolves the occupant index
  against the SAME entity's `Inventory.items.len()`. Verify the `inv.iter().find`
  per-occupant is O(equip×inv) but correct; flag the None-Inventory and
  out-of-bounds cases produce distinct errors. An off-by-one (`>` vs `>=`) here
  passes a save that loads an out-of-bounds equip → corruption-on-load.
- **Load-side validation (regression guard, #1844, `dc89ff68`).** `decode`
  validates the CONTAINER (magic/CRC/version/schema); `log_validation_warnings`
  now ALSO re-runs `validate_world`(+`validate_form_ids`) post-load, wired into
  both `restore_world` (`crates/save/src/driver.rs`) and
  `execute_pending_save_loads` (`byroredux/src/save_io.rs`, right after
  `apply_deltas`) — diagnostic-only (WARN-log, no abort; a load can't cleanly
  revert). Verify it stays diagnostic-only and covers both restore paths.
  Regression: the `round_trip.rs` test proving `restore_world` neither aborts
  nor silently repairs broken-but-decodable data.
- **Typed-decode preflight gate — this one DOES abort (added 2026-08-24,
  #3163).** Distinct from the diagnostic-only post-load `validate_world` above:
  `validate_snapshot_types` runs a non-mutating `serde_json::from_value`
  decode of every registered column and, on ANY `SaveError::Serde`, is
  treated as a hard abort by both callers — `restore_world` returns the error
  before `clear_entities` runs (verified by
  `typed_snapshot_preflight_rejects_bad_column_without_world_mutation` in
  `crates/save/tests/round_trip.rs`), and `execute_pending_save_loads` returns
  before ANY cell/streaming teardown (`byroredux/src/save_io.rs`, right after
  `build_save_registry()`, before `snapshot_cell_context`/`reload_interior_session`).
  This closes exactly the gap Dimension 5/6's "delta apply failed mid-overlay"
  concern used to describe: a malformed column now fails BEFORE the
  irreversible teardown, not after. Verify a delta-apply (`apply_deltas`)
  failure that still occurs post-preflight (an "unexpected apply failure" per
  its own log message) is a DIFFERENT, narrower failure class — this preflight
  guarantees typed-decodability, not that every `ApplyFn`'s entity-remap logic
  succeeds — and that `execute_pending_save_loads` still returns immediately
  on that later failure too (`crate::combat::reconcile_dead_actor_runtime_state`
  runs, then it returns without falling through to post-load `validate_world`
  or `apply_player_pose` on a partially-overlaid world — this `return` was
  ADDED in the same 2026-08-24 change; prior code logged the error and fell
  through, continuing on into pose-restore over an admittedly-partial overlay).
**Output**: `/tmp/audit/save/dim_4.md`

### Dimension 5: Frame-Boundary Capture & Off-Frame Apply
**Entry points**: `crates/save/src/driver.rs` — `save_world` (read-only capture),
`restore_world` (`&mut World`); `byroredux/src/save_io.rs` —
`SaveCommand` (read-only), `LoadCommand` (queues), `execute_pending_save_loads`
(the `&mut World` drain), `capture_player_pose`, `SaveLoadNotifications`,
`notify_player`; `byroredux/src/app_events.rs` run-loop ordering (the
`about_to_wait` arm, ~line 684 — post-#2731; do not look for it in *main.rs*);
`byroredux/src/app_frame.rs` — `render_one_frame`'s `SaveLoadNotifications`
drain (~line 104, immediately after the debug-UI snapshot is built).
**Checklist**:
- **Capture is read-only and consistent.** `save_world` takes `&World` (queries +
  `try_resource`), so it can run as a console command without `&mut`. Verify the
  capture reads a CONSISTENT world — it must run at a frame boundary, NOT mid-system
  with some storages already mutated this tick. `SaveCommand` runs through the
  console drain; confirm the console drain executes at a point where the scheduler
  is between ticks (no system holds a storage write lock). A capture interleaved
  with a running system would snapshot torn state (e.g. half-propagated transforms)
  — CRITICAL if a system can be mid-mutation during the capture.
- **`capture_player_pose` ordering.** It runs in `app_events.rs` (`about_to_wait`) AFTER the scheduler's
  camera systems published this frame's `Transform`/`GlobalTransform` and BEFORE
  `step_save_loads`, every frame. Verify the pose source is post-propagation
  (reads `Transform.translation` of the body in Character mode, camera in FlyCam),
  not stale interpolation state. A pre-propagation read saves last-frame's pose
  (MEDIUM, position-off-by-one-frame; not data loss).
- **Load is off-frame, drained between ticks.** `restore_world` /
  `apply_deltas` need `&mut World`, which a system can't get. `LoadCommand` only
  decodes + pushes to `PendingSaveLoadSlot`; `execute_pending_save_loads` drains
  it in `step_save_loads` where the App owns `&mut World` + `&mut VulkanContext`.
  Verify the load NEVER runs inside the scheduler (it would alias the world). This
  mirrors `PendingDebugLoadSlot`; confirm the drain `take()`s the slot (load runs
  once) and no-ops on an empty slot.
- **`SaveLoadNotifications` drain is unconditional (added 2026-08-24).**
  `notify_player` pushes onto this `Vec<String>` resource from every failure
  arm inside `reload_interior_session` / `reload_exterior_session` /
  `execute_pending_save_loads` (aborted preflight, failed cell reload, failed
  resource restore, failed delta apply, lost cell/exterior context) — it is
  NOT used on the success path, so don't expect a "save complete" toast here.
  `render_one_frame` (`app_frame.rs`) drains it via `mem::take` regardless of
  whether `self.debug_ui` is `Some` — verify that stays true, since a `None`
  debug-UI (headless bench / dedicated-server-style run) that instead
  SKIPPED the drain would let the `Vec` grow unbounded across every failed
  load attempt for the process lifetime. Because `step_save_loads` (which can
  call `notify_player`) and `render_one_frame`'s drain both run inside the
  SAME `about_to_wait` invocation (drain call site is AFTER the save-load
  step), a failure this frame surfaces as a toast in this frame's draw, not
  next frame's — verify that ordering holds if either call site moves.
- **`clear_entities` does NOT tear down GPU/physics handles.** `restore_world`
  drops component data but the docstring (and `world.rs`) explicitly state GPU/
  physics handles are the CALLER's responsibility. The live path
  (`execute_pending_save_loads`) uses `unload_current_interior` +
  `drain_streaming_state` BEFORE the reload to release those handles — but it uses
  `apply_deltas` (overlay), NOT `restore_world`. Verify the live path's teardown
  fully releases GPU/physics handles before reload so no leaked BLAS/texture/Rapier
  body survives the load (HIGH resource leak per load otherwise). Confirm the
  `restore_world` clear-path is ONLY reached in tests / loose mode where there are
  no GPU handles to strand.
- **Two restore paths, divergent semantics.** `restore_world` (clear + full
  repopulate at saved ids) vs. the live `restore_resources` + `apply_deltas`
  (overlay onto a freshly-reloaded cell, id-remapped). They are NOT interchangeable:
  `restore_world` reuses SAVED entity ids; `apply_deltas` remaps to the reloaded
  cell's FRESH ids. Verify the live load never accidentally calls `restore_world`
  (which would resurrect the saved cell's ids on top of the reloaded cell's ids =
  id collision / CRITICAL corruption). Confirm `execute_pending_save_loads` calls
  ONLY `restore_resources` + `apply_deltas`, never `restore_world`.
**Output**: `/tmp/audit/save/dim_5.md`

### Dimension 6: M45.1 Live Load-Apply (cell reload + FormId deltas + pose)
**Entry points**: `byroredux/src/save_io.rs` — `execute_pending_save_loads`,
`build_form_id_remap` (in `crates/save/src/driver.rs`), `apply_deltas`,
`apply_player_pose`, `snapshot_cell_context`, `snapshot_player_pose`;
`crates/save/src/driver.rs` — `validate_snapshot_types` (typed preflight, first
step of the drain, #3163); `byroredux/src/cell_loader/transition.rs` —
`CurrentCellContext`, `reposition_camera`; `crates/physics/src/sync.rs` —
`set_kinematic_translation`.
Companion doc: `docs/engine/save-load-roundtrip.md` (cross-cutting trace of this
exact flow, verified against the tree 2026-07-15 — the preflight step below
postdates that doc's last verification pass; cross-check it's still accurate
there too).
**Checklist**:
- **Strict apply ordering.** `execute_pending_save_loads` must run:
  drain slot → `validate_snapshot_types` typed preflight (#3163, added
  2026-08-24 — ABORTS before any teardown on a decode failure) → resolve
  `CurrentCellContext` → teardown (`drain_streaming_state` +
  `unload_current_interior`) → `load_cell_with_masters` → apply lighting +
  `signal_temporal_discontinuity` + record `LoadedPluginSet` → `restore_resources`
  → `build_form_id_remap` → `apply_deltas(MUTABLE_DELTA_COLUMNS)` →
  `apply_player_pose`. Verify `restore_resources` precedes `apply_deltas` so
  `ItemInstancePool` ids that `Inventory` rows reference resolve against the
  RESTORED arena (a delta-before-resource order would dangle every item instance —
  HIGH reference-break). Verify pose-restore is LAST (after the cell reload places
  the player at the default door spawn, and after `apply_deltas` has already
  overlaid the saved `CharacterController` breath/drowning fields onto the
  reloaded body — pose-restore's own momentum-zeroing, below, must run AFTER
  that overlay, not before). Verify a failed `apply_deltas` call also aborts —
  it `return`s (after reconciling dead actors) rather than falling through
  into post-load validation/pose-restore on a partial overlay (fixed
  2026-08-24; prior code merely logged and continued).
- **Remap correctness & identity.** `build_form_id_remap` matches saved
  `FormIdPair` → live entity carrying the same pair in the RELOADED cell, producing
  `saved-id → live-id`. Verify: (a) entities WITHOUT a form id (NIF child nodes,
  particles) are absent from the map and their deltas silently skipped (correct —
  they're respawned identically by the loader); (b) `apply_deltas`/`ApplyFn`
  `filter_map`s out rows whose saved id isn't in the remap (no panic, no
  wrong-entity write); (c) a `FormIdPair` present in the save but NOT in the
  reloaded cell (record removed from a plugin, or cell content changed) is dropped
  with the delta lost — flag whether this is logged so a silently-vanished moved
  object is diagnosable (MEDIUM; data-loss class = reference-break, but arguably
  correct behaviour — the target no longer exists). The player body itself now
  carries a reserved `FormIdComponent` (`PLAYER_FORM_ID_PAIR`, #1846, `91b8c5df`,
  `crates/core/src/form_id.rs`, attached at spawn in `byroredux/src/scene.rs`) so
  it participates in this remap like any NPC instead of being invisible to it —
  verify it stays attached at spawn.
- **Idempotency.** A `load` is `apply_deltas` OVERLAY onto a freshly reloaded
  cell. Loading the SAME slot twice must yield the same world (the teardown +
  reload resets to a clean cell each time). Verify the teardown is unconditional
  (`if streaming.is_some()` drain + `unload_current_interior` always) so a second
  load doesn't stack deltas on a world that already has the first load's deltas.
- **Cell-resolve failure (regression guard, #1697, `3043ffdc`).**
  `validate_cell_loadable` runs a non-destructive pre-flight (parse + cell-lookup,
  `byroredux/src/cell_loader/load.rs`) BEFORE teardown in
  `execute_pending_save_loads`, covering the two named failure modes
  (missing/corrupt ESM, unresolvable cell id) — the current cell survives on
  either. Residual: a failure *after* cell-resolve (mid spawn/GPU-setup) still
  tears down first; that narrower window remains MEDIUM. Confirm the snapshot's
  `CurrentCellContext` is re-validated (it was already verified present by
  `LoadCommand`, but `execute_pending_save_loads` re-reads it and errors if it
  vanished — a defensive double-check; confirm it's there).
- **`AnimationPlayer`/`AnimationStack` exclusion (regression guard, #1696, `92f8f663`).**
  Deliberately excluded from `MUTABLE_DELTA_COLUMNS` — the reloaded cell owns
  their post-spawn state instead of overlaying a stale saved
  `root_entity`/`clip_handle`. Verify they stay excluded; regression test in
  `crates/save/tests/round_trip.rs` asserts both the hazard and the fix.
- **Player-pose restore correctness.** `apply_player_pose`: yaw/pitch always go to
  `InputState` (the source of truth both camera modes rebuild rotation from — a
  saved `Transform.rotation` alone wouldn't survive a tick). Character mode +
  live body → set body `Transform` + `GlobalTransform` translation, zero the
  `CharacterController` momentum (`vertical_velocity` / `is_grounded` /
  `wants_jump`), and `set_kinematic_translation` to sync the Rapier KCC. Verify:
  (a) `set_kinematic_translation` no-ops cleanly without a Rapier handle (returns
  `false`, no panic — guarded by `player_pose_character_tracks_body`); (b) the
  Character-saved-but-no-live-body fallback drops the CAMERA at the saved spot via
  `reposition_camera` (FlyCam reload of a Character save still honours look dir);
  (c) momentum is CLEARED so the body doesn't carry stale free-fall velocity into
  the reloaded cell. A missing momentum-clear = player launches/falls on every
  load (MEDIUM, gameplay correctness). Note the momentum fields this bullet zeroes
  are a DIFFERENT subset of `CharacterController` from the fields
  `MUTABLE_DELTA_COLUMNS`'s `"CharacterController"` entry overlays (#3165,
  2026-08-24) — `apply_deltas` restores `breath_remaining`/
  `drowning_damage_accumulator` (fractional swim/drowning carry, genuine
  gameplay state) onto the SAME struct earlier in the ordering above, and this
  pose-restore step then zeroes only the three motion fields on top. Verify
  that split still holds field-for-field: a future field added to
  `CharacterController` needs an explicit decision about which side of this
  split it belongs on, not a default assumption either way.
- **Pose capture/restore mode mismatch.** `PlayerPose.character_mode` records the
  SAVE-time mode; restore branches on `character_now` alone (#2018, `SAVE-D6-03`)
  — a live Character-mode session always relocates the body, converting the saved
  *camera* position to a body position via eye-height when the pose was captured
  in FlyCam mode. Verify a mode change saved-FlyCam/loaded-Character still
  relocates the body correctly, that saved-Character/loaded-FlyCam falls through
  to the camera-reposition branch, and that a body Transform is never written
  when no body is live.
- **Schema/cell-context guards.** `LoadCommand` refuses a save with no
  `CurrentCellContext` ("loose/exterior save — live load needs an interior cell").
  Verify exterior/loose saves are rejected at queue time, not silently half-applied
  at drain time. Confirm `snapshot_player_pose` returning `None` (pre-refinement
  save) is handled — but note schema-fingerprint drift would reject such a save
  first; confirm that's actually true (a `PlayerPose`-less save has a different
  fingerprint, so `decode` rejects it before pose-restore is reached).
**Output**: `/tmp/audit/save/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/save/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_SAVE_<TODAY>.md` with structure:
   - **Executive Summary** — M45 (crate: snapshot/registry/disk/validate) + M45.1
     (live load-apply, player-pose restore) shipped status, verified against the
     `crates/save/src/lib.rs` docstring's claimed design (full snapshot / atomic
     write / ring / validation gate / off-frame load) — for each claim, state
     CODE-CONFIRMED or DRIFTED. Findings count by severity AND by Data-Loss Class
     (silent-drop / corruption-on-load / irrecoverable-write / reference-break).
   - **Data-Loss Class Matrix** — each finding × class × dimension, so the reader
     sees the silent-drop / corruption surface at a glance.
   - **Completeness Ledger** — the two parallel lists (`build_save_registry`
     registrations × `MUTABLE_DELTA_COLUMNS`), marking each registered column
     SAVED-only vs SAVED+OVERLAID vs structural-identity, to expose any
     save-but-never-replay drift. Cross-check the registered side against the
     SAVE-D1-12 guard's `NOT_SAVED_BY_DESIGN` allowlist (Phase 1 step 6)
     instead of re-deriving it — anything in neither list is the guard's job
     to catch, not this report's.
   - **Findings** — grouped by severity (CRITICAL first), deduplicated.
   - **Regression Guards Discovered** — the existing tests
     (`crates/save/tests/round_trip.rs`, the `save_io.rs` test module, the
     `snapshot.rs` / `disk.rs` `#[cfg(test)]` modules) and which invariant each
     pins, so a future change knows what it'd break.
3. Remove cross-dimension duplicates: the two-list drift is owned by Dim 1
   (pointer from Dim 6); the `form_id_column` heuristic trap is owned by Dim 2
   (pointer from Dim 6's remap checklist); the GPU/physics-handle teardown is
   owned by Dim 5 (pointer from Dim 6's ordering checklist).

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/save`
2. Inform user the report is ready.
3. Suggest: `/audit-publish docs/audits/AUDIT_SAVE_<TODAY>.md`
   (domain label: `save-load`; add `test-gap` for coverage findings and `doc-rot` for
   drifted save/load docs).
