# Issue #3492: PHYS-D6-2026-08-27b-02: ragdoll bodies are structurally invisible to the WATAL buoyancy sink

Labels: medium, physics, water, bug
Filed: 2026-08-27 (published 2026-08-28)

---

**Source**: `docs/audits/AUDIT_PHYSICS_2026-08-27b.md` — PHYS-D6-2026-08-27b-02
**Severity**: MEDIUM · **Dimension**: 6 — Water / Buoyancy (with a Ragdoll Articulation seam)

## Location
- `crates/physics/src/water.rs:684-701` — the target scan
- `byroredux/src/ragdoll.rs:414-441` — the `#1772` teardown that removes both selectors (`RigidBodyData` at `:429-433`)
- `crates/physics/src/ragdoll.rs:355-363` — `Ragdoll` is the only place the built bodies' handles land
- The doc claims at `docs/engine/physics.md:136-139` and `crates/physics/src/water.rs:262-265`

## Trigger Conditions
Any actor killed, or ragdolled via the `ragdoll` console command, in or above water. Universal across titles — nothing about it is content- or game-specific.

## Description
`apply_buoyancy_with_scratch` builds its target set by iterating `RapierHandles` and keeping rows whose `RigidBodyData` says `Dynamic` (`water.rs:684-701`):

```rust
for (entity, handles) in handles_q.iter() {
    let Some(bd) = body_q.get(entity) else { continue; };
    if bd.motion_type != MotionType::Dynamic { continue; }
    ...
    targets.push(BuoyancyTarget { entity, handles: *handles, ... });
}
```

A ragdoll's bodies are created by `build_ragdoll` and their handles are stored **only** on the `Ragdoll` component (`crates/physics/src/ragdoll.rs:355-363`); no `RapierHandles` row is ever written for them. And the ragdoll bones' *own* `RapierHandles` rows — the Keyframed followers, already rejected by the `motion_type != Dynamic` test — are deleted outright by `activate_ragdoll`'s `#1772` teardown (`byroredux/src/ragdoll.rs:429-441`):

```rust
if let Some(mut rbq) = world.query_mut::<RigidBodyData>() {
    for (bone, _) in &bone_handles { rbq.remove(*bone); }
}
if let Some(mut hq) = world.query_mut::<RapierHandles>() {
    for (bone, _) in &bone_handles { hq.remove(*bone); }
}
```

So both before and after activation no ragdoll body can appear in `targets`, and nothing re-adds them: `collect_newcomers` requires `CollisionShape + RigidBodyData + GlobalTransform` (`sync.rs:846-862`) and `RigidBodyData` is gone. The `#1772` removal is *correct* for its own purpose (it stops kinematic followers fighting the multibody); the gap is that the buoyancy scan keys off the same two components and has no `Ragdoll` arm.

## Evidence
Two live docs assert the opposite of the code. `docs/engine/physics.md:136-139` — *"Call `water::apply_buoyancy`, which adds Archimedes lift and submerged damping to **every dynamic body** inside a `WaterVolume`"*. And `water.rs:262-265`, in `submerged_fraction`'s own doc-comment — *"for an irregular **ragdoll bone** it slightly over/under-estimates near the surface, which only shifts the rest height by a few BU"* — reasoning about an accuracy trade-off on a code path ragdoll bones cannot reach. `grep -n "Ragdoll\|ragdoll" crates/physics/src/water.rs` returns three hits, all comments; there is no `Ragdoll` query anywhere in the module. By contrast the release path *does* have a `Ragdoll` arm (`byroredux/src/cell_loader/unload.rs:541`, #1531) — the precedent for adding one here.

Re-verified at HEAD during publish: the scan, the teardown, the `Ragdoll`-only handle storage, and both doc claims are all unchanged.

## Impact
A corpse that falls into water sinks at full gravity with its authored *air* damping — no submerged linear/angular damping, no Archimedes lift, no current drag — and emits no `WaterContact`, so every downstream consumer of that component (splash/ripple markers, underwater audio, the FX transition edge) sees nothing for it either. It comes to rest on the lakebed. This is a visible, reproducible divergence from the source engines, where corpses float; it is also the most likely way a player encounters a dynamic body in water at all, since placed clutter is authored resting on land. Not higher than MEDIUM: no corruption, no leak, and no effect on water-free scenes.

## Related
`#1772` (the teardown, correct in itself); `#1531` (the ragdoll arm on the release path — the precedent); `docs/engine/watal.md` "Physics / gameplay" §.

## Suggested Fix
Give the target scan a second source. After the `RapierHandles` pass, iterate `Ragdoll` and push one `BuoyancyTarget` per `(bone, handle, _)` triple, taking `authored_lin`/`authored_ang` from the `RagdollTemplate` body (the values `build_ragdoll` used) rather than the now-absent `RigidBodyData`. The rest of the loop needs no change — it already works off `t.handles.body` / `t.handles.collider`. Gate the extra pass on `ragdoll_q.is_some()` so non-actor scenes pay nothing. Correct `physics.md:136-139` and `water.rs:262-265` in the same edit; whichever way the decision goes, one of those two currently lies.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
