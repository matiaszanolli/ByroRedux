# PHYSAL / Physics Audit — 2026-08-27b (full 7-dimension pass)

**Run**: `/audit-physics --depth deep`, executed as part of an `audit-suite
--preset comprehensive` run. Solo execution — no sub-agent fan-out
(`feedback_audit_suite_nested_agent_relay`); every dimension was read, grepped
and traced directly. No engine process was launched
(`feedback_no_parallel_engine_launch`).

**Filename note**: `docs/audits/AUDIT_PHYSICS_2026-08-27.md` was already taken
by an earlier same-day preset run that covered **Dimension 3 only**. This
report is the `-27b` sibling and is the full-scope pass; it does not supersede
the `-27` report's two findings, it reconciles them (both still open).

**Scope**: `crates/physics/src/` (world, sync, convert, components, config,
ragdoll, water, lib) + `byroredux/src/ragdoll.rs` +
`byroredux/src/systems/character.rs` + `byroredux/src/commands/physics.rs` +
`byroredux/src/commands/water.rs` + the parse side
`crates/nif/src/import/collision/{shape,ragdoll,mod}.rs` + the victim-list
producers in `byroredux/src/cell_loader/{exterior,load,unload}.rs` +
`crates/core/src/ecs/components/{collision,water,global_transform}.rs`.

**Tests**: `cargo test -p byroredux-physics` — **153 passed, 0 failed**
(151 at the `-27` pass, 148 at the 08-24 pass).
`cargo check -p byroredux-physics --tests` — **1 warning**, see
PHYS-D6-2026-08-27b-03.

**Delta audited**: `07a029ea..HEAD` over `crates/physics/src/` is +506/−114
(`sync.rs`, `water.rs`, `lib.rs`), plus +111 in `byroredux/src/ragdoll.rs` and
+227 in `byroredux/src/systems/character.rs`. That covers `bbfd742f` (#3268
current-volume wake), `6aa3d8f6` (#3317/#3318/#3319 FNV ragdoll geometry),
`ee9e3b6b` (#2339), `303329d9` (#3125/#3128/#3130), `df162912` (#3303),
`b5bd561a` (#3267), `1d6131b8` (#3266), `fed2e6ab` (#3265), `d8d952b1` (#3260),
`c1fe3ae2` (#3304).

**Games traced**: the solver path is game-agnostic (doctrine re-verified below).
Shape translation was traced against the shared classic-`bhk` producer
(Oblivion / FO3 / FNV / Skyrim LE+SE) and the FO4+ `BhkNPCollisionObject`
opaque-payload census route; ragdoll articulation against the FNV corpus
figures `6aa3d8f6` cites; water against the FO3/FNV `XWCU` current-marker
producer.

---

## Executive Summary

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| 1 — Shape Translation | 0 | 0 | 0 | 0 |
| 2 — Step Determinism & Budget | 0 | 0 | 0 | 0 |
| 3 — ECS Sync | 0 | 0 | 0 | 0 |
| 4 — Ragdoll Articulation | 0 | 0 | 0 | 0 |
| 5 — Character Controller | 0 | 0 | 0 | 0 |
| 6 — Water / Buoyancy | 0 | 0 | 2 | 1 |
| 7 — Queries & Diagnostics | 0 | 0 | 0 | 1 |
| **Total (new)** | **0** | **0** | **2** | **2** |

Four new findings, none above MEDIUM. Both MEDIUMs live in the WATAL physics
sink and share one root question: **which point on a body does the water
model measure?** `#2887` fixed the surface branch to measure from the collider
AABB centre instead of the rigid-body origin; the current-volume branch
twenty-six lines above it in the same loop was never updated (finding 01). And
the buoyancy scan's target set is `RapierHandles` × `RigidBodyData::Dynamic` —
the two components `activate_ragdoll` deliberately strips from every ragdoll
bone — so the one class of dynamic body a player is most likely to see in
water is the one class the sink cannot see (finding 02).

The rest of the layer re-verified clean. **Dimensions 1–5 produced no new
findings on a line-by-line re-read**: the `#3238` shared `clamp_shape_extent`,
the `#2867` collect-time registration gate, the `#1698` anti-spiral budget, the
`#2856` deferred `pending_wake` consumption, the `#2860` scale-at-the-boundary
rule, and the `#3303`/`#3266`/`#3260` lock-order splits are all present and
correct at HEAD.

### PHYSAL doctrine verdict — **HOLDS**

```
$ grep -rn "GameKind\|bsver\|NifVersion\|game_kind\|is_skyrim\|is_fo4\|game ==" \
        crates/physics/src/ byroredux/src/ragdoll.rs
(no matches)
```

Not even a comment mentions a game or version discriminator any more — a
tightening since the 08-24 pass, which still found Skyrim/Oblivion rationale
notes in prose. The constraint CInfo decode remains the only per-game seam and
`docs/engine/physal.md` §3 still matches the code. The FO4+ opaque
`BhkSystemBinary` case is still surfaced as *blocked*, not as "no collision",
by `spawn_collider_census_report`'s `new_physics>0` arm
(`crates/physics/src/sync.rs:715-726`).

One notable *strengthening* since 08-24: `#3318` (`6aa3d8f6`) reversed the
`bhkRigidBodyT` CInfo reading in `template_from_imported`
(`byroredux/src/ragdoll.rs`) from "skeleton-root space, subtract the bone rest
pose" to "owning-NiNode-relative, use verbatim", on the strength of a corpus
measurement (FNV median authored |translation| 8.6 units; 351/351 T bodies came
out >1 unit off under the old reading). That is exactly the shape of change the
doctrine wants — a *decode* correction on the parse side, no solver-side branch.

### Premises checked and disproved (not filed)

Recorded because `feedback_audit_findings` says roughly 1 in 6 findings in past
sweeps was stale, and a disproved premise is worth more in the record than
silently dropped.

- **"The walkable-surface test's `normal_y.abs()` is an undocumented magic
  behaviour."** It is documented, on the function it belongs to:
  `crates/physics/src/world.rs:868-870` — *"Normal orientation is deliberately
  ignored: legacy Havok architecture can be consistently inward-wound, but its
  geometric slope is still a valid basis for deciding whether a surface is
  walkable."* Drafted as a LOW, withdrawn on reading the doc block. The residual
  (no test feeds a negative normal) is below the reporting floor.
- **"`remove_ragdoll` leaks the multibody joints it stored."** It does not —
  `RigidBodySet::remove` is called with `&mut self.multibody_joints` and
  `remove_attached_colliders = true` (`world.rs:246-266`), so bodies, colliders
  and joints all cascade. Pinned by the `#2884` repeat-cycle test
  (`crates/physics/src/ragdoll.rs:1134-1185`).
- **"`register_newcomers_and_refresh_queries` (#3267) leaves `colliders_dirty`
  set, causing a redundant rebuild in the next `step`."** It does not —
  `update_query_pipeline` clears the flag (`world.rs:712-715`).
- **"`ConvexHull` has no non-finite vertex guard at the `convert.rs` choke
  point the way `TriMesh` does (#1779)."** True as stated, but both producers
  guard: `crates/nif/src/import/collision/shape.rs:188-190` rejects a non-finite
  hull outright, and `ragdoll_dynamic_shape`'s TriMesh→hull substitution
  (`crates/physics/src/ragdoll.rs:206-209`) can only be reached through the
  already-guarded TriMesh path. Defense-in-depth asymmetry only; below the floor.

---

## Solver Invariant Matrix

| Invariant | State | Evidence |
|---|---|---|
| Every `CollisionShape` variant has a translation arm (7/7) | ✅ VERIFIED | `crates/core/src/ecs/components/collision.rs:24-48` vs `crates/physics/src/convert.rs:183-360` |
| Compound child transform composes parent-then-child, scale applied per level | ✅ VERIFIED | `convert.rs:191-214`; the `s·t₁ + R₁·(s·t₂) = s·(t₁ + R₁·t₂)` note at `convert.rs:170-176` |
| Non-finite Compound child `(t, r)` neutralised in release builds | ✅ VERIFIED | `convert.rs:211-212` (#2862 backstop under the `debug_assert!`) |
| Every primitive extent routed through the shared clamp | ✅ VERIFIED | `clamp_shape_extent` (`convert.rs:26-32`) called by Ball `:220`, Cuboid `:246-248`, Capsule `:259-260`, Cylinder `:271-272` (#3238) |
| Caller-supplied scale sanitised once at the boundary | ✅ VERIFIED | `sanitize_scale` (`convert.rs:42-47`) called from `convert.rs:158` |
| Degenerate TriMesh (empty / non-finite / out-of-range index) cannot reach Rapier | ✅ VERIFIED | `convert.rs:322` (#1779) and `convert.rs:339-350` (#2878), both at the choke point |
| Degenerate ConvexHull classified before parry sees it | ✅ VERIFIED | `hull_degeneracy` (`convert.rs:404-445`): `Pointlike` / `Collinear` / `Buildable` (#2551 / #3066) |
| `TriMeshFlagBits` pinned against Rapier's own bits | ✅ VERIFIED | `config.rs:35-44`; `trimesh_flag_bits_match_rapier_definitions` (`config.rs:116`) compiles and passes against the current rapier |
| Contact skin applied at **every** collider producer | ✅ VERIFIED | `sync.rs:981` (newcomers), `crates/physics/src/ragdoll.rs:271` (ragdoll bodies, #2861) |
| `kcc_offset_bu > 2 × default_contact_skin_bu` | ✅ VERIFIED (defaults only) | `kcc_offset_clears_the_combined_contact_skin` (`config.rs:163`) asserts `ContactConfig::default()`, not a live override |
| Accumulator clamped **before** the substep loop | ✅ VERIFIED | `world.rs:465-469` |
| Negative / NaN `frame_dt` cannot poison the accumulator | ✅ VERIFIED | `frame_dt.max(0.0)` (`world.rs:465`), rationale `world.rs:456-464`, pinned by *non_finite_frame_dt_cannot_poison_the_accumulator* |
| Static-scene fast path gated on `active_dynamic_bodies().is_empty() && !pending_wake` | ✅ VERIFIED | `world.rs:520-527`; the deliberate kinematic exclusion is argued at `world.rs:512-519` |
| Anti-spiral budget timer starts before substep 1, checked after | ✅ VERIFIED | `world.rs:534-535` then `world.rs:571-574` |
| Query pipeline rebuilt at most once per frame, never per substep | ✅ VERIFIED | `None` passed at `world.rs:560`; single rebuild at `world.rs:624-627`; `update_query_pipeline` clears `colliders_dirty` (`world.rs:712-715`) |
| `pending_wake` consumed only after a substep ran | ✅ VERIFIED | `world.rs:588-590` (#2856) |
| Every motion-introducing mutation calls `wake()` | ✅ VERIFIED | `set_linear_velocity` `sync.rs:72`, `set_kinematic_translation` `sync.rs:103`, `push_kinematic` `sync.rs:1106-1108`, `remove_body` `world.rs:263`, `set_motion_type` `world.rs:424`, `build_ragdoll` `ragdoll.rs:352`, buoyancy `water.rs:996-998` |
| Determinism: no wall-clock or map iteration order feeds the solver | ✅ VERIFIED | only the budget `Instant` (`world.rs:535`), which truncates and is documented as such |
| Phase order collect/register → push kin → buoyancy → step → pull dyn | ✅ VERIFIED | `sync.rs:126-166`; the `BYRO_PROFILE` labels match the spans they time |
| Phase-1 read guards released before write guards | ✅ VERIFIED | `collect_newcomers` returns an owned `Vec` before `register_newcomers` takes `resource_mut` (`sync.rs:129-131` → `sync.rs:890`) |
| Newcomer registration exactly-once; storage-miss refuses to reach the solver | ✅ VERIFIED | `handles_q.contains` gate `sync.rs:849`; `#2867` early return `sync.rs:821-830` |
| `pull_dynamic` writes `Transform`, dividing the parent global back out | ✅ VERIFIED | `sync.rs:1183-1204` via `GlobalTransform::local_from_world` (#2866), which inverts parent scale too (`crates/core/src/ecs/components/global_transform.rs:113-128`) |
| `pull_dynamic` no longer closes the `GlobalTransform → Transform` cycle | ✅ **NEWLY VERIFIED** | `#3303`: two sequential passes, `global_q` dropped at `sync.rs:1206` before `transform_q` is acquired at `sync.rs:1213` |
| `PhysicsWorld` absent → whole system early-returns | ✅ VERIFIED | `sync.rs:113-115` |
| Ragdoll teardown releases bodies, colliders **and** joints | ✅ VERIFIED | `remove_ragdoll` (`ragdoll.rs:569-573`) → `remove_body` with `&mut multibody_joints` + `remove_attached_colliders = true` (`world.rs:246-266`); repeat-cycle leak pinned at `ragdoll.rs:1134-1185` (#2884) |
| Re-activation frees the prior ragdoll before building a new one | ✅ VERIFIED | `byroredux/src/ragdoll.rs:374`, `:385` (#2083) |
| Disconnected constraint forest is surfaced, not silently built | ✅ VERIFIED | `crates/physics/src/ragdoll.rs:292-302` (#1539) |
| `ragdoll_extra_angular_damping` default inert, added once per body | ✅ VERIFIED | default `0.0` (`config.rs:99`), pinned by *default_contact_config_matches_previous_inline_values*; applied once at `ragdoll.rs:233-236` |
| Ragdoll writeback inverts the activation seed once; no double Z-up→Y-up | ✅ VERIFIED | `byroredux/src/ragdoll.rs:541-556` uses `seed_scale` (not a fresh `gt.scale`, #1852); the coord conversion lives upstream in NIFAL `coord.rs` and is not repeated |
| Non-finite simulated pose cannot reach the bone palette | ✅ VERIFIED | `byroredux/src/ragdoll.rs:535-537` (#1534) |
| Every solid-world probe excludes sensors and masks actor bones | ✅ VERIFIED | `solid_probe_filter` (`world.rs:108-112`, #3116) used at `world.rs:768` and `:917`; `move_character` carries its own `.exclude_sensors()` at `world.rs:1132` |
| Every cast excludes the caster's own body/collider | ✅ VERIFIED | `world.rs:769-771`, `:811-813`, `:918-920`, `:1133-1137` |
| Walkable-slope threshold derived from a named constant | ✅ VERIFIED | `min_walkable_normal_y = cos(max_slope_climb_deg)` (`byroredux/src/scene.rs:205-207`); `max_slope_climb_deg = 50.0` with cited NavMesh rationale (`crates/physics/src/components.rs:120-122`, `:193`) |
| Walkable test's sign-blindness is a stated policy, not an accident | ✅ VERIFIED | `world.rs:863-870` states it explicitly (inward-wound legacy Havok); filter at `world.rs:887-889` |
| `integrate_vertical`: terminal clamp after accumulation, jump replaces | ✅ VERIFIED | `byroredux/src/systems/character.rs:1097-1112`; pinned by three tests (`:1367`, `:1388`, `:1401`) |
| `horizontal_motion` normalises before the speed multiply | ✅ VERIFIED | `character.rs:881`; pinned by *horizontal_motion_diagonal_does_not_exceed_speed* |
| `swim_vertical_velocity` is dt-correct | ✅ **RESTORED** | `SWIM_DAMPING = 19.71` (`character.rs:973`) applied as `exp(-SWIM_DAMPING · dt)` (`character.rs:985-1008`) — #3125 CLOSED |
| `advance_breath` no-ops on a zero-dt tick | ✅ **RESTORED** | dedicated `dt <= 0.0` arm at `character.rs:1037-1050`, distinct from the `!head_submerged` reset — #3128 CLOSED |
| `submerged_fraction` clamps to [0,1] and survives a zero-height AABB | ✅ VERIFIED | `water.rs:273-279`, pinned by *submerged_fraction_clamps_and_survives_degenerate_aabb* |
| Archimedes lift is +Y in the renderer frame, proportional to displacement | ✅ VERIFIED | `buoyancy_force` `water.rs:168-176` (`gravity_y.abs()`, `Vec3::new(0.0, f, 0.0)`) |
| Current drag is bounded | ✅ VERIFIED | `current_force` is a velocity-matching term (`water.rs:196-223`) — self-limiting, zero at target speed, reversing above it |
| Buoyancy force reset exactly once per body per frame | ✅ VERIFIED | `water.rs:832-834` (#3114) |
| Buoyancy wakes a body only on a real edge, never pinning the fast path | ✅ VERIFIED | surface `!prior_wet` edge `water.rs:853-860`; current-volume `in_current_prev` latch `water.rs:949-967` (#3268) |
| `n_new > 0` escape hatch for a body that streams in already submerged | ✅ VERIFIED | `sync.rs:150` |
| Buoyancy applies force **before** the step | ✅ VERIFIED | `sync.rs:150` precedes `pw.step(dt)` at `sync.rs:156-157` |
| Buoyancy target set covers every dynamic body | ❌ **GAP** | ragdoll bodies excluded by construction — PHYS-D6-2026-08-27b-02 |
| Water-volume membership uses the same reference point as the surface test | ❌ **GAP** | current branch uses body-origin Y — PHYS-D6-2026-08-27b-01 |
| `colliders_near_xz`'s per-call `Vec` is off the per-frame path | ✅ VERIFIED | two production callers, both diagnostic: `spawn_collider_census_report` (`sync.rs:617`) and the `ragdoll` console command (`byroredux/src/ragdoll.rs:869`) |
| Census distinguishes *not authored* / *dropped in translation* / *not walkable* | ✅ VERIFIED | `sync.rs:692-742` — the `SpawnProbeVerdict` arm fires first, then the `SpawnCensusAuthoring` split, then the column tally |
| Census reachable from `byro-dbg` | ✅ VERIFIED | `phys.census` / `phys.stats` in `byroredux/src/commands/physics.rs` (#2876), registry-pinned at `byroredux/src/commands_tests.rs:986` |
| Fast-path cost justification is a current measurement, not a stale quote | ✅ VERIFIED | `world.rs:472-503` re-attributes the budget to the post-loop `QueryPipeline::update` (≈2.1 ms of a 2.1–2.4 ms step over 30 k cuboids) and explicitly retires the old "8–10 ms × 5 substeps" figure (#2890) |
| PHYSAL: constraint CInfo decode is the only per-game seam | ✅ VERIFIED | zero game/version symbols in the solver path |
| Cell unload releases every registered handle | ⚠️ **QUALIFIED** | `release_victim_rapier_bodies` (`byroredux/src/cell_loader/unload.rs:510-544`) releases everything the victim list names — but the list itself is not an exact set (**#3379**, still open) |

---

## Findings

### MEDIUM

#### PHYS-D6-2026-08-27b-01: the current-volume containment test measures the body **origin** Y while the surface test measures the collider **AABB centre** — the exact split `#2887` closed, twenty-six lines apart in the same loop

- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:735-748` (the current-volume
  containment test) · `crates/physics/src/water.rs:761-774` (the surface test's
  `center_y`, carrying the `#2887` rationale in-comment) ·
  `crates/physics/src/water.rs:725-733` (`pos`, the body origin both read) ·
  `crates/physics/src/water.rs:839` (the surface branch re-deriving `center_y`)
- **Status**: NEW (sibling site of `#2887`, which is CLOSED and correctly fixed
  on the surface branch only)
- **Trigger Conditions**: an authored `XWCU` / `WaterCurrentVolume` marker whose
  vertical band (`volume.min[1] .. volume.max[1]`) does not comfortably contain
  the whole body, plus a dynamic body whose collider is offset in Y from its
  rigid-body origin. The second half is not exotic — it is the norm for every
  `bhk` compound, because `collision_shape_to_parts` attaches each part at its
  own local isometry (`convert.rs:191-214`) and nothing re-centres the body on
  the shape. Rivers and waterfalls are the authored shape most likely to have a
  tight vertical band.
- **Description**: inside `apply_buoyancy_with_scratch`'s per-body loop, `pos`
  is `*body.translation()` — the rigid-body **origin**
  (`water.rs:725-733`). The current-volume branch tests all three axes against
  it verbatim:

  ```rust
  let current_flow = if pos.x < ux0 || pos.x > ux1 || pos.z < uz0 || pos.z > uz1 {
      None
  } else {
      current_volumes
          .iter()
          .find(|current| {
              let v = &current.volume;
              pos.x >= v.min[0] && pos.x <= v.max[0]
                  && pos.y >= v.min[1] && pos.y <= v.max[1]   // ← body ORIGIN
                  && pos.z >= v.min[2] && pos.z <= v.max[2]
          })
          .map(|current| current.flow)
  };
  ```

  The surface test immediately below deliberately does **not** do this. It
  computes the collider AABB and uses its centre, and says why in a comment
  that applies word for word to the block above it
  (`water.rs:764-774`):

  ```rust
  let aabb = collider.compute_aabb();
  let (min_y, max_y) = (aabb.mins.y, aabb.maxs.y);
  // #2887 — the collider AABB centre, NOT `pos.y` (the rigid
  // body's ORIGIN). They coincide only for a shape centred on
  // its body, which is exactly what this module's test balls
  // are and exactly what the bhk import path is not:
  // `collision_shape_to_parts` attaches every compound part at
  // its own local isometry, and ragdoll bones are offset by
  // construction. ...
  let center_y = 0.5 * (min_y + max_y);
  ```

  XZ is *consistently* origin-based on both branches, which is defensible (the
  union-footprint prune at `water.rs:735` is origin-based too, and horizontal
  offsets are small relative to marker footprints). Y is the axis on which the
  two disagree, and Y is the axis a current marker's band is tight on.
- **Evidence**: `#2887` was filed by `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
  as PHYS-D6-04 — *"`WaterContact::depth` is measured from the body origin, not
  the collider AABB centre its doc promises … the norm for compound bhk shapes
  and ragdoll bones"* — and its fix is pinned by
  *depth_is_measured_from_the_collider_aabb_centre_not_the_body_origin*
  (`water.rs:1592`), whose fixture is *"a compound whose leaf hangs 40 BU below
  the body origin"*. Run that same fixture through the current-volume branch and
  it fails the same way: a 40 BU error against a river band is the difference
  between in and out.
- **Impact**: a body whose collider sits below its origin leaves an authored
  current *early* (the origin exits the band while the body is still in the
  water) and is picked up *late* on entry. Because this branch is also the sole
  feeder of the `#3268` `in_current_prev` latch
  (`in_current_now.push(t.entity)`, `water.rs:949`), a body oscillating across
  the band boundary alternately enters and leaves the latch and is re-woken on
  every re-entry — the one path in this module that can defeat the latch's
  wake-once intent. No crash and no force leak (the `#3114` reset at
  `water.rs:832-834` is gated on `current_flow.is_some() || surface.is_some() ||
  t.prior_wet`, so it still runs). The visible symptom is *"debris in this river
  only sometimes moves"*. Blast radius: every title with authored `XWCU`
  markers (FO3/FNV are confirmed producers).
- **Related**: `#2887` (CLOSED — the same defect on the surface branch);
  `#3268` (CLOSED — the latch this feeds); `docs/engine/watal.md` §7 Phase 2.
- **Suggested Fix**: hoist the collider-AABB fetch above the `current_flow`
  computation and use `center_y` for the current volume's Y test, exactly as
  the surface test does. The AABB is already computed a few lines later for
  every body that reaches the surface test, so on the common path (marker
  co-located with its plane) this is free; only the current-only path pays a new
  `compute_aabb`. Extend the existing offset-compound fixture with a
  current-volume assertion so one fixture pins both branches.

#### PHYS-D6-2026-08-27b-02: ragdoll bodies are structurally invisible to the WATAL buoyancy sink — `activate_ragdoll` strips the two components the scan selects on

- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy (with a Ragdoll Articulation seam)
- **Location**: `crates/physics/src/water.rs:684-701` (the target scan) ·
  `byroredux/src/ragdoll.rs:414-441` (the `#1772` teardown that removes both
  selectors, `RigidBodyData` at `:429-433`) ·
  `crates/physics/src/ragdoll.rs:355-363` (`Ragdoll` is the only place the built
  bodies' handles land) · the doc claims at `docs/engine/physics.md:136-139`
  and `crates/physics/src/water.rs:262-265`
- **Status**: NEW
- **Trigger Conditions**: any actor killed, or ragdolled via the `ragdoll`
  console command, in or above water. Universal across titles — nothing about
  it is content- or game-specific.
- **Description**: `apply_buoyancy_with_scratch` builds its target set by
  iterating `RapierHandles` and keeping rows whose `RigidBodyData` says
  `Dynamic` (`water.rs:684-701`):

  ```rust
  for (entity, handles) in handles_q.iter() {
      let Some(bd) = body_q.get(entity) else { continue; };
      if bd.motion_type != MotionType::Dynamic { continue; }
      ...
      targets.push(BuoyancyTarget { entity, handles: *handles, ... });
  }
  ```

  A ragdoll's bodies are created by `build_ragdoll` and their handles are stored
  **only** on the `Ragdoll` component (`crates/physics/src/ragdoll.rs:355-363`);
  no `RapierHandles` row is ever written for them. And the ragdoll bones' *own*
  `RapierHandles` rows — the Keyframed followers, already rejected by the
  `motion_type != Dynamic` test — are deleted outright by `activate_ragdoll`'s
  `#1772` teardown (`byroredux/src/ragdoll.rs:429-441`):

  ```rust
  if let Some(mut rbq) = world.query_mut::<RigidBodyData>() {
      for (bone, _) in &bone_handles { rbq.remove(*bone); }
  }
  if let Some(mut hq) = world.query_mut::<RapierHandles>() {
      for (bone, _) in &bone_handles { hq.remove(*bone); }
  }
  ```

  So both before and after activation no ragdoll body can appear in `targets`,
  and nothing re-adds them: `collect_newcomers` requires
  `CollisionShape + RigidBodyData + GlobalTransform` (`sync.rs:846-862`) and
  `RigidBodyData` is gone. The `#1772` removal is *correct* for its own purpose
  (it stops kinematic followers fighting the multibody); the gap is that the
  buoyancy scan keys off the same two components and has no `Ragdoll` arm.
- **Evidence**: two live docs assert the opposite of the code.
  `docs/engine/physics.md:136-139` — *"Call `water::apply_buoyancy`, which adds
  Archimedes lift and submerged damping to **every dynamic body** inside a
  `WaterVolume`"*. And `water.rs:262-265`, in `submerged_fraction`'s own
  doc-comment — *"for an irregular **ragdoll bone** it slightly over/under-
  estimates near the surface, which only shifts the rest height by a few BU"* —
  reasoning about an accuracy trade-off on a code path ragdoll bones cannot
  reach. `grep -n "Ragdoll\|ragdoll" crates/physics/src/water.rs` returns three
  hits, all comments; there is no `Ragdoll` query anywhere in the module. By
  contrast the release path *does* have a `Ragdoll` arm
  (`byroredux/src/cell_loader/unload.rs:541`, #1531) — the precedent for adding
  one here.
- **Impact**: a corpse that falls into water sinks at full gravity with its
  authored *air* damping — no submerged linear/angular damping, no Archimedes
  lift, no current drag — and emits no `WaterContact`, so every downstream
  consumer of that component (splash/ripple markers, underwater audio, the FX
  transition edge) sees nothing for it either. It comes to rest on the lakebed.
  This is a visible, reproducible divergence from the source engines, where
  corpses float; it is also the most likely way a player encounters a dynamic
  body in water at all, since placed clutter is authored resting on land. Not
  higher than MEDIUM: no corruption, no leak, and no effect on water-free scenes.
- **Related**: `#1772` (the teardown, correct in itself); `#1531` (the ragdoll
  arm on the release path — the precedent); `docs/engine/watal.md`
  "Physics / gameplay" §.
- **Suggested Fix**: give the target scan a second source. After the
  `RapierHandles` pass, iterate `Ragdoll` and push one `BuoyancyTarget` per
  `(bone, handle, _)` triple, taking `authored_lin`/`authored_ang` from the
  `RagdollTemplate` body (the values `build_ragdoll` used) rather than the
  now-absent `RigidBodyData`. The rest of the loop needs no change — it already
  works off `t.handles.body` / `t.handles.collider`. Gate the extra pass on
  `ragdoll_q.is_some()` so non-actor scenes pay nothing. Correct
  `physics.md:136-139` and `water.rs:262-265` in the same edit; whichever way
  the decision goes, one of those two currently lies.

---

### LOW

#### PHYS-D6-2026-08-27b-03: `water.rs` carries a duplicated `#[test]` attribute and the `#3114` test's rationale doc is now attached to the `#3268` test — a live `rustc` warning

- **Severity**: LOW
- **Dimension**: Water / Buoyancy (test hygiene)
- **Location**: `crates/physics/src/water.rs:1889-1912`
- **Status**: NEW
- **Trigger Conditions**: none at runtime — a build-time lint plus a
  documentation mis-attribution.
- **Description**: `bbfd742f` inserted the new `#3268` regression test
  *between* the `#3114` test's doc comment and its `#[test]` attribute. The
  `#3114` paragraph now ends at `water.rs:1897`; the `#[test]` at `:1898`
  binds to the *new* function; the `#3268` doc runs `:1899-1910`; a second
  `#[test]` sits at `:1911`; and
  `fn current_volume_without_a_water_plane_wakes_a_body_resting_in_it` starts at
  `:1912`. Net effect: the new test has two `#[test]` attributes, it is
  documented by the *other* test's rationale, and
  `current_volume_without_a_water_plane_does_not_wind_up_user_force` — the
  regression guard for a HIGH-severity "havok explosion" force wind-up — is left
  with no rationale doc at all.
- **Evidence**:
  ```
  $ cargo check -p byroredux-physics --tests
  warning: duplicated attribute
      --> crates/physics/src/water.rs:1911:5
       |
  1911 |     #[test]
       |     ^^^^^^^
       |
       = note: `#[warn(duplicate_macro_attributes)]` on by default
  warning: `byroredux-physics` (lib test) generated 1 warning
  ```
- **Impact**: both tests still run and pass (153/153). The cost is a permanent
  warning on every `cargo check --tests` — exactly the noise floor that hides
  the *next* warning — plus a rationale doc that explains the wrong test, on a
  pair of tests whose whole value is encoding why the current-volume branch is
  shaped the way it is.
- **Related**: `#3114`, `#3268` (both CLOSED); the "clear the advisory list
  rather than learning to scroll past it" posture in `_audit-common.md`
  § Path-Reference Convention.
- **Suggested Fix**: delete the `#[test]` at `:1898` and move the whole `#3268`
  test (doc + attribute + body) below
  `current_volume_without_a_water_plane_does_not_wind_up_user_force`, restoring
  the `#3114` doc to its own function. The vanished warning is the acceptance
  criterion.

#### PHYS-D7-2026-08-27b-04: `byroredux/src/commands/physics.rs` — the physics console surface — is outside this audit's declared scope and missing from `_audit-common.md`'s Commands row

- **Severity**: LOW
- **Dimension**: Queries, Diagnostics & Cost
- **Location**: `.claude/commands/audit-physics/SKILL.md` § Scope
  ("Engine-side (Dimensions 4 + 6)" list) ·
  `.claude/commands/_audit-common.md` § Project Layout, the `Commands:` row ·
  the un-listed file `byroredux/src/commands/physics.rs` (200 LOC)
- **Status**: NEW
- **Trigger Conditions**: any `/audit-physics` run that follows the skill's
  scope list literally.
- **Description**: `#2876` added `phys.census` and `phys.stats` in a new
  `byroredux/src/commands/physics.rs`, promoting `PhysicsWorld`'s whole query
  surface (`colliders_near_xz`, `static_colliders_aabb`, `cast_capsule_down*`,
  `body_count`, `awake_counts`) to the live console. Dimension 7's checklist
  explicitly asks whether `dump_spawn_collider_census` "is reachable from
  `byro-dbg`" — and the file that makes it reachable is named neither in the
  skill's scope list (which still names only `commands/scene.rs` and
  `commands/water.rs`) nor in `_audit-common.md`'s per-domain `Commands:` row
  (which enumerates `world_info`, `assets`, `view`, `scene`, `actor_value`,
  `condition`, `time`, `water`, `quest`, `env_health`, `shared` — eleven of
  twelve).
- **Evidence**:
  ```
  $ ls byroredux/src/commands/physics.rs
  byroredux/src/commands/physics.rs
  $ grep -n "physics" .claude/commands/_audit-common.md   # the Commands: row
  (no match on that row)
  ```
  The commands are real and registered: `byroredux/src/commands_tests.rs:986`
  asserts `["phys.census", "phys.stats"]` are in the registry and `:1034` pins
  their argument handling.
- **Impact**: audit-scope rot — the class `_audit-common.md`'s Path-Reference
  Convention exists to prevent. A physics-diagnostics audit that follows the
  scope list examines the census *producer* and never its console *consumer*.
  Concretely, this pass only found by going off-list that `phys.census` sweeps
  `CharacterController::HUMAN` rather than the live player's controller
  (`byroredux/src/commands/physics.rs:117-119`) — harmless today only because
  the spawn rungs also use `HUMAN`, and silently wrong the moment they diverge.
- **Related**: `#2876`; the "Un-owned subsystems" coverage table in
  `_audit-common.md`.
- **Suggested Fix**: add `byroredux/src/commands/physics.rs` to the
  `/audit-physics` Dimension 7 entry-point list and to `_audit-common.md`'s
  `Commands:` row, then run `.claude/commands/_audit-validate.sh`.

---

## Prior-Report Reconciliation

Both prior reports were read in full and every finding re-verified at HEAD.

### `docs/audits/AUDIT_PHYSICS_2026-08-24.md` (full 7-dimension pass, 0/0/1/0)

| Finding | State at HEAD | Evidence |
|---|---|---|
| **PHYS-D6-2026-08-24-01** — a dynamic body resting in a `WaterCurrentVolume` with no overlapping `WaterPlane` never wakes | ✅ **CLOSED** (#3268, `bbfd742f`) | The `in_current_prev` / `in_current_now` double-buffered latch on `WaterContactScratch` (`water.rs:116-141`) plus the one-shot wake at `water.rs:949-967`. The scratch is now taken and handed back **whole** (`std::mem::take(&mut *scratch)`, `water.rs:561-563`) precisely so the latch cannot be dropped field-by-field. Every early return meaning "no body can be in a current" clears the latch (`water.rs:599`, `:627`, `:709`); the quiesced fast path deliberately does not (`water.rs:646-651`). The 08-24 report's own suggested fix — remove the test's manual per-iteration `wake_up` and assert the body still accelerates from rest — is implemented as `current_volume_without_a_water_plane_wakes_a_body_resting_in_it` (`water.rs:1912`), with a no-marker control world guarding against "fix it by waking everything". |

The 08-24 report's three carried-forward opens are now **all closed** too:

| Carried-forward | State | Evidence |
|---|---|---|
| **#3125** — `swim_vertical_velocity` frame-rate-dependent damping | ✅ CLOSED (`303329d9`) | `SWIM_DAMPING = 19.71` chosen so `exp(-SWIM_DAMPING/60) == 0.72`, applied as `prev_velocity * (-SWIM_DAMPING * dt).exp()` (`byroredux/src/systems/character.rs:973`, `:1007`) |
| **#3128** — `advance_breath` refills the reserve on a zero-dt tick | ✅ CLOSED (`303329d9`) | dedicated `if dt <= 0.0` arm preserving both breath and the fractional damage remainder (`character.rs:1037-1050`), separate from the `!head_submerged` reset at `:1027-1036` |
| **#3130** — `pull_dynamic` lock-ordering comment 75 lines from the drops | ✅ CLOSED (`303329d9`) | the `#2135` comment now sits immediately above `drop(handles_q); drop(body_q);` (`crates/physics/src/sync.rs:1130-1140`) |

Its housekeeping note is also resolved: **#3122** and **#3238** are both CLOSED
on the tracker, matching the code.

### `docs/audits/AUDIT_PHYSICS_2026-08-27.md` (Dimension 3 only, streaming-deep, 0/1/0/1)

| Finding | State at HEAD | Evidence |
|---|---|---|
| **PHYS-D3-2026-08-27-01** (HIGH, tracker **#3379 OPEN**) — `PersistentCellApplyJob` re-stamps its whole entity range on every yield, duplicating the persistent CELL's unload victim list | ❌ **STILL OPEN**, unchanged | `grep -n "first_entity" byroredux/src/cell_loader/exterior.rs` still shows exactly one assignment for the persistent job (`:967`, the constructor) against three reads (`:258`, `:276`, `:311`), while the sibling `ExteriorCellApplyJob` still takes a fresh per-slice `first_entity` (`:1628`, `:1794`, `:1836`). `stamp_cell_root_range`'s index half is still `entry.extend(first..last)` with no dedup (`byroredux/src/cell_loader/load.rs:230-238`). Re-verified, **not re-filed**. |
| **PHYS-D3-2026-08-27-02** (LOW, tracker **#3380 OPEN**) — `release_victim_rapier_bodies`' duplicate-tolerance is incidental, undocumented and untested | ❌ **STILL OPEN**, unchanged | `byroredux/src/cell_loader/unload.rs:510-544` still collects one entry per *occurrence*; `rapier_release_tests.rs` still has no duplicated-victim case. Re-verified, **not re-filed**. |

The `-27` report's Dimension 3 "found clean" list was independently re-checked
and still holds in full: the `#2867` collect-time gate, the
read-guards-before-write-guards discipline, the phase order, the
`PhysicsWorld`-absent early return, the `#1772` activation teardown, and
`scripted_motion_type_system`'s live-body update. `#3254` (cinematic unload
retention) is likewise still open with the same nil-distinct physics
consequence; not re-filed.

**Net across both reports**: 1 of 3 findings closed
(PHYS-D6-2026-08-24-01), 2 still open and already tracked (#3379, #3380), plus
3 carried-forward opens from 2026-08-20 all closed. This pass adds 4 new
findings, none CRITICAL or HIGH.

---

## Known-Open Register

Restated per the skill's requirement. **This pass re-investigated none of them
and changed nothing about any of them.**

1. **`tes_grounding_zero_mass_dynamic_fix`** — Skyrim architecture ships mass=0
   Dynamic-family Havok bodies, since reclassified Static (19 → 416 colliders,
   #1832, `ae083d69`). The mass=0 angle is **closed** and was not touched. The
   **door-threshold spawn gap** remains open; this pass traced the grounding
   *mechanism* along the way (probe filter, walkability threshold, KCC offset —
   all verified sound, see the matrix) and found nothing new to add to it.
2. **`interior_spawn_point_fix`** — interiors spawn at the first door's own
   placement; vanilla `coc` has no auto spawn-point logic. Untouched.
3. **`fnv_furniture_sit_needs_transition`** — `dynamicidle_*` sit loops carry no
   pelvis/root channel; M42 seat-snap stays behind `BYRO_SANDBOX_SIT` pending
   that milestone. Untouched.

Per the skill's own "never write an instruction to not look" rule (#3199), the
shipped swim/drown core **was** audited rather than assumed absent:
`swimlevel_reached` (`character.rs:976`), `swim_vertical_velocity` (`:985`),
`advance_breath` (`:1020`), `apply_player_drowning_damage` (`:1067`) and
`water_damage_for_contact` (`:1088`) were all read. Both previously-open defects
there (#3125, #3128) are fixed and no new one was found. One design observation
recorded without filing: authored `damage_per_second` water damage is applied
only once `swim.is_some()` (`character.rs:485-490`), so a player wading below
swim level in damaging water takes none — deliberate-looking (it mirrors the
swim-level gate on the whole water branch) rather than defective, and not
contradicted by any doc.

---

## Cross-Audit Dedup

- **Lock ordering / access declarations** → `/audit-concurrency` Dim 5. The
  concurrency pass running alongside this one filed a **HIGH lock-order cycle**
  between `crates/scripting/src/condition.rs:470-509` and
  `crates/core/src/character/regen.rs:176-180`. Neither file is on any physics
  path traced here; **not re-filed**. From the physics side this pass
  independently re-verified the four guard sequences around
  `physics_sync_system` and found all correct at HEAD: `collect_newcomers`'
  Handles → Shape → Body → Global order (`sync.rs:821-846`, matching
  `push_kinematic` per `#313`), the `#3303` two-pass split in `pull_dynamic`
  (`sync.rs:1156-1213`), the `#3266` resolve-after-drop in both
  `dump_awake_fallers` (`sync.rs:387`) and `spawn_collider_census_report`
  (`sync.rs:676`), and the `#3260` `GlobalTransform`-before-
  `CharacterController` split in `camera_follow_system`
  (`byroredux/src/systems/character.rs:540-561`). The 08-24 report's
  CONC-D5-2026-08-24-01 (undeclared `Parent` / `ActorBoneCollider` reads) and
  -03 (`FormIdPool` across storage guards) are both resolved in code.
- **`unsafe`** → none in `crates/physics/`; nothing for `/audit-safety`.
- **Water rendering half** → `/audit-renderer` Dim 15. The shader-side crest
  mirror of `authored_wave_height_with_weather` is still asserted only by prose
  with no cross-check test — unchanged since 2026-08-20 and still owned there.
- **`bhk*` wire parsing / `havok_filter` decode** → `/audit-nif` Dim 5.
  **#3330** (undecoded `bhkHinge` / `bhkPrismatic` / breakable edges fragmenting
  three FNV creature ragdolls) is OPEN and is the parse-side counterpart of the
  `#1539` forest warning verified here at `crates/physics/src/ragdoll.rs:292-302`;
  not re-filed.
- **`CollisionShape` resolution** → `/audit-nifal` Dim 6.
- **`XCLW` / `WATR` canonical decode feeding `WaterPlane`** → `/audit-esm`
  Dim 5. **#3270** (FO4 `WATR.DNAM` offsets 12/16 misread as fog near/far) is
  OPEN there and is the canonical-side seam of the water findings here; not
  re-filed.
- **The GPU-refcount escalation path** of #3379 belongs to `/audit-renderer`
  Dim 1/3 and `/audit-performance` — see the `-27` report, unchanged.
- **`stamp_cell_root_range`** is also the subject of `PERF-D7-2026-08-27-04`
  (`docs/audits/AUDIT_PERFORMANCE_2026-08-27.md`, LOW — batch the `CellRoot`
  inserts). Different half of the same function; a fix for either should land
  aware of the other.

---

## Recommended Fix Order

1. **PHYS-D6-2026-08-27b-01** — smallest and most mechanical: hoist the AABB
   fetch, reuse `center_y`, extend the existing `#2887` fixture. It also removes
   the one path that can defeat the freshly-landed `#3268` latch.
2. **PHYS-D6-2026-08-27b-02** — a bounded second pass over `Ragdoll` in the
   buoyancy target scan, plus the doc correction in `physics.md` and in
   `water.rs`'s own docstring. Worth doing before further WATAL work, since two
   docs currently promise coverage the code does not have.
3. **PHYS-D6-2026-08-27b-03** — one attribute deletion and one block move;
   clears a live `cargo check --tests` warning.
4. **PHYS-D7-2026-08-27b-04** — two skill-file edits plus
   `.claude/commands/_audit-validate.sh`.
5. **#3379** (still open, HIGH, owned by the `-27` report) — outranks all of the
   above on severity, but it is already filed and its fix lives in
   `byroredux/src/cell_loader/exterior.rs`, not in this layer.

---

*Report ready.*

```
/audit-publish docs/audits/AUDIT_PHYSICS_2026-08-27b.md
```

*(domain label: `physics`; add `water` for findings 01/02/03, `test-gap` for 03,
`tech-debt` + `doc-rot` for 04; no `game:*` label applies — every finding's
trigger is content-shape- or engine-wide, not per-title.)*

TALLY: CRITICAL=0 HIGH=0 MEDIUM=2 LOW=2
