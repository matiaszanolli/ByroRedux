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
- GPU memory: BLAS scratch buffer, TLAS instance/result buffers, G-buffer images, SVGF history buffers, TAA per-frame-in-flight history images, caustic accumulator images, per-skinned-entity SkinSlot output buffers, MaterialBuffer SSBO (R1) all tracked and freed
- M40 streaming: cell-loaded resources (NIF imports, BLAS entries, textures) freed when cell unloads — verify no leak path through the async pre-parse worker thread
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
- M29.5 skin compute: per-skinned-mesh output buffer usage flags include `STORAGE_BUFFER` AND `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`; M29.3 also re-adds `VERTEX_BUFFER` (#681 / `MEM-2-6` regression note). Wrong usage mask = device lost.
- Skin BLAS refit: vertex/geometry count must match the original BUILD; bone count change forces full rebuild
- Bone palette: `MAX_TOTAL_BONES` overflow guard at `render.rs:204` actually fires (silent truncation past cap was the M29 regression)

### 7. New Compute Pipeline Safety (TAA, Caustic, Skin)
- TAA history images held in `GENERAL` layout (storage write + sampled read coexist); no UNDEFINED transitions per frame
- TAA `should_force_history_reset` path produces no NaN reads, weight α forced to 1.0 on first frame
- Caustic accumulator R32_UINT: `imageAtomicAdd` only — never float storage, no race
- Caustic CLEAR-before-COMPUTE invariant: missing clear = persistent ghost contributions across frames
- Skin compute push constants ≤ 128 B (Vulkan-guaranteed minimum); current `SkinPushConstants` is 12 B
- SPIR-V reflection (`reflect.rs::validate_set_layout`): Rust descriptor layout MUST match shader-declared bindings — reflection mismatch is the only sound layer for catching binding drift before runtime

### 8. R1 Material Table Safety
- `GpuMaterial` size pinned at 272 B by `gpu_material_size_is_272_bytes` test — failure means GPU-side struct is reading wrong bytes
- ALL fields scalar f32/u32 — never `[f32; 3]` (std430 vec3 alignment ≠ tightly-packed Rust)
- Named pad fields explicitly zeroed (no uninit bytes leak into byte-Hash dedup)
- `material_id` bounds: GpuInstance.material_id used as SSBO index — CPU must guarantee in-range; GPU has no bounds check

## Process

1. Use Grep to find all `unsafe` blocks in `crates/` (`.rs` files)
2. Read each unsafe block and its surrounding context
3. Check Vulkan resource pairing with Drop implementations
4. Check RT-specific safety (acceleration structures, device addresses, SSBO indexing)
5. Check new-compute-pipeline safety (TAA, caustic, skin compute)
6. Check R1 material table layout invariants
7. Save report to `docs/audits/AUDIT_SAFETY_<TODAY>.md`
