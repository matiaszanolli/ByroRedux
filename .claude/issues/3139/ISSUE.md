# PERF-D1-03: WaterContact grew 4.2x in the delta and is still round-tripped through a freshly-allocated Vec every frame

**Issue**: #3139 — https://github.com/matiaszanolli/ByroRedux/issues/3139
**Labels**: `low,performance,bug`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md`

---

**Severity**: LOW
**Dimension**: CPU Per-Frame Allocations & Hot Paths
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md` (PERF-D1-03)

## Location

- `crates/core/src/ecs/components/water.rs:88-287` (`WaterMaterial`), `:564-590` (`WaterContact`)
- `crates/physics/src/water.rs:581` (the `writes` Vec), `:722-734` / `:745` / `:759` (the pushes), `:802-810` (the drain)

## Status

NEW — quantification of a lead surfaced independently by this suite's ECS audit, verified against HEAD here from the performance angle.

## Description

`WaterMaterial` went from **18 fields / 104 B** at `85b77371` to **63 fields / 433 B raw → 436 B** at HEAD — a **4.2× growth in one session**. It is `Copy` and embedded **by value** in three places on the hot path:

- `WaterPlane.material` (the ECS component)
- `WaterSurface.material` (the per-frame physics snapshot)
- `WaterContact.material` as `Option<WaterMaterial>` — which makes `WaterContact` ≈**480 B**, up from ≈150 B

`apply_buoyancy` collects contacts into `let mut writes: Vec<(EntityId, WaterContact)> = Vec::new();` (`:581`), pushes a full ≈480 B value per wet-or-transitioning body (`:722`, `:745`, `:759`), then drains it into the storage after the `PhysicsWorld` write lock drops (`:802-808`).

**The Vec is freshly allocated every frame and dropped at the end**, so a scene with N wet bodies pays one allocation plus 2×N×480 B of copy traffic per frame (once into the Vec, once into the storage).

## Evidence

Confirmed at HEAD:
```
crates/core/src/ecs/components/water.rs:564:pub struct WaterContact {
crates/core/src/ecs/components/water.rs:589:    pub material: Option<WaterMaterial>,
crates/physics/src/water.rs:581:    let mut writes: Vec<(EntityId, WaterContact)> = Vec::new();
```
Field count and byte sum computed directly from the struct at HEAD (63 fields, 433 B raw, 4-byte alignment → 436 B) and at `85b77371` (18 fields, 104 B).

**The lock-ordering reason for the deferral is real and documented** (`:579-580`) — this finding is about the **buffer**, not the deferral.

`WaterMaterial` is also copied wholesale in `byroredux/src/render/water.rs:179` (`let mut mat = plane.material;`, per plane per frame, to apply the TOD blend) and compared/stored in `submersion_system`'s `best: Option<(f32, WaterMaterial)>` (`byroredux/src/systems/water.rs:147`).

## Impact

Bounded by the number of wet bodies, which is small in every cell measured so far — **this is not a present-tense hot-path problem. It is a trajectory problem.** The struct grew 4.2× in one session while remaining a by-value payload in a per-frame `Vec` **and** in a `SparseSetStorage` row, and there is no guard pinning its size the way `gpu_instance_layout_tests.rs` pins `GpuInstance` at 128 B. No quantitative allocation guard exists for this site.

## Suggested Fix

Two independent moves, **either sufficient**:

1. **Reuse the buffer**: hoist `writes` (and `surfaces` / `current_volumes`) into persistent scratch cleared per frame.
2. **Stop embedding**: replace `WaterContact.material: Option<WaterMaterial>` with the `surface_entity` that is *already on the struct* plus a lookup, or with a small handle — the material is **per-plane, not per-contact**, and every consumer already has the plane entity in hand.

Additionally, add a `size_of::<WaterMaterial>()` assertion so the next growth is a deliberate decision rather than a diff artifact.

## Related

- The `apply_buoyancy` O(all bodies) prologue (filed separately) — same function
- #2887 (`PHYS-D6-04`, OPEN — `WaterContact::depth` semantics)

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved — the deferred drain exists precisely to keep the `PhysicsWorld` and storage locks disjoint; a scratch hoist must not change that ordering
- [ ] **SIBLING**: Same pattern checked in related files — the other two by-value `WaterMaterial` embeddings (`WaterPlane`, `WaterSurface`) and the per-frame copy in `render/water.rs`
- [ ] **TESTS**: A regression test pins this specific fix (a `size_of::<WaterMaterial>()` / `size_of::<WaterContact>()` assertion, mirroring the `GpuInstance` 128 B pin)
