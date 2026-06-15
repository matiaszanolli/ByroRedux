# LC-D1-01: Z-up→Y-up coordinate-flip duplicated at ~10 sites that bypass the single source of truth

**Severity**: LOW · **Dimension**: D1 (Coordinate-System Correctness)
**Status**: NEW (incomplete consolidation of CLOSED #1318)
**From**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-06-14.md`

**Location**:
- `byroredux/src/cell_loader/references.rs:235-238`
- `byroredux/src/cell_loader/refr.rs:496`
- `byroredux/src/cell_loader/transition.rs:135-136`
- `byroredux/src/systems/particle.rs:90-91`
- `crates/nif/src/import/mod.rs:208`
- `crates/nif/src/import/mesh/tangent.rs` + `mesh/skin.rs:475-477`
- `crates/nif/src/import/collision.rs:796-833` (`havok_to_engine` / `havok_quat_to_engine` / `decompose_havok_matrix`)

## Description
`crates/core/src/math/coord.rs` (`zup_to_yup_pos`, `zup_to_yup_quat_wxyz`) + its NIF-typed wrappers in `crates/nif/src/import/coord.rs` are the **single source of truth** for the `(x, z, -y)` axis swap. #1318 ("Z-up coord-flip leaked 4 sites", CLOSED) consolidated four sites; the sites above were left behind. The two highest-blast-radius are on the **live REFR placement path**: `references.rs:235` (every spawned object's position — built inline as `Vec3::new(pos[0], pos[2], -pos[1])` while the adjacent rotation *does* route through `euler_zup_to_quat_yup_refr`) and `refr.rs:496` (SCOL child placement, same asymmetry). The Havok helpers in `collision.rs` are a self-consistent **parallel** SoT; `decompose_havok_matrix` uses `Quat::from_mat3` directly, skipping the `#333` explicit-normalize guard that `zup_matrix_to_yup_quat` carries.

## Evidence
`references.rs:236-238` `placed_ref.position[0], placed_ref.position[2], -placed_ref.position[1]`; `refr.rs:496` `Vec3::new(p.pos[0], p.pos[2], -p.pos[1])`; `collision.rs:796` `fn havok_to_engine(x,y,z) -> Vec3 { Vec3::new(x, z, -y) }`. All verified bit-identical in value to the SoT helpers — there is **no current mis-placement**.

## Impact
No runtime defect today — values are correct. The risk is maintainability/regression: a future fix to the canonical swap (e.g. another `#333`-class normalize guard, or a precision change) will not propagate to these copies, and a hand edit to the REFR-placement copy could silently skew all object placement. Filed LOW because there is no current incorrect behavior; blast radius is "future divergent edit on a hot path."

## Suggested Fix
Route `references.rs` / `refr.rs` / `transition.rs` / `particle.rs` / `import/mod.rs` position swaps through `coord::zup_to_yup_pos`, and fold the three Havok helpers into the `coord.rs`/`import/coord.rs` family (routing `decompose_havok_matrix`'s rotation through `zup_matrix_to_yup_quat` so it picks up the normalize guard). Keep the magnitude-only `half_extents` variant (`import/mod.rs:209`) as-is — it is a deliberate non-swap.

## Related
CLOSED #1318 (the partial consolidation this completes).

## Completeness Checks
- [ ] **SIBLING**: All ~10 listed swap sites routed through the SoT (or the Havok family folded in); the deliberate `half_extents` non-swap left intact
- [ ] **CANONICAL-BOUNDARY**: The coord SoT (`crates/core/src/math/coord.rs` + `crates/nif/src/import/coord.rs`) remains the single producer of the `(x, z, -y)` swap; `decompose_havok_matrix` inherits the `#333` normalize guard
- [ ] **TESTS**: A test pins each consolidated site to the SoT result (bit-identical value preserved)
