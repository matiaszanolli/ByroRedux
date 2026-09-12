# TD2-001: `ssao.rs` hand-rolls an image barrier identical to an existing `descriptors.rs` helper

Labels: low,tech-debt,renderer,bug

**Description**: `SsaoPipeline::dispatch`'s post-dispatch barrier is field-for-field identical to the existing `image_barrier_general_to_shader_read` helper; `ssao.rs` already imports sibling `descriptors::` helpers a few lines above — this one site was simply missed when the module was wired up. No divergent history on either side.

**Evidence**:
`crates/renderer/src/vulkan/ssao.rs:512-519`, `crates/renderer/src/vulkan/descriptors.rs` (`image_barrier_general_to_shader_read`, ~379-387).

**Impact**: No runtime impact — pure duplication risk (a future barrier-policy change would need updating in two places).

**Related**: None named.

**Suggested Fix**: Replace the manual construction with `super::descriptors::image_barrier_general_to_shader_read(ao_image)`.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
