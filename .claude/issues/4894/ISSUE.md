# REN-D5-2026-09-26-16: About a dozen `ray_query_supported == false` branches are still maintained although #3759 made them unreachable

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4894

**Labels**: low,renderer,tech-debt,bug

- **Severity**: LOW (dead code with a real cost: it presents fallbacks that no longer exist)
- **Dimension**: Memory/Lifecycle (device init)
- **Location**: `crates/renderer/src/vulkan/context/init.rs` (`create_allocator` arg, `SceneBuffers::new` arg, `accel_manager`, `skin_compute`, `skin_palette`, `groundcover`, `water`), `context/geometry_pass.rs`, `context/assemble_camera_and_lights.rs`, `context/dispatch_skin_and_cluster.rs`, `context/telemetry.rs`, `crates/renderer/src/vulkan/device.rs` (`create_logical_device`, `RT_EXTENSIONS` doc)
- **Status**: NEW (the follow-up #3759's comment promised was never filed)
- **Description / Evidence**: `is_device_suitable` returns `None` when `!ray_query_supported`, so `caps.ray_query_supported` is a constant `true` at every reachable site. The gates keep the rt-disabled descriptor-layout permutation and the `buffer_device_address` allocator flag alive. They also mask finding 10: the *real* optional cap is the one left un-gated.
- **Suggested Fix**: Delete the branches or collapse them behind one `debug_assert!`, and correct the `RT_EXTENSIONS` doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
