# #4957: REN-D3-2026-09-27-06: `GcCameraUBO` (`groundcover_blade.vert`) is a name-diverging prefix mirror of `GpuCamera` pinned by block size only, outside both discovery legs

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4957
- **Labels**: low,renderer,shaders,terrain-exterior,test-gap,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D3-2026-09-27-06**._

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (mirror coverage)
- **Location**: `crates/renderer/shaders/groundcover_blade.vert:210` (`layout(set = 1, binding = 1) uniform GcCameraUBO { mat4 gcViewProj; … vec4 gcJitter; }`); guard `crates/renderer/src/vulkan/groundcover.rs:2237` (`blades_project_with_the_jittered_camera_ubo`)
- **Status**: NEW
- **Description**: The blade vertex shader binds the scene camera UBO (set 1 binding 1) under a different block name, declaring the first 8 `GpuCamera` members (through `jitter`) with `gc`-prefixed names.
  - The only pin is `uniform_block_size_by_name(spv, "GcCameraUBO") == offset_of!(GpuCamera, jitter) + 16`, a size check.
  - `camera_ubo_glsl_copies_stay_in_lockstep`'s discovery leg matches only `uniform CameraUBO {`, and `every_shader_struct_is_classified` walks only `struct` lines. Neither can see this block.
  - A transposition among the same-typed `mat4` lanes (`gcViewProj`/`gcPrevViewProj`/`gcInvViewProj`) or `vec4` lanes would keep the size and pass. This is the same size-only class as #4778 (`VolumetricsParams`) and #3684 (`CameraUBO` before its order test).
- **Evidence**: `grep -rn 'uniform .*Camera' crates/renderer/shaders` shows 5 `CameraUBO` blocks plus `GcCameraUBO`. The test source lists the 5 `CameraUBO` files only.
- **Impact**: A field reorder in `GpuCamera`'s first 8 lanes, or a GLSL-side edit, would silently project ground-cover blades with the wrong matrix (for example the previous-frame VP, which gives wrong motion vectors and TAA smear) with every guard green.
- **Related**: #3684, #4028, #4778, #4296.
- **Suggested Fix**: Add `GcCameraUBO` to the camera lockstep test as a prefix mirror. Compare member names, allowing `gc` prefixes through `NameAlias`, plus order and type against the first 8 `GpuCamera` fields. Make the discovery leg match any `uniform …` at `set = 1, binding = 1`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
