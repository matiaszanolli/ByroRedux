# #4792: PERF-D4-2026-09-23b-02: `0572bfd5a` widened each fog cluster from 40 B to 784 B, so the per-frame cluster upload is now ~1.7–3.2 MB. The sun-swept portal pass that fills it runs every frame, at night and in open-sky scenes too

**Severity**: MEDIUM
**Labels**: medium, performance, renderer, memory, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D4-2026-09-23b-02)

- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Upload (with CPU Hot Paths and GPU Pipeline angles). This merges the Dim 1, 4 and 5 reports of the same site: PERF-D1-2026-09-23b-01, PERF-D4-2026-09-23b-02 and PERF-D5-2026-09-23b-01.
- **Location**:
  - `crates/renderer/shaders/include/shader_constants.glsl:342-343` and `crates/renderer/src/shader_constants_data.rs:1287,1291`: `MAX_FOG_VOLUMES_PER_CLUSTER` 8 → 64, new `MAX_FOG_PORTALS_PER_CLUSTER` = 128.
  - `crates/renderer/src/vulkan/volumetrics.rs`:
    - `:177`: `MAX_GPU_FOG_VOLUMES` 128 → 512;
    - `:191-195`: `FOG_CLUSTER_INDEX_STRIDE` = 192;
    - `:313-318`: `GpuFogClusterEntry` 8 → 16 B;
    - `:574-655`: `fog_portal_swept_bounds`;
    - `:735-779`: the portal slab loop;
    - `:1481-1488`: the upload `[..write_hi]` entries plus `[..write_hi * FOG_CLUSTER_INDEX_STRIDE]` indices.
  - `byroredux/src/render/fog_volumes.rs:86-93`: LightShaft/SkyAperture volumes bypass the frustum test.
- **Status**: NEW. It amplifies Existing #4788 (written against `17c01a4e5`, before this commit). #4788's `[lo, hi)` range fix does not bound it, for the reasons in the last bullet of the Description.
- **Description**:
  - #3834's layout gives each cluster a permanent offset (`cluster_index * FOG_CLUSTER_INDEX_STRIDE`). The upload is therefore O(touched clusters × capacity), not O(live references).
  - **Before**: 8 B entry + 8 × 4 B indices = 40 B per cluster; 10 KB per Z layer; 160 KB for the full set (#4788's figure).
  - **Now**: 16 B + 192 × 4 B = 784 B per cluster; ~200 KB per layer; 4,096 × 784 = **3.21 MB** for the full set.
  - The grid is camera-centred, so a single volume in the camera's cluster (8,8,8) gives `write_hi = 2185`, which uploads **1.71 MB/frame**.
  - `fog_portal_swept_bounds` sweeps every LightShaft/SkyAperture source along −sun to the grid boundary. Any sun azimuth with a +Z component pushes `write_hi` toward 4,096, and a diagonal sun makes the swept AABB span up to 4,096 clusters, each slab-tested on the CPU.
  - None of these gates the sweep:
    - **frustum**: portal volumes skip it (`fog_volumes.rs:89`), so one converted beam anywhere in the cell keeps the list non-empty and the pass dispatching;
    - **radiance**: the sweep checks only that the sun direction is finite and non-zero (`:579-586`), so at night (`portal_sun` radiance 0, `render/sky.rs`) it still fills portal lists and inflates the upload;
    - **open sky**: exteriors and Show-Sky interiors run it too, although the shader reads portal lists only behind `!hasOpenSky && localSkyAperture(...)` (`volumetrics_inject.comp:2936-2937`).
  - Why #4788's fix does not help: the unit stays 784 B however few references a cluster holds (usually 1–3), and a portal sweep crosses the grid, so even a tight `[lo, hi)` spans most layers.
- **Evidence**:
  - `git show f97775ca8:crates/renderer/shaders/include/shader_constants.glsl` has `MAX_FOG_VOLUMES_PER_CLUSTER 8u` and no portal constant. The orchestrator confirmed the HEAD constants 64/128 and the 192 stride.
  - The buffers are host-visible CpuToGpu and written with `write_mapped`.
  - `memory-budget.md:889` ledgers the ~6.5 MB of residency correctly. No doc or telemetry counter records the per-frame upload bytes.
  - Base-game beam placement counts from `docs/engine/interior-godrays-status.md`: FO3 6,363, FNV 3,051, Oblivion 1,249.
- **Impact**:
  - *est.* 0.1–0.6 ms of main-thread write-combined memcpy per frame, plus µs to sub-ms of slab tests per portal.
  - Paid in essentially every interior with a hearth or a converted beam, and in exteriors with a fire in view.
  - It runs in `record_post_passes`, after the all-slots fence wait, so on GPU-bound frames the GPU idles through it (the #4606 mechanism). It is reported under `cmd_record`, not `brd` (PERF-D8-2026-09-23b-01).
  - No quantitative guard exists for this site.
- **Related**: #4788, #3834, #4606, #4783; PERF-D8-2026-09-23b-01
- **Suggested Fix**:
  - Compact the cluster lists to CSR (count pass, prefix-sum, fill) and upload `entries[lo..hi]` plus the live index count only. This also subsumes #4788.
  - Skip the portal sweep when portal radiance is zero or the scene has open sky.
  - Replace the unconditional frustum bypass for LightShaft volumes with a reach cull.
  - Pin the upload size with a unit test (one camera-cluster volume ≤ N KB), and add an upload-bytes counter to the volumetrics telemetry.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **DROP**: If fog-cluster buffers are resized or split, the Drop / deferred-destroy path still covers every new buffer
- [ ] **TESTS**: A regression test pins this specific fix
