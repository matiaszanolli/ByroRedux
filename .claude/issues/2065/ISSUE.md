# TD2-106: Parent-child TRS composition hand-rolled at 6 sites instead of calling GlobalTransform::compose

**GitHub Issue**: #2065
**Labels**: low,ecs,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `byroredux/src/cell_loader/spawn.rs:480-482,819-821` (duplicated within the same file), `refr.rs:499-501`, `placement_lod.rs:513-515`, plus position-only variants at `spawn.rs:350,422`

## Description
`GlobalTransform::compose` already implements this formula and is well-tested, but none of the REFR/SCOL/LOD placement sites use it.

## Evidence
Confirmed live: `crates/core/src/ecs/components/global_transform.rs:49` defines `pub fn compose(parent: &GlobalTransform, local_translation: Vec3, local_rotation: Quat, local_scale: f32) -> Self` with body `parent.translation + parent.rotation * (parent.scale * local_translation)`. `byroredux/src/cell_loader/spawn.rs` hand-rolls the identical formula (`ref_rot * (ref_scale * nif_pos) + ref_pos`) at multiple sites (light placement, particle emitter placement, collision placement, mesh placement); `refr.rs` and `placement_lod.rs` do the same at their claimed lines.

## Impact
Sits on the hot REFR-spawn path; a future composition-order fix would need 6 hand-applications.

## Suggested Fix
Call `GlobalTransform::compose` directly where available, or add a `compose_trs()` free function for loose-component callers; route all 6 sites through it.

**Effort**: small-medium

## Completeness Checks
- [ ] **SIBLING**: All 6 sites (2 in `spawn.rs`, 1 each in `refr.rs`/`placement_lod.rs`, plus 2 position-only variants) need the same swap — verify none has drifted formula-wise before assuming a mechanical replace
- [ ] **TESTS**: A regression test pins bit-identical placement output before/after routing through `GlobalTransform::compose`
