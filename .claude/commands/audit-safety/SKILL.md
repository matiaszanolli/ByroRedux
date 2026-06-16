---
description: "Safety audit — unsafe blocks, memory leaks, undefined behavior, Vulkan spec violations"
---

# Safety Audit

Read `_audit-common.md` (layout, methodology, dedup, report format) and
`_audit-severity.md` (the unified scale + the Special-Rules table this domain
leans on heavily) before starting. Do not restate their content here.

Severity anchors for this domain (from `_audit-severity.md`):
FFI lifetime violation = **CRITICAL** · BLAS/TLAS wrong geometry/address or
SSBO index mismatch = **CRITICAL** · leak that compounds per frame = **HIGH** ·
Vulkan spec violation = **HIGH** · `unsafe` without a safety comment = **MEDIUM**.

## Scale of the surface

`unsafe` is concentrated, not scattered: ~612 occurrences live in
`crates/renderer/src` (ash FFI + gpu-allocator), then a long tail —
~11 in `crates/nif`, ~6 in `crates/core`, and one each in `byroredux`,
`crates/plugin`, `crates/facegen`, `crates/cxx-bridge`. Renderer carries roughly
one `SAFETY` comment per two `unsafe` tokens; the gap is where the
unsafe-without-comment (MEDIUM) findings live. Budget your time accordingly —
do not audit the nif/core tail at the expense of the renderer FFI mass.

Dimensions below are ordered by safety blast radius: FFI lifetime, then
memory-corruption/UB, then per-frame leaks, then unsafe-block discipline, then
Vulkan-spec compliance, then the narrower regression-guard surfaces.

## Dimensions

### 1. FFI Lifetime Safety (cxx bridge) — CRITICAL class

- **The cxx surface is currently a placeholder.** `crates/cxx-bridge/src/lib.rs`
  exposes one bridge fn, `native_hello() -> String` (impl in
  `crates/cxx-bridge/cpp/native_utils.cpp`). There is **no raw-pointer exchange,
  no Rust-string-into-C++ borrow, no shared-ownership handoff** across the
  boundary today. Do NOT report speculative "string lifetime / dangling pointer
  across cxx" findings against this crate — they describe a surface that does not
  exist yet. The real check here is a **scope guard**: confirm the bridge still
  has no owned-pointer / borrowed-slice signatures. The instant a `*const`,
  `&[u8]`, `Box<…>`, or `unsafe extern "C++"` fn taking a Rust reference appears,
  this becomes a live CRITICAL-class dimension and the lifetime analysis from
  `_audit-severity` applies.
- `unsafe extern "C++"` in the bridge marks the C++ side as trusted — verify no
  new fn returns a pointer Rust then dereferences past the call.

### 2. Memory Corruption / UB

- **ECS cached-pointer contract (regression guard, #35 + #1367).** `World::get`
  (`crates/core/src/ecs/world.rs`) returns a `ComponentRef<'_, T>`, NOT a raw
  pointer with a dropped guard (the unsound #35 pattern). `ComponentRef`,
  `StorageRef`, and `StorageRefMut` in `crates/core/src/ecs/query.rs` cache a
  `*const T` / `*mut T` resolved once in `new()` and deref it in the hot path
  (#1367). Each cached-deref `unsafe` block carries a SAFETY comment tying the
  pointer's validity to the lock guard the wrapper pins. The invariant: **the
  guard must outlive every deref, and `&mut self` must gate `&mut *self.storage`.**
  Verify the SAFETY comments still match the field layout and that no refactor
  let a guard drop before its pointer (use-after-free → CRITICAL).
- **`#[repr(C)]` GPU-struct soundness** (`crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`):
  `GpuInstance`/`GpuCamera`/`GpuLight` etc. are uploaded byte-for-byte to SSBOs.
  vec3 must be three scalar `f32`, never `[f32; 3]` (std430 vec3 padding). A
  layout drift here is silent per-instance corruption — see Dimension 6 for the
  GpuMaterial pin and `_audit-severity`'s `#[repr(C)]`-drift HIGH row.
- **NIF bulk POD reads** (`NifStream::read_pod_vec`, `crates/nif/src/stream.rs`;
  the header mirror `read_pod_vec_from_cursor`, `crates/nif/src/header.rs`):
  `read_exact` of raw LE bytes into a `T: AnyBitPattern` vector. SAFETY comments
  must hold — `T` is restricted to bit-pattern-safe types (a sealed bound stops
  `read_pod_vec::<bool>`). Verify the byte-count overflow guard (`count × size`)
  is present and no caller widens `T` past `AnyBitPattern`.
- **sfmaterial enum decode** (`BuiltinType::from_u32`, `crates/sfmaterial/src/types.rs`):
  MUST stay a checked `match` over the `0xFFFFFF##` tags with a
  `_ => return Err(Error::UnsupportedBuiltin { raw })` arm (confirmed present).
  The module doc's "transmute into this enum" wording is aspirational prose, NOT
  the impl — an actual `std::mem::transmute` of an unmatched `#[repr(u32)]` byte
  pattern is UB. Verify the `match` + `Err` arm survive any "optimization."
- Stack-overflow risk: no unbounded recursion in block-walk / scene-graph traversal.

### 3. Memory & Resource Leaks (HIGH when per-frame/per-cell)

- **Rapier bodies on cell unload (regression guard, #1520, `34c7a218`).**
  `crates/physics/src/world.rs::remove_*` and `byroredux/src/cell_loader/unload.rs`
  must release a cell's rigid bodies, colliders, and impulse joints from
  `RigidBodySet` / `ColliderSet` / `ImpulseJointSet` (plus broad-phase /
  query-pipeline state) when the cell unloads. Without it they accumulate per
  cell — a steady leak under exterior streaming. Guard test:
  `byroredux/src/cell_loader/rapier_release_tests.rs`. Verify the release path is
  still wired and the test still asserts emptiness post-unload.
- **Deferred-destroy drain** (`crates/renderer/src/deferred_destroy.rs`,
  `DeferredDestroyQueue<T>` shared by mesh + BLAS + texture + skin compute):
  objects are destroyed only after the in-flight fence clears (#418 moved the tick
  after fence wait; #732 added an explicit shutdown drain). Verify the tick still
  runs **after** fence wait in `context/draw.rs` and the shutdown sweep drains the
  queue — a missed drain leaks GPU memory across the app lifetime, a too-early
  destroy is use-after-free (CRITICAL).
- **`AllocatorResource` drop ordering (regression guard, #1406, `299e6a84`).**
  `AllocatorResource` (`crates/renderer/src/vulkan/allocator.rs`; held in
  `byroredux/src/main.rs`) must be removed from the ECS `World` BEFORE
  `VulkanContext::drop()` runs. The allocator holds a live `Arc<Device>`; if the
  `World` outlives the context, the allocator's `Drop` calls the driver against a
  destroyed logical device (use-after-free → CRITICAL). Verify the main loop
  removes the resource before dropping the renderer, including the panic-unwind
  path that could skip the removal.
- **GPU allocation inventory** — every long-lived allocation tracked and freed:
  BLAS scratch/result, TLAS instance/result, G-buffer images, SVGF history, TAA
  per-FIF history images, caustic + water-caustic R32_UINT accumulators
  (`caustic.rs` / `water_caustic.rs`), per-skinned-entity SkinSlot output buffers,
  MaterialBuffer SSBO, volumetric/bloom mip pyramids. Cross-check eviction
  thresholds against `docs/engine/memory-budget.md`; do not re-derive.
- **CPU-side unbounded growth** — `Vec`/`HashMap` keyed by cell or path that never
  shrinks. The MaterialTable dedup map and AnimationClipRegistry (Dimension 8) are
  the known per-cell-growth risks.

### 4. Unsafe-Block Discipline (MEDIUM — the bread-and-butter sweep)

- Grep every `unsafe` in `crates/` + `byroredux/` (`.rs`). For each: is there a
  SAFETY comment, and does the comment's stated invariant actually hold at this
  call site? A correct unsafe block with no comment is still a MEDIUM finding
  (`_audit-severity` Special Rules). A commented block whose invariant is FALSE is
  the higher-severity finding.
- Heaviest in `crates/renderer/src/vulkan/` ash FFI — the SAFETY/unsafe count gap
  (~310 vs ~612) is the haystack. Spot-check the ash dispatch wrappers, the
  gpu-allocator `Arc<Mutex<…>>` interactions, and any `from_raw_parts` / `cast` on
  mapped memory.
- Report unsafe blocks lacking comments as a batched MEDIUM finding (list the
  sites) rather than one finding per block, unless an invariant is actually unsound.

### 5. Vulkan Spec Compliance (HIGH — but flag what cargo test can't see)

> Per the No-Speculative-Vulkan-Fixes rule: render-pass / barrier / pipeline-state
> spec claims that are invisible to `cargo test` MUST be framed as **"needs
> validation-layer or RenderDoc verification"**, not asserted as confirmed bugs.
> Run the engine with validation layers (debug build) and report ANY emitted
> error verbatim — that is the sound evidence channel for this dimension.

- All `vkCreate*`/`vkDestroy*` paired; Drop ordering destroys children before
  parents (device-destroy is last).
- Queue submission ordering: wait-before-signal; per-image semaphores.
- **Acceleration structures** (`crates/renderer/src/vulkan/acceleration/`): correct
  geometry flags, valid device addresses, buffers carry `SHADER_DEVICE_ADDRESS`.
  TLAS UPDATE mode — instance/geometry count must match the original BUILD.
  Skin BLAS refit — vertex/geometry count must match BUILD; a bone-count change
  forces a full rebuild. (Wrong AS geometry/address = CRITICAL per `_audit-severity`.)
- **TLAS resize wait (regression guard, #1390, `a7e1502b`).** The resize branch in
  `acceleration/tlas.rs` calls `device.device_wait_idle()` before freeing the old
  allocation (confirmed present). Verify the wait survives — without it the GPU may
  still consume the old TLAS scratch during free under a resize-under-load refactor.
- `VK_KHR_ray_query` enabled + feature-gated before any ray-query use.
- Per-frame compute layout hygiene (TAA / caustic / water-caustic / volumetrics /
  bloom): images that coexist as storage-write + sampled-read are held in `GENERAL`;
  `initialize_layouts` does the one-time UNDEFINED→GENERAL transition for **every**
  mip / FIF slot. A missed slot is an UNDEFINED-read validation error. CLEAR-before-
  COMPUTE invariant (caustic R32_UINT `imageAtomicAdd`, volumetric inject) — a
  missing clear is persistent cross-frame ghost accumulation. Verify the
  volumetrics caller honors the dispatch gate: dispatch is dead while
  `VOLUMETRIC_OUTPUT_CONSUMED == false` (`crates/renderer/src/vulkan/volumetrics.rs`);
  callers MUST gate `vol.dispatch()` on that const.
- SPIR-V reflection (`crates/renderer/src/vulkan/reflect.rs`): the Rust descriptor
  layout must match shader-declared bindings — this is the one binding-drift check
  that IS visible to `cargo test` (scene_descriptor_reflection_tests). Prefer it
  over eyeballing descriptor writes.

### 6. R1 Material Table Layout Soundness

- **`GpuMaterial` size is pinned at 300 B** by `gpu_material_size_is_300_bytes`
  (`crates/renderer/src/vulkan/material.rs`) — the test name now matches the
  asserted size (history: 272 → 260 after #804 dropped `avg_albedo`, → 296 with the
  Disney sheen/subsurface lobe #1249, → 300 with `anisotropic` #1250). A stale
  260/272/296 in audit prose, or any test-name-vs-asserted-size mismatch, means the
  GPU is reading wrong bytes.
- **Per-field offset pin** `gpu_material_field_offsets_match_shader_contract` (#806):
  every named field's byte offset asserted against the shader contract. The size pin
  alone cannot catch a within-vec4 reorder (swap `texture_index ↔ normal_map_index`
  is size-invisible, runtime-lethal). Adding a field without updating this assertion
  is a regression.
- ALL fields are flat scalar `f32`/`u32` — never `[f32; 3]` (std430 vec3 alignment).
  This includes the newest scalars: the BGSM translucency suite
  (`translucency_subsurface_r/g/b`, `…_transmissive_scale`, `…_turbulence`) and the
  Disney lobe (`ior`, `subsurface`, `sheen`, `sheen_tint`, `anisotropic`).
- Pad fields explicitly zeroed (the byte-`Hash`/`Eq` dedup hashes the raw 300 B; an
  uninit hole poisons dedup). New scalars must be zeroed in `GpuMaterial::default()`
  so default materials still dedup to slot 0.
- **Intern cap (#797).** `MaterialTable::intern` caps at `MAX_MATERIALS = 16384`
  (`scene_buffer/constants.rs`); over-cap interns return id `0` with a one-shot warn —
  no SSBO over-index, no DEVICE_LOST. `upload_materials` (`scene_buffer/upload.rs`)
  `debug_assert`s `len <= MAX_MATERIALS` and clamps with `.min(MAX_MATERIALS)`. Verify
  the intern cap and the upload truncation stay in lockstep.
- `GpuInstance.material_id` indexes the SSBO with NO GPU bounds check — CPU must
  guarantee in-range (SSBO index mismatch = CRITICAL).
- `ui.vert` MaterialBuffer read offsets must stay in lockstep with the canonical
  `struct GpuMaterial` / `GpuInstance` in `crates/renderer/shaders/include/bindings.glsl`
  (`triangle.frag` `#include`s it) — #785 was a stale-hunk regression reading wrong
  bytes — name `ui.vert` explicitly.

### 7. RT IOR-Refraction Safety (regression guards)

- **Glass-passthrough loop guard (#789):** the texture-equality identity check at
  the refraction hit prevents unbounded recursion when coincident glass surfaces
  share an albedo/normal-map descriptor pair. A regression is a frame-time hang on
  any paired-glass cell. Verify the check is present.
- **Glass ray budget** `GLASS_RAY_BUDGET = 1048576`
  (`crates/renderer/src/shader_constants_data.rs`; raised from 8192 in `6efe1706`).
  It is a runaway-recursion cap, not a quality knob. #1438 documented that the
  atomicAdd accounting can overshoot the budget unconditionally — note that nuance
  rather than reporting the overshoot as new. Verify the budget is enforced at every
  glass call site.
- **Frisvad orthonormal basis (#820):** the naive `cross(N, world-up)` basis
  degenerates near-vertical (zero-length → NaN ray). Verify Frisvad is the active
  path for IOR refraction roughness spread.
- IOR miss fallback for interiors uses cell-ambient, not global sky tint (open-sky
  leakage into dungeons is a visible regression).
- `DBG_VIZ_GLASS_PASSTHRU = 0x80` is a permanent diagnostic bit — verify it hasn't
  collided with a new debug flag (full catalog in
  `crates/renderer/src/shader_constants_data.rs`, mirrored to the generated
  `crates/renderer/shaders/include/shader_constants.glsl`).

### 8. NPC / Animation Spawn Safety

- **B-spline pose-fallback sentinel (#772):** NPCs vanishing under FNV
  `BSPSysSimpleColorModifier` particle stacks sharing keyframe time-zero with the
  actor's player must be gated on an `FLT_MAX` sentinel. Removing the gate is
  whole-NPC disappearance, not a stuck pose. Verify the sentinel is wired.
- **AnimationClipRegistry dedup (#790):** the registry interns by lowercased path so
  cell streaming doesn't grow it unboundedly (otherwise one keyframe set leaks per
  cell load → steady RAM growth). Verify case-insensitive interning is preserved.
- B-splines reach FNV / FO3 too (`feedback_bspline_not_skyrim_only.md`) — do NOT
  rule out `NiBSplineCompTransformInterpolator` by game era.
- Starfield content is WALKABLE (Cydonia) — SF cells reach the spawn/animation path;
  don't short-circuit spawn-safety reasoning with "no SF content exercises this."
- `MAX_TOTAL_BONES` overflow guard at the bone-palette emit site
  (`byroredux/src/render/skinned.rs`, `Once`-gated warn) must fire — silent
  truncation past cap was the M29 regression. Guard tests:
  `byroredux/src/render/bone_palette_overflow_tests.rs`.

### 9. NIFAL Boundary — NaN/Inf on the GPU (UB facet only)

*See `/audit-nifal` for correctness-of-mapping; this dimension covers ONLY the
safety facet — NaN/inf scalars reaching the GPU, unbounded allocation.*

- `byroredux/src/material_translate.rs::translate_material` deliberately seeds
  `f32::NAN` into `Material.metalness`/`roughness`
  (`mesh.metalness_override.unwrap_or(f32::NAN)`, same for roughness).
  `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`) is the ONLY
  thing that detects (`is_nan()`) and clamps these sentinels before they reach
  `GpuMaterial`. Both fields are now plain `f32` (no `Option`), so a producer that
  skips `resolve_pbr()` ships a NaN into the SSBO silently (NaN-on-GPU = UB). Verify
  EVERY renderer-bound `Material` producer runs `resolve_pbr()` or constructs
  already-finite values (the `static_meshes.rs` fallback constructs finite defaults
  directly — confirm it still does).
- Collision translate (`crates/nif/src/import/collision.rs`, covers
  `BhkMultiSphereShape` + `BhkConvexListShape`): emitted half-extents / radii /
  sphere centers must be finite and bounded — a NaN/inf shape param propagates into
  the physics solver and the BLAS build.
- Typed particle blocks (`crates/nif/src/blocks/particle.rs`) →
  `extract_emitter_params`/`extract_emitter_rate` (`crates/nif/src/import/walk/mod.rs`)
  → `apply_emitter_params` (`byroredux/src/systems/particle.rs`): emitter rate /
  lifespan / size must be finite and non-negative at the extract boundary — an
  unbounded or NaN rate is an unbounded-allocation / NaN-transform risk downstream.

### 10. debug-ui (egui overlay) Teardown & Shared-Allocator Safety

- `crates/debug-ui/src/lib.rs` `DebugUiState` holds an `ash::Device`, a
  `vk::RenderPass`, and the renderer's shared `Arc<Mutex<gpu_allocator …>>`; it lives
  as an ECS resource (`impl Resource for DebugUiState`) and is owned by the main loop.
- It wraps `egui-ash-renderer`, which owns its own descriptor pool + per-texture
  images. Those MUST be freed before the engine destroys the `ash::Device` — same
  class as Dimension 3's allocator-before-device rule. Verify `DebugUiState` teardown
  runs ahead of `VulkanContext`'s device-destroy.
- The allocator mutex is SHARED with the render thread — verify it is held for
  minimum duration during egui texture upload; a long hold stalls rendering.

## Procedure

1. Grep all `unsafe` in `crates/` + `byroredux/` (`.rs`); note the renderer mass
   and the SAFETY-comment gap (Dimension 4).
2. Confirm the cxx bridge is still a no-pointer placeholder (Dimension 1).
3. Audit the cached-pointer ECS contract + repr(C) GPU structs + NIF POD reads +
   sfmaterial decode (Dimension 2).
4. Walk the leak inventory and the three drop-ordering regression guards — Rapier
   release, deferred-destroy drain, AllocatorResource removal (Dimension 3).
5. Sweep unsafe-block discipline; batch the comment-less blocks (Dimension 4).
6. Vulkan-spec pass — run validation layers, report emitted errors verbatim; frame
   barrier/layout claims invisible to cargo test as "needs RenderDoc verification"
   (Dimension 5).
7. R1 material layout pins (Dimension 6), IOR/glass guards (7), NPC/anim spawn (8),
   NIFAL NaN boundary (9), debug-ui teardown (10).
8. Dedup against open/closed issues (`_audit-common` Deduplication) — most items
   above are regression guards; recast a confirmed-intact guard as PASS, not a NEW
   finding.
9. Save the report to `docs/audits/AUDIT_SAFETY_<TODAY>.md` (see `_audit-common`
   Report Finalization).
