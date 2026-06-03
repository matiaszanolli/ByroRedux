---
description: "Safety audit — unsafe blocks, memory leaks, undefined behavior, Vulkan spec violations"
---

# Safety Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

## Dimensions

### 1. Unsafe Rust Blocks
- List every `unsafe` block in the codebase
- For each: is there a safety comment? Is the invariant actually upheld?
- Common risks: dangling pointers, aliasing violations, uninitialized memory
- Focus on World::get() raw pointer extension and any FFI boundaries
- `crates/sfmaterial` enum decode: `BuiltinType::from_u32` (`src/types.rs`) MUST stay a checked `match` over the `0xFFFFFF##` tags with an `Err` arm for unmatched patterns — the doc-comment's "transmute into this enum" wording is aspirational, NOT the impl. An actual `std::mem::transmute` of an unmatched `#[repr(u32)]` byte pattern is UB; verify the match (and its `_ => Err`) survives any "optimization"

### 2. Vulkan Spec Compliance
- All vkCreate*/vkDestroy* paired correctly
- No use-after-destroy (check Drop ordering)
- Validation layers enabled in debug — run and check for ANY errors
- Queue submission ordering correct (wait before signal)
- Acceleration structure builds: correct geometry flags, valid device addresses
- TLAS UPDATE mode: instance count and geometry count must match original BUILD
- Ray query extension: `VK_KHR_ray_query` enabled before use, feature gate checked

### 3. Memory Safety
- GPU memory: all allocations freed before allocator drop
- GPU memory: allocator dropped before device destroy
- **`AllocatorResource` ECS drop ordering (#1406, `299e6a84`)**: `AllocatorResource` must be removed from the ECS `World` BEFORE `VulkanContext::drop()` fires. The `gpu-allocator` holds a live `Arc<Device>`; if the `World` outlives the `VulkanContext`, the allocator's `Drop` invokes driver calls against a destroyed logical device (use-after-free). Verify the main loop / app handler removes the resource before dropping the renderer; a panic unwind that skips this removal is an equally valid failure path.
- **TLAS resize device_wait_idle (#1390, `a7e1502b`)**: the TLAS resize path must call `device.device_wait_idle()` before freeing the old allocation. Without it, the GPU may still be consuming the old TLAS scratch during the free. Latent today but would materialise under a future resize-under-load refactor. Verify the wait is present in `acceleration/tlas.rs` in the resize branch.
- GPU memory: BLAS scratch buffer, TLAS instance/result buffers, G-buffer images, SVGF history buffers, TAA per-frame-in-flight history images, caustic accumulator images, per-skinned-entity SkinSlot output buffers, MaterialBuffer SSBO (R1) all tracked and freed
- Cell streaming (`byroredux/src/streaming.rs`): cell-loaded resources (NIF imports, BLAS entries, textures) freed when cell unloads — verify no leak path through the async pre-parse worker thread (M40 milestone closed, but the unload leak path is live since exterior streaming is real)
- CPU memory: no unbounded growth (Vec without clear, HashMap without remove)
- Material table dedup map (R1): cleared per frame or pooled? Either is fine, but unbounded HashMap growth across cells is a leak
- Stack overflow risk: no deep recursion without bounds

### 4. Thread Safety
- RwLock: no potential for deadlock (TypeId ordering enforced?)
- Arc<Mutex<Allocator>>: lock held for minimum duration?
- Send + Sync bounds on Component and Resource traits correct?

### 5. FFI Safety (cxx bridge)
- C++ exceptions: does cxx handle them correctly?
- String lifetime: Rust strings passed to C++ — valid for duration of call?
- No raw pointer exchange across FFI without clear ownership

### 6. RT Pipeline Safety
- BLAS/TLAS device address queries: buffers must have SHADER_DEVICE_ADDRESS usage
- Global vertex/index SSBO: `instance_custom_index` bounds not checked on GPU — verify CPU-side encoding is correct
- Ray query origin bias: self-intersection avoidance (tMin > 0 or offset along normal)
- TLAS refit: `last_blas_addresses` comparison must handle mesh registry changes (add/remove)
- Skin compute (GPU pre-skinning, milestone closed): per-skinned-mesh output buffer usage flags include `STORAGE_BUFFER` AND `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`, plus `VERTEX_BUFFER` (#681 / `MEM-2-6` regression note). Wrong usage mask = device lost — the regression guard is live regardless of milestone status.
- Skin BLAS refit: vertex/geometry count must match the original BUILD; bone count change forces full rebuild
- Bone palette: `MAX_TOTAL_BONES` overflow guard in `byroredux/src/render/skinned.rs` (`Once`-gated warn at the bone-palette emit site, post-#1115 split) actually fires (silent truncation past cap was the M29 regression). Regression-guard tests live at `byroredux/src/render/bone_palette_overflow_tests.rs`

### 7. New Compute Pipeline Safety (TAA, Caustic, Skin)
- TAA history images held in `GENERAL` layout (storage write + sampled read coexist); no UNDEFINED transitions per frame
- TAA `should_force_history_reset` path produces no NaN reads, weight α forced to 1.0 on first frame
- Caustic accumulator R32_UINT: `imageAtomicAdd` only — never float storage, no race
- Caustic CLEAR-before-COMPUTE invariant: missing clear = persistent ghost contributions across frames
- Skin compute push constants ≤ 128 B (Vulkan-guaranteed minimum); current `SkinPushConstants` is 12 B
- SPIR-V reflection (`reflect.rs::validate_set_layout`): Rust descriptor layout MUST match shader-declared bindings — reflection mismatch is the only sound layer for catching binding drift before runtime
- Volumetrics (`crates/renderer/src/vulkan/volumetrics.rs`, froxel grid `volumetrics_inject.comp` / `volumetrics_integrate.comp`): per-frame-in-flight 3D froxel volumes held in `GENERAL` (storage write + sampled read); `initialize_layouts` does the one-time UNDEFINED→GENERAL transition AND clears to `(rgb=0, a=1)` — same persistent-ghost-accumulation class as the caustic CLEAR-before-COMPUTE invariant. Per-froxel TLAS shadow ray fires once; verify no UNDEFINED transition leaks in per frame. (Note: dispatch is currently gated behind a `false` const while per-froxel banding is fixed — verify callers honor the gate.)
- Bloom (`crates/renderer/src/vulkan/bloom.rs`, `bloom_downsample.comp` / `bloom_upsample.comp`): the down/up `B10G11R11_UFLOAT_PACK32` mip pyramids live in `GENERAL` for their entire lifetime; `initialize_layouts` does the one-time UNDEFINED→GENERAL barrier for every mip in every frame slot (`NONE` srcStageMask, no prior writes). Same UNDEFINED-transition class as TAA — a missed mip in the init barrier is a validation error / undefined read

### 8. R1 Material Table Safety
- `GpuMaterial` (in `crates/renderer/src/vulkan/material.rs`) size pinned at **300 B** by the `gpu_material_size_is_260_bytes` test (the test NAME still says 260 for grep continuity, but the asserted constant is 300 — the struct grew via the BGSM translucency suite `#1147` at offsets 260–276 and the Disney-BSDF lobe scalars + IOR at offsets 280–296, `#1248`/`#1249`/`#1250`; was 272 B until #804 / R1-N4 dropped `avg_albedo`, then 260 B until those additions) — a stale 260/272 in audit prose or a test-name-vs-asserted-size mismatch both mean the GPU-side struct is reading wrong bytes
- Per-field offset pin (`gpu_material_field_offsets_match_shader_contract`, #806): every named field's byte offset asserted against the shader contract. Size-only pin cannot catch within-vec4 reorders (e.g. swap `texture_index ↔ normal_map_index` is invisible to size, lethal at runtime). Adding a field WITHOUT updating this assertion is a regression
- ALL fields scalar f32/u32 — never `[f32; 3]` (std430 vec3 alignment ≠ tightly-packed Rust). This includes the newest scalars: the BGSM translucency suite (`translucency_subsurface_r/g/b`, `translucency_transmissive_scale`, `translucency_turbulence`) and the Disney lobe (`ior`, `subsurface`, `sheen`, `sheen_tint`, `anisotropic`) — each is a flat f32, never folded back into a `vec3`/`vec4` pair
- Named pad fields explicitly zeroed (no uninit bytes leak into the byte-`Hash`/`Eq` dedup — `GpuMaterial::as_bytes` hashes the raw 300 B representation, so the larger struct only widens the blast radius of any alignment hole). The Disney/translucency scalars must be zeroed in `GpuMaterial::default()` so default materials still dedup to slot 0
- `material_id` bounds: GpuInstance.material_id used as SSBO index — CPU must guarantee in-range; GPU has no bounds check
- `MaterialTable::intern` cap (#797 SAFE-22): over `MAX_MATERIALS = 16384` (raised from 4096 in `7823eb59`; `crates/renderer/src/vulkan/scene_buffer/constants.rs`) distinct interns return id `0` with one-shot warn — no SSBO over-index, no DEVICE_LOST. Verify the cap fires AND the SSBO upload (`scene_buffer/upload.rs::upload_materials` — `materials.len().min(MAX_MATERIALS)`) truncates to `MAX_MATERIALS`. Mismatch between intern cap and upload truncation is the class of bug the cap was added to prevent
- `ui.vert` MaterialBuffer read offsets stay in lockstep with `triangle.frag` (#785 R-N1 was a stale-hunk regression of #776 reading wrong bytes — name `ui.vert` explicitly in any R1 audit)

### 9. RT IOR-Refraction Safety (Sessions 27–29)
- Glass-passthrough infinite loop (#789): texture-equality identity check at the refraction hit prevents unbounded recursion when two coincident glass surfaces share the same albedo/normal-map descriptor pair. Verify the check is still in place — a regression here is a frame-time hang under any cell with paired glass
- Frisvad orthonormal basis (#820 / REN-D9-NEW-01): the `cross(N, world-up)` construction degenerates near vertical surfaces (zero-length basis → NaN ray). Verify Frisvad is the active code path for IOR refraction roughness spread
- Glass ray budget bounded: `GLASS_RAY_BUDGET = 8192` (raised from 512 in 9a4dc15) — the cap exists to prevent runaway recursion, not as a quality knob. Verify the budget is enforced at every call site
- IOR miss fallback for interiors uses cell-ambient (bb53fd5), NOT the global sky tint — open-sky leakage into dungeons is a visible regression
- `DBG_VIZ_GLASS_PASSTHRU = 0x80` debug bit kept as a permanent diagnostic; verify the bit position has not collided with new debug-flag additions (full catalog: `crates/renderer/src/shader_constants_data.rs::DBG_*`, mirrored into the auto-generated `crates/renderer/shaders/include/shader_constants.glsl` consumed by `triangle.frag` via `#include`)

### 10. NPC / Animation Spawn Safety (M41.0 long-tail)
- B-spline pose-fallback (#772): NPC vanishing under FNV `BSPSysSimpleColorModifier` particle stacks that share keyframe time-zero with the actor's animation player must be gated on a `FLT_MAX` sentinel. Removing the gate causes whole-NPC disappearance, not just a stuck pose — verify the sentinel is still wired
- AnimationClipRegistry dedup (#790): registry deduplicates by lowercased path so cell streaming does not grow it unboundedly. Without dedup, one full keyframe set leaks per cell load — observable as steady RAM growth across exterior streaming. Verify case-insensitive interning is preserved
- B-splines are reachable on FNV / FO3 too (`feedback_bspline_not_skyrim_only.md`) — do NOT rule out `NiBSplineCompTransformInterpolator` audits by game era. Skyrim-only assumption is a stale premise that has bitten this audit before
- Starfield content is now WALKABLE (Cydonia) — do NOT short-circuit spawn-safety reasoning with a "no SF content exercises this path" assumption; SF cells reach the spawn / animation path like any other game

### 11. NIFAL Canonical-Translation Safety (NaN sentinels at the import boundary)
*See also `/audit-nifal` — the dedicated NIFAL canonical-translation-layer audit. This dimension covers only the safety/UB facet (NaN-on-GPU, unbounded scalars); leave correctness-of-mapping to that audit.*
- The single import boundary `byroredux/src/material_translate.rs::translate_material` deliberately seeds `f32::NAN` into `Material.metalness` / `Material.roughness` (the unresolved sentinels: `mesh.metalness_override.unwrap_or(f32::NAN)` / `roughness_override.unwrap_or(f32::NAN)`). `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`) is the ONLY thing that detects (`is_nan`) and clamps those sentinels (`metalness.clamp(0.0,1.0)`, `roughness.clamp(0.04,1.0)`) before the value reaches `GpuMaterial`. A translation path that skips `resolve_pbr()` ships a NaN into the SSBO — NaN-on-GPU is the UB class this dimension exists to catch. `Material.metalness`/`roughness` are now plain `f32` (no `Option`), so a missing resolve is silent until it lands on the GPU
- Verify EVERY producer of a renderer-bound `Material` runs `resolve_pbr()` (or constructs already-finite values); the `static_meshes.rs` fallback path constructs finite defaults directly — confirm it still does
- Collision translate (`crates/nif/src/import/collision.rs`) now covers `BhkMultiSphereShape` and `BhkConvexListShape` as additional NIFAL boundaries — verify their emitted half-extents / radii / sphere centers are finite and bounded (a NaN/inf shape param propagates into the physics + BLAS build)
- Typed particle blocks (`crates/nif/src/blocks/particle.rs`: `NiPSysEmitter` / `NiPSysEmitterCtlr` / `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier`) feed `extract_emitter_params` / `extract_emitter_rate` (`crates/nif/src/import/walk/mod.rs`) → `systems::particle::apply_emitter_params` (`byroredux/src/systems/particle.rs`). Verify emitter rate / lifespan / size scalars are finite and non-negative at the extract boundary — an unbounded or NaN emitter rate is an unbounded-allocation / NaN-transform risk downstream

### 12. debug-ui (egui overlay) Vulkan Teardown Safety
- `crates/debug-ui` (`src/lib.rs`) holds an `ash::Device`, a `vk::RenderPass`, and a shared `Arc<Mutex<gpu_allocator::vulkan::Allocator>>`; it is owned by the engine main loop as `byroredux::main.rs`'s `debug_ui: Option<byroredux_debug_ui::DebugUiState>`
- The crate wraps `egui-ash-renderer`, which manages its own descriptor pool + per-texture images. Those MUST be freed before the engine destroys the `ash::Device` — ties into Dimension 3 ("allocator dropped before device destroy"). Verify `DebugUiState` Drop / explicit teardown runs ahead of `VulkanContext`'s device-destroy, not after
- The allocator is SHARED (`Arc<Mutex<…>>`) with the renderer — verify the lock is held for minimum duration during egui texture upload (Dimension 4 lock-duration); a long hold on the shared allocator mutex stalls the render thread

1. Use Grep to find all `unsafe` blocks in `crates/` (`.rs` files)
2. Read each unsafe block and its surrounding context
3. Check Vulkan resource pairing with Drop implementations
4. Check RT-specific safety (acceleration structures, device addresses, SSBO indexing)
5. Check new-compute-pipeline safety (TAA, caustic, skin compute, volumetrics, bloom)
6. Check R1 material table layout invariants (300 B size pin + per-field offset pin + intern cap; Disney/translucency scalars stay flat f32 + zeroed in Default)
7. Check IOR-refraction safety (glass loop, Frisvad, ray budget, interior fallback)
8. Check NPC / animation spawn safety (B-spline FLT_MAX, AnimationClipRegistry dedup)
9. Check NIFAL canonical-translation safety (no NaN sentinel reaches the GPU — every Material producer runs `resolve_pbr`; collision + particle extract boundaries emit finite, bounded scalars) — see also `/audit-nifal`
10. Check debug-ui (egui overlay) Vulkan teardown ordering + shared-allocator lock duration
11. Save report to `docs/audits/AUDIT_SAFETY_<TODAY>.md`
