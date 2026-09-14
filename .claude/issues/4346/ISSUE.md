# #4346 — TD2-004: The hybrid froxel Z-slice mapping is copied four times across three shaders, and its Rust mirror has drifted clamps

**Labels**: low, shaders, renderer, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4346

- **Severity**: LOW · **Dimension**: 2
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp:1345-1367`, `crates/renderer/shaders/volumetrics_integrate.comp:49-59`, `crates/renderer/shaders/composite.frag:130-140`; Rust `crates/renderer/src/vulkan/volumetrics.rs:422-455` · **Status**: NEW · **Age**: `5d3625416` (07-28) · **Effort**: small · **Kind**: tech-debt
- **Finding**: The copies are body-identical (only UBO member names differ). The Rust mirror floors at `1.0e-4` where every shader floors at `1.0`, so it can't catch shader drift.
- **Suggested Fix**: A guarded *include/froxel_slices.glsl* (new) exposing `froxelSliceDistance`/`froxelSliceCoordinate`; align the Rust floors or document why they differ.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
