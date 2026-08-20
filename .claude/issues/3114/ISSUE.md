# PHYS-D6-2026-08-20-01: placed WaterCurrentVolume force is add_force'd with no reset_forces — user_force winds up linearly and launches the body

**Issue**: #3114 — https://github.com/matiaszanolli/ByroRedux/issues/3114
**Finding**: `PHYS-D6-2026-08-20-01`
**Labels**: bug, high, legacy-compat
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_PHYSICS_2026-08-20.md` — Dimension 6 (Water / Buoyancy)
**Severity**: HIGH · **Status**: NEW (introduced by `7e65c46c`, `feat(water): apply placed-reference current volumes`, this cycle)

## Location
- `crates/physics/src/water.rs:767-782` — the un-reset `add_force`
- `crates/physics/src/water.rs:718`, `:743`, `:757` — the three paths that *do* reset
- Producer: `byroredux/src/cell_loader/references/synth_child.rs:13-49`

## Trigger conditions
A cell with a `REFR` carrying `XWCU` + `XPRM` (an authored water-current marker — FO3/FNV rivers, Skyrim streams), **and** an awake dynamic body whose translation is inside that marker's box while it is *not* simultaneously resolved to a water surface with `submerged_fraction > 0`. That is: any body above the waterline inside the marker's vertical extent (the box is `position ± XPRM bounds × scale`, so its top routinely clears the surface), any body in a marker that does not overlap a `WaterPlane` volume in XZ, and any body in the `frac == 0.0 && !prior_wet` band.

## Description
Rapier's `RigidBody::add_force` accumulates into `forces.user_force`, which **persists across `pipeline.step()`** and is cleared only by `reset_forces` (`rapier3d-0.22.0/src/dynamics/rigid_body.rs:961-969` vs `:937-945`; `rigid_body_components.rs:796` recomputes `force = user_force + gravity·m` each step from the persisted value).

`apply_buoyancy` respects that in the water-plane branch — every application is preceded by `b.reset_forces(false)` (`water.rs:718`, plus the two dry-restore paths at `:743` / `:757`). The current-volume branch appended after it deliberately does **not** reset:

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

The reasoning is correct for the *co-located* case (a body floating in a river: the plane branch resets, then the marker adds on top). It is wrong for every case where the plane branch never runs — `surface == None && !prior_wet`, and `frac == 0.0 && !prior_wet`. There `user_force` is never zeroed and the per-frame term is added on top of the previous frame's total.

## Evidence
The per-frame term is `f_k = d·(s − v_k·d)·m·c` with `c = current_drag = 4.0` (`water.rs:73`). For a body held at `v ≈ 0` by ground contact and friction, `f_k` is a **constant** `m·c·s`, so the accumulated `user_force` after `n` frames is `n·m·c·s` — unbounded linear growth, not the bounded first-order response `current_force`'s own doc comment promises (`water.rs:118-128`). Once the accumulated force exceeds static friction the body is ejected.

Even airborne, the closed loop becomes `v̈ = −c·v + c·s` integrated twice — an integral controller where a proportional one was intended — so the body overshoots the authored current speed and oscillates instead of converging.

The prior audit explicitly cleared `current_force` as convergent. That verdict was correct for the call site that existed then (`water.rs:718-719`, reset-then-add) and does not carry to this one.

Verified at HEAD: `grep -n reset_forces crates/physics/src/water.rs` → `:428`, `:718`, `:743`, `:757`, `:1130`; none of them dominates the `add_force` at `:781`.

## Impact
Dynamic clutter near an authored water-current marker accelerates without bound while awake. The observable is Bethesda's classic "havok explosion" — a barrel on a riverbank creeping, then launching. It also pins the static-scene fast path indefinitely, because a body under a growing force never sleeps, which is the exterior-freeze regression `watal.md` §0 exists to prevent.

## Related
- #2889 — the documented `add_force`/`reset_forces` path had no production caller; this is now its second one, and the first without a reset.
- PHYS-D6-2026-08-20-04 (same function), `docs/engine/watal.md` §7 Phase 2.
- The `XWCU`/`WATR` decode feeding this path is the same decoder family settled by the WATR layout arbitration: #3104 #3105 #3106 #3107 #3108 #3109 #3110. The current-volume data path feeds off that decoder, so a fix here should be re-validated once those land — a field-shifted `wind_speed`/flow scalar changes the magnitude of the term that winds up, not the wind-up itself.

## Suggested fix
Hoist a single unconditional `b.reset_forces(false)` to the top of the per-target body work (before the surface branch) and delete the three per-branch resets — one owner of the force clear per body per frame, which is also what makes the "apply the marker after the plane" ordering safe by construction.

Pin it with a test that puts an awake dynamic body inside a `WaterCurrentVolume` with **no** overlapping `WaterPlane`, steps ~120 frames, and asserts `body.user_force()` is bounded (it must not grow monotonically).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other force producers in `crates/physics/src/water.rs`, and any future `add_force` caller — see #2889)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
