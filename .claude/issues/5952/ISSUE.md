# PERF-VOL-03: Volumetrics' persistent CPU scratches have no telemetry row and no shrink path

Labels: low,performance,renderer,bug
Source: docs/audits/AUDIT_PERFORMANCE_2026-10-17.md

- **Severity**: LOW
- **Dimension**: Telemetry (Dim 8)
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:630` (`fog_cluster_refs: Vec<FogClusterRef>`), `:617` (`combustion_light_candidates`); `crates/renderer/src/vulkan/context/mod.rs:2105-2150` (`scratch_telemetry_coverage_tests`)
- **Status**: NEW
- **Description**: `fog_cluster_refs` is cleared and refilled each frame by `build_fog_volume_clusters` and only ever grows. Its ceiling is 786,432 refs × 6 B ≈ 4.7 MB (4096 clusters × (64 density + 128 portal)); the pathological scene in "Measured here" reached 180,224 refs. `ctx.scratch` cannot show it, and `shrink_frame_scratch` does not release it. The coverage test that is supposed to catch exactly this walks for fields whose name `ends_with("_scratch")` (`mod.rs:2150`); neither field is named that way and both live on `VolumetricsPipeline`, not `VulkanContext`.
- **Evidence**: `grep fog_cluster_refs crates/renderer/src/vulkan/context/{telemetry,shrink_frame_scratch}.rs` returns nothing. `volumetric_fog_scratch` (the sibling Vec) does have a row.
- **Impact**: Observability gap and a one-way high-water mark of a few MB of host RAM after a pathological scene. No per-frame cost.
- **Suggested Fix**: Add a `ScratchRow` for each and let the coverage test key on type or on a shared marker, not only the name suffix.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other init/teardown sites)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
