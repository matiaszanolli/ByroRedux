# REG-2026-08-20-D3-01: WATAL nearest-overlapping-surface rule implemented twice; only the camera copy has a test

**Issue**: #3212 — https://github.com/matiaszanolli/ByroRedux/issues/3212
**Severity**: MEDIUM
**Labels**: `medium,renderer,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_REGRESSION_2026-08-20.md` § REG-2026-08-20-D3-01 (Dimension 3 — Water / streaming / terrain-LOD churn surface).

**Severity**: MEDIUM
**Status**: NEW — **resolves the premise of the OPEN #2888, and re-opens it in a new form.** #2888 is recommended for close (comment posted there).
**Location**: `crates/physics/src/water.rs:235` (`nearest_surface_distance`) + `:661-662` (its use); `byroredux/src/systems/water.rs:226-236` (the inline camera copy); `byroredux/src/systems/water.rs:874` (`overlapping_water_volumes_choose_nearest_surface` — the sole guard).

## Description

**#2888** (`PHYS-D6-05`, OPEN) says: *"the two ends of WATAL disagree on which overlapping water plane wins — physics takes the first match, the camera the nearest."*

`4c383433` (2026-08-19, *"fix(water): choose nearest overlapping surface"*) fixed it on **both ends in the same commit** — but with **two independent implementations**:

- **Physics side**: a named private helper
  `fn nearest_surface_distance(surface_y: f32, reference_y: f32) -> f32 { (surface_y - reference_y).abs() }`, consumed by a `min_by`.
- **Camera side**: the same rule written **inline** as `depth.abs() < prev_depth.abs()` inside `submersion_system`'s selection loop.

The helper is **private** to `crates/physics` (`fn`, not `pub`), so `byroredux` could not reuse it even deliberately.

`4c383433` added **exactly one test** — `overlapping_water_volumes_choose_nearest_surface`, in `byroredux/src/systems/water.rs`, exercising the **camera** side. The 17 `#[test]`s in `crates/physics/src/water.rs` contain no `nearest` / `overlapping` case, so **the physics half of the convergence has no guard at all.**

## Evidence (verified at HEAD `bb0b92f2`)

```
$ grep -rn "nearest_surface_distance" --include='*.rs' .
crates/physics/src/water.rs:235:fn nearest_surface_distance(surface_y: f32, reference_y: f32) -> f32 {
crates/physics/src/water.rs:661:                        nearest_surface_distance(a.1, center_y)
crates/physics/src/water.rs:662:                            .total_cmp(&nearest_surface_distance(b.1, center_y))
```

One definition, both uses inside the same crate; **nothing in `byroredux`**.

The camera copy (`byroredux/src/systems/water.rs:231-236`):

```rust
let candidate = (depth, plane.material);
match best {
    None => best = Some(candidate),
    Some((prev_depth, _)) if depth.abs() < prev_depth.abs() => best = Some(candidate),
    _ => {}
}
```

```
$ grep -rn "overlapping_water_volumes_choose_nearest_surface" --include='*.rs' .
byroredux/src/systems/water.rs:874:    fn overlapping_water_volumes_choose_nearest_surface() {
```

Test census of `crates/physics/src/water.rs`: **17 `#[test]`, none covering surface selection.**

## Impact

This is the **exact shape of the last sweep's `REG-D1-01`** (#3081 — a fix copy-pasted into a second unguarded site): filed, fixed, and now **recurring in the delta's single hottest file** (`crates/physics/src/water.rs`, 17 commits since 2026-08-16).

The two copies already differ in **reference point** — camera position vs body-AABB centre, correct per consumer — which is precisely the kind of local divergence that makes the *next* edit touch one and not the other.

**Reverting either half to a signed comparison restores #2888's disagreement, and only one revert is detectable.**

WATAL's own design premise is a single canonical rule shared by its render and physics ends. Two implementations of that rule is the defect the layer exists to prevent.

## Suggested Fix

1. Make `nearest_surface_distance` **`pub`** in `crates/physics`. `byroredux` already imports `authored_wave_height_with_weather` and `weather_wave_adjustment` from that crate **in this same function**, so there is no new dependency edge.
2. Call it from `submersion_system`, deleting the inline comparison.
3. Add the **physics-side twin** of `overlapping_water_volumes_choose_nearest_surface` — two stacked `WaterVolume`s, one body between them — so **both ends fail loudly on a revert**.

## Related

- **#2888** (OPEN) — its stated divergence is *resolved* by `4c383433`; recommended for close, with this filed in its place
- **#3081** — the prior instance of this exact shape (last sweep's `REG-D1-01`)
- **#2887** (OPEN) — the sibling question of which reference point `WaterContact::depth` should use
- `4c383433` · `docs/engine/watal.md`
- The `WATERLINE_HYSTERESIS` and `weather_wave_adjustment` cross-end duplications filed from `AUDIT_TECH_DEBT_2026-08-20.md` — the same WATAL two-ends-drift family

## Completeness Checks
- [ ] **SIBLING**: Both ends call one function; `grep -n "prev_depth.abs()" byroredux/src/systems/water.rs` returns nothing
- [ ] **REFERENCE-POINT**: The `pub` helper's doc states that the *reference point* is the caller's choice (camera position vs body-AABB centre) and only the *comparison rule* is shared
- [ ] **TESTS**: A physics-side guard exists and **fails** when `crates/physics/src/water.rs:661` is reverted to a signed comparison — verify by reverting locally
- [ ] **CANONICAL-BOUNDARY**: The rule lives once, in the crate both ends already depend on — not re-derived at either consumer
