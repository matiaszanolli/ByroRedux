# TD2-101: terrain_lod.rs reimplements the canonical Z-up→Y-up swizzle inline instead of calling zup_to_yup_pos

**GitHub Issue**: #2061
**Labels**: low,legacy-compat,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `byroredux/src/cell_loader/terrain_lod.rs:447,464,483-486,500,591,594`

## Description
Third recurrence of the same bug class (#1318 → #1617 → #1753, now `terrain_lod.rs`). The sibling full-detail builder `terrain.rs` was fixed under #1753 and now calls `zup_to_yup_pos`; the LOD variant, whose own comment says it mirrors `terrain.rs`, was never swept.

## Evidence
Confirmed live: `byroredux/src/cell_loader/terrain.rs` imports and calls `zup_to_yup_pos` (lines 19, 352, 363). `byroredux/src/cell_loader/terrain_lod.rs` hand-derives the same Z-up→Y-up axis swap inline at the claimed lines — e.g. the normal decode reads `let normal = ... [nx / len, nz / len, -ny / len]` with a comment noting "same Z-up→Y-up decode as the full-detail terrain path" rather than calling the shared helper.

## Impact
Bit-equivalent today, but signals a process gap — new coordinate-conversion sites keep bypassing the canonical helper.

## Related
#1318, #1617, #1753/TD2-005 (all closed) — new site not covered by any.

## Suggested Fix
Replace manual swizzle literals with `zup_to_yup_pos(...)` calls at all 4 forward sites.

**Effort**: small

## Completeness Checks
- [ ] **SIBLING**: Third recurrence of this exact bug class (#1318 → #1617 → #1753) — consider a lint/grep-based CI check that flags new `(x, z, -y)`-shaped literal swizzles outside `crates/core/src/math/coord.rs`
- [ ] **TESTS**: A regression test pins bit-identical output between the old inline swizzle and `zup_to_yup_pos` for the LOD terrain path
