# Safety Audit — 2026-06-01

## Executive Summary

ByroRedux's safety posture is substantially sound across all twelve dimensions audited. No
CRITICAL findings were identified. One HIGH finding was found in the compute pipelines
dimension: the volumetrics pipeline survives a failed `initialize_layouts` call, leaving
froxel images in `VK_IMAGE_LAYOUT_UNDEFINED` — a latent VUID-vkCmdDispatch-None-04115
violation that is currently dormant behind the `VOLUMETRIC_OUTPUT_CONSUMED=false` gate but
will trigger the moment that gate is flipped. The remaining 58 findings span MEDIUM (14),
LOW (19), and INFO (25) severity levels, with the highest-density risk areas being the RT
pipeline, Vulkan spec compliance, memory management, and the NIFAL particle emission paths.

The most structurally significant medium-severity risks are: (1) `PipelineStageFlags::NONE`
used in sync1 barriers without a synchronization2 feature gate, (2) AS scratch-alignment
enforcement removed in release builds (debug_assert only), (3) TLAS resize relying on a
comment-documented rather than code-enforced double-fence-wait invariant, (4) the IOR
division in `triangle.frag` lacking a clamp guard against `ior = 0`, (5) `MATERIAL_KIND_GLASS`
being a local shader constant rather than an auto-generated value, (6) the volumetrics TLAS
latch not being reset per frame, and (7) particle emitter physical scalars propagating raw
NIF binary floats without finite/positive validation. The egui overlay integration also has
two medium-severity Vulkan API misuse patterns around command pool selection and queue Mutex
discipline.

The codebase's foundational safety infrastructure is verified clean: ECS query raw-pointer
caching is sound and documented (post-#1367), `read_pod_vec` bulk byte-casts are correct
and well-commented, `BuiltinType::from_u32` uses a fully checked match (no transmute), the
FLT_MAX sentinel is guarded at all three animation emission sites, NIF stream allocation
guards are comprehensive, cell unload GPU resource sweep is complete, Vulkan teardown order
is correct, `GpuMaterial` struct layout is pinned by tests, glass IOR Frisvad basis handles
the singularity correctly, and no unsafe `Send`/`Sync` impls exist anywhere in the codebase.

---

## Findings Table

| ID | Severity | Dimension | Title | File | Status |
|----|----------|-----------|-------|------|--------|
| NCPS-01 | HIGH | Compute Pipeline Safety | Volumetrics survives failed initialize_layouts — froxel images remain UNDEFINED | crates/renderer/src/vulkan/context/mod.rs:1801 | OPEN |
| SAFE-U1 | MEDIUM | Unsafe Rust | Stale doc comment claims `transmute` on `BuiltinType::from_u32` | crates/sfmaterial/src/types.rs:10 | OPEN |
| SAFE-U2 | MEDIUM | Unsafe Rust | `slice::from_raw_parts` on `WaterPush` has no SAFETY comment | crates/renderer/src/vulkan/water.rs:466 | OPEN |
| SAFE-U3 | MEDIUM | Unsafe Rust | Inner `unsafe` block in `debug_callback` missing SAFETY comment | crates/renderer/src/vulkan/debug.rs:50 | OPEN |
| SAFE-U4 | MEDIUM | Unsafe Rust | `CStr::from_ptr` in `check_validation_layer_support` has no SAFETY comment | crates/renderer/src/vulkan/instance.rs:71 | OPEN |
| SAFE-U5 | MEDIUM | Unsafe Rust | `upload_lights` SAFETY comment omits pointer-arithmetic in-bounds invariant | crates/renderer/src/vulkan/scene_buffer/upload.rs:41 | OPEN |
| SAFE-U6 | MEDIUM | Unsafe Rust | Pervasive absence of SAFETY comments on Vulkan-API `unsafe` blocks | crates/renderer/src/vulkan/:various | OPEN |
| VKC-001 | MEDIUM | Vulkan Spec | `PipelineStageFlags::NONE` used without synchronization2 feature gate | crates/renderer/src/vulkan/bloom.rs:399 | OPEN |
| VKC-002 | MEDIUM | Vulkan Spec | Scratch alignment violation is debug-only; release builds proceed silently | crates/renderer/src/vulkan/acceleration/mod.rs:243 | OPEN |
| VKC-003 | MEDIUM | Vulkan Spec | TLAS resize destroys live resources with no code-level enforcement of fence-wait | crates/renderer/src/vulkan/acceleration/tlas.rs:289 | OPEN |
| MEM-01 | MEDIUM | Memory Safety | NifImportRegistry unbounded by default — process-lifetime RAM growth | byroredux/src/cell_loader/nif_import_registry.rs:107 | OPEN |
| MEM-02 | MEDIUM | Memory Safety | MeshRegistry handle Vec has no u32 overflow guard | crates/renderer/src/mesh.rs:284 | OPEN |
| MEM-03 | MEDIUM | Memory Safety | GPU allocator Arc leak on Drop leaks device/surface/instance handles | crates/renderer/src/vulkan/context/mod.rs:2760 | OPEN |
| TS-02 | MEDIUM | Thread Safety | Global ABBA detector is opt-in and absent from CI | crates/core/src/ecs/lock_tracker.rs:217 | OPEN |
| NCPS-02 | MEDIUM | Compute Pipeline Safety | Volumetrics tlas_written latch never reset — debug_assert ineffective after first frame | crates/renderer/src/vulkan/volumetrics.rs:764 | OPEN |
| EGUI-01 | MEDIUM | Debug-UI Teardown | egui set_textures uses main draw command pool instead of transfer pool | crates/renderer/src/vulkan/context/draw.rs:2970 | OPEN |
| IOR-01 | MEDIUM | RT IOR-Refraction | GLASS_IOR division has no clamp guard before ETA computation | crates/renderer/shaders/triangle.frag:2147 | OPEN |
| IOR-02 | MEDIUM | RT IOR-Refraction | MATERIAL_KIND_GLASS is a local shader const — sync hazard | crates/renderer/shaders/triangle.frag:1948 | OPEN |
| NIFAL-S3 | MEDIUM | NIFAL Translation | extract_emitter_params passes NIF binary scalars with no finite/positive guard | crates/nif/src/import/walk/mod.rs:688 | OPEN |
| VKC-004 | LOW | Vulkan Spec | TLAS UPDATE primitiveCount mismatch guard is debug-only in release | crates/renderer/src/vulkan/acceleration/tlas.rs:568 | OPEN |
| VKC-005 | LOW | Vulkan Spec | Allocator Arc leak path skips device_wait_idle before early return | crates/renderer/src/vulkan/context/mod.rs:2787 | OPEN |
| MEM-04 | LOW | Memory Safety | BGSM material cache uses flush-on-overflow eviction | byroredux/src/asset_provider.rs:900 | OPEN |
| MEM-05 | LOW | Memory Safety | `read_pod_vec` safety relies on prose comment rather than type-system bound | crates/nif/src/stream.rs:311 | OPEN |
| MEM-06 | LOW | Memory Safety | Collision shape recursion depth unbounded for deeply-nested BhkListShape | crates/nif/src/import/collision.rs:261 | OPEN |
| TS-03 | LOW | Thread Safety | Systems with undeclared access generate unknown conflict pairs | crates/core/src/ecs/scheduler.rs:594 | OPEN |
| TS-06 | LOW | Thread Safety | SSAO OOM path fix guards against self-deadlock by comment only | crates/renderer/src/vulkan/ssao.rs:149 | OPEN |
| TS-08 | LOW | Thread Safety | Scheduler parallel panic policy leaves partial ECS state without recovery | crates/core/src/ecs/scheduler.rs:386 | OPEN |
| FFI-01 | LOW | FFI Safety | Exported Rust function `engine_info` declared but never called from C++ | crates/cxx-bridge/src/lib.rs:17 | OPEN |
| FFI-02 | LOW | FFI Safety | No test coverage for the cxx bridge | crates/cxx-bridge/src/lib.rs:1 | OPEN |
| RT-01 | MEDIUM | RT Pipeline | water.frag caustic floor-ray: no origin bias and mismatched tMin=0.001 | crates/renderer/shaders/water.frag:554 | OPEN |
| RT-02 | LOW | RT Pipeline | water.frag shadow-ray: N-bias (0.05) and tMin (0.001) are inconsistent | crates/renderer/shaders/water.frag:536 | OPEN |
| RT-03 | LOW | RT Pipeline | water.frag reflection/refraction rays: no origin bias on water surface | crates/renderer/shaders/water.frag:436 | OPEN |
| RT-04 | LOW | RT Pipeline | Skin output buffer missing VERTEX_BUFFER usage flag (M29.3 deferred) | crates/renderer/src/vulkan/skin_compute.rs:405 | OPEN |
| NCPS-03 | LOW | Compute Pipeline Safety | TAA/bloom/volumetrics dispatch() emit redundant HOST→COMPUTE UBO barriers | crates/renderer/src/vulkan/taa.rs:671 | OPEN |
| NCPS-04 | LOW | Compute Pipeline Safety | R32_UINT storage image atomic format assumed without device query | crates/renderer/src/vulkan/caustic.rs:62 | OPEN |
| R1-MAT-02 | LOW | Material Table | gpu_material_glsl_field_names_pinned test omits 11 GLSL field needles | crates/renderer/src/vulkan/material.rs:1197 | OPEN |
| EGUI-02 | LOW | Debug-UI Teardown | egui texture upload bypasses the graphics_queue Mutex | crates/renderer/src/vulkan/context/draw.rs:2962 | OPEN |
| EGUI-03 | LOW | Debug-UI Teardown | EguiPass::destroy() does not flush pending_free before Renderer drop | crates/renderer/src/vulkan/egui_pass.rs:194 | OPEN |
| EGUI-04 | LOW | Debug-UI Teardown | egui render pass has no outgoing EXTERNAL subpass dependency | crates/renderer/src/vulkan/egui_pass.rs:244 | OPEN |
| IOR-03 | LOW | RT IOR-Refraction | atomicAdd budget guard permanently overshoots rayBudgetCount on rejected threads | crates/renderer/shaders/triangle.frag:2124 | OPEN |
| IOR-04 | LOW | RT IOR-Refraction | Three distinct bitfields all assign 128u in shader_constants.glsl | crates/renderer/shaders/include/shader_constants.glsl:53 | OPEN |
| ANIM-05 | LOW | NPC Animation | BSPSysSimpleColorModifier RGBA values skip the finite/FLT_MAX guard | crates/nif/src/import/walk/mod.rs:652 | OPEN |
| ANIM-08 | LOW | NPC Animation | Multi-emitter NIFs: color curve and rate extraction are first-match only | crates/nif/src/import/walk/mod.rs:616 | OPEN |
| NIFAL-S4 | LOW | NIFAL Translation | Collision shape radii/half-extents carry raw NIF floats with no finite guard | crates/nif/src/import/collision.rs:291 | OPEN |
| SAFE-U7 | LOW | Unsafe Rust | Test code calls `String::as_bytes_mut` without SAFETY comment | crates/core/src/string/mod.rs:233 | OPEN |
| SAFE-U8 | INFO | Unsafe Rust | `BuiltinType::from_u32` is correctly a checked match — no transmute | crates/sfmaterial/src/types.rs:36 | VERIFIED CLEAN |
| SAFE-U9 | INFO | Unsafe Rust | ECS query raw-pointer caching is well-documented and sound | crates/core/src/ecs/query.rs:63 | VERIFIED CLEAN |
| SAFE-U10 | INFO | Unsafe Rust | `read_pod_vec` byte-cast is sound and well-commented | crates/nif/src/stream.rs:340 | VERIFIED CLEAN |
| VKC-006 | INFO | Vulkan Spec | Validation layers enabled only in debug builds | crates/renderer/src/vulkan/instance.rs:41 | OPEN |
| VKC-007 | INFO | Vulkan Spec | sync1 barriers use ACCELERATION_STRUCTURE_READ_KHR instead of BUILD_INPUT_READ_ONLY_KHR | crates/renderer/src/vulkan/context/draw.rs:795 | OPEN |
| MEM-07 | INFO | Memory Safety | NIF stream allocation guards are comprehensive and correct | crates/nif/src/stream.rs:33 | VERIFIED CLEAN |
| MEM-08 | INFO | Memory Safety | Cell unload GPU resource sweep is comprehensive and correctly ordered | byroredux/src/cell_loader/unload.rs:34 | VERIFIED CLEAN |
| MEM-09 | INFO | Memory Safety | VulkanContext Drop order is correct — allocator freed before device | crates/renderer/src/vulkan/context/mod.rs:2564 | VERIFIED CLEAN |
| TS-01 | INFO | Thread Safety | TypeId-sorted multi-lock acquisition consistently enforced in World | crates/core/src/ecs/world.rs:410 | VERIFIED CLEAN |
| TS-04 | INFO | Thread Safety | Vulkan queue external synchronization correctly uses Arc<Mutex<vk::Queue>> | crates/renderer/src/vulkan/context/mod.rs:1374 | VERIFIED CLEAN |
| TS-05 | INFO | Thread Safety | Allocator and queue locks are never held concurrently | crates/renderer/src/vulkan/scene_buffer/upload.rs:652 | VERIFIED CLEAN |
| TS-07 | INFO | Thread Safety | No unsafe Send/Sync impls exist anywhere in the codebase | crates/core/src/ecs/storage.rs:17 | VERIFIED CLEAN |
| TS-09 | INFO | Thread Safety | Debug server uses correct non-reentrant single-lock patterns | crates/debug-server/src/listener.rs:148 | VERIFIED CLEAN |
| FFI-03 | INFO | FFI Safety | No panic=abort profile — cxx exception bridging relies on unwinding | Cargo.toml:194 | OPEN |
| FFI-04 | INFO | FFI Safety | cxx-bridge is a minimal placeholder — FFI surface area is near-zero | crates/cxx-bridge/src/lib.rs:7 | VERIFIED CLEAN |
| RT-05 | INFO | RT Pipeline | instance_custom_index overflow guard is debug_assert-only (release silently truncates) | crates/renderer/src/vulkan/acceleration/tlas.rs:207 | OPEN |
| RT-06 | INFO | RT Pipeline | BLAS result/compaction buffers use correct minimal usage flags | crates/renderer/src/vulkan/acceleration/blas_static.rs:216 | VERIFIED CLEAN |
| NCPS-05 | INFO | Compute Pipeline Safety | TAA history images: first-frame safety relies solely on shader guard | crates/renderer/src/vulkan/taa.rs:513 | OPEN |
| NCPS-06 | INFO | Compute Pipeline Safety | SVGF, TAA, bloom histories correctly handle UNDEFINED→GENERAL | crates/renderer/src/vulkan/svgf.rs:729 | VERIFIED CLEAN |
| NCPS-07 | INFO | Compute Pipeline Safety | Skin compute push constants and vertex stride pinned by unit tests | crates/renderer/src/vulkan/skin_compute.rs:45 | VERIFIED CLEAN |
| R1-MAT-01 | INFO | Material Table | Stale doc comment: MAX_MATERIALS comment claims 304 B/entry, struct is 300 B | crates/renderer/src/vulkan/scene_buffer/constants.rs:154 | OPEN |
| R1-MAT-03 | INFO | Material Table | Release-build FxHash collision silently aliases distinct materials | crates/renderer/src/vulkan/material.rs:1045 | OPEN |
| R1-MAT-04 | INFO | Material Table | upload_materials overflow guard is debug_assert only, not a hard assert | crates/renderer/src/vulkan/scene_buffer/upload.rs:516 | OPEN |
| R1-MAT-05 | INFO | Material Table | No vec3 alignment hazard: struct is all-scalar f32/u32 | crates/renderer/src/vulkan/material.rs:67 | VERIFIED CLEAN |
| R1-MAT-06 | INFO | Material Table | Default impl correctly zeros all new Disney/translucency/IOR scalars | crates/renderer/src/vulkan/material.rs:273 | VERIFIED CLEAN |
| R1-MAT-07 | INFO | Material Table | Over-cap intern silently degrades to neutral-default material (Once-gated warn) | crates/renderer/src/vulkan/material.rs:1066 | OPEN |
| IOR-05 | INFO | RT IOR-Refraction | Frisvad buildOrthoBasis singularity at (0,0,-1) correctly handled | crates/renderer/shaders/triangle.frag:436 | VERIFIED CLEAN |
| IOR-06 | INFO | RT IOR-Refraction | Interior glass refraction miss-fallback correctly uses cell ambient | crates/renderer/shaders/triangle.frag:550 | VERIFIED CLEAN |
| IOR-07 | INFO | RT IOR-Refraction | Glass passthru loop same-texture identity check is coarse but documented | crates/renderer/shaders/triangle.frag:2273 | VERIFIED CLEAN |
| ANIM-01 | INFO | NPC Animation | FLT_MAX sentinel handling correctly implemented and tested | crates/nif/src/anim/bspline.rs:326 | VERIFIED CLEAN |
| ANIM-02 | INFO | NPC Animation | B-spline dispatch correctly ungated — not restricted to Skyrim+ | crates/nif/src/blocks/mod.rs:832 | OPEN |
| ANIM-03 | INFO | NPC Animation | AnimationClipRegistry dedup is case-insensitive and allocation-optimal | crates/core/src/animation/registry.rs:95 | VERIFIED CLEAN |
| ANIM-04 | INFO | NPC Animation | Particle emitter spawn-rate FLT_MAX sentinel is guarded and tested | crates/nif/src/import/walk/mod.rs:714 | VERIFIED CLEAN |
| ANIM-06 | INFO | NPC Animation | Starfield NPC spawn correctly returns None for idle KF path | byroredux/src/npc_spawn.rs:249 | VERIFIED CLEAN |
| ANIM-07 | INFO | NPC Animation | Animation stack correctly falls back to bind pose when all channels sample None | crates/core/src/animation/stack.rs:329 | VERIFIED CLEAN |
| NIFAL-S1 | INFO | NIFAL Translation | Material PBR scalars: translate_material is single boundary, resolve_pbr always runs | byroredux/src/material_translate.rs | VERIFIED CLEAN |
| NIFAL-S2 | INFO | NIFAL Translation | BGSM merge path uses clamped arithmetic for metalness/roughness override | byroredux/src/asset_provider.rs:1110 | VERIFIED CLEAN |
| NIFAL-S5 | INFO | NIFAL Translation | NiPSysGrowFadeModifier.base_scale has no finite guard before size computation | crates/nif/src/import/walk/mod.rs:687 | OPEN |
| NIFAL-S6 | INFO | NIFAL Translation | Glass roughness override is always finite (GLASS_ROUGHNESS = 0.10 constant) | byroredux/src/helpers.rs:70 | VERIFIED CLEAN |
| NIFAL-S7 | INFO | NIFAL Translation | particle_system spawn loop guards life with .max(0.05) but not size or rate | byroredux/src/systems/particle.rs:332 | OPEN |
| EGUI-05 | INFO | Debug-UI Teardown | EguiPassConfig is dead code — defined but never constructed | crates/debug-ui/src/lib.rs:208 | OPEN |
| EGUI-06 | INFO | Debug-UI Teardown | App struct field ordering correctly sequences DebugUiState drop before VulkanContext | byroredux/src/main.rs:266 | VERIFIED CLEAN |

---

## Per-Dimension Findings

### Dimension 1: Unsafe Rust Blocks

The codebase contains approximately 539 `unsafe {}` blocks, predominantly Vulkan API calls
in the renderer. No transmutes on unvalidated data, dangling pointer dereferences, or
aliasing violations were found. The highest-risk patterns are well-documented. The main
weaknesses are stale or missing SAFETY comments, and the pervasive absence of comments on
the ~330 Vulkan API `unsafe` blocks outside the critical ECS/NIF paths.

#### SAFE-U1 — MEDIUM: Stale doc comment claims `transmute` on `BuiltinType::from_u32`
**File:** `crates/sfmaterial/src/types.rs:10`

The module-level doc comment on `BuiltinType` reads: "Cast `TypeReference.id` to `u32`,
then transmute into this enum via `BuiltinType::from_u32`." The actual implementation at
lines 36–55 is a fully checked `match` expression with an explicit `Err(UnsupportedBuiltin)`
arm — no `transmute` is used anywhere in the file. A reviewer trusting the comment would
wrongly believe unvalidated enum transmute was happening, potentially missing a real
occurrence if it were later introduced.

**Fix:** Replace the stale comment with accurate text: "Cast `TypeReference.id` to `u32`,
then pass it to `BuiltinType::from_u32`, which validates the pattern with a checked `match`
and returns `Err` for unknown tags. No unsafe code is involved."

#### SAFE-U2 — MEDIUM: `slice::from_raw_parts` on `WaterPush` has no SAFETY comment
**File:** `crates/renderer/src/vulkan/water.rs:466`

Inside `WaterPipeline::record_draw` (marked `unsafe fn`), a `std::slice::from_raw_parts`
call byte-casts `&WaterPush` to `&[u8]` for `cmd_push_constants`. The struct is `#[repr(C)]`
and `Copy` with all `[f32; 4]` fields, so the invariant is upheld. However, unlike the
parallel pattern in `skin_compute.rs` (lines 559–563 and 880–882), this site has no
`// SAFETY:` comment. A future field addition to `WaterPush` (e.g. a `bool` flag or a
non-`Copy` type) would silently break the invariant without any textual warning.

**Fix:** Add a `// SAFETY: WaterPush is #[repr(C)] + Copy with only [f32;4] fields (no
padding, no invalid byte patterns). `push` is a valid shared reference; the byte slice is
bounded by `size_of::<WaterPush>()`.` comment immediately before the `from_raw_parts` call.

#### SAFE-U3 — MEDIUM: Inner `unsafe` block in `debug_callback` missing SAFETY comment
**File:** `crates/renderer/src/vulkan/debug.rs:50`

The `unsafe extern "system" fn debug_callback` correctly checks `callback_data.is_null()`
before dereferencing. Inside the nested `unsafe {}` block (line 50–58), `&*callback_data`
is used to access `p_message`, which is passed to `CStr::from_ptr`. Neither the outer
function body nor the inner block carries a `// SAFETY:` comment explaining the Vulkan spec
guarantee (VK_EXT_debug_utils §7.1).

**Fix:** Add `// SAFETY: Vulkan spec (VK_EXT_debug_utils) guarantees callback_data is valid
if non-null, and p_message is a valid NUL-terminated C string or NULL.` before the inner
`unsafe {}` block.

#### SAFE-U4 — MEDIUM: `CStr::from_ptr` in `check_validation_layer_support` has no SAFETY comment
**File:** `crates/renderer/src/vulkan/instance.rs:71`

`CStr::from_ptr` is called on a Vulkan-returned `VkLayerProperties::layerName` fixed-size
array. This is safe because the Vulkan spec mandates that `layerName` is a null-terminated
UTF-8 string within a 256-byte `char[256]` field. However, there is no `// SAFETY:`
comment, leaving the invariant implicit.

**Fix:** Add: `// SAFETY: VkLayerProperties::layerName is a null-terminated C string of at
most 256 bytes per the Vulkan spec; the pointer is valid for the lifetime of the iteration.`

#### SAFE-U5 — MEDIUM: `upload_lights` SAFETY comment omits pointer-arithmetic in-bounds invariant
**File:** `crates/renderer/src/vulkan/scene_buffer/upload.rs:41`

The `// SAFETY` comment states buffer sizing but does not explicitly document that
`header_size + light_size * count <= mapped.len()`. There is no `debug_assert!` to catch a
regression if buffer sizing changes. `upload_materials` and `upload_instances` use similar
patterns with the same omission.

**Fix:** Add `debug_assert!(header_size + light_size * count <= mapped.len(), "upload_lights:
write would exceed mapped buffer");` before the unsafe block, and amend the SAFETY comment
to note the `.add(header_size)` is in bounds because buffer capacity covers
`header_size + MAX_LIGHTS * light_size`.

#### SAFE-U6 — MEDIUM: Pervasive absence of SAFETY comments on Vulkan-API `unsafe` blocks
**File:** `crates/renderer/src/vulkan/` (various)

Approximately 208 of 539 `unsafe {}` blocks in the renderer carry `// SAFETY:` comments.
The majority of uncommented blocks are `ash::Device` / `ash::Instance` API calls in
`gpu_timers.rs` (30+ uncommented), `texture_registry.rs` (10+), and
`acceleration/blas_skinned.rs` (11 of 13), where synchronisation ordering matters and
silence makes it impossible to distinguish "trivially safe" from "relies on subtle invariant."

**Fix:** Adopt a tiered policy: (1) trivial destroy/create calls may carry a one-line
`// SAFETY: handle was created by this struct; GPU idle` note; (2) calls that depend on
synchronisation state must have full `// SAFETY:` comments. Start with `gpu_timers.rs` and
`acceleration/blas_skinned.rs`.

#### SAFE-U7 — LOW: Test code calls `String::as_bytes_mut` without SAFETY comment
**File:** `crates/core/src/string/mod.rs:233`

In the unit test `case_insensitive_across_fast_and_slow_paths`, `String::as_bytes_mut()` is
used to mutate ASCII bytes in-place. The mutation (ASCII 'X' → 'x') maintains UTF-8 validity,
but the block has no `// SAFETY:` comment.

**Fix:** Add: `// SAFETY: only ASCII single-byte codepoints are written (b'x' = 0x78);
replacing an ASCII 'X' keeps the string valid UTF-8.`

#### SAFE-U8 — INFO: `BuiltinType::from_u32` is correctly a checked match — VERIFIED CLEAN
**File:** `crates/sfmaterial/src/types.rs:36`

Confirmed: implementation is an exhaustive `match` with `Err(UnsupportedBuiltin { raw })`
arm. No transmute anywhere in the file. No action required beyond fixing the stale doc comment
(SAFE-U1).

#### SAFE-U9 — INFO: ECS query raw-pointer caching is well-documented and sound — VERIFIED CLEAN
**File:** `crates/core/src/ecs/query.rs:63, 130, 138, 284`

All four `unsafe` dereferences carry detailed `// SAFETY:` comments. The raw pointer is
resolved from the guard's boxed storage in `new()`, and the guard is held for the wrapper's
lifetime. The pattern is the correct fix for the pre-#1367 soundness hole.

#### SAFE-U10 — INFO: `read_pod_vec` byte-cast is sound and well-commented — VERIFIED CLEAN
**File:** `crates/nif/src/stream.rs:340`

Both `read_pod_vec` and `read_pod_vec_from_cursor` carry multi-line `// SAFETY:` comments
documenting: non-null aligned pointer origin, overflow-safe byte count, `T: Copy + Default`
any-bit-pattern contract, and LE-host compile-error gate.

---

### Dimension 2: Vulkan Spec Compliance

The overall Vulkan spec compliance posture is solid: teardown follows reverse-creation order
with a leading `device_wait_idle`, `FrameSync` uses per-image render-finished semaphores to
avoid VUID-00067, queue submissions are correctly serialized via Mutex, TLAS UPDATE/BUILD
mode selection guards VUID-03708, and deferred-destroy queues prevent use-after-destroy for
evicted BLAS entries.

#### VKC-001 — MEDIUM: `PipelineStageFlags::NONE` used without synchronization2 feature gate
**File:** `crates/renderer/src/vulkan/bloom.rs:399` (also ssao.rs:461, caustic.rs:653, volumetrics.rs:692, texture.rs:346)

Multiple `initialize_layouts` call sites emit `cmd_pipeline_barrier` with
`srcStageMask = vk::PipelineStageFlags::NONE`. Under VK sync1 (Vulkan <1.3 or 1.3 without
`synchronization2` enabled), `NONE` (== 0) is not a legal stage mask value per
VUID-vkCmdPipelineBarrier-srcStageMask-4957. The device.rs code correctly probes
`synchronization2_supported` but the barrier call sites use NONE unconditionally.

**Fix:** Either (a) require `synchronization2` as a mandatory device feature (since RT already
requires Vulkan 1.3-class hardware) and document the hard minimum, or (b) have each barrier
site fall back to `TOP_OF_PIPE` when `sync2` is false. Option (a) is simpler given the
engine's existing VRAM baseline policy.

#### VKC-002 — MEDIUM: Scratch alignment violation is debug-only in release builds
**File:** `crates/renderer/src/vulkan/acceleration/mod.rs:243`

All BLAS and TLAS build sites enforce `minAccelerationStructureScratchOffsetAlignment` via
`debug_assert_scratch_aligned`, which compiles to nothing in `--release`. If gpu-allocator
returns an allocation whose device address is not a multiple of `scratch_align`, the AS
build silently proceeds in release with a misaligned scratch address, violating
VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03715. Drivers may produce corrupted BVH
with silent incorrect ray hits.

**Fix:** Upgrade the debug_assert to a run-time check, or enforce alignment explicitly:
`let aligned = (raw_addr + (scratch_align as u64 - 1)) & !(scratch_align as u64 - 1);`

#### VKC-003 — MEDIUM: TLAS resize destroys live resources with no code-level enforcement of fence-wait
**File:** `crates/renderer/src/vulkan/acceleration/tlas.rs:289`

When `need_new_tlas` is true, the old TLAS is destroyed immediately without an explicit
`device_wait_idle`. Correctness relies on the caller having run a double-slot `wait_for_fences`
before reaching this path. This invariant is documented in a comment but not enforced
structurally. A future refactor that moves the fence wait after the TLAS build would
create a use-after-destroy violating VUID-vkDestroyAccelerationStructureKHR-accelerationStructure-02442.

**Fix:** Accept a `_fence_waited: &FenceWaitProof` zero-size token at the resize entry point,
making the invariant type-checked rather than comment-documented. Alternatively, add a
defensive `device_wait_idle` inside the `if need_new_tlas` block.

#### VKC-004 — LOW: TLAS UPDATE primitiveCount mismatch guard is debug-only in release
**File:** `crates/renderer/src/vulkan/acceleration/tlas.rs:568`

The guard at tlas.rs:774 (`debug_assert_eq!(tlas.built_primitive_count, instance_count)`)
is removed in release builds. The runtime guard at line 568 only covers the case where count
grew. If `instance_count` shrinks below `built_primitive_count` AND `use_update` is true,
the code still uses `tlas.built_primitive_count` as `range_primitive_count`, which may cause
the AS build to read instance data beyond what was written this frame — a silent BVH
corruption.

**Fix:** Also force a full BUILD when `instance_count < tlas.built_primitive_count` on the
UPDATE path, at the same site as the existing count-growth guard:
`if use_update && instance_count != tlas.built_primitive_count { use_update = false; }`

#### VKC-005 — LOW: Allocator Arc leak path skips `device_wait_idle` before early return
**File:** `crates/renderer/src/vulkan/context/mod.rs:2787`

When `Arc::try_unwrap` fails, the code logs an error and returns early, leaking device,
surface, instance, and debug messenger handles. The path does not call `device_wait_idle`
before returning, so the outstanding Arc clones wrapping live VkBuffer/VkImage handles could
still be referenced by in-flight GPU work when they naturally drop.

**Fix:** Before the early `return`, call `let _ = self.device.device_wait_idle();` to settle
in-flight GPU work.

#### VKC-006 — INFO: Validation layers enabled only in debug builds
**File:** `crates/renderer/src/vulkan/instance.rs:41`

Standard pattern, correctly guarded with `cfg!(debug_assertions)`. No CI job runs a full
frame under validation.

**Recommendation:** Consider a CI step running the headless bench in a debug build with
`VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation` — the debug messenger already routes
validation callbacks through `log::error!`.

#### VKC-007 — INFO: sync1 barriers use ACCELERATION_STRUCTURE_READ_KHR for COMPUTE-to-AS transitions
**File:** `crates/renderer/src/vulkan/context/draw.rs:795`

Documented and intentional drift. On current drivers, `ACCELERATION_STRUCTURE_READ_KHR`
and `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR` are aliased. The more specific flag
is required when migrating to `cmd_pipeline_barrier2`.

**Recommendation:** When sync2 is adopted, perform a project-wide search for
`ACCELERATION_STRUCTURE_READ_KHR` used as dst_access on COMPUTE→AS_BUILD barriers.

---

### Dimension 3: Memory Safety

Two medium-severity memory safety concerns. `NifImportRegistry` is unlimited by default,
growing for the process lifetime. `MeshRegistry` allocates handles via `self.meshes.len() as
u32` with no overflow guard while `TextureRegistry` has an explicit cap. The GPU allocator
Arc leak fallback on Drop is the third medium concern.

#### MEM-01 — MEDIUM: NifImportRegistry unbounded by default — process-lifetime RAM growth
**File:** `byroredux/src/cell_loader/nif_import_registry.rs:107-115`

`NifImportRegistry` is unlimited by default (`BYRO_NIF_CACHE_MAX=0`). In a long streaming
session across a large world, every unique NIF path parsed is retained permanently including
full parsed scene, mesh geometry, lights, and embedded animation clips. The LRU eviction
machinery exists but is opt-in via an env var not set by default. Over a multi-hour exterior
session on a Skyrim radius-3 grid this can accumulate several hundred MB of parsed NIF data.

**Fix:** Set a practical default for `max_entries` (e.g., 2048) rather than 0. The
`BYRO_NIF_CACHE_MAX` override can still raise or lower it. Add a startup warning when the
cache is unlimited and exterior streaming is active.

#### MEM-02 — MEDIUM: MeshRegistry handle Vec has no u32 overflow guard
**File:** `crates/renderer/src/mesh.rs:284`

In `MeshRegistry::upload` and `upload_scene_mesh` (lines 284, 430), the new handle is
computed as `self.meshes.len() as u32` with no overflow guard. Handles are never reused:
`drop_mesh` sets the slot to `None` but does not compact the handle index Vec. On a very
long session, `meshes.len()` could eventually exceed `u32::MAX`, silently aliasing new
handles to old ones, producing silent corruption in `GpuInstance.mesh_id` lookups and BLAS
table entries. `TextureRegistry` has an explicit guard but `MeshRegistry` does not.

**Fix:** Add an explicit capacity check before the `len() as u32` cast:
`if self.meshes.len() >= u32::MAX as usize { return Err(...); }`. Also consider adding a
`max_meshes` field mirroring `TextureRegistry.max_textures`.

#### MEM-03 — MEDIUM: GPU allocator Arc leak on Drop leaks device/surface/instance handles
**File:** `crates/renderer/src/vulkan/context/mod.rs:2760-2795`

When `Arc::try_unwrap` on the allocator fails, the code takes an early-return path and
leaks `VkDevice`, `VkSurfaceKHR`, `VkInstance`, and `VkDebugUtilsMessengerEXT`. On any
exit path where the allocator's refcount is non-unity (e.g., a panic mid-load while textures
are queued), the OS must reclaim the leaked handles. In release builds no debug_assert fires.

**Fix:** Audit all sites that clone the `SharedAllocator` Arc to ensure they are dropped
before `VulkanContext::drop` reaches allocator teardown. Consider avoiding cloning the
allocator Arc into objects that can outlive the teardown window — pass by reference instead.

#### MEM-04 — LOW: BGSM material cache uses flush-on-overflow eviction
**File:** `byroredux/src/asset_provider.rs:900-911`

`MaterialProvider.bgem_cache` caps at `MAX_BGEM_CACHE_ENTRIES=1024` but evicts by clearing
the entire map when the cap is hit. This causes repeated full flushes on high-churn sessions.
The `failed_paths` cache uses the same strategy. A performance concern, not a correctness issue.

**Fix:** Replace flush-on-overflow with LRU eviction (retain the most-recently-used half).

#### MEM-05 — LOW: `read_pod_vec` safety relies on prose comment rather than type-system bound
**File:** `crates/nif/src/stream.rs:311`

`read_pod_vec` is `pub(crate)` and parameterized over `T: Copy + Default` — there is no
compile-time enforcement that `T` is actually all-bit-patterns-valid. A future call site with
a `T` that has uninhabited variants would produce undefined behavior that bypasses unsafe
block review because the call site looks safe.

**Fix:** Introduce an `unsafe trait AnyBitPattern {}` (or use `bytemuck::AnyBitPattern`) and
change the bound to `T: Copy + Default + AnyBitPattern`.

#### MEM-06 — LOW: Collision shape recursion depth unbounded for deeply-nested BhkListShape
**File:** `crates/nif/src/import/collision.rs:261-277`

`resolve_shape` uses a `HashSet<usize>` to break cycles but does not bound recursion depth
for acyclic but deeply nested `BhkListShape` trees. A corrupt NIF could in principle contain
a 10,000-node chain, overflowing the stack. The codebase has a documented no-recursion
policy for parser robustness.

**Fix:** Add a depth counter parameter to `resolve_shape_inner` and return `None` with a
`warn!` log when depth exceeds 64.

#### MEM-07 — INFO: NIF stream allocation guards are comprehensive and correct — VERIFIED CLEAN
**File:** `crates/nif/src/stream.rs:33-278`

`NifStream::check_alloc` correctly enforces a 256 MB hard cap and a remaining-bytes check
before any file-driven allocation. `read_pod_vec` uses `checked_mul` for the byte count.
The header parser independently validates block-type and block-count fields.

#### MEM-08 — INFO: Cell unload GPU resource sweep is comprehensive and correctly ordered — VERIFIED CLEAN
**File:** `byroredux/src/cell_loader/unload.rs:34-193`

`unload_cell` correctly sweeps all per-entity texture-bearing components. Drop order within
`unload_cell` is correct: terrain tiles freed first, BLAS dropped before mesh buffer, mesh
dropped before entity despawn.

#### MEM-09 — INFO: VulkanContext Drop order is correct — allocator freed before device — VERIFIED CLEAN
**File:** `crates/renderer/src/vulkan/context/mod.rs:2564`

Drop correctly sequences: `device_wait_idle` first, then subsystem destroys, then allocator
extracted via `Arc::try_unwrap`, then `device.destroy_device`, then surface, then debug
messenger, then instance. The only concern is the Arc leak fallback (MEM-03).

---

### Dimension 4: Thread Safety

Thread safety is well-architected at the ECS layer and adequately implemented in the Vulkan
renderer. No unsafe `Send`/`Sync` impls exist anywhere.

#### TS-01 — INFO: TypeId-sorted multi-lock acquisition consistently enforced in World — VERIFIED CLEAN
**File:** `crates/core/src/ecs/world.rs:410-503`

All multi-component/resource queries acquire locks in TypeId-ascending order. ABBA deadlock
pattern is structurally prevented for paired-type access. Unit tests verify the ordering.

#### TS-02 — MEDIUM: Global ABBA detector is opt-in and absent from CI
**File:** `crates/core/src/ecs/lock_tracker.rs:217`

The cross-thread lock-order graph is only active when `BYRO_LOCK_ORDER_CHECK=1` is set. No
CI configuration sets this variable. The parallel-scheduler feature (rayon) is enabled by
default, so cross-thread ABBA risks are real in production. The thread-local same-thread
re-entrancy check always fires, but the cross-thread ABBA check does not.

**Fix:** Add `BYRO_LOCK_ORDER_CHECK=1` to at least one CI job that runs `cargo test` with
the parallel-scheduler feature enabled. Zero code change required.

#### TS-03 — LOW: Systems with undeclared access generate unknown conflict pairs
**File:** `crates/core/src/ecs/scheduler.rs:594-609`

Any pairing of an undeclared system with any other system produces `AccessConflict::Unknown`,
which the scheduler cannot resolve statically. With rayon-parallel execution enabled by
default, undeclared systems block the parallel conflict analyzer from verifying safety.

**Fix:** Migrate system registration to include `Access` declarations. Drive
`undeclared_parallel_count()` to zero.

#### TS-04 — INFO: Vulkan queue external synchronization correctly uses Arc<Mutex<vk::Queue>> — VERIFIED CLEAN
**File:** `crates/renderer/src/vulkan/context/mod.rs:1374-1381`

Both graphics and present queues are wrapped in `Arc<Mutex<vk::Queue>>`. The queue guard is
explicitly bound to a named variable for the lifetime of `vkQueueSubmit`.

#### TS-05 — INFO: Allocator and queue locks are never held concurrently — VERIFIED CLEAN
**File:** `crates/renderer/src/vulkan/scene_buffer/upload.rs:652-714`

Allocator is always acquired, result captured, guard dropped, then queue is locked.
No ABBA risk between the allocator Mutex and the queue Mutex exists.

#### TS-06 — LOW: SSAO OOM path guards against self-deadlock on allocator Mutex by comment only
**File:** `crates/renderer/src/vulkan/ssao.rs:149-165`

Correctness depends on implicit RAII drop timing: the `MutexGuard` from `.lock()` must drop
before the `Err` arm calls `partial.destroy()` which re-locks the same non-reentrant Mutex.
Similar patterns in `gbuffer.rs`, `svgf.rs`, and `caustic.rs` lack equivalent comments.

**Fix:** Add a brief comment at each similar call site referencing the #1163 fix, or
introduce a helper wrapping the allocate-or-cleanup idiom.

#### TS-07 — INFO: No unsafe Send/Sync impls exist anywhere in the codebase — VERIFIED CLEAN
**File:** `crates/core/src/ecs/storage.rs:17-18`

Grep across all crates returned no results for `unsafe impl Send` or `unsafe impl Sync`.
All thread safety is derived from Rust's standard derives and RwLock/Mutex wrappers.

#### TS-08 — LOW: Scheduler parallel panic policy leaves partial ECS state without recovery
**File:** `crates/core/src/ecs/scheduler.rs:386-406`

With rayon, a panicking system can leave component storage RwLocks in a poisoned state.
Subsequent stages do not run for the rest of the frame. The main loop does not `catch_unwind`.
A panicking system terminates the process.

**Fix:** Document explicitly. For future robustness, consider wrapping each rayon task body
in `catch_unwind` and serializing the error to a shared error slot.

#### TS-09 — INFO: Debug server uses correct non-reentrant single-lock patterns — VERIFIED CLEAN
**File:** `crates/debug-server/src/listener.rs:148-218`

Locks are acquired independently and not nested. The post-accept shutdown check folds into
the `active_streams.lock()` critical section to prevent TOCTOU races.

---

### Dimension 5: FFI Safety

The cxx-bridge is a minimal two-function scaffold. No raw pointers, `CStr` lifetime hazards,
`#[no_mangle]` bypasses, or manual `Send`/`Sync` impls were found.

#### FFI-01 — LOW: Exported Rust function `engine_info` declared but never called from C++
**File:** `crates/cxx-bridge/src/lib.rs:17-19`

`engine_info` is declared in `extern "Rust"` but the only C++ file (`native_utils.cpp`)
never references it. A dead exported symbol that could become a maintenance trap if C++ code
were later added that calls it with wrong types from a non-Rust thread.

**Fix:** Remove the `extern "Rust" { fn engine_info() -> EngineInfo; }` declaration if it
is purely aspirational scaffolding, or add a corresponding C++ test caller.

#### FFI-02 — LOW: No test coverage for the cxx bridge
**File:** `crates/cxx-bridge/src/lib.rs:1-36`

No `#[test]` blocks or `#[cfg(test)]` module. The only validation that `native_hello()`
works is its call in `main.rs` at startup.

**Fix:** Add a `#[cfg(test)]` module with at least one test calling `ffi::native_hello()`.

#### FFI-03 — INFO: No `panic = "abort"` profile — cxx exception bridging relies on unwinding
**File:** `Cargo.toml:194-209`

The workspace `Cargo.toml` has no `[profile.release]` section, leaving the default
`panic = "unwind"`. If a future profile sets `panic = "abort"`, the C++-to-Rust exception
path would become unsound.

**Recommendation:** Document in `[profile.release]` that `panic = "unwind"` is required for
cxx exception safety, or ensure all C++ functions are `noexcept` if `abort` is ever desired.

#### FFI-04 — INFO: cxx-bridge is a minimal placeholder — FFI surface area is near-zero — VERIFIED CLEAN
**File:** `crates/cxx-bridge/src/lib.rs:7-27`

One function each way. C++ implementation is a trivial string literal return. No raw pointers,
`CStr` lifetime hazards, or `#[no_mangle]` bypasses found.

---

### Dimension 6: RT Pipeline Safety

RT pipeline safety is substantially sound. BLAS flag invariants, scratch-buffer alignment
assertions, inter-build scratch-serialize barriers, deferred BLAS destruction, and
`instance_custom_index` documentation are all correct. Three live issues were found in
`water.frag` ray origin biasing.

#### RT-01 — MEDIUM: water.frag caustic floor-ray: no origin bias and mismatched tMin=0.001
**File:** `crates/renderer/shaders/water.frag:554-558`

The caustic floor-ray fired from `vWorldPos` uses `tMin = 0.001` with no surface-normal
origin bias. The water-surface geometry is in the TLAS and `vWorldPos` lies on it, so this
ray can self-intersect the water plane at t < 0.001 on drivers that round the zero-thickness
hit downward, producing a false floor-hit at t ≈ 0 and misplacing the caustic splat at the
water surface rather than on the floor. The sibling caustic shadow-ray (line 539) uses an
explicit `N * 0.05` bias; the floor-ray has neither the bias nor a matching tMin. All other
ray-query sites in the codebase use `tMin = 0.05`.

**Fix:** Apply `N * 0.05` bias to the floor-ray origin and raise tMin to `0.05` to match the
established convention in `triangle.frag`, `caustic_splat.comp`, and `foamShoreline`.

#### RT-02 — LOW: water.frag shadow-ray: N-bias (0.05) and tMin (0.001) are inconsistent
**File:** `crates/renderer/shaders/water.frag:536-540`

The caustic shadow-ray biases the origin by `N * 0.05` but uses `tMin = 0.001` — a 50×
mismatch. The comment at `triangle.frag:3132` explicitly documents why tMin should equal
the bias distance. With a 0.05-unit bias and a 0.001 tMin, rays fired at near-grazing
angles can still re-intersect the water surface between t=0.001 and t=0.05.

**Fix:** Raise `tMin` to `0.05` on the caustic shadow ray to match the bias.

#### RT-03 — LOW: water.frag reflection/refraction rays: no origin bias on water surface
**File:** `crates/renderer/shaders/water.frag:436, 457`

`traceWaterRay` is called with `origin = vWorldPos` directly for both reflection (line 436)
and refraction (line 457) rays, with only an internal `tMin = 0.05`. This provides no
protection when the perturbed surface normal causes the origin to be slightly below the water
plane (bump-displaced vertices, high-frequency wave displacement).

**Fix:** Pass a biased origin: `traceWaterRay(vWorldPos + N * 0.05, ...)` for both rays,
and update the `traceWaterRay` signature comment to note that callers are responsible for
biasing the origin when firing from a surface.

#### RT-04 — LOW: Skin output buffer missing VERTEX_BUFFER usage flag (M29.3 deferred)
**File:** `crates/renderer/src/vulkan/skin_compute.rs:405-411`

The output buffer deliberately omits `VERTEX_BUFFER`. Until added in M29.3, the buffer
cannot be bound as a VBO to the raster pipeline. The deferred milestone is not tracked in
the constants or a visible TODO at the buffer creation site.

**Fix:** Add a `// TODO(M29.3): add VERTEX_BUFFER flag here when wiring raster skinned path`
comment with an issue reference at the buffer creation site.

#### RT-05 — INFO: instance_custom_index overflow guard is debug_assert-only
**File:** `crates/renderer/src/vulkan/acceleration/tlas.rs:207-215`

The 24-bit `instance_custom_index` bounds check uses `debug_assert!`. In release builds,
`Packed24_8::new` will silently truncate any `ssbo_idx >= 2^24`, misdirecting all
RT-hit SSBO lookups. Current `MAX_INSTANCES = 0x40000` is safely below the ceiling.

**Fix:** Promote to a hard `assert!` or add a `u32::MAX` sentinel path that logs-and-skips
the over-range instance. Add a compile-time assertion that `MAX_INSTANCES < (1 << 24)`.

#### RT-06 — INFO: BLAS result/compaction buffers use correct minimal usage flags — VERIFIED CLEAN
**File:** `crates/renderer/src/vulkan/acceleration/blas_static.rs:216-222`

Static and skinned BLAS result buffers are correctly allocated with
`ACCELERATION_STRUCTURE_STORAGE_KHR | SHADER_DEVICE_ADDRESS`. No additional flags needed.

---

### Dimension 7: New Compute Pipeline Safety (TAA, Caustic, Skin, Volumetrics, Bloom)

Seven findings across the five compute pipelines. One HIGH, one MEDIUM, and two LOW
actionable issues. Two INFOs confirmed correct.

#### NCPS-01 — HIGH: Volumetrics pipeline survives failed initialize_layouts — froxel images remain UNDEFINED
**File:** `crates/renderer/src/vulkan/context/mod.rs:1801-1813`

A failed `VolumetricsPipeline::initialize_layouts` only emits a `log::warn!` and leaves
`self.volumetrics = Some(v)` with froxel images still in `VK_IMAGE_LAYOUT_UNDEFINED`. When
`VOLUMETRIC_OUTPUT_CONSUMED` is flipped to `true`, `dispatch()` will record
`imageStore`/`imageLoad` operations against UNDEFINED-layout images — a
VUID-vkCmdDispatch-None-04115 violation. By contrast, SVGF (lines 1883-1891) and caustic
(lines 1933-1940) both `take()` and destroy their pipeline on `initialize_layouts` failure.

**Fix:** Mirror the SVGF/caustic pattern: on `initialize_layouts` failure, take and destroy
the volumetrics pipeline so `self.volumetrics` becomes `None`. Alternatively, add a
`layouts_initialized: bool` flag to `VolumetricsPipeline` and check it in `dispatch()` with
a hard error return.

#### NCPS-02 — MEDIUM: Volumetrics tlas_written latch never reset — debug_assert ineffective after first frame
**File:** `crates/renderer/src/vulkan/volumetrics.rs:764-774`

`tlas_written[frame]` is set to `true` in `write_tlas()` and never cleared. From the second
frame onwards the latch stays `true` permanently, so a future code path that conditionally
skips `write_tlas` will silently inject a stale TLAS handle from a previous frame into the
injection shader's shadow ray queries.

**Fix:** Reset `self.tlas_written[frame] = false` at the start of `dispatch()` (after the
assert), or at frame start in `draw.rs`.

#### NCPS-03 — LOW: TAA/bloom/volumetrics dispatch() emit redundant HOST→COMPUTE UBO barriers
**File:** `crates/renderer/src/vulkan/taa.rs:671-680`

TAA, bloom, and volumetrics each emit a separate `vkCmdPipelineBarrier` per-dispatch, rather
than folding UBO uploads into the pre-render-pass bulk barrier as SVGF does. Each extra
barrier is a potential GPU pipeline stall on tiler hardware.

**Fix:** Lift TAA and bloom UBO uploads out of `dispatch()` and into the pre-render-pass
upload phase, then drop the per-dispatch `HOST→COMPUTE` barriers.

#### NCPS-04 — LOW: R32_UINT storage image atomic format assumed without device query
**File:** `crates/renderer/src/vulkan/caustic.rs:62`

The caustic accumulator uses `VK_FORMAT_R32_UINT` with `imageAtomicAdd` in
`caustic_splat.comp`. `VK_FORMAT_FEATURE_STORAGE_IMAGE_ATOMIC_BIT` for `R32_UINT` must be
queried via `vkGetPhysicalDeviceFormatProperties`; it is not unconditionally guaranteed.
No runtime assertion confirms it.

**Fix:** Add a `vkGetPhysicalDeviceFormatProperties(R32_UINT)` check in `device.rs` or
`VulkanContext::new` before enabling the caustic pipeline.

#### NCPS-05 — INFO: TAA first-frame safety relies solely on shader guard
**File:** `crates/renderer/src/vulkan/taa.rs:513-524`

The first-frame guard (params.y > 0.5) skips `texelFetch` of uninitialized history.
Correctness depends entirely on a GPU-side conditional with no CPU-side or barrier-based
protection.

**Fix:** Add a unit test asserting `TaaParams::params.y == 1.0` for `frames_since_creation == 0`.

#### NCPS-06 — INFO: SVGF, TAA, bloom histories correctly handle UNDEFINED→GENERAL at construction and resize — VERIFIED CLEAN
**File:** `crates/renderer/src/vulkan/svgf.rs:729-767`

All three pipelines follow the `initialize_layouts` pattern correctly. On resize, both
SVGF and TAA call `initialize_layouts` internally, eliminating the pre-#1031 caller-must-chain
requirement.

#### NCPS-07 — INFO: Skin compute push constants and vertex stride pinned by unit tests — VERIFIED CLEAN
**File:** `crates/renderer/src/vulkan/skin_compute.rs:45-56`

`SkinPushConstants` is 12 bytes with matching GLSL block. Unit tests pin
`PUSH_CONSTANTS_SIZE == 12`, `SKIN_PALETTE_PUSH_CONSTANTS_SIZE == 4`,
`VERTEX_STRIDE_BYTES == 100`. The post-dispatch barrier uses correct stage pairing.

---

### Dimension 8: R1 Material Table Safety

No critical or high-severity issues. `GpuMaterial` struct layout is correctly pinned by tests.
`MaterialTable::intern_by_hash` overflow cap is correctly enforced. Shader struct sync contract
is correctly enforced for `triangle.frag`.

#### R1-MAT-01 — INFO: Stale doc comment: MAX_MATERIALS claims 304 B/entry, struct is 300 B
**File:** `crates/renderer/src/vulkan/scene_buffer/constants.rs:154`

Doc comment reads `16384 × 304 B = 4.75 MB`. Actual size is 300 bytes, making the true
allocation 16384 × 300 = 4.8 MB. The code path is correct; only the comment is wrong.

**Fix:** Update the doc comment to read `16384 × 300 B = 4.8 MB`.

#### R1-MAT-02 — LOW: gpu_material_glsl_field_names_pinned test omits 11 GLSL field needles
**File:** `crates/renderer/src/vulkan/material.rs:1197`

11 GLSL field names declared in `triangle.frag`'s `struct GpuMaterial` are absent from the
needle list: `sparkleR`, `sparkleG`, `sparkleB`, `eyeLeftCenterX/Y/Z`, `eyeRightCenterX/Y/Z`,
`multiLayerInnerScaleU`, `multiLayerInnerScaleV`. A GLSL rename of any of these would not
be caught by this test.

**Fix:** Add the 11 missing needle strings to the `for name in &[...]` list in
`gpu_material_glsl_field_names_pinned`. Confirm trailing punctuation against `triangle.frag`
lines 145–149.

#### R1-MAT-03 — INFO: Release-build FxHash collision silently aliases distinct materials
**File:** `crates/renderer/src/vulkan/material.rs:1045`

64-bit FxHash collision probability is ~1.5×10^-11 per frame for 16384 materials. Negligible
for typical workloads. Debug-build path asserts byte equality. No immediate action required.

#### R1-MAT-04 — INFO: upload_materials overflow guard is debug_assert only, not a hard assert
**File:** `crates/renderer/src/vulkan/scene_buffer/upload.rs:516`

The guard `materials.len() <= MAX_MATERIALS` is a `debug_assert!`. The subsequent `.min(MAX_MATERIALS)`
provides a safety net, but single-point-of-enforcement hard assert would be more defensive.

**Fix:** Consider promoting to hard `assert!`.

#### R1-MAT-05 — INFO: No vec3 alignment hazard — VERIFIED CLEAN
**File:** `crates/renderer/src/vulkan/material.rs:67`

`GpuMaterial` is all-scalar f32/u32. The `gpu_material_alignment_is_4_bytes` test pins
4-byte alignment. No alignment hazard exists.

#### R1-MAT-06 — INFO: Default impl correctly zeros all new Disney/translucency/IOR scalars — VERIFIED CLEAN
**File:** `crates/renderer/src/vulkan/material.rs:273`

All fields added in #1147, #1248, #1249, #1250 have correct neutral defaults. Tests pin
these values.

#### R1-MAT-07 — INFO: Over-cap intern silently degrades to neutral-default material (Once-gated warn)
**File:** `crates/renderer/src/vulkan/material.rs:1066`

Over-cap events return id 0 and log a warning once per session via `Once`. The overflow
counter is only surfaced through the `mem` console command, not via frame HUD or telemetry.

**Fix:** Consider adding a debug-build `debug_assert_eq!(overflow_count, 0)` at frame end
in the `DebugStats` drain system, or exposing `overflow_count` in the frame stats HUD.

---

### Dimension 9: RT IOR-Refraction Safety

Two actionable medium-severity issues. Two low-severity observations. Four informational
items confirmed correct.

#### IOR-01 — MEDIUM: GLASS_IOR division has no clamp guard before ETA computation
**File:** `crates/renderer/shaders/triangle.frag:2147-2148`

`float ETA_AIR_TO_GLASS = 1.0 / GLASS_IOR` performs an unguarded divide by `mat.ior`. If
any future importer writes `ior = 0.0` into a `GpuMaterial`, this yields `ETA = Inf`,
`refract()` returns the zero vector, and the TIR guard may not catch it. The companion
function `dielectricF0FromIor()` has an explicit `max(eta, 1e-3)` clamp (added in #1253)
but that clamp does NOT cover the `ETA_AIR_TO_GLASS` path. Current code paths supply 1.5 or
1.45, so the bug is latent, not live.

**Fix:** Add `float GLASS_IOR = max(mat.ior, 1e-3);` at line 2147, mirroring the existing
`dielectricF0FromIor()` clamp.

#### IOR-02 — MEDIUM: MATERIAL_KIND_GLASS is a local shader const — sync hazard
**File:** `crates/renderer/shaders/triangle.frag:1948-1952`

`const uint MATERIAL_KIND_GLASS = 100u` is declared locally inside the function body rather
than being emitted into `shader_constants.glsl` by `build.rs`. The `#1190` lockstep
framework does NOT cover this value. A renumber in `constants.rs` silently breaks all glass
IOR rendering without a compile error. Same issue for `MATERIAL_KIND_EFFECT_SHADER = 101u`
and `MATERIAL_KIND_NO_LIGHTING = 102u`.

**Fix:** Add these three material-kind values to `shader_constants_data.rs` and emit via
`build.rs` into the generated `shader_constants.glsl` include. Add a `#[test]` analogous
to `instance_flag_bits_match_scene_buffer_consts` to assert numeric equality.

#### IOR-03 — LOW: atomicAdd budget guard permanently overshoots rayBudgetCount on rejected threads
**File:** `crates/renderer/shaders/triangle.frag:2124-2127`

The budget guard calls `atomicAdd` unconditionally before testing whether the claim was
within budget. Rejected threads permanently inflate the counter with no compensating
`atomicSub`. The per-frame CPU reset correctly zeroes it. Visible effect: telemetry overlay
would report inflated values after the cliff is hit. No incorrect ray is fired.

**Fix:** Document the overshoot in the comment block. If a telemetry overlay is added, clamp
the displayed value to `GLASS_RAY_BUDGET`.

#### IOR-04 — LOW: Three distinct bitfields all assign 128u in shader_constants.glsl
**File:** `crates/renderer/shaders/include/shader_constants.glsl:53-85`

`INSTANCE_FLAG_FLAT_SHADING`, `MAT_FLAG_MODEL_SPACE_NORMALS`, and `DBG_VIZ_GLASS_PASSTHRU`
are all defined as `128u`. They are consumed from entirely separate registers so there is no
runtime collision. However, a developer consulting only the generated header could confuse
the shared numeric value.

**Fix:** Add section-separator comments in `build.rs` output grouping defines by their target
register clearly.

#### IOR-05 — INFO: Frisvad buildOrthoBasis singularity correctly handled — VERIFIED CLEAN
**File:** `crates/renderer/shaders/triangle.frag:436`

The sign_z branch correctly handles `dir = (0,0,-1)`. The `dot(refractDir, refractDir) > 0.0001`
guard skips spread when `refractDir` is zero (TIR case).

#### IOR-06 — INFO: Interior glass refraction miss-fallback correctly uses cell ambient — VERIFIED CLEAN
**File:** `crates/renderer/shaders/triangle.frag:550`

Both the reflection miss path and the refraction loop escape path correctly branch on
`jitter.w > 0.5` (the `is_exterior` flag). The stale-comment issue noted in #1125 /
REN-D9-NEW-01 is correctly resolved.

#### IOR-07 — INFO: Glass passthru loop same-texture identity check is coarse but documented — VERIFIED CLEAN
**File:** `crates/renderer/shaders/triangle.frag:2273`

The coarseness (shared texture = both skipped) is explicitly acknowledged and deemed
acceptable. The `REFRACT_PASSTHRU_BUDGET = 2` hard cap ensures the loop terminates in at
most 3 iterations regardless of texture sharing.

---

### Dimension 10: NPC Animation Spawn Safety

NPC/animation spawn safety is in good shape. The critical FLT_MAX sentinel path is correctly
guarded at all three emission sites with dedicated regression tests. Two minor gaps exist in
the particle emission paths.

#### ANIM-01 — INFO: FLT_MAX sentinel handling correctly implemented and tested across all TRS channels — VERIFIED CLEAN
**File:** `crates/nif/src/anim/bspline.rs:326`

The `FLT_MAX_SENTINEL` guard is applied consistently across B-spline evaluation, constant
transform channel fallback, and single-key static fallback. Regression tests for FO3 and FNV
finger-bone cases (#772) are present.

#### ANIM-02 — INFO: B-spline dispatch correctly ungated — not restricted to Skyrim+ only
**File:** `crates/nif/src/blocks/mod.rs:832`

The dispatcher has no version or game guard. The misleading comments saying "Skyrim / FO4
actor KF files" are documentation imprecision only — the code path is reachable on FO3/FNV.

**Fix:** Update the comment at `crates/nif/src/blocks/mod.rs:832` to include FO3/FNV.

#### ANIM-03 — INFO: AnimationClipRegistry dedup is case-insensitive and allocation-optimal — VERIFIED CLEAN
**File:** `crates/core/src/animation/registry.rs:95`

`get_or_insert_by_path()` uses in-place ASCII lowercasing. LRU eviction removes the reverse-map
entry so the next insert for the same key rebuilds rather than returning an empty stub.

#### ANIM-04 — INFO: Particle emitter spawn-rate FLT_MAX sentinel is guarded and tested — VERIFIED CLEAN
**File:** `crates/nif/src/import/walk/mod.rs:714`

`extract_emitter_rate()` uses a `sane()` closure rejecting non-finite, negative, and >= 3.0e38
values. Three regression tests cover: FLT_MAX rejection, valid constant pass-through, and
negative/INFINITY rejection.

#### ANIM-05 — LOW: BSPSysSimpleColorModifier RGBA values skip the finite/FLT_MAX guard
**File:** `crates/nif/src/import/walk/mod.rs:652`

`extract_first_color_curve()` passes `scm.colors[0]` and `scm.colors[2]` directly into
`ParticleColorCurve` without any `is_finite()` or FLT_MAX check. A malformed NIF could
trigger GPU NaN propagation in the particle shader. Risk is low — NaN color causes invisible
particles not geometry explosion.

**Fix:** Add a `is_valid_color(c: [f32; 4]) -> bool` guard analogous to `sane()` in
`extract_emitter_rate()`.

#### ANIM-06 — INFO: Starfield NPC spawn correctly returns None for idle KF path — VERIFIED CLEAN
**File:** `byroredux/src/npc_spawn.rs:249`

`humanoid_default_idle_kf_path(GameKind::Starfield)` returns `None` matching all
post-Skyrim games. Test at line 1590 asserts the `None` return.

#### ANIM-07 — INFO: Animation stack correctly falls back to bind pose when all channels sample to None — VERIFIED CLEAN
**File:** `crates/core/src/animation/stack.rs:329`

`sample_blended_transform()` returns `None` when `total_weight < 0.001`. Callers use
`unwrap_or(Vec3::ZERO / Quat::IDENTITY / 1.0)`.

#### ANIM-08 — LOW: Multi-emitter NIFs: color curve and rate extraction are first-match only
**File:** `crates/nif/src/import/walk/mod.rs:616`

Both `extract_first_color_curve()` and `extract_emitter_rate()` scan and return on the
first matching block. NIFs with multiple `NiParticleSystem` nodes all share the first
emitter's rate and color ramp. A correctness gap, not a safety hazard.

**Fix:** When a multi-emitter regression surfaces, walk each `NiParticleSystem.modifier_refs`
list independently.

---

### Dimension 11: NIFAL Canonical Translation Safety

The NIFAL material translation boundary is sound. `translate_material` is the single creation
point for renderer-bound `Material` components, `resolve_pbr` runs unconditionally on every
code path, and PBR scalar guarantees are well-tested. Three lower-severity gaps exist in the
particles slice.

#### NIFAL-S1 — INFO: Material PBR scalars: translate_material is single boundary, resolve_pbr always runs — VERIFIED CLEAN
**File:** `byroredux/src/material_translate.rs`

Both `cell_loader/spawn.rs:873` and `scene/nif_loader.rs:816` route through `translate_material`.
`resolve_pbr` clamps metalness to [0,1] and roughness to [0.04,1] at every material
construction exit.

#### NIFAL-S2 — INFO: BGSM merge path uses clamped arithmetic for metalness/roughness override — VERIFIED CLEAN
**File:** `byroredux/src/asset_provider.rs:1110`

BGSM v>=8 PBR path derives metalness and roughness with explicit `.clamp()` before writing
to `mesh.metalness_override / roughness_override`.

#### NIFAL-S3 — MEDIUM: extract_emitter_params passes NIF binary scalars with no finite/positive guard
**File:** `crates/nif/src/import/walk/mod.rs:688`

`extract_emitter_params` copies `p.speed`, `p.speed_variation`, `p.initial_radius`,
`p.life_span`, `p.life_span_variation` directly from `EmitterBaseParams` with no `sane()`
filter. `apply_emitter_params` then copies these directly onto `ParticleEmitter.life`,
`start_size`, etc. A corrupt NIF with `initial_radius = NaN` or `life_span = 0.0` will write
NaN/zero into the live `ParticleEmitter`. The particle system's `life.max(0.05)` guard runs
after accumulator arithmetic, so a NaN `em.rate` still poisons `spawn_accumulator`.

**Fix:** Add a sane-filter inside `extract_emitter_params` checking: `life_span.is_finite() &&
life_span > 0.0`; `initial_radius.is_finite() && initial_radius >= 0.0`; `speed.is_finite()`.
Mirror the pattern in `extract_emitter_rate`. At minimum gate the `apply_emitter_params`
call site with a `None` return from `extract_emitter_params` when any physical scalar is
non-finite.

#### NIFAL-S4 — LOW: Collision shape radii/half-extents carry raw NIF binary floats with no finite guard
**File:** `crates/nif/src/import/collision.rs:291`

`resolve_shape_inner` propagates `s.radius * scale` directly to `CollisionShape::Ball`,
`Capsule`, `Cylinder` without `is_finite()` check. `BhkBoxShape.dimensions` go through
`havok_to_engine() * scale` to `CollisionShape::Cuboid::half_extents` without validation.
The only existing guard is on `BhkMeshShape.scale`. A corrupt NIF with NaN radius
propagates to the physics integration layer.

**Fix:** Add `is_finite()` guards at `CollisionShape` construction sites for radius and
dimension array elements, mirroring the existing `BhkMeshShape` pattern. Return `None`
on non-finite values so the trimesh fallback fires.

#### NIFAL-S5 — INFO: NiPSysGrowFadeModifier.base_scale has no finite guard before size computation
**File:** `crates/nif/src/import/walk/mod.rs:687`

`base_scale` from the NIF binary is passed as `Some(...)` to `ImportedEmitterParams.base_scale`
without `is_finite()` check. A NaN or infinite `base_scale` would produce NaN `start_size /
end_size` on the `ParticleEmitter`.

**Fix:** Wrap the `base_scale` extraction:
`base_scale = base_scale.filter(|s| s.is_finite() && *s > 0.0);`

#### NIFAL-S6 — INFO: Glass roughness override is always finite — VERIFIED CLEAN
**File:** `byroredux/src/helpers.rs:70`

`GLASS_ROUGHNESS` is a `const f32 = 0.10`. No NaN risk.

#### NIFAL-S7 — INFO: particle_system spawn loop guards life with .max(0.05) but not size or rate
**File:** `byroredux/src/systems/particle.rs:332`

`spawn_accumulator += em.rate * dt` is not guarded against NaN `em.rate`.
`em.start_size` is passed directly to spawned particles without a finite guard.

**Fix:** Add a guard at the top of the per-emitter block:
`if !em.rate.is_finite() || em.rate <= 0.0 || !em.start_size.is_finite() || em.start_size <= 0.0 { continue; }`

---

### Dimension 12: Debug-UI Vulkan Teardown Safety

The debug-UI egui overlay integration is mostly sound: `device_wait_idle` precedes all
`EguiPass` teardown, `DebugUiState` drops before `VulkanContext`, and `Renderer::drop` cleans
up all GPU resources. Two actionable medium issues and two low issues were found.

#### EGUI-01 — MEDIUM: egui set_textures uses main draw command pool instead of transfer pool
**File:** `crates/renderer/src/vulkan/context/draw.rs:2970`

`EguiPass::dispatch` is passed `self.command_pool` (the main per-frame draw pool, created
with `RESET_COMMAND_BUFFER`). Inside `egui-ash-renderer`'s `set_textures`, a one-shot
command buffer is allocated from this pool and submitted while the main frame command buffer
is still recording. The codebase explicitly created `transfer_pool` (`TRANSIENT`) for
one-time uploads to avoid Vulkan external-sync contention. All other one-shot uploads use
`self.transfer_pool`; the egui path is the sole exception.

**Fix:** Pass `self.transfer_pool` instead of `self.command_pool` to `EguiPass::dispatch`.

#### EGUI-02 — LOW: egui texture upload bypasses the graphics_queue Mutex
**File:** `crates/renderer/src/vulkan/context/draw.rs:2962`

At lines 2962–2965, the `graphics_queue` Mutex guard is acquired, the underlying `vk::Queue`
(a `Copy` type) is dereferenced into a local, and the guard is immediately dropped. The bare
queue handle is then passed into `EguiPass::dispatch` which calls `vkQueueSubmit` without
the Mutex held, inconsistent with CONC-D2-NEW-01.

**Fix:** Restructure `EguiPass::dispatch` to accept `&Arc<Mutex<vk::Queue>>` and hold the
lock across `set_textures + cmd_draw`, matching the pattern used for the main `queue_submit`.

#### EGUI-03 — LOW: EguiPass::destroy() does not flush pending_free before Renderer drop
**File:** `crates/renderer/src/vulkan/egui_pass.rs:194`

`EguiPass::destroy` destroys framebuffers and the render pass but does not call
`self.renderer.free_textures(&self.pending_free)` for textures deferred from the last
rendered frame. `Renderer::drop` does drain `managed_textures`, so no resource leak. But the
`free_textures` path (calling `vkFreeDescriptorSets`) is skipped, leaving the descriptor
pool accounting mismatched until the pool itself is destroyed.

**Fix:** Add `if !self.pending_free.is_empty() { let _ = self.renderer.free_textures(&self.pending_free); }`
at the start of `EguiPass::destroy`, before framebuffer/render-pass destruction.

#### EGUI-04 — LOW: egui render pass has no outgoing EXTERNAL subpass dependency
**File:** `crates/renderer/src/vulkan/egui_pass.rs:244`

`create_render_pass` declares only an incoming `EXTERNAL → subpass-0` dependency. There is
no outgoing `subpass-0 → EXTERNAL` dependency. The final layout transition from
`COLOR_ATTACHMENT_OPTIMAL` to `PRESENT_SRC_KHR` relies on Vulkan's implicit external
dependency. Technically correct for presentation, but omitting the explicit dependency is a
common source of validation warnings on some drivers.

**Fix:** Add an outgoing `SubpassDependency` with `src_subpass=0`, `dst_subpass=SUBPASS_EXTERNAL`,
`src_stage=COLOR_ATTACHMENT_OUTPUT`, `dst_stage=BOTTOM_OF_PIPE`, `src_access=COLOR_ATTACHMENT_WRITE`,
`dst_access=0`.

#### EGUI-05 — INFO: EguiPassConfig is dead code — defined but never constructed
**File:** `crates/debug-ui/src/lib.rs:208`

`EguiPassConfig` is a public struct with `pub` fields referencing `ash` types forcing a
dependency on `ash` + `gpu-allocator` in the `debug-ui` crate even though it never uses them.

**Fix:** Remove `EguiPassConfig` from `crates/debug-ui/src/lib.rs`.

#### EGUI-06 — INFO: App struct field ordering correctly sequences DebugUiState drop before VulkanContext — VERIFIED CLEAN
**File:** `byroredux/src/main.rs:266`

Rust drops struct fields in reverse declaration order. `debug_ui` (declared after `renderer`)
drops before `renderer`. `DebugUiState` holds no Vulkan handles. `VulkanContext::drop`
explicitly calls `device_wait_idle()` before destroying `EguiPass`.

**Fix:** Add a comment near the `renderer` and `debug_ui` fields noting that drop ordering
is load-bearing and must not be reordered.

---

## Prioritized Fix Order

### HIGH (Fix before or during current milestone)

1. **NCPS-01** — Volumetrics pipeline survives failed `initialize_layouts`, leaving froxel
   images in `VK_IMAGE_LAYOUT_UNDEFINED`. Mirror the SVGF/caustic `take()`-and-destroy
   pattern on failure. Will trigger VUID-vkCmdDispatch-None-04115 the moment
   `VOLUMETRIC_OUTPUT_CONSUMED` is flipped.

### MEDIUM (Fix soon — degraded behavior or latent UB risk)

2. **IOR-01** — Add `float GLASS_IOR = max(mat.ior, 1e-3);` clamp before the ETA division
   in `triangle.frag:2147`. Zero-IOR would produce Inf refraction. Trivial one-liner.

3. **IOR-02** — Emit `MATERIAL_KIND_GLASS/EFFECT_SHADER/NO_LIGHTING` from `build.rs` into
   `shader_constants.glsl`. Add lockstep tests. Renumber in Rust currently breaks all glass
   rendering silently.

4. **NCPS-02** — Reset `tlas_written[frame] = false` at the start of `dispatch()` in
   `volumetrics.rs` to give the latch correct per-frame semantics.

5. **NIFAL-S3** — Add `sane()` filter in `extract_emitter_params` for `life_span`,
   `initial_radius`, `speed`. NaN propagates to spawn accumulator and GPU particle buffer.

6. **VKC-001** — Resolve `PipelineStageFlags::NONE` in sync1 barriers. Either require
   `synchronization2` as a mandatory feature (preferred — matches the existing RT/VRAM
   baseline) or add per-site fallback to `TOP_OF_PIPE`.

7. **VKC-002** — Upgrade scratch-alignment `debug_assert` to a run-time check or explicit
   alignment round-up. Release builds silently violate VUID-03715.

8. **VKC-003** — Add `FenceWaitProof` token (or defensive `device_wait_idle`) to the TLAS
   resize path. Current correctness relies on comment-documented invariant only.

9. **MEM-01** — Set a default `max_entries = 2048` for `NifImportRegistry`. Unlimited cache
   causes unbounded RAM growth in long streaming sessions.

10. **MEM-02** — Add `u32::MAX` overflow guard to `MeshRegistry::upload` before the
    `len() as u32` cast. `TextureRegistry` already has an equivalent guard.

11. **MEM-03** — Call `device_wait_idle()` before the early-return in the allocator Arc leak
    path. Audit `SharedAllocator` Arc clone sites to prevent this path from triggering.

12. **TS-02** — Add `BYRO_LOCK_ORDER_CHECK=1` to at least one CI job running `cargo test`
    with the parallel-scheduler feature. Zero code change required.

13. **EGUI-01** — Pass `self.transfer_pool` instead of `self.command_pool` to
    `EguiPass::dispatch`. One-line fix.

14. **RT-01** — Apply `N * 0.05` bias to the caustic floor-ray origin in `water.frag:557`
    and raise `tMin` to `0.05`. Fixes self-intersection on the water plane.

15. **SAFE-U1** — Update the stale doc comment on `BuiltinType` from "transmute" to
    "checked match". One-line fix.

16. **SAFE-U2** — Add `// SAFETY:` comment to the `slice::from_raw_parts` call on `WaterPush`
    in `water.rs:466`.

17. **SAFE-U3** — Add `// SAFETY:` comment to the inner `unsafe {}` block in
    `debug_callback`.

18. **SAFE-U4** — Add `// SAFETY:` comment to `CStr::from_ptr` in
    `check_validation_layer_support`.

19. **SAFE-U5** — Add `debug_assert!` for the pointer-arithmetic in-bounds invariant in
    `upload_lights` and amend the SAFETY comment.

20. **SAFE-U6** — Adopt tiered SAFETY comment policy. Start with `gpu_timers.rs` and
    `acceleration/blas_skinned.rs`.

### LOW (Fix when convenient — minor inconsistencies, suboptimal patterns)

21. **VKC-004** — Force full TLAS BUILD when `instance_count < built_primitive_count` on the
    UPDATE path.

22. **VKC-005** — Call `device_wait_idle()` before the early-return leak path in
    `VulkanContext::drop`.

23. **RT-02** — Raise `tMin` to `0.05` on the caustic shadow ray to match the `N * 0.05`
    bias.

24. **RT-03** — Pass `vWorldPos + N * 0.05` as origin to both `traceWaterRay` call sites.

25. **RT-04** — Add a `// TODO(M29.3): add VERTEX_BUFFER flag here` comment at the skin
    output buffer creation site.

26. **RT-05** — Promote `instance_custom_index` overflow guard to hard `assert!` or add a
    compile-time assertion that `MAX_INSTANCES < (1 << 24)`.

27. **NCPS-03** — Lift TAA and bloom UBO uploads into the pre-render-pass upload phase,
    dropping per-dispatch `HOST→COMPUTE` barriers.

28. **NCPS-04** — Add `vkGetPhysicalDeviceFormatProperties(R32_UINT)` check in `device.rs`
    before enabling the caustic pipeline.

29. **MEM-04** — Replace BGSM material cache flush-on-overflow with LRU eviction.

30. **MEM-05** — Add `unsafe trait AnyBitPattern {}` bound to `read_pod_vec` to shift
    unsafety from prose comment to the type system.

31. **MEM-06** — Add depth counter to `resolve_shape_inner`, returning `None` at depth > 64.

32. **TS-03** — Migrate undeclared systems to include `Access` declarations. Drive
    `undeclared_parallel_count()` to zero.

33. **TS-06** — Add comments at all `allocator.lock().allocate()` + error-cleanup sites
    referencing the #1163 fix.

34. **TS-08** — Document scheduler panic policy explicitly. Consider `catch_unwind` per
    rayon task as medium-term quality item.

35. **FFI-01** — Remove the dead `engine_info` `extern "Rust"` declaration, or add a C++
    test caller.

36. **FFI-02** — Add a `#[cfg(test)]` module to the cxx bridge with at least one test.

37. **R1-MAT-02** — Add 11 missing GLSL field needles to `gpu_material_glsl_field_names_pinned`
    test.

38. **EGUI-02** — Restructure `EguiPass::dispatch` to hold the `graphics_queue` Mutex across
    `set_textures` and `cmd_draw`.

39. **EGUI-03** — Add `free_textures(&self.pending_free)` flush at the start of
    `EguiPass::destroy`.

40. **EGUI-04** — Add an outgoing `subpass-0 → EXTERNAL` subpass dependency to the egui
    render pass.

41. **EGUI-05** — Remove dead `EguiPassConfig` struct from `crates/debug-ui/src/lib.rs`.

42. **IOR-03** — Document the atomicAdd overshoot behavior; add clamp in any future
    telemetry overlay that reads the budget counter.

43. **IOR-04** — Add section-separator comments in `build.rs` output to clarify that 128u
    appears in three separate registers.

44. **ANIM-05** — Add `is_valid_color()` guard in `extract_first_color_curve()` analogous
    to `sane()` in `extract_emitter_rate()`.

45. **ANIM-08** — Defer; document the first-match limitation with a comment referencing this
    finding until a multi-emitter regression surfaces.

46. **NIFAL-S4** — Add `is_finite()` guards at `CollisionShape` construction sites for
    radius and dimension array elements.

47. **NIFAL-S5** — Wrap `base_scale` extraction with `.filter(|s| s.is_finite() && *s > 0.0)`.

48. **NIFAL-S7** — Add finite/positive guards for `em.rate` and `em.start_size` at the top
    of the per-emitter block in `particle_system`.

49. **SAFE-U7** — Add `// SAFETY:` comment to `String::as_bytes_mut` call in the unit test.

50. **ANIM-02** — Update the misleading "Skyrim / FO4 only" comments on the B-spline
    dispatcher and `anim/transform.rs:47` to include FO3/FNV.

---

## Closed / Verified Clean Items

The following 25 items were audited and found to require no corrective action:

| ID | Dimension | Finding |
|----|-----------|---------|
| SAFE-U8 | Unsafe Rust | `BuiltinType::from_u32` is a fully checked match; no transmute |
| SAFE-U9 | Unsafe Rust | ECS query raw-pointer caching is sound and well-documented |
| SAFE-U10 | Unsafe Rust | `read_pod_vec` byte-cast is sound and well-commented |
| MEM-07 | Memory Safety | NIF stream allocation guards are comprehensive and correct |
| MEM-08 | Memory Safety | Cell unload GPU resource sweep is complete and correctly ordered |
| MEM-09 | Memory Safety | VulkanContext Drop ordering is correct (allocator before device) |
| TS-01 | Thread Safety | TypeId-sorted multi-lock acquisition consistently enforced |
| TS-04 | Thread Safety | Vulkan queue Mutex pattern is correct and consistent |
| TS-05 | Thread Safety | Allocator and queue locks are never held concurrently |
| TS-07 | Thread Safety | No unsafe `Send`/`Sync` impls anywhere in the codebase |
| TS-09 | Thread Safety | Debug server single-lock patterns are correct |
| FFI-04 | FFI Safety | cxx-bridge is a minimal placeholder with near-zero FFI surface |
| RT-06 | RT Pipeline | BLAS result/compaction buffers use correct minimal usage flags |
| NCPS-06 | Compute Pipeline Safety | SVGF, TAA, bloom histories correctly handle UNDEFINED→GENERAL |
| NCPS-07 | Compute Pipeline Safety | Skin compute push constants and vertex stride pinned by unit tests |
| R1-MAT-05 | Material Table | No vec3 alignment hazard — all-scalar f32/u32 struct |
| R1-MAT-06 | Material Table | Default impl correctly zeros all Disney/translucency/IOR scalars |
| IOR-05 | RT IOR-Refraction | Frisvad buildOrthoBasis singularity at (0,0,-1) correctly handled |
| IOR-06 | RT IOR-Refraction | Interior glass refraction miss-fallback correctly uses cell ambient |
| IOR-07 | RT IOR-Refraction | Glass passthru loop same-texture identity check is documented |
| ANIM-01 | NPC Animation | FLT_MAX sentinel correctly guarded at all three emission sites |
| ANIM-03 | NPC Animation | AnimationClipRegistry dedup is case-insensitive and allocation-optimal |
| ANIM-04 | NPC Animation | Particle emitter spawn-rate FLT_MAX sentinel guarded and tested |
| ANIM-06 | NPC Animation | Starfield NPC spawn correctly returns None for idle KF path |
| ANIM-07 | NPC Animation | Animation stack correctly falls back to bind pose for empty channels |
| NIFAL-S1 | NIFAL Translation | translate_material is single boundary; resolve_pbr always runs |
| NIFAL-S2 | NIFAL Translation | BGSM merge path uses clamped arithmetic for PBR override values |
| NIFAL-S6 | NIFAL Translation | Glass roughness override is always finite (const 0.10) |
| EGUI-06 | Debug-UI Teardown | App field ordering correctly sequences DebugUiState drop before VulkanContext |
