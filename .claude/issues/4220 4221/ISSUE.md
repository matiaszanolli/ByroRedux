# #4220 — TD2-001: `ssao.rs` hand-rolls an image barrier identical to an existing `descriptors.rs` helper

**Description**: `SsaoPipeline::dispatch`'s post-dispatch barrier is field-for-field identical to the existing `image_barrier_general_to_shader_read` helper; `ssao.rs` already imports sibling `descriptors::` helpers a few lines above — this one site was simply missed when the module was wired up.

**Evidence**: `crates/renderer/src/vulkan/ssao.rs:512-519`, `crates/renderer/src/vulkan/descriptors.rs` (`image_barrier_general_to_shader_read`, ~379-387).

**Suggested Fix**: Replace the manual construction with `super::descriptors::image_barrier_general_to_shader_read(ao_image)`.

## Completeness Checks
- [x] **UNSAFE**: No new unsafe blocks; the surrounding `cmd_pipeline_barrier` call is unchanged.
- [x] **SIBLING**: Found an additional, previously unreported sibling — `frame_upscaler.rs::record_fsr_barriers_after`'s second barrier element (GENERAL → SHADER_READ_ONLY_OPTIMAL for `self.outputs[frame].image`) is also field-for-field identical to the helper. Fixed alongside.
- [x] **DROP**: No Vulkan object lifecycle changes — pure barrier-construction dedup.
- [x] **TESTS**: `cargo test -p byroredux-renderer` (982 tests) verified passing unchanged; this is a mechanical, compiler-verified extraction with identical field values, not a behavior change.

**Disposition**: Replaced the manual barrier construction in `ssao.rs::dispatch` and the second barrier element in `frame_upscaler.rs::record_fsr_barriers_after` with calls to the existing `image_barrier_general_to_shader_read(image)` helper.

---

# #4221 — TD2-002: UNDEFINED->SHADER_READ_ONLY_OPTIMAL init barrier duplicated verbatim in two files, no shared helper to catch it

**Description**: Both `gbuffer.rs` and `frame_upscaler.rs` build an identical one-shot discard-and-transition barrier with only the image list differing. `descriptors.rs` has the mirror-image `image_barrier_undef_to_general` (UNDEFINED->GENERAL) but no UNDEFINED->SHADER_READ_ONLY_OPTIMAL equivalent, so this shape had nowhere to funnel through.

**Evidence**: `crates/renderer/src/vulkan/gbuffer.rs:330-338`, `crates/renderer/src/vulkan/frame_upscaler.rs:306-318`, `crates/renderer/src/vulkan/descriptors.rs`.

**Suggested Fix**: Add `image_barrier_undef_to_shader_read(image) -> vk::ImageMemoryBarrier<'static>` to `descriptors.rs` mirroring `image_barrier_undef_to_general`; migrate `gbuffer.rs`/`frame_upscaler.rs` onto it; optionally add a `_layers` variant so `caustic.rs` can adopt it too.

## Completeness Checks
- [x] **UNSAFE**: No new unsafe blocks.
- [x] **SIBLING**: `caustic.rs::initialize_layouts` builds a related but structurally different barrier (UNDEFINED → **GENERAL**, `dst_access = SHADER_READ | SHADER_WRITE`, over a layered subresource range) — it's a layered cousin of `image_barrier_undef_to_general`, not of the new `image_barrier_undef_to_shader_read` (different target layout and access mask entirely). Left as-is: the issue's own suggested fix marks this part "optionally", and forcing a mismatched helper onto it would be a functional guess, not a mechanical dedup.
- [x] **DROP**: No Vulkan object lifecycle changes.
- [x] **TESTS**: `cargo test -p byroredux-renderer` (982 tests) verified passing unchanged; mechanical, compiler-verified extraction with identical field values.

**Disposition**: Added `image_barrier_undef_to_shader_read(image)` to `descriptors.rs`, mirroring `image_barrier_undef_to_general`'s doc style. Migrated `gbuffer.rs::initialize_layouts` and `frame_upscaler.rs`'s one-time output-image init barrier onto it. Did not add a `_layers` variant for `caustic.rs`, since its barrier targets a different layout (GENERAL, not SHADER_READ_ONLY_OPTIMAL) with a different access mask — adopting the new helper there would require guessing at a functional change rather than extracting identical existing code, out of scope for this mechanical dedup.
