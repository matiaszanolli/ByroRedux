# REN-D5-01: a truncated BC DDS panics the floor-first upload path when streaming is on; the size check comes after the slice

Labels: medium,renderer,memory,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6737

**Severity**: MEDIUM · **Dimension**: Memory/Lifecycle

## Location
`TextureRegistry::flush_upload_batch` (`crates/renderer/src/texture_registry/upload.rs`), `dds::upload_byte_range` / `dds::level_byte_range_in` / `dds::payload_first_level` (`crates/renderer/src/vulkan/dds.rs`)

## Description
With `render.texture_streaming` on, a 2D chain above `FLOOR_EDGE_PX` takes the floor-first branch. `payload_first_level` returns 0 for any payload that is shorter than the chain and not an exact level suffix, so `floor_first` is `Some(floor)` with `payload_first == 0`. `upload_byte_range(&meta, first_mip > 0, 0, len)` then returns `level_byte_range_in(..)`, whose end is the whole chain's byte total, not clamped to `pixel_data.len()`. `flush_upload_batch` slices `&pixel_data[levels.clone()]` (for `average_rgb_levels`) and `&pixel_data[levels]` (for `record_dds_upload`) with it; the residency-change arm slices `&pixel_data[level_byte_range_in(..)]` the same way. All of these run before `check_dds_payload` can return the #4511 error.

## Evidence
`parse_dds` does not validate payload length for BC formats (only the expand path refuses short data). The existing test `a_truncated_dds_payload_is_a_returned_error_not_a_panic` (`vulkan/texture.rs`) exercises `check_dds_payload`, not the registry slice. The `payload_first_level` doc says a truncated file "still fails the upload's size check", which holds only if nothing slices first. Traced from code by the audit and re-traced by the publisher; not run on a device.

## Impact
A corrupt or mod-authored BC1/BC3/BC5 texture (>256 px, multi-mip, short payload) panics the render thread inside the one-time-commands path (holding the transfer-fence mutex) instead of falling back to the checkerboard. Streaming is OFF by default today, but #6631 (Phase 4) plans default-on, and `dds.rs` parses untrusted archive data.

## Suggested Fix
Reject (log + `continue`, as the parse-failure arm does) when `levels.end > pixel_data.len()`, in both the floor-first and the residency-change arms, or make `upload_byte_range` / `level_byte_range_in` return `Option`. Add a device-free test: a 512x512 DXT1 header with a few-byte payload must not panic the slicing step (factor the range check into a pure fn).

Related: #6626 (M39 tracking), #6631, closed #4511.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Other `pixel_data[..]` slices in `texture_registry/` checked
- [ ] **TESTS**: A regression test pins this specific fix
