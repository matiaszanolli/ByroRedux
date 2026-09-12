# TD2-002: UNDEFINED->SHADER_READ_ONLY_OPTIMAL init barrier duplicated verbatim in two files, no shared helper to catch it

Labels: low,tech-debt,renderer,bug

**Description**: Both `gbuffer.rs` and `frame_upscaler.rs` build an identical one-shot discard-and-transition barrier with only the image list differing. `descriptors.rs` has the mirror-image `image_barrier_undef_to_general` (UNDEFINED->GENERAL) but no UNDEFINED->SHADER_READ_ONLY_OPTIMAL equivalent, so this shape had nowhere to funnel through. `caustic.rs::initialize_layouts` builds a close cousin over a layered subresource range, same root cause (no layered variant).

**Evidence**:
`crates/renderer/src/vulkan/gbuffer.rs:330-338`, `crates/renderer/src/vulkan/frame_upscaler.rs:306-318`, `crates/renderer/src/vulkan/descriptors.rs`.

**Impact**: No runtime impact — duplication risk across two (soon three, counting caustic.rs's cousin) independently hand-rolled init barriers.

**Related**: None named.

**Suggested Fix**: Add `image_barrier_undef_to_shader_read(image) -> vk::ImageMemoryBarrier<'static>` to `descriptors.rs` mirroring `image_barrier_undef_to_general`; migrate `gbuffer.rs`/`frame_upscaler.rs` onto it; optionally add a `_layers` variant (following the existing `image_barrier_undef_to_transfer_dst`/`_layers` split) so `caustic.rs` can adopt it too.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
