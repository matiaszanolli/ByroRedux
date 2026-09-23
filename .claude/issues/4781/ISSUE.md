# #4781: SAFE-D5-2026-09-23-01: Froxel XY extent is never checked against `maxImageDimension3D`, and `--froxel-xy-divisor` < 8 can create an over-limit 3D image

**Severity**: HIGH
**Labels**: high, safety, renderer, vulkan, bug
**Source**: docs/audits/AUDIT_SAFETY_2026-09-23.md (SAFE-D5-2026-09-23-01)

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

**Source**: `docs/audits/AUDIT_SAFETY_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
