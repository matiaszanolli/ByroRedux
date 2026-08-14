# #OPEN PHYS-D6-03: WaterFlow.speed carries the raw WATR wind_speed with no unit conversion — the same scalar is a shader scroll rate and a BU/s velocity target (SEAM)

Found by `/audit-physics` Dimension 6 (WATAL Physics Sink). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW · **This is a cross-layer SEAM — reported once, here.**
**Location**: `byroredux/src/env_translate.rs:475-488` (`resolve_water_material` flow synthesis), consumed at `crates/physics/src/water.rs:147` and `crates/core/src/ecs/components/water.rs:214-218` (the unit contract)

## Trigger Conditions
Any cell whose XCWT resolves to a WATR whose EDID matches the `rapid`/`waterfall`/`falls`/`river`/`stream` heuristic (`env_translate.rs:462-471`) — i.e. every river/rapids plane, in every game. Calm water carries no `WaterFlow` and is unaffected.

## Description
`WaterFlow::speed` is documented as *"World units per second. Typical: 0.5 (calm river) ... 8.0 (whitewater rapids) ... 25.0 (Tamriel-tall waterfall sheet)"* (`components/water.rs:216-218`). The single translate site assigns it `rec.params.wind_speed.abs().max(0.5)` — the WATR `DATA`/`DNAM` wind-velocity float, copied verbatim with **no scale factor and no documented unit** at the parse boundary (`crates/plugin/src/esm/records/misc/water.rs:96` says only "wind speed", default `1.0`).

The **same** scalar is then also used as a shader scroll rate in the very next lines:

```rust
// env_translate.rs:477-488
let speed = rec.params.wind_speed.abs().max(0.5);
flow = Some(WaterFlow { direction: [cos_theta, 0.0, sin_theta], speed });
mat.scroll_a = [cos_theta * speed * 0.5, sin_theta * speed * 0.5];   // vs default [0.020, 0.011]
```

against a `scroll_a` default of `[0.020, 0.011]` documented as *"world-space scroll vectors ... (xy = m/s)"*. Physics consumes it directly as a velocity target: `speed_error = target_speed - body_velocity.dot(direction)` (`water.rs:148`).

A value that is simultaneously a ~0.02-magnitude UV scroll rate and a 0.5-25 BU/s world velocity **cannot be dimensionally correct in both consumers**. Nothing in the repo establishes which reading is right, and there is no clamp or sanity band on either.

## Impact
The physics current's terminal drift speed is set by a field of unverified unit.
- If WATR wind velocity is the small normalised float the `scroll_a` defaults imply, the `.max(0.5)` floor pins every real river/rapids plane at ~0.5 BU/s ~ 7 mm/s — authored currents are effectively **inert** in the physics sink, and the "downstream drift" behaviour only exists in the hand-authored `speed: 8.0` test (`water.rs:726-729`).
- If it is a large float on some records, an unclamped `speed` is the **unbounded** terminal velocity clutter converges to.

**Vanilla WATR values were deliberately NOT verified on disk** — that is the disproof step this finding explicitly leaves open, per the no-guessing rule. What is proven from code alone is the two-consumers-one-scalar inconsistency and the total absence of unit documentation, conversion, or clamp.

## Suggested Fix
Establish the WATR wind-velocity unit from the Gamebryo 2.3 / nif.xml / UESP reference **first** (No-Guessing), then either:
- (a) apply an explicit BU/s conversion at the single `resolve_water_material` site and derive `scroll_a` from the canonical `WaterFlow` rather than the raw field, or
- (b) if the field genuinely is a scroll rate, stop feeding it to `WaterFlow.speed` and synthesize the physics current from a documented engine constant x `WaterKind`.

Clamp the result to the documented 0.5-25 BU/s band either way.

## Seam owners
- Decode side: `/audit-esm` Dim 5 (WATR `DATA`/`DNAM` field semantics)
- `scroll_a`/`scroll_b` consumer: `/audit-renderer` Dim 15 (cf. the already-open #2787 on the neighbouring `ampScale`/`freqScale` sentinels)

`docs/engine/watal.md` §4 lists `WaterFlow` as "SYNTHESIZED from wind" for Oblivion/FO3/FNV and "AUTHORED flow" for Skyrim — the synthesis is currently the only path for all games, since no DNAM linear-velocity decode exists.
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other collider producers, other cast sites, other wake sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, `physics_sync_system` still releases read guards before taking write guards
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parse->canonical boundary; no `GameKind`/`bsver` branch is introduced downstream of it (PHYSAL doctrine, `docs/engine/physal.md`)
- [ ] **TESTS**: A regression test pins this specific fix


---
# #OPEN PHYS-D7-02: step_toward's ground-snap ray self-hits the actor's own keyframed ragdoll-bone colliders — walking NPCs elevator upward

Found by `/audit-physics` Dimension 7 (Queries & Diagnostics). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `byroredux/src/systems/locomotion.rs:49-70` (`step_toward`); `crates/physics/src/world.rs:563-588` (`cast_ray_down`); `byroredux/src/npc_spawn.rs:169-198` (`keyframe_live_ragdoll_bones`)

## Trigger Conditions
One of the six env-gated locomotion AI systems is enabled (`BYRO_WANDER` / `BYRO_TRAVEL` / `BYRO_FOLLOW` / `BYRO_ESCORT` / `BYRO_GUARD` / `BYRO_PATROL`), a `PhysicsWorld` resource exists, and the moving actor is a live NPC whose skeleton NIF authored bhk ragdoll bodies (Oblivion / FO3 / FNV / Skyrim — the classic-chain games where the bone bodies actually decode).

Not reachable in the default scheduler, which is the only reason the severity is not higher.

## Description
`step_toward` ground-snaps the actor by casting straight down from `(new_pos.x, current.y + 256, new_pos.z)` and assigning the hit Y to `new_pos.y`. The cast is `PhysicsWorld::cast_ray_down`, whose only filter is `exclude_dynamic`.

`keyframe_live_ragdoll_bones` (#1698) deliberately flips every live actor's ragdoll bone from `Dynamic` to `Keyframed` *before* first registration, so each bone registers as a `KinematicPositionBased` Rapier body with a real collider — its own doc puts this at *"~18 bones/NPC"* and *"~480+ such bodies across a dense interior"*.

`exclude_dynamic` does **not** filter kinematic bodies, and the ray origin is 256 BU directly above the actor's root, so the first thing the ray meets on its way down is the actor's **own** upper-body bone collider, not the floor.

There is no way for the caller to fix this locally: `cast_ray_down` accepts no exclusion, and `step_toward` receives only `Option<&PhysicsWorld>` — it is handed neither the actor's `EntityId` nor its `RapierHandles`.

## Evidence
Empirically confirmed with a throwaway test against the real `PhysicsWorld` (reverted after running):

```
// Fixed floor cuboid, top surface y=1.0, plus a KinematicPositionBased ball(6) at y=120
// (stand-in for a keyframed head/spine bone), ray from y=256 as locomotion.rs does:
locomotion ground ray -> Some(126.0)     // its own bone, not the floor at 1.0
```

`locomotion.rs:64-66` then does `new_pos.y = ground_y`, writing the actor's *root* to what is really its own bone's top surface.

## Impact
An actor under any of the six locomotion procedures is re-seated each tick at its own bone height rather than the ground. Because the bones are children driven from the actor's animated `GlobalTransform` (via `push_kinematic`), the whole rig rises with the root, so the next tick's ray hits the bone again from an even higher origin — **a monotonic elevator, not a one-off offset**. Symptom is NPCs ascending out of the cell while "walking". Also silently corrupts every `Travel`/`Escort` arrival test that depends on real ground Y.

## Suggested Fix
Thread the moving actor's `RigidBodyHandle` into `step_toward` and pass it to an exclusion-aware `cast_ray_down` (the same signature change PHYS-D7-01 needs — fix them together).

Excluding a single body handle is **not sufficient on its own** — each bone is a *separate* body. The cleanest fix is a `QueryFilter` predicate that rejects colliders whose parent body resolves to an entity under the actor's skeleton root, or (cheaper) an interaction-group / collision-group bit for "actor bone" that ground probes mask out.

## Related
- PHYS-D7-01 (same missing-exclusion root cause, different caller)
- `docs/audits/AUDIT_CONCURRENCY_2026-07-16.md:72,170` previously reviewed `step_toward`'s `cast_ray_down` for lock ordering only, never for filter correctness
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other collider producers, other cast sites, other wake sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, `physics_sync_system` still releases read guards before taking write guards
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parse->canonical boundary; no `GameKind`/`bsver` branch is introduced downstream of it (PHYSAL doctrine, `docs/engine/physal.md`)
- [ ] **TESTS**: A regression test pins this specific fix


---
# #OPEN PHYS-D7-03: The spawn census cannot separate 'no collider authored' from 'dropped in translation', and is blind to 'present but not walkable'

Found by `/audit-physics` Dimension 7 (Queries & Diagnostics). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/sync.rs:375-476` (`dump_spawn_collider_census`, its doc block, and the summary `log::warn!` at `:445-454`); `byroredux/src/scene.rs:1113-1128` (its only call site); `crates/physics/src/world.rs:694-727` (`cast_capsule_down_surface_and_normal`, private)

## Trigger Conditions
A door-teleport spawn whose floor probe misses all three rungs (`floor_probe_failed == true`, `scene.rs:1120`). Every such run produces a census that under-determines the cause.

## Description
The census's own doc block (`sync.rs:380-393`) enumerates four candidate causes and the summary log line (`:445-454`) maps them onto observable tallies. Two of the three cases this diagnostic exists to separate are not actually separable by that mapping:

**(a) no collider authored vs (b) collider dropped in translation** — both land on the same bucket. The log says *"0 total => the collider never spawned (per-NIF trimesh-fallback gate, or a REFR-level gap)"*; that single sentence **is** the conflation. The engine already computes the discriminator elsewhere and the census never consults it: `summarize_collision_authoring` / `CollisionAuthoringSummary` (`crates/nif/src/import/collision/mod.rs`, retained on `CachedNifImport`) carries the classic / new-physics / phantom counts, and `docs/engine/physics.md:330-338` states its whole purpose is that *"an empty decoded-collision array no longer conflates 'intentionally no collision' with 'packed collision exists but is undecodable'"*. The census reads the Rapier side only, so it **re-introduces exactly the conflation that summary was built to remove**.

**(c) collider present but not walkable** — invisible. All three spawn rungs call `cast_capsule_down_onto_walkable_surface`, which returns `None` for *both* "swept capsule hit nothing" and "swept capsule hit something whose `normal1.y` failed the walkable test" (`world.rs:675-692`). The normal is computed and then discarded: the surface-and-normal helper is private and only `cast_capsule_down` (surface only) is public.

So a spawn that is blocked by a 60-degree ramp logs *"MISS on all 3 rungs"* and then a census showing `Fixed>0` — and the summary line instructs the reader to conclude *"Fixed>0 at a wrong Y => transform composition"*. **The diagnostic actively mis-attributes a walkability rejection to a transform bug.**

## Evidence
- `sync.rs:448-451` — the summary text: three arms, no walkability arm, no authoring-summary arm
- `world.rs:689-691` — `.and_then(|(surface_y, normal_y)| (...).then_some(...))`, the `None` return collapsing miss and reject
- `world.rs:694` — `fn cast_capsule_down_surface_and_normal` is private, so no caller can recover the normal
- `sync.rs:343-353` — `SpawnCensusEntry` has no walkable/normal field and no authoring field

## Impact
The one diagnostic built for "why is there no floor here" leaves the operator with two of the three real causes indistinguishable, and steers them toward the wrong one in the third. This is the observability layer under a defect class that has already consumed #1295, #2013 and #2202 — and, per this audit, PHYS-D5-01 / D5-02 / D5-03.

## Suggested Fix
1. Make `cast_capsule_down_surface_and_normal` `pub`, and on the failure path re-run it unfiltered so the log can say *"unfiltered sweep hit y=... normal_y=... -> REJECTED as non-walkable (min=...)"* versus *"no hit"*.
2. Add the cell's `CollisionAuthoringSummary` totals to the census header so `0 total` splits into "nothing authored" vs "N classic / M new-physics authored, none registered".

## Related
- PHYS-D7-04 (same function, different defect), PHYS-D7-05 (unreachable at runtime — worth fixing in the same change)
- #2202 (the issue that created the census)
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other collider producers, other cast sites, other wake sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, `physics_sync_system` still releases read guards before taking write guards
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parse->canonical boundary; no `GameKind`/`bsver` branch is introduced downstream of it (PHYSAL doctrine, `docs/engine/physal.md`)
- [ ] **TESTS**: A regression test pins this specific fix


---
# #OPEN PHYS-D7-04: The spawn census sorts by absolute world Y and truncates at 24 — in a dense column the floor is exactly what gets cut

Found by `/audit-physics` Dimension 7 (Queries & Diagnostics). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/world.rs:761-817` (`colliders_near_xz` + its doc claim at `:777-778`); `crates/physics/src/sync.rs:335-338` (`SPAWN_CENSUS_DETAIL_CAP = 24`) and `:455`, `:470-475`; `byroredux/src/scene.rs:1121` (`SPAWN_CENSUS_RADIUS_BU = 256.0`)

## Trigger Conditions
A failed door spawn in any column containing more than 24 colliders within +/-256 BU in XZ — i.e. essentially every interior with an upper storey, beams or shelving, and every exterior grid cell.

## Description
`colliders_near_xz` takes `(x, z, radius)` and **no Y**. It sorts descending by AABB centre Y and its doc block claims this is *"so the nearest thing above the probe reads first"*. There is no probe Y in scope, so the sort key is not "nearest above the probe" — it is **"highest in the world column"**. `dump_spawn_collider_census` then prints only the first 24.

The consequence inverts the diagnostic's purpose. The question being asked is *"is there a floor at/below the spawn?"*, whose answer lives at the **low** end of the sort. In a Skyrim inn (two storeys, roof beams, rafters, an upper landing) the 24 shown entries are the roof and the upper floor; the actual spawn-height geometry falls under *"... N further colliders omitted"*.

The very cell shape the doc itself calls out — *"2560 fixed colliders and a hole exactly under the player's spawn"* (`world.rs:769-771`) — is the worst case for this ordering.

## Evidence
- `world.rs:779` signature `colliders_near_xz(&self, x: f32, z: f32, radius: f32)` — no Y parameter exists to sort relative to
- `world.rs:811-815` sort comparator uses `a.aabb_min[1] + a.aabb_max[1]` (absolute centre Y)
- `sync.rs:455` `entries.iter().take(SPAWN_CENSUS_DETAIL_CAP)`
- the unit test that pins the ordering (`world.rs:1136-1153 census_sorts_by_aabb_centre_y_descending`) uses three slabs, so it can never observe the truncation interaction

## Impact
In precisely the dense-interior case the census was written for, it prints a wall of irrelevant ceiling geometry and omits the evidence. Degrades a MEDIUM-value diagnostic to a misleading one; costs a debugging session per occurrence.

## Suggested Fix
Pass the probe origin Y through to `colliders_near_xz` and sort by `|centre_y - probe_y|` (nearest to the probe first, which is what the doc already promises), or keep the descending sort but take 12 from each end. Fix the doc sentence either way.

## Related
- PHYS-D7-03 (same function), PHYS-D7-05
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other collider producers, other cast sites, other wake sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, `physics_sync_system` still releases read guards before taking write guards
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parse->canonical boundary; no `GameKind`/`bsver` branch is introduced downstream of it (PHYSAL doctrine, `docs/engine/physal.md`)
- [ ] **TESTS**: A regression test pins this specific fix


---
