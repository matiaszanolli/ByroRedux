# REN-D8-06: the local-grid irradiance image is twice the froxel width, but `fit_froxel_divisor_to_device` only bounds the froxel — a 3D image over `maxImageDimension3D` is fatal at boot (regression of #4781)

Labels: high,renderer,vulkan,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: HIGH (Vulkan spec violation floor; narrow reach). **Status**: Regression of #4781 (closed 2026-09-23; its fix bounds the six froxel volumes, but the stage-1b irradiance field was added afterwards by #6533). Also found independently as SAFE-2026-11-09-01 in `docs/audits/AUDIT_SAFETY_2026-11-09.md`.

**Location**: `irradiance_extent` and `LocalGrid::build` (`crates/renderer/src/vulkan/volumetrics/local_grid.rs`); `fit_froxel_divisor_to_device` (`crates/renderer/src/vulkan/volumetrics.rs`); callers via `VolumetricsPipeline::new` (`volumetrics/init.rs`), `create_volumetrics` (`context/init_stages.rs`), `recreate_bloom_and_volumetrics` (`context/resize.rs`).

## Description
#4781 raises the XY divisor until `froxel_extent(..)` fits `maxImageDimension3D`. `irradiance_extent` is `width: froxel.width * 2` (radiance and mean light direction side by side, `include/local_irradiance.glsl`), height and depth unchanged. That image is created per frame in flight in `LocalGrid::build` through `GpuImage::create` with no limit check, and `fit_froxel_divisor_to_device` never sees it. A froxel width in `(limit/2, limit]` therefore passes the fit and yields an irradiance image over the limit (VUID-VkImageCreateInfo-extent-02254).

## Evidence
- `irradiance_extent`: `vk::Extent3D { width: froxel.width * 2, ..froxel }`.
- The only 3D-limit check is `fit_froxel_divisor_to_device` (`needed = max(w,h).div_ceil(max_image_dimension_3d)`); nothing in `local_grid.rs` checks a limit.
- `froxel_divisor_is_raised_to_fit_the_device_3d_limit` asserts only `froxel_extent(..).width <= MESA_MAX_3D`; its comment "default divisor x 2048 must cover a 16384-wide render" is false for the irradiance image (4096 wide).
- On a 2048-limit device (Mesa ANV / RADV / lavapipe, the limit #4781 targeted) the image is over the limit whenever `ceil(render_w / divisor) > 1024`: default divisor 8 above an 8192-wide render, divisor 4 above 4096, divisor 2 above 2048 (a 2560-wide render fails).

## Impact
Engine init treats a missing volumetrics pipeline as a hard error; a resize returns `Err`, which the app treats as fatal. The engine that booted before 2026-10-31 refuses to start with `--froxel-xy-divisor 2..4` at 1440p+ on a 2048-limit driver. NVIDIA (limit 16384, the dev card) and the default config at <= 8192-wide render are unaffected. Not run on such a device (CONFIRMED on the missing bound, PLAUSIBLE on the failure mode).

## Suggested Fix
Bound the doubled axis too: compute `needed` from `max(2 * render.w, render.h)` in `fit_froxel_divisor_to_device` (or lay the direction half along Y/Z). Extend the test to assert `irradiance_extent(froxel_extent(..)).width <= MESA_MAX_3D` for the same device-limit matrix (2048 and 16384, divisors 2..=32).

Related: #4781, #6533. Reports: `docs/audits/AUDIT_RENDERER_2026-11-09.md`, `docs/audits/AUDIT_SAFETY_2026-11-09.md`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every other image/buffer created from a derived froxel extent)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

