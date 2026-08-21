---
description: "Deep audit of PHYSAL — the Havok→Rapier physics layer: collider classification, fixed-step determinism, 4-phase ECS sync, ragdoll articulation, character controller, WATAL buoyancy sink"
argument-hint: "--focus <dimensions> --game <name> --depth shallow|deep"
---

# Physics / PHYSAL Audit (M28 + M41.x + WATAL Phase 2)

Audit `crates/physics/` + `byroredux/src/ragdoll.rs` — the double-ended physics
layer: per-game Havok authoring on one side, the Rapier3D solver on the other.
Until this skill landed the subsystem had **no owner** — `/audit-concurrency`
checked its locks and `/audit-safety` its `unsafe`, but nothing audited whether
the simulation is *correct*: collider classification, fixed-step determinism,
constraint decode, or the buoyancy sink WATAL now feeds.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication, context rules, and finding format. See
`.claude/commands/_audit-severity.md` for the severity scale. Do NOT duplicate
those here.

## Scope

**Crate**: `crates/physics/src/`
- `crates/physics/src/world.rs` — `PhysicsWorld` resource: pipeline, sets,
  fixed-step accumulator (`PHYSICS_DT`, `MAX_SUBSTEPS`, `SUBSTEP_TIME_BUDGET`),
  the static-scene fast path, query pipeline, `cast_ray` / `cast_ray_down` /
  `cast_capsule_down` / `cast_capsule_down_onto_walkable_surface`,
  `move_character`, `colliders_near_xz`, `static_colliders_aabb`.
- `crates/physics/src/sync.rs` — `physics_sync_system`, the 4-phase (+2.5)
  per-tick sync; `set_linear_velocity`, `set_kinematic_translation`,
  `dump_spawn_collider_census`.
- `crates/physics/src/convert.rs` — glam ↔ nalgebra + `collision_shape_to_parts`
  (the `CollisionShape` → Rapier collider translation).
- `crates/physics/src/components.rs` — `RapierHandles`, `CharacterController`,
  `Ragdoll`.
- `crates/physics/src/config.rs` — `ContactConfig`, `TriMeshFlagBits`.
- `crates/physics/src/ragdoll.rs` — `RagdollSpec` / `RagdollBodySpec` /
  `RagdollConstraintSpec` / `RagdollJointSpec`, `build_ragdoll`,
  `remove_ragdoll`, `body_pose`.
- `crates/physics/src/water.rs` — the WATAL physics sink:
  `PhysicsWaterConstants`, `buoyancy_force`, `submerged_fraction`, current drag.

**Engine-side** (Dimensions 4 + 6): `byroredux/src/ragdoll.rs`
(`template_from_imported`, `activate_ragdoll`, `ragdoll_writeback_system`),
`byroredux/src/systems/character.rs` (the player/character controller driver),
`byroredux/src/commands/scene.rs` (the `ragdoll` console command),
`byroredux/src/commands/water.rs`, and the parse side
`crates/nif/src/import/collision/` (`shape.rs`, `ragdoll.rs`,
`summarize_collision_authoring`).

**Ground truth — read before auditing**:
- `docs/engine/physal.md` — the layer spec. Its central claim is that the
  per-game seam is **only** the constraint CInfo decode; Oblivion/FO3/FNV/Skyrim
  are converged, FO4+ is blocked on the opaque `BhkSystemBinary` payload.
  Verify that claim still holds — a new per-game branch anywhere else in the
  solver path is a PHYSAL doctrine violation, which is the finding class this
  audit exists for.
- `docs/engine/physics.md` — the implementation companion.
- `docs/engine/watal.md` — the water half, and the source of truth for what is
  open. As of the 2026-08-10 checkpoint the physics end **is** built (buoyancy,
  submerged damping, bounded current drag); character swimming, bounded drowning
  damage, splash/ripple markers and underwater audio went live after it. The
  genuinely open items are **water-walking, freezing, the exact Skyrim DNAM tail
  decode, and the cross-game visual smoke matrix** (`docs/engine/watal.md:415-425`).
  Re-read that list rather than trusting this one — it is a snapshot and rots.

**Known-open, do NOT re-litigate**:
- *tes_grounding_zero_mass_dynamic_fix* — Skyrim architecture ships mass=0
  Dynamic-family Havok bodies, reclassified Static (19 → 416 colliders, #1832).
  The mass=0 angle is closed; the door-threshold spawn gap is still open.
- *interior_spawn_point_fix* — interiors spawn at the first door's own
  placement; vanilla `coc` has no auto spawn-point logic. Don't assume one.
- *fnv_furniture_sit_needs_transition* — sit loops have no pelvis/root channel;
  M42 seat-snap is gated behind `BYRO_SANDBOX_SIT` pending that milestone.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated dimension numbers. Default: all 7.
- `--game <name>`: restrict the per-game seam checks (Dim 4) to one title.
- `--depth shallow|deep`: `shallow` = API/contract; `deep` = trace a cell's
  colliders end-to-end from NIF `bhk*` through to the solver. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Shape Translation | Step Determinism & Budget | ECS Sync |
  Ragdoll Articulation | Character Controller | Water / Buoyancy | Queries &
  Diagnostics
- **Trigger Conditions**: what has to be true in a cell for the bug to fire.

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--game`, `--depth`.
2. `mkdir -p /tmp/audit/physics`.
3. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/physics/issues.json`.
4. Read the most recent `docs/audits/AUDIT_PHYSICS_*.md` if one exists; otherwise
   read the PHYSAL/ragdoll dimensions of the most recent per-game reports
   (`/audit-fnv` Dim 7, `/audit-fo3` Dim 5) and of `AUDIT_LEGACY_COMPAT_*`
   (Dim 4) — that is where this subsystem's findings have lived.
5. `cargo test -p byroredux-physics` and record the counts.
6. Re-read the three "known-open" memory items above before writing anything —
   two prior audits re-filed the mass=0 finding after it was fixed.

## Phase 2: Launch Dimension Agents

### Dimension 1: Shape Translation (`CollisionShape` → Rapier collider)
**Entry points**: `crates/physics/src/convert.rs` — `collision_shape_to_parts`,
`iso_from_trs`, the glam↔nalgebra helpers; `crates/physics/src/config.rs` —
`TriMeshFlagBits`, `ContactConfig`; parse side
`crates/nif/src/import/collision/shape.rs`
**Checklist**:
- Every `CollisionShape` variant the NIF importer can produce must have a
  translation arm. Enumerate the variants and the arms and report the delta —
  a silently-dropped shape is an invisible hole in the world, and it is the same
  dispatch/resolve parity trap *nif_shape_dispatch_resolve_parity* describes on
  the parse side.
- Compound / transform shapes: the child transform must compose with the parent
  in the same order the NIF authored it. A transposed compose is a collider in
  the wrong place — visually invisible, physically fatal.
- Scale: Havok authors in Bethesda units; a non-uniform parent scale cannot be
  baked into a Rapier primitive. Verify non-uniform scale is either rejected,
  converted to a TriMesh, or explicitly documented — not silently averaged.
- `TriMeshFlagBits::FIX_INTERNAL_EDGES` transitively ORs in `ORIENTED |
  MERGE_DUPLICATE_VERTICES`; the bit values are pinned against Rapier's own
  `TriMeshFlags` by a test so a Rapier upgrade cannot silently reorder them.
  Verify the pin still exists and still compiles against the current Rapier.
- `default_contact_skin_bu` (1.0 BU ≈ 1.4 cm) is the anti-leak margin for
  TriMesh seams. Verify it is applied at collider creation for *every* collider
  kind, not just TriMesh — an unskinned collider next to a skinned one is where
  a kinematic body tunnels.
- Degenerate input: zero-area triangles, empty vertex sets, NaN in authored
  transforms. Verify each is rejected before reaching Rapier (Rapier will
  happily build a broken BVH).
**Output**: `/tmp/audit/physics/dim_1.md`

### Dimension 2: Step Determinism, Substep Budget & the Fast Path
**Entry points**: `crates/physics/src/world.rs` — `PhysicsWorld::step`,
`PHYSICS_DT`, `MAX_SUBSTEPS`, `SUBSTEP_TIME_BUDGET`, `wake`, `pending_wake`,
`awake_counts`, `update_query_pipeline`
**Checklist**:
- The accumulator clamps to `MAX_SUBSTEPS * PHYSICS_DT` before stepping. Verify
  the clamp precedes the loop (a post-loop clamp re-arms the same backlog next
  frame) and that `frame_dt.max(0.0)` guards a negative/NaN dt.
- **Static-scene fast path**: skipped when `active_dynamic_bodies()` is empty
  **and** `!pending_wake`. The comment is explicit that kinematic bodies are
  deliberately *not* part of the gate (Rapier keeps them in the set for life).
  Verify: every path that can start motion calls `wake()` — spawn, set velocity,
  kinematic push, ragdoll activation, buoyancy application. A missed `wake()` is
  a body that never moves and produces no error.
- The fast path zeroes the accumulator on skip. Confirm that is intended
  (it discards backlog for a scene with nothing to simulate) and that it cannot
  swallow a wake that arrives in the same frame.
- **Anti-spiral budget (#1698)**: the catch-up loop times itself against
  `substep_time_budget` (default `SUBSTEP_TIME_BUDGET == PHYSICS_DT`) and drops
  the remaining backlog rather than re-arming. Verify the timer starts before the
  first substep and that dropping backlog is slow-motion, never a jump.
- **Query-pipeline rebuild is O(all colliders)** and is deliberately kept *out*
  of the substep loop. Verify no new caller re-added an `update` inside the loop,
  and that `update_query_pipeline` runs exactly once per frame before the casts
  that depend on it — a stale query pipeline means the character controller
  casts against last frame's world.
- Determinism: same inputs, same substep count, same result. Flag any use of
  wall-clock time or iteration order over a `HashMap` that feeds the solver
  (the budget timer is allowed — it only truncates; note it as a documented
  non-determinism source rather than a bug).
**Output**: `/tmp/audit/physics/dim_2.md`

### Dimension 3: ECS Sync — the 4(+1)-Phase Tick
**Entry points**: `crates/physics/src/sync.rs` — `physics_sync_system`,
`collect_newcomers`, `register_newcomers`, `push_kinematic`, the buoyancy
phase 2.5 call into `crate::water::apply_buoyancy`, the pull-dynamic phase;
`crates/physics/src/components.rs` — `RapierHandles`
**Checklist**:
- **Lock discipline is the whole design.** Phase 1 collects newcomers under read
  locks and releases them *before* taking write locks on `PhysicsWorld` +
  `RapierHandles`. Verify no read lock survives into the write phase — this is
  the ECS deadlock class from `_audit-common.md` § Rust rules, and it is the one
  place in the codebase where a resource-mut and a storage-write are both needed.
  Cross-reference `/audit-concurrency` Dim 5 but report the trace here.
- Phase ordering is load-bearing: collect/register → push kinematic → buoyancy →
  step → pull dynamic. Buoyancy must apply forces **before** the step integrates
  them; a reorder makes lift lag one frame and look like a bug in the water
  system. Verify the order and that the profiling hooks (`BYRO_PROFILE=1`) still
  label the phases they actually time.
- **Newcomer registration idempotency**: an entity that already has
  `RapierHandles` must never be registered twice (double colliders, doubled
  mass). Verify the collect predicate and that cell unload removes handles —
  `byroredux/src/cell_loader/rapier_release_tests.rs` is the regression guard;
  confirm it still covers the current release path.
- Pull-dynamic writes back to `Transform`, not `GlobalTransform`. Verify the
  write target and that a body parented under a scene node doesn't get its
  parent transform applied twice.
- Kinematic push: `set_kinematic_translation` / `set_linear_velocity` must call
  `wake()` (see Dim 2). Verify both.
- `PhysicsWorld` absent → the whole system early-returns (the loose-NIF demo
  path). Verify no phase panics on that path.
**Output**: `/tmp/audit/physics/dim_3.md`

### Dimension 4: Ragdoll Articulation & the Per-Game Constraint Seam
**Entry points**: `crates/physics/src/ragdoll.rs` — `RagdollSpec`,
`RagdollBodySpec`, `RagdollConstraintSpec`, `RagdollJointSpec`, `build_ragdoll`,
`remove_ragdoll`, `body_pose`, `body_translation`;
`byroredux/src/ragdoll.rs` — `template_from_imported`, `activate_ragdoll`,
`ragdoll_writeback_system`; parse side `crates/nif/src/import/collision/ragdoll.rs`
**Checklist**:
- **The PHYSAL doctrine check.** `docs/engine/physal.md` says the only per-game
  seam is the constraint CInfo decode. Grep the solver-side path for any
  game/version branch (`GameKind`, `bsver`, version constants) and report each
  one: either it belongs in the parse-side decode, or the doc is stale. Both are
  findings, with different fixes.
- Bone→body mapping: every ragdoll body must resolve to a skeleton bone, and the
  writeback must target the same bone. An off-by-one in the constraint's
  parent/child indices produces a ragdoll that explodes on activation.
- Constraint limits (cone/twist/hinge ranges) come from authored Havok data.
  Verify unit handling (degrees vs radians) and that a missing/degenerate limit
  falls back to a documented default rather than an unconstrained joint.
- `ragdoll_extra_angular_damping` is added **on top of** the authored Havok
  damping and defaults to `0.0` (inert). Verify the default is still inert and
  that the addition is applied once per body, not per constraint.
- Activation/teardown: `activate_ragdoll` returns a body count;
  `remove_ragdoll` must release every body, collider and joint it created.
  Verify a repeated activate→remove cycle leaks nothing in the Rapier sets
  (`body_count`, `awake_counts` before/after are the observable).
- Writeback: `ragdoll_writeback_system` drives `Transform` from `body_pose`.
  Verify the Z-up→Y-up conversion happened upstream (NIFAL `coord.rs`) and is not
  re-applied here — a double conversion is a ragdoll lying in the wrong plane.
- FO4+ is blocked on the opaque `BhkSystemBinary` payload; the census helper
  `summarize_collision_authoring` (`crates/nif/src/import/collision/mod.rs`)
  exists so the loader can tell "nothing authored" from "authored but opaque".
  Verify the blocked case is reported as blocked, not as "no collision".
**Output**: `/tmp/audit/physics/dim_4.md`

### Dimension 5: Character Controller & Grounding
**Entry points**: `crates/physics/src/world.rs` — `move_character`,
`CharacterMoveParams`, `CharacterMoveResult`, `cast_ray_down`,
`cast_capsule_down`, `cast_capsule_down_onto_walkable_surface`;
`crates/physics/src/components.rs` — `CharacterController` (and its `HUMAN`
preset); `byroredux/src/systems/character.rs` — `character_controller_system`,
`player_controller_system`, `camera_follow_system`,
`snap_character_body_to_camera`, `toggle_player_mode`, `horizontal_motion`,
`integrate_vertical`
**Checklist**:
- `kcc_offset_bu` (default 4.0) is the KCC skin. Verify it is applied to the
  controller, is larger than `default_contact_skin_bu`, and that the two are not
  independently tuned into an inconsistent pair.
- Walkable-surface classification (`cast_capsule_down_onto_walkable_surface`):
  verify the slope threshold is a named constant with a cited rationale, not an
  inline magic number (`/audit-tech-debt` Dim 7 floor otherwise).
- Grounding vs the door-threshold spawn gap (still open per
  *tes_grounding_zero_mass_dynamic_fix*): confirm the *mechanism* — collider
  present, cast hits, controller grounded — and report where it breaks, without
  re-filing the closed mass=0 finding.
- `integrate_vertical`: free-fall accumulation, terminal-velocity clamp, and
  jump-replaces-velocity are each pinned by tests in
  `byroredux/src/systems/character.rs`. Verify the terminal clamp is applied
  after accumulation and that dt spikes (a stalled frame) cannot tunnel the
  capsule through a floor — this is where the substep budget (Dim 2) and the
  controller meet.
- `horizontal_motion` must not let diagonal input exceed the speed cap (guarded
  by a test). Verify the normalization happens before the speed multiply.
- Input gating: `player_accepts_movement_input` respects the Papyrus
  control/restraint state and UI focus. Cross-reference `/audit-ui` Dim 7 for the
  focus half; verify here that a restrained player cannot move.
- `snap_character_body_to_camera` / `toggle_player_mode` mutate the world
  structurally (`&mut World`). Verify they are only reachable from console /
  exclusive-stage paths, never from a `&World` system.
**Output**: `/tmp/audit/physics/dim_5.md`

### Dimension 6: WATAL Physics Sink — Buoyancy, Damping, Current
**Entry points**: `crates/physics/src/water.rs` — `PhysicsWaterConstants`,
`buoyancy_force`, `current_force`, `submerged_fraction`, `apply_buoyancy`;
`crates/core/src/ecs/components/water.rs` — `WaterPlane`, `WaterVolume`,
`WaterContact`, `SubmersionState`; `byroredux/src/commands/water.rs`
**Checklist**:
- `submerged_fraction(aabb_min_y, aabb_max_y, surface_y)` must clamp to `[0,1]`
  and handle a zero-height AABB without dividing by zero.
- Archimedes lift must be proportional to submerged volume and opposed to
  gravity in the **renderer** frame (Y-up). A frame mix-up here pushes bodies
  sideways. Verify against the constants' documented units (BU).
- **Wake discipline**: the buoyancy phase is documented as never pinning the
  static-scene fast path. Verify it only wakes bodies whose submersion state
  actually changed, and that the `n_new > 0` escape hatch (a body that streams in
  already submerged, spawned asleep) is still present — removing it leaves
  streamed-in bodies sunk on the bottom.
- Current drag must be **bounded**; an unbounded drag term at high flow is a
  body launched out of the water. Verify the clamp and its constant.
- `WaterContact` is the ECS-visible result (`submerged_fraction`, `material`) and
  is documented to emit one transition frame at zero. Verify the transition
  contract holds so downstream FX/audio consumers see the edge.
- Cross-check the canonical side: the tri-state `XCLW` decode (`/audit-esm`
  Dim 5) and the render half (`/audit-renderer` Dim 15). Report the seam once,
  here, with pointers.
- Character swimming and bounded drowning damage **shipped in `c7561d74`
  (2026-08-19)** and are in scope like any other code — `swimlevel_reached`,
  `swim_vertical_velocity`, `advance_breath`, `apply_player_drowning_damage` in
  `byroredux/src/systems/character.rs`. Audit them; do not confirm their absence.
**Output**: `/tmp/audit/physics/dim_6.md`

### Dimension 7: Queries, Diagnostics & Cost
**Entry points**: `crates/physics/src/world.rs` — `cast_ray`, `colliders_near_xz`,
`static_colliders_aabb`, `NearbyCollider`, `PhysicsRayHit`, `body_count`,
`awake_counts`; `crates/physics/src/sync.rs` — `dump_spawn_collider_census`,
`SpawnCensusEntry`; `byroredux/src/commands/scene.rs`
**Checklist**:
- `colliders_near_xz` allocates a `Vec` per call. Find its callers and their
  frequency — a per-frame call on a radius-12 exterior is a hot-path allocation
  (`/audit-performance` Dim 1 floor).
- Ray/capsule casts must exclude the caster's own collider. Verify the filter
  exists on every cast used by the controller (self-hit = permanently grounded
  on your own capsule).
- `dump_spawn_collider_census` is the debugging channel for "why is there no
  floor here". Verify it is reachable from `byro-dbg` and that its output
  distinguishes *no collider authored* from *collider dropped in translation*
  (Dim 1) from *collider present but not walkable* (Dim 5). Those three look
  identical to a user and need different fixes.
- Step cost: the fast path exists because a full `pipeline.step()` on a radius-12
  exterior was ~8–10 ms × up to 5 substeps. Re-measure or cite the current
  number; if the comment's figure is stale, that is doc rot worth fixing because
  it is the justification for the fast path's existence.
- Do not launch a windowed engine instance for measurements
  (`feedback_no_parallel_engine_launch`) — use `cargo test`, `BYRO_PROFILE=1` on
  an existing headless run, or read only.
**Output**: `/tmp/audit/physics/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/physics/dim_*.md`.
2. Combine into `docs/audits/AUDIT_PHYSICS_<TODAY>.md`:
   - **Executive Summary** — findings by severity; an explicit verdict on the
     PHYSAL doctrine claim (is the constraint CInfo decode still the only
     per-game seam?); which games' collision data was actually traced.
   - **Solver Invariant Matrix** — fixed step / wake discipline / lock ordering /
     phase order / teardown completeness, each verified or drifted.
   - **Findings** — grouped by severity, deduplicated.
   - **Known-Open Register** — restate the three don't-re-litigate items and what
     this pass did or did not change about them.
3. Cross-audit dedup: lock ordering → `/audit-concurrency` Dim 5, `unsafe` →
   `/audit-safety`, water rendering → `/audit-renderer` Dim 15, `bhk*` parsing →
   `/audit-nif` Dim 5, shape→`CollisionShape` translation → `/audit-nifal` Dim 6.

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/physics`
2. Inform the user the report is ready.
3. Suggest: `/audit-publish docs/audits/AUDIT_PHYSICS_<TODAY>.md`
   (there is no `physics` domain label — map to `legacy-compat` or `tech-debt`).
