# REN-D5-2026-09-26-20: `instance_map_scratch` (#4193) is the only per-frame scratch `Vec` with no shrink

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4897

**Labels**: low,renderer,memory,bug

- **Severity**: LOW (host RAM only, ≤ 2 MiB at 262,144 draws)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/mod.rs` (`ScratchBuffers::instance_map_scratch`), `crates/renderer/src/vulkan/acceleration/predicates.rs` (`build_instance_map`: `out.clear(); out.reserve(len)`), `crates/renderer/src/vulkan/context/shrink_frame_scratch.rs` (`shrink_frame_scratch`, four `Vec`s only)
- **Status**: NEW (sibling of the #2486 shrink-policy class; #4610 gave it a telemetry row but no shrink)
- **Description / Evidence**: `Vec<Option<u32>>` (8 B/entry) reserves to the draw count each frame and is never passed to `shrink_scratch_if_oversized`, unlike `gpu_instances_scratch`, `frame_lights_scratch`, `previous_models_scratch` and `batches_scratch`. One large exterior frame pins its capacity for the session.
- **Suggested Fix**: Add it to `shrink_frame_scratch` with the standard `2 × max(working, 512)` band.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
