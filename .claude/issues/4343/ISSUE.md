# #4343 — TD2-001: `volumetrics.rs` hand-rolls five copies of `image_barrier_general_write_to_read`, which it imports

**Labels**: low, renderer, vulkan, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4343

- **Severity**: LOW · **Dimension**: 2
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:1201-1263` (×5); helper `crates/renderer/src/vulkan/descriptors.rs:270` · **Status**: NEW · **Age**: `5d3625416` (07-28) → `c98436b72`/`2325c1de4`/`2155cc917`/`e56d7654d` (08-18); all later than the helper · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: Each transported combustion field copied the previous barrier literal; the file calls the helper at `:1306`/`:1357`. The subresource is `color_subresource_single_mip()`, so the fields match exactly.
- **Suggested Fix**: Call the helper five times. Optionally add `image_barrier_general_read_to_write` for the paired pre-write barriers.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
