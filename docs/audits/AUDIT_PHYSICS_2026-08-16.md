# PHYSAL / Physics Audit — 2026-08-16

**Scope**: `crates/physics/` (world, sync, convert, components, config, ragdoll,
water) + `byroredux/src/ragdoll.rs` + `byroredux/src/systems/character.rs`, with
the collider-producer sites in `byroredux/src/cell_loader/spawn.rs` and the parse
side `crates/nif/src/import/collision/`.

**Depth**: deep. **Games traced**: shape/scale path is game-agnostic; the
synthesized-trimesh producer traced against the FO4/Starfield
`MissingCollisionFallback::ArchitectureTriMesh` route and the exterior LAND route.

**Prior pass**: `docs/audits/AUDIT_PHYSICS_2026-08-13.md` (21 of its findings are
still OPEN as #2862–#2890). This pass is largely a re-verification of that report
plus a delta audit of everything that landed in the physics-owned files since it
was written — commits `264f44fd` (#2866–#2869), `d8dc7608` (#2872–#2875),
`4de5e78e` (#2834/#2860/#2861), `eb5d76fe` and `869cdf76`.

`cargo test -p byroredux-physics`: **104 passed, 0 failed**.

---

## Executive Summary

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| 1 — Shape Translation | 0 | 2 | 1 | 0 |
| 2 — Step Determinism & Budget | 0 | 0 | 0 | 0 |
| 3 — ECS Sync | 0 | 0 | 0 | 1 |
| 4 — Ragdoll Articulation | 0 | 0 | 0 | 0 |
| 5 — Character Controller | 0 | 0 | 0 | 0 |
| 6 — Water / Buoyancy | 0 | 0 | 0 | 0 |
| 7 — Queries & Diagnostics | 0 | 0 | 0 | 0 |
| **Total** | **0** | **2** | **1** | **1** |

Every dimension was walked. Dimensions 2, 4, 5, 6 and 7 produced **no new
findings** — their open items are all already filed from the 2026-08-13 pass and
were re-verified as still-open rather than re-reported. Dimension 4's two new
findings both reduce to the same defect as Dimension 1's first one, so it is
filed once, under Dimension 1.

### The headline: the two independent scale fixes now overlap

The 2026-08-13 pass filed two separate "the collider ignores the placement
scale" findings — **#2860** (PHYS-D1-01, authored-bhk colliders) and **#2868**
(PHYS-D4-01, ragdoll pivots + shapes). Both are CLOSED and both fixes are in the
tree. They were written against different layers of the same pipeline and
**neither knew about the other**, so two of the three collider producers now
apply the same uniform scale twice and hand Rapier `scale²` geometry:

| Producer | Scale applied at | Scale applied again at | Net |
|---|---|---|---|
| authored `bhk` (`spawn_collision_shapes`) | — | `register_newcomers` (#2860) | correct ×1 |
| `spawn_packed_havok_proxy` | `synthesize_packed_havok_proxy` | — (stores `GlobalTransform` scale 1.0) | correct ×1 |
| `spawn_trimesh_collider_ghost` | `synthesize_static_trimesh` | `register_newcomers` (#2860) | **×scale²** |
| ragdoll body shape | `activate_ragdoll` (#2868) | `build_ragdoll` (#2860 sibling) | **×scale²** |

This is invisible at `scale == 1.0`, which is the overwhelming majority of
placements — and it is invisible to `cargo test`, because each crate's own test
pins its own half of the contract in isolation
(`ragdoll_collider_shape_follows_the_bone_seed_scale` asserts the physics crate
scales an *unscaled* spec shape; nothing tests the composed engine→crate path).

### PHYSAL doctrine verdict — HOLDS

`grep -rn "GameKind|bsver|NifVersion|game_kind"` over `crates/physics/src/` and
`byroredux/src/ragdoll.rs` returns **nothing**. No game or version discriminator
has leaked past the parse-side constraint decode. The doctrine claim in
`docs/engine/physal.md` §3 is still true of the code (the *wording* problem that
#2883 describes is unchanged and is not re-filed here).

---

## Solver Invariant Matrix

| Invariant | State | Evidence |
|---|---|---|
| Fixed step: accumulator clamped **before** the loop | ✅ VERIFIED | `world.rs:379-383` |
| Negative / NaN `frame_dt` guarded | ✅ VERIFIED | `frame_dt.max(0.0)`; Rust `f32::max` returns the non-NaN operand |
| Anti-spiral budget starts before substep 1, checked after | ✅ VERIFIED | `world.rs:415`, `world.rs:449` |
| `pending_wake` consumed only when a substep ran | ✅ VERIFIED | `world.rs:466` (#2856 fix intact) |
| Query pipeline rebuilt once per frame, never per substep | ✅ VERIFIED | `None` passed to `pipeline.step`; `world.rs:502` |
| Determinism: no wall-clock or map order into the solver | ✅ VERIFIED | budget `Instant` is truncating-only |
| Phase order collect → push → buoyancy → step → pull | ✅ VERIFIED | `sync.rs:112-149` |
| Newcomer registration idempotent | ✅ VERIFIED | `sync.rs:728` + #2867 gate at collect time |
| Teardown completeness (cell unload + ragdoll) | ✅ VERIFIED | `unload.rs:446`, `ragdoll.rs:569`, `world.rs:219` |
| Contact skin applied to every collider producer | ✅ VERIFIED | `sync.rs:830`, `ragdoll.rs:262` (#2861 intact) |
| Placement scale reaches the collider **exactly once** | ❌ **DRIFTED** | PHYS-D1-2026-08-16-01 / -02 |
| Degenerate shape input cannot reach Rapier | ⚠️ PARTIAL | TriMesh guarded; `ConvexHull` vertex count is not — PHYS-D1-2026-08-16-03 |
| PHYSAL: constraint CInfo decode is the only per-game seam | ✅ VERIFIED | no game/version symbol in the solver path |

---

## Findings

### HIGH

#### PHYS-D1-2026-08-16-01: Synthesized static-trimesh colliders are scaled twice — every scaled REFR on the missing-collision fallback gets a `scale²` collider

- **Severity**: HIGH
- **Dimension**: Shape Translation
- **Location**: `byroredux/src/cell_loader/spawn.rs:327-368` (producer) ·
  `byroredux/src/cell_loader/spawn.rs:395-396` (placement) ·
  `crates/physics/src/sync.rs:786` (second application)
- **Status**: NEW — a defect *in* the fix for #2860 (commit `4de5e78e`), not a
  regression of the original bug. #2860 is correctly closed for the authored-bhk
  path it was written about.
- **Trigger Conditions**: a REFR whose composed `ref_scale × mesh.scale != 1.0`
  that takes the `MissingCollisionFallback::ArchitectureTriMesh` route — i.e.
  any NIF with no authored `bhk` collision. That is the *primary* static-collision
  path for FO4 / FO76 / Starfield architecture. Exterior LAND terrain is
  unaffected (its call site passes `1.0`).
- **Description**: `synthesize_static_trimesh` bakes `world_scale` into every
  vertex, and `spawn_trimesh_collider_ghost` then stores that same scale in the
  ghost's `Transform` **and** `GlobalTransform`. Since #2860,
  `register_newcomers` reads `n.global.scale` and multiplies the already-baked
  geometry by it again. The sibling producer `spawn_packed_havok_proxy` gets this
  right — it bakes the scale and deliberately stores `GlobalTransform` scale
  `1.0` — so the two ghost producers now disagree with each other.
- **Evidence**:
  ```rust
  // spawn.rs:343 — scale baked into the vertices
  .map(|p| Vec3::new(p[0] * world_scale, p[1] * world_scale, p[2] * world_scale))
  // spawn.rs:396 — and stored on the ghost's GlobalTransform
  world.insert(ghost, GlobalTransform::new(pos, rot, scale));
  // sync.rs:786 — and applied a second time at collider creation
  let parts = collision_shape_to_parts(&n.shape, n.global.scale, &cfg);
  ```
  The stale premise is still in the tree as a comment, and is the proof that the
  two edits never met — `spawn.rs:318-320`: *"baking `world_scale` into the
  vertices so the collider matches the rendered geometry (the physics sync places
  bodies by translation+rotation only, ignoring `GlobalTransform` scale)"*. That
  parenthetical stopped being true with #2860.
  Contrast `spawn.rs:277`, which is correct:
  `world.insert(ghost, GlobalTransform::new(world_center, world_rot, 1.0));`
- **Impact**: static world collision is the wrong size by `scale²`. A `2.0` REFR
  gets a 4× collider — invisible walls extending well past the drawn geometry and
  overlapping neighbouring architecture; a `0.5` REFR gets a 0.25× collider, so
  the player walks through the visible surface or falls through the floor. This
  is the same blast radius as #2860 itself, with the sign inverted.
- **Related**: #2860 (PHYS-D1-01, CLOSED), PHYS-D1-2026-08-16-02 (same defect
  class in the ragdoll producer), #2878.
- **Suggested Fix**: pick one owner of the scale per producer. The smallest edit
  that matches the corrected sibling is to store `1.0` in the ghost's
  `GlobalTransform` (the ghost is physics-only — it carries no `MeshHandle`, so
  nothing else reads that scale), and update the stale
  `synthesize_static_trimesh` doc comment in the same edit. Pin it with a test
  that spawns a ghost at scale 2 and asserts the registered collider's AABB, not
  the shape in isolation.

#### PHYS-D1-2026-08-16-02: Ragdoll limb colliders are scaled twice — a scaled actor's rig is `scale²` geometry on `scale¹` articulation

- **Severity**: HIGH
- **Dimension**: Shape Translation / Ragdoll Articulation
- **Location**: `byroredux/src/ragdoll.rs:314` (first application) ·
  `crates/physics/src/ragdoll.rs:250` (second application)
- **Status**: NEW — a defect created by the interaction of the fix for #2868
  (commit `264f44fd`) with the later fix for #2860 (commit `4de5e78e`). Both
  issues are CLOSED and both are individually correct in isolation.
- **Trigger Conditions**: ragdoll activation (death, or the `ragdoll <id>`
  console command) on an actor whose bone `GlobalTransform.scale != 1.0` —
  Skyrim children (0.85), giants (1.7), and any creature/NPC carrying a non-unit
  `XSCL`. Unit-scale actors are untouched.
- **Description**: `activate_ragdoll` writes `shape: b.shape.scaled(gt.scale)`
  into the `RagdollBodySpec` *and* records `scale: gt.scale` on the same spec.
  `build_ragdoll` then calls `collision_shape_to_parts(&…, b.scale, cfg)`, which
  multiplies the already-scaled geometry by the same factor again. The crate's own
  test `ragdoll_collider_shape_follows_the_bone_seed_scale`
  (`crates/physics/src/ragdoll.rs:665`) pins the *crate* contract — "the spec
  carries a bind-space shape, `build_ragdoll` applies `b.scale`" — and passes,
  because it constructs the spec directly and never goes through
  `activate_ragdoll`. The engine violates that contract, and nothing tests the
  composed path.
- **Evidence**:
  ```rust
  // byroredux/src/ragdoll.rs:311-314
  scale: gt.scale,
  shape: b.shape.scaled(gt.scale),   // #2868
  // crates/physics/src/ragdoll.rs:250
  let parts = collision_shape_to_parts(&ragdoll_dynamic_shape(&b.shape), b.scale, cfg); // #2860
  ```
  `RagdollTemplate.bodies[].shape` is confirmed unscaled at its source —
  `template_from_imported` stores `shape: b.shape.clone()`
  (`byroredux/src/ragdoll.rs:149`). The joint pivots are *not* affected: they are
  scaled exactly once, in `activate_ragdoll` via `scaled_pivots`, and
  `build_joint` is scale-agnostic.
- **Impact**: the rig is self-inconsistent in the opposite direction to the one
  #2868 fixed — limb colliders at `scale²` around body poses and joint frames
  seeded at `scale¹`. On an up-scaled actor (1.7 → 2.89×) adjacent limbs start
  deeply interpenetrating at build time, and a multibody with mutually
  penetrating links resolves explosively on the first step; on a down-scaled
  actor the limbs are undersized and the ragdoll passes through itself and the
  floor. Bounded to scaled actors and to the frame of death, so it degrades a
  visual set-piece rather than the walkable world.
- **Related**: #2868 (PHYS-D4-01, CLOSED), #2860 (CLOSED),
  PHYS-D1-2026-08-16-01.
- **Suggested Fix**: drop the `.scaled(gt.scale)` at `byroredux/src/ragdoll.rs:314`
  and keep `scale: gt.scale`. That restores the documented crate contract (spec
  carries bind-space geometry + a scale field; the sink boundary applies it) and
  leaves the already-correct pivot and seed-pose scaling untouched. Add an
  engine-side test that activates a ragdoll on a scale-2 skeleton and asserts the
  built collider's radius, which is the assertion neither crate currently owns.

### MEDIUM

#### PHYS-D1-2026-08-16-03: A `ConvexHull` with fewer than 3 vertices panics inside parry — the documented `None` fallback does not exist for that input

- **Severity**: MEDIUM
- **Dimension**: Shape Translation
- **Location**: `crates/physics/src/convert.rs:243-254`
- **Status**: NEW
- **Trigger Conditions**: a `BhkConvexVerticesShape` whose on-disk
  `Num Vertices` is 0, 1 or 2 — corrupt, truncated, or unusually-authored
  (modded) collision. Vanilla content authors ≥ 4.
- **Description**: the arm treats `SharedShape::convex_hull` as total, relying on
  its `Option` return for degenerate input:
  *"falls back to a tiny ball if the hull is degenerate — Rapier rejects fewer
  than 4 non-coplanar points"*. That premise is wrong. `SharedShape::convex_hull`
  → `ConvexPolyhedron::from_convex_hull` → `parry::transformation::convex_hull`,
  and that function is `try_convex_hull(points).unwrap()`
  (`parry3d-0.17.6/src/transformation/convex_hull3/convex_hull.rs:11`).
  `try_convex_hull` returns `Err(ConvexHullError::IncompleteInput)` for
  `points.len() < 3`, so the call **panics** before the `Option` the
  `unwrap_or_else` is waiting for can ever be produced.
- **Evidence**: confirmed empirically with a temporary integration test against
  the real dependency (since deleted):
  ```
  thread 'probe_convex_hull_empty' panicked at parry3d-0.17.6/.../convex_hull.rs:11:29:
  called `Result::unwrap()` on an `Err` value: IncompleteInput
  thread 'probe_convex_hull_with_two_points' panicked at ... IncompleteInput
  test probe_convex_hull_coplanar ... ok   // 4 coplanar points DO return a shape
  ```
  Reachability: `num_vertices` is read straight from the file
  (`crates/nif/src/blocks/collision/shape_compound.rs:30`), and the importer arm
  filters non-finite vertices but never the count
  (`crates/nif/src/import/collision/shape.rs:179-191`). The TriMesh sibling arm
  *does* guard its own degenerate cases at this same choke point
  (`convert.rs:261`), so the asymmetry is the defect.
- **Impact**: an unrecoverable process panic during cell load on malformed or
  unusual collision data — the failure mode #1779 and #2543 both exist to
  prevent, on the one shape arm neither of them covered. Not reachable from
  vanilla archives, which is why it has never fired.
- **Related**: #2878 (same function, index-range half), #2862 (`BhkTransformShape`
  finite guard), #1779, #2543.
- **Suggested Fix**: reject `vertices.len() < 3` in the `ConvexHull` arm of
  `flatten_to_parts` before calling `convex_hull`, falling through to the same
  tiny-ball placeholder the arm already uses, and correct the doc comment's claim
  about what Rapier rejects. Optionally mirror the guard at the importer boundary
  (`resolve_shape`) so a degenerate hull becomes `None` and the cell loader's
  synthesized-collision fallback can take over instead.

### LOW

#### PHYS-D3-2026-08-16-04: `register_newcomers`' `parts.is_empty()` skip is unreachable

- **Severity**: LOW
- **Dimension**: ECS Sync
- **Location**: `crates/physics/src/sync.rs:787-789`
- **Status**: NEW
- **Trigger Conditions**: none — the branch cannot be taken.
- **Description**: `collision_shape_to_parts` guarantees a non-empty result: it
  pushes a `SharedShape::ball(1e-3)` placeholder when the flatten produced no
  leaves (`convert.rs:144-147`), and the contract is pinned by
  `empty_nested_compound_falls_back_to_ball_part`. The `continue` in
  `register_newcomers` is therefore dead, and it reads as a live safety net for a
  case that is actually handled one layer down — which is misleading in the exact
  function where a genuine early-`continue` would leak (#2867's lesson).
- **Evidence**:
  ```rust
  let parts = collision_shape_to_parts(&n.shape, n.global.scale, &cfg);
  if parts.is_empty() { continue; }   // collision_shape_to_parts never returns empty
  ```
- **Impact**: none at runtime; a small correctness-reasoning hazard in a function
  whose skip paths are load-bearing.
- **Related**: #2867, #2878.
- **Suggested Fix**: delete the branch and note in the call site's comment that
  the placeholder-part guarantee lives in `collision_shape_to_parts`; or, if the
  defensive shape is preferred, replace it with a `debug_assert!`.

---

## Disproved Candidates

Recorded so the next pass does not re-derive them.

- **`move_character` does not exclude sensors.** `QueryFilter::default()` leaves
  `EXCLUDE_SENSORS` unset and rapier 0.22's `KinematicCharacterController` does
  not add it (`rapier3d-0.22.0/src/control/character_controller.rs` only ever ORs
  in `EXCLUDE_DYNAMIC`), so a trigger volume would block the player. **Inert**:
  nothing in the engine creates a sensor collider — `.sensor(` / `set_sensor`
  appear only in the spawn-census plumbing and in tests. Not filed; worth
  re-checking the day `TriggerVolume` grows a real Rapier body.
- **Out-of-range TriMesh indices panic in `TriMesh::with_flags`.** True of parry,
  but both production producers already range-check:
  `finish_trimesh` retains only in-range triangles
  (`crates/nif/src/import/collision/shape.rs:707-712`) and
  `synthesize_static_trimesh` skips out-of-range triangles
  (`byroredux/src/cell_loader/spawn.rs:349-352`). The missing choke-point check is
  already #2878.
- **`ground_character_body_at`'s relaxation to `&World` (commit `869cdf76`) opens
  a lock-order hazard.** It performs only non-structural `query_mut` writes plus a
  reentrant `physics_sync_system(world, 0.0)`. Its console caller
  (`byroredux/src/commands/view.rs:186-190`) holds no guard across the call: the
  `world.try_resource::<PhysicsWorld>()` temporary is dropped at the end of the
  `if` condition's temporary scope, and command dispatch runs inside the
  `Stage::Late` exclusive debug drain. No deadlock edge.
- **Ragdoll activation leaves each bone doubly represented in Rapier.** Handled:
  #1772 frees the bone's keyframed body and drops both `RigidBodyData` and
  `RapierHandles` (`byroredux/src/ragdoll.rs:400-427`).
- **`keyframe_live_ragdoll_bones` may run after the bones were already
  registered, under the budgeted resumable NPC spawn.** It does not: the call is
  in the same `RuntimePhase::Skeleton` unit that creates the bone entities
  (`byroredux/src/npc_spawn/resumable.rs:661`, `:1118`), so no
  `physics_sync_system` tick can intervene. #2873's ordering premise holds.
- **`current_force` is unbounded at high flow.** It is a first-order response
  toward the authored speed, scaled by submerged fraction, with every non-finite
  or negative input zeroed (`crates/physics/src/water.rs:123-150`). Converges;
  the doc's claim is accurate.

---

## Known-Open Register

The three don't-re-litigate items, and what this pass changed about them:

1. **`tes_grounding_zero_mass_dynamic_fix`** — mass=0 Dynamic Skyrim architecture
   reclassified Static (#1832). Untouched by this pass; not re-investigated. The
   door-threshold spawn gap remains open, and this pass adds nothing to it.
2. **`interior_spawn_point_fix`** — interiors spawn at the first door's own
   placement; there is no vanilla auto-spawn-point. Untouched.
3. **`fnv_furniture_sit_needs_transition`** — sit loops have no pelvis/root
   channel; M42 seat-snap stays behind `BYRO_SANDBOX_SIT`. Untouched.

Additionally, the 21 open findings from `AUDIT_PHYSICS_2026-08-13.md` (#2862,
#2863, #2864, #2865, #2870, #2871, #2876, #2877, #2878, #2879, #2880, #2881,
#2882, #2883, #2884, #2885, #2886, #2887, #2888, #2889, #2890) were each checked
against current code and are all still live. None is re-filed here. Two of that
report's closed findings were re-verified as fixed-but-overshooting and are the
subject of PHYS-D1-2026-08-16-01 and -02.

---

## Cross-Audit Dedup

- Storage-read-across-resource-guard in `push_kinematic` / `pull_dynamic` →
  `/audit-concurrency` Dim 5, already #2404. Not re-filed.
- `unsafe` → none in `crates/physics/`; nothing for `/audit-safety`.
- Water rendering half → `/audit-renderer` Dim 15 (#2782, #2787, #2789, #2790).
- `bhk*` wire parsing → `/audit-nif` Dim 5.
- `CollisionShape` resolution → `/audit-nifal` Dim 6. PHYS-D1-2026-08-16-03's
  optional second half (guarding the hull vertex count at `resolve_shape`) lands
  in that layer.
- `byroredux/src/combat.rs`'s use of `ActorColliderOwner` and its camera ray sit
  in the **un-owned gameplay slice** (`_audit-common.md` § coverage gaps). This
  pass read the resolution path but did not audit combat's invariants.

---

## Recommended Fix Order

1. **PHYS-D1-2026-08-16-01** — wrong static world collision, the widest blast
   radius, and a two-line edit.
2. **PHYS-D1-2026-08-16-02** — same class, same one-line shape; fix both together
   and add the composed-path tests neither crate owns, so the next scale fix
   cannot re-open either.
3. **PHYS-D1-2026-08-16-03** — cheap guard, removes a hard panic on untrusted
   input; natural to land alongside #2878, which is in the same function.
4. **PHYS-D3-2026-08-16-04** — delete the dead branch during any of the above.

---

*Report ready. Publish with:*

```
/audit-publish docs/audits/AUDIT_PHYSICS_2026-08-16.md
```

*(there is no `physics` domain label — map to `legacy-compat`, or `tech-debt`
for PHYS-D3-2026-08-16-04.)*
