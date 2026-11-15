# #6960: TD2-2026-11-15-07: Three raw-`Allocation` readback paths re-spell `GpuBuffer::invalidate_if_needed` and lack its #2683 range guard

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6960
- **Labels**: low,renderer,vulkan,memory,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none
- **Age**: screenshot ddd05a9df (2026-04-13) + #2751/#2752 (be39814d9); depth_capture 283b71824 (2026-08-29); `linear_capture.rs` 75b4024df (2026-11-02) is the third copy
- **Location**: `crates/renderer/src/vulkan/context/screenshot.rs:47-72`, `depth_capture.rs:55-80`, `linear_capture.rs:150-172`; the home is `GpuBuffer::invalidate_if_needed` (`crates/renderer/src/vulkan/buffer.rs:1270`)
- **Description**: each copy is the same 25 lines: `if !allocation.memory_properties().contains(HOST_COHERENT) { aligned_flush_range(..); unsafe { invalidate_mapped_memory_ranges(&[MappedMemoryRange..]) }; warn on Err }`. Each cites the earlier one in its comment ("Same reasoning and same shared range helper as the screenshot path"). `GpuBuffer::invalidate_if_needed` additionally calls `debug_assert_flush_range_bounded(alloc, ..)` (#2683, `buffer.rs:747-`), which flags a widened range that escapes a dedicated allocation (VUID-VkMappedMemoryRange-size-01389); none of the three copies has it. #2740 had to retrofit the depth path to match the screenshot path, and the third path was added by copying the second. The only divergence is a debug assertion, which is why this is LOW; it fits the promotion trigger in letter only.
- **Suggested Fix**: `buffer::invalidate_allocation(device: &ash::Device, allocation: &vulkan::Allocation) -> Result<()>` holding the coherent check, `aligned_flush_range`, `debug_assert_flush_range_bounded` and the `invalidate_mapped_memory_ranges` call; `GpuBuffer::invalidate_if_needed` delegates to it when `!self.is_coherent`; the three readbacks call it and keep their own `warn!`. `screenshot_readback_invalidates_before_mapping` (`context/resources.rs:1319`) scans `screenshot.rs` for the literal `invalidate_mapped_memory_ranges`, so re-point it at `invalidate_allocation` in the same commit.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
