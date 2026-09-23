**HEAD**: `2237da9c3` · **Baseline**: `docs/audits/AUDIT_SAFETY_2026-09-21.md` (@ `f97775ca8`) · **Audited**: Dims 2–7, restricted to the M55 volumetric fog system · **Unchanged since baseline (skimmed)**: none skimmed. The volumetrics sources had zero commits since `f97775ca8`, but the 09-21 baseline only spot-checked them, so this suite leg covered them in depth. · **Out of suite scope**: Dim 1 (FFI) and Dim 8 (mod runtime).

# Safety Audit (area-scoped: volumetrics) — ByroRedux — 2026-09-23

**Command**: `/audit-safety`, one leg of `/audit-suite --preset volumetrics-deep`
**Scope**: area-scoped to the volumetric fog system (M55). Files covered:
- `crates/renderer/src/vulkan/volumetrics.rs`, `volumetrics/init.rs`, `volumetrics/noise.rs`
- `crates/renderer/shaders/volumetrics_inject.comp` and `volumetrics_integrate.comp`, plus their includes `medium_transport.glsl`, `froxel_slices.glsl`, `shadow_common.glsl` and `blue_noise.glsl`
- the lifecycle and dispatch call sites: `context/{init,resize,teardown,post_passes,assemble_camera_and_lights,draw}.rs`
- the composite consumer (`composite.frag`, `sampleVolumetricColumn`)
- the CPU producer `byroredux/src/render/fog_volumes.rs`

Anything outside the volumetrics area is excluded.

**Severity scale**: `.claude/commands/_audit-severity.md`

## Method

- **Delta.** Since `f97775ca8`, no commits touch `volumetrics.rs`, `volumetrics/`, `volumetrics_inject.comp` or `volumetrics_integrate.comp`. The recent volumetrics commits (BFECC `10798823b`, `f9494d7e1`, multiscatter `33d253b73`, #4532/#4533/#4535 `486619c7c`) all predate the baseline. The baseline covered them only as guard spot-checks, so this run reads every volumetrics file end to end. Adjacent commits since the baseline were checked for volumetrics impact and have none:
  - `post_passes.rs`: `77eb4b752` (#4591, the exposure-meter gate);
  - include/constants: `268cfd8ff`, `b9e961eeb`;
  - `2237da9c3`: documentation only.
- **No sub-agents.** Per-dimension notes are in `/tmp/audit/safety/dim_{1..8}.md`.
- **Commands run.** No engine launch, no source edits.
  - `cargo test -p byroredux-renderer --lib -- volumetric froxel fog_ combustion gpu_boundary_instance mirror` → **57 passed, 0 failed**. This includes `volumetrics_ubo_sizes_match_host_structs_in_every_shader`, the three `gpu_fog_volume_*` pins, `combustion_light_moment_abi_is_eight_std430_words`, `failed_combustion_moment_drain_leaves_the_slot_latched`, `volume_far_shader_constant_agrees_with_the_volumetrics_config_default` and `froxel_grid_cost_matches_the_memory_budget_doc`.
  - `glslangValidator -V` recompile of both volumetrics shaders to scratch: **byte-identical** to the committed `.spv`, so there is no stale-SPIR-V drift.
- **Dedup.** Checked against `/tmp/audit/issues.json` (4,660 issues, 181 open; title search on volumetric / froxel / fog / combustion / NaN / poison / image-dimension / field-order) and against `docs/audits/`. No match for any finding below.

## Census (volumetrics files, HEAD)

| File | word tokens | `unsafe {` blocks | `unsafe fn` | `unsafe impl` | `SAFETY` |
|---|---|---|---|---|---|
| `vulkan/volumetrics.rs` | 13 | 4 | 3 | 5 | 9 |
| `vulkan/volumetrics/init.rs` | 18 | 17 | 1 | 0 | 17 |
| `vulkan/volumetrics/noise.rs` | 2 | 1 | 1 | 0 | 1 |

- Every block carries a SAFETY comment. The renderer's `#![deny(clippy::undocumented_unsafe_blocks)]` at `lib.rs:21` covers them.
- The 5 `unsafe impl NoUninit` are re-proven padding-free under Dim 2.

## Findings summary

| ID | Severity | Dim | Status | Title |
|---|---|---|---|---|
| SAFE-D5-2026-09-23-01 | HIGH | 5 | NEW | Froxel XY extent is never checked against `maxImageDimension3D`, and `--froxel-xy-divisor` < 8 can create an over-limit 3D image |
| SAFE-D7-2026-09-23-01 | MEDIUM | 7 | NEW | The raw V-buffer temporal history has no non-finite guard: one NaN/Inf froxel self-perpetuates and spreads until a history reset |
| SAFE-D6-2026-09-23-01 | LOW | 6 | NEW | `VolumetricsParams` has only a block-size pin; a swap of two `vec4` members passes every test |
| Existing #4599 | MEDIUM | 3 | open | The `.expect("allocator lock poisoned")` in `GpuBuffer::destroy` is also reachable from `VolumetricsPipeline::destroy` teardown |

Counts: **0 CRITICAL · 1 HIGH · 1 MEDIUM · 1 LOW** (new), plus 1 existing reference.

## Findings

### SAFE-D5-2026-09-23-01: Froxel XY extent is never checked against `maxImageDimension3D`, and `--froxel-xy-divisor` < 8 can create an over-limit 3D image
- **Severity**: HIGH. This meets the Vulkan-spec-violation floor. It is reachable only with a non-default divisor at large render extents on devices with a 2048-texel 3D limit; the default configuration is safe.
- **Dimension**: Vulkan Spec Compliance
- **Location**:
  - `crates/renderer/src/vulkan/volumetrics.rs:664-676` (`froxel_extent`)
  - `crates/renderer/src/vulkan/upscaling.rs:151-161` (`VolumetricsConfig::validate`)
  - `crates/renderer/src/vulkan/volumetrics/init.rs:122-191` and `:827-840` (`create_volume`)
  - `crates/renderer/src/vulkan/context/init.rs:242-253`
- **Status**: NEW
- **Description**:
  - The froxel grid's width and height are `render_extent / froxel_xy_divisor`. `validate()` accepts any divisor in `2..=32`.
  - Nothing compares the resulting extent with `VkPhysicalDeviceLimits::maxImageDimension3D` or with the `imageCreateMaxExtent` of `vkGetPhysicalDeviceImageFormatProperties`. Six 3D images per frame-in-flight are created at that extent.
  - The renderer does have the sibling guard for 2D: `context/init.rs` reads only `max_image_dimension2_d` and feeds it to `FrameExtentSet::for_output`, which rejects render/output extents above it (`upscaling.rs:217-242`). The 3D limit is never read anywhere in the crate. `grep max_image_dimension crates/renderer/src` finds only the 2D field.
  - The Z axis is safe by construction: `froxel_z_slices` is capped at 256, which is exactly the spec's guaranteed minimum for `maxImageDimension3D`. X and Y have no equivalent cap.
- **Evidence**:
  ```rust
  // volumetrics.rs:664
  pub fn froxel_extent(render_extent: vk::Extent2D, config: VolumetricsConfig) -> vk::Extent3D {
      vk::Extent3D {
          width: render_extent.width.div_ceil(config.froxel_xy_divisor).max(1),
          height: render_extent.height.div_ceil(config.froxel_xy_divisor).max(1),
          depth: config.froxel_z_slices,
      }
  }
  // upscaling.rs:152 — the only bound on the divisor
  if !(2..=32).contains(&self.froxel_xy_divisor) { ... }
  ```
  Trigger arithmetic:
  - Mesa ANV and lavapipe expose `maxImageDimension3D = 2048`, and RADV exposes the same value to my knowledge. All three expose `maxImageDimension2D = 16384`.
  - The default divisor 8 is safe only because `8 × 2048 = 16384`, the largest render width the 2D guard admits.
  - With `--froxel-xy-divisor 2` (`byroredux/src/cli_args.rs:161`), any render width above 4096 (for example 5K native/TAA) yields a width above 2048.
  - With divisor 4, the same happens above 8192.
- **Impact**:
  - `vkCreateImage` with an extent over the image-format limit is invalid usage (VUID-VkImageCreateInfo-extent-02253), which is undefined behaviour. In practice the driver either returns an error or crashes.
  - If it returns an error:
    - At boot, `VolumetricsPipeline::new` fails and `VulkanContext::new` refuses to build composite ("Volumetric pipeline failed to initialize"), so the engine will not start.
    - On a resize or upscaler switch, `recreate_bloom_and_volumetrics` returns an error and the app exits.
  - The failure message names no limit, so the cause is opaque to the user.
- **Trigger Conditions**: a non-default `--froxel-xy-divisor` below 8, a render width or height above `divisor × maxImageDimension3D`, and a device whose 3D limit is below its 2D limit (ANV, lavapipe, RADV).
- **Related**:
  - The 2D sibling guard is `FrameExtentSet::for_output`.
  - #3839 moved the BLAS budget with the render extent, but it assumes the grid fits.
- **Suggested Fix**: Read `max_image_dimension3_d` alongside the 2D limit and check it in `VolumetricsPipeline::new` (or in `froxel_extent`). Either raise the effective divisor to `ceil(render / max3d)` with a warning, or fail with a message that names the limit. Pin the "default divisor × 2048 ≥ max 2D render" invariant with a test.

### SAFE-D7-2026-09-23-01: The raw V-buffer temporal history has no non-finite guard: one NaN/Inf froxel self-perpetuates and spreads until a history reset
- **Severity**: MEDIUM (a defense-in-depth gap). No producer has been demonstrated on reference hardware, but the blast radius once one occurs is large.
- **Dimension**: GPU-Fed Data & Shader Loop Bounds (NaN/Inf)
- **Location**:
  - `crates/renderer/shaders/volumetrics_inject.comp:2891-2954`, where `current` is assembled, blended with history and stored.
  - `volumetrics_inject.comp:1411-1430` (`sampleHistoryColumn`).
  - `volumetrics_integrate.comp:61-94`.
  - `composite.frag:187-243` and `:622-627`.
- **Status**: NEW. This is the same class as the closed #903 (TAA history) and SVGF's history guard, applied to a history buffer those fixes did not cover.
- **Description**:
  - Within this same shader, the transported chemistry, dynamics and optical fields are scrubbed on every read and write: `sanitizedChemistry`, `sanitizedDynamics` and `sanitizedOptical` at `:1455-1512`, `:2091-2093` and `:2362-2370`.
  - TAA (`taa.comp:268`) and SVGF (`svgf_temporal.comp:171`) reject non-finite history for the same reason.
  - The raw V-buffer is the exception. Neither `current` nor the reprojected `history` (binding 6 / `lighting_volumes`) is ever checked before `current = mix(current, history, historyWeight)` and `imageStore(froxel, coord, current)`.
- **Evidence**: why the poisoning holds under any GLSL `min`/`max` NaN semantics.
  - If `history.rgb` is NaN, then `relativeRadianceDelta` is NaN (`dot(abs(NaN - c))`, `:2942`), and `emissionAgreement = exp(-2.5 * NaN * frac)` is NaN even when `frac == 0`. That makes `historyWeight` NaN, so `mix(...)` returns NaN and the texel is re-stored as NaN every frame.
  - If `history.a` is NaN, `relativeDensityDelta` (`:2916`) is NaN, with the same result.
  - `sampleHistoryColumn` blends `z0` and `z1` linearly (`:1425-1429`). NaN in either tap gives a NaN result (`NaN * 0 = NaN`), so the poison spreads one slice per frame along the column in both directions. Camera reprojection spreads it sideways.
  - Integration then carries it to every deeper slice of the column (`inscatter_total += NaN`).
  - Composite blends a 2×2 set of columns with no guard (`accumulated += columnValue * weight`, `combined = combined * vol.a + vol.rgb`), so NaN reaches the linear-HDR scene.
  - That scene feeds the bloom pyramid, whose bright pass has no guard (`bloom_downsample.comp:83-88`). Downsample and upsample smear it over a large screen area every frame.
  - Recovery comes only from `signal_history_reset` or `record_neutral_frame`: a cell transition or a skip streak.
  - Candidate producers, none confirmed on the reference 4070 Ti:
    - fp16 overflow of `current.rgb` above 65504. The target is RGBA16F. The worst case is dense transported soot (σ up to 0.2/BU plus multiscatter gain) next to a bright combustion surface light, whose inverse-square falloff reaches up to 2500× at the 0.02 m source-radius floor (`froxelLightAtten`, `:1250-1259`).
    - The implementation-defined NaN results of GLSL `max`/`clamp` on the `max(x, 0.0)` guards.
    - `normalize(vec3(0))` for `view_dir` when `jitter.z` is exactly 0 on slice 0, so `t == 0` and `world_pos == camera_pos`. This is reachable at large frame counts, but most drivers' IEEE `max` then scrubs it.
- **Impact**: a persistent screen-space corruption: black or blown patches that grow via bloom until the next history reset. The #2736 image-health counter (`presentation.frag:199`) would count it, but nothing corrects it. The damage is visual only, with no memory unsafety.
- **Related**: #903 (TAA NaN history), #2736 (non-finite pixel counter), #1021 (HG g clamp), #2241 (slab saturation). The missing composite and bloom guards are renderer-owned and outside this leg's scope.
- **Suggested Fix**:
  - In `volumetrics_inject.comp`, replace a non-finite `history` with `current` before the blend, the same way `taa.comp` does. Clamp `current` to finite fp16 range (for example `min(rgb, 6.0e4)`) and zero it if it is non-finite before `imageStore`.
  - Optionally, add a single finite check on `vol` in `sampleVolumetricColumn`.

### SAFE-D6-2026-09-23-01: `VolumetricsParams` has only a block-size pin; a swap of two `vec4` members passes every test
- **Severity**: LOW (a test-coverage gap; the Rust and GLSL orders match today)
- **Dimension**: GPU Struct Layout Soundness
- **Location**:
  - `crates/renderer/src/vulkan/volumetrics.rs:68-153` (Rust struct)
  - `crates/renderer/shaders/volumetrics_inject.comp:43-101` (GLSL block)
  - `crates/renderer/src/vulkan/reflect.rs:662-689` (the only pin)
- **Status**: NEW
- **Description**:
  - `volumetrics_ubo_sizes_match_host_structs_in_every_shader` (#1493) checks only the reflected std140 block size (336 B).
  - `VolumetricsParams` is 2 `mat4` followed by 13 `vec4`s. Any swap of two `vec4` members on either side, for example `fog_tint` ↔ `temporal_params`, keeps the size and passes.
  - The sibling `GpuFogVolume` received a field-order lockstep test under #2228 (`gpu_fog_volume_glsl_field_order_matches_rust_struct`). The mirrors fixed under #3982 got the same. This UBO, which has the most members and the densest lane overloading (`render_origin.w` = is_exterior, `sun_color.a` = cluster far, `fog_reference.w` = simulation dt, `wind_gust.y` = BFECC switch), never did.
  - I compared the two declarations by hand at HEAD and they match.
- **Impact**: none today. A future reorder would silently feed wrong parameters, such as the wrong extinction, dt or cluster basis, into every froxel.
- **Related**: #1493, #2228, #3982.
- **Suggested Fix**: Add a source-scan field-order test, modelled on `gpu_fog_volume_glsl_field_order_matches_rust_struct`, that compares the GLSL `VolumetricsParams` block's member names with the Rust field order.

### Existing #4599 (MEDIUM, open): `.expect()` on a poisoned allocator lock is reachable from teardown
`VolumetricsPipeline::destroy` (`volumetrics.rs:1821-1848`) calls `GpuBuffer::destroy` for six buffer Vecs, and `GpuBuffer::destroy` still calls `.expect("allocator lock poisoned")` (`buffer.rs:1471-1475`). This is the same finding as #4599 and is not re-filed.

## Per-dimension detail

- **Dim 1 — FFI**: out of suite scope. Volumetrics has no FFI crossing.
- **Dim 2 — Memory corruption / UB**: clean.
  - The padding-free proofs for the 5 `NoUninit` impls hold:
    - `VolumetricsParams`: 336 B;
    - `GpuFogVolume`: 96 B;
    - `GpuFogVolumeUpload`: 12,304 B;
    - `GpuFogClusterEntry`: 8 B;
    - `IntegrationParams`: 16 B.
  - The moment readback decodes through safe `from_ne_bytes` behind an `ensure!` length check (`:1454`).
  - The CPU cluster build is bounded on every axis, and NaN/Inf inputs degrade through saturating casts.
  - Upload prefixes are bounded by `COUNT` and `INDEX_COUNT`.
  - The empty `write_mapped` when `write_hi == 0` cannot produce a zero-size flush: gpu-allocator 0.28 requires `HOST_COHERENT` for `CpuToGpu` and `GpuToCpu`, so the non-coherent branch is dead for these buffers.
- **Dim 3 — Leaks / drop order**: clean, apart from Existing #4599.
  - `destroy` frees every handle: 6×FIF froxel slots plus 2 noise volumes, 6 buffer Vecs, 2 pipelines, 2 layouts, 2 set layouts, 2 pools and 3 samplers.
  - The `try_or_cleanup!` rollback covers construction. Two rollback holes exist, where a freshly created `GpuBuffer` is not yet owned by `partial` (`init.rs:316-326` and `:675-684`). Both are unreachable in practice and are caught by the `GpuBuffer` Drop safety net (#656), so they are not reported.
  - Resize destroys the old pipeline under device idle before building the new one. A failure there is fatal on both callers, and the #1211 empty-framebuffer guard prevents a draw with the stale binding 6.
  - Teardown destroys the pipeline after composite and before the allocator `Arc::try_unwrap`.
  - There is no per-frame CPU growth.
- **Dim 4 — Unsafe discipline**: clean.
  - Each stated precondition was checked at its call site and holds:
    - `dispatch`, `record_neutral_frame` and the three `write_*` descriptor updates run only from `record_volumetrics_pass`, after the `in_flight[frame]` fence wait (`draw.rs:1812` → `:2033`).
    - `destroy` runs only on paths with nothing in flight.
  - Observation, not filed: `write_tlas`, `write_lights_and_clusters` and `write_boundary_geometry` are safe `pub fn`s whose soundness depends on the caller's fence discipline. This is a renderer-wide convention (caustic and scene_buffers use it too), not specific to volumetrics.
- **Dim 5 — Vulkan spec**: 1 finding (SAFE-D5-2026-09-23-01).
  - The full barrier chain was verified: HOST→COMPUTE; history READ→WRITE and WRITE→READ per field; inject→integrate; `pre_int_write` including `TRANSFER_WRITE` (#3647); integrate→FRAGMENT; COMPUTE→HOST for the binding-18 atomics.
  - The NONE-src sync1 barrier is legal because synchronization2 is required and enabled.
  - Layouts match the descriptors.
  - The AS build→COMPUTE barrier runs on both build arms.
  - Dispatch counts and in-shader bounds guards agree.
  - Every `rayQueryInitializeEXT` has `tMin ≤ tMax` and finite directions from finite inputs.
  - The lavapipe CI lane was not re-verified here (see #4596).
- **Dim 6 — GPU struct layout**: 1 LOW finding (SAFE-D6-2026-09-23-01). All existing pins are green, and the committed SPIR-V is byte-identical to a fresh compile.
- **Dim 7 — GPU-fed data / loop bounds**: 1 MEDIUM finding (SAFE-D7-2026-09-23-01).
  - Every shader loop is constant-bounded, and every rayQuery loop uses Opaque plus TerminateOnFirstHit.
  - Every SSBO and image index is clamped or `length()`-checked: clusters, light indices, fog clusters and indices, fog volumes, boundary instance/vertex/index data, moment bins, blue noise, and history `texelFetch`.
  - CPU fog volumes pass `is_renderable`, the finite-geometry gate and `sanitize_emission`.
  - Not filed: per-bin `u32` moment accumulation could wrap in principle. In practice physical emission × volume keeps it far below that, and the host decode is clamp-safe, so the effect would be visual only.
- **Dim 8 — Mod runtime**: out of suite scope.

## Next step

`/audit-publish docs/audits/AUDIT_SAFETY_2026-09-23.md`
