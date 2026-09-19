---
description: "Deep audit of PHYSAL — the Havok→Rapier physics layer: collider classification, fixed-step determinism + explosion recovery, 4-phase ECS sync, ragdoll articulation, character/NPC kinematic controller, WATAL buoyancy sink"
argument-hint: "--focus <dimensions> --game <name> --depth shallow|deep"
---

# Physics / PHYSAL Audit

Read `.claude/commands/_audit-common.md` (delta-first scoping, dedup, finding format) and `_audit-severity.md` for shared protocol.

Audits whether the simulation is *correct*, not just locked/unsafe: `crates/physics/` + `byroredux/src/ragdoll.rs` + the character/NPC drivers that call the solver. Orchestrator; one Task agent per dimension (max 3 concurrent).

## Scope

- `crates/physics/src/`: `world.rs` (`PhysicsWorld`: step/accumulator, fast path, query pipeline, casts, `move_character`, `capsule_overlaps_solid`, dynamic-body snapshot/recovery), `sync.rs` (`physics_sync_system`, spawn census), `convert.rs` (`CollisionShape` → Rapier), `config.rs` (`ContactConfig`, `TriMeshFlagBits`), `components.rs`, `ragdoll.rs`, `water.rs` (WATAL sink).
- Engine side: `byroredux/src/ragdoll.rs` (+ `ragdoll_installed_tests.rs`), `byroredux/src/systems/character.rs` (player controller, swim, breath), `byroredux/src/systems/locomotion.rs` (NPC KCC step), `byroredux/src/commands/{physics,water}.rs`, `byroredux/src/boot/schedule/physics.rs`; parse side `crates/nif/src/import/collision/`.
- **Handoffs**: NPC *behaviour* on top of locomotion (Wander/Travel/…, stuck re-pick, walk speed, gait) → `/audit-gameplay`; WATR/XCWT→`WaterMaterial` translation → `/audit-exterior`; water shading → `/audit-renderer` Dim 8; `bhk*` byte parsing → `/audit-nif`; `CollisionShape` extraction → `/audit-nifal`; lock order → `/audit-concurrency`.

**Ground truth**: `docs/engine/physal.md` (layer spec), `docs/engine/physics.md`, `docs/engine/watal.md` (water half; re-read its open list rather than trusting a copy here).

**Known-open register** (dated 2026-09-19; check the owning doc/issue before re-filing):
- Skyrim mass=0 Dynamic bodies are reclassified Static (*tes_grounding_zero_mass_dynamic_fix*, #1832 closed). Remaining door-threshold spawn gap is a content/collision-import question, unchanged in `AUDIT_PHYSICS_2026-09-11.md`. Interiors spawn at the first door's placement (*interior_spawn_point_fix*).
- Open: #4134 (KCC/ground-probe capsules use a floor-only `.max(1e-3)` instead of `clamp_shape_extent`), #3477 (`collect_newcomers` rescans every collider row per tick; perf-owned). NIFAL-owned: #4407, #4408.
- Water-walking and freezing are unbuilt (`docs/engine/watal.md`).
- Ball-and-socket / stiff-spring / chain constraints are decoded (#4212) but have no canonical joint kind, so `extract_ragdoll` declines them like `Other` (`docs/engine/physal.md` §3 table). By-design gap; a finding needs occupancy-census evidence that shipped content uses them.

## Parameters

`--focus <dims>` (default all 6) · `--game <name>` restricts Dim 3's per-game seam checks · `--depth shallow|deep` (`deep` traces a cell's colliders from NIF `bhk*` to solver; default deep).

**Extra finding fields**: **Dimension**: Shape Translation | Step & Sync | Ragdoll & Constraint Seam | Character & NPC Controller | Water / Buoyancy | Queries & Diagnostics. **Trigger Conditions**: what a cell must contain for the bug to fire.

## Phase 1: Setup

1. `mkdir -p /tmp/audit/physics`; dedup per `_audit-common.md`. Read the latest `docs/audits/AUDIT_PHYSICS_*.md` (date D = delta baseline).
2. `cargo test -p byroredux-physics` and `cargo test -p byroredux -- ragdoll` / `character` / `locomotion` / `water` (bin crate, no `--lib`); record counts. Note that `byroredux/src/ragdoll_installed_tests.rs` (6 tests) is entirely `#[ignore]` behind FO3 game data: only `-- --ignored` with `BYROREDUX_FO3_DATA` runs it.
3. No windowed engine launches (*feedback_no_parallel_engine_launch*): use `cargo test`, `BYRO_PROFILE=1` on an existing headless run, or read only.

## Phase 2: Dimensions

### Dim 1: Shape Translation
**Paths**: `crates/physics/src/{convert,config}.rs`, `crates/nif/src/import/collision/shape.rs`
**First step**: `git log --since=D -- crates/physics/src/convert.rs crates/physics/src/config.rs crates/nif/src/import/collision/`
**Guards** (default lane): `config.rs::trimesh_flag_bits_match_rapier_definitions` (Rapier flag pin), `convert.rs::huge_finite_*_clamps_to_sane_ceiling` + `non_finite_cuboid_extent_clamps_instead_of_reaching_rapier` (extent clamp). Aim at what they cannot see:
- Every `CollisionShape` variant the importer emits has a translation arm (list variants vs arms, report the delta; same trap as *nif_shape_dispatch_resolve_parity*). A silently dropped shape is an invisible hole.
- Compound/transform children compose in authored order; non-uniform parent scale is rejected, converted to TriMesh, or documented, never averaged.
- Scale is applied exactly once, at the Rapier sink (`docs/engine/physal.md`: producers must not bake scale into vertices/primitives, else scale²).
- Every primitive routes through `clamp_shape_extent` (floor 1e-3, ceiling `MAX_SANE_SHAPE_EXTENT`, non-finite → floor); a new variant with a local `.max(1e-3)` regresses this. #4134 is the known open instance in the controller/probe capsules (Existing).
- `default_contact_skin_bu` (1.0 BU) is applied to every collider kind, not just TriMesh. Degenerate input (zero-area triangles, empty vertex sets, NaN transforms) is rejected before Rapier builds a BVH.
**Output**: `/tmp/audit/physics/dim_1.md`

### Dim 2: Step, Wake & the 4(+1)-Phase Sync Tick
**Paths**: `crates/physics/src/{world,sync,components}.rs`, `byroredux/src/boot/schedule/physics.rs`, `byroredux/src/cell_loader/rapier_release_tests.rs`
**First step**: `git log --since=D -- crates/physics/src/world.rs crates/physics/src/sync.rs byroredux/src/boot/schedule/physics.rs`
**Guards**: `byroredux/src/scheduler_access_tests.rs::physics_sync_declaration_reads_all_phase_and_diagnostic_types` + `contact_config_read_is_declared_on_both_physics_systems` (access declaration; it has been forgotten four times), `cell_loader/rapier_release_tests.rs` (cell-unload handle release). Beyond them:
- Accumulator clamp to `MAX_SUBSTEPS * PHYSICS_DT` precedes the loop; `frame_dt.max(0.0)` guards NaN/negative. The catch-up loop stops at `SUBSTEP_TIME_BUDGET` and drops backlog (slow-motion, never a jump); the budget timer is the one documented non-determinism source, no wall-clock or `HashMap` order elsewhere in the solver path.
- Static-scene fast path (`active_dynamic_bodies()` empty and `!pending_wake`) zeroes the accumulator. Every motion starter calls `wake()`: spawn, `set_linear_velocity`, kinematic push, ragdoll activation, buoyancy. Confirm a same-frame wake cannot be swallowed.
- `wake()` does not guarantee a substep; only `mark_colliders_dirty()` reaches the query pipeline on a no-substep frame. Anything inserting colliders (`build_ragdoll` since #3968) must mark dirty. The BVH rebuild is once per frame, outside the substep loop, before dependent casts.
- **Explosion recovery**: each substep snapshots ALL dynamic-body poses (not just active islands, so a freshly activated ragdoll's first solve is covered; check the per-substep `Vec` cost on a body-heavy exterior); `restore_invalid_dynamic_bodies` reverts any body that is non-finite or moved more than `MAX_DYNAMIC_SUBSTEP_DISPLACEMENT` (2,048 BU) in one substep, sleeps it, logs at error, and zeroes the accumulator. Verify the snapshot precedes every substep, multibody (ragdoll) joints are restored with their bodies, the bound is a delta not a coordinate (far exteriors), and the zeroed accumulator does not eat the frame's wake. Tests: `world.rs::invalid_dynamic_body_reverts_to_its_last_valid_substep_pose`, `finite_but_impossible_dynamic_jump_reverts_to_its_last_valid_pose`.
- Kill plane (`KILL_PLANE_Y`): active dynamic bodies below it are zeroed and slept so unsupported clutter cannot keep the fast path off forever. Verify the constant sits below any real geometry (deep water/exterior lows) and only bodies that actually stepped are scanned.
- Lock discipline: newcomers are collected under read locks that are released before write locks on `PhysicsWorld` + `RapierHandles` (report the trace here; `/audit-concurrency` owns the rule). Phase order is collect/register → push kinematic → buoyancy → step → pull dynamic (buoyancy must precede the step). An entity with `RapierHandles` is never registered twice. Pull-dynamic writes `Transform`, not `GlobalTransform`. Absent `PhysicsWorld` early-returns without panic.
**Output**: `/tmp/audit/physics/dim_2.md`

### Dim 3: Ragdoll Articulation & the Per-Game Constraint Seam
**Paths**: `crates/physics/src/ragdoll.rs`, `byroredux/src/ragdoll.rs`, `byroredux/src/ragdoll_installed_tests.rs`, `crates/nif/src/import/collision/{ragdoll,mod}.rs`, `crates/nif/src/blocks/collision/constraints.rs`
**First step**: `git log --since=D -- crates/physics/src/ragdoll.rs byroredux/src/ragdoll.rs crates/nif/src/import/collision/ragdoll.rs`
**Guards**: `ragdoll.rs::ragdoll_solver_budget_is_local_and_preserves_authored_mass_and_limits`, `build_ragdoll_is_queryable_on_a_zero_substep_frame`, `byroredux/src/ragdoll.rs::activation_before_first_physics_sync_prevents_duplicate_followers`; the FO3 finite-pose tests only run with data (Phase 1).
- **PHYSAL doctrine.** `docs/engine/physal.md` §3 names three source-boundary seams, all at parse→canonical: constraint CInfo decode, `havok_scale_for` (`crates/nif/src/lib.rs`), collision-object-kind dispatch. Grep the solver side (`crates/physics/`, `byroredux/src/ragdoll.rs`) for `GameKind` / `bsver` / version constants: each hit is either parse-side material that belongs upstream or a stale doc; both are findings with different fixes.
- Constraint coverage matches the doc table: Ragdoll / LimitedHinge / Hinge (synthesised ±π) / Prismatic have importers; the breakable wrapper decodes all four; `bhkGenericConstraint` and unknown malleable inners stay `Other`. Byte consumption is exact for the decoded types, so drift telemetry is live for them.
- Ragdoll bodies get `additional_solver_iterations(12)` (local to ragdoll islands, global `num_solver_iterations` untouched, authored masses/limits preserved). Confirm nothing raised the global budget instead.
- Bone→body index mapping (off-by-one explodes on activation); authored limits use consistent units with a documented fallback for degenerate limits; `ragdoll_extra_angular_damping` stays inert (0.0) and is added once per body.
- Activation removes every spec body's `RigidBodyData` recipe even when no follower handles exist yet (restored corpse activated before its first sync), so no duplicate kinematic bodies feed the solver. `remove_ragdoll` releases every body/collider/joint (`body_count`/`awake_counts` before/after are the observable).
- Writeback drives `Transform` from `body_pose`; the Z-up→Y-up conversion happens upstream, not again here. FO4+ is blocked on the opaque `BhkSystemBinary`; `summarize_collision_authoring` must let the loader report "authored but opaque" rather than "no collision".
**Output**: `/tmp/audit/physics/dim_3.md`

### Dim 4: Character Controller, Grounding & the NPC KCC (M42.10)
**Paths**: `crates/physics/src/{world,components}.rs`, `byroredux/src/systems/{character,locomotion}.rs`
**First step**: `git log --since=D -- byroredux/src/systems/character.rs byroredux/src/systems/locomotion.rs crates/physics/src/world.rs`
**Guards**: `world.rs::kcc_filter_groups_mask_actor_bone_colliders` (non-vacuous: the unmasked sweep must be blocked), `capsule_clearance_rejects_door_overlap_and_can_exclude_self`, `capsule_clearance_ignores_sensors_and_live_actor_bones`; `character.rs` tests for `integrate_vertical`/`horizontal_motion`; `locomotion.rs::stuck_tests`. Physics-side contract only; NPC behaviour is `/audit-gameplay`.
- **NPC step**: `step_toward_detailed` drives XZ through `PhysicsWorld::move_character` with `filter_groups: Some(actor_move_interaction_groups())`, which equals `ground_probe_groups()` (masks `ACTOR_BONE_GROUP`, keeps dynamics so NPCs shove clutter). The airborne fallback `cast_ray_down` is origin-lifted by `LOCOMOTION_GROUND_RAY_UP_OFFSET` (256) and clamped to `LOCOMOTION_MAX_DROP` (256 BU) so a ledge cannot teleport the actor under the shell; its `None` exclusion is correct only because the group mask covers the actor's ~18 bone bodies (a bone is a separate body, so a single `exclude_collider` can never cover them). Any new NPC cast without the mask self-hits.
- The `LOCOMOTION_NPC_*` constants are pinned copies: capsule 32/20 must equal `install_fallback_actor_collider` in `byroredux/src/npc_spawn.rs`; `LOCOMOTION_NPC_KCC_OFFSET_BU` (4.0) must equal `ContactConfig::kcc_offset_bu` and keep `> 2 * default_contact_skin_bu`. Nothing reads the resource, so a `ContactConfig` retune silently desyncs them; verify.
- `capsule_overlaps_solid` tests the final placement (spawn ladder, doors) with `solid_probe_filter` (kinematic architecture in; dynamics, sensors, actor bones out); the query pipeline must be current.
- Player: `kcc_offset_bu` applied and above the contact-skin pair; walkable-slope threshold is a named constant with rationale; grounding mechanism (collider present → cast hits → grounded) traced without re-filing the closed mass=0 case; `integrate_vertical` terminal clamp after accumulation and `MAX_FRAME_DT` so a stalled frame cannot tunnel the floor; diagonal input normalised before the speed multiply; `player_accepts_movement_input` honours `PlayerControlState` and `ActorControlState.restrained` (UI focus is enforced upstream: `/audit-ui` Dim 6); `snap_character_body_to_camera` / `toggle_player_mode` take `&mut World` and are reachable only from console / exclusive paths.
**Output**: `/tmp/audit/physics/dim_4.md`

### Dim 5: WATAL Physics Sink: Buoyancy, Damping, Current, Swim
**Paths**: `crates/physics/src/water.rs`, `crates/core/src/ecs/components/water.rs`, `byroredux/src/systems/{character,water}.rs`, `byroredux/src/commands/water.rs`, `byroredux/src/boot/schedule/late.rs`
**First step**: `git log --since=D -- crates/physics/src/water.rs byroredux/src/systems/water.rs byroredux/src/systems/character.rs`
**Guards**: `scheduler_access_tests.rs::water_and_animation_parallel_accesses_are_complete`; `character.rs::{swim_damping_is_frame_rate_independent, zero_dt_tick_while_submerged_does_not_refill_breath, player_water_state_falls_back_to_a_placed_current_volume, swimlevel_predicate_agrees_between_pose_and_depth_forms}`; `water.rs::placed_current_volume_is_collected_without_becoming_a_water_surface`.
- `submerged_fraction` clamps to [0,1] and survives a zero-height AABB. Lift is proportional to submerged volume and opposes gravity in the renderer's Y-up frame (units BU).
- Wake discipline: buoyancy never pins the static-scene fast path; it wakes only on submersion change, and the `n_new > 0` escape (a body streaming in already submerged, spawned asleep) still exists.
- **Two target sources** (#3492): the `RapierHandles`+`RigidBodyData{Dynamic}` scan and the `Ragdoll` scan (`RagdollBuoyancy` caches first collider + effective damping at `build_ragdoll` because activation strips the bones' rows). Both feed the same targets/writes pipeline and `clear_stale_water_contacts` has the matching `Ragdoll` arm, else a corpse leaving water keeps a latched `WaterContact`.
- Current drag is bounded (clamp + named constant). One `reference_point` (collider AABB centre) feeds the union prefilter, current-volume containment and surface XZ containment; a revert to the raw body origin on any axis is the regression.
- `WaterContact` emits one transition frame at zero so FX/audio consumers see the edge.
- **Two samplers must agree**: the dynamic path (`apply_buoyancy_with_scratch`) and the kinematic player's `player_water_state` in `character.rs`. Every water input added to one (plane flow, placed `WaterCurrentVolume` marker, wave height, damage) must exist in the other; plane flow wins over the marker in both (#3974 closed the last gap).
- Swim/drown (`swimlevel_reached`, `swim_vertical_velocity`, `advance_breath`, `apply_player_drowning_damage`) are in scope like any code: damping is dt-correct, zero-dt never credits breath. Water-hazard and drowning deaths reconcile in ONE place, `reconcile_pending_dead_actors_system` (`Stage::Late` exclusive, `byroredux/src/boot/schedule/late.rs`); a second inline teardown site is the regression (#3119).
- Contact→event adapters in `byroredux/src/systems/water.rs` (`Stage::Late` exclusives: submersion → `water_damage_system` → death reconcile → `make_water_interaction_system` → water audio): `submersion_system` reads the camera pose after `camera_follow_system` (guard `scheduler_access_tests.rs::submersion_runs_after_camera_follow_and_before_water_audio`); damage and splash/ripple markers derive from `WaterContact` edges, keeping `crates/physics` free of scripting/presentation deps. Open: #4183 (`submersion_system` samples `TotalTime`/`WindField` under storage guards; concurrency-owned). `compute_underwater_params` (tint) belongs to `/audit-renderer` / `/audit-exterior`.
- Seams reported once, here, with pointers: tri-state `XCLW` decode (`/audit-esm`), render half (`/audit-renderer` Dim 8).
**Output**: `/tmp/audit/physics/dim_5.md`

### Dim 6: Queries, Diagnostics & Cost
**Paths**: `crates/physics/src/{world,sync}.rs` (casts, census), `byroredux/src/commands/physics.rs`
**First step**: `git log --since=D -- crates/physics/src/sync.rs byroredux/src/commands/physics.rs`
**Guards**: `sync.rs::census_excluded_body_prevents_a_self_hit_on_the_players_own_capsule`, `census_reports_full_extent_and_scale_not_just_the_y_range`.
- Every cast the controllers use excludes the caster (self-hit = permanently grounded on your own capsule): the player passes its body handle, actors use the group mask (Dim 4). A live census caller (`phys.census`) must pass `Some(player_body)`.
- The census (`dump_spawn_collider_census`, `phys.stats`/`phys.census` via `byro-dbg`) distinguishes *no collider authored* from *dropped in translation* from *present but not walkable*, and reports full AABB extent + owning placement scale so a scale-drift collider is distinguishable.
- `colliders_near_xz` allocates per call: check caller frequency (a per-frame call on a radius-12 exterior is a hot-path allocation; `/audit-performance` owns the fix).
- Cost model: the dominant per-frame cost is the once-per-frame `QueryPipeline::update` (a full QBVH rebuild over every collider; the in-code proxy measured ~2.1 ms at 30k cuboids, release), not the solver. `world.rs::step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre` pins the fast-path comment against restating the pre-fix "~8-10 ms/step" figure as current; budget from the rebuild, and re-measure with real TriMesh-heavy content before citing a number.
**Output**: `/tmp/audit/physics/dim_6.md`

## Phase 3: Merge

Combine `/tmp/audit/physics/dim_*.md` into `docs/audits/AUDIT_PHYSICS_<TODAY>.md` (header per `_audit-common.md` Report finalization): Executive Summary (findings by severity; PHYSAL verdict = are the three named seams the only per-game branches; which games' collision data was traced), **Solver Invariant Matrix** (fixed step / wake / lock order / phase order / explosion recovery / teardown, each verified or drifted), Findings (deduplicated), **Known-Open Register** (what this pass changed). Then `rm -rf /tmp/audit/physics`; suggest `/audit-publish docs/audits/AUDIT_PHYSICS_<TODAY>.md` (labels `physics`, plus `water` for buoyancy-sink findings and `game:*` when Havok-data-specific).
