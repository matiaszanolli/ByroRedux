# LC-2026-10-27-D1-01: A scattered-name Z-up→Y-up swap in the exterior apply job is invisible to the #6321 ratchet

Labels: low, legacy-compat, tech-debt, bug
Source: docs/audits/AUDIT_LEGACY_COMPAT_2026-10-27.md

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-27.md` finding `LC-2026-10-27-D1-01` (Dimension 1, coordinate + placement fidelity) · Severity LOW · Status NEW (the line dates from `10d320f07`, 2026-04-12; moved by `1d383cc5a` #6271; survived the #1044, #1617 and #6321 consolidations).

**Location**: `byroredux/src/cell_loader/exterior/apply_job.rs:237-244` (`terrain_center`); the guard is `byroredux/src/workspace_hygiene_tests.rs` (`no_production_vec3_hand_rolls_the_zup_to_yup_swap` / `hand_rolled_zup_swaps`).

## Description
The bulk loader's initial-camera terrain point builds its Y-up vector by hand instead of calling `zup_to_yup_pos` / `cell_grid_to_world_yup`. The ratchet's matcher only recognises a swap whose three arguments share one prefix (`x/z/y` fields or `[0]/[2]/[1]` indices). `world_x, mid_height + 200.0, -world_y` has three different names, so it passes the scan. A grep over production code for `Vec3::new(<…x…>, <…>, -<…y…>)` finds this as the only survivor.

## Evidence
```rust
let world_x = gx as f32 * EXTERIOR_CELL_UNITS + 16.0 * 128.0;
let world_y = gy as f32 * EXTERIOR_CELL_UNITS + 16.0 * 128.0;
// Z-up → Y-up: (x, height, -y), plus 200 units above ground.
Vec3::new(world_x, mid_height + 200.0, -world_y)
```

## Impact
No wrong output today; the result equals `zup_to_yup_pos([world_x, world_y, mid_height + 200.0])`. The risk is the one #1044 and #6321 exist to remove: a future change to the canonical convention would not reach this site, and the guard would not flag it.

## Related
#6321 (closed), #1044, #1617.

## Suggested Fix
Write it as `Vec3::from(zup_to_yup_pos([world_x, world_y, mid_height + 200.0]))`. Optionally extend `hand_rolled_zup_swaps` to flag any `Vec3::new(a, b, -c)` whose first and third arguments end in `x` and `y`, with an allowlist for real Y-up literals.

## Completeness Checks
- [ ] **SIBLING**: Re-run the `Vec3::new(<x>, <>, -<y>)` grep over `byroredux/src` and `crates/*/src` production code after the fix
- [ ] **CANONICAL-BOUNDARY**: The conversion goes through `crates/core/src/math/coord.rs` helpers only
- [ ] **TESTS**: The extended `hand_rolled_zup_swaps` matcher has a fixture for the scattered-name shape

