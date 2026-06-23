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

**Crate** (`crates/save/src/`, ~1.2k LOC — read ALL of it before auditing):
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
  `restore_resources`, `build_form_id_remap`, `apply_deltas`.
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

**Cross-cut ground truth — read before auditing the relevant dimension**:
- `byroredux/src/main.rs` — registry/state install at boot (~line 1014), the
  per-frame ordering of `capture_player_pose` THEN `step_save_loads` (~line 2300),
  and `step_save_loads` body (~line 1345).
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
- Live load = reload saved cell via `load_cell_with_masters` → `restore_resources`
  → `build_form_id_remap` → `apply_deltas(MUTABLE_DELTA_COLUMNS)` → `apply_player_pose`.
- `restore_world` (clear + full repopulate) is the **test/loose path**; the LIVE
  load path uses `apply_deltas` overlay, NOT `restore_world` — two divergent
  restore code paths.

**Doc-rot to flag (KNOWN, confirm + report)**: `docs/feature-matrix.md` line ~176
still reads `Save / load (M45) | … | M45 (unstarted)`. M45 + M45.1 shipped (crate,
console commands, live load, player-pose restore). Treat the matrix as a floor;
report the stale "unstarted" row as a LOW documentation finding.

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
4. **No prior save audit exists** (this is the first — `docs/audits/` has no
   `AUDIT_SAVE_*`). Still scan `docs/audits/` for any save/load mention in other
   reports and grep `issues.json` for `save`, `load`, `snapshot`, `corrupt`,
   `FormId` before reporting anything NEW.
5. Read the `crates/save/src/lib.rs` module docstring and the `crates/save/src/snapshot.rs`
   container-layout doc-comment. They state the design intent (atomic write,
   ring, validation gate, off-frame load). For each claim, the matching dimension
   must verify the CODE delivers it — a docstring promise the code doesn't keep is
   itself a finding.

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
  `ItemInstancePool` / `CurrentCellContext` / `PlayerPose` resources). For EACH
  persistent component type in the codebase that carries player-mutable state,
  confirm it is either registered OR documented as reconstruct-on-load (derived
  data: `GlobalTransform`, `WorldBound`; GPU handles: `MeshHandle`,
  `TextureHandle`, `SkinnedMesh`; transient event markers). An unregistered
  *mutable* component = HIGH silent-drop finding.
- **Two lists, one truth (drift hazard).** `MUTABLE_DELTA_COLUMNS` in
  `byroredux/src/save_io.rs` is a SEPARATE hardcoded `&[&str]` from the
  `register_component` calls in `build_save_registry`. The live load only overlays
  columns named in BOTH the registry AND `MUTABLE_DELTA_COLUMNS`. A component
  registered (so it's SAVED) but absent from `MUTABLE_DELTA_COLUMNS` is captured
  to disk yet **never replayed on a live load** — its post-spawn changes are
  silently lost. Verify every mutable column in `build_save_registry` appears in
  `MUTABLE_DELTA_COLUMNS` (or is deliberately structural/identity: `Name`,
  `Parent`, `Children`, the form-id key). Flag any registered-but-not-overlaid
  mutable column as HIGH (silent-drop on load).
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
  `restore_world` only `load`s columns present in the snapshot. Confirm a
  component that legitimately went to ZERO rows between saves doesn't resurrect
  stale rows on the NEXT load (it can't via `restore_world` because
  `clear_entities` runs first — but the live `apply_deltas` path does NOT clear,
  it overlays. Verify the live path can't leave orphaned rows from the reloaded
  cell that the save intended to be gone). Data-Loss Class = corruption-on-load if
  it can.
**Output**: `/tmp/audit/save/dim_1.md`

### Dimension 2: Registry & (De)serialization Fidelity
**Entry points**: `crates/save/src/registry.rs` — `register_component`,
`register_resource`, `register_form_id_component`, the `SaveFn`/`LoadFn`/`ApplyFn`
closures, `schema_fingerprint`, `form_id_column`, `FnvHasher`.
**Checklist**:
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
  load OLD saves silently — verify no save-participating struct carries
  `#[serde(default)]`/`Option` in a way that masks data loss across versions.
- **Fingerprint stability across builds.** `FnvHasher` is hand-rolled
  specifically because `DefaultHasher` is unspecified across std versions. Verify
  the FNV constants (`0xcbf2_9ce4_8422_2325` offset basis, `0x100_0000_01b3`
  prime) are the canonical 64-bit FNV-1a values and that the hash depends ONLY on
  registered names + order — not on any address/TypeId (which would vary per run
  and reject every save).
- **`form_id_column` fragility.** `form_id_column()` returns the first component
  with `apply: None`. Today only `register_form_id_component` sets `apply: None`
  on a component (resources are a separate `Vec`). If ANY future
  `register_component` variant ships with `apply: None`, `form_id_column` returns
  the WRONG column and the entire live-load remap silently mis-keys → every delta
  drops or lands on the wrong entity. Flag this heuristic as a HIGH latent trap;
  recommend keying off an explicit `is_form_id` flag.
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
- **Directory durability gap.** `sync_all` fsyncs the FILE; on most filesystems
  the `rename` itself is not durable until the DIRECTORY is fsynced. A power cut
  after `rename` returns but before the dir entry is flushed can lose the rename.
  Check whether the parent dir is fsynced after rename — if not, flag as MEDIUM
  (the read-back + tmp pattern still protects against half-written content; this
  is the residual rename-durability gap).
- **Ring never clobbers the last good save.** `SaveRing::advance` is round-robin
  over `0..size` (size floored to ≥1); `SaveCommand` with no arg calls
  `ring.advance()`. Verify a quicksave spreads across slots so the previous good
  save survives (the explicit design goal vs. Bethesda's "F5 ate my save"). Note:
  the cursor is IN-MEMORY only (`SaveRing` is not persisted) — on restart the ring
  resets to slot 0 and the FIRST quicksave of a session overwrites slot 0
  regardless of which slot is newest on disk. Flag the session-reset-clobber as
  MEDIUM (data-loss class = irrecoverable-write of slot 0's prior contents).
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
`ValidationKind`; `byroredux/src/save_io.rs` — `SaveCommand::execute` (the gate
caller).
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
- **Coverage vs. claim.** `validate_world` checks exactly THREE reference classes:
  Hierarchy (`Parent`⇄`Children` bidirectional agreement + dangling-id), Equipment
  (`EquipmentSlots` occupant indexes a live `Inventory` row), Animation
  (`AnimationPlayer.clip_handle` resolves in `AnimationClipRegistry`,
  `root_entity` is spawned). Enumerate every OTHER inter-entity/inter-resource
  reference in the saved type set (e.g. `ItemInstancePool` ids referenced by
  `Inventory`, FormId resolvability, any `EntityId` field in a saved component) and
  flag references the gate does NOT cover as MEDIUM defense-in-depth gaps (the gate
  claims to prevent the corruption tail; an unchecked reference class is a hole in
  that claim). The docstring explicitly DEFERS cross-plugin FormId-resolves to the
  binary — verify the binary actually layers that check (`SaveCommand` only calls
  the core `validate_world`; if no binary-side FormId-resolve check exists, the
  deferred check is MISSING, not deferred → MEDIUM).
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
- **Validation runs on SAVE only, not LOAD.** `decode` validates the CONTAINER
  (magic/CRC/version/schema) but `restore_world`/`apply_deltas` do NOT re-run
  `validate_world` on the loaded data. A save written by an OLDER engine (before a
  validation rule existed) or hand-edited within a valid CRC could load a
  referentially broken world. Flag the absent load-side validation as MEDIUM
  defense-in-depth (the corruption-tail thesis is symmetric; loading unvalidated
  data re-introduces it).
**Output**: `/tmp/audit/save/dim_4.md`

### Dimension 5: Frame-Boundary Capture & Off-Frame Apply
**Entry points**: `crates/save/src/driver.rs` — `save_world` (read-only capture),
`restore_world` (`&mut World`); `byroredux/src/save_io.rs` —
`SaveCommand` (read-only), `LoadCommand` (queues), `execute_pending_save_loads`
(the `&mut World` drain), `capture_player_pose`; `byroredux/src/main.rs` run-loop
ordering (~line 2300).
**Checklist**:
- **Capture is read-only and consistent.** `save_world` takes `&World` (queries +
  `try_resource`), so it can run as a console command without `&mut`. Verify the
  capture reads a CONSISTENT world — it must run at a frame boundary, NOT mid-system
  with some storages already mutated this tick. `SaveCommand` runs through the
  console drain; confirm the console drain executes at a point where the scheduler
  is between ticks (no system holds a storage write lock). A capture interleaved
  with a running system would snapshot torn state (e.g. half-propagated transforms)
  — CRITICAL if a system can be mid-mutation during the capture.
- **`capture_player_pose` ordering.** It runs in main.rs AFTER the scheduler's
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
`byroredux/src/cell_loader/transition.rs` — `CurrentCellContext`,
`reposition_camera`; `crates/physics/src/sync.rs` — `set_kinematic_translation`.
**Checklist**:
- **Strict apply ordering.** `execute_pending_save_loads` must run:
  drain slot → resolve `CurrentCellContext` → teardown (`drain_streaming_state` +
  `unload_current_interior`) → `load_cell_with_masters` → apply lighting +
  `signal_temporal_discontinuity` + record `LoadedPluginSet` → `restore_resources`
  → `build_form_id_remap` → `apply_deltas(MUTABLE_DELTA_COLUMNS)` →
  `apply_player_pose`. Verify `restore_resources` precedes `apply_deltas` so
  `ItemInstancePool` ids that `Inventory` rows reference resolve against the
  RESTORED arena (a delta-before-resource order would dangle every item instance —
  HIGH reference-break). Verify pose-restore is LAST (after the cell reload places
  the player at the default door spawn).
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
  correct behaviour — the target no longer exists).
- **Idempotency.** A `load` is `apply_deltas` OVERLAY onto a freshly reloaded
  cell. Loading the SAME slot twice must yield the same world (the teardown +
  reload resets to a clean cell each time). Verify the teardown is unconditional
  (`if streaming.is_some()` drain + `unload_current_interior` always) so a second
  load doesn't stack deltas on a world that already has the first load's deltas.
- **Cell-resolve failure.** If `load_cell_with_masters` errors,
  `execute_pending_save_loads` logs + RETURNS — but it has ALREADY torn down the
  current cell. Verify this leaves the engine in a defined empty-cell state, not a
  half-loaded one (the teardown ran, the reload failed → no cell). Flag the
  "destructive teardown before a load that can fail" as MEDIUM (a corrupt/missing
  ESM on a `load` strands the player in the void with no recovery). Confirm the
  snapshot's `CurrentCellContext` is re-validated (it was already verified present
  by `LoadCommand`, but `execute_pending_save_loads` re-reads it and errors if it
  vanished — a defensive double-check; confirm it's there).
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
  load (MEDIUM, gameplay correctness).
- **Pose capture/restore mode mismatch.** `PlayerPose.character_mode` records the
  SAVE-time mode; restore branches on `pose.character_mode && character_now`.
  Verify a mode change between save and load (saved in Character, loaded in FlyCam
  or vice versa) is handled (falls through to the camera-reposition branch) and
  never writes a body Transform when no body is live.
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
     Explicitly call out the `docs/feature-matrix.md` "M45 (unstarted)" doc-rot.
   - **Data-Loss Class Matrix** — each finding × class × dimension, so the reader
     sees the silent-drop / corruption surface at a glance.
   - **Completeness Ledger** — the two parallel lists (`build_save_registry`
     registrations × `MUTABLE_DELTA_COLUMNS`), marking each registered column
     SAVED-only vs SAVED+OVERLAID vs structural-identity, to expose any
     save-but-never-replay drift.
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
