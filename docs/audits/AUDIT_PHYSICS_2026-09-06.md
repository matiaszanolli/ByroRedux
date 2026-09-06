# PHYSAL / Physics Audit — 2026-09-06 (full 7-dimension pass)

**Run**: `/audit-physics --depth deep`, orchestrator + 7 dimension agents (max 3
concurrent), per `audit-physics/SKILL.md`. **HEAD** `229306ce` — the run started
at `a8233f2f` and the tree advanced 3 commits mid-run (#3617 LVSP, #3614/#3616
INFO, #3628 depth-capture); `git log a8233f2f..HEAD --stat` touches **nothing**
in physics scope, so every citation holds at both revisions.

**Scope**: `crates/physics/src/` (world, sync, convert, components, config,
ragdoll, water, lib) + `byroredux/src/ragdoll.rs` +
`byroredux/src/systems/character.rs` + `byroredux/src/systems/water.rs` +
`byroredux/src/commands/{physics,water,scene}.rs` +
`byroredux/src/cell_loader/{spawn,spawn/mesh_instance,unload,transition}.rs` +
`byroredux/src/scene/nif_loader.rs` + `byroredux/src/boot.rs` (access
declarations) + the parse side `crates/nif/src/import/collision/` and
`crates/nif/src/blocks/collision/constraints.rs`.

**Tests**: `cargo test -p byroredux-physics` — **156 passed, 0 failed,
0 ignored** (0.04s). Run **once**, by the orchestrator, before dispatch; every
dimension agent was explicitly forbidden from re-running cargo (~8 GB of 29 GB
RAM free, and whole-ESM parsing in this workspace has OOM-killed sessions).

**Engine**: not launched (`feedback_no_parallel_engine_launch`). No source file,
game file, or GitHub issue was modified.

**Delta audited**: `2026-08-30..HEAD` is **8 commits** over the whole physics
scope, and **zero** since 2026-09-04. `crates/physics/src/world.rs` and
`convert.rs` are byte-identical to the last full pass. This was therefore a
**fix-verification sweep**, not a delta review — and that framing is what
produced the results below.

**Orchestrator verification**: every finding's load-bearing claim was
independently re-derived by the orchestrator from the code before inclusion —
not accepted from the agent. The two HIGHs were traced link-by-link (see
**Verification** at the end). One agent overstatement was caught and corrected
(D4's HEAD attribution); one orchestrator grep was too narrow and the agent was
right (D7-02's `assets.rs` precedent).

---

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | **2** |
| MEDIUM | **7** |
| LOW | **9** |
| **Total (new)** | **18** |

### The result that matters

The 2026-08-30 pass ended with a thesis, stated as an observation about two
findings:

> **This subsystem's current failure mode is partial closes, not new bugs.**
> Nine consecutive dimension-level invariants re-verified clean; the risk is in
> how fixes are scoped, not in the code drifting.

**This pass tested that thesis across all seven dimensions and it held in seven
of seven.** Every dimension that examined a recently-landed fix found that fix
incomplete against its own evidence. Not one finding in this report is a fresh
defect in code that nobody had recently touched:

| Dim | The fix | What it closed | What it left |
|---|---|---|---|
| 1 | `b8c4e6af` (#3064/#3065) | 2 of the 3 producers that pre-bake scale | `synthesize_packed_havok_proxy` → collider is `XSCL²` |
| 2 | #2864 deferred-rebuild contract | `register_newcomers` announces its inserts | `build_ragdoll`'s insert does not |
| 3 | #2866 | the *writeback* half of the NIF-collision local/world contract | the *registration* half reads an uncomposed `GlobalTransform` |
| 4 | #3792 | every enum-exhaustive site the compiler forced | `seed_joint_from_body_poses`, which dispatches on `ndofs()` |
| 5 | #3799 | the pure `resolve_ground_contact` (3 tests) | the impure gate carrying the whole safety argument (0 tests) |
| 6 | `0fd72cb6` (#3490) | the **Y** half of the AABB-vs-origin split | the **XZ** half — which the predecessor finding filed *specifically to prevent this* |
| 7 | #2869, #2874 | the sibling probe's self-exclusion; the report function's 3-way split | the census re-sweep; the console caller |

Seven dimensions, seven independent instances, none of which knew about the
others. The 08-30 report's framing — "the risk is in how fixes are scoped" — is
no longer a hypothesis about this subsystem. It is the measured result, and it
is the single most actionable output of this run.

**Two of the instances compound.** Dimensions 1 and 3 found *different halves of
the same `b8c4e6af` partial close*, independently: D1 the live `scale²` defect,
D3 the three contradictory contract statements a maintainer would consult while
fixing it — one of which would steer them to "fix" a currently-correct producer.
**They must land as one commit.**

**One instance is about this audit's own ground truth.** Dimension 6 found that
`AUDIT_PHYSICS_2026-09-04.md` records a finding as FIXED that its cited commit
never touched — and because that finding recommended "fold into #3490 rather
than tracking separately", it never received an issue number. As of HEAD the
only record of it anywhere says it is done.

### PHYSAL doctrine verdict — **HOLDS in code, STALE in the spec**

```
$ grep -rnE "GameKind|bsver|NifVersion|game_kind|is_skyrim|is_fo4|is_oblivion|is_fo3|game ==|BS_F76|SF_FORM_ID" \
        crates/physics/src/ byroredux/src/ragdoll.rs byroredux/src/systems/character.rs
(no matches)
```

Zero game/version branches on the solver side, for the fifth consecutive pass.
The per-game seam remains confined to the parse-side CInfo decode
(`BhkConstraint::parse` / `BhkBreakableConstraint::parse`, gated only on
`bsver() <= NI_BS_LTE_16`), exactly where `docs/engine/physal.md` §1 places it.
`a8233f2f` (#3921) adds an FO3 arm to a **test**, not to production.

What is stale is the doctrine document's description of the seam's *size*:
`physal.md` §3 still says "two constraint CInfos" and names two importer
functions. #3330 and #3792 made it **four wire types → three CInfo structs →
three importer functions**, across three wrappers. Filed as
PHYS-D4-2026-09-06-02.

### Games traced

The solver path is game-agnostic by construction (above). Shape translation was
traced against the shared classic-`bhk` producer (Oblivion / FO3 / FNV / Skyrim
LE+SE) and the FO4+/Starfield `BhkNPCollisionObject` opaque-payload proxy route.
Ragdoll articulation was traced against the FNV Protectron evidence carried
forward from #3330/#3792. Water was traced against the FO3/FNV `XWCU`
current-marker producer. **No corpus census was run** — game data is on disk but
whole-BSA/ESM parsing was forbidden this run for the RAM reason above; every
occupancy figure is re-quoted from a cited prior measurement.

### Per-dimension counts (every dimension enumerated)

| Dimension | CRIT | HIGH | MED | LOW |
|---|---:|---:|---:|---:|
| 1 — Shape Translation | 0 | 1 | 0 | 1 |
| 2 — Step Determinism & Budget | 0 | 0 | 0 | 2 |
| 3 — ECS Sync | 0 | 1 | 1 | 0 |
| 4 — Ragdoll Articulation | 0 | 0 | 1 | 1 |
| 5 — Character Controller | 0 | 0 | 0 | 2 |
| 6 — Water / Buoyancy | 0 | 0 | 2 | 2 |
| 7 — Queries & Diagnostics | 0 | 0 | 3 | 1 |

---

## Solver Invariant Matrix

| Invariant | Verdict | Note |
|---|---|---|
| Fixed step — accumulator clamps **before** the loop; `frame_dt.max(0.0)` guards NaN | **HOLDS** | `world.rs:474-478`; clamp value and loop bound agree so no backlog re-arms |
| Anti-spiral budget (#1698) — timer starts before substep 1; drop is slow-motion, never a jump | **HOLDS** | `world.rs:544` precedes `:546`; ≥1 substep always runs |
| Static-scene fast path — gated on `active_dynamic_bodies().is_empty() && !pending_wake`; kinematic deliberately excluded | **HOLDS** | `world.rs:529`; cannot swallow a same-frame wake (#2856 pin) |
| Wake discipline — every motion-starting path wakes | **HOLDS (14/14 paths)** | exhaustive enumeration in Dim 2; but the *contract doc* misdescribes it → D2-02 |
| Collider-set mutation announces itself via `mark_colliders_dirty` | **DRIFTED** | `register_newcomers` does; `build_ragdoll` does not → D2-01 |
| Query pipeline — rebuilt once per frame, outside the substep loop | **HOLDS** | `None` passed at `world.rs:566`; two mutually-exclusive rebuild sites |
| Lock ordering — Phase 1 releases every storage guard before the `PhysicsWorld`/`RapierHandles` writes | **HOLDS** | full 7-site trace in Dim 3; no new edge in `sync.rs` |
| Phase order — collect/register → push kinematic → buoyancy → step → pull dynamic | **HOLDS** | `sync.rs:129/132 → 138 → 150 → 157 → 166`; `BYRO_PROFILE` labels bracket what they name |
| Phase 1 precondition — `GlobalTransform` is fresh when a newcomer is registered | **DRIFTED** | true for `physics_sync_system` by stage order; **false** for the out-of-schedule bootstrap → **D3-01 (HIGH)** |
| Scale applied exactly once, at `collision_shape_to_parts` | **DRIFTED** | 3 of 4 producers correct; the packed-Havok proxy pre-bakes → **D1-01 (HIGH)** |
| Registration idempotency — no entity registers twice | **HOLDS** | collect-time gate `sync.rs:849` (#2867) |
| Teardown completeness — activate→remove leaks no body/collider/joint | **HOLDS** | 3-cycle branching-tree test asserts all three sets return to 0 |
| Ragdoll bone→body mapping; writeback targets the same bone | **HOLDS** | `block_to_body` + `old_to_new` remap; index-aligned by construction |
| Z-up→Y-up applied once, upstream at the NIF boundary | **HOLDS** | `body_pose` / `vec3_from_na` are pure component copies, no permutation |
| Joint seeding carries the animated pose into reduced coordinates (#2337) | **DRIFTED** | holds for Ragdoll and LimitedHinge; **lost for Prismatic** → D4-01 |
| Scheduler `Access` declares every storage the system reads | **DRIFTED** | `Ragdoll` read added by #3492, never declared → D6-03 (4th instance of this class) |
| Buoyancy never pins the fast path; `n_new > 0` escape hatch present | **HOLDS** | all forces `wake_up=false`; the two wake sites are latched one-shots |
| Water containment uses one reference point | **DRIFTED** | AABB centre on Y, body **origin** on XZ → D6-01 |
| Death reconciliation happens in exactly one place (#3119) | **HOLDS** | 2 queue-only producers, 1 `Stage::Late` drain |
| Diagnostics do not lie | **DRIFTED** | census self-hits the player, disables its own 3-way split, cannot see wrong-size, and `awake_counts().1` is not an awake count → D7-01..04 |

---

## Findings — HIGH

### PHYS-D1-2026-09-06-01: `synthesize_packed_havok_proxy` pre-bakes `ref_scale`, so FO4+/Starfield packed-Havok proxies get an `XSCL²` collider — the third site of #3064/#3065, closed on the first two only

- **Severity**: HIGH
- **Dimension**: Shape Translation
- **Location**: `byroredux/src/cell_loader/spawn.rs:101-105` (the stale rationale), `:227` (the first application), `:277-291` (the parented ghost); consumer `crates/physics/src/sync.rs:907` → `crates/physics/src/convert.rs:246-251`
- **Status**: NEW — partial close of #3064 / #3065 (both CLOSED, fixed in `b8c4e6af`)
- **Trigger Conditions**: all of — (1) FO4 / FO76 / Starfield, so collision routes through the opaque `BhkSystemBinary` blob and `needs_packed_havok_fallback()` is true; (2) `collisions.is_empty()` — no decodable classic `bhk` body; (3) the REFR's base layer is `Clutter` or `Actor` (Architecture takes the precise `ArchitectureTriMesh` arm, which is correct); (4) at least one eligible mesh with finite geometry; (5) **the REFR's `XSCL` is not 1.0**. At `XSCL == 1.0` the bug is a no-op, which is why it survived four passes.
- **Description**: `collision_shape_to_parts` has owned scale application since #2860. `synthesize_packed_havok_proxy` predates that and bakes `ref_scale.abs()` into the cuboid half-extents itself, under a rationale that is now false and still written down: *"only the cuboid half-extents need the REFR scale baked because physics ignores `GlobalTransform::scale`."* Physics stopped ignoring it twelve days after that comment was written.
- **Evidence** — the full chain, each link re-verified by the orchestrator:
  1. Producer bakes: `let half_extents = ((max - min) * 0.5 * ref_scale.abs()).max(Vec3::splat(0.5));` (`spawn.rs:227`). `min`/`max` come from `transformed_mesh_aabb`, mesh-local TRS only.
  2. The ghost is **parented** with local scale 1.0: `world.insert(ghost, Transform::new(local_center, Quat::IDENTITY, 1.0)); world.insert(ghost, GlobalTransform::new(world_center, world_rot, 1.0)); … world.insert(ghost, Parent(placement_root)); add_child(...)` (`spawn.rs:277-291`).
  3. Propagation **overwrites** the seeded `1.0`. `placement_root` carries `GlobalTransform::new(ref_pos, ref_rot, ref_scale)` (`spawn.rs:851-854`); the BFS reaches the ghost and does `if let Some(g) = gq.get_mut(entity) { *g = composed; }` where `composed = GlobalTransform::compose(&parent_global, local.t, local.r, local.scale)` (`crates/core/src/ecs/systems.rs:316-325`), whose scale term is `parent_scale * local_scale` = `ref_scale`.
  4. Ordering is unconditional: cell loading runs after `scheduler.run(...)`, and `Stage::PostUpdate` (propagation) precedes `Stage::Physics`. `collect_newcomers` always reads the propagated value.
  5. Consumer applies again: `collision_shape_to_parts(&n.shape, n.global.scale, &cfg)` (`sync.rs:907`) → `SharedShape::cuboid(clamp_shape_extent(half_extents.x * scale), …)` (`convert.rs:246-251`).

  Net: **half-extents = authored_local × `ref_scale²`**.

  The sibling that was fixed: `synthesize_static_trimesh(positions, mesh_indices)` takes **no scale parameter** at HEAD, and `b8c4e6af`'s replacement test is named `placement_scale_is_applied_once_by_the_shared_converter`. The proxy's own test, `packed_proxy_bakes_outer_scale_into_cuboid_extent` (`spawn/synthesize_trimesh_tests.rs:234`), pins the **producer in isolation** at `ref_scale = 2.0` → `(2,4,6)` — it certifies the pre-bake rather than catching the doubling.
- **Impact**: every FO4 / FO76 / Starfield Clutter or Actor placement matching the trigger gets a proxy `XSCL²` the intended size. `XSCL 2.0` → a 4× cuboid: an invisible wall metres from anything visible. `XSCL 0.5` → a 0.25× cuboid the player walks through. The visual mesh is correct in both directions and nothing is logged — the same invisible-failure profile #3064 was filed HIGH for. Bounded, not catastrophic: `RT_ABSOLUTE_PRECISION_CEILING` and `clamp_shape_extent`'s `MAX_SANE_SHAPE_EXTENT` (both 2^20) cap the product, so this is wrong size, not broadphase poisoning.
- **Related**: #3064, #3065, #2860, #2355, #2543; **PHYS-D3-2026-09-06-02** (the doc rot that preserved the false premise — fix together); PHYS-D7-2026-09-06-03 (the diagnostic that cannot see this).
- **Suggested Fix**: drop `* ref_scale.abs()` from `spawn.rs:227` and let the shared converter own scaling, exactly as `b8c4e6af` did for the trimesh sibling. Keep `ref_scale` as a parameter (still needed for the `is_finite` gate and `local_center`). Rewrite the `:101-105` rationale — it is the false premise, not a comment. Convert `packed_proxy_bakes_outer_scale_into_cuboid_extent` into the end-to-end form its sibling already has: spawn the ghost under a scaled `placement_root`, run propagation + `physics_sync_system`, assert the resulting collider AABB.

### PHYS-D3-2026-09-06-01: Phase 1's fresh-`GlobalTransform` precondition is enforced only by scheduler stage order, and the out-of-schedule bootstrap walks around it — NIF-node colliders register at the world origin, permanently

- **Severity**: HIGH
- **Dimension**: ECS Sync
- **Location**: `crates/physics/src/sync.rs:200-219` (the bootstrap) and `:807-866` / `:880-940` (the Phase 1 it re-enters); callers `byroredux/src/scene.rs:1263` and `byroredux/src/systems/character.rs:822`; producer `byroredux/src/scene/nif_loader.rs:519-542`
- **Status**: NEW
- **Trigger Conditions**: any collision-carrying entity spawned by `load_nif_bytes_with_skeleton` collected by `register_newcomers_and_refresh_queries` before a `transform_propagation_system` pass has run. Two live routes: (a) `cargo run -- <mesh>.nif` — the default loose-NIF viewer invocation, no flags; (b) any interior cell transition in Character mode where the reference phase spawned an NPC.
- **Description**: the tick's module doc opens with the precondition — *"Runs after `transform_propagation_system` so that `GlobalTransform` is fresh for Phase 1 spawning"*. That is true for `physics_sync_system` (`Stage::PostUpdate` = 2 precedes `Stage::Physics` = 3). But `register_newcomers_and_refresh_queries` is a **second entry into the same Phase 1** that runs outside the scheduler. Its doc is careful about the *exclusivity* half of the contract (#3267) and says nothing about the *freshness* half. Neither caller satisfies it.
- **Evidence**:
  - The producer seeds identity: `world.insert(entity, GlobalTransform::IDENTITY);` (`nif_loader.rs:520`), with the collision shape and body inserted on the same entity at `:540-541`. The correctly-composed `rest_pose` **is computed 8 lines earlier** (`:512-516`) and used only for a name map.
  - Route (a): `scene.rs:944` runs propagation only under `if studio_mode && has_nif_content`. `diagnostic_scene = combustion_lab || cornell_glass_dragon || cornell_oracle.is_some() || cornell_sun.is_some() || studio_mode` (`:700-704`) — all false for a bare NIF run. So `spawn_plan`'s `else` branch at `:1259` calls `register_newcomers_and_refresh_queries(world)` (`:1263`) with no propagation having occurred.
  - Route (b): `transition.rs:559` → `ground_character_body_at` → `register_newcomers_and_refresh_queries(world)` (`character.rs:822`), firing the instant the reference phase reports `Complete` — and NPC spawn is part of that phase, so every bone entity just created sits at `GlobalTransform::IDENTITY`.
  - Phase 1 reads it as world truth: `collision_shape_to_parts(&n.shape, n.global.scale, &cfg)` (`sync.rs:907`) and `.position(iso_from_trs(n.global.translation, n.global.rotation))` (`:938`).
  - Registration is **one-shot** — the collect gate `if handles_q.contains(entity) { continue; }` (`sync.rs:849`) means the body is never rebuilt, and the collider *shape* (where the scale lives) is never rebuilt for any motion type.
- **Impact**, by motion type: **Static** — permanently stranded at the world origin, identity rotation, scale 1.0; nothing re-poses a fixed body; silent. **Keyframed** (every NPC ragdoll bone after `keyframe_live_ragdoll_bones`) — `push_kinematic` re-targets the pose next tick, so for one frame ~18 bone colliders per NPC stack at the origin and Rapier derives an enormous implied kinematic velocity from the one-step correction; the baked **scale** is never revisited, so a bone collider on a non-unit-scaled actor is permanently the wrong size. **Dynamic** — worst: spawned asleep, `pull_dynamic`'s sleeping-skip compares the ECS local against the origin-derived local, they differ, and **the ECS `Transform` is overwritten from the bad Rapier pose** — the visual sub-object teleports toward the origin. In all cases the BVH the bootstrap refreshes (the entire point of both call sites) is the one the ground probe immediately queries.
- **Related**: **#2866 fixed the *writeback* half of this same entity's local/world contract and left the registration half unexamined** — the same partial-close shape; #2867 (the gate that makes registration one-shot); #3267 (narrowed these call sites to this bootstrap); #1698 (awake-faller storms — the Keyframed origin excursion is a plausible unlisted contributor).
- **Suggested Fix**: make the precondition explicit rather than accidental. Preferred: have `register_newcomers_and_refresh_queries` run a propagation pass over the pending set before collecting — it already owns exclusive access, which is what makes that legal. Alternative: state the precondition in its doc beside the exclusivity contract, add a `debug_assert` that no collected newcomer carries a `Parent` whose composed global disagrees with its own, and give both callers an explicit `propagate(world, 0.0)`. Independently, having the producer seed its already-computed `rest_pose` instead of `GlobalTransform::IDENTITY` removes the sharp edge for every consumer.

---

## Findings — MEDIUM

### PHYS-D3-2026-09-06-02: the scale contract is stated **backwards** on the rustdoc of the exact function `register_newcomers` calls, and in the doc that rustdoc cites

- **Severity**: MEDIUM
- **Dimension**: ECS Sync
- **Location**: `crates/physics/src/convert.rs:138-141` · `docs/engine/physics.md:424-428`
- **Status**: NEW
- **Trigger Conditions**: static. It misfires the moment anyone reasons about who applies scale at the `sync.rs:907` boundary — which is exactly what fixing PHYS-D1-2026-09-06-01 requires.
- **Description**: `collision_shape_to_parts` is the sink `register_newcomers` hands `n.global.scale` to. Its own rustdoc carries a census of which producers pre-bake scale, and **both entries are wrong**: it says `synthesize_static_trimesh` multiplies every vertex by `world_scale` (it has taken no scale parameter since `b8c4e6af`, and its own header says the opposite) and that `spawn_packed_havok_proxy` passing `ref_scale` through is *consistency with the design*. `docs/engine/physics.md:424-428` repeats the error and generalises it into a false statement about this dimension: *"physics consumes position and rotation, not `GlobalTransform::scale`"* — untrue since #2860.
- **Evidence**: `git show b8c4e6af --stat` touches `spawn.rs` (removed the pre-bake), `convert.rs` (edited ~100 lines below the stale doc) and `docs/engine/physal.md` — where it **added the correct contract**: *"Producers must not bake that scale into vertices or primitive dimensions; doing both produces scale² geometry"* (`physal.md:105-110`). The commit names, in its own evidence, the exact failure mode D1 found still live — and left both contradicting statements standing in the two places a reader of `collision_shape_to_parts` would actually look.
- **Impact**: three statements of one contract, two inverted, and the correct one is in the file least likely to be open while editing `sync.rs:907`. A maintainer fixing D1-01 from `convert.rs:138-141` would conclude that pre-baking *is* the intended contract for the proxy (leaving the real bug alone) and that `spawn_trimesh_collider_ghost` is the one out of line — "fixing" a currently-correct producer. Not cosmetic: it is the contract statement a HIGH-severity fix will be checked against.
- **Related**: #3064, #3065, #2860, **PHYS-D1-2026-09-06-01 (land together)**, `docs/engine/physal.md:105-110` (the correct wording to propagate).
- **Suggested Fix**: replace `convert.rs:138-141` with the `physal.md` wording — "every producer keeps the shape in local units; this function applies `GlobalTransform::scale` exactly once" — and **delete the per-producer census from the rustdoc** rather than re-listing it (a list of producers on the consumer is the thing that rots). Correct `docs/engine/physics.md:424-428` the same way, in the same commit as the D1 fix.

### PHYS-D4-2026-09-06-01: `seed_joint_from_body_poses` dispatches on `ndofs()`, so #3792's new prismatic joints are seeded with a rotation angle on their linear axis

- **Severity**: MEDIUM
- **Dimension**: Ragdoll Articulation
- **Location**: `crates/physics/src/ragdoll.rs:623-652` (the function and its now-false docstring), reached from `:392` (the unconditional per-edge call in `build_ragdoll`); the joint it cannot distinguish is built at `:575-620`, its lock mask at `:489-496`
- **Status**: NEW
- **Trigger Conditions**: activate a ragdoll (death, or the `ragdoll <id>` console command) on any actor whose `RagdollTemplate` contains at least one `RagdollJointSpec::Prismatic` edge. Known vanilla content: `meshes\creatures\protectron\skeleton.nif` (FNV `Fallout - Meshes.bsa`, and its FO3 counterpart). Error magnitude scales with how far the two bodies' relative rotation is from the joint frames' rest alignment at activation — smallest in bind pose, largest mid-animation, which is the normal case.
- **Description**: #3792 added a third `RagdollJointSpec` variant, `Prismatic`, whose lock mask leaves **`LinX`** free. `build_joint` and `scaled_pivots` both gained a `Prismatic` arm — the compiler forced them, because both are exhaustive `match`es on the enum. `seed_joint_from_body_poses` is the one dispatch on joint kind in the whole path that is **not** a match on the enum: it switches on `MultibodyJoint::ndofs()`, an integer.
- **Evidence**: `LimitedHinge` locks `lin_locked() | ANG_Y | ANG_Z` = `LIN_X|LIN_Y|LIN_Z|ANG_Y|ANG_Z` — 5 axes. `Prismatic` locks `LIN_Y|LIN_Z|ANG_X|ANG_Y|ANG_Z` (`prismatic_locked()`, `:489-496`) — also 5. Rapier 0.22 computes `ndofs() = 6 - locked.count_ones()`, so **both are `ndofs == 1`** and are indistinguishable here:
  ```rust
  let angular_displacement = joint_rotation.scaled_axis();
  match joint.ndofs() {
      // Limited hinge: only local angular X is free.
      1 => joint.apply_displacement(&[angular_displacement.x]),
      3 => joint.apply_displacement(angular_displacement.as_slice()),
      ndofs => log::warn!("ragdoll: cannot seed unsupported {ndofs}-DOF …"),
  }
  ```
  `apply_displacement` walks the **linear** axes first (`multibody_joint.rs:88-95`), so for a prismatic edge `coords[LinX] = angular_displacement.x` — the X component of a rotation vector in radians assigned to a slide distance in engine units. No clamp against the joint's own `[min_distance, max_distance]` is applied. The `ndofs => warn!` catch-all cannot fire.
- **Impact**: two defects on one line, scoped to `Prismatic` edges. (1) **The #2337 guarantee is lost for this kind** — the function exists to carry the animated pose into the multibody's reduced coordinates before the first step overwrites it; the correct quantity (the along-rail component) is never computed, so the child link snaps to the rail's zero position. That is precisely the failure #2337 was filed against, reintroduced. (2) **A type-confused value is injected** — `|angular_displacement.x| ≤ π`, so the child is displaced along its rail by up to ~π units × bone scale (`havok_scale = 7.0` on FNV/FO3), which can exceed the joint's authored travel range; `apply_displacement` does not clamp, leaving the solver's limit constraint to resolve an impulse at t=0. That is the "ragdoll pops on activation" shape, not a cosmetic offset. #3792's own measurement records the Protectron as *2 Prismatic* of 12 joints, so exactly two edges take this path per Protectron ragdoll, every time.
- **Why the tests cannot see it**: the crate contains **exactly one** `RagdollJointSpec::Prismatic` — the production arm at `:575`. Zero tests construct one. #3792's headline test asserts NIF-import connectivity and is `#[ignore]`d; it never calls `build_ragdoll`. 156/156 green with the defect present.
- **Related**: #3792 (`13fdb48e`, introduced the third `ndofs == 1` kind), #2337 (the issue this function implements), #3330 / #1539 / #1850.
- **Suggested Fix**: dispatch on the joint kind, not on `ndofs()`. Either pass the `&RagdollJointSpec` down (the call site at `:392` has it in hand) or read `joint.data.locked_axes` and branch on whether the free axis is linear or angular — for the prismatic case seed `(frame1⁻¹ ∘ parent_to_child ∘ frame2).translation.x`. Update the docstring's now-false *"All ragdoll linear DOFs are locked"* precondition. Add a `prismatic_seed_uses_the_slide_distance_not_the_twist_angle` test; the module currently has no `Prismatic` coverage at all.

### PHYS-D6-2026-09-06-02: `AUDIT_PHYSICS_2026-09-04.md` records a finding as FIXED that its own cited commit did not touch

- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy (audit ground truth)
- **Location**: `docs/audits/AUDIT_PHYSICS_2026-09-04.md:54` (the state table) · `:62-72` (the checklist row) · `:104-115` (the Verification Log entry for #3490)
- **Status**: NEW
- **Trigger Conditions**: any future Dimension-6 pass that reads the 09-04 report's clean verdict — which is what the report exists to provide.
- **Description**: the state table reads `| #3490 … | OPEN, verified true (PHYS-D6-2026-08-30-01 extended it) | **FIXED** — 0fd72cb6 |`. `0fd72cb6` fixed #3490's own premise; it did not address the extension. The report names the extension by ID **in the same cell** and then marks the row fixed. Its Verification Log entry is accurate about what it checked (*"the diff hoists one shared `aabb_y` fetch above both branches"*) but never re-opens the extension it cited, and the checklist row generalises that Y-only check into *"both branches now share one `compute_aabb()` call"* — true of the **fetch**, false of the **containment predicate** the row is about.
- **Evidence**: `git show 0fd72cb6 -- crates/physics/src/water.rs` removes and re-adds the union prefilter line **verbatim** (renamed `current_flow` → `aabb_y`); no XZ predicate line is touched. `.claude/issues/3490/ISSUE.md:33-38` records the sibling search as scoped to *"other body-origin-vs-collider-centre **Y** reads"* — the XZ half was never in the fixer's search space. `grep -rn PHYS-D6-2026-08-30-01` across the repo returns exactly two hits: the report that filed it and the report that (incorrectly) closed it. It has no GitHub issue, because its own recommendation was "fold into #3490 rather than tracking separately", and #3490 closed without folding.
- **Impact**: the defect in PHYS-D6-2026-09-06-01 is now recorded as fixed in the only place it is recorded at all. A pass that trusts the 09-04 report — the normal and intended use of a clean report — will not re-check it, and #3490's own sibling-search note steers anyone who does look toward the Y axis only.
- **Related**: PHYS-D6-2026-09-06-01, #3490, PHYS-D6-2026-08-30-01.
- **Suggested Fix**: amend the 09-04 report's row to "FIXED (Y axis); XZ extension still open" and file the extension under its own number so it stops depending on a closed issue's scope. **Process rule worth adopting**: an audit that closes finding A *by absorbing* finding B must verify B's predicate independently rather than inheriting A's commit.

### PHYS-D6-2026-09-06-03: #3492 added a `Ragdoll` storage read to `physics_sync_system` and updated neither the `Access` declaration nor the guard test that exists to catch that

- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy (ECS access declaration)
- **Location**: `byroredux/src/boot.rs:1452-1494` (the declaration) · `crates/physics/src/water.rs:487` + `:747` (the two undeclared reads) · `byroredux/src/scheduler_access_tests.rs:148-172` (`water_and_animation_parallel_accesses_are_complete`, the #3121 guard)
- **Status**: NEW — **fourth** instance of a class already filed and fixed three times, twice on this exact declaration: #1787 / CONC-D4-01, #2676 / CONC-D3-NEW-02, PHYS-D3-2026-08-20-05 (fixed `5428e872`, MEDIUM)
- **Trigger Conditions**: none today — `physics_sync_system` is currently the only system in `Stage::Physics` and nothing else declares `Ragdoll`. Becomes live the moment any system reading or writing `Ragdoll` joins a parallel batch that can overlap `Stage::Physics`.
- **Description**: `1e4d83a7` gave the buoyancy sink a second target source keyed on `Ragdoll`. Both `apply_buoyancy_with_scratch` (`water.rs:747`) and `clear_stale_water_contacts` (`:487`) now take a `world.query::<Ragdoll>()` read guard on the default path. The declaration lists every other storage the buoyancy phase touches — `WaterPlane`, `WaterVolume`, `WaterFlow`, `WaterCurrentVolume`, `WaterContact`, `RapierHandles`, `RigidBodyData` — and even declares three components read only behind an env-var-gated diagnostic, with a comment explaining that *"the analyzer can't see the runtime gate"*.
- **Evidence**: `git show --stat 1e4d83a7` → four files (`crates/physics/src/{components,ragdoll,water}.rs`, `docs/engine/physics.md`); neither `boot.rs` nor `scheduler_access_tests.rs` is among them. `grep -n Ragdoll byroredux/src/boot.rs` → only the three `world.register` lines at `:622-624`; **no `Access` declaration anywhere mentions it**. The guard test — whose doc reads *"#3121 — WATAL buoyancy reads live time/weather/current state … Keep those reads on the exact parallel-system declarations so the scheduler can reject future conflicts"* — still asserts its pre-#3492 needles and passes.
- **Impact**: `scheduler.access_report().known_conflict_count() == 0` is computed from an incomplete surface, and `sys.accesses` under-reports what `physics_sync_system` acquires. Same impact statement PHYS-D3-2026-08-20-05 carried when it was filed and fixed at MEDIUM.
- **Related**: #3492, #1787, #2676, PHYS-D3-2026-08-20-05, #3121, `/audit-concurrency` Dim 4.
- **Suggested Fix**: add `.reads::<byroredux_physics::Ragdoll>()` to the `physics_sync_system` declaration with the same one-line "#3492 — the buoyancy phase's second target source" comment the neighbouring water entries carry, and extend `water_and_animation_parallel_accesses_are_complete`'s needle list so the guard covers the read it was written to guard.

### PHYS-D7-2026-09-06-01: the census's re-sweep hard-codes `excluded_body: None`, so `phys.census` casts through the player's own capsule — #2869 fixed exactly this at the sibling probe and #2876 shipped the console arm without carrying it over

- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**: `crates/physics/src/sync.rs:621-627` (the hard-coded `None`); contract at `crates/physics/src/world.rs:749-762`; live caller `byroredux/src/commands/physics.rs:117-141`; sibling that does exclude: `byroredux/src/scene.rs:227-253`
- **Status**: NEW — partial propagation of #2859 / #2869 (both CLOSED)
- **Trigger Conditions**: `PlayerMode::Character` (so the player capsule exists as a real non-sensor `KinematicPositionBased` collider), and `phys.census` run in its default no-argument form or with an XZ within ~36 BU of the player — i.e. precisely the situation the command exists for.
- **Description**: `cast_capsule_down_surface_and_normal`'s own doc makes the exclusion mandatory *by design*: *"`excluded_body` must be passed whenever the origin can lie inside a body … The parameter is deliberately mandatory (rather than a defaulted sibling method) so every call site has to decide — a silent self-hit is invisible (#2859)."* `spawn_collider_census_report` does not decide; it passes a literal `None`, with no field on `SpawnCensusProbe` through which a caller could supply one. That was correct when the only caller ran before the capsule was spawned. It stopped being correct on 2026-08-21.
- **Evidence**: geometry from the constants — `HUMAN` is `half_height = 46`, `radius = 18`; `floor_probe_lift = 80`. With the player centre at `py`, the player capsule spans `[py−64, py+64]` and the probe capsule at t=0 spans `[py+16, py+144]` — segments overlapping on `[py+34, py+46]` at the same XZ. `solid_probe_filter()` does not mask it: `exclude_dynamic()` skips only Dynamic, `ground_probe_groups()` masks only `ACTOR_BONE_GROUP` (the player registers with `InteractionGroups::all()`), and the collider is not a sensor. The sibling probe was given the parameter for exactly this reason and both live-world call sites pass `Some(player)`; the census re-sweep is **the only live-world downward probe in the workspace that passes `None`**. Timeline: `264f44fd` (#2869, sibling fixed) → `d8dc7608` (#2874, re-sweep added, `None` correct because boot-only) → `fa5f75f7` (#2876, exposed as `phys.census` whose default reference point *is* the player body; the `None` was not revisited).
- **Impact**: a surviving self-hit returns `time_of_impact = 0`, beating every real floor, and maps to a phantom `surface_y` 16 BU **above the player's own centre**. Both `SpawnProbeVerdict` arms are then wrong and both print **first**, on the strength of the census's own rationale that the verdict *"comes FIRST because when it fires, the column tallies below are a red herring"* — `RejectedNonWalkable` actively tells the operator to ignore the real evidence. **Honest bound**: in the exactly-coaxial default case the outcome sits on a floating-point knife-edge (two coaxial capsules have a horizontal MTD normal, and parry drops a t=0 hit only when `normal1.dot(vel12) >= 0.0`, which is `±ε` here), so it is discarded or kept on rounding alone; any horizontal offset tilts the normal and keeps it deterministically. An intermittent lie in a diagnostic is worse than a consistent one, and the knife-edge is why nothing caught it — no test runs the census on a world containing a player capsule.
- **Related**: #2859, #2869, #2874, #2876; PHYS-D7-2026-09-06-02 is a second defect on the same call — fix both in one edit.
- **Suggested Fix**: add `excluded_body: Option<RigidBodyHandle>` to `SpawnCensusProbe` and thread it into the cast; the boot caller keeps `None`, and `PhysCensusCommand` resolves the player's `RapierHandles.body` exactly as `probe_walkable_floor_near` already does (resolve the handle, drop the component guard, then take the `PhysicsWorld` lock). Pin it with a test that puts a kinematic capsule at the probe origin and asserts `NoHit`.

### PHYS-D7-2026-09-06-02: `phys.census` disables its own three-way split on a false premise — `NifImportRegistry` is a live world resource that a sibling console command already reads

- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**: `byroredux/src/commands/physics.rs:129-132`; consumer `crates/physics/src/sync.rs:719-744`; the resource `byroredux/src/cell_loader/nif_import_registry.rs:316` (`impl Resource` at `:641`), accessor `:445-463`; the boot arm that *does* pass it `byroredux/src/scene.rs:603-606`; live precedent `byroredux/src/commands/assets.rs:523`
- **Status**: NEW — partial close of #2874 (CLOSED)
- **Trigger Conditions**: any `phys.census` run whose column comes back with **zero** colliders — the exact case the three-way split exists to resolve. On a non-empty column it still costs the `classic=/new_physics=/phantom=` line that distinguishes an FO4+ packed-Havok cell from a classic-`bhk` one.
- **Description**: the *report function* does implement the split — `sync.rs:719-744` has four arms keyed on `probe.authoring`, and both the 08-27b and 08-30 passes verified it **there**. Neither followed the value back to the *console* caller, which hard-codes it off with the comment *"The live path has no NIF import cache to sum."* **That premise is false.** `NifImportRegistry` is documented as a process-lifetime cache *"promoted to a world-resource so cell-to-cell traversal re-uses every previously-parsed mesh"*, it implements `Resource`, the boot arm reads it in two lines (`scene.rs:603-606`), and `commands/assets.rs:523` — a sibling console command in the same module tree — does `world.try_resource::<crate::cell_loader::NifImportRegistry>()` live.
- **Evidence**: the console arm renders *"#2874 cell collision authoring unavailable (no NIF import cache) — 0 total below cannot be split into 'nothing authored' vs 'dropped in translation'."* — i.e. it prints the exact conflation #2874 was filed to remove, on the one route an operator can invoke. The boot arm on the identical world renders the discriminating text. `collision_authoring_totals`' own doc reasons that cache-scope is a superset of cell-scope and that *"zero totals do prove nothing colliding was ever authored, and that is the arm the census was mis-reporting"* — so the console arm is not avoiding a scoping hazard, it is discarding the discriminator its dependency was built to provide.
- **Impact**: `phys.census` cannot answer *"is there no collider here because nothing was authored, or because everything authored was dropped in translation?"* — the split that decides whether the bug lives in the ESM/REFR layer or in `bhk` decode/registration. #2876's own module doc claims the console command exists so *"the `phys.census` console command can render the identical text"*; it does not. Worst on FO4/FO76/Starfield, where `new_physics > 0` is the marker that the packed-Havok proxy was supposed to fire.
- **Related**: #2874 (this is its unclosed third), #2876 (introduced the gap). Same call site as PHYS-D7-2026-09-06-01.
- **Suggested Fix**: replace `authoring: None` with the two lines from `scene.rs:603-606` and delete the stale comment. Guard with a `commands_tests.rs` case asserting the output does **not** contain `"authoring unavailable"` on a world holding a `NifImportRegistry`.

### PHYS-D7-2026-09-06-03: the diagnostic channel has no *wrong-size* arm — `SpawnCensusEntry` discards two thirds of an AABB it already computed, and nothing reports placement scale

- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**: `crates/physics/src/sync.rs:517-531` (`SpawnCensusEntry`), `:644-670` (the lossy map), `:754-768` (the render line); source data `crates/physics/src/world.rs:122-135` (`NearbyCollider.aabb_min` / `aabb_max`, both `[f32; 3]`); `byroredux/src/commands/physics.rs:158-199` (`phys.stats`)
- **Status**: NEW. **Not a re-file of PHYS-D1-2026-09-06-01** — that is the defect; this is the answer to whether the channel meant to surface it can.
- **Trigger Conditions**: any investigation of *"collision does not match what I see"* where the collider exists, is Fixed or Keyframed, passes the walkable test, and is simply the wrong **size** — D1-01's `XSCL²` proxy, any future producer/converter scale drift, or a mis-decoded `bhk` half-extent.
- **Description**: three independent reasons the surface cannot express it. (1) **The XZ extents are computed and discarded** — `colliders_near_xz` returns a full 3-D AABB per collider; `spawn_collider_census_report` narrows it to `center_y` / `min_y` / `max_y` before anything is rendered, and the render line prints only `y=[min, max] centre=`. A cuboid four times too wide in X and Z and correct in Y is byte-identical in the output to a correct one. (2) **There is no expected value** — nothing reports the placement's `GlobalTransform.scale` / `XSCL` or the authored half-extent, so even the Y range that *is* printed has no reference. (3) **The verdict enum has no arm** — `NoHit | RejectedNonWalkable | Walkable`. An oversized-but-walkable collider lands in `Walkable`, which the census renders as *"the census probe and the spawn rungs disagree"*, blaming the probe; an undersized one lands in `NoHit`, whose text says *"walkability is not the cause; the column census below is"* — and the census then shows the collider present and looking normal.
- **Evidence**: the codebase already knows this query answers the scale² question — but only in a test. `byroredux/src/ragdoll.rs:895-901`: `let diameter = root[0].aabb_max[0] - root[0].aabb_min[0]; assert!((diameter - 20.0).abs() < 1e-3, "a 2× actor with an authored radius-5 limb needs diameter 20, not scale²: {diameter}");` — the X extent, the exact field the census drops one line after computing it. The *test* surface can catch scale²; the *operator* surface cannot. `phys.stats` adds nothing: `static_colliders_aabb` reports one world-wide envelope that an oversized collider inflates indistinguishably.
- **Impact**: the confirmed HIGH is invisible through the entire diagnostic channel — `phys.census` prints the collider and it looks normal, `phys.stats` prints a healthy envelope, no log fires. An operator hitting invisible walls on a scaled REFR has no console route to the evidence. That is a real gap in the channel whose stated purpose is answering *"why does collision not match what I see"*, and it costs two struct fields and one format string to close.
- **Related**: PHYS-D1-2026-09-06-01 (the defect this cannot see); #2202, #2874, #2875.
- **Suggested Fix**: carry the whole AABB — replace `min_y`/`max_y`/`center_y` with the `aabb_min`/`aabb_max` pair already in `NearbyCollider`, render `extent=[dx, dy, dz]` alongside the existing `y=[…]`, and add the owning entity's `GlobalTransform.scale` (the census already opens the ECS storages to resolve `entity` / `form` / `layer`, so it is one more query in the same guarded block). `scale != 1.0` on a census line is the single highest-value hint for this defect class.

---

## Findings — LOW

### PHYS-D1-2026-09-06-02: `convert.rs`'s and `physics.md`'s "the other producers pre-bake scale" claim is the false premise that hid the HIGH
- **Severity**: LOW · **Dimension**: Shape Translation · **Status**: NEW
- **Location**: `crates/physics/src/convert.rs:139-142` · `docs/engine/physics.md:424-427`
- **Trigger Conditions**: none at runtime — it fires on a *reader*, at the converter itself and in the layer doc.
- **Description / Evidence**: recorded in full as **PHYS-D3-2026-09-06-02**, which Dimension 3 derived independently from the consumer side. Kept here only as the Dimension 1 cross-reference; **do not file twice** — one issue, one fix, landing with PHYS-D1-2026-09-06-01.
- **Impact**: it is the mechanism by which the HIGH survived four passes — the converter's doc presents the proxy's pre-baking as consistency with the design.
- **Related**: PHYS-D3-2026-09-06-02 (the owning finding), PHYS-D1-2026-09-06-01, #3064, #2860.
- **Suggested Fix**: see PHYS-D3-2026-09-06-02.

### PHYS-D2-2026-09-06-01: `build_ragdoll`'s collider insert never marks the query pipeline dirty, and the disproof that closed it in 2026-08-20 rests on a premise the crate's own #2856 pin falsifies
- **Severity**: LOW · **Dimension**: Step Determinism & Budget · **Status**: NEW (the behaviour was investigated and dismissed on 2026-08-20; the dismissal's stated rationale is wrong at HEAD)
- **Location**: `crates/physics/src/ragdoll.rs:322` (insert) and `:412` (`pw.wake()`); gate at `crates/physics/src/world.rs:633-636`; contract at `crates/physics/src/sync.rs:1011-1017`; the false disproof at `docs/audits/AUDIT_PHYSICS_2026-08-20.md:568-573`
- **Trigger Conditions**: a ragdoll activated on an actor whose skeleton bones have no live `RapierHandles` — activation reaching `activate_ragdoll` before the first `physics_sync_system` tick after spawn — on a frame where `accumulator < PHYSICS_DT` (every frame above 60 fps). Reachable via `byroredux/src/combat.rs:546` (a kill on the actor's spawn frame) and the `ragdoll <id>` console command.
- **Description**: since #2864, `mark_colliders_dirty()` is the sole mechanism by which a collider-set mutation reaches the query BVH on a frame that runs no substep. `register_newcomers` honours it; `build_ragdoll` inserts one collider per part and calls only `pw.wake()`. **`wake()` does not guarantee a substep** — it only makes `step` skip the static-scene early return; the `while accumulator >= PHYSICS_DT` gate is independent. With `pending_wake == true`, `accumulator < PHYSICS_DT` and `colliders_dirty == false`, `steps == 0` and `world.rs:633` evaluates `false || false`.
- **Evidence**: `grep -c mark_colliders_dirty crates/physics/src/ragdoll.rs` → **0**. The gate is `if steps > 0 || self.colliders_dirty`. And the crate's own #2856 pin asserts the exact state: `w.wake(); assert_eq!(w.step(PHYSICS_DT / 2.0), 0, "half a tick cannot step yet"); assert!(w.pending_wake(), "wake must still be armed after a 0-substep frame");`. The 2026-08-20 pass dropped this candidate with *"**Inert**: `build_ragdoll` calls `pw.wake()`, so the next `step` **always** takes the substep path"* — the pre-#2856 mental model of `wake()`, recorded as a settled disproof for future passes to lean on.
- **Impact**: bounded. The behaviour *is* safe today, but for a reason the disproof never states: `activate_ragdoll`'s #1772 keyframed-bone teardown calls `pw.remove_body` per bone, and `remove_body` sets `colliders_dirty = true`. A collider mutation in `crates/physics/src/ragdoll.rs` is covered by a side effect of a loop in `byroredux/src/ragdoll.rs` that exists for an unrelated reason and is guarded by `if !bone_handles.is_empty()`. When that guard is false the window is ≤ one banked tick (~16.7 ms, 2 frames at 120 fps) during which a fresh corpse's colliders are absent from the query BVH. The durable cost is the recorded false disproof.
- **Related**: #2864, #2856, #1772, #2863; sibling shape: PHYS-D1-2026-09-06-01, PHYS-D4-2026-08-30-01.
- **Suggested Fix**: add `pw.mark_colliders_dirty();` beside `pw.wake();` at `ragdoll.rs:412`. Separately, correct the 2026-08-20 disproof — restate it as "safe *because* `activate_ragdoll`'s #1772 teardown marks dirty", which is checkable and reveals the `bone_handles.is_empty()` hole.

### PHYS-D2-2026-09-06-02: the wake-discipline contract says spawning a body arms `pending_wake`; the production spawn path deliberately does not, and the `had_newcomers` escape hatch exists because it doesn't
- **Severity**: LOW · **Dimension**: Step Determinism & Budget · **Status**: NEW
- **Location**: `crates/physics/src/world.rs:287-292` (the `wake` docstring) and `:519-520` (the fast-path comment), contradicted by `crates/physics/src/sync.rs:932-946` + `:1011-1017` and by `crates/physics/src/water.rs:668-671`
- **Trigger Conditions**: latent — fires when a maintainer adds a body-creating path, or "restores" the missing wake in `register_newcomers`, on the authority of the two comments.
- **Description**: `wake`'s docstring states the subsystem contract — *"Must be called by every mutation that can introduce motion — **spawning a body**, pushing a kinematic target, setting a velocity"*. Two of the three are true. The third is false for the path that spawns essentially every body in the engine: `register_newcomers` calls **only** `mark_colliders_dirty()`, and its dynamic bodies are built `sleeping(true)` on purpose (the EXTERIOR-FREEZE FIX, whose comment records `atw_scheduler=3005ms` with ~3000 awake dynamics on a Skyrim exterior streaming frame).
- **Evidence**: the crate has exactly 8 production `.wake()` call sites and **none is in the spawn path**. The strongest counter-evidence is in the buoyancy sink, which grew a parameter to work around it: *"The `had_newcomers` term is load-bearing: a body that streams in already submerged spawns ASLEEP and Phase 1 does NOT wake it (`register_newcomers`), so without this term its first-frame dry→wet float-up would be skipped here."* (`water.rs:668-671`), fed by `apply_buoyancy(world, n_new > 0)`. The code contains both the false claim and its own refutation.
- **Impact**: no incorrect behaviour today; the hazard is asymmetric. (a) A future body-creating path added on the strength of "spawn arms it" inherits a body that never moves and produces no error — the silent-failure mode this dimension's checklist exists for. (b) A maintainer reconciling comment with code in the **wrong** direction reintroduces a measured multi-second streaming stall and simultaneously makes `had_newcomers` look redundant, inviting its removal.
- **Related**: #2856, #2889, #2890, #3121, `docs/engine/watal.md` §0.
- **Suggested Fix**: in both comments replace "spawning a body" with the true rule and its reason — spawn is exempt (dynamics spawn asleep by design), announces itself with `mark_colliders_dirty`, and hands first-frame visibility to consumers through the `n_new > 0` argument. One sentence each, cross-referencing `water.rs:668-671`.

### PHYS-D4-2026-09-06-02: the PHYSAL spec still describes a two-CInfo constraint seam that #3330 and #3792 made a four-type one, and `ecs.md`'s #3655 edit asserts a non-overlap property the site it names does not have
- **Severity**: LOW · **Dimension**: Ragdoll Articulation · **Status**: NEW
- **Location**: `docs/engine/physal.md:130-154` (§3) · `crates/nif/src/blocks/collision/constraints.rs:16-21`, `:44-46`, `:503-510`, `:521-525` · `docs/engine/ecs.md:682-689`
- **Trigger Conditions**: none at runtime. Surfaces when a maintainer plans the next constraint-decode slice, or adds a new `PhysicsWorld` consumer.
- **Description**: two doc-vs-code divergences, both introduced by the commits this pass verified. (1) At HEAD the seam decodes **four** wire types (`bhkRagdoll`, `bhkLimitedHinge`, `bhkHinge` — #3330, `bhkPrismatic` — #3792) into **three** CInfo structs and **three** importer functions, across three wrappers. `physal.md` still says *"the typed decode of **two** constraint CInfos"*, names only two importer functions, says *"One `RagdollCInfo` / `LimitedHingeCInfo` therefore feeds every game"*, lists only two byte layouts in its per-game table, and cites only two nif.xml structs. The parser's own docstrings match — *"the two joints a humanoid ragdoll uses"*, *"Every other type stays a `type_name`-only stub"* — all false at HEAD. Neither `1ccf1abe` nor `13fdb48e` touched `physal.md`. (2) `9ce9a9e6` added `ragdoll_writeback_system` to a list whose enclosing sentence reads *"every site … collects what it needs from the component queries, **drops them**, and only then takes the resource … the guards **do not overlap at all**."* Every other named site does exactly that; `ragdoll_writeback_system` holds **eight** storage guards across the entire `PhysicsWorld` guard. The *ordering* invariant the hoist was for is satisfied (`PhysicsWorld` last, nothing taken under it — it is a sink with no outgoing edges, so no cycle is recordable); the stated *non-overlap* property is not.
- **Impact**: documentation only. It matters because both documents are the ones a future change consults — `physal.md` §3 is where someone deciding whether `bhkBallAndSocket` is worth decoding will look and find a table that omits the two kinds already done.
- **Related**: #3330, #3792, #3655, #2883 (the earlier `physal.md` seam-count correction — same paragraph, same failure mode).
- **Suggested Fix**: in `physal.md` §3 replace "two constraint CInfos" with the live four-type / three-CInfo inventory, add `prismatic_joint` to the importer list, add Hinge and Prismatic rows to the *Constraint layout* column, add the two nif.xml structs to the sources-of-truth list, and note that prismatic `friction` and the motors are captured-and-unused; refresh the four `constraints.rs` docstrings in the same edit. In `ecs.md:686-689`, split `ragdoll_writeback_system` into its own clause: it satisfies *`PhysicsWorld` last with nothing under it* while holding its component guards — the weaker but sufficient form of the rule.

### PHYS-D5-2026-09-06-01: the per-frame ground probe is the one production floor cast with no walkable-normal screen, and #3799 promoted its answer to a co-authority on `is_grounded`
- **Severity**: LOW · **Dimension**: Character Controller · **Status**: NEW
- **Location**: `byroredux/src/systems/character.rs:344-352` (the cast), `:353` (`probe_found_support = true`), `:400` (the OR into `grounded`); compare `crates/physics/src/world.rs:854-870` (unfiltered) vs `:880-899` (walkable-filtered)
- **Trigger Conditions**: `PlayerMode::Character`, `controller.is_grounded` already true, not swimming, no jump this frame, and a collider whose contact normal satisfies `|normal_y| < cos(max_slope_climb_deg)` within 36 BU below the capsule centre. Exterior rock faces and terrain on Skyrim/FNV/FO3 routinely exceed 50°; interiors mostly do not, which is why it is not visible in the FO4 session #3799 was filed from.
- **Description**: `world.rs` exposes three downward capsule casts; the walkable variant exists specifically to reject hits failing `min_walkable_normal_y` (#2193). Every other production floor probe uses the filtered form — the cold-start spawn ladder, the door-arrival ladder, `phys.census`. **The character controller's per-frame probe is the sole production caller of the unfiltered `cast_capsule_down`** (verified: every other call site is inside `world.rs`'s `#[cfg(test)]` module), and nothing says the omission is deliberate. Before `b9df7c46` that only affected a clamped motion correction; now `resolve_ground_contact` is `kcc_grounded || probe_found_support`, so the same unscreened hit sets the frame's `is_grounded`, which gates jump input and the probe's own next-frame execution.
- **Impact**: deliberately narrow. On a surface steeper than `max_slope_climb_deg`, `is_grounded` reads true on 100% of frames instead of ~50%, so jump is always available on a slope the spawn ladder would refuse (`plan_character_spawn` demotes the whole boot to FlyCam rather than stand the player there). It does **not** produce a "glued to a cliff" pin — on a slope the vertical gap is `offset / cos θ > offset`, so the correction is a real downward request and `handle_slopes` still deflects it. The larger cost is latent: `is_grounded` is currently consumed only by the controller, `save_io.rs`, `commands/view.rs` and a log — the moment the fall-damage / footstep / locomotion-state consumers #3799's own issue anticipates are written, they inherit a ground-contact signal with no walkability discipline and no comment warning them.
- **Related**: #2193, #2874, #2857, #3799.
- **Suggested Fix**: this needs a *decision*, not a mechanical filter swap — rapier's own `result.grounded` accepts anything within 89.94° of up, so applying `min_walkable_normal_y` here would make the probe stricter than the KCC it is ORed with. Either (a) switch to `cast_capsule_down_surface_and_normal` and gate only the `probe_found_support` half on the walkable normal, leaving `correction` on the raw hit, or (b) keep the behaviour and add one comment recording that the unfiltered form is intentional and why. The current silence is what makes this a drift risk rather than a design.

### PHYS-D5-2026-09-06-02: #3799's entire safety argument lives in an untested three-clause gate — all four new tests feed `probe_found_support` as a literal
- **Severity**: LOW · **Dimension**: Character Controller · **Status**: NEW
- **Location**: `byroredux/src/systems/character.rs:341` (the gate), `:1734-1795` (the three `resolve_ground_contact` tests), `:1182-1195` (the function under test)
- **Trigger Conditions**: any future edit to `character.rs:341` or to the `jump_fired` / `swim` derivations above it.
- **Description**: `b9df7c46`'s commit message states the correctness argument in one sentence — *"The probe is suppressed while airborne, swimming, and on the frame a jump fires, so this can neither keep a falling character grounded nor re-ground a launch."* All three suppressions are clauses of a single `if`: `if swim.is_none() && controller.is_grounded && !jump_fired`. **Nothing pins any of them.** The three tests added for #3799 all call the pure `resolve_ground_contact` with `probe_found_support` supplied as a hand-written `bool` — they verify the OR, never the thing that computes the operand. `character_controller_system` has no test caller anywhere in the repo. Deleting `!jump_fired` would re-ground a jump on its launch frame; deleting `swim.is_none()` would let a swimmer's probe assert ground contact off the lake bed. Both leave the suite green.
- **Evidence**: `stationary_capsule_stays_grounded_instead_of_flickering` models the production loop with `let probe_found_support = grounded; let kcc_grounded = !grounded;` — a hand-written restatement of the gate's expected behaviour, in the same file as the gate and free to disagree with it silently.
- **Impact**: no present defect. It is the structural version of this pass's theme: the fix is pinned where it is easy (a pure function) and unpinned where it is load-bearing (the impure gate). The `!jump_fired` clause is one edit away from the double-jump / hover class.
- **Related**: #3799; same shape as #2857's closing note that *"`PhysicsWorld::move_character` currently has zero unit tests, which is why this is invisible to `cargo test`"* — still open for the controller system as a whole.
- **Suggested Fix**: extract the gate to a pure `fn support_probe_enabled(swimming: bool, was_grounded: bool, jump_fired: bool) -> bool` and pin its truth table next to `resolve_ground_contact`'s; or add a source-text pin in the style of `scheduler_access_tests.rs` asserting the literal still contains all three clauses.

### PHYS-D6-2026-09-06-01: the XZ half of the collider-AABB-vs-body-origin split never landed — `0fd72cb6` fixed only Y, exactly as the predecessor finding predicted
- **Severity**: LOW · **Dimension**: Water / Buoyancy · **Status**: NEW as an issue; re-derivation of **PHYS-D6-2026-08-30-01**, which was never filed under its own number
- **Location**: `crates/physics/src/water.rs:816` (union prefilter) · `:828-840` (current-volume containment) · `:855-870` (surface containment)
- **Trigger Conditions**: a Dynamic `bhk` body whose collider is offset in **X or Z** from its rigid-body origin — a `Compound`/`List` part at its own local isometry, or a ragdoll capsule bone (which since #3492 now *reaches* this loop) — positioned within that offset of a `WaterVolume`'s or `WaterCurrentVolume`'s XZ boundary. Shorelines and river banks.
- **Description**: `0fd72cb6` hoisted one shared `compute_aabb()` above both branches and moved the **vertical** metric onto the AABB centre. The horizontal pair was left on the rigid-body origin in *all three* places it appears, so the loop decides "is this body inside the volume?" using two different reference points on two different axes. The consequence is sharper than the pre-fix state: for the surface branch, `submerged_fraction(min_y, max_y, …)` is computed from the **AABB span** while eligibility to compute it at all is decided from the **origin's** XZ, so a body mostly outside the volume horizontally can still be handed a full submerged fraction.
- **Evidence**: `git show 0fd72cb6 -- crates/physics/src/water.rs` removes and re-adds the prefilter line **verbatim** (renamed `current_flow` → `aabb_y`); no XZ predicate is touched. At HEAD both containment predicates read `pos.x` / `pos.z` alongside `center_y` / `max_y`. `.claude/issues/3490/ISSUE.md:33-38` records the sibling search as scoped to *"other body-origin-vs-collider-centre **Y** reads"*.
- **Impact**: bounded and small — the discrepancy is the compound/bone XZ offset (tens of BU) against a water plane's XZ extent (typically thousands), so it mis-classifies only bodies within their own collider offset of a shore edge: a floating corpse or crate keeps or loses lift one body-radius early. No wake, force-accumulation or damping-restore invariant is violated. The reason to file remains scope completion — now compounded by a report that says it is done (PHYS-D6-2026-09-06-02).
- **Related**: #3490 (closed), #2887, PHYS-D6-2026-08-30-01, PHYS-D6-2026-09-06-02.
- **Suggested Fix**: what the 08-30 report already prescribed — derive one `reference_point` from `collider.compute_aabb().center()` at the top of the per-body loop and use it for the union prefilter, the current-volume containment, and the surface XZ/Y containment alike. A regression test can reuse the existing offset-compound fixture with the offset on X instead of Y.

### PHYS-D6-2026-09-06-04: the kinematic player's water sampler has no `WaterCurrentVolume` arm, so a swimmer feels nothing from an authored XWCU rapids marker that every dynamic body in it drifts on
- **Severity**: LOW · **Dimension**: Water / Buoyancy · **Status**: NEW (not covered by the 09-04 pass — its declared `character.rs` entry points are the four `c7561d74` functions; `player_water_state` is not among them)
- **Location**: `byroredux/src/systems/character.rs:939-1004` (`player_water_state`) · consumed at `:251-263` (the swim current drift)
- **Trigger Conditions**: a placed REFR carrying `XWCU` + `XPRM` (a rapids / current marker) whose volume overlaps swimmable water, where the containing `WaterPlane` carries no `WaterFlow` of its own. The plane's flow comes from the CELL's own velocity; the marker's comes from the REFR — independent authoring channels.
- **Description**: `player_water_state`'s own doc says it *"mirrors the dynamic-body `WaterContact` calculation without creating a transient component for the kinematic player."* That calculation has had a placed `WaterCurrentVolume` branch since #3114/#3268. The player sampler queries `WaterPlane`, `WaterVolume` and `WaterFlow` only, so the mirror is incomplete: the swimmer's horizontal drift can only ever come from the plane's own `WaterFlow`.
- **Evidence**: `grep -n WaterCurrentVolume byroredux/src/systems/character.rs` → no hits. The dynamic-body counterpart is `collect_water_current_volumes` feeding the `current_flow` branch. `WaterCurrentVolume` is data-driven, built from `placed_ref.water_velocity` + `placed_ref.primitive`.
- **Impact**: cosmetic-to-gameplay divergence at authored rapids — a barrel dropped next to the player drifts downstream while the player does not. Bounded: the player is never pushed *wrongly*, only not pushed, and the kinematic path never touches `user_force`.
- **Related**: #3114, #3268, `docs/engine/watal.md` (the physics-half status paragraph, which describes the character path as consuming "horizontal flow" without distinguishing the two sources).
- **Suggested Fix**: give `player_water_state` the same current-volume lookup the dynamic path uses (capsule centre against `WaterCurrentVolume.volume`), returning the marker's flow when the plane has none — or, if the omission is deliberate for controller feel, say so in the doc comment instead of claiming the calculation is mirrored.

### PHYS-D7-2026-09-06-04: `awake_counts().1` is not an awake count — Rapier never drains `active_kinematic_set`, and the fast path knows it while the accessor and both render sites do not
- **Severity**: LOW · **Dimension**: Queries & Diagnostics · **Status**: NEW
- **Location**: `crates/physics/src/world.rs:278-285` (the accessor + doc); render sites `byroredux/src/commands/physics.rs:164` + `:169-174` and `crates/physics/src/sync.rs:170-182`; the contradicting in-repo note at `crates/physics/src/world.rs:521-528`
- **Trigger Conditions**: any read of the second tuple element — `phys.stats` over `byro-dbg`, or `BYRO_PROFILE=1`'s per-frame `awake dyn={} kin={}` line — in a cell containing authored-keyframed clutter, FO4+/Starfield packed-Havok proxies (registered `Keyframed`), or a live actor's keyframed ragdoll bones.
- **Description**: the first element is honest — `IslandManager::update_active_set_and_energy` drains `active_dynamic_set` every step and re-pushes only bodies that failed the sleep test. The second is not: `active_kinematic_set` is **never drained**. A body is pushed in when it becomes kinematic or when its position/colliders change, and leaves only when removed or type-changed; the step loop merely iterates it and `continue`s past every body with zero velocity. So `active_kinematic_bodies().len()` is the count of **all live kinematic bodies**, awake or not.
- **Evidence**: the repo already established this — 240 lines below the accessor, in the fast path's own rationale: *"NOTE: we deliberately do NOT gate on `active_kinematic_bodies()`. Rapier keeps every kinematic body in that set structurally for its whole life … so it's never empty in a cell with authored-keyframed clutter."* The knowledge landed where acting on it mattered and was not carried to the accessor that publishes the same number to an operator. `phys.stats` consequently prints `awake: dynamic=0 kinematic=487 pending_wake=false` and then, from the next branch, `→ quiesced: step is taking the static-scene fast path` — a self-contradiction on consecutive lines.
- **Impact**: diagnostic-only and bounded — no production decision reads the second element (all consumers use `.0` or display only), and as a *leak* signal it is well-behaved, since it only falls when bodies are genuinely removed. The cost is a misdirected investigation: an operator or a future audit reading `kinematic=487` as churn will hunt a wake-discipline bug that does not exist. `body_count()` by contrast is correct (`RigidBodySet::len()` is the live arena count).
- **Related**: #2890 / #1698 (the fast path whose comment carries the correct statement). No prior audit flagged it — every earlier mention in `docs/audits/` reads `.0`.
- **Suggested Fix**: rename to `active_island_counts` (or return a named struct) and rewrite the doc to say the second element is *"every live kinematic body — Rapier keeps them in the active set structurally, see the fast-path note"*. Relabel both render sites `awake dyn=N · kinematic bodies=M`. A source-text assertion in the style of `step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre` keeps the two statements from diverging again.

---

## Known-Open Register

### The three don't-re-litigate items

| Item | What this pass did |
|---|---|
| **`tes_grounding_zero_mass_dynamic_fix`** — Skyrim architecture ships mass=0 Dynamic-family Havok bodies, reclassified Static (19 → 416 colliders, #1832). The **mass=0 angle is CLOSED**; the door-threshold spawn gap is still open. | **Not re-filed.** Dimension 5 confirmed the *mechanism* end-to-end instead: all five 2026-08-13 PHYS-D5 fixes are live (#2858 probe blind zone, #2857 clamp, #2869 door-transition routing, #2885 skin pair, #2886 `MAX_FRAME_DT`), the query pipeline is fresh before every probe, self-exclusion holds on the spawn/door paths, and the three-rung ladder plus typed `SpawnProbeVerdict` census is intact. **The controller side does not break anywhere.** What remains open is whether a walkable collider exists at the threshold at all — a content/collision-import question. Unchanged from the 08-30 verdict. |
| **`interior_spawn_point_fix`** — interiors spawn at the first door's own placement; vanilla `coc` has no auto spawn-point logic. | Untouched; no finding assumes one exists. |
| **`fnv_furniture_sit_needs_transition`** — sit loops have no pelvis/root channel; M42 seat-snap gated behind `BYRO_SANDBOX_SIT`. | Outside every dimension's file set this run. |

### Deliberately deferred, verified still deferred — not gaps

- **`bhkBallAndSocketConstraint` / `bhkStiffSpringConstraint`** remain the only undecoded constraint kinds; both bare and breakable-wrapped forms still fall into `Other` (`constraints.rs:679`, `:305-315`, `:894-903`). Deferred by #3792's own checklist. **Not re-filed** — doing so needs a fresh occupancy census, which this run was forbidden from producing.
- **FO4+ / FO76 / Starfield ragdolls** blocked on the opaque `BhkSystemBinary` payload (open as **#3809**). Verified the blocked case is still reported *as blocked* rather than as "no collision": `needs_packed_havok_fallback()` has three production consumers, and `scene/nif_loader.rs:343-355` (#2882) logs the "constraint graph is inside an undecoded `BhkSystemBinary` … documented PHYSAL limitation, not an unrigged asset" line so a `ragdoll <id>` failure is distinguishable from a rock.
- **Havok cone+2-plane → Rapier per-axis limit mapping** remains a documented approximation; **motors captured but unused**. Prismatic `friction` joins them (folded into PHYS-D4-2026-09-06-02's doc fix rather than filed).
- **`plane_min` / `plane_max` swing limits** dropped at `joint_from_imported` — documented in-source, in `physal.md` § Known approximation, and closed as #1982. Third consecutive pass to check it; not re-filed.
- **WATAL open items**, re-read from `docs/engine/watal.md` rather than inherited: water-walking and freezing (`:438`), the exact Skyrim DNAM tail decode, the cross-game visual smoke matrix and the real-data traversal gates, the `XNAM`→`SPEL` water-quality runtime, and replacing the EDID `WaterKind` heuristic with data-driven classification. None filed.

### Open issues verified still true, cited not re-filed

- **#3477** — `collect_newcomers` rescans every `CollisionShape` row per tick to answer "nothing new" (`sync.rs:848-864`). Still an unconditional `shape_q.iter()` walk with a `handles_q.contains` probe per row; no dirty set or insertion queue. Owned by `/audit-performance` Dim 1. Independently re-verified by Dimensions 3 and 7.
- **#3809** — FO4 precombine collision / `BhkSystemBinary` decoder research spike.
- **#3495** — `byroredux/src/commands/physics.rs` still absent from CLAUDE.md's tree. `/audit-tech-debt` Dim 7.

---

## Cross-Audit Deduplication

| Topic | Owner | Note |
|---|---|---|
| Lock ordering / `Access` declaration completeness as a class | `/audit-concurrency` Dim 4–5 | PHYS-D6-2026-09-06-03 is filed **here** because #3492 is this audit's own fix; the physics-crate lock discipline itself traced clean (Dim 3) |
| `combat.approach`'s `PhysicsWorld → RapierHandles → GlobalTransform → PhysicsWorld` cycle | `/audit-concurrency` | already routed by the 2026-08-30 pass; unchanged |
| `unsafe` blocks | `/audit-safety` | this file set contributes none |
| Water rendering, wave shader, foam/shoreline | `/audit-renderer` Dim 15 | — |
| `XCLW` tri-state / WATR DNAM decode | `/audit-esm` Dim 5 | — |
| `bhk*` block parsing | `/audit-nif` Dim 5 | the constraint decode itself is in-scope here as the PHYSAL source seam |
| `CollisionShape` translation as a NIFAL boundary | `/audit-nifal` Dim 6 | PHYS-D1-2026-09-06-01 is filed here because the double-application is a *solver-side* consequence |
| `collect_newcomers` per-tick rescan (#3477) | `/audit-performance` Dim 1 | cited, not re-filed |
| `docs/engine/physics.md` prose generally | `/audit-tech-debt` Dim 7 | **except** the lines in PHYS-D3-2026-09-06-02, which govern a live HIGH and belong with its fix |
| Debug-server command surface as a whole | un-owned (see `_audit-common.md`) | — |

---

## Verification

- **Every finding's load-bearing claim was independently re-derived by the orchestrator from the code**, not accepted from the dimension agent. For the two HIGHs this meant tracing each link separately: for D1-01 the producer's `* ref_scale.abs()`, the ghost's `Parent` + local scale 1.0, propagation's unconditional `*g = composed`, `compose`'s `parent_scale * local_scale` term, `placement_root`'s `ref_scale`, and the converter's second multiply; for D3-01 the `GlobalTransform::IDENTITY` seed, the `studio_mode`-only propagation gate, `diagnostic_scene`'s five disjuncts, the `spawn_plan` branch condition, the one-shot collect gate, and both bootstrap call sites.
- **One agent overstatement was caught and corrected.** Dimension 4 reported HEAD as `229306ce` while the brief said `a8233f2f`; verified that the three intervening commits touch nothing in physics scope, so its citations hold at both revisions. Its claim that `a8233f2f` (#3921) is test-only was independently confirmed (`+32/-1`, one `#[ignore]`d test plus a module-doc line).
- **One orchestrator grep was too narrow and the agent was right.** D7-02's claim that `commands/assets.rs` reads `NifImportRegistry` live initially appeared unsupported; a wider grep confirmed `assets.rs:523` does `world.try_resource::<crate::cell_loader::NifImportRegistry>()`. The finding's premise stands.
- **Negative results were required to show their work.** Dimension 5 was asked whether #3799's grounded latch could outlive its evidence and returned a disproof, not a finding: `probe_found_support` is a per-frame stack local re-derived from a live cast, `remove_body` sets `colliders_dirty` which **both** arms of `step` flush, and the worst case is one stale frame. Dimension 6 was told explicitly that finding nothing was a legitimate result — and then found that the report it was asked to re-verify was itself wrong.
- **Six candidates in Dim 5, nine in Dim 2, seven in Dim 6, six in Dim 7, seven in Dim 4 and six in Dim 1 were investigated and dropped** after failing the disprove bar; each is recorded in its dimension scratch file with its disproof, several specifically because they are traps a future pass would otherwise re-derive.
- **No corpus census was run** and no engine was launched. Every occupancy figure is re-quoted from a cited prior measurement, and each finding whose premise depends on unmeasured occupancy says so.
- `cargo test -p byroredux-physics` — 156 passed, 0 failed, 0 ignored — run **once**, by the orchestrator, before dispatch.

## Summary

Eighteen findings, and not one of them is a fresh defect in code nobody had
recently touched. Seven of seven dimensions found the same thing: a fix that
closed on fewer sites than its own evidence named. The two HIGHs are both
invisible at runtime — an `XSCL²` collider on a correct-looking mesh, and a
collider registered at the world origin with no log — and both were missed by
four prior passes for the same structural reason: every earlier review verified
`convert.rs` and `sync.rs` *in isolation*, and these defects only exist across
the producer → ECS-propagation → converter boundary.

The 2026-08-30 pass wrote that "the risk is in how fixes are scoped, not in the
code drifting." This pass turned that from an observation into a measurement.
The corresponding process change is small and specific: **an audit that closes
finding A by absorbing finding B must verify B's predicate independently**, and
**a fix must enumerate every site of its defect class before it closes** —
Dimension 6 found the first rule broken in a report two days old, and Dimensions
1, 3 and 7 found the second broken in commits that shipped with passing tests.
