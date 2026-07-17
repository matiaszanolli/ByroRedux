# 2010: NIFAL-D4-01: Canonical FurnitureMarker.heading_z_radians Option is re-resolved by a per-era gameplay heuristic

https://github.com/matiaszanolli/ByroRedux/issues/2010

Labels: medium, ecs, bug

**Severity**: MEDIUM · **Dimension**: Nodes (Furniture sub-category, NIFAL canonical translation)
**Tier Violated**: no-leak
**Location**: `crates/core/src/ecs/components/furniture.rs:41` (canonical field); consumer `byroredux/src/systems/sandbox.rs:69-71` (`is_sit_marker`) and `:97-104` (`seat_world_transform`)
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIFAL_2026-07-16.md (D4-01)

## Description
Commit `004b51c7` built a genuine `BsFurnitureMarker → ImportedFurnitureMarker → Furniture` translate path, clean on all four NIFAL tier invariants. However, the canonical `FurnitureMarker.heading_z_radians: Option<f32>` is re-resolved per-source-era at the gameplay layer by the M42 sandbox-seating system instead of at the translate boundary: `is_sit_marker` uses `.is_none()` as a proxy for "legacy content, treat as sit," and `seat_world_transform` branches `Some`/`None` into different heading derivations. This is the "Option/raw discriminator reaching a consumer that must re-resolve it" pattern the no-leak invariant exists to catch.

## Evidence
```rust
// sandbox.rs:69
fn is_sit_marker(m: &FurnitureMarker) -> bool {
    m.animation_type == 1 || m.heading_z_radians.is_none()
}
// sandbox.rs:97-104
let facing = match m.heading_z_radians {
    Some(h) => Quat::from_rotation_y(h),
    None if /* ... */ => Quat::from_rotation_y((-seat_local.x).atan2(-seat_local.z)),
    None => Quat::IDENTITY,
};
```
Both branches are doc-commented as intentional v0 approximations and unit-tested — a knowing, tested design choice, not an oversight.

## Impact
On FNV/FO3/Oblivion, every `BSFurnitureMarker` position is treated as a sit marker regardless of whether the source furniture is a bed (should be Sleep) or a lean-spot (should be Lean). Self-acknowledged v0 over-match; the whole M42 sandbox-seating system is opt-in (`BYRO_SANDBOX_SIT` off by default), so no default-behavior impact today. Architectural concern: the leak surface grows as more gameplay logic accretes onto `heading_z_radians.is_some()`/`is_none()` checks.

## Related
Furniture markers spawn only via the cell-loader path, not the loose-NIF path (mirrors the existing `BSBound` asymmetry, not yet documented for Furniture) — not filed separately, noted for a doc update.

## Suggested Fix
Not urgent given the opt-in gate. When the seating feature matures past v0: resolve the era discriminant once at the `furniture_component`/`imported_furniture_marker` translate boundary into an explicit canonical field (e.g. `pub kind: FurnitureMarkerKind` with `Sit`/`Sleep`/`Lean`/`Unknown` variants) rather than leaving `heading_z_radians.is_none()` as an implicit flag for gameplay code to re-derive.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in related files (other canonical `Option<T>` fields consumed by gameplay systems outside the translate boundary)
- [ ] TESTS: A regression test pins this specific fix (once resolved, update `is_sit_marker_modern_sit_and_legacy`)
