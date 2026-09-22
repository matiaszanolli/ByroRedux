---
description: "Safety audit — unsafe blocks, memory leaks, undefined behavior, Vulkan spec violations"
---

# Safety Audit

Read `_audit-common.md` (layout, methodology, dedup, report format) and `_audit-severity.md` (the unified
scale + Special-Rules table this domain leans on) before starting. Do not restate their content here.

Severity anchors: FFI lifetime violation = **CRITICAL** · BLAS/TLAS wrong geometry/address or SSBO index
mismatch = **CRITICAL** · leak that compounds per frame = **HIGH** · Vulkan spec violation = **HIGH** ·
`unsafe` without a safety comment = **MEDIUM**.

Scope: unsafe / FFI / memory-safety / GPU-lifetime rules. Untrusted-input *parser discipline* (BSA/BA2 size
ceilings, BGSM, CDB, HKX, FaceGen, MenuXml, archive-name slicing) is `/audit-parsers`; the `.pex` / `.psc`
front ends are `/audit-papyrus`. Dimensions run by blast radius; each has `Paths:` and `First step:` — skip
a dimension whose Paths have no commits since the last `AUDIT_SAFETY_*` report.

## Scale of the surface (re-measure; do not trust these figures)

Census recipe: `grep -rwo unsafe <crate>/src | wc -l` for tokens, `grep -rnE 'unsafe[[:space:]]*\{' <crate>/src`
for blocks — **read the hits**: substring counts also match identifiers (`byroredux` reports 3 tokens and
only 1 real block). Measured 2026-09-19:

- `crates/renderer/src`: **879** word tokens (884 by the substring recipe) — 678 `unsafe {` blocks, 133
  `unsafe fn`, 33 `unsafe impl`, ~766 `SAFETY` mentions. The last recorded figure was ~827–830 and the
  earlier "flat, not growing" note is wrong: new pipelines (`groundcover.rs`, `groundcover_bench.rs`, sky
  bake) outpaced the `GpuImage` consolidation (`vulkan/image.rs`, 9 tokens). Compare against the previous
  report, not this text.
- Tail: `crates/fsr3-sys` 9 blocks (Dim 1), `crates/nif` 1 POD-read site + 4 `unsafe impl AnyBitPattern`
  (Dim 2), `crates/core` 4 in `ecs/query.rs` (`/audit-ecs` Dim 3) + 1 in `string/mod.rs`
  (`from_utf8_unchecked` after an ASCII-only fold), `crates/pex` 1 transmute (Dim 2),
  `byroredux/src/cell_loader/unload.rs` 1, `tools/byro-launcher/src/preflight.rs` 1 block + 1 `unsafe fn`
  (Vulkan loader probe), `crates/plugin` test-only env-var edits, `crates/cxx-bridge` one `unsafe extern "C++"`.
- No `unsafe` at all: `crates/{bsa,save,scripting,sdk,ui,facegen,sfmaterial,mod-runtime,hkx,bgsm,menuxml,physics,audio}`.
  **Only `crates/sdk` carries `#![forbid(unsafe_code)]`** — for every other unsafe-free crate the absence is
  unenforced, so re-run the census on each and report any newcomer.

## Dimensions

### 1. FFI Lifetime Safety (live crossings first) — CRITICAL class
Paths: `crates/fsr3-sys/`, `crates/renderer/src/vulkan/frame_upscaler.rs`, `crates/ui/src/`, `tools/byro-launcher/src/preflight.rs`, `crates/cxx-bridge/`
First step: `grep -nE 'unsafe|# Safety|extern "C' crates/fsr3-sys/src/lib.rs`

- **`crates/fsr3-sys` is the workspace's only real FFI crossing** and sits on the default render path:
  `extern "C"` functions over `*mut RawContext` / `*const RawCreateDesc` (native shim under
  `crates/fsr3-sys/native/`, vendored FidelityFX C++ — trusted, not audited by Rust rules);
  `Context::create` and `Context::dispatch` are `pub unsafe fn` with `# Safety` docs (device / physical
  device / proc-addr outlive the `Context`; dispatch handles belong to the creating device); `Drop`
  calls the native destroy, whose Vulkan-idle requirement is part of `create`'s contract. Every new
  `pub unsafe fn` needs a `# Safety` section; the renderer call site (`frame_upscaler.rs`) must honour
  it, and `destroy_allocations` ordering is `/audit-concurrency` Dim 6.
- **Ruffle / wgpu (`crates/ui`)** is safe Rust end to end. Memory-safety questions: the captured pixel
  slice's lifetime vs. the engine upload (`update_rgba` / `write_rgba_inplace` — a borrow outliving the
  backend frame is a UAF), and wgpu device/allocator teardown vs. `VulkanContext`. Stride/format/resize
  contract is `/audit-ui`.
- **`tools/byro-launcher/src/preflight.rs`** loads the Vulkan loader and creates an instance
  (`ash::Entry::load`, `enumerate_physical_devices`, `destroy_instance`): check the instance is destroyed on
  every path and no handle escapes `probe`.
- **cxx bridge scope guard**: `crates/cxx-bridge/src/lib.rs` exposes one fn, `native_hello() -> String`. No
  raw pointers, borrowed slices or shared ownership cross it today, so there is no lifetime surface yet; the scope guard is that no `*const`, `&[u8]`, `Box<…>` or Rust-reference-taking `unsafe extern "C++"`
  fn appears (then it becomes a live CRITICAL surface).

### 2. Memory Corruption / UB
Paths: `crates/core/src/ecs/query.rs`, `crates/nif/src/{stream,header}.rs`, `crates/pex/src/opcode.rs`, `crates/renderer/src/vulkan/{buffer,scene_buffer/gpu_types,material}.rs`, `Cargo.toml`
First step: `grep -rnE 'transmute|from_raw_parts|set_len|from_utf8_unchecked|unsafe impl' crates byroredux tools --include='*.rs' | grep -v test`

- **ECS cached-pointer contract** (#35/#1367): audit lives in `/audit-ecs` Dim 3 (four derefs, Miri job); report a
  broken guard there, cross-reference here.
- **`NoUninit` byte views** (`vulkan/buffer.rs`): `unsafe trait NoUninit` (local twin of `bytemuck::NoUninit`)
  gates `write_mapped`, `create_device_local_buffer` and `byte_view`; `GpuMaterial::as_bytes` and the
  dedup hash route through `byte_view` (#4445, #3990). Every `unsafe impl NoUninit for X` must be `#[repr(C)]`
  with no padding — the layout tests in Dim 6 and in `scene_buffer/gpu_instance_layout_tests.rs` are its
  proof; a struct that gains a hole fails at hash/equality, not silently. A new byte-slice cast that
  bypasses `byte_view` is the regression.
- **`#[repr(C)]` GPU structs** (`scene_buffer/gpu_types.rs`, `material.rs`): vec3 is three scalar `f32`, never
  `[f32; 3]` (std430); drift is per-instance corruption (`_audit-severity` repr(C) HIGH row) — pins in Dim 6.
- **NIF bulk POD reads** (`NifStream::read_pod_vec`, `header::read_pod_vec_from_cursor`, single unsafe site
  `read_pod_vec_from` in `stream.rs`): `T: AnyBitPattern` is a sealed `unsafe trait` (impls: primitives, arrays,
  `NiPoint3`-style structs, `BoneWeight`/`Meshlet`/`CullData` in `blocks/bs_geometry.rs`); callers compute
  `count × size` with `checked_mul`; `set_len` happens only after `read_exact` succeeds; a big-endian
  compile-error gate protects the LE layout. A new `unsafe impl AnyBitPattern` needs a padding-free,
  every-bit-pattern-valid proof.
- **pex opcode decode** (`OpCode::from_u8`): a real `transmute::<u8, OpCode>`, sound only while `OpCode` is
  `#[repr(u8)]` with contiguous discriminants `0..MAX_OPCODE` AND `byte >= MAX_OPCODE` is rejected first. A gap
  in the table or a dropped bound is UB.
- **LZ4 `safe-decode` pin** (`Cargo.toml`, `byroredux-bsa` the sole dependent, #3392): with the feature on,
  `lz4_flex::decompress` is bounds-checked; **off**, a short hint is a heap overflow no `catch_unwind` can
  catch. Verify the pin survives dependency bumps; `default-features = false` or an unpinned range is HIGH.
  The archive size ceilings themselves are `/audit-parsers`.
- Stack-overflow risk: no unbounded recursion in block-walk / scene-graph traversal (ESM GRUP walkers bounded
  per `/audit-esm`; NIF shape resolution #1385; ECS hierarchy via `HierarchyTraversalGuard`).

### 3. Memory & Resource Leaks, Drop Ordering (HIGH when per-frame/per-cell)
Paths: `crates/physics/src/world.rs`, `byroredux/src/cell_loader/unload.rs`, `crates/renderer/src/{deferred_destroy,mesh}.rs`, `crates/renderer/src/texture_registry/`, `byroredux/src/app_events.rs`
First step: `cargo test -p byroredux rapier_release && grep -rn 'DeferredDestroyQueue<' crates/renderer/src | grep -v deferred_destroy.rs`

- **Rapier bodies on cell unload (#1520)**: `release_victim_rapier_bodies` frees bodies, colliders, joints and
  broad-phase state per unloaded cell; guard `byroredux/src/cell_loader/rapier_release_tests.rs`
  (`release_sweeps_both_ragdoll_and_rapier_handles`) — confirm it still asserts emptiness.
- **Deferred destroy**: `DeferredDestroyQueue<T>` (`deferred_destroy.rs`) has four production instantiations —
  mesh vertex/index buffers (`mesh.rs`), retired instance SSBOs (`scene_buffer/buffers.rs`), BLAS entries and
  BLAS scratch (`acceleration/mod.rs`). Textures use a different mechanism: per-entry `pending_destroy` drained
  by `TextureRegistry::tick_deferred_destroy` (`texture_registry/release.rs`, backlog published as
  `texture_pending_destroy_count`). All ticks run AFTER the in-flight fence wait
  (`context/sync_and_acquire_frame.rs`) and shutdown drains; a missed drain leaks GPU memory, a too-early
  destroy is UAF (CRITICAL). Skin-compute resources use no shared queue — verify their own path when touched.
- **`AllocatorResource` drop order (#1406)**: removed from the ECS `World` BEFORE `VulkanContext::drop()`
  (`remove_resource` / re-insert in `byroredux/src/app_events.rs`); its `Drop` calls the driver, so a
  `World` that outlives the context makes it call a destroyed device (CRITICAL). Check the
  panic-unwind path can't skip the removal.
- **Allocator-lock poison policy is not uniform**: `GpuImage` create/free recover with `into_inner()` (#4089;
  a poisoned lock means another thread panicked, and a second panic in teardown is not recovery), while
  `buffer.rs`, `allocator.rs` and `texture.rs` still `.expect("… poisoned")`. Flag an `.expect` reachable from
  `Drop`/teardown (double panic) or a silent recovery with no rationale comment (#2398); a create-path panic is
  a deliberate choice, not automatically a finding.
- **egui**: texture free is deferred one frame (`pending_free`) on `draw_frame`'s fence wait — freeing on the
  arriving frame is UAF; `EguiPass` teardown ordering is `/audit-concurrency` Dim 6.
- **GPU allocation inventory** (BLAS/TLAS + scratch, G-buffer, SVGF/TAA history, caustic accumulators, skin
  slots, MaterialBuffer, volumetric/bloom pyramids, ground-cover and sky-bake buffers): each tracked and freed;
  budgets are `docs/engine/memory-budget.md` — do not re-derive.
- **CPU unbounded growth**: `Vec` / `HashMap` keyed by cell or path that never shrinks. `MaterialTable` is
  cleared at the top of every frame (`material_table.clear()`, `byroredux/src/render/mod.rs`) and cannot grow;
  `AnimationClipRegistry` stub growth is documented in `/audit-ecs` Dim 7.

### 4. Unsafe-Block Discipline — are the stated invariants TRUE?
Paths: `crates/renderer/src/vulkan/`, `crates/renderer/src/lib.rs`
First step: `grep -rn undocumented_unsafe_blocks crates` (expect the one `deny` in `crates/renderer/src/lib.rs`, no `allow` escapes), then the census recipe

- **Guard (renderer)**: `crates/renderer/src/lib.rs` carries `#![deny(clippy::undocumented_unsafe_blocks)]` (#1904), so a
  comment-less `unsafe {}` in the renderer fails `cargo clippy` in the CI job `Test + Check + Clippy`
  (`cargo clippy --workspace -- -D warnings`); it is inert under `cargo build` / `cargo test`. Confirm the `deny` is
  present and unescaped. The lint sees `unsafe {}` blocks only: `unsafe fn` bodies and the 33 renderer `unsafe impl`s
  need a justification found by reading. Crates outside the renderer have no such lint — sweep comment-less blocks
  there by hand (all commented at the last count: `fsr3-sys`, `nif`, `core`, `pex`, `byroredux`, `byro-launcher`); a
  comment-less block is MEDIUM.
- The audit's value is therefore the *truth* of each invariant, not its presence: for each new or changed block, does the
  stated precondition (device live, handles from this device, not in flight, pointer valid for the call) hold at THIS call
  site? A commented block whose invariant is FALSE is the higher-severity finding.
- Do not hunt a "SAFETY vs `unsafe` count gap" (#2692): raw token counts include `unsafe fn`, `unsafe impl` and prose.
- Heaviest in ash FFI wrappers, the gpu-allocator `Arc<Mutex<…>>` interplay, and `from_raw_parts` / `cast` on mapped
  memory. Prioritise files new or heavily edited since the last report over the nif/core/pex tail.

### 5. Vulkan Spec Compliance (HIGH — flag what `cargo test` can't see)
Paths: `crates/renderer/src/vulkan/`
First step: CI job `vulkan-validation` (lavapipe, `.github/workflows/ci.yml`) fails on any `[Vulkan]` ERROR line; locally `BYRO_VALIDATION=1`. #4596 — the lane reached Vulkan only after its fixes (libxkbcommon-x11-0 + BYRO_ALLOW_CPU_VULKAN_DEVICE admitting lavapipe); a lane-red-from-boot means it is inert again, and findings cannot claim lane coverage

Render-pass / barrier / pipeline-state claims invisible to `cargo test` are "needs validation-layer or RenderDoc
verification" (`/audit-concurrency` guardrail); report emitted validation errors verbatim.

- Every `vkCreate*` has a matching `vkDestroy*`; children destroyed before parents, device last.
- **Acceleration structures**: correct geometry flags, valid device addresses, `SHADER_DEVICE_ADDRESS` on buffers;
  TLAS UPDATE count == BUILD count; skin-BLAS refit vertex/geometry count == BUILD (a bone-count change forces a
  rebuild). Wrong AS geometry/address = CRITICAL. The TLAS resize `device_wait_idle` (#1390) is defence behind the
  both-slots fence wait (`/audit-concurrency` Dim 1) — verify one of the two survives.
- **Depth-capture format (#3570)**: `depth_capture_record_copy` / `_finish_readback` (`context/depth_capture.rs`)
  consult the live depth format; D16 devices refuse rather than misdecode as f32.
- `VK_KHR_ray_query` is enabled and feature-gated before any ray-query use.
- **Compute layout hygiene**: images used as storage-write + sampled-read stay in `GENERAL`; `initialize_layouts`
  transitions every mip / FIF slot once; caustic / volumetric-inject CLEAR-before-COMPUTE (a missing clear is
  persistent ghost accumulation); `vol.dispatch()` gated on `VOLUMETRIC_OUTPUT_CONSUMED` (read the const, don't
  assume). **Volumetric far-plane lockstep (#3611)**: `VolumetricsConfig::DEFAULT`, `DEFAULT_GRID_FAR_METERS`
  (derived) and the literal `VOLUME_FAR` (`shader_constants_data.rs`) agree —
  `volume_far_shader_constant_agrees_with_the_volumetrics_config_default` pins it.
- **SPIR-V reflection** (`vulkan/reflect.rs`, `scene_buffer/scene_descriptor_reflection_tests.rs`) is the one
  binding-drift check visible to `cargo test`; prefer it to eyeballing descriptor writes.

### 6. GPU Struct Layout Soundness (`GpuMaterial` and siblings)
Paths: `crates/renderer/src/vulkan/{material.rs,material_tests.rs}`, `crates/renderer/src/vulkan/scene_buffer/`, `crates/renderer/shaders/include/bindings.glsl`
First step: `cargo test -p byroredux-renderer gpu_material gpu_instance gpu_camera`

- **Sizes are pinned by tests named for the size** (`gpu_material_size_is_432_bytes` — `GpuMaterial` is 432 B;
  `gpu_instance_is_160_bytes_std430_compatible`, `gpu_camera_is_368_bytes`, `gpu_light_is_64_bytes`,
  `gpu_terrain_tile_is_160_bytes` in `scene_buffer/gpu_instance_layout_tests.rs`). A test-name-vs-asserted-size
  mismatch or a stale number in prose means the GPU reads wrong bytes. Guards that police the prose:
  `bindings_glsl_states_the_real_struct_size`, `the_pin_test_bindings_glsl_names_actually_exists` and
  `gpu_material_size_claims` (scans `crates`, `byroredux`, `tools`, `docs/engine`, `.claude/commands`, top-level
  docs for stale sizes) in `material_tests.rs` — run them after editing any audit skill that states a size.
- **Per-field offsets**: `gpu_material_field_offsets_match_shader_contract` (#806) — the size pin cannot see a
  within-vec4 reorder. Adding a field without updating it is a regression; the other half is
  `gpu_material_glsl_field_names_pinned`.
- All fields are flat scalar `f32`/`u32`, no `[f32; 3]`, and (since #3909) no explicit padding: `NoUninit` (Dim 2)
  enforces that at the dedup hash. New scalars must be zero in `GpuMaterial::default()` so default materials dedup
  to slot 0.
- **Intern cap (#797)**: `MaterialTable::intern` caps at `MAX_MATERIALS` (`scene_buffer/constants.rs`) and
  returns id `0` with a one-shot warn; `upload_materials` `debug_assert`s and clamps with `.min(MAX_MATERIALS)` —
  keep both in lockstep.
- `GpuInstance.material_id` indexes the SSBO with NO GPU bounds check — the CPU must guarantee in-range (CRITICAL).
- `ui.vert` MaterialBuffer read offsets stay in lockstep with `bindings.glsl`'s `GpuMaterial` / `GpuInstance`
  (`triangle.frag` `#include`s it; #785 was a stale-hunk regression). Standing lockstep rule: `GpuInstance` in
  `bindings.glsl` plus its standalone shader copies change together.

### 7. GPU-Fed Data & Shader Loop Bounds
Paths: `crates/renderer/shaders/triangle.frag`, `crates/renderer/src/{shader_constants.rs,shader_constants_data.rs}`, `byroredux/src/material_translate.rs`, `byroredux/src/app_frame.rs`, `crates/core/src/ecs/resources/skin_slot_pool.rs`
First step: `cargo test -p byroredux-renderer shader_constants && cargo test -p byroredux bone_palette_overflow`

- **RT glass loop bound**: the unbounded-recursion guard is `MAX_REFRACT_PASSTHRUS` (`= 2 + 2 * MAX_RAY_QUALITY_TIER`,
  emitted into the generated `shader_constants.glsl`); `triangle.frag`'s passthrough loop is bounded by
  `int(MAX_REFRACT_PASSTHRUS)` and exits early at the adaptive `refractPassthruBudget` (2/4/6/8 by quality tier).
  Pinned by `triangle_frag_scales_glass_interface_depth_with_honest_ray_cost` (`shader_constants.rs`). Frisvad's
  orthonormal basis (`include/math_common.glsl`) replaces the degenerate `cross(N, up)` near vertical (#820).
  `GLASS_RAY_BUDGET` is now the tier-3 ceiling for telemetry, **not** a runaway cap (`glass_ray_limit` stays in
  the GPU ABI for comparison only), so an unenforced budget at a call site is not a hang risk.
- **NaN/Inf reaching the GPU**: `translate_material` seeds `f32::NAN` into `Material.metalness`/`roughness`;
  `Material::resolve_pbr` (`/audit-ecs` Dim 8) is the only place that clamps them. Every renderer-bound `Material`
  producer must run `resolve_pbr()` or build already-finite values. Collision translate
  (`crates/nif/src/import/collision/mod.rs`) half-extents/radii and emitter rate/lifespan/size
  (`extract_emitter_params` → `apply_emitter_params`) must be finite and bounded at the extract boundary.
- **Bone palette overflow**: `SkinSlotPool` warns once (`overflow_warned`, count in `overflow_attempt_count`) and
  excess entities fall back to bind pose rather than over-indexing; tests in
  `byroredux/src/render/bone_palette_overflow_tests.rs`.
- **First-sight `bind_inverses` requeue (#3569/#3991)**: `app_frame.rs` rolls back pose commits and requeues
  pending uploads when `!ctx.skin_state_submitted || ctx.bind_inverse_upload_failed`. `skin_state_submitted` is the
  SUBMIT-time latch (set only after `queue_submit` returns `Ok`); reading the record-time `skin_dispatch_ran`
  instead loses palettes on the three tail `Err` sites of `draw_frame`. Verify the check and latch resets survive.
- Starfield content is walkable (Cydonia): do not dismiss spawn/animation safety with "no SF content exercises this".

### 8. Sandboxed Mod Runtime — Trust Boundary (`crates/mod-runtime`)
Paths: `crates/mod-runtime/src/`, `crates/sdk/src/{identity,service}.rs`, `byroredux/src/extensions/`, workspace `Cargo.toml` (`wasmtime`)
First step: `cargo tree -p byroredux-mod-runtime | grep -i wasi` (must print nothing) and `grep -c 'require_' crates/mod-runtime/src/runtime/capabilities.rs`

`crates/mod-runtime` (wasmtime component-model host, no `unsafe`) is wired into the engine: `byroredux/src/extensions/`
(~10.7k LOC, `unsafe`-free) is reached from `main.rs` (`load_requested_extensions`, `queue_session_event`) and
`app_events.rs` (`shutdown_extension_host`, `extension_ui_menu_sync`). Audit it as a live path.

- **Absence, not promise**: the crate doc says no WASI is linked; `wasmtime` is declared with
  `default-features = false, features = [anyhow, component-model, cranelift, runtime, std]`. A `wasi` feature pulled
  in transitively turns the claim false.
- **Capability gating**: authority types live in `crates/sdk/src/identity.rs` (`CapabilitySet`, `CapabilityId`;
  `Principal` stays in `crates/mod-runtime/src/identity.rs`); the 28 `*_CAPABILITY` constants are in
  `crates/sdk/src/service.rs`. Gating is centralised in `crates/mod-runtime/src/runtime/capabilities.rs` (`require_*`
  guards) and used from `runtime/host/*.rs`; registration-time checks (`grants.contains(...)` for script functions,
  console, settings, events) live in `byroredux/src/extensions/{install,dispatch}.rs`. **No test enumerates host
  functions against capabilities**, so enumerate the `impl <wit>::Host for HostState` fns and confirm each reaches a
  `require_*` before acting (or is documented capability-free), and that a missing grant is an `Err`, never a no-op
  (a swallowed denial looks like success to the guest and to the log reader).
- Every `*_CAPABILITY` constant has a real call site: `/audit-tooling` (the reverse direction of the check above).
- **Per-instance isolation**: each `ModInstance` has its own principal and store; no `static`, shared `Arc` or global
  buffer lets one instance observe or affect another (matters with several mods hosted side by side).
- **Resource limits**: `SandboxConfig::validate()` (`limits.rs`) rejects absurd limits (`MAX_SANE_LIMIT`,
  `MAX_WASM_STACK_BYTES_CEILING`, `MAX_FUEL_PER_ENTRY`); fuel exhaustion yields a `FaultInfo` / terminal
  `InstanceStatus`, not a hang; guest-controlled buffers are capped (`max_log_entries` / `max_log_message_bytes` /
  `max_log_bytes`, command and console-output budgets) with distinct `FaultKind`s.
- **Lifecycle**: a fault in one `LifecyclePhase` cannot leave the instance usable; `shutdown` is idempotent; a
  trapping guest is quarantined, not retried in a loop.
- **Untrusted bytes at compile time**: `SandboxRuntime::compile` takes arbitrary bytes bounded by
  `max_component_bytes`; a malformed component is `SandboxError`, never a panic.
- **Guest entry vs ECS guards / host `Mutex`**: `/audit-concurrency` Dim 3.

## Procedure

1. Census (`Scale of the surface`); note new unsafe files since the last report.
2. Dims 1–2 first (live FFI, UB); then leaks/drop order (3), unsafe sweep (4), Vulkan spec (5), GPU layout (6), GPU-fed
   data (7), mod-runtime (8).
3. Dedup against open/closed issues (`_audit-common` Deduplication) — most items are regression guards; report an
   intact guard as PASS, not a NEW finding.
4. Save to `docs/audits/AUDIT_SAFETY_<TODAY>.md` (see `_audit-common` Report Finalization).
