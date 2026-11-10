**HEAD**: dbc815c26 · **Baseline**: `docs/audits/AUDIT_SAFETY_2026-10-31.md` (@ `670f81917`, 544 commits ago) · **Audited**: Dims 3 (leak/partial-init/teardown), 4 (unsafe truth), 5 (Vulkan spec), 6 (GPU struct contracts), 7 (GPU-fed data) over `crates/renderer/src/vulkan/volumetrics.rs`, `vulkan/volumetrics/{init,local_grid,noise,fog_clusters,combustion}.rs`, `shaders/volumetrics_{inject,integrate,local_sim}.comp`, `shaders/include/{volumetrics_bindings,local_grid_params}.glsl`, and the seams (`context/{post_passes,resize,init,init_stages,teardown,draw}.rs`, `composite.rs`, `buffer.rs` mapped-write helpers) · **Unchanged since baseline (skimmed)**: Dim 1 (no commits on `fsr3-sys`, `cxx-bridge`, `frame_upscaler.rs`, `preflight.rs`; `crates/ui` has one safe-Rust `navigator.rs` edit), Dim 2 (`Cargo.toml` diff is `half`/`image` features only, `lz4_flex` pin intact; no diff on `nif/stream.rs`, `pex/opcode.rs`, `core/query.rs`), Dim 8 (no `mod-runtime` change; `cargo tree -p byroredux-mod-runtime | grep -ic wasi` = 0)

# Safety Audit: volumetrics surface (suite `volumetrics-deep`)

**Command**: `/audit-safety`, run inside the 2026-11-09 `volumetrics-deep` suite
**Severity scale**: `.claude/commands/_audit-severity.md`

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 (Existing: REN-D8-06) |
| MEDIUM | 0 |
| LOW | 2 |

- **Premise correction.** `SUITE_RULES.md` says the volumetrics paths have zero commits since 2026-10-31. That is wrong. The baseline HEAD `670f81917` is dated 2026-10-31 15:49, and 32 commits on the volumetrics paths follow it (2026-11-01 to 2026-11-08), plus 29 on the seam files. The largest is the new world-space combustion local grid: `volumetrics/local_grid.rs` (1726 lines, 15 `unsafe {}` blocks, 5 `unsafe fn`, 1 `unsafe impl NoUninit`), `volumetrics_local_sim.comp`, `combustion_phase_bake.comp`, inject set 0 bindings 26..=32, and composite bindings 12..=17 (with a second composite pipeline). The baseline audit (Dim 4/5, whole `vulkan/` tree) never saw this code, so this report treats it as new surface.
- **Result.** One HIGH (a duplicate of sibling REN-D8-06, a #4781 regression), two NEW LOW. Everything else on the new unsafe surface checked out: the `SAFETY` claims are true, the barriers are complete, the cleanup paths are sound and the shader/host struct contracts hold.

## Method

- **Delta.** `git log 670f81917..HEAD -- <volumetrics paths>` gives 32 commits. `git diff --stat` shows `volumetrics.rs` +330/-44, `init.rs` +56, `noise.rs` +73, `combustion.rs` and `fog_clusters.rs` small. Every changed region was read, and `local_grid.rs` and `init.rs` were read in full.
- **Process.** No sub-agents. No source was edited. No engine or GPU process was launched. Cargo used the repo `target/` with `CARGO_INCREMENTAL=0`. Scratch is `/tmp/audit/safety/`.
- **Commands run at HEAD:**
  - `cargo clippy -p byroredux-renderer --no-deps -- -D clippy::undocumented_unsafe_blocks`: **exit 0**.
  - `cargo test -p byroredux-renderer --lib`: **1627 passed, 0 failed, 1 ignored.** This includes the `GpuLocalGridParams` / `LocalSimPush` / `VolumetricsParams` contract tests, `validate_local_grid_layout`, and `every_created_resource_is_pushed_before_anything_else_runs`.
  - `bash scripts/check-shader-artifacts.sh`: all 40 shaders plus the opaque early-test variant match glslang. Every volumetrics `.spv` is fresh, so there is no stale-SPIR-V risk.
  - `cargo tree -p byroredux-mod-runtime | grep -ic wasi` = 0.
- **Dedup.** `/tmp/audit/issues.json` (104 open), `docs/audits/` (`AUDIT_RENDERER_2026-11-07.md` covers the local grid for ledger, barrier and perf angles only) and closed-issue search.

## Census (re-measured at dbc815c26)

| Scope | Tokens | `unsafe {` blocks | `unsafe fn` | `unsafe impl` | Delta vs 10-31 |
|---|---|---|---|---|---|
| `crates/renderer/src` | **933** (900) | **684** (669) | **117** (102) | **45** (44) | +33 tokens, +15 blocks, +15 `unsafe fn`, +1 impl |
| volumetrics files only | | `volumetrics.rs` 5 (4), `init.rs` 17, `local_grid.rs` **15 (new)**, `noise.rs` 1, `combustion.rs` 0, `fog_clusters.rs` 0 | `volumetrics.rs` 3, `init.rs` 1, `local_grid.rs` 5, `noise.rs` 1 | `volumetrics.rs` 2, `fog_clusters.rs` 3, `local_grid.rs` 1 (`GpuLocalGridParams`) | |
| `fsr3-sys` / `nif` / `core` / `pex` | | 10 / 1 / 6 / 1 | | | unchanged |
| every other crate | | No newcomer. The only non-renderer files with unsafe tokens are the known set: `byroredux/src/{app_events,cell_loader/unload}.rs`, `core`, `cxx-bridge`, `fsr3-sys`, `nif`, `pex`, `plugin/test_paths.rs`, `byro-launcher/preflight.rs`. `material_translate.rs` hits are prose. | | | |

The `+15 blocks` is `local_grid.rs` plus one `unsafe { grid.destroy }` in `VolumetricsPipeline::destroy`. Every new `unsafe {}` carries a `SAFETY:` comment (clippy gate green).

## Findings

### SAFE-2026-11-09-01: the local grid's irradiance image is twice the froxel width, so the #4781 `maxImageDimension3D` fit no longer protects it
- **Severity**: HIGH (Vulkan spec violation floor; narrow trigger)
- **Dimension**: 5 (Vulkan spec compliance) / 3 (partial-init)
- **Location**: `crates/renderer/src/vulkan/volumetrics/local_grid.rs:662-667` (`irradiance_extent`) and `:875-884` (creation); `crates/renderer/src/vulkan/volumetrics.rs:528-542` (`fit_froxel_divisor_to_device`); test `froxel_divisor_is_raised_to_fit_the_device_3d_limit`, `volumetrics.rs:3360-3420`
- **Status**: Existing: REN-D8-06 (`docs/audits/AUDIT_RENDERER_2026-11-09.md`, filed by the sibling renderer audit; Regression of #4781). Found independently here and kept for the spec/partial-init framing. Do not publish a second issue.
- **Description**: `fit_froxel_divisor_to_device` raises the XY divisor until `ceil(max(render.w, render.h) / divisor) <= maxImageDimension3D`, so the froxel grid fits. #6533 added `irradiance_extent(froxel)`, whose width is `froxel.width * 2` (radiance and mean light direction side by side, per `include/local_irradiance.glsl`). It is created as a 3D image through `GpuImage::create`, which does not clamp or check limits. Whenever `froxel.width > maxImageDimension3D / 2`, `vkCreateImage` receives `extent.width > maxImageDimension3D`, a VkImageCreateInfo valid-usage violation (3D extent must not exceed the device limit).
- **Evidence**:
  - `irradiance_extent`: `vk::Extent3D { width: froxel.width * 2, ..froxel }`.
  - `LocalGrid::build`: `GpuImage::create(.., &GpuImageDesc::color_3d(.., irradiance_extent(self.froxel_extent), FROXEL_FORMAT, usage))`.
  - The fit test asserts only the froxel extent, e.g. a 5120x2880 render with divisor 2 gives divisor 3 and a 1707-wide froxel. The irradiance image is then 3414 wide, over Mesa's 2048 (the figure the test and the #4781 text use).
  - The test comment "default divisor x 2048 must cover a 16384-wide render" is false for the irradiance image: 16384/8 = 2048 froxels gives a 4096-wide image.
  - Concrete triggers on a 2048-limit device (Mesa ANV, lavapipe): `--froxel-xy-divisor 2` at 3840x2160 (froxel 1920, irradiance 3840), or the default divisor with a render wider than 8192. A 16384-limit device (the NVIDIA dev card) cannot reach it, which is why no run saw it.
- **Impact**: `LocalGrid::build` returns `Err`, so `VolumetricsPipeline::new` fails. At boot the context cannot be built, because composite hard-requires both the froxel views and `local_fire_views_required(..)?`. On a resize the whole `recreate_swapchain` fails, and the app treats that as fatal. It is a clean failure rather than memory corruption, but a driver that does not validate the extent is undefined behaviour at `vkCreateImage`. This is the same failure class #4781 closed (graded HIGH then).
- **Related**: #4781 (closed, same class), #6533, REN-D8-01.
- **Suggested Fix**: fit with the doubled axis, i.e. `needed = ceil(max(2 * render.w, render.h) / max3D)`, or lay the direction half out along Y or Z instead of X. Add a unit test over `irradiance_extent(froxel_extent(..))` for the same device-limit matrix the existing test walks (2048 and 16384, divisors 2..=32).

### SAFE-2026-11-09-02: host workgroup sizes for the local-grid compute passes are unpinned literals
- **Severity**: LOW
- **Dimension**: 7 (GPU-fed data) / test gap
- **Location**: `volumetrics/local_grid.rs:52` (`PHASE_BAKE_WORKGROUP = 64`), `:66` (`LOCAL_SIM_WORKGROUP = 4`), `:989-993`, `:1243-1264`; shaders `volumetrics_local_sim.comp:35` (`local_size_x = 4, ..`), `combustion_phase_bake.comp` (`local_size_x = 64`, `texel.y >= 2u`)
- **Status**: NEW (the class was #5620 / #2768, both CLOSED, and was cured by generated `WORKGROUP_*` constants. These two shaders re-introduced bare literals.)
- **Description**: The froxel passes take `WORKGROUP_X/Y/Z` from generated `shader_constants.glsl` and divide by the same Rust constants. The local sim and the phase bake instead hard-code `local_size` in GLSL and re-type it as private Rust constants (`groups = LOCAL_GRID_CELLS / LOCAL_SIM_WORKGROUP`, `PHASE_TABLE_ROWS` for the Y count). No test links them. The reflection test pins descriptor bindings, not `local_size`. `push_block_in_the_shader_matches_the_encoder` pins only the push block.
- **Evidence**: grep for `LOCAL_SIM_WORKGROUP` and `PHASE_BAKE_WORKGROUP` shows only the definitions and the two dispatch sites. `shader_constants.rs` has no entry for either.
- **Impact**: Drift is not memory-unsafe, because the shader bounds-checks `global.x/y >= dim` and `box >= push.info.x` and returns. It shows as an under-covered dispatch: part of an atlas slab or the phase table silently never written (stale or cleared zeros), which looks like a rendering bug.
- **Related**: #5620, #2768.
- **Suggested Fix**: emit `LOCAL_SIM_WORKGROUP` and `PHASE_BAKE_WORKGROUP` through `shader_constants_data.rs` and use them in both places, or add a SPIR-V reflection assertion on the `LocalSize` execution mode.

### SAFE-2026-11-09-03: composite's two-pipeline create discards a successfully built sibling when the other fails
- **Severity**: LOW
- **Dimension**: 3 (partial-init cleanup)
- **Location**: `crates/renderer/src/vulkan/composite.rs:1130-1146` (`Err((_, e))` arm). The same shape exists in `presentation.rs:521` and `pipeline.rs:568`/`:942`, and in `water.rs:1085` via `.map_err(|(_, err)| err)`.
- **Status**: NEW (the pair form is #6748, `15a4ecf10`; the discard shape is renderer-wide)
- **Description**: `create_graphics_pipelines(cache, &[sunlit, sunless], None)` builds two pipelines in one call. The Vulkan spec lets a multi-pipeline create report an error while still returning valid handles for the entries that succeeded. Only the failed entries are `VK_NULL_HANDLE`, unless `EARLY_RETURN_ON_FAILURE` is set. ash returns the `Vec<Pipeline>` in the `Err` tuple. The arm throws it away (`Err((_, e))`) and calls `partial.destroy`, which only knows `pipeline` and `pipeline_sunless`, both still null. A pipeline that did build is leaked. Composite was a single-info call before #6748, so this is a new exposure at this site.
- **Evidence**: `Err((_, e)) => { unsafe { partial.destroy(device, allocator) }; return Err(..) }`, with the handles only assigned in the `Ok` arm.
- **Impact**: One leaked `VkPipeline` on an init path that already ends in `Err`, which is fatal for the context. Low. A repeat of this on resize failure is fatal as well. The leak does not compound per frame.
- **Related**: none.
- **Suggested Fix**: in the `Err((pipelines, e))` arm, assign any non-null element to the `partial` fields before `partial.destroy`, or build the two variants with two single-info calls. Apply the same pattern to the other multi-info call sites if the owner wants the class closed.

## Dimension results

### Dim 3: leaks, partial-init, drop order (PASS apart from SAFE-03)
- **`LocalGrid::new` / `build` rollback.**
  - Every created resource is pushed into its `Vec` or `Option` before anything fallible touches it. The `stats` and `params` buffers are pushed before their zeroing `write_mapped` (`let zeroed = ..; push; zeroed?;`), the same shape as #5946. Samplers, pipelines and layouts are null-initialised.
  - `LocalGrid::destroy` null-guards every handle and drains every `Vec`. On `Err`, `new` calls it once, and `VolumetricsPipeline::new_inner` then runs `try_or_cleanup!` on a `partial` whose `local_grid` is still `None`, so there is no double destroy.
  - The `.context("local grid ..")?` arms return `Err` instead of panicking. The older `.expect("volumetrics descriptor layout drifted ..")` calls in `init.rs:539/748` still panic with resources live. That is the deliberate #427 test-guarded policy, not a finding.
- **Teardown order.**
  - `VolumetricsPipeline::destroy` takes `local_grid`, destroys it first, then the froxel slots, buffers, pipelines and pools. `teardown.rs` destroys composite before volumetrics.
  - Resize destroys the old pipeline with the device idle and rebuilds. `recreate_composite_and_egui` rewrites composite bindings 12..=17 from `local_fire_views()`, and a missing grid is an `Err`. The `post.volumetrics = None` window after a failed rebuild leaves composite dangling by design (#905), and the app treats the resulting `Err` as fatal (`app_events.rs:1315`, `app_frame.rs:934`).
- **Deliberate leaks.** `initialize_layouts` failure in both `init_stages.rs` and `resize.rs` uses `OneTimeCommandError::may_be_in_flight` to choose between `mem::forget` and destroy (#5947, unchanged). The local-grid images and the phase table ride the same path.
- **Host-visible memory.**
  - The stats buffer is `create_host_readback`. `begin_frame` runs `invalidate_if_needed`, reads, zeroes and `flush_if_needed` in the right order. `bytes.chunks_exact(4)` zips against a fixed 16-word array, so a larger mapped slice cannot overrun.
  - The `GpuLocalGridParams` UBO is sized from `size_of::<GpuLocalGridParams>()` (144 B). `write_mapped_bytes` caps the copy at the mapping length. The mapped-flush superset-range reasoning (#2683) is untouched.
- **CPU growth.** `LocalBoxSet.sources` is capacity-only retained (`#6753`), and `update` clears it before use and after.

### Dim 4: unsafe truth (PASS)
Every new block's stated invariant was re-checked against its call site.
- `local_grid.rs`
  - `record_phase_bake`: called in `initialize_layouts` after the UNDEFINED to GENERAL barrier and the clear, then a TRANSFER to COMPUTE `memory_barrier`. It binds only set 1. The bake shader declares nothing in set 0, so set 0 never being bound is valid.
  - `record_sim`: the brick `cmd_fill_buffer` is the first command, uses an 8192 B size (multiple of 4) on a `TRANSFER_DST` buffer, and is followed by TRANSFER to COMPUTE. The history barriers use distinct images (`frame != previous`). Pipeline layout, push range (80 B, within the 128 B minimum) and sets are compatible, and set 0 is the inject layout object the pipeline layout was built from.
  - `write_inject_bindings`: the claim "freshly written, not yet submitted" holds, because it is called only from `new_inner` before any frame.
  - `destroy`: single null-guarded destroys.
- `volumetrics.rs`
  - `dispatch`, `record_neutral_frame` and `destroy` keep valid `# Safety` text. `write_tlas`, `write_lights_and_clusters` and `write_boundary_geometry` update the slot's inject set only after the all-slots fence wait, which is what makes the "not in use by an in-flight frame" claim true.
- `init.rs`
  - The 17 blocks are unchanged in meaning. The new `initialize_layouts` barrier set includes the local-grid images (`self.local_grid.iter().flat_map(|g| g.images())`) in both the barrier loop and the clear loop.
- `fog_clusters.rs`, `volumetrics.rs`, `local_grid.rs`: the `unsafe impl NoUninit` set (`GpuFogVolume` 96 B, `GpuFogVolumeUpload`, `GpuFogClusterEntry`, `VolumetricsParams`, `IntegrationParams`, `GpuLocalGridParams`) has no padding. `GpuLocalGridParams` is 4 vec4 plus uvec4 plus 4 vec4 = 144 B, matching the std140 block in `local_grid_params.glsl`.
- `unsafe fn` bodies: edition 2021, so `unsafe_op_in_unsafe_fn` is not in force. The `dispatch` body issues raw `cmd_*` calls under the fn-level `# Safety` contract. That is consistent with the rest of the renderer and not a finding.

### Dim 5: Vulkan spec (PASS apart from SAFE-01)
- **Hazards checked.**
  - Bricks: written by the sim (COMPUTE), published with COMPUTE to FRAGMENT `SHADER_WRITE` to `SHADER_READ` (`record_publish_bricks`), read by composite.
  - Atlas (state, optical) and irradiance: the final COMPUTE to FRAGMENT image barriers include all three (`composite_barriers`). The inject read of this frame's atlas follows the sim's COMPUTE to COMPUTE publish barrier.
  - Stats: written by the sim atomics, then the closing COMPUTE to HOST `SHADER_WRITE` to `HOST_READ` global barrier covers them. The host drains after the slot fence and an invalidate.
  - Occupancy: unchanged (rider 15 in `sync.rs`).
- **Cross-submission WAR.** The sim's and fill's reads from frame N-2 (composite fragment) are not in the in-cmd barrier source scope. They rest on the top-of-frame all-slots fence wait, the engine's documented contract and the same basis as the existing froxel history barriers. The local grid's resources are slot-local, so none needs to join the `sync.rs` rider list. A syncval run (not available here) would be the proof. No finding is claimed.
- **Layout and format.** Every new image stays in GENERAL (the init barrier moves all of them). RGBA16F supports `STORAGE`, `SAMPLED` and linear filtering on every device. The `TRANSFER_DST` usage covers the init clear. The atlas depth is 64 x 4 = 256, which equals the guaranteed 3D minimum, so it fits any device. Dispatch counts are far below the limits: 16 x 16 x 64 groups for the sim and 8 x 2 for the bake.
- **Pipeline layout.** The sim and bake share a layout of `[inject_set_layout, local set]` with an 80 B compute push range. The inject dispatch re-binds both of its sets, so the local set 1 does not leak into it.
- **Far-plane lockstep (#3611).** `volume_far_shader_constant_agrees_with_the_volumetrics_config_default` passes. `VOLUMETRIC_OUTPUT_CONSUMED` is `true`, so `vol.dispatch()` is live.

### Dim 6: GPU struct contracts (PASS)
- `VolumetricsParams` (2 mat4 + 13 vec4 = 336 B) matches `uniform VolumetricsParams` in `volumetrics_bindings.glsl` field for field. The guard is `composite_and_volumetrics_uniforms_match_rust_field_order`. Note: that test scans the whole `volumetrics.rs` with `include_str!` rather than `production_text`. That is a pre-existing weak scan and it happens not to be vacuous here.
- `GpuLocalGridParams` (144 B) is pinned against both consumers (#6742). `LocalSimPush` is pinned by `push_constants_encode_boxes_then_counts_in_shader_order` and `push_block_in_the_shader_matches_the_encoder`.
- `GpuFogVolume`, `FogClusterEntry`, `CombustionLightMoment` and `GpuBoundaryInstance` are all guarded. `every_shader_struct_is_classified` passes with `LocalSimPush` classified.
- Index bounds the GPU relies on, derived from the same constants on both sides:
  - Stats: `box * 4 + {0,1,2}` is at most 14 of 16 words, and the descriptor range is exactly 64 B.
  - Bricks: `box * 512 + idx` is below 2048 words, which is exactly `BRICK_BUFFER_BYTES` (8192 B).
  - Moments: bin index is clamped to 255, below `COMBUSTION_LIGHT_GRID_COUNT` = 256.

### Dim 7: GPU-fed data (PASS)
- Composite's `localFireMarch` loops are bounded by `maxSteps` (160, or 1280 when dense) and `LOCAL_FIRE_MAX_BRICK_JUMPS` (64). The brick index is clamped. Sim writes are bounds-guarded. A NaN `t` exits the loop.
- The host rejects non-finite source centres and radii and NaN camera distance in `LocalBoxSet::update`.
- The froxel V-buffer keeps its #4782 and #5957 finite clamps. The new irradiance store clamps radiance with `clamp(.., 0, 6e4)` and the history read already skips NaN. The one residual NaN route (`normalize` of a zero vector for a light exactly at a froxel centre) is measure-zero and was not reported.

## Sibling dedup (received after the draft)

- REN-D8-06 equals SAFE-2026-11-09-01 (above).
- REN-D8-07 (`heightFogOpticalDepth` overflow in `composite.frag`, from #6728) and REN-D4-02 are renderer findings. The safety angle of REN-D8-07 is Inf/NaN reaching the HDR target, which falls under Dim 7. I did not re-derive it and defer to the renderer report.
- CONC-2.1 (LOW PLAUSIBLE: the brick `cmd_fill_buffer` has no unconditional FRAGMENT to TRANSFER barrier) overlaps my Dim 5 note. I reached the same observation and judged it covered by the all-slots fence wait, as the SAFETY text says (`record_sim`); I did not claim a hazard. Both reports agree it needs syncval. Not a separate safety finding.

## Not verified here

- No validation-layer or syncval run (no GPU session was launched). The "safe" verdicts on barriers are source reads against the spec, not measured results.
- The `cargo test -p byroredux rapier_release ..` first steps for Dims 3 and 7 were not re-run. Their paths have no volumetrics involvement.
- Dim 1 beyond the diff check (the `fsr3-sys` call site) was not re-read.

## Next step

`/audit-publish docs/audits/AUDIT_SAFETY_2026-11-09.md`. SAFE-2026-11-09-01 is the one to fix.
