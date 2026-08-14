# PHYSAL / Physics Audit — 2026-08-13

**Command**: `/audit-physics` (all 7 dimensions, `--depth deep`)
**Subject**: `crates/physics/src/` + `byroredux/src/ragdoll.rs` +
`byroredux/src/systems/character.rs`, plus the parse-side seam
`crates/nif/src/import/collision/`.
**Baseline**: `cargo test -p byroredux-physics` → **72 passed, 0 failed**.
Rapier **0.22.0** / parry3d **0.17.6** (`Cargo.lock:4385`, `:4050`).
**Dedup basis**: full 2 769-issue set (`gh issue list --state all`) plus every
report in `docs/audits/`. Working tree clean at completion.

> **First pass under a dedicated owner.** No prior `AUDIT_PHYSICS_*` report
> exists. Until this skill landed, physics findings lived scattered across
> `/audit-fnv` Dim 7, `/audit-fo3` Dim 5 and `AUDIT_LEGACY_COMPAT_*` Dim 4 —
> which is why several of the defects below sit *inside* landed, closed fixes
> that nobody re-examined as a subsystem.

---

## Executive Summary

**37 findings: 0 CRITICAL, 5 HIGH, 17 MEDIUM, 15 LOW.**

| Dim | Area | CRIT | HIGH | MED | LOW |
|---|---|---|---|---|---|
| 1 | Shape Translation | 0 | 1 | 2 | 2 |
| 2 | Step Determinism & Budget | 0 | 1 | 2 | 1 |
| 3 | ECS Sync | 0 | 0 | 4 | 2 |
| 4 | Ragdoll Articulation | 0 | 0 | 1 | 4 |
| 5 | Character Controller | 0 | 2 | 1 | 2 |
| 6 | Water / Buoyancy | 0 | 0 | 3 | 3 |
| 7 | Queries & Diagnostics | 0 | 1 | 4 | 1 |
| | **Total** | **0** | **5** | **17** | **15** |

### PHYSAL doctrine verdict

> **Is the constraint CInfo decode still the only per-game seam?**
> **NO — but the doctrine's substance holds completely.** There is **no
> per-game branch anywhere downstream of the parse boundary**. `GameKind` /
> `NifVariant` / `bsver()` appear **zero** times as control flow in all eight
> files of `crates/physics/`, in `byroredux/src/ragdoll.rs`, in
> `byroredux/src/systems/character.rs`, or in `byroredux/src/cell_loader/spawn.rs`.
> What is false is `physal.md` §3's *sentence* claiming one seam: there are
> **three** parse-side seams, and §1 of the same document already enumerates
> them correctly. §3 contradicts §1 within twenty lines. → **PHYS-D4-03** (LOW,
> documentary).

The three real parse-side seams:

1. **Constraint CInfo field order** — `crates/nif/src/blocks/collision/constraints.rs:344`, `:620`
   (`bsver() <= NI_BS_LTE_16`). Correctly sited, correctly documented.
2. **`havok_scale`** — `crates/nif/src/lib.rs:85-103` (`havok_scale_for`, 7.0 vs
   69.99125). Parse-side and data-carried; doctrinally clean, but §3 denies it exists
   while §3's own table depends on it.
3. **Skeleton asset selection** — `byroredux/src/npc_spawn.rs:200-209`
   (`humanoid_skeleton_path(GameKind)`). Asset-path resolution, not physics
   translation; no divergent physics behaviour. Recorded for completeness.

§5's FO4+/FO76/Starfield "AABB-proxy vs static-triangle" split was checked
specifically and is **correctly sited**: `missing_collision_fallback`
(`byroredux/src/cell_loader/spawn.rs:71-90`) keys on
`CollisionAuthoringSummary::needs_packed_havok_fallback()` (a block census, i.e.
what the *file* authored) plus `RenderLayer` — never on a game or version. An FNV
file shipping an NP collision object would take the same arm. The doc's claim is
accurate as written.

### What was actually traced

Collision data was traced end-to-end **structurally** (NIF `bhk*` →
`CollisionShape` → `collision_shape_to_parts` → Rapier collider → solver →
writeback) for the classic-Havok chain (Oblivion / FO3 / FNV / Skyrim), and the
packed-Havok census path for FO4 / FO76 / Starfield. Six dimensions ran
**measured** probes against the real `PhysicsWorld` (throwaway tests, all
reverted) rather than reasoning from code alone — the two HIGH controller
findings, the wake stall, the ragdoll scale collapse and the fog self-hit are all
empirically reproduced, not inferred. **No engine instance was launched**
(`feedback_no_parallel_engine_launch`), so no real-cell telemetry was gathered;
where that mattered it is stated as a limit, not papered over.

### The three themes worth reading first

**1. Scale is dropped at the physics boundary (5 HIGH-to-MEDIUM findings, one root cause).**
Nothing in `crates/physics/` ever reads `GlobalTransform.scale`. The bhk collision
path is the **only** collider producer that does not pre-bake scale — both
synthesized paths (`synthesize_static_trimesh`, `synthesize_packed_havok_proxy`)
correctly do. So every scaled REFR on Oblivion/FO3/FNV/Skyrim gets a wrong-size
collider while its mesh renders correctly. It is worse than a uniform ignore:
`compose_trs` *does* scale each collision part's **position**, so multi-part
assemblies on a scaled REFR spread apart while parts keep original size — real
gaps open between adjacent colliders. The ragdoll path drops it a second time, in
the joint pivots, where the shape-side fix would not reach.
→ **PHYS-D1-01** (HIGH) + **PHYS-D3-03** (registration face of the same defect)
+ **PHYS-D4-01** (pivots, distinct fix).

**2. Three of four casts cannot exclude their caster (1 HIGH, 1 MEDIUM).**
`cast_ray` grew an `excluded_body` parameter for exactly this reason and
documents why. `cast_ray_down`, `cast_capsule_down` and
`cast_capsule_down_onto_walkable_surface` never got it. Consequence: the #2225 /
REN-D16-01 height-fog fix is a **silent no-op in every Character-mode frame** —
the camera sits inside the player capsule by design, `solid = true` returns the
self-hit at toi 0, and the returned value is numerically identical to the
fallback it was meant to replace. The same gap makes AI actors ground-snap to
their own ragdoll-bone colliders and elevator upward.
→ **PHYS-D7-01** (HIGH), **PHYS-D7-02** (MEDIUM).

**3. The wake protocol has an absorbing state above 60 fps (1 HIGH + 2 dependents).**
`step()` clears `pending_wake` **before** the substep loop, but the loop runs zero
substeps when `accumulator < PHYSICS_DT`. The next frame the fast path zeroes the
accumulator. Above 60 fps — the project's own target on an RTX 4070 Ti — a
quiesced scene can never re-reach `PHYSICS_DT`, so one-shot wakes (ragdoll
activation, `apply_impulse`, `SetMotionType`, buoyancy's dry→wet transition) are
swallowed and the simulation stalls indefinitely. Measured: 600 frames at
`PHYSICS_DT/2` after an explicit `wake()` → **0 substeps**; the control at exactly
`PHYSICS_DT` passes. Ragdoll activation is worst-hit: the debug server is
`Stage::Late`, so its wake is *always* consumed by the next frame's step.
→ **PHYS-D2-01** (HIGH), with **PHYS-D2-02** and **PHYS-D6-02** as dependents.

**Why 72 green tests miss all of this**: every one of the 21 `step()` call sites
in the suite passes exactly `PHYSICS_DT` or `100.0` — the entire >60 fps regime is
untested (**PHYS-D2-04**). `PhysicsWorld::move_character` has **zero** unit tests.
`remove_ragdoll` has **zero** tests. The two compound compose-order tests use
`Quat::IDENTITY` only, so a transposed compose would pass them.

---

## Solver Invariant Matrix

| Invariant | Verdict | Evidence / finding |
|---|---|---|
| Accumulator clamps **before** the substep loop | ✅ VERIFIED | `world.rs:346-350`; no post-loop clamp exists |
| Negative / NaN `frame_dt` guarded | ✅ VERIFIED (unpinned) | `world.rs:346` — correct only via `f32::max` NaN semantics; undocumented → PHYS-D2-04 |
| Anti-spiral budget timer starts before first substep | ✅ VERIFIED | `world.rs:382-383`, pinned by 3 tests |
| Backlog drop is slow-motion, never a position jump | ✅ VERIFIED | `world.rs:417-420` — no pose advance, no time-scaled step |
| Fast-path gate excludes kinematics deliberately | ✅ VERIFIED (rationale correct) | `world.rs:364-374` vs rapier `island_manager.rs` |
| **Wake discipline — every motion-introducing mutator** | ⚠️ **DRIFTED** | 8 of 9 sites wake; `remove_body` does not → **PHYS-D2-02** |
| **`pending_wake` survives a zero-substep frame** | ❌ **BROKEN** | cleared pre-loop → absorbing stall → **PHYS-D2-01** (HIGH) |
| Query-pipeline rebuild kept **out** of the substep loop | ✅ VERIFIED (cost drifted) | `world.rs:398-406`; but two rebuilds per streaming frame → PHYS-D2-03 |
| Determinism — no wall-clock / `HashMap` order into the solver | ✅ VERIFIED | budget timer is the only wall-clock read, documented, truncating-only |
| Lock ordering — reads released before writes | ✅ VERIFIED | `sync.rs:504-544` → `:566`; `drop(pw)` at `:661` before `:670`. Residual overlap = **#2404 (OPEN)**, not re-filed |
| Phase order collect → push → **buoyancy** → step → pull | ✅ VERIFIED | `sync.rs:112-149`; forces applied strictly before integration |
| `BYRO_PROFILE` labels match what they time | ✅ VERIFIED | `sync.rs:111-166`, all five brackets correct |
| Newcomer registration idempotency | ✅ VERIFIED (given storage present) | `sync.rs:524-528`; miss path is **PHYS-D3-04** |
| Cell-unload teardown releases every handle | ✅ VERIFIED | `unload.rs:446-479` + 7 live regression tests |
| Ragdoll teardown leaks nothing | ✅ VERIFIED (measured, untested in CI) | 5 build→step→remove cycles → 0 bodies/colliders/multibodies; **PHYS-D4-04** is the coverage gap |
| Z-up → Y-up applied exactly once | ✅ VERIFIED | `import/collision/mod.rs:498-510`; zero `zup_to_yup` hits downstream |
| Bone→body mapping + post-drop constraint remap | ✅ VERIFIED | `byroredux/src/ragdoll.rs:89,144,178-191`, pinned by 3 tests |
| Constraint limits in radians end-to-end | ✅ VERIFIED | no `to_radians`/`PI/180` anywhere on the path; matches Havok + nif.xml |
| `CollisionShape` variant ↔ translation-arm parity | ✅ VERIFIED | 7 variants, 7 arms, **no catch-all** — compiler-enforced |
| `TriMeshFlagBits` pinned against parry's own bits | ✅ VERIFIED | `config.rs:104-119`, byte-matched to parry 0.17.6, test passes |
| **Contact skin applied at every collider site** | ⚠️ **DRIFTED** | `register_newcomers` yes, `build_ragdoll` no → **PHYS-D1-02** |
| **Scale preserved through the collider boundary** | ❌ **BROKEN** | dropped at registration, shapes and pivots → **PHYS-D1-01 / D3-03 / D4-01** |
| **Casts exclude the caster** | ⚠️ **DRIFTED** | `cast_ray` + `move_character` yes; 3 down-probes have no parameter → **PHYS-D7-01 / D7-02** |
| `submerged_fraction` clamped, zero-height AABB safe | ✅ VERIFIED | `water.rs:174-175`, 2 tests |
| Buoyant lift Y-up, proportional to submerged volume | ✅ VERIFIED | `water.rs:101-102` against −686.7 BU/s² |
| Current drag bounded | ✅ VERIFIED | velocity-*matching*, terminal = `flow.speed`; magnitude of the input data is PHYS-D6-03 |
| Buoyancy wakes only on state change; `n_new > 0` hatch intact | ✅ VERIFIED | `water.rs:399-404`, `sync.rs:129-133` |
| `WaterContact` one-transition-at-zero contract | ⚠️ **DRIFTED** | holds on a clean exit, lost on a band exit → **PHYS-D6-01** |

---

## Findings

### HIGH

#### PHYS-D1-01: Uniform `GlobalTransform` scale is silently dropped at collider creation
- **Severity**: HIGH · **Dimension**: Shape Translation · **Status**: NEW
- **Location**: `crates/physics/src/sync.rs:585-587` (+ `crates/physics/src/convert.rs:57-59`);
  producers `byroredux/src/cell_loader/spawn.rs:1064-1090`, `byroredux/src/scene/nif_loader.rs:481-504`
- **Merged**: **PHYS-D3-03** (Dim 3) is the registration-side face of this same
  defect and is folded in here; **PHYS-D4-01** below is a *distinct* sibling
  (joint pivots) that this fix would not cover.
- **Trigger Conditions**: any cell containing a REFR with `XSCL ≠ 1.0` (or a
  collision-bearing `NiNode` with node scale ≠ 1.0) whose NIF carries decodable
  classic `bhk` collision — Oblivion / FO3 / FNV / Skyrim, where scaled rocks,
  rubble and clutter are routine. FO4+/Starfield unaffected (they take the
  synth-trimesh path, which bakes scale).
- **Description**: `spawn_collision_shapes` composes
  `final_scale = ref_scale × coll.scale` into both `Transform` and
  `GlobalTransform`, then `register_newcomers` builds the body from
  **translation and rotation only** (`iso_from_trs(n.global.translation, n.global.rotation)`)
  and hands `collision_shape_to_parts` the *unscaled* shape. Nothing in
  `crates/physics` reads `GlobalTransform::scale`. Rapier exposes
  `SharedShape::scaled` and every primitive here is uniformly scalable, so this
  is a **dropped** value, not an unrepresentable one. The checklist's three
  acceptable outcomes (reject / convert to TriMesh / explicitly document) are all
  unmet. The bhk path is the only one of three collider producers that does not
  pre-bake scale — `synthesize_static_trimesh` multiplies every vertex by
  `world_scale` (`spawn.rs:340-343`) and `spawn_packed_havok_proxy` passes
  `ref_scale` through (`spawn.rs:263`).
- **Impact**: colliders are the wrong size relative to the geometry they
  represent on every scaled placement — a 2× rock has a half-size collider
  (player clips into visible stone), a 0.5× one has an oversized invisible wall.
  **Worse for multi-part collision**: `compose_trs` *does* scale each part's
  position, so a multi-node assembly on a scaled REFR gets its parts spread apart
  while each keeps its original size — literal gaps open between adjacent
  colliders that a KCC or dynamic body passes through. Invisible to `cargo test`:
  no test exercises a non-unit scale through the collider boundary.
- **Suggested Fix**: bake uniform scale at the single `collision_shape_to_parts`
  boundary (multiply primitive dims / vertex sets, scale composed child
  translations during the compound flatten), or wrap each part in
  `SharedShape::scaled`. Pass `GlobalTransform::scale` in explicitly so the drop
  cannot recur silently. Regression test: `ref_scale = 2.0` cuboid emits doubled
  half-extents. State the convention in `docs/engine/physics.md` beside the
  existing `:383-384` note so all three producers document one rule.

#### PHYS-D2-01: A one-shot `wake()` is consumed by a sub-tick frame and the fast path then discards the accumulator — physics stalls indefinitely above 60 fps
- **Severity**: HIGH · **Dimension**: Step Determinism & Budget · **Status**: NEW
- **Location**: `crates/physics/src/world.rs:345-386` — the pairing of line 375
  with lines 371-374 and the loop guard at 386
- **Trigger Conditions** (all four are the *normal* case on the dev box):
  (1) `frame_dt < PHYSICS_DT`, i.e. above 60 fps — `dt` is raw unclamped
  wall-clock (`byroredux/src/app_events.rs:419`); (2) `active_dynamic_bodies()`
  empty — a settled cell, which is the fast path's *design goal*, and
  `register_newcomers` deliberately spawns every Dynamic newcomer asleep;
  (3) motion introduced by a **one-shot** wake rather than a recurring one;
  (4) nothing else re-arms `wake()` later.
- **Description**: `step()` clears `pending_wake` at `:375` **before** the loop,
  but the loop legitimately runs **zero** substeps when
  `accumulator < PHYSICS_DT` (`:386`). The wake is consumed without a step. Next
  frame the island lists are still stale (they only update inside
  `pipeline.step`), `pending_wake` is now false, so the fast path fires and
  **zeroes the accumulator** (`:371-374`) — discarding the sub-tick time that was
  about to cross the threshold. The two behaviours compose into an absorbing
  state: the accumulator resets every frame, can never reach `PHYSICS_DT`, and
  every subsequent `wake()` is swallowed the same way.
- **Evidence** (measured, temporary test against the unmodified crate, deleted):
  ```
  frame 6 (post-wake): steps=0 acc=0.008333334   <- explicit wake(), still 0 substeps
  frame 7 (post-wake): steps=0 acc=0             <- wake already cleared; backlog discarded

  wake_at_high_fps_eventually_steps ... FAILED
    one-shot wake was swallowed: total substeps = 0     (600 frames = 5 s wall time)
  wake_at_60fps_steps ... ok                            (control: dt == PHYSICS_DT)
  ```
  Island staleness confirmed against vendored rapier: `IslandManager::wake_up`
  (`island_manager.rs:93-113`) is the only writer of `active_dynamic_set` and is
  called from `handle_user_changes` **inside** `PhysicsPipeline::step`.
- **Impact**: anything starting motion from a single event never simulates.
  **Ragdoll activation — the PHYSAL headline path — is worst-hit**: the debug
  server is a `Stage::Late` exclusive, i.e. *after* `Stage::Physics`, so its
  `pw.wake()` is always consumed by the next frame's `step()`. The actor stays in
  bind pose until the player moves; `docs/smoke-tests/m41-ragdoll.sh` passes only
  because the operator moves the camera. Also: scripted `SetMotionType` /
  `apply_impulse` / one-shot `set_linear_velocity`, and the first `step()` after
  construction. Blast radius: every game, every cell, whenever fps > 60.
- **Suggested Fix**: move the clear to after the loop, gated on work having
  happened — `if steps > 0 { self.pending_wake = false; }`. A still-armed wake
  bypasses the fast path next frame so the accumulator keeps accruing until it
  crosses `PHYSICS_DT` (2 frames at 120 fps, 17 at 1000 fps). Keep the fast-path
  zeroing as-is. Add the `PHYSICS_DT / 2` regression test from PHYS-D2-04.

#### PHYS-D5-01: Door-spawn walkable probe has a 15 BU blind zone centred on the door's own floor
- **Severity**: HIGH · **Dimension**: Character Controller · **Status**: NEW
  (latent defect *inside* the landed #2013 fix, which is CLOSED and present)
- **Location**: `byroredux/src/scene.rs:990-1022` (probe origins),
  `crates/physics/src/world.rs:694-727`
- **Trigger Conditions**: cold-start spawn where a `DoorTeleport` REFR is
  selected and the real walkable floor top lies within ~15 BU of the door REFR's
  own Y. Since the code's own premise is *"Doors sit at floor level by
  construction"* (`scene.rs:1006-1009`), this is the **normal** case.
- **Description**: rungs 1 and 2 cast a `CharacterController::HUMAN`-sized
  capsule down from `door_pos.y + 50.0`. The capsule half-extent is
  `half_height + radius = 46 + 18 = 64 BU`, so the probe capsule's **bottom
  starts at `door_pos.y − 14`** — already 14 BU *below* the floor it is looking
  for. Any floor at or above `door_pos.y − 15` is either an initial-penetration
  configuration (discarded by `stop_at_penetration: false`, or reported at
  `time_of_impact = 0` with a degenerate normal the walkable filter then rejects)
  or simply outside the search half-space. The `+50.0` bump is 14 BU short of
  well-posed.
- **Evidence** (measured; `hh=46, r=18, origin = door_y + 50, range = 150,
  min_walkable_normal_y = cos(50°)`, door_y = 0, 4 BU slab):

  | true floor top | `cast_capsule_down` | `..._onto_walkable_surface` |
  |---|---|---|
  | `0.0` (door level) | `None` | `None` |
  | `-5.0` | `Some(-14.01)` ← 9 BU lie | `None` |
  | `-14.0` | `Some(-14.00)` | `None` |
  | `-15.0` | `Some(-15.0)` ✓ | `Some(-15.0)` ✓ |

  Control: the identical world probed from `door_y + 150` returns `Some(0.0)`.
  The existing test `walkable_capsule_probe_accepts_floor`
  (`world.rs:1016-1030`) uses `half_height=10, radius=5, origin.y=100` against a
  slab at `y=1` — probe bottom 84 BU *above* the floor, i.e. only the well-posed
  geometry the production call site never produces.
- **Impact**: the door rung never answers on an ordinary flat threshold. Every
  such spawn silently degrades to **rung 3**, the full-cell ceiling sweep the
  code itself documents as unreliable (*"picks up whatever clutter — shelves,
  beams, upper floors — happens to sit anywhere above the nudged XZ, which is
  **not** the floor the door actually opens onto"*). Multi-storey interiors spawn
  the player on the wrong storey or on a beam. The `floor_rung` telemetry
  mis-attributes: an operator reading *"full-cell sweep at nudged XZ"* concludes
  the room isn't flat, when it is flat and the probe was mis-aimed.
- **Suggested Fix**: raise the rung-1/rung-2 origin to at least
  `door_pos.y + (half_height + radius) + margin` (e.g. `+80`, preserving the
  "modest margin above the door, not the cell ceiling" intent) and extend
  `FLOOR_PROBE_RANGE_BU` by the same amount so the searched band is unchanged.
  Add a regression test whose probe capsule starts *penetrating* the target floor.

#### PHYS-D5-02: The grounded `-step_height` probe is not clamped by convex floor colliders — the capsule sinks 32 BU/frame through solid box collision
- **Severity**: HIGH · **Dimension**: Character Controller · **Status**: NEW
- **Location**: `byroredux/src/systems/character.rs:236-252`,
  `crates/physics/src/world.rs:832-906`; rationale at `docs/engine/physics.md:237-242`
- **Trigger Conditions**: `PlayerMode::Character`, `is_grounded == true`, no jump
  this frame, and the supporting collider is a **convex primitive**
  (`CollisionShape::Cuboid` — from `BhkBoxShape` *and* from the synthesized AABB
  proxy at `spawn.rs:246`, which is the FO4+/packed-Havok fallback collider).
  Whether it fires is a knife-edge numerical condition depending on the
  collider's absolute world Y and extents — nothing a content author controls.
- **Description**: while grounded the controller discards integrated vertical
  motion and sends a **fixed −32 BU** desired translation, relying entirely on
  `move_shape`'s swept cast to clamp it. The capsule at rest sits 3.963 BU above
  the surface — *inside* the KCC's `target_distance = offset = 4.0` band — so
  every grounded frame's cast starts "already within target distance". In that
  configuration parry's shape cast against a convex primitive frequently returns
  **no interference**, rapier takes the `else` branch
  (`character_controller.rs:317-322`) and applies the **entire −32 BU**. The
  character passes through the floor, keeps reporting `grounded = true` for 2–3
  more frames (32 BU each), then free-falls out of the world.
- **Evidence** (measured, production parameters, 120 frames at dt = 1/60, 40
  floor heights × 2 slab extents × 2 shape kinds):

  | shape | probe branch | outcome |
  |---|---|---|
  | Cuboid, half 50 | `-step_height` (current) | **sank 20/40** |
  | Cuboid, half 500 | `-step_height` (current) | **sank 28/40** |
  | TriMesh (`FIX_INTERNAL_EDGES` + 1.0 skin) | `-step_height` | sank 0/40 |
  | Cuboid, both extents | pure `v*dt` gravity (probe removed) | sank 0/40 |

  ```
  f0 desired= -0.339 dy= -0.037 y=62.963 feet= -1.037 grounded=true   <- settles
  f1 desired=-32.000 dy=-32.000 y=30.963 feet=-33.037 grounded=true   <- through the floor
  f4 desired=-32.000 dy=-32.000 y=-65.011 feet=-129.011 grounded=false <- free fall
  ```
  Identical relative geometry with the floor top at `0.0` instead of `-5.0` does
  **not** sink — the failure is selected by absolute world Y, which is why it
  reads as intermittent. The stated rationale is also wrong on current rapier:
  `snap_to_ground` is guarded by `translation.dot(&up) < -1.0e-5`
  (`character_controller.rs:370-371`), so on a truly resting frame it does **not**
  engage — the exact thing the probe is documented to guarantee. And
  `check_and_fix_penetrations` (`:182`) is an empty stub.
- **Impact**: the player falls through the floor and out of the world **while
  standing still**, with no in-engine recovery (the kill-plane freezes only
  *dynamic* bodies; the kinematic capsule falls forever). Presents as the same
  black-screen / "0 draws" symptom as closed #2202 and #2013, so it will be
  mis-triaged as a missing-collider problem. TriMesh architecture is immune —
  which is why interiors mostly work and why this survived — but `BhkBoxShape`
  platforms/stairs and **every** synthesized packed-Havok proxy (the
  FO4/FO76/Starfield fallback) are exactly the affected class.
  `PhysicsWorld::move_character` has **zero** unit tests.
- **Suggested Fix**: stop sending an unclamped fixed 32 BU probe. Either clamp it
  to a small multiple of `kcc_offset_bu` (e.g. `-(kcc_offset_bu * 2)`), or keep
  `-step_height` but reject any result whose `translation.y` is more negative than
  the offset while `result.grounded` is still true, treating it as a failed cast.
  Add `move_character` unit tests: a resting capsule on a `Cuboid` floor across a
  sweep of absolute Y values, asserting `|dy| <= kcc_offset`. Correct the
  rationale in `docs/engine/physics.md:237-242` and `character.rs:240-246`.

#### PHYS-D7-01: The per-frame fog ground probe starts inside the player capsule and `cast_ray_down` cannot exclude it — the #2225 height-fog fix is a no-op in every gameplay mode
- **Severity**: HIGH · **Dimension**: Queries & Diagnostics · **Status**: NEW
- **Location**: `crates/physics/src/world.rs:563-588`;
  `byroredux/src/render/mod.rs:42-47` + `:670`;
  `byroredux/src/systems/character.rs:444-453`;
  `crates/physics/src/components.rs:120-127`
- **Trigger Conditions**: `PlayerMode::Character` (interior / exterior grid /
  `--player` — every real content path), any frame after the player capsule is
  registered. Fires on **100 %** of frames thereafter. Does *not* fire in
  `--mesh` / `--tree` / `--fly`, which is exactly why the existing tests and the
  renderer sign-off missed it.
- **Description**: `fog_height_reference` casts down from the camera's world
  position. In Character mode the camera is pinned to `body_pos + eye_height*Y`
  with the same XZ, and `CharacterController::HUMAN` is deliberately sized so the
  eye sits **inside** the capsule — `components.rs:122-123` states the invariant
  and a unit test asserts `eye_height < half_height + radius` (52 < 64).
  `cast_ray_down` passes `solid = true` and filters with
  `QueryFilter::exclude_dynamic()`; the player body is
  `KinematicPositionBased`, which `exclude_dynamic` does **not** filter. Rapier's
  doc for `solid` is explicit: *"if this is `true` an impact at time 0.0 (i.e. at
  the ray origin) is returned if it starts inside of a shape."* A toi of 0 always
  wins the closest-hit search, so `cast_ray_down` returns `cam_pos.y` —
  **numerically identical to the `.unwrap_or(cam_pos.y)` fallback**. The fix
  degrades to the exact pre-#2225 behaviour it was written to remove, with no
  log, no `None`, and no test failure. `cast_ray_down` has **no exclusion
  parameter at all**, unlike sibling `cast_ray`, which grew `excluded_body`
  precisely for this and documents why (`world.rs:590-596`).
- **Evidence** (measured against the real `PhysicsWorld`, reverted):
  ```
  // floor cuboid at y=0 (Fixed) + KinematicPositionBased capsule_y(46,18) centred at y=100
  cast_ray_down from eye (0,152,0) inside capsule -> Some(152.0)   // == origin.y, the fallback
  ```
  The three existing tests (`render/mod.rs:57-110`) construct a world with a
  floor collider and **no player capsule**, so they cannot observe this.
- **Impact**: height fog is anchored to eye level again in every interior and
  exterior cell — `proceduralDensityScale` and `heightFogOpticalDepth` track the
  camera vertically, climbing a hill never clears the fog, and pure vertical
  camera motion changes density at a fixed world point. That is the ghost-band
  failure mode `/audit-renderer` rated HIGH as REN-D16-01 and recorded as fixed
  in `docs/audits/AUDIT_RENDERER_2026-08-03.md:32`. Blast radius: every frame of
  every game.
- **Suggested Fix**: give `cast_ray_down` and both capsule probes the same
  `excluded_body: Option<RigidBodyHandle>` parameter `cast_ray` already carries,
  and have `fog_height_reference` pass the `PlayerEntity`'s
  `RapierHandles::body` — the resolution `interaction.rs:311-322` already performs
  correctly. Regression test: register a kinematic capsule around the camera and
  assert the floor height is still returned.

---

### MEDIUM

#### PHYS-D1-02: `build_ragdoll` never applies `default_contact_skin_bu`
- **Location**: `crates/physics/src/ragdoll.rs:177-185` vs `crates/physics/src/sync.rs:621-630` · NEW
- `ContactConfig::default_contact_skin_bu` (1.0 BU ≈ 1.4 cm) is documented as the
  anti-leak margin that keeps TriMesh seams from leaking the kinematic player
  through. `register_newcomers` applies it to **every** part regardless of shape
  kind. `build_ragdoll` receives the same `&ContactConfig` — it reads
  `ragdoll_extra_angular_damping` from it two lines earlier — but omits
  `.contact_skin(..)` entirely, so every ragdoll collider is built with rapier's
  default skin of `0.0`. `grep -rn "contact_skin"` returns exactly `sync.rs` +
  `config.rs`; the ragdoll site is the only unskinned production path.
- **Impact**: rapier's skin is **additive between the pair**
  (`collider.rs:1002-1008`), so a limb against skinned static world geometry gets
  **half** the intended margin (1.0 instead of 2.0), and two ragdolls against
  each other get **zero** — exactly the "unskinned collider adjacent to a skinned
  one" seam the config exists to eliminate. Self-collision within one ragdoll is
  already suppressed by interaction groups (#2338), so the exposure is
  ragdoll-vs-world tunnelling and ragdoll-vs-ragdoll interpenetration.
- **Fix**: add `.contact_skin(cfg.default_contact_skin_bu.max(0.0))`, or — if zero
  skin is intentional for multibody stability — add an explicit
  `ragdoll_contact_skin_bu` field and say so, making the divergence a decision.

#### PHYS-D1-03: `BhkTransformShape` is the only shape-resolve arm with no finite guard
- **Location**: `crates/nif/src/import/collision/shape.rs:252-259`,
  `crates/physics/src/convert.rs:117-123` · NEW (same class as CLOSED #1409 / #1779, different path)
- Every other arm of `resolve_shape_inner` funnels through `finite()` /
  `finite_vec()` or an explicit `is_finite()` sweep. The `BhkTransformShape` arm
  alone calls `decompose_havok_matrix` and emits its `(translation, rotation)`
  straight into a `Compound` child with **no guard**; the parser reads all 16
  matrix words unvalidated, and `decompose_havok_matrix` only `.normalize()`s
  (which propagates NaN). `flatten_to_parts`'s `Compound` arm then passes
  `(*t, *r)` to `iso_from_trs` unchecked — the same function hardened for
  vertices (#1779) and Cuboid extents (#2543) has no equivalent guard on the
  child TRS. `quat_to_na` uses `new_normalize`, yielding a NaN quaternion.
- **Impact**: a NaN collider isometry gives rapier's broad-phase a NaN AABB —
  the #1779 corruption mode reached through the transform rather than the vertex
  buffer, silently poisoning proximity/ray queries for the entire island, with no
  tiny-ball fallback to absorb it. Vanilla content will not fire it; a mod or a
  mid-stream desync will.
- **Fix**: return `None` when `!translation.is_finite() || !rotation.is_finite()`
  (matching the `ConvexVertices` precedent), plus a release-profile backstop in
  `flatten_to_parts` so the choke point holds for every producer.

#### PHYS-D2-02: `remove_body` does not re-arm the fast path
- **Location**: `crates/physics/src/world.rs:186-197` · NEW
- Every other `PhysicsWorld` mutator that can change contact state calls
  `self.wake()` — `add_force`, `apply_impulse`, `set_motion_type`,
  `set_linear_velocity`, `set_kinematic_translation`, `push_kinematic`, the WATAL
  dry→wet transition, `build_ragdoll`. `remove_body` does not. Rapier *does* wake
  a removed collider's neighbours, but only inside
  `NarrowPhase::handle_user_changes` → **inside `pipeline.step()`**. With the fast
  path engaged there is no step, so the removal is never processed and the
  supported body hangs in mid-air. Live callers: `cell_loader/unload.rs:474`,
  `byroredux/src/ragdoll.rs:393`, `crates/physics/src/ragdoll.rs:482`.
- **Impact**: clutter left floating after the thing it rested on is unloaded or a
  ragdoll deactivated; exterior boundary unloads strand clutter in the
  still-loaded neighbour. Self-heals as soon as anything else wakes the sim —
  hence MEDIUM — but strictly worse combined with PHYS-D2-01, which can make
  "anything else" never happen.
- **Fix**: `let removed = self.bodies.remove(...).is_some(); if removed { self.wake(); }`

#### PHYS-D2-03: Streaming frames pay two full O(all colliders) QBVH clear-and-rebuilds
- **Location**: `crates/physics/src/sync.rs:657-659` + `crates/physics/src/world.rs:455-457` · NEW
- `register_newcomers` refreshes the query pipeline before the step and
  `PhysicsWorld::step` refreshes it again after the substep loop. Nothing between
  reads it: `pipeline.step` is passed `None`, `apply_buoyancy` uses
  `compute_aabb()` not the pipeline, and every cast consumer runs in
  `Stage::Early`/`Update`, i.e. before `Stage::Physics`. `QueryPipeline::update`
  is a **full `clear_and_rebuild`**, not a refit
  (`query_pipeline/mod.rs:348-358`) — the comment calling it "a BVH refit over the
  whole set" understates it. Because the pre-step rebuild happens *before*
  `loop_start` is sampled, it is also **invisible to the `#1698` substep budget**.
- **Impact**: doubles the dominant per-frame physics cost on exactly the frames
  the anti-spiral budget exists to protect. Dim 7's synthetic proxy puts
  `QueryPipeline::update` at ~4× `pipeline.step()` itself, so this is the real
  cost centre.
- **Fix**: drop the `register_newcomers` refresh and gate the post-step one on
  "steps > 0 **or** colliders inserted this frame"; or switch registration to
  rapier 0.22's `update_incremental`.

#### PHYS-D3-01: `pull_dynamic` arms the `Transform` dirty bit for every dynamic body every frame
- **Location**: `crates/physics/src/sync.rs:744-795` · NEW (same class as CLOSED #1374, unfixed here)
- Phase 4 iterates every `RapierHandles` row, filters to `Dynamic`, and pushes an
  update **unconditionally** — no `is_sleeping()` check, no comparison against the
  current `Transform`, no use of `active_dynamic_bodies()` (which the sibling
  `dump_awake_fallers` already demonstrates is available).
  `PackedStorage::get_mut` calls `mark_dirty` on the mere handing out of `&mut`,
  and `Transform::TRACK_CHANGES = true`. N dynamic bodies → N dirty entries per
  frame regardless of motion, which defeats `transform_propagation_system`'s
  `transform_dirty.is_empty()` fast path *and*, transitively,
  `world_bound_propagation_system`'s.
- **Impact**: two O(N log N)-plus-BFS passes per frame that would otherwise
  early-return, on exactly the streamed-clutter population the sleep-on-spawn
  "EXTERIOR-FREEZE FIX" (`sync.rs:592-606`, citing `atw_scheduler=3005ms`) exists
  to keep free. The comment cites ~3 000 dynamics on one Skyrim exterior frame.
  Aggravating: bhk colliders spawn as `MeshHandle`-free ghost entities, so the
  `Transform` being written is consumed by nothing — the per-frame cost currently
  buys zero observable behaviour.
- **Fix**: skip `body.is_sleeping()` bodies and epsilon-compare before
  `get_mut` (mirroring `push_kinematic`'s own gate at `:727-732`); better, drive
  the loop from `islands.active_dynamic_bodies()`.

#### PHYS-D3-02: Phase 4 writes a world-space Rapier pose into a *local* `Transform` on a parented entity
- **Location**: `crates/physics/src/sync.rs:786-794`; producer
  `byroredux/src/scene/nif_loader.rs:493-504` + `:529-537` · NEW
- `register_newcomers` seeds the body from `GlobalTransform`, so rapier's pose is
  world-space by construction; `pull_dynamic` assigns it to the **local**
  `Transform`. Correct for a root entity, wrong for a parented one — propagation
  then composes `parent_global ∘ local`, applying the parent chain twice. The
  cell-loading path avoids this deliberately and names the hazard in a comment
  (`spawn.rs:1099-1104`); the hierarchical NIF loader has no equivalent guard —
  it attaches `CollisionShape`/`RigidBodyData` to the node entity and then
  unconditionally inserts `Parent` on every non-root node. Reachable from
  `cargo run -- mesh.nif`, the headline usage in `CLAUDE.md`/`README.md`, because
  `PhysicsWorld` is inserted unconditionally (`boot.rs:451`).
- **Impact**: the rendered pose diverges from the simulated pose by the full
  parent-chain transform — a fixed offset, no runaway (Phase 2 only reads
  `GlobalTransform` for `Keyframed`, so no feedback loop), but the object is drawn
  in the wrong place for as long as it exists.
- **Fix**: store `parent_global⁻¹ ∘ world_pose` into the local `Transform`, or
  reject parented dynamic bodies at registration with a one-shot warn plus a
  matching guard in `nif_loader.rs`.

#### PHYS-D3-04: `register_newcomers` commits bodies + colliders to Rapier *before* checking the `RapierHandles` storage exists
- **Location**: `crates/physics/src/sync.rs:566-683` (leak window `:607-635` vs check at `:670-679`) · NEW
- The function inserts every newcomer's `RigidBody` and all its colliders into
  `pw`, calls `update_query_pipeline()`, drops the guard, and only *then* asks for
  the `RapierHandles` write query. On `None` it logs and returns — with the rapier
  objects already in the sets and no ECS row pointing at them. Nothing can free
  them: `release_victim_rapier_bodies` walks `RapierHandles` and `Ragdoll` rows,
  and neither exists. It then repeats: `collect_newcomers` only skips when the
  handles query is `Some` **and** `contains(entity)`, so the identical set is
  re-collected, re-cloned (full TriMesh vertex/index data) and re-inserted every
  tick — unbounded growth of `RigidBodySet`, `ColliderSet`, broad-phase and BVH.
- **Severity note**: held at MEDIUM rather than the HIGH the "leak per frame" rule
  implies, because the trigger is a setup error that also emits `log::error!`
  every frame and the shipping binary pre-registers the storage (`boot.rs:504`).
  It is a defense-in-depth ordering bug, not a live production leak.
- **Fix**: hoist the availability check to the top of `register_newcomers`, or
  have `collect_newcomers` return early when the query is `None` (which also
  fixes the re-collect).

#### PHYS-D4-01: Ragdoll joint pivots and collider shapes are scale-blind
- **Location**: `byroredux/src/ragdoll.rs:292-330`,
  `crates/physics/src/ragdoll.rs:150-187` + `:331-410` · NEW
- **Sibling of PHYS-D1-01, distinct fix.** `activate_ragdoll` composes the body
  world seed **with** the live bone scale and snapshots `scale: gt.scale` for the
  writeback inverse only. It never applies that scale to `RagdollBodySpec::shape`
  or to the `RagdollJointSpec` pivot vectors, which are carried verbatim in
  NIF/`havok_scale` units. `build_ragdoll` locks all three linear DOF and builds
  `local_frame1`/`local_frame2` from those unscaled pivots. Because the joint is a
  **multibody** (reduced-coordinate) joint, forward kinematics *defines* the child
  link's translation — so the animated, scaled separation the seed established is
  discarded on the first step and replaced by the bind-scale one.
  `RagdollBodySpec::scale` is threaded all the way to `build_ragdoll`, so the
  value is **dropped, not unavailable**.
- **Evidence** (measured, throwaway probe, deleted): two bodies seeded 100 units
  apart (a 2× actor whose authored pivots are ±25), gravity zeroed —
  `after 1 step: separation 100 -> 50`.
- **Impact**: a scaled NPC (child / creature / giant REFR, or any mod that
  rescales an actor) collapses to bind-scale skeleton proportions the frame it
  ragdolls, while `ragdoll_writeback_system` keeps writing `gt.scale = seed_scale`
  onto the bones — so the skinned mesh renders scaled-up bones packed at
  unscaled-apart positions: a visibly crushed, interpenetrating corpse rather than
  a crumple. No console workaround.
- **Fix**: multiply every `RagdollJointSpec` pivot by the seed-time scale inside
  `activate_ragdoll` (the single translate boundary — **not** in `build_joint`,
  which must stay unit-agnostic), and pass the same scale into
  `collision_shape_to_parts` alongside PHYS-D1-01. Axes (`twist_*`/`plane_*`/
  `axis_*`/`perp_*`) are unit directions and must stay unscaled.

#### PHYS-D5-03: Runtime door transitions bypass the entire spawn-grounding ladder
- **Location**: `byroredux/src/app_step.rs:709-721`,
  `byroredux/src/cell_loader/transition.rs:337-348` + `:411-423`,
  `byroredux/src/systems/character.rs:531-595` · NEW (defect in the landed #1874 fix)
- `reposition_camera` places the **camera** at the raw Y-up-converted XTEL
  destination; `snap_character_body_to_camera` then places the capsule at
  `cam_pos - Vec3::Y * eye_height`, i.e. **feet at `dest.y − 116`**. XTEL
  destinations are at floor level — the premise the cold-start ladder is built on
  — and the cold-start path consequently places the capsule *centre* at
  `floor_y + 68`. The two paths disagree by **120 BU** for the same door, and the
  transition path runs **no** ground probe, no walkable-normal check, and no
  `is_grounded` verification. `snap_character_body_to_camera` is correct for its
  original caller (`toggle_player_mode`, where the camera genuinely *is* at eye
  height); #1874 reused it where the camera had just been set to a floor-level
  door pose, inheriting the eye-height subtraction.
- **Impact**: after every door walk the capsule starts deeply embedded in the
  destination floor. `check_and_fix_penetrations` is a stub and the body is
  kinematic, so nothing pushes it out: the character either sticks
  blocked-and-ungrounded (the #2193 failure mode) or, given PHYS-D5-02, falls
  through. Symptom is the view sinking into the floor on arrival.
- **Fix**: route transition arrival through the same grounding code as cold start
  — probe the destination XZ with `cast_capsule_down_onto_walkable_surface` (with
  PHYS-D5-01's corrected origin), place the body via `character_spawn_center_y`,
  and let `camera_follow_system` derive the camera from the body rather than the
  body from a floor-level camera pose.

#### PHYS-D6-01: Waterline-band exit permanently loses the wet→dry restore
- **Location**: `crates/physics/src/water.rs:390-459`, `:183`, `:317-321` · NEW
- The containment predicate accepts a body whose collider AABB bottom sits up to
  `WATERLINE_HYSTERESIS` (4 BU) **above** the surface, while `submerged_fraction`
  returns exactly `0.0` for any `min_y >= surface_y`. So the band
  `surface_y <= min_y <= surface_y + 4` yields `Some(surface)` **and**
  `frac == 0.0`. In that state the `if frac > 0.0` guard skips the whole
  body-mutation block — authored damping is **not** restored and `reset_forces` is
  **not** called (rapier forces persist until explicitly reset) — yet a
  `WaterContact { submerged_fraction: 0.0 }` **is** still written, which clears
  `prior_wet`, which then gates off the `None` arm's restore **forever**.
- **Impact**: three persistent, non-self-healing consequences — a stale upward
  force (up to a full gravity-cancelling force on an abrupt exit: the body hovers
  or creeps), damping pinned at 1.5 instead of the authored 0.0 (permanently
  sluggish out of water), and `WaterContact` contract drift (`material: Some(..)`
  at zero fraction, against a documented `None`). It also weakens the documented
  "buoyancy CANNOT pin the sim" guarantee for thin floats whose `min_y` oscillates
  across the surface. **The entire exit path is untested** — the eight water tests
  cover entry, equilibrium, sleep and pure math, never an exit.
- **Fix**: treat `frac == 0.0` inside the band as the dry case — move the restore
  into a shared `frac == 0.0` path and write `WaterContact::default()` — or make
  the band entry-only by gating it on `t.prior_wet`.

#### PHYS-D6-02: Buoyancy's one-shot dry→wet wake is swallowed above 60 fps and never re-armed
- **Location**: `crates/physics/src/water.rs:272-277`, `:399-404`, `:471-473` ·
  NEW (interaction; **root cause is PHYS-D2-01** — do not double-fix)
- What is specific to buoyancy is that its wake is a **latched one-shot** and its
  own fast path then locks the state in: frame N the streamed-in submerged body
  wakes and writes a wet contact; `step` consumes `pending_wake` and runs 0
  substeps, so `awake_counts().0` is still 0; frame N+1 `prior_wet` is true so no
  new wake fires, the quiesced guard
  (`awake_counts().0 == 0 && !pending_wake() && !had_newcomers`) is satisfied and
  `apply_buoyancy` returns early, and `step`'s fast path zeroes the accumulator.
  The pair is self-sustaining: the body is non-sleeping but out of the island set,
  buoyancy refuses to look at it, and the step refuses to run.
- **Impact**: a body streaming into a cell already submerged — the exact case the
  `n_new > 0` escape hatch exists for — freezes mid-water-column. In normal play
  the player's kinematic capsule re-arms `wake()` on the next frame it moves, so
  it is a visible hang rather than permanent. It **is** permanent for a parked
  camera — including `--bench-hold` / `byro-dbg` runs, which is precisely how
  WATAL §7 Phase 2's remaining real-data GPU smoke gate would be executed. That
  gate could report "buoyancy doesn't work" for a reason that isn't buoyancy.
- **Fix**: fix PHYS-D2-01. If deferred, harden locally by re-arming when any
  target's rapier body is non-sleeping while `awake_counts().0 == 0`.

#### PHYS-D6-03: `WaterFlow.speed` carries the raw WATR `wind_speed` with no unit conversion (SEAM)
- **Location**: `byroredux/src/env_translate.rs:475-488`, consumed at
  `crates/physics/src/water.rs:147` and `crates/core/src/ecs/components/water.rs:214-218` · NEW
- `WaterFlow::speed` is documented as "World units per second. Typical: 0.5 (calm
  river) … 25.0 (waterfall sheet)". The single translate site assigns it
  `rec.params.wind_speed.abs().max(0.5)` — the WATR wind-velocity float copied
  verbatim, no scale factor, no documented unit at the parse boundary. The
  **same** scalar is then used as a shader scroll rate in the next lines
  (`mat.scroll_a = [cos·speed·0.5, sin·speed·0.5]`, against a `scroll_a` default
  of `[0.020, 0.011]`). A value that is simultaneously a ~0.02-magnitude UV scroll
  rate and a 0.5–25 BU/s world velocity cannot be dimensionally correct in both.
- **Impact**: if WATR wind velocity is the small normalised float the `scroll_a`
  defaults imply, the `.max(0.5)` floor pins every real river at ~0.5 BU/s ≈
  7 mm/s and authored currents are effectively inert; if it is large on some
  records, an unclamped `speed` is the unbounded terminal velocity clutter
  converges to. **Vanilla WATR values were deliberately not verified on disk** —
  that is the disproof step this finding leaves open, per the no-guessing rule.
  What is proven from code alone is the two-consumers-one-scalar inconsistency and
  the total absence of unit documentation, conversion, or clamp.
- **Seam owners**: decode side `/audit-esm` Dim 5 (WATR `DATA`/`DNAM` semantics);
  `scroll_a` consumer `/audit-renderer` Dim 15. **Reported once, here.**
- **Fix**: establish the unit from the Gamebryo 2.3 / nif.xml / UESP reference
  first, then either apply an explicit BU/s conversion at the single
  `resolve_water_material` site and derive `scroll_a` from the canonical
  `WaterFlow`, or stop feeding the field to `WaterFlow.speed` and synthesize the
  current from a documented constant × `WaterKind`. Clamp to 0.5–25 BU/s either way.

#### PHYS-D7-02: `step_toward`'s ground-snap ray self-hits the actor's own keyframed ragdoll-bone colliders
- **Location**: `byroredux/src/systems/locomotion.rs:49-70`,
  `crates/physics/src/world.rs:563-588`, `byroredux/src/npc_spawn.rs:169-198` · NEW
- `step_toward` ground-snaps by casting down from `current.y + 256` with
  `cast_ray_down`, whose only filter is `exclude_dynamic`.
  `keyframe_live_ragdoll_bones` (#1698) deliberately flips every live actor's
  ragdoll bone from `Dynamic` to `Keyframed` before first registration, so each
  bone registers as a `KinematicPositionBased` body with a real collider — its own
  doc says "~18 bones/NPC" and "~480+ across a dense interior".
  `exclude_dynamic` does not filter kinematic bodies, and the ray origin is
  directly above the actor's root, so the first thing it meets is the actor's
  **own** upper-body bone. There is no local fix available: `cast_ray_down`
  accepts no exclusion, and `step_toward` receives neither the actor's `EntityId`
  nor its `RapierHandles`.
- **Evidence** (measured): floor at `y=1.0` plus a `KinematicPositionBased`
  `ball(6)` at `y=120` (stand-in for a bone) → `locomotion ground ray -> Some(126.0)`.
- **Impact**: the actor is re-seated each tick at its own bone height. Because the
  bones are driven from the actor's `GlobalTransform` via `push_kinematic`, the
  whole rig rises with the root, so the next tick's ray hits the bone from an even
  higher origin — a **monotonic elevator**, not a one-off offset. Also silently
  corrupts every Travel/Escort arrival test depending on real ground Y. Reachable
  only via the six env-gated locomotion systems, which is the only reason this is
  not higher.
- **Fix**: thread the actor's handle into `step_toward` and pass it to an
  exclusion-aware `cast_ray_down` (same signature change PHYS-D7-01 needs).
  Excluding one body is insufficient — each bone is a separate body — so use a
  `QueryFilter` predicate rejecting colliders under the actor's skeleton root, or
  a collision-group bit for "actor bone" that ground probes mask out.

#### PHYS-D7-03: The spawn census cannot separate the three causes it exists to separate
- **Location**: `crates/physics/src/sync.rs:375-476`; call site `byroredux/src/scene.rs:1113-1128` · NEW
- **(a) no collider authored vs (b) dropped in translation** collapse onto one
  bucket — the summary log's *"0 total ⇒ the collider never spawned"* **is** the
  conflation. The engine already computes the discriminator and the census never
  consults it: `summarize_collision_authoring` / `CollisionAuthoringSummary` is
  retained on `CachedNifImport` and `docs/engine/physics.md:330-338` states its
  whole purpose is that "an empty decoded-collision array no longer conflates
  'intentionally no collision' with 'packed collision exists but is undecodable'".
  The census reads the rapier side only, re-introducing exactly the conflation
  that summary was built to remove.
- **(c) present but not walkable** is invisible: all three rungs call
  `cast_capsule_down_onto_walkable_surface`, which returns `None` for *both* "hit
  nothing" and "hit something whose `normal1.y` failed the walkable test". The
  normal is computed then discarded — the surface-and-normal helper is private.
  So a spawn blocked by a 60° ramp logs "MISS on all 3 rungs" plus a census
  showing `Fixed>0`, and the summary line instructs the reader to conclude
  *"Fixed>0 at a wrong Y ⇒ transform composition"*. **The diagnostic actively
  mis-attributes a walkability rejection to a transform bug.**
- **Fix**: make `cast_capsule_down_surface_and_normal` `pub` and on failure re-run
  it unfiltered so the log can distinguish "hit y=… normal_y=… → REJECTED as
  non-walkable" from "no hit"; add the cell's `CollisionAuthoringSummary` totals
  to the census header so `0 total` splits into "nothing authored" vs "N authored,
  none registered".

#### PHYS-D7-04: The census sorts by absolute world Y and truncates at 24 — the floor is what gets cut
- **Location**: `crates/physics/src/world.rs:761-817`,
  `crates/physics/src/sync.rs:335-338` + `:455` · NEW
- `colliders_near_xz` takes `(x, z, radius)` and **no Y**. It sorts descending by
  AABB centre Y and its doc claims this is "so the nearest thing above the probe
  reads first" — but there is no probe Y in scope, so the key is really "highest
  in the world column". The census then prints only the first 24. The question
  being asked is *"is there a floor at/below the spawn?"*, whose answer lives at
  the **low** end of the sort. In a Skyrim inn the 24 shown entries are the roof
  and upper floor; the spawn-height geometry falls under "N further colliders
  omitted". The very cell shape the doc calls out — *"2560 fixed colliders and a
  hole exactly under the player's spawn"* — is the worst case for this ordering.
  The pinning test uses three slabs, so it can never observe the interaction.
- **Fix**: pass the probe origin Y through and sort by `|centre_y − probe_y|`
  (what the doc already promises), or keep the descending sort and take 12 from
  each end. Fix the doc sentence either way.

#### PHYS-D7-05: `dump_spawn_collider_census` is unreachable from `byro-dbg`
- **Location**: `crates/physics/src/sync.rs:397`; sole call site
  `byroredux/src/scene.rs:1120-1128`; registry `byroredux/src/commands/mod.rs:52-107` · NEW
- The census is public and re-exported but gated behind a single
  `if floor_probe_failed` inside `setup_scene`'s door-teleport branch —
  boot-time, failure-only, one call site in the whole workspace.
  `build_command_registry` registers 50+ commands and **none** touch
  `PhysicsWorld`'s query surface: `colliders_near_xz`, `static_colliders_aabb`,
  `cast_ray_down`, `cast_capsule_down*`, `body_count` and `awake_counts` have zero
  console exposure. So there is no way to answer "what collision is under *this*
  point" while the engine runs — which is the operator's actual situation, since
  the failure is noticed by falling, not by reading frame-0 logs. The sibling
  `dump_awake_fallers` is one-shot per **process** and env-gated, so both physics
  diagnostics are effectively boot-time-only.
- **Precedent**: **#518 (CLOSED)** — "tex.missing / tex.loaded / mesh.cache /
  mesh.info unreachable via byro-dbg" — established that a diagnostic without a
  console entry point is a defect in this repo, not a nice-to-have.
- **Fix**: register `phys.census <x> <z> [radius]` (defaulting XZ to the
  player/camera) plus `phys.stats` surfacing `body_count` / `awake_counts` /
  `static_colliders_aabb`. Both are pure reads of already-`pub` API;
  `water.contacts` is the existing template. Worth fixing together with
  PHYS-D7-03/04 so the exposed command is worth having.

---

### LOW

| ID | Title | Location |
|---|---|---|
| **PHYS-D1-04** | Compound compose-order tests use `Quat::IDENTITY` only — pure translations commute, so a reversed/transposed compose would pass them. Production code is correct but unguarded; no test anywhere exercises a non-identity rotation through a nested compound. | `crates/physics/src/convert.rs:349-412` |
| **PHYS-D1-05** | `collision_shape_to_parts` documents a TriMesh "construction failed → tiny ball" fallback that **cannot exist** (`trimesh_with_flags` returns `Self`; `TriMesh::with_flags` panics on an empty index buffer), and does no index-range check at its own documented "single choke point every TriMesh source passes through" — the two producers each carry their own copy instead. | `crates/physics/src/convert.rs:84-85` vs `:191-217` |
| **PHYS-D2-04** | Every one of the 21 `step()` test call sites passes exactly `PHYSICS_DT` or `100.0` — the entire >60 fps regime is untested, which is why PHYS-D2-01 ships green. Also: the NaN guard relies on unpinned `f32::max` semantics (a refactor to `f32::maximum` would wedge the loop forever), and the "at least one substep always runs" doc claim is false outside the budget bail-out. | `crates/physics/src/world.rs:333-346`, `:909-1534` |
| **PHYS-D3-05** | Phase 2.5 (buoyancy) is absent from both the `sync.rs` module doc and `docs/engine/physics.md` — both say "four phases". Its *position* is the correctness property. Separately, the doc's "loose-NIF viewer opt-out" premise is **false** in the shipping binary (`boot.rs:451` inserts `PhysicsWorld` unconditionally) — that stale premise is what makes PHYS-D3-02 reachable. | `crates/physics/src/sync.rs:1-14`; `docs/engine/physics.md:88-141` |
| **PHYS-D3-06** | Per-frame `env::var_os` probes documented as "Zero cost when the flag is unset" — the lookup itself is the cost (process-wide environ lock), not the branch. | `crates/physics/src/sync.rs:104`, `:170-173` |
| **PHYS-D4-02** | The FO4+/FO76/Starfield packed-Havok ragdoll blockage is **silent**: `extract_ragdoll` bails at `has_constraint_authoring` with no log, `nif_loader.rs` only logs on success, and the console reports "has no RagdollTemplate" — byte-identical to a rock. `summarize_collision_authoring` exists exactly to distinguish these and the ragdoll path never consults it (the *collider* path does, correctly). `physal.md` §5's promise "documented limitation, **not** a silent leak" is not kept at runtime. | `crates/nif/src/import/collision/ragdoll.rs:39-44`; `byroredux/src/scene/nif_loader.rs:1159-1172` |
| **PHYS-D4-03** | `physal.md` §3's "the whole per-game seam is the typed decode of two constraint CInfos" contradicts §1 of the same document (which correctly names three) and is contradicted by §3's own table, which depends on `havok_scale_for`. Left unfixed it will keep producing stale-premise findings in **both** directions. | `docs/engine/physal.md:111-113` vs `:51-54` |
| **PHYS-D4-04** | **Zero** tests call `remove_ragdoll` (3 grep hits: definition, doc ref, one production call site), and nothing pins `ragdoll_extra_angular_damping`'s inert 0.0 default or its once-per-body application. Both behaviours verified correct by probe (5 cycles → 0 leaked bodies/colliders/multibodies), but #1531 is exactly the regression this gap would hide, and §4 calls the damping dial "the biggest 'less floppy than Havok' lever". | `crates/physics/src/ragdoll.rs:475-485`, `:160-162`; `crates/physics/src/config.rs:131-136` |
| **PHYS-D4-05** | **Issue hygiene — #2339 is OPEN but was fixed on 2026-08-07.** All four silent `extract_ragdoll` drop sites now carry `log::warn!` in the #1539/#1850 house phrasing; `git log -S` points at commit `8ee151e0`, four days *after* the issue was filed. **Should be closed.** | `crates/nif/src/import/collision/ragdoll.rs:58-226` |
| **PHYS-D5-04** | No invariant pins `kcc_offset_bu > default_contact_skin_bu`. Defaults are consistent today (4.0 > 2×1.0) but the existing test asserts only `== 4.0` and `>= 0.0`, and `capsule_center_y_on_surface` computes spawn height from `kcc_offset_bu` alone, ignoring the collider skin — so an inverted pair spawns the capsule inside the skin-inflated floor (the #2193 configuration). The module doc explicitly invites single-field re-tuning. | `crates/physics/src/config.rs`; `byroredux/src/scene.rs:137-156` |
| **PHYS-D5-05** | `integrate_vertical` tests hardcode `-1373.4` / `380.0` with comments claiming to pin `CharacterController::HUMAN`; the live preset is `-1220.8` / `506.6667` (retuned for 2× jump height). The asserted link does not exist in code, so a preset change cannot break these tests. Related nit: `character_controller_human_dimensions` compares a BU/s velocity against a BU/s² acceleration. | `byroredux/src/systems/character.rs:691-726`; `crates/physics/src/components.rs:195-198` |
| **PHYS-D6-04** | `WaterContact::depth` is measured from the **body origin**, not the collider AABB centre its doc promises — and the AABB is already in hand two lines earlier. Wrong for every body whose collider is offset from its origin, which is the norm for compound bhk shapes and ragdoll bones. Cosmetic today, but `depth` is the field the not-yet-built drowning/underwater-FX gate is documented to consume, so the error would be inherited rather than discovered. | `crates/physics/src/water.rs:360` + `:437` |
| **PHYS-D6-05** | The two ends of WATAL disagree on which overlapping water plane wins: physics takes `Vec::find` (ECS storage order, non-deterministic across cell loads), the camera explicitly takes the *nearest*. With overlap, a body and the camera at the same spot get different `surface_y`, `WaterMaterial` and `WaterFlow`. Real content rarely overlaps today, but a cell-transition frame with both planes live produces exactly this. | `crates/physics/src/water.rs:376-388` vs `byroredux/src/systems/water.rs:136-143` |
| **PHYS-D6-06** | `PhysicsWorld::add_force` / `apply_impulse` / `reset_forces` — documented by `water.rs:15-17` as the WATAL "force application path" — have **no production caller**. `apply_buoyancy` bypasses them deliberately because they hard-wire `wake_up = true` + `self.wake()`, which would defeat the wake discipline the module is built around. `apply_impulse` is the documented hook for the unbuilt splash kick, so the mismatch would be inherited by Phase 3. | `crates/physics/src/world.rs:246-294` |
| **PHYS-D7-06** | The fast path's justification comment cites, in the present tense, a cost model the same function no longer pays (~8–10 ms/step incl. a per-substep query-pipeline rebuild it now passes `None` for). `git log` shows the figure and its invalidation landed in the **same** commit, `6e55b492` (~45 ms → ~0.02 ms). Also calls rapier's `clear_and_rebuild` a "refit" at two sites. Synthetic proxy (30 k fixed cuboids, release): `step` = 2.82 ms of which `QueryPipeline::update` = **2.22 ms** — the rebuild dominates `pipeline.step()` ~4:1, the inverse of what the comment attributes the cost to. | `crates/physics/src/world.rs:352-368`, `:398-406`, `:452-457` |

---

## Known-Open Register

The three items the skill forbids re-litigating, and what this pass did with them.

| Item | Status entering | What this pass did |
|---|---|---|
| **`tes_grounding_zero_mass_dynamic_fix`** — Skyrim architecture ships mass=0 Dynamic-family Havok bodies, reclassified Static (19 → 416 colliders, #1832). Mass=0 angle CLOSED; door-threshold spawn gap OPEN. | Verified fixed before dimensions launched | **Not re-filed.** Fix confirmed live at `crates/nif/src/import/collision/mod.rs:371`. Dim 5 was instructed explicitly not to re-investigate it and did not. **The door-threshold gap itself was mechanised for the first time** — see below. |
| **`interior_spawn_point_fix`** — interiors spawn at the first door's own placement; vanilla `coc` has no auto spawn-point logic. | Assumed | **Assumption honoured.** No finding proposes inventing spawn-point logic. PHYS-D5-01/D5-03 both work *within* the door-placement premise: D5-01 fixes the probe origin, D5-03 makes the runtime path use the same ladder as cold start. |
| **`fnv_furniture_sit_needs_transition`** — sit loops have no pelvis/root channel; M42 seat-snap gated behind `BYRO_SANDBOX_SIT`. | Out of scope | **Untouched.** No finding proposes changing seat-snap or the env gate. |
| **WATAL open items** — character swimming/drowning, exact tail decode, disturbance events. | Open by design | **Confirmed absent, not reported as bugs.** Dim 6 verified by search that no swim mode, breath accumulator, `SplashEvent`/`RippleEvent`, or water-as-solid-collider exists anywhere, and that `PhysicsWaterConstants` ships no `swim_speed_mult`/`drown_dps` field — i.e. deliberately trimmed to the shipped subset rather than stubbed. |

### The door-threshold spawn gap — newly mechanised

This is the substantive movement on a long-open item. Dim 5 established, by
measurement rather than inference:

1. **Collider present → YES.** The #1832 reclassification holds; TriMesh and
   primitive statics both register and are visible to `exclude_dynamic` probes
   once `update_query_pipeline()` has run, which `setup_scene` does explicitly.
2. **Cast hits → NO.** The rung-1/rung-2 probe is mis-aimed by 14 BU and is
   structurally blind to any floor within ~15 BU of the door's own Y — i.e. to
   the door's own floor, in the normal case. **PHYS-D5-01**, with a truth table.
3. **Controller grounded → CONDITIONALLY.** Once grounded on a convex/primitive
   floor, the fixed −32 BU probe is intermittently unclamped, sinking the capsule
   through solid collision into unbounded free-fall. TriMesh floors immune;
   `BhkBoxShape` and every synthesized packed-Havok proxy are not. **PHYS-D5-02**,
   48/80 measured failure rate.
4. **Runtime door walks never enter the ladder at all** and place the capsule
   ~116 BU below the destination floor. **PHYS-D5-03.**

**Not established** (stated so it is not re-derived): which of the three dominates
any *specific* in-game report — that needs a live run with the existing
`floor_rung` / `dump_spawn_collider_census` telemetry, which this audit did not
perform. Note that PHYS-D7-05 says that telemetry is currently unreachable from
`byro-dbg`, so fixing D7-05 is a prerequisite for closing that question.

---

## Cross-Audit Dedup

| Topic | Owner | Disposition |
|---|---|---|
| Lock ordering in `push_kinematic` / `pull_dynamic` | `/audit-concurrency` Dim 5 | **Existing #2404 (OPEN)** — traced and verified here, **not re-filed** |
| ABBA `Transform`/`RapierHandles` (#2135), `dump_awake_fallers` (#2136), cell-unload leak (#1520), under-declared access (#1787) | `/audit-concurrency` | All **CLOSED**, fixes verified live in current code — **no regressions** |
| `unsafe` blocks | `/audit-safety` | None in `crates/physics/` — nothing to hand over |
| Water **rendering** (`water.frag`/`water.vert`) — #2790 #2789 #2787 #2785 #2784 #2782 #2763 | `/audit-renderer` Dim 15 | All render-half or doc-rot; **untouched here**. The one genuinely shared object is `WaterFlow`/`scroll_a` → reported once, as **PHYS-D6-03** |
| Height fog / REN-D16-01 (#2225) | `/audit-renderer` Dim 16 | **PHYS-D7-01 reports that the landed fix is nullified by the physics-side cast.** The defect is in `cast_ray_down`, so it is filed here, but the renderer audit's "fixed" record needs correcting |
| `bhk*` **parsing** | `/audit-nif` Dim 5 | Only the parse→`CollisionShape` handoff was examined (PHYS-D1-03); wire decode untouched |
| `bhk*Shape` → `CollisionShape` translation | `/audit-nifal` Dim 6 | `resolve_shape` parity verified clean (dispatch/resolve arms match); PHYS-D1-03 is the one finite-guard gap |
| `XCLW` tri-state decode | `/audit-esm` Dim 5 | Verified harmless for physics (sentinels filtered at the CELL boundary). **Pointer handed over**: the WRLD `DNAM` default height (`cell/wrld.rs:140`) is read raw and does **not** pass through `xclw_water_height` |
| Per-frame `Transform` dirty-set churn | `/audit-performance` Dim 1 | **PHYS-D3-01** is the same class as CLOSED #1374, unfixed in `pull_dynamic` |

---

## Recommended Fix Order

1. **PHYS-D2-01** — one-line change (`if steps > 0 { pending_wake = false; }`),
   unblocks ragdoll activation and PHYS-D6-02, and is a prerequisite for trusting
   any smoke-test result above 60 fps.
2. **PHYS-D5-02** then **PHYS-D5-01** then **PHYS-D5-03** — the player falls
   through the world; these three are the door-threshold gap end-to-end.
3. **PHYS-D7-01** — restores a HIGH renderer fix that is currently inert; the
   signature change also fixes PHYS-D7-02.
4. **PHYS-D1-01** (+ D3-03) and **PHYS-D4-01** — the scale-blindness pair; one
   boundary change plus one pivot change.
5. **PHYS-D2-04 / D4-04 / D1-04** — the test gaps that let the above ship green.
   Landing these first would be defensible, since each pins a defect above.
6. **PHYS-D7-05 → D7-03 → D7-04** — make the census reachable, then make it
   correct; this is what lets the remaining door-gap question be closed with real
   telemetry.
7. **PHYS-D4-05** — close #2339 referencing `8ee151e0` (no code change).
8. Doc corrections: **PHYS-D4-03** (`physal.md` §3), **PHYS-D3-05**
   (`physics.md` phase count + opt-out premise), **PHYS-D7-06** (fast-path cost
   rationale).

---

*Generated by `/audit-physics` — 7 dimensions, all launched as independent
agents, findings deduplicated against 2 769 issues and 20 prior audit reports.
No engine instance was launched. All temporary probe tests were reverted;
`git status` clean, `cargo test -p byroredux-physics` → 72 passed.*
