# #4349 — TD2-007: `descriptors.rs` has no layered GENERAL→SHADER_READ helper, so the sky cube bake hand-rolls it and the ground-cover bench grew a private builder

**Labels**: low, renderer, vulkan, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4349

- **Severity**: LOW · **Dimension**: 2
- **Location**: `crates/renderer/src/vulkan/sky_cube.rs:463-505`, `crates/renderer/src/vulkan/groundcover_bench.rs:1181-1206`; family `crates/renderer/src/vulkan/descriptors.rs:404-466` · **Status**: NEW (same gap class as closed #4221, for layered images) · **Age**: `b54b86b7b`/`6db9eac2e` (09-13), `40b5c5b6a` (09-06) · **Effort**: small · **Kind**: tech-debt
- **Suggested Fix**: Add `image_barrier_general_to_shader_read_layers(image, layer_count)` with the single-layer helper delegating to it; migrate both sites and delete the bench builder.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
