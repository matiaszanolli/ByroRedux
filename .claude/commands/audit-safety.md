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
- Bone palette: `MAX_TOTAL_BONES` overflow guard in `byroredux/src/render.rs` (was `render.rs:204` pre-Session-34; line may have drifted — search `MAX_TOTAL_BONES` to confirm) actually fires (silent truncation past cap was the M29 regression). Regression-guard tests live at `byroredux/src/render/bone_palette_overflow_tests.rs`

### 7. New Compute Pipeline Safety (TAA, Caustic, Skin)
- TAA history images held in `GENERAL` layout (storage write + sampled read coexist); no UNDEFINED transitions per frame
- TAA `should_force_history_reset` path produces no NaN reads, weight α forced to 1.0 on first frame
- Caustic accumulator R32_UINT: `imageAtomicAdd` only — never float storage, no race
- Caustic CLEAR-before-COMPUTE invariant: missing clear = persistent ghost contributions across frames
- Skin compute push constants ≤ 128 B (Vulkan-guaranteed minimum); current `SkinPushConstants` is 12 B
- SPIR-V reflection (`reflect.rs::validate_set_layout`): Rust descriptor layout MUST match shader-declared bindings — reflection mismatch is the only sound layer for catching binding drift before runtime

### 8. R1 Material Table Safety
- `GpuMaterial` size pinned at **260 B** by `gpu_material_size_is_260_bytes` test (was 272 B until #804 / R1-N4 dropped `avg_albedo`) — failure means GPU-side struct is reading wrong bytes
- Per-field offset pin (`gpu_material_field_offsets_match_shader_contract`, #806): every named field's byte offset asserted against the shader contract. Size-only pin cannot catch within-vec4 reorders (e.g. swap `texture_index ↔ normal_map_index` is invisible to size, lethal at runtime). Adding a field WITHOUT updating this assertion is a regression
- ALL fields scalar f32/u32 — never `[f32; 3]` (std430 vec3 alignment ≠ tightly-packed Rust)
- Named pad fields explicitly zeroed (no uninit bytes leak into byte-Hash dedup)
- `material_id` bounds: GpuInstance.material_id used as SSBO index — CPU must guarantee in-range; GPU has no bounds check
- `MaterialTable::intern` cap (#797 SAFE-22): over `MAX_MATERIALS = 4096` distinct interns return id `0` with one-shot warn — no SSBO over-index, no DEVICE_LOST. Verify the cap fires AND the SSBO upload at `scene_buffer.rs:~975` truncates to `MAX_MATERIALS`. Mismatch between intern cap and upload truncation is the class of bug the cap was added to prevent
- `ui.vert` MaterialBuffer read offsets stay in lockstep with `triangle.frag` (#785 R-N1 was a stale-hunk regression of #776 reading wrong bytes — name `ui.vert` explicitly in any R1 audit)

### 9. RT IOR-Refraction Safety (Sessions 27–29)
- Glass-passthrough infinite loop (#789): texture-equality identity check at the refraction hit prevents unbounded recursion when two coincident glass surfaces share the same albedo/normal-map descriptor pair. Verify the check is still in place — a regression here is a frame-time hang under any cell with paired glass
- Frisvad orthonormal basis (#820 / REN-D9-NEW-01): the `cross(N, world-up)` construction degenerates near vertical surfaces (zero-length basis → NaN ray). Verify Frisvad is the active code path for IOR refraction roughness spread
- Glass ray budget bounded: `GLASS_RAY_BUDGET = 8192` (raised from 512 in 9a4dc15) — the cap exists to prevent runaway recursion, not as a quality knob. Verify the budget is enforced at every call site
- IOR miss fallback for interiors uses cell-ambient (bb53fd5), NOT the global sky tint — open-sky leakage into dungeons is a visible regression
- `DBG_VIZ_GLASS_PASSTHRU = 0x80` debug bit kept as a permanent diagnostic; verify the bit position has not collided with new debug-flag additions (full catalog: `triangle.frag:628-686`)

### 10. NPC / Animation Spawn Safety (M41.0 long-tail)
- B-spline pose-fallback (#772): NPC vanishing under FNV `BSPSysSimpleColorModifier` particle stacks that share keyframe time-zero with the actor's animation player must be gated on a `FLT_MAX` sentinel. Removing the gate causes whole-NPC disappearance, not just a stuck pose — verify the sentinel is still wired
- AnimationClipRegistry dedup (#790): registry deduplicates by lowercased path so cell streaming does not grow it unboundedly. Without dedup, one full keyframe set leaks per cell load — observable as steady RAM growth across exterior streaming. Verify case-insensitive interning is preserved
- B-splines are reachable on FNV / FO3 too (`feedback_bspline_not_skyrim_only.md`) — do NOT rule out `NiBSplineCompTransformInterpolator` audits by game era. Skyrim-only assumption is a stale premise that has bitten this audit before

## Process

1. Use Grep to find all `unsafe` blocks in `crates/` (`.rs` files)
2. Read each unsafe block and its surrounding context
3. Check Vulkan resource pairing with Drop implementations
4. Check RT-specific safety (acceleration structures, device addresses, SSBO indexing)
5. Check new-compute-pipeline safety (TAA, caustic, skin compute)
6. Check R1 material table layout invariants (260 B size pin + per-field offset pin + intern cap)
7. Check IOR-refraction safety (glass loop, Frisvad, ray budget, interior fallback)
8. Check NPC / animation spawn safety (B-spline FLT_MAX, AnimationClipRegistry dedup)
9. Save report to `docs/audits/AUDIT_SAFETY_<TODAY>.md`
