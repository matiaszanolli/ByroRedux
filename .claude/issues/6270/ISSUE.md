# #6270: SAFE-2026-10-21-01: Push-constant blocks outside ground cover are still hand-cast and not mirror-guarded; #6033 closed without the shadow-filter `Push`

**Labels**: low,safety,renderer,shaders,test-gap,bug

**Source**: docs/audits/AUDIT_SAFETY_2026-10-21.md · **Severity**: LOW · **Dimension**: Dim 2 (`NoUninit` byte views) / Dim 4 (does the stated invariant stay true). · **Location**: push-constant byte views at `crates/renderer/src/vulkan/{shadow_filter.rs:382,svgf.rs:1246,presentation.rs:646,water.rs:789,skin_compute.rs:799,1262}`; mirror walk `every_shader_struct_is_classified` (`crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`)

- **Severity**: LOW. Every remaining cast is sound today. The gap is in tracking and guarding, not a live defect.
- **Dimension**: Dim 2 (`NoUninit` byte views) / Dim 4 (does the stated invariant stay true).
- **Location**: push-constant byte views at:
  - `crates/renderer/src/vulkan/shadow_filter.rs:382`
  - `crates/renderer/src/vulkan/svgf.rs:1246`
  - `crates/renderer/src/vulkan/presentation.rs:646`
  - `crates/renderer/src/vulkan/water.rs:789`
  - `crates/renderer/src/vulkan/skin_compute.rs:799` and `:1262`

  Bulk byte views at `crates/renderer/src/vulkan/morph_compute.rs:262`, `scene_buffer/upload.rs:750`, `mesh/geometry_ssbo.rs:690` / `:720` and `mesh.rs:1293` / `:1302`. The mirror walk is `every_shader_struct_is_classified` in `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:2117`.
- **Status**: NEW. This was a counted-out note in `AUDIT_SAFETY_2026-10-19.md` ("Add `Push` to #6033's mirror-table / `NoUninit` routing scope; no separate issue"). #6033 (`02b828fff`) then closed, routing only the three ground-cover blocks through `byte_view` and the mirror table, so the residue is now tracked nowhere. #5122 (closed) was the ground-cover-models instance. #3564 (closed) made the guards drive off a walk instead of a list, but that walk only sees `struct` declarations.
- **Description**: `byte_view` (`vulkan/buffer.rs:42-56`) documents itself as "the single place" byte views are taken, with one listed exemption (`hash_indirect_slice`). Twelve `std::slice::from_raw_parts` casts still bypass it. The six push-constant views are the ones that matter:
  - The structs are `#[repr(C)]`, all-scalar and padding-free: `Push` {`[u32;2]`, `u32`, `u32`}, `AtrousPush`, `PresentationPushConstants`, `WaterPush`, `SkinPushConstants`, `SkinPalettePushConstants`. So each cast is sound, and each `// SAFETY:` comment is true today (the clippy gate confirms the comments are present).
  - None of them has a `NoUninit` impl. A field later added at a different alignment would make the cast read uninitialised padding, and nothing would flag it.
  - Their GLSL counterparts are `layout(push_constant) uniform …` blocks. The classification walk matches only lines starting with `struct `, so those blocks are invisible to it.
  - A reorder or a `uint`↔`float` swap therefore reaches the GPU with a green suite. That is the exact hazard #6033 closed for ground cover, still open for the shadow filter, à-trous, presentation, water and skinning blocks.

  Push-range sizes are not at risk: every layout's range is `size_of::<T>()`, the same value the cast uses.
- **Evidence**: `shadow_filter.rs:49-53` (`struct Push { screen: [u32; 2], step: u32, flags: u32 }`, no `NoUninit` impl) and `:382-385` (`from_raw_parts((&push as *const Push).cast::<u8>(), size_of::<Push>())`). `shader_contract_tests.rs:2266-2275` (`line.strip_prefix("struct ")` is the only declaration matcher). A grep finds no `unsafe impl NoUninit for Push` / `AtrousPush` / `WaterPush` / `PresentationPushConstants` / `Skin*PushConstants`.
- **Impact**: None today. The value of fixing it is guard coverage: per-field corruption in a denoiser, presentation or skinning pass would show up as a visual regression rather than a test failure.
- **Related**: #6033, #5122, #3564, #4956. The two `geometry_ssbo.rs` casts say "mirrors `create_device_local_buffer`'s identical cast", which already requires `Vertex: NoUninit`, so they can route through `byte_view` with no new impl.
- **Suggested Fix**: Add `unsafe impl NoUninit` for the six push structs and replace the casts with `byte_view(std::slice::from_ref(&push))`, as #6033 did. Extend the classification walk to `layout(push_constant) uniform <Name>` blocks, putting each in `name_diverging_glsl_rust_mirrors_stay_in_lockstep` or marking it `ShaderLocal`. Then re-tighten `byte_view`'s "single place" doc to list the remaining bulk-slice exemptions.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_SAFETY_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
