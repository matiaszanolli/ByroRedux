# PHYSAL / Physics Audit — 2026-08-20

**Scope**: `crates/physics/` (world, sync, convert, components, config, ragdoll,
water) + `byroredux/src/ragdoll.rs` + `byroredux/src/systems/character.rs` +
`byroredux/src/systems/water.rs` + `byroredux/src/commands/water.rs`, with the
producer sites in `byroredux/src/cell_loader/spawn.rs` /
`byroredux/src/cell_loader/references/synth_child.rs` and the parse side
`crates/nif/src/import/collision/`.

**Depth**: deep. **Games traced**: the solver path is game-agnostic (doctrine
verified below); the new placed-current path was traced against the FO3/FNV
`REFR.XWCU + XPRM` producer, and the sensor path against the classic-bhk
`havok_filter` layer-15 route shared by Oblivion / FO3 / FNV / Skyrim LE+SE.

**Prior pass**: `docs/audits/AUDIT_PHYSICS_2026-08-16.md`. All four of its
findings were filed and **three are now CLOSED and verified fixed at HEAD**
(#3064 trimesh ghost scale, #3065 ragdoll shape scale, #3066 undersized convex
hull). Its 21 inherited open findings from 2026-08-13 were re-checked; none is
re-filed here.

**Delta audited**: `23068af0..HEAD` over the physics-owned files —
`water.rs` +541 / `sync.rs` +175 / `convert.rs` +111 / `world.rs` +32 /
`components.rs` +9 in the crate, plus `systems/character.rs` +364 (the new
swim/breath controller), `components/water.rs` +233, `commands/water.rs` +145.

Per the suite briefing, `cargo test -p byroredux-physics` was **not** run (target
lock contention); all findings are static and each cites the exact line evidence.

---

## Executive Summary

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| 1 — Shape Translation | 0 | 0 | 0 | 0 |
| 2 — Step Determinism & Budget | 0 | 0 | 0 | 0 |
| 3 — ECS Sync | 0 | 0 | 1 | 1 |
| 4 — Ragdoll Articulation | 0 | 1 | 0 | 0 |
| 5 — Character Controller | 0 | 1 | 1 | 1 |
| 6 — Water / Buoyancy | 0 | 1 | 1 | 1 |
| 7 — Queries & Diagnostics | 0 | 0 | 0 | 0 |
| **Total** | **0** | **3** | **3** | **3** |

(Finding PHYS-D5-2026-08-20-02 spans Dimensions 1/5/7 and is counted once, under
Dimension 5 — the character controller is where it does damage.)

### The headline: #2549 turned a dormant hazard into a live one

The 2026-08-16 report's **Disproved Candidates** section recorded this, verbatim:

> **`move_character` does not exclude sensors.** … a trigger volume would block
> the player. **Inert**: nothing in the engine creates a sensor collider … Not
> filed; worth re-checking the day `TriggerVolume` grows a real Rapier body.

Commit `00fc0f3b` (Fix #2549) added `.sensor(!n.body_data.collidable)` to
`register_newcomers`. **The engine now creates sensor colliders**, so the
precondition that made that candidate inert is gone — and the KCC / ground-probe
filters were not updated with it. A body authored on Havok's non-collidable
layer is still a solid wall to the player, which is the precise outcome #2549
existed to remove. That is PHYS-D5-2026-08-20-02.

### Second: the WATAL delta added an integral force term with no reset

`crates/physics/src/water.rs` changed 17 times this cycle. The new placed-current
branch (`7e65c46c`) was deliberately positioned *after* the water-plane
`reset_forces` so a plane's reset "cannot discard the marker's current" — but on
every path where **no** plane reset runs, nothing clears `user_force` at all, and
Rapier's `user_force` persists across steps until `reset_forces`. That is
PHYS-D6-2026-08-20-01.

### PHYSAL doctrine verdict — HOLDS

`grep -rn "GameKind|bsver|NifVersion|game_kind"` over `crates/physics/src/` and
`byroredux/src/ragdoll.rs` returns **comments only** — `components.rs:156/159/162`
(the Skyrim-sized `HUMAN` preset rationale), `convert.rs:501`, `ragdoll.rs:86/491`
(Oblivion-authored degenerate-axis notes), `sync.rs:359/361/603`, `world.rs:971/1912`.
No game or version discriminator has leaked into the solver path. The constraint
CInfo decode remains the only per-game seam. `docs/engine/physal.md` §3 still
matches the code.

### WATAL spec drift worth recording (not a bug)

`docs/engine/watal.md`'s open-items list — and this audit skill's Dimension 6
instruction to *"confirm absence rather than reporting it"* — say character
swimming/drowning are unbuilt. **They are built as of this delta**
(`byroredux/src/systems/character.rs:216-283`, `:880-1044`: `player_water_state`,
`swimlevel_reached`, `swim_vertical_velocity`, `advance_breath`,
`apply_player_drowning_damage`). Two findings below are *in* that new code. The
skill's Dim 6 checklist line and watal.md's open-item list should be refreshed.

---

## Solver Invariant Matrix

| Invariant | State | Evidence |
|---|---|---|
| Fixed step: accumulator clamped **before** the loop | ✅ VERIFIED | `world.rs:393-398` |
| Negative / NaN `frame_dt` guarded | ✅ VERIFIED | `frame_dt.max(0.0)`, `world.rs:393` |
| Anti-spiral budget starts before substep 1, checked after | ✅ VERIFIED | `world.rs:431-432`, `:464` |
| `pending_wake` consumed only when a substep ran | ✅ VERIFIED | `world.rs:481` (#2856 intact) |
| Query pipeline rebuilt at most once per frame, never per substep | ✅ VERIFIED | `None` to `pipeline.step`; `world.rs:518-521` |
| No-step frames still flush a dirty collider set | ✅ VERIFIED | `world.rs:419-423` (#2864 fix) |
| `remove_body` re-arms the fast path | ✅ VERIFIED | `world.rs:236-243` (#2863 fix) |
| Determinism: no wall-clock or map order into the solver | ✅ VERIFIED | budget `Instant` truncates only |
| Phase order collect → push → buoyancy → step → pull | ✅ VERIFIED | `sync.rs:112-149` |
| Phase 1 read guards released before write guards | ✅ VERIFIED | `sync.rs:931-950`, `:990-1073` (#2404 fix landed this cycle) |
| Newcomer registration idempotent | ✅ VERIFIED | `sync.rs:728` + #2867 collect-time gate |
| Placement scale reaches the collider exactly once | ✅ **RESTORED** | #3064 / #3065 both fixed; `spawn.rs:337-340`, `byroredux/src/ragdoll.rs:309-314` |
| Degenerate shape input cannot reach Rapier | ✅ **RESTORED** | #3066 `convert.rs:263-281`; #2862 `convert.rs:172-195` |
| Contact skin applied at every collider producer | ✅ VERIFIED | `sync.rs:851`, `ragdoll.rs:262` |
| Sensors excluded from every cast that must see solids | ❌ **DRIFTED** | PHYS-D5-2026-08-20-02 |
| Buoyancy force is reset each frame before re-application | ❌ **DRIFTED** | PHYS-D6-2026-08-20-01 |
| Buoyancy quiesced fast path reachable in shipping config | ❌ **DRIFTED** | PHYS-D6-2026-08-20-04 |
| Death → ragdoll handoff is single-sinked | ❌ **DRIFTED** | PHYS-D4-2026-08-20-03 |
| Vertical integration is dt-correct | ⚠️ PARTIAL | terrestrial yes; swim no — PHYS-D5-2026-08-20-06 |
| PHYSAL: constraint CInfo decode is the only per-game seam | ✅ VERIFIED | no game/version symbol in the solver path |

---

## Findings

### HIGH

#### PHYS-D6-2026-08-20-01: Placed `WaterCurrentVolume` force is added with no `reset_forces` — `user_force` winds up linearly and eventually launches the body

- **Severity**: HIGH
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:767-782` (the un-reset `add_force`) ·
  `crates/physics/src/water.rs:718-719`, `:743`, `:757` (the three paths that *do*
  reset) · producer `byroredux/src/cell_loader/references/synth_child.rs:13-49`
- **Status**: NEW — introduced by `7e65c46c` (`feat(water): apply placed-reference
  current volumes`), this cycle.
- **Trigger Conditions**: a cell with a `REFR` carrying `XWCU` + `XPRM` (an
  authored water-current marker — FO3/FNV rivers, Skyrim streams), **and** an
  awake dynamic body whose translation is inside that marker's box while it is
  *not* simultaneously resolved to a water surface with `submerged_fraction > 0`.
  That is any body above the waterline inside the marker's vertical extent (the
  box is `position ± XPRM bounds × scale`, so its top routinely clears the
  surface), any body in a marker that does not overlap a `WaterPlane` volume in
  XZ, and any body in the `frac == 0.0 && !prior_wet` band.
- **Description**: Rapier's `RigidBody::add_force` accumulates into `forces.user_force`,
  which **persists across `pipeline.step()`** and is cleared only by `reset_forces`
  (`rapier3d-0.22.0/src/dynamics/rigid_body.rs:961-969` vs `:937-945`;
  `rigid_body_components.rs:796` recomputes `force = user_force + gravity·m` each
  step from the persisted value). `apply_buoyancy` respects that in the
  water-plane branch — every application is preceded by `b.reset_forces(false)`
  (`water.rs:718`, and the two dry-restore paths at `:743` / `:757`). The
  current-volume branch appended after it deliberately does **not** reset:

  ```rust
  // water.rs:764-767 — the comment that created the leak
  // Placed XWCU markers are current volumes, not water surfaces.
  // Apply their bounded drag after the surface branch so a
  // water-plane force reset cannot discard the marker's current.
  if let Some(flow) = current_flow {
      if let Some(b) = pw.bodies.get_mut(t.handles.body) {
          if !b.is_sleeping() {
              let f = current_force(flow, …, /* fraction = */ 1.0, consts.current_drag);
              b.add_force(vector![f.x, f.y, f.z], false);   // no reset on this path
  ```

  The reasoning is correct for the *co-located* case (a body floating in a river:
  the plane branch resets, then the marker adds on top). It is wrong for every
  case where the plane branch never runs — `surface == None && !prior_wet`, and
  `frac == 0.0 && !prior_wet`. There, `user_force` is never zeroed and the
  per-frame term is added on top of the previous frame's total.
- **Evidence**: the per-frame term is `f_k = d·(s − v_k·d)·m·c` with
  `c = current_drag = 4.0` (`water.rs:73`). For a body held at `v ≈ 0` by ground
  contact and friction, `f_k` is a **constant** `m·c·s`, so the accumulated
  `user_force` after `n` frames is `n·m·c·s` — unbounded linear growth, not the
  bounded first-order response `current_force`'s own doc comment promises
  (`water.rs:118-128`). Once the accumulated force exceeds static friction the
  body is ejected. Even airborne, the closed loop becomes
  `v̈ = −c·v + c·s` integrated twice (an integral controller where a proportional
  one was intended), so the body overshoots the authored current speed and
  oscillates instead of converging. The prior audit explicitly cleared
  `current_force` as convergent — that verdict was correct for the call site that
  existed then (`water.rs:718-719`, reset-then-add) and does not carry to this one.
- **Impact**: dynamic clutter near an authored water-current marker accelerates
  without bound while awake. The observable is Bethesda's classic "havok
  explosion" — a barrel on a riverbank creeping, then launching. It also pins the
  static-scene fast path indefinitely, because a body under a growing force never
  sleeps, which is the exterior-freeze regression `watal.md` §0 exists to prevent.
- **Related**: #2889 (the documented `add_force`/`reset_forces` path had no
  production caller — this is now its second one, and the first without a reset),
  PHYS-D6-2026-08-20-04 (same function), `watal.md` §7 Phase 2.
- **Suggested Fix**: hoist a single unconditional `b.reset_forces(false)` to the
  top of the per-target body work (before the surface branch), and delete the
  three per-branch resets — one owner of the force clear per body per frame, which
  is also what makes the "apply the marker after the plane" ordering safe by
  construction. Pin it with a test that puts an awake dynamic body inside a
  `WaterCurrentVolume` with **no** overlapping `WaterPlane`, steps ~120 frames,
  and asserts `body.user_force()` is bounded (it must not grow monotonically).

#### PHYS-D5-2026-08-20-02: #2549's sensor colliders are invisible to every filter except `cast_ray` — the non-collidable Havok layer still walls off the player

- **Severity**: HIGH
- **Dimension**: Character Controller (with Shape Translation + Queries &
  Diagnostics consequences)
- **Location**: `crates/physics/src/sync.rs:851-861` (the producer) ·
  `crates/physics/src/world.rs:1013-1017` (`move_character`) ·
  `crates/physics/src/world.rs:666` (`cast_ray_down`) ·
  `crates/physics/src/world.rs:815` (`cast_capsule_down_surface_and_normal`, the
  shared body of `cast_capsule_down` / `cast_capsule_down_onto_walkable_surface`) ·
  `crates/physics/src/world.rs:846-872` (`static_colliders_aabb`)
- **Status**: NEW — but explicitly *pre-registered* as a conditional hazard in
  `docs/audits/AUDIT_PHYSICS_2026-08-16.md` § Disproved Candidates ("worth
  re-checking the day … a real Rapier body" exists). Commit `00fc0f3b` (Fix #2549)
  supplied that day. Not a regression of #2549 — #2549's parse-side and
  registration-side halves are both correct.
- **Trigger Conditions**: any REFR whose NIF authors a `bhkRigidBody` on Havok
  layer 15 (`OL_NONCOLLIDABLE` / `FOL_NONCOLLIDABLE` / `SKYL_NONCOLLIDABLE` —
  the same numeric value across Oblivion / FO3 / FNV / Skyrim LE+SE, per
  `crates/nif/src/import/collision/mod.rs:240-248`), positioned where the player
  walks or where a spawn probe lands.
- **Description**: `register_newcomers` now builds those colliders as Rapier
  sensors (`.sensor(!n.body_data.collidable)`), on the stated grounds that a
  sensor is "present in the solver, no contact response … and (per
  `gameplay_ray_ignores_trigger_sensors` in world.rs) already excluded from ray
  queries elsewhere in this crate". The second half of that claim covers exactly
  one of the five query entry points:

  | Entry point | Filter at HEAD | Sees sensors? |
  |---|---|---|
  | `cast_ray` (gameplay/combat ray) | `QueryFilter::default().exclude_sensors()` | no ✅ |
  | `move_character` (the KCC) | `QueryFilter::default()` (+ optional `exclude_collider`) | **yes** ❌ |
  | `cast_ray_down` (spawn ground probe) | `QueryFilter::exclude_dynamic().groups(ground_probe_groups())` | **yes** ❌ |
  | `cast_capsule_down*` (walkable probe) | same as above | **yes** ❌ |
  | `static_colliders_aabb` (world-health census) | iterates all `Fixed`-parented colliders | **yes** ❌ |

  Rapier 0.22's `KinematicCharacterController` does not add the flag for you: the
  only mutation it makes to the caller's filter is
  `filter.flags |= QueryFilterFlags::EXCLUDE_DYNAMIC`
  (`rapier3d-0.22.0/src/control/character_controller.rs:670`), and the sweep at
  `:264-277` passes that same filter straight into `queries.cast_shape`. Nothing
  in the file references `EXCLUDE_SENSORS` or `is_sensor`.
  `ground_probe_groups()` (`world.rs:87-90`) is an *interaction-group* mask, not a
  sensor filter — it only masks out `ACTOR_BONE_GROUP`.
- **Evidence**:
  ```rust
  // world.rs:1013-1017 — the KCC filter
  let filter = if let Some(exclude) = params.exclude_collider {
      QueryFilter::default().exclude_collider(exclude)
  } else {
      QueryFilter::default()
  };
  ```
  vs the sibling that got it right, `world.rs:708`:
  `let mut filter = QueryFilter::default().exclude_sensors();`
  The two new tests that landed with #2549 (`noncollidable_body_registers_as_a_sensor`,
  `collidable_body_does_not_register_as_a_sensor`, `sync.rs:1121`/`:1157`) assert
  only `collider.is_sensor()` — neither drives a cast or the controller past one,
  so the whole consumer half of the change is untested.
- **Impact**: three distinct user-visible failures from one gap.
  (1) **Invisible walls** — the player is blocked by geometry the author marked
  explicitly non-collidable, which is the exact bug #2549 was filed to fix; before
  the fix the body was a *solid* collider, after it, it is a sensor that the KCC
  still treats as solid, so for the character controller the change is a no-op.
  (2) **False floors** — `cast_ray_down` / `cast_capsule_down_onto_walkable_surface`
  ground the player on a non-solid marker; the player then falls through it on the
  first step. This is a new member of the door-threshold spawn-gap family and can
  be mistaken for it.
  (3) **False health signal** — `static_colliders_aabb` counts sensors toward the
  "collision world is populated" census, the opposite of the discrimination
  #2874 built into `NearbyCollider::is_sensor` ("a sensor sitting where the floor
  should be is not a floor", `world.rs:108-110`).
- **Related**: #2549 (CLOSED, correct as far as it goes), #2874, #2876,
  `AUDIT_PHYSICS_2026-08-16.md` § Disproved Candidates.
- **Suggested Fix**: add `.exclude_sensors()` to the `move_character` filter and
  to the shared `QueryFilter::exclude_dynamic().groups(ground_probe_groups())`
  construction used by `cast_ray_down` and `cast_capsule_down_surface_and_normal`
  (best done by factoring one `fn solid_probe_filter()` so a fourth cast cannot
  drift again), and skip `c.is_sensor()` colliders in `static_colliders_aabb`'s
  count/bounds. Extend the two #2549 tests to walk a capsule *through* the sensor
  and to probe *for a floor* beneath one.

#### PHYS-D4-2026-08-20-03: The two water death sites insert `Dead` without `reconcile_dead_actor` — a drowned actor keeps its AI, keeps its `AnimationPlayer`, and never ragdolls

- **Severity**: HIGH
- **Dimension**: Ragdoll Articulation
- **Location**: `byroredux/src/systems/water.rs:60-68` (`water_damage_system`) ·
  `byroredux/src/systems/character.rs:1027-1044` (`apply_player_drowning_damage`) ·
  contract owner `byroredux/src/combat.rs:376-389` (`reconcile_dead_actor`)
- **Status**: NEW. **Co-surfaced by sibling audits in this suite** — filed here
  once, from the physics side, for the ragdoll-handoff contract. Downstream of
  #3022 (which created the single reconciler) and adjacent to #3030 (CLOSED —
  which gated AI *re-installation* on `Dead`, not the already-installed behavior).
- **Trigger Conditions**: an actor killed by water rather than by combat — FO3/FNV
  authored harmful water (`WaterPlane::damage_per_second`, the `06f84f0d` /
  `93851ecd` path) on an NPC with an active `WaterContact`, or the player's breath
  reserve reaching zero (`advance_breath` → `DROWNING_DAMAGE_PER_SECOND = 12.0`).
- **Description**: `reconcile_dead_actor` is documented as the single reconciler
  that rebuilds "the runtime consequences of the persisted `Dead` fact", and both
  existing death transitions route through it — `apply_hit_damage`
  (`combat.rs:242`) and the save-load drain (`save_io.rs:1014` via
  `reconcile_dead_actor_runtime_state`). The two water death sites added this
  cycle insert the marker directly and stop:
  ```rust
  // systems/water.rs:64-67
  if killed {
      if let Some(mut dead_q) = world.query_mut::<Dead>() { dead_q.insert(entity, Dead); }
  }
  // systems/character.rs:1039-1043 — identical shape
  ```
  Three derived state changes are therefore skipped: `clear_ambient_behavior`
  (which removes 16 behavior/state components — `ai_package.rs:416-436`),
  `remove_component::<AnimationPlayer>` on the skeleton root, and
  `activate_ragdoll`.
- **Evidence**: the AI gate added by #3030 is on the *evaluation* path only —
  `ai_package.rs:472` and `:543` skip re-selecting a package for a `Dead` actor,
  but nothing removes an already-installed `WanderBehavior` / `TravelBehavior` /
  `SandboxBehavior`, and their driver systems do not consult `Dead`. From the
  physics side the decisive one is `activate_ragdoll`: it is the only path that
  frees the actor's keyframed per-bone Rapier bodies and replaces them with the
  dynamic articulated rig (`byroredux/src/ragdoll.rs:392-427`, the #1772
  discipline). Skipping it leaves the corpse's bones as `Keyframed` bodies that
  `push_kinematic` continues to drive from an `AnimationPlayer` that also was not
  removed.
- **Impact**: a drowned NPC keeps walking its package and playing its idle, with
  its skeleton still kinematically pushed into the solver every frame — a
  walking corpse, not a body. The player case is softer (the controller now
  early-returns on `Dead`, `character.rs:157-159`) but equally inconsistent: no
  death ragdoll, and any `HavokAnimationTarget`-driven animation keeps running.
  Because `Dead` is persisted and `reconcile_dead_actor_runtime_state` runs on
  load, a save taken after a drowning and then reloaded produces a *different*
  world state from the live one — the reload finally ragdolls the actor.
- **Related**: #3022, #3030 (CLOSED), #1772, #2882.
- **Suggested Fix**: widen `reconcile_dead_actor` to `pub(crate)` and call it from
  both water death sites immediately after the `Dead` insert, exactly as
  `combat.rs:242` does. `water_damage_system` is already an exclusive
  `Stage::Late` system so the structural removals are legal there; the character
  path needs the call hoisted out of `character_controller_system`'s parallel
  window, or routed through a `Stage::Late` sink. Both sites' `Access`
  declarations must then be widened to the union `reconcile_dead_actor` touches.
  A single test — kill an actor by water damage, assert `Ragdoll` is present —
  pins the contract for whichever third death site lands next.

### MEDIUM

#### PHYS-D6-2026-08-20-04: `apply_buoyancy`'s quiesced-scene fast path is unreachable in the shipping binary — its own regression test only passes because the test world has no `TotalTime`

- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:484-487` (`waves_active`) ·
  `crates/physics/src/water.rs:497-511` (the fast path) ·
  `crates/physics/src/water.rs:1296-1382` (the test that pins it)
- **Status**: NEW — the gate was widened with `&& !waves_active` by `a70f80d9` /
  `6b960349` (`follow authored waves in buoyancy contacts`) this cycle.
- **Trigger Conditions**: every frame of every cell that has at least one
  `WaterPlane` — i.e. the entire delta's target workload.
- **Description**: the fast path is gated on
  `awake_counts().0 == 0 && !pending_wake() && !had_newcomers && !waves_active`,
  and `waves_active` is
  `time_secs.is_some() && surfaces.iter().any(|s| s.material.wave_amplitude.abs() > 1.0e-4)`.
  Both terms are effectively constant-true in the shipping binary:
  `TotalTime` is inserted unconditionally at boot (`byroredux/src/boot.rs:375`), and
  `WaterMaterial::wave_amplitude` defaults to **0.05**
  (`crates/core/src/ecs/components/water.rs:347`) with real vanilla WATR authoring
  **0.1** (pinned by `crates/plugin/tests/parse_real_esm.rs:220` and `:1357`) — both
  three orders of magnitude above the `1.0e-4` threshold. There is no code path in
  a water cell that leaves `waves_active` false.
- **Evidence**: the regression test that exists to prove the fast path works,
  `buoyant_body_sleeps_so_static_fast_path_re_engages` (`water.rs:1296`), builds a
  bare `World::new()` and never inserts `TotalTime`. `time_secs` is therefore
  `None`, `waves_active` is `false`, and the test exercises a configuration the
  binary never reaches. It uses `WaterMaterial::default()`, whose amplitude
  (0.05) would flip the gate the moment `TotalTime` were present. This is the same
  "each half pins its own contract in isolation; nothing tests the composed path"
  shape as the two scale defects from 2026-08-16.
- **Impact**: two, of different weight.
  (1) **Cost** — a fully settled water cell now pays the whole per-body scan every
  frame: `collect_water_surfaces` + `collect_water_current_volumes` + a `targets`
  `Vec` built by iterating **every** entity with `RapierHandles` (all static
  colliders included — the Skyrim-architecture census is ~416/cell interior and
  tens of thousands on a radius-12 exterior), with a `RigidBodyData` and
  `WaterContact` lookup each, plus `collider.compute_aabb()` for every body inside
  the union XZ footprint. That is precisely the work the fast path was added to
  avoid, and the exterior-freeze goal `watal.md` §0 states.
  (2) **A now-conditional invariant** — the `apply_buoyancy` docstring still
  asserts "The sim quiesces; buoyancy can't pin it awake." With waves live, the
  re-wake condition at `water.rs:679-684`
  (`b.is_sleeping() && (surface_y − center_y − prior_depth).abs() > 0.1`) re-wakes
  a settled float, and `woke_any` then re-arms `pw.wake()` (`water.rs:794`). At the
  0.05–0.1 amplitudes vanilla authors the per-frame crest delta stays under the
  0.1 BU threshold so the sim still quiesces, but the guarantee is now an
  amplitude-dependent accident rather than a structural property, and any WATR
  authoring ≳0.25 amplitude pins the step loop awake for the whole cell.
- **Related**: #2871 (OPEN — the dry→wet wake swallowed above 60 fps; same wake
  discipline), PHYS-D6-2026-08-20-01, `watal.md` §0.
- **Suggested Fix**: decide which property is wanted and make it structural.
  Either (a) keep the wave-follow and drop the dead gate, replacing it with a
  cheaper reachable one — skip the scan unless `!surfaces.is_empty()` **and** some
  body already carries a `WaterContact` or is awake, so an all-dry settled cell
  still short-circuits; or (b) keep the fast path and make wave-following
  event-driven (recompute `surface_y` only for bodies that already have a
  `WaterContact`, which is a bounded set, instead of re-scanning all bodies).
  Either way, insert `TotalTime` into
  `buoyant_body_sleeps_so_static_fast_path_re_engages` so the test measures the
  shipping configuration, and restate the docstring's pinning claim with its real
  precondition.

#### PHYS-D3-2026-08-20-05: `physics_sync_system`'s access declaration omits `TotalTime`, `WindField` and `WaterCurrentVolume`

- **Severity**: MEDIUM
- **Dimension**: ECS Sync
- **Location**: `byroredux/src/boot.rs:1234-1269` (the declaration) ·
  `crates/physics/src/water.rs:476-483` (`TotalTime` + `WindField` reads) ·
  `crates/physics/src/water.rs:378-384` (`WaterCurrentVolume` read)
- **Status**: NEW — a third instance of the class already filed as #1787 /
  CONC-D4-01 and #2676 / CONC-D3-NEW-02, whose remediation comments sit six lines
  above the gap in this very declaration.
- **Trigger Conditions**: none today. Becomes live the moment any `Stage::Physics`
  system writes `WindField`, `TotalTime` or `WaterCurrentVolume`.
- **Description**: the buoyancy phase's new reads were not added to the system's
  declared access surface. `apply_buoyancy` reads `TotalTime`
  (`world.try_resource::<TotalTime>()`, `water.rs:476`), reads `WindField` twice —
  directly at `water.rs:477-480` and again inside `weather_wave_adjustment`
  (`water.rs:324-327`) — and reads the `WaterCurrentVolume` storage via
  `collect_water_current_volumes`. The declaration lists `WaterPlane`,
  `WaterVolume`, `WaterFlow`, `WaterContact` and `PhysicsWaterConstants`, but none
  of those three. The sibling `submersion_system` declaration eight lines above
  (`boot.rs:1220-1233`) *does* declare `TotalTime` and `WindField`, which is what
  makes the omission legible as an oversight rather than a convention.
- **Evidence**: the only `WindField` writer is `weather_system`
  (`boot.rs:737-738`, `Stage::Early`), so the scheduler's conflict analyzer has no
  counterparty to catch today. That is exactly the situation #2676's own comment
  describes — "No live race today … but this system" — and the reason that finding
  was still filed.
- **Impact**: the `Stage::Physics` parallel batch's `known_conflict_count() == 0`
  invariant is computed from an incomplete declaration. A future weather or
  current system placed in or after `Stage::Physics` races silently instead of
  being rejected at registration.
- **Related**: #1787 (CONC-D4-01), #2676 (CONC-D3-NEW-02), `/audit-concurrency` Dim 4.
- **Suggested Fix**: add
  `.reads_resource::<TotalTime>()`,
  `.reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()`
  and `.reads::<byroredux_core::ecs::components::water::WaterCurrentVolume>()`
  to the `physics_sync_system` declaration, with the same one-line
  "buoyancy phase / declaration completeness" comment the neighbouring entries carry.

#### PHYS-D5-2026-08-20-06: `swim_vertical_velocity`'s damping is per-frame, not per-second — the new swim controller is frame-rate dependent

- **Severity**: MEDIUM
- **Dimension**: Character Controller
- **Location**: `byroredux/src/systems/character.rs:964-984`
- **Status**: NEW — the whole function landed this cycle (`e7cf6373` /
  `c7561d74`).
- **Trigger Conditions**: any frame in which the player is swimming
  (`swimlevel_reached` true), at any refresh rate other than the 60 Hz the tests
  use.
- **Description**: the integrator mixes a dt-scaled spring term with a **dt-free**
  multiplicative decay:
  ```rust
  let target_y = surface_y - half_span * SWIM_HEIGHT_SCALE;
  let spring = (target_y - center_y) * (5.0 + 7.0 * fraction.clamp(0.0, 1.0));
  (prev_velocity * 0.72 + spring * dt).clamp(-120.0, 160.0)
  ```
  `prev_velocity * 0.72` decays by a fixed 28 % **per frame** rather than per unit
  time. Its terrestrial sibling `integrate_vertical`
  (`character.rs:1053-1070`) is dt-correct (`prev + gravity*dt`), so the two halves
  of the same controller disagree on time discretization.
- **Evidence**: the steady state of `v = 0.72·v + spring·dt` is
  `v* = spring·dt / 0.28`. At 60 fps that is `spring · 0.0595`; at 144 fps
  `spring · 0.0248`; at 30 fps `spring · 0.119`. The swimmer's approach speed to
  the waterline therefore varies by ~**4.8×** across the refresh rates the project
  targets, and the standing depth offset needed to hold station scales with it.
  All three pinning tests (`character.rs:1370`, `:1374`, `:1383`) pass
  `dt = 1.0/60.0` exclusively, so none can observe it.
- **Impact**: swimming feels correct only at 60 fps. On a 144 Hz display the
  player rises to the surface roughly 2.4× slower and sinks lower before the
  spring catches them, which interacts with the drowning path — `head_submerged`
  is depth-derived, so a frame-rate-dependent rest depth makes breath drain
  frame-rate dependent too. Bounded to the player; no solver or NPC impact.
- **Related**: PHYS-D5's `integrate_vertical` invariants (#1698 substep budget is
  what makes dt spikes survivable elsewhere), `watal.md` character-swimming item.
- **Suggested Fix**: replace the fixed factor with a dt-correct decay —
  `prev_velocity * (-SWIM_DAMPING * dt).exp()` (or the cheaper
  `prev_velocity / (1.0 + SWIM_DAMPING * dt)`), naming `SWIM_DAMPING` so it reads
  in 1/s and choosing it to reproduce today's 60 fps behaviour
  (`0.72 == e^(−k/60)` → `k ≈ 19.7`). Add a test asserting that two 1/120 s steps
  land within epsilon of one 1/60 s step.

### LOW

#### PHYS-D6-2026-08-20-07: `clear_stale_water_contacts` is now skipped when a current marker outlives the water plane

- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:492-495`
- **Status**: NEW — the guard was widened from `if surfaces.is_empty()` to
  `if surfaces.is_empty() && current_volumes.is_empty()` by `7e65c46c`.
- **Trigger Conditions**: a cell transition that unloads every `WaterPlane` while
  at least one `WaterCurrentVolume` entity remains resident, with a sleeping
  dynamic body still carrying a wet `WaterContact`.
- **Description**: the surfaces-empty branch is the only caller of
  `clear_stale_water_contacts` (the restore-on-unload path added by `808ecfae`
  for #2870's sibling case). With currents still present the function falls
  through to the quiesced fast path (`water.rs:509`), which returns before the
  per-body loop that would otherwise have taken the `surface == None && prior_wet`
  restore branch. The body keeps `linear_damping = angular_damping = 1.5` and its
  latched buoyancy `user_force` instead of its authored values.
- **Evidence**: the restore in the main loop (`water.rs:753-760`) is only reachable
  once the scan runs; with nothing awake and `waves_active` false (no surfaces to
  make it true) the scan never runs.
- **Impact**: a sleeping crate keeps water damping and a body-weight up-force in
  air. Invisible until something wakes it, at which point it drifts upward.
  Narrow: needs a current marker to survive a plane unload.
- **Related**: #2870 (CLOSED), PHYS-D6-2026-08-20-04.
- **Suggested Fix**: call `clear_stale_water_contacts(world)` whenever
  `surfaces.is_empty()`, regardless of `current_volumes`, and keep the early
  `return` only for the both-empty case.

#### PHYS-D5-2026-08-20-08: `advance_breath` refills the breath reserve on a zero-`dt` tick

- **Severity**: LOW
- **Dimension**: Character Controller
- **Location**: `byroredux/src/systems/character.rs:994-1010`
- **Status**: NEW
- **Trigger Conditions**: `character_controller_system` invoked with `dt <= 0.0`
  while the player's head is submerged — a paused/zero-delta frame, or any
  re-entrant call that passes 0.0.
- **Description**: the guard collapses two cases:
  ```rust
  if !head_submerged || dt <= 0.0 {
      return (MAX_BREATH, DrowningDamage { whole: 0.0, remainder: 0.0 });
  }
  ```
  `!head_submerged` correctly means "surfaced, refill". `dt <= 0.0` means "no time
  passed" and should be a no-op — but it returns `MAX_BREATH` and discards the
  accumulated fractional damage, resetting a drowning player to a full 15-second
  reserve.
- **Impact**: a drowning player survives indefinitely across any sequence that
  interleaves a zero-dt tick. Reachability in the shipping loop is not
  established (the scheduler passes the real frame delta), so this is a
  correctness/hardening issue rather than an observed bug.
- **Related**: PHYS-D5-2026-08-20-06 (same new controller).
- **Suggested Fix**: split the branches — return `(MAX_BREATH, zero)` for
  `!head_submerged`, and `(previous_breath, DrowningDamage { whole: 0.0,
  remainder: previous_damage_remainder })` for `dt <= 0.0`.

#### PHYS-D3-2026-08-20-09: `pull_dynamic`'s lock-ordering comment describes drops that no longer happen there

- **Severity**: LOW
- **Dimension**: ECS Sync
- **Location**: `crates/physics/src/sync.rs:1075-1080`
- **Status**: NEW — doc rot introduced by `6c8f1058` (`Separate physics storage
  and resource guards`, the #2404 fix).
- **Description**: the comment reads *"Drop the `RapierHandles`/`RigidBodyData`
  read guards before taking the `Transform` write lock below"*, but both guards
  are now dropped ~85 lines earlier (`sync.rs:1002-1003`) as part of the #2404
  restructure, and the statements it annotates are `if updates.is_empty()` /
  `query_mut::<Transform>()`. The lock-ordering *rationale* it records (the ABBA
  edge against `character_controller_system`, #2135) is still true and still
  worth keeping — only its placement and the drops it claims are stale.
- **Impact**: none at runtime. It is the load-bearing comment of the function's
  lock discipline, so a reader checking the #2135 invariant against it finds it
  describing code that is not there.
- **Related**: #2404, #2135.
- **Suggested Fix**: move the comment up to the actual `drop(handles_q);
  drop(body_q);` site and reword the first clause.

---

## Disproved Candidates

Recorded so the next pass does not re-derive them.

- **`build_ragdoll` inserts colliders without calling `mark_colliders_dirty`**
  (`crates/physics/src/ragdoll.rs:273`), which since #2864 is how a collider-set
  mutation reaches the query pipeline. **Inert**: `build_ragdoll` calls
  `pw.wake()` at `:352`, so the next `step` always takes the substep path and its
  post-loop `query_pipeline.update` (`world.rs:518-521`) covers the insert.
  Worth a `mark_colliders_dirty()` for symmetry, but not a defect.
- **`register_newcomers` no longer refreshes the query pipeline, so a cast in the
  same frame sees a stale BVH.** It does not: `mark_colliders_dirty`
  (`sync.rs:885`) is honoured by both the stepping path and the no-step fast path
  (`world.rs:419-423`), and Phase 3 runs immediately after Phase 1 inside the same
  `physics_sync_system` call, before any consumer of the pipeline.
- **`pull_dynamic`'s new sleeping-skip compares a world-space pose against a local
  `Transform`.** It does not — the `(translation, rotation)` binding is shadowed by
  the parent-divide `match` (`sync.rs:1052-1058`) before the comparison
  (`sync.rs:1062-1070`), and the quaternion test uses `dot().abs()` so double
  cover is handled.
- **The buoyancy `reset_forces(false)` wipes forces applied by another system.**
  No other production caller of `PhysicsWorld::add_force` / `apply_impulse` exists
  — that is #2889, still open, and it is what makes the reset safe today.
- **`submerged_fraction` divides by zero on a flat AABB.** Guarded by
  `.max(1e-6)` (`water.rs:211`), and the clamp turns a zero-height body into a
  clean fully-dry/fully-wet step.
- **`current_force` / `wind_force` are unbounded at high flow.** The *functions*
  are first-order and bounded, with every non-finite and negative input zeroed
  (`water.rs:130-155`, `:164-184`); `wind_force` additionally one-sides the gust
  to match the renderer and SpeedTree contract. The unboundedness in
  PHYS-D6-2026-08-20-01 is in the *application*, not the math.
- **A body inside a `WaterCurrentVolume` can never be started from rest, because
  the current branch requires `!b.is_sleeping()`.** True in isolation, but not a
  separate finding: in the co-located case (the overwhelmingly common one — XWCU
  markers are placed in rivers) the dry→wet transition at `water.rs:679-684` wakes
  the body, and in the non-co-located case PHYS-D6-2026-08-20-01's force wind-up
  is the dominant, opposite-signed defect. Fix that one first and re-evaluate.

---

## Known-Open Register

The three don't-re-litigate items, and what this pass changed about them:

1. **`tes_grounding_zero_mass_dynamic_fix`** — mass=0 Dynamic Skyrim architecture
   reclassified Static (#1832, `ae083d69`, 19 → 416 colliders). **Not
   re-investigated; nothing here touches the mass=0 angle.** The separate,
   still-open door-threshold spawn gap does gain a new confounder this pass:
   PHYS-D5-2026-08-20-02 (3) is a *second* mechanism that produces "the spawn probe
   found a floor that isn't there", so a future investigation of the door-threshold
   gap must first rule the sensor path out.
2. **`interior_spawn_point_fix`** — interiors spawn at the first door's own
   placement; vanilla `coc` has no auto-spawn-point logic. Untouched.
3. **`fnv_furniture_sit_needs_transition`** — sit loops have no pelvis/root
   channel; M42 seat-snap stays behind `BYRO_SANDBOX_SIT`. Untouched.

**Prior-report status.** Of `AUDIT_PHYSICS_2026-08-16.md`'s four findings, three
are CLOSED and verified fixed at HEAD (#3064, #3065, #3066); the fourth
(PHYS-D3-2026-08-16-04, the unreachable `parts.is_empty()` skip at
`sync.rs:787-789`) is **unchanged since 2026-08-16** and is not re-filed. The 21
findings inherited from 2026-08-13 (#2862–#2890) were re-checked: **#2862, #2863,
#2864, #2867, #2870, #2873, #2874 are now CLOSED and their fixes are present**;
#2871, #2876, #2878, #2879, #2880, #2881, #2882, #2883, #2884, #2885, #2886,
#2887, #2888, #2889, #2890 remain open and are not re-reported. #2888 (the two
ends of WATAL disagree on which overlapping plane wins) is **materially narrowed**
by `4c383433`: `apply_buoyancy` now picks the nearest surface by
`nearest_surface_distance` (`water.rs:234-237`, `:661-665`) rather than the first
match, matching the camera path — the issue should be re-verified before any
further work on it.

---

## Cross-Audit Dedup

- Lock ordering / access declarations → `/audit-concurrency` Dim 4-5.
  PHYS-D3-2026-08-20-05 is the physics-side trace of that class; the storage-vs-
  resource guard overlap (#2404) landed this cycle in `push_kinematic` /
  `pull_dynamic` and is verified fixed here, not re-filed.
- `unsafe` → none in `crates/physics/`; nothing for `/audit-safety`.
- Water **rendering** half (the shader-side crest that `authored_wave_height_with_weather`
  mirrors) → `/audit-renderer` Dim 15. The CPU/GPU crest-agreement contract is
  asserted only by prose in `water.rs:243-256`; no test pins the two evaluations
  against each other. Flagged there, not filed here.
- `bhk*` wire parsing and the `havok_filter` decode → `/audit-nif` Dim 5.
  PHYS-D5-2026-08-20-02's *producer* half (`havok_filter_is_collidable`,
  `crates/nif/src/import/collision/mod.rs:246`) is correct; the finding is entirely
  in the consumer.
- `CollisionShape` resolution → `/audit-nifal` Dim 6.
- The `Dead` → AI/animation half of PHYS-D4-2026-08-20-03 overlaps
  `/audit-ecs` and `/audit-character`; the ragdoll-handoff half is owned here.
- The `XCLW` / `WATR` canonical decode feeding `WaterPlane` → `/audit-esm` Dim 5
  (see also the `watr_data_layout_shift` memory item — `wind_speed` is a constant
  90.0 on 88 % of vanilla WATR; this audit did not re-derive that).

---

## Recommended Fix Order

1. **PHYS-D6-2026-08-20-01** — unbounded force accumulation is the only finding
   here that can visibly break a cell; the fix is one hoisted `reset_forces`.
2. **PHYS-D5-2026-08-20-02** — three filters plus one census; restores #2549's
   intended outcome and removes a false-floor mechanism that will otherwise be
   mistaken for the door-threshold spawn gap.
3. **PHYS-D4-2026-08-20-03** — one `pub(crate)` widening and two call sites;
   land with the sibling audits' halves so the contract is closed once.
4. **PHYS-D6-2026-08-20-04** — decide the fast path's fate and fix its test's
   configuration in the same edit; do it *after* #1, whose fix changes the force
   bookkeeping the scan does.
5. **PHYS-D5-2026-08-20-06**, **PHYS-D3-2026-08-20-05** — small, independent.
6. **PHYS-D6-2026-08-20-07**, **PHYS-D5-2026-08-20-08**, **PHYS-D3-2026-08-20-09** —
   fold into whichever of the above touches the same file.

---

*Report ready. Publish with:*

```
/audit-publish docs/audits/AUDIT_PHYSICS_2026-08-20.md
```

*(there is no `physics` domain label — map to `legacy-compat`, or `tech-debt` for
PHYS-D3-2026-08-20-09.)*

TALLY: CRITICAL=0 HIGH=3 MEDIUM=3 LOW=3
