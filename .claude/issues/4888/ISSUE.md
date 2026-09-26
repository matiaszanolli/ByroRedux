# REN-D5-2026-09-26-10: The ground-cover blade draw issues a multi-draw indirect on a device that may lack `multiDrawIndirect`; #4827's "every indirect consumer is gated" pin does not cover it

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4888

**Labels**: medium,renderer,vulkan,terrain-exterior,bug

- **Severity**: MEDIUM (a Vulkan spec violation where reachable, downgraded from HIGH only because no RT-capable device in the supported class lacks `multiDrawIndirect`)
- **Dimension**: Memory/Lifecycle (device-feature degradation)
- **Location**: `crates/renderer/src/vulkan/groundcover.rs` (`GroundCoverPipeline::record_draw`: `cmd_draw_indirect(.., self.frame_chunk_count, 16)` per LOD stream), `crates/renderer/src/vulkan/context/init.rs` (`groundcover` creation gated on `device_caps.ray_query_supported` only), `crates/renderer/src/vulkan/device.rs` (`caps_tests::first_instance_feature_is_enabled_and_every_indirect_consumer_is_gated`)
- **Status**: NEW (sibling of closed #4827)
- **Description / Evidence**:
  - I confirmed both halves in code. `create_logical_device` enables `multiDrawIndirect` only `if caps.multi_draw_indirect_supported`. `#4827` routed the main-batch path and the model tier through `DeviceCapabilities::indirect_draws_supported()`. The blade draw is a third indirect consumer with `drawCount = frame_chunk_count` (up to `GROUNDCOVER_MAX_CHUNKS` = 256 per stream).
  - With the feature off this violates VUID-vkCmdDrawIndirect-drawCount-02718 (drawCount must be 0 or 1). Its `firstInstance` is always 0, so only the multi-draw bit is at issue.
  - The pin test's name claims "every indirect consumer" but scans only `geometry_pass.rs`, `build_and_upload_instances.rs` and the model-tier creation site in `init.rs`. It passes today.
- **Impact**: A per-frame spec violation on any exterior with ground cover, on a device without `multiDrawIndirect`.
- **Suggested Fix**: Either create `groundcover` only when `multi_draw_indirect_supported`, or fall back to one `cmd_draw_indirect` per chunk. Or make `multiDrawIndirect` a hard requirement in `is_device_suitable` and delete the `multi_draw_indirect_supported` branches. Extend the pin to `groundcover.rs`. **Needs a `BYRO_VALIDATION=gpuav` run on a feature-limited device**; do not change the draw ordering blind.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
