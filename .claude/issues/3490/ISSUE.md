# Issue #3490: PHYS-D6-2026-08-27b-01: the current-volume containment test measures the body origin Y while the surface test measures the collider AABB centre

Labels: medium, physics, water, bug
Filed: 2026-08-27 (published 2026-08-28)

---

**Source**: `docs/audits/AUDIT_PHYSICS_2026-08-27b.md` — PHYS-D6-2026-08-27b-01
**Severity**: MEDIUM · **Dimension**: 6 — Water / Buoyancy

## Location
- `crates/physics/src/water.rs:735-748` — the current-volume containment test
- `crates/physics/src/water.rs:761-774` — the surface test's `center_y`, carrying the `#2887` rationale in-comment
- `crates/physics/src/water.rs:725-733` — `pos`, the body origin both read
- `crates/physics/src/water.rs:839` — the surface branch re-deriving `center_y`

## Trigger Conditions
An authored `XWCU` / `WaterCurrentVolume` marker whose vertical band (`volume.min[1] .. volume.max[1]`) does not comfortably contain the whole body, plus a dynamic body whose collider is offset in Y from its rigid-body origin. The second half is not exotic — it is the norm for every `bhk` compound, because `collision_shape_to_parts` attaches each part at its own local isometry (`convert.rs:191-214`) and nothing re-centres the body on the shape. Rivers and waterfalls are the authored shape most likely to have a tight vertical band.

## Description
Inside `apply_buoyancy_with_scratch`'s per-body loop, `pos` is `*body.translation()` — the rigid-body **origin** (`water.rs:725-733`). The current-volume branch tests all three axes against it verbatim:

```rust
let current_flow = if pos.x < ux0 || pos.x > ux1 || pos.z < uz0 || pos.z > uz1 {
    None
} else {
    current_volumes
        .iter()
        .find(|current| {
            let v = &current.volume;
            pos.x >= v.min[0] && pos.x <= v.max[0]
                && pos.y >= v.min[1] && pos.y <= v.max[1]   // <- body ORIGIN
                && pos.z >= v.min[2] && pos.z <= v.max[2]
        })
        .map(|current| current.flow)
};
```

The surface test immediately below deliberately does **not** do this. It computes the collider AABB and uses its centre, and says why in a comment that applies word for word to the block above it (`water.rs:764-774`):

```rust
let aabb = collider.compute_aabb();
let (min_y, max_y) = (aabb.mins.y, aabb.maxs.y);
// #2887 - the collider AABB centre, NOT `pos.y` (the rigid
// body's ORIGIN). They coincide only for a shape centred on
// its body, which is exactly what this module's test balls
// are and exactly what the bhk import path is not:
// `collision_shape_to_parts` attaches every compound part at
// its own local isometry, and ragdoll bones are offset by
// construction. ...
let center_y = 0.5 * (min_y + max_y);
```

XZ is *consistently* origin-based on both branches, which is defensible (the union-footprint prune at `water.rs:735` is origin-based too, and horizontal offsets are small relative to marker footprints). Y is the axis on which the two disagree, and Y is the axis a current marker's band is tight on.

## Evidence
`#2887` was filed by `docs/audits/AUDIT_PHYSICS_2026-08-13.md` as PHYS-D6-04 — *"`WaterContact::depth` is measured from the body origin, not the collider AABB centre its doc promises … the norm for compound bhk shapes and ragdoll bones"* — and its fix is pinned by *depth_is_measured_from_the_collider_aabb_centre_not_the_body_origin* (`water.rs:1592`), whose fixture is *"a compound whose leaf hangs 40 BU below the body origin"*. Run that same fixture through the current-volume branch and it fails the same way: a 40 BU error against a river band is the difference between in and out.

Re-verified at HEAD during publish: the `pos.y >= v.min[1] && pos.y <= v.max[1]` test is still present at `water.rs:735-748`, and the `#2887` `center_y` comment is still confined to the surface branch below it.

## Impact
A body whose collider sits below its origin leaves an authored current *early* (the origin exits the band while the body is still in the water) and is picked up *late* on entry. Because this branch is also the sole feeder of the `#3268` `in_current_prev` latch (`in_current_now.push(t.entity)`, `water.rs:949`), a body oscillating across the band boundary alternately enters and leaves the latch and is re-woken on every re-entry — the one path in this module that can defeat the latch's wake-once intent. No crash and no force leak (the `#3114` reset at `water.rs:832-834` is gated on `current_flow.is_some() || surface.is_some() || t.prior_wet`, so it still runs). The visible symptom is *"debris in this river only sometimes moves"*. Blast radius: every title with authored `XWCU` markers (FO3/FNV are confirmed producers).

## Related
`#2887` (CLOSED — the same defect on the surface branch); `#3268` (CLOSED — the latch this feeds); `docs/engine/watal.md` §7 Phase 2.

## Suggested Fix
Hoist the collider-AABB fetch above the `current_flow` computation and use `center_y` for the current volume's Y test, exactly as the surface test does. The AABB is already computed a few lines later for every body that reaches the surface test, so on the common path (marker co-located with its plane) this is free; only the current-only path pays a new `compute_aabb`. Extend the existing offset-compound fixture with a current-volume assertion so one fixture pins both branches.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
