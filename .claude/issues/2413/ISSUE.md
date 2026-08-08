# TD2-116: Undef→transfer-dst→shader-read barrier pair hand-rolled 3x instead of calling existing descriptors.rs helpers

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2413
**Finding ID**: TD2-116 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 2 — Logic Duplication
**Location**: `crates/renderer/src/vulkan/exposure.rs:150-199`, `ssao.rs:413-478`, `placeholder.rs:279-293` vs. `descriptors.rs:263-312`
**Status**: NEW

## Description
`descriptors.rs` already exposes parameterized `image_barrier_undef_to_transfer_dst_layers`/`image_barrier_transfer_dst_to_shader_read_layers` helpers. Three sites hand-roll the identical `vk::ImageMemoryBarrier` pair instead of calling them. `exposure.rs` (new file, 2026-07-22) additionally reintroduced a stale `TOP_OF_PIPE` stage-mask idiom the rest of the family already migrated off of, despite being brand-new — concrete evidence of the divergence risk this duplication shape creates.

## Related
Prior consolidations in this file family (#2200, same category, already fixed).

## Suggested Fix
Replace hand-rolled barrier pairs with the existing `descriptors.rs` helpers in all three files; switch `exposure.rs`'s stage mask to `PipelineStageFlags::NONE` while touching it.

## Completeness Checks
- [ ] **SIBLING**: Grep for any other hand-rolled undef→transfer-dst→shader-read barrier pair outside these three sites
- [ ] **TESTS**: Vulkan validation layers remain clean in debug after the barrier consolidation
