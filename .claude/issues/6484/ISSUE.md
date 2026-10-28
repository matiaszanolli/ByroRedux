# PERF-D1-01: `vats_scan` rebuilds the target's collider-body set over every physics entity once per limb part

Labels: low, performance, gameplay, combat, bug
Source: docs/audits/AUDIT_PERFORMANCE_2026-10-27.md

**Audit report**: `docs/audits/AUDIT_PERFORMANCE_2026-10-27.md` -- finding `PERF-D1-01` (LOW)
**Location (re-mapped to current code)**: `byroredux/src/systems/vats.rs` (`visible_fraction`, called per part from `vats_scan`). Premise re-read at HEAD: `visible_fraction` still builds `target_bodies: HashSet<_>` from `world.query::<RapierHandles>()` + a per-entry `world.get::<ActorColliderOwner>` on every call.
**Status at filing**: NEW

## Description
For every part of every in-range actor, `visible_fraction` walks the whole `RapierHandles` storage and calls `world.get::<ActorColliderOwner>` per entry to build a std `HashSet` of the target's own bodies. The physics sync attaches `RapierHandles` to every registered collision body, statics included. The set depends only on the target, but it is rebuilt for each of its BPTD parts (FNV/FO3 limb tables carry ~6-14).

## Evidence
```rust
let target_bodies: HashSet<_> = world.query::<byroredux_physics::RapierHandles>().map(|handles| handles.iter().filter(|(owner, _)| world.get::<byroredux_physics::ActorColliderOwner>(*owner).is_some_and(|owner| owner.0 == target)).map(|(_, handles)| handles.body).collect()).unwrap_or_default();
```
inside the per-part path of `vats_scan`.

## Impact
A one-frame hitch on VATS entry and on each `vats.scan` console call. Derived at 20 actors x 10 parts x 3,000 bodies: ~600 k storage lookups, tens of ms (derived, not measured). VATS freezes the world, so the hitch is cosmetic rather than sustained. No quantitative guard exists for this site.

## Related
ECS-2026-10-27-02 / CONC-01 (lock-order cycle in the same loop and the same `visible_fraction` guards; fix together), #6350 (VATS runtime), #6429.

## Suggested Fix
Before the actor loop, build `owner -> bodies` once per scan from the sparse `ActorColliderOwner` storage (it covers ragdoll bodies only), then pass the target's slice into `visible_fraction`. This also drops the per-part SipHash set.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

