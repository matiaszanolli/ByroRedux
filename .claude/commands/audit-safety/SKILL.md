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

`unsafe` is concentrated, not scattered: **~865** occurrences live in
`crates/renderer/src` (ash FFI + gpu-allocator; ~830 at the last count, so this
mass grows) then a long tail — ~14 in `crates/nif` and ~11 in `crates/fsr3-sys`
(the vendored FSR 3.1 FFI, Dimension 1), ~6 in `crates/core`, **1** in
`byroredux` (`cell_loader/unload.rs`), and one each in `crates/plugin`,
`crates/cxx-bridge`, and `crates/pex` (the M47.2
decompiler — its single `unsafe` is a guarded `transmute` in `opcode.rs`, see
Dimension 2). `crates/save` (M45), `crates/hkx` (M47.2, a deliberately safe
packfile reader), `crates/bsa` (its untrusted-input bounds in `safety.rs` are
entirely safe code), `crates/mod-runtime` (the sandbox host, Dimension 11),
`crates/facegen` (M41 FaceGen decode — its one raw hit is a "No `unsafe`" doc
comment, not code), `crates/ui` (the Ruffle/wgpu boundary in Dimension 1 is
safe Rust end to end — its one raw hit is inside a log string, not code), and
`crates/sdk` (renderer-independent tooling/mod-API surface, added 2026-08-25,
`#![forbid(unsafe_code)]`, ~14k LOC) have **no** `unsafe` — for these the absence is itself the safety
property, so verify it rather than skipping them.
Counts drift — recount with `grep -ro unsafe crates/<c>/src | wc -l` rather
than trusting these figures, **but read the hits, don't just count them**: that
recipe matches the substring inside identifiers too (it reports ~14 for
`byroredux` on the strength of `serde_attr_declares_unsafe_default` alone), so
a count that jumps without a matching `unsafe {` block is a false alarm. Renderer carries roughly nine `SAFETY` comments
per ten `unsafe` tokens; the residual gap is where the unsafe-without-comment
(MEDIUM) findings live. Budget your time accordingly — do not audit the
nif/core/pex tail at the expense of the renderer FFI mass. The Dimension-4
sweep greps **all** of `crates/` so pex / save / fsr3-sys / hkx are covered
automatically.

Dimensions below are ordered by safety blast radius: FFI lifetime, then
memory-corruption/UB, then per-frame leaks, then unsafe-block discipline, then
Vulkan-spec compliance, then the narrower regression-guard surfaces.

## Dimensions

### 1. FFI Lifetime Safety (live crossings first) — CRITICAL class

**Ordering note (2026-08-13):** this dimension used to lead with the cxx bridge.
That is backwards — cxx-bridge is a 36-LOC placeholder while `crates/fsr3-sys`
is the workspace's only real FFI crossing and sits on the **engine-default**
render path. Audit `fsr3-sys` first, then the Ruffle/wgpu boundary
(`crates/ui`, cross-reference `/audit-ui` Dim 6), then confirm the cxx scope
guard below.

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
- **`crates/fsr3-sys` (added 2026-07-22) is a real, live FFI crossing** — unlike
  the cxx-bridge placeholder above, this is not hypothetical. `extern "C"`
  functions take `*mut RawContext`/`*const RawCreateDesc`/`*mut RawVersion`
  etc.; `pub unsafe fn Context::create`/`Context::dispatch` carry `# Safety`
  doc sections stating caller contracts (device/physical-device/proc-addr must
  outlive the `Context`; dispatch handles must belong to the creating device),
  and `Drop` calls back into the native shim. Audit every `unsafe fn` here for
  a `# Safety` doc and a lifetime contract the way this dimension used to
  reserve for a *hypothetical* live cxx-bridge.
- **The Ruffle / wgpu boundary (`crates/ui`) is the second live crossing.**
  `SwfPlayer` (`crates/ui/src/player.rs`) creates its own wgpu device and
  captures frames into a pixel buffer the engine then uploads through
  `update_rgba`. Two safety questions belong here: (a) the captured slice's
  lifetime vs. the upload (a borrow that outlives the backend's frame is a
  use-after-free), and (b) device/allocator teardown ordering against
  `VulkanContext` (same class as Dimension 3's allocator-before-device rule).
  The *contract* half — stride, format, resize — is `/audit-ui` Dim 6; report
  the memory-safety half here and cross-reference.

### 2. Memory Corruption / UB

- **ECS cached-pointer contract (regression guard, #35 + #1367).** `World::get`
  (`crates/core/src/ecs/world.rs`) returns a `ComponentRef<'_, T>`, NOT a raw
  pointer with a dropped guard (the unsound #35 pattern). `ComponentRef`,
  `QueryRead`, and `QueryWrite` in `crates/core/src/ecs/query.rs` cache a
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
- **pex opcode decode** (`OpCode::from_u8`, `crates/pex/src/opcode.rs`): unlike
  sfmaterial, this one IS a real `unsafe { std::mem::transmute::<u8, OpCode>(byte) }`
  — sound ONLY because the SAFETY comment's two preconditions hold: `OpCode` is
  `#[repr(u8)]` with **contiguous** discriminants `0..MAX_OPCODE`, AND `byte` is
  range-checked (`< MAX_OPCODE`) before the transmute. Both must stay true: a gap
  in the discriminant sequence, or a refactor that drops the bound check, makes an
  out-of-range byte UB. Verify the guard and the contiguity (no skipped values in
  the enum) on any opcode-table change.
- **Archive/record decompression bounds (`crates/bsa/src/safety.rs`, 2026-08-28).** This module is the one place the untrusted-archive size ceilings live: `checked_entry_count` (`MAX_ENTRY_COUNT` = 10 M), `checked_chunk_size` / `checked_chunk_size_usize` (`MAX_CHUNK_BYTES` = 1 GiB), `checked_chunk_total` (`MAX_RECORD_TOTAL_BYTES` = 2 GiB) and `inflate_bounded` (#3410). Its ESM counterpart is `read_sub_records`' bound (#3399, `/audit-esm` Dim 1) — same shape, same "a short decode stays `Ok`" contract. A new archive reader that hands a file-controlled length to `Vec::with_capacity` or an unbounded `read_to_end`, rather than routing through these, is the regression; so is the two implementations diverging.
- **The LZ4 `safe-decode` feature is load-bearing and pinned (#3392, `caa14cc5`).** The `catch_unwind` around `lz4_flex::decompress` was originally attributed to "a property of one pinned version" — wrong: the absence of panics is a property of the **`safe-decode` Cargo feature**. With it on, `decompress` is `vec![0; n]` plus a bounds-checked `SliceSink` under `forbid(unsafe_code)`, so a short hint is `Err(OutputTooSmall)` and the documented panic is structurally impossible. With it **off**, the same call writes through raw pointers with no capacity check and a short hint is a **heap overflow — UB no `catch_unwind` can intercept**. The feature was reachable only via `default` on an unpinned `lz4_flex = "0.11"`. It is now pinned explicitly, with `byroredux-bsa` its sole dependent, and the `catch_unwind` stays as defence-in-depth. Verify the pin survives any dependency bump; treat a `default-features = false` or an unpinned range here as HIGH, not a housekeeping nit.
- **Byte-range `&str` slicing on archive-derived names is a panic, not a miss (#3391, HIGH).** `canonical_mesh_path` tested its `.mesh` suffix with a `&str` byte-range slice, which panics whenever the cut lands inside a multi-byte scalar. `mesh_name` arrives from `read_sized_string`, which falls back to `from_utf8_lossy`, so both valid non-ASCII (`"модель"` is 12 bytes; 12−5 is mid-char) and lossily-decoded invalid bytes reach it — and `extract_bs_geometry` runs on the main thread **outside every `catch_unwind` guard** under `panic = "unwind"`, so it aborted the process rather than missing a lookup. Vanilla content is ASCII-only; the exposure is mods, localized paths and corrupt archives. Sweep for the pattern generally: any `&s[a..b]` on a name that came off disk must be byte-wise (`as_bytes()`), the technique `has_head` and `normalize_mesh_path` already use.
- Stack-overflow risk: no unbounded recursion in block-walk / scene-graph traversal. The ESM GRUP walkers are bounded as of #3503 (`/audit-esm` Dim 1) and NIF shape resolution as of #1385.

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
  `DeferredDestroyQueue<T>` — confirmed as of #2274 to have exactly three
  production instantiations: mesh vertex/index buffers (`mesh.rs`), BLAS entries
  and BLAS scratch buffers (`vulkan/acceleration/mod.rs`, #1782). No instantiation
  exists for texture or skin-compute resources; if those subsystems free deferred
  resources it is via a different, not-yet-verified mechanism — don't assume they
  share this queue):
  objects are destroyed only after the in-flight fence clears (#418 moved the tick
  after fence wait; #732 added an explicit shutdown drain). Verify the tick still
  runs **after** fence wait in `context/draw.rs` and the shutdown sweep drains the
  queue — a missed drain leaks GPU memory across the app lifetime, a too-early
  destroy is use-after-free (CRITICAL).
- **`AllocatorResource` drop ordering (regression guard, #1406, `299e6a84`).**
  `AllocatorResource` (`crates/renderer/src/vulkan/allocator.rs`; the live
  remove/insert sites are in `byroredux/src/app_events.rs` — `remove_resource`
  at ~line 68, re-insert on `resumed` at ~line 149, post-#2731; *main.rs* only
  carries the explanatory comment now) must be removed from the ECS `World` BEFORE
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
  shrinks. `MaterialTable`'s dedup map is NOT one of these: `build_render_data`
  calls `material_table.clear()` at the top of every frame (`byroredux/src/render/mod.rs`),
  so it is rebuilt fresh each frame and cannot grow across cells or the session
  (re-verified #2274). AnimationClipRegistry (Dimension 8) is the known
  per-cell-growth risk here.

### 4. Unsafe-Block Discipline (MEDIUM — the bread-and-butter sweep)

- Grep every `unsafe` in `crates/` + `byroredux/` (`.rs`). For each: is there a
  SAFETY comment, and does the comment's stated invariant actually hold at this
  call site? A correct unsafe block with no comment is still a MEDIUM finding
  (`_audit-severity` Special Rules). A commented block whose invariant is FALSE is
  the higher-severity finding.
- **Do NOT hunt a "SAFETY vs unsafe count gap."** #2692 retired that work item:
  the ~676-vs-~761 spread this section used to cite is a **token-counting
  artefact**, not a comment gap. A bare `grep -c unsafe` also counts `unsafe fn`
  declarations, `unsafe impl`, and the word in prose, none of which take a
  SAFETY comment. Counting actual `unsafe {` blocks against SAFETY comments
  shows no meaningful shortfall. Chasing the phantom gap burns a dimension's
  budget on a haystack with no needle.
- Heaviest in `crates/renderer/src/vulkan/` ash FFI. Spot-check the ash dispatch
  wrappers, the gpu-allocator `Arc<Mutex<…>>` interactions, and any
  `from_raw_parts` / `cast` on mapped memory — by reading invariants, not by
  counting.
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
- **Depth-capture format consistency (regression guard, #3570).**
  `depth_capture_record_copy` / `depth_capture_finish_readback`
  (`crates/renderer/src/vulkan/context/depth_capture.rs`) must consult
  `self.depth_format` rather than hardcoding 4 bytes / one f32 per sample —
  `find_depth_format`'s fallback chain tries `D32_SFLOAT` then `D16_UNORM`, and
  Vulkan mandates `D16_UNORM` depth-attachment support but not `D32_SFLOAT`, so
  the D16 arm is reachable on real hardware even though the dev RTX 4070 Ti
  never selects it. Confirmed fixed: the capture now refuses (with a warning)
  on a `D16_UNORM` device rather than misdecoding pairs of adjacent u16 samples
  as one f32. Verify the format check still gates the capture.
- `VK_KHR_ray_query` enabled + feature-gated before any ray-query use.
- Per-frame compute layout hygiene (TAA / caustic / water-caustic / volumetrics /
  bloom): images that coexist as storage-write + sampled-read are held in `GENERAL`;
  `initialize_layouts` does the one-time UNDEFINED→GENERAL transition for **every**
  mip / FIF slot. A missed slot is an UNDEFINED-read validation error. CLEAR-before-
  COMPUTE invariant (caustic R32_UINT `imageAtomicAdd`, volumetric inject) — a
  missing clear is persistent cross-frame ghost accumulation. Verify the
  volumetrics caller honors the dispatch gate: `VOLUMETRIC_OUTPUT_CONSUMED`
  (`crates/renderer/src/vulkan/volumetrics.rs`) is now `true`, so the pass is live —
  dispatch is dead only while it reads `false`. Callers MUST gate `vol.dispatch()` on
  that const either way (`context/post_passes.rs`); read the const rather than
  assuming a state. **Far-plane default lockstep (regression guard, #3611):**
  three independent copies of the volumetric far plane —
  `VolumetricsConfig::DEFAULT` (`upscaling.rs`), `DEFAULT_GRID_FAR_METERS`
  (`volumetrics.rs`), and `VOLUME_FAR` (`shader_constants_data.rs`, pre-scaled
  to world units and included verbatim by `build.rs`) — must agree; a drift is a
  behavioral divergence from `--fog-grid-far-m`, not cosmetic. Two now derive
  from one `const`; `VOLUME_FAR` stays a literal, pinned by a test asserting it
  equals `DEFAULT_GRID_FAR_METERS * WORLD_UNITS_PER_METER`. Verify that test
  still runs and the two `const` derivations haven't been re-forked into literals.
- SPIR-V reflection (`crates/renderer/src/vulkan/reflect.rs`): the Rust descriptor
  layout must match shader-declared bindings — this is the one binding-drift check
  that IS visible to `cargo test` (scene_descriptor_reflection_tests). Prefer it
  over eyeballing descriptor writes.

### 6. R1 Material Table Layout Soundness

- **`GpuMaterial` size is pinned at 432 B** by `gpu_material_size_is_432_bytes`
  (`crates/renderer/src/vulkan/material.rs`) — the test name now matches the
  asserted size (history: 272 → 260 after #804 dropped `avg_albedo`, → 280
  under #1147 (+20 B, `translucency_subsurface_r/g/b` + `…_transmissive_scale`
  + `…_turbulence`), → 284 under #1248 (+4 B, per-material `ior`), → 296 with the
  Disney sheen/subsurface lobe #1249, → 300 with `anisotropic` #1250, → 348 on
  2026-07-27 (`1d94eb24`) with the twelve common supplemental texture roles, →
  364 on 2026-08-23 (#2221) with animated shader color/float fields, → 396 with
  BGEM v21+ glass optics, → **432 on 2026-08-25 with the soft/rim/back Bethesda
  lighting response**).
  A stale 260/272/296/300/348/364/396 in audit prose, or any test-name-vs-asserted-size
  mismatch, means the GPU is reading wrong bytes.
- **Per-field offset pin** `gpu_material_field_offsets_match_shader_contract` (#806):
  every named field's byte offset asserted against the shader contract. The size pin
  alone cannot catch a within-vec4 reorder (swap `texture_index ↔ normal_map_index`
  is size-invisible, runtime-lethal). Adding a field without updating this assertion
  is a regression.
- ALL fields are flat scalar `f32`/`u32` — never `[f32; 3]` (std430 vec3 alignment).
  This includes the newest scalars: the BGSM translucency suite
  (`translucency_subsurface_r/g/b`, `…_transmissive_scale`, `…_turbulence`) and the
  Disney lobe (`ior`, `subsurface`, `sheen`, `sheen_tint`, `anisotropic`).
- Pad fields explicitly zeroed (the byte-`Hash`/`Eq` dedup hashes the raw 432 B; an
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

- **Glass-passthrough loop guard (#789):** the passthrough check at the refraction
  hit is keyed on `materialKind == MATERIAL_KIND_GLASS` (`triangle.frag`, since
  `a09d2b76` — replaced the original texture-equality identity check, which
  misfired whenever glass shared a texture with opaque geometry). The actual
  unbounded-recursion guard is `const int MAX_REFRACT_PASSTHRUS = 8;`, a
  fixed compile-time loop bound (`triangle.frag`, near the passthru loop) —
  independent of which identity check gates continuation. Within that hard
  bound, `refractPassthruBudget` early-exits the loop at an ADAPTIVE
  2/4/6/8 interfaces (quality tier 0-3, `rayBudget.qualityTier`) — verify
  the tier value, not a fixed 2 (#3052 / SAFE-2026-08-16-05: an earlier
  revision of this bullet named a nonexistent *REFRACT_PASSTHRU_BUDGET = 2*
  symbol that doesn't exist anywhere in the tree — `shader_constants.rs`
  actively asserts that exact string is ABSENT from triangle.frag). A
  regression is a frame-time hang on any paired-glass cell. Verify
  `MAX_REFRACT_PASSTHRUS` is still the loop bound and the `materialKind`
  check is present.
- **Glass ray budget** `GLASS_RAY_BUDGET`
  (`crates/renderer/src/shader_constants_data.rs`, mirrored in
  `crates/renderer/shaders/include/shader_constants.glsl` — verify the two stay
  in lockstep; raised from 8192 in `6efe1706`, and again since — check the
  constant by name rather than trusting a hard-coded figure here).
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
- `MAX_TOTAL_BONES` overflow guard must fire — silent truncation past cap was the
  M29 regression. The slot-exhaustion warn lives on `SkinSlotPool`
  (`crates/core/src/ecs/resources/skin_slot_pool.rs`), one-shot via the
  `overflow_warned` flag with `overflow_attempt_count` carrying total demand; excess
  skinned entities fall back to bind pose rather than over-indexing. Guard tests:
  `byroredux/src/render/bone_palette_overflow_tests.rs`.
- **First-sight `bind_inverses` upload failure requeue (regression guard,
  #3569).** `skin_dispatch_ran` alone only latches whether `draw_frame` reached
  the skin-dispatch section, not whether the `upload_pending_bind_inverses` call
  inside it actually succeeded — a failed host-visible map/flush let the frame
  report `skin_dispatch_ran = true` while `SkinSlotPool::drain_pending` had
  already irrevocably removed the entries from the pool, so the caller's
  rollback/requeue check in `app_frame.rs` never fired and those first-sight
  entities silently lost their bone palette. Fixed with a sibling latch,
  `bind_inverse_upload_failed` (set only in the upload's error arm, reset
  alongside `skin_dispatch_ran` at the top of `draw_frame`), widening the
  rollback check to `!ctx.skin_dispatch_ran || ctx.bind_inverse_upload_failed`
  (`byroredux/src/app_frame.rs`). Verify the widened check and the reset
  ordering survive — narrowing back to `skin_dispatch_ran` alone reintroduces
  the loss.

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
- Collision translate (`crates/nif/src/import/collision/mod.rs`, covers
  `BhkMultiSphereShape` + `BhkConvexListShape`): emitted half-extents / radii /
  sphere centers must be finite and bounded — a NaN/inf shape param propagates into
  the physics solver and the BLAS build.
- Typed particle blocks (`crates/nif/src/blocks/particle.rs`) →
  `extract_emitter_params`/`extract_emitter_rate` (`crates/nif/src/import/walk/mod.rs`)
  → `apply_emitter_params` (`byroredux/src/systems/particle.rs`): emitter rate /
  lifespan / size must be finite and non-negative at the extract boundary — an
  unbounded or NaN rate is an unbounded-allocation / NaN-transform risk downstream.

### 10. debug-ui (egui overlay) Teardown & Shared-Allocator Safety

- `crates/debug-ui/src/lib.rs` `DebugUiState` is the CPU half only — egui context,
  `egui_winit` state, last `FullOutput`, panel state. It holds no Vulkan handle. It
  lives as an ECS resource (`impl Resource for DebugUiState`) and is owned by the
  main loop.
- The Vulkan half is `EguiPass` (`crates/renderer/src/vulkan/egui_pass.rs`, held as
  `VulkanContext::egui_pass: Option<EguiPass>`). It takes an `ash::Device` + the
  shared `Arc<Mutex<gpu_allocator::vulkan::Allocator>>` and wraps `egui-ash-renderer`,
  which owns its own descriptor pool + per-texture images, plus `EguiPass`'s own
  `vk::RenderPass` + per-swapchain-image framebuffers. Those MUST be freed before the
  engine destroys the `ash::Device` — same class as Dimension 3's
  allocator-before-device rule. Verify the `Option<EguiPass>` teardown runs ahead of
  `VulkanContext`'s device-destroy.
- Texture free is deferred one frame (`pending_free`), leaning on `draw_frame`'s fence
  wait — verify the defer survives; freeing on the arriving frame is a use-after-free.
- The allocator mutex is SHARED with the render thread — verify it is held for
  minimum duration during egui texture upload; a long hold stalls rendering. The
  graphics queue is likewise passed as a `Mutex` (`EguiDispatchCtx::queue`) so the
  lock scopes to the `set_textures` submit only (CONC-D1-01 / #1713) — a widened
  hold regresses that.

### 11. Sandboxed Mod Runtime — Trust Boundary (`crates/mod-runtime`, added 2026-08-13)

`crates/mod-runtime/src/` is the engine-owned boundary between untrusted
community code and host services. It still contains **no `unsafe`**, but it is
**no longer contract-only**: `24df5304` (2026-08-31, "host sandboxed extensions
natively") gave it a real, wired-in consumer — `byroredux/src/extensions.rs`
(~10.6k LOC, itself `unsafe`-free), reached from `main.rs`
(`load_requested_extensions`, `queue_session_event`) and `app_events.rs`
(`shutdown_extension_host`, `extension_ui_menu_sync`). Audit this dimension as
a live path now, not a contract — "unused" is no longer an available excuse
for any gap found here.

- **Absence, not promise.** The crate docstring claims no WASI implementation is
  linked by default, so OS access is *absent* rather than merely unused. Verify
  that against `crates/mod-runtime/Cargo.toml` — a wasi feature pulled in
  transitively (even unused) turns the claim false, and a future
  *add_to_linker_sync* would silently activate it.
- **Capability gating.** The authority model split across two crates on
  2026-08-25 (`21a840d5`, new `crates/sdk`, "renderer-independent tools",
  `#![forbid(unsafe_code)]`, ~14k LOC): `CapabilitySet` / `CapabilityId`
  now live in `crates/sdk/src/identity.rs` (re-exported from
  `crates/mod-runtime/src/lib.rs`), and only `Principal` remains in
  `crates/mod-runtime/src/identity.rs`. `LOG_CAPABILITY` is **no longer the
  only defined capability** — it is one alias (`byroredux_sdk::service::LOG_WRITE_CAPABILITY`)
  among 28 named `*_CAPABILITY` constants in `crates/sdk/src/service.rs`
  spanning components/events/input/storage/world/actor-values/animation/
  reputation/inventory/factions/perks/packages/console/scripts/settings.
  Verify every host function reachable from a guest still checks
  `grants.contains(...)` **before** acting (confirmed present in
  `extensions.rs` for `SCRIPT_FUNCTIONS_REGISTER_CAPABILITY`,
  `CONSOLE_REGISTER_CAPABILITY`, and `EVENTS_SUBSCRIBE_CAPABILITY`, but
  spot-check the rest of the 28 rather than assuming the pattern holds
  uniformly), and that a missing grant is an error rather than a no-op — a
  silently-ignored denial is indistinguishable from success to the guest and
  to the log reader.
- **Per-instance isolation.** Each `ModInstance` gets its own principal and
  store. Verify no shared mutable state (a `static`, a shared `Arc`, a global
  logger buffer) lets one instance observe or affect another — this now
  matters for real with `extensions.rs` potentially hosting more than one
  loaded mod side by side.
- **Resource limits.** `SandboxConfig` (`crates/mod-runtime/src/limits.rs`) plus
  `fuel_remaining` are the DoS defenses. Verify `validate()` rejects degenerate
  configs (zero/absurd fuel, zero memory) and that fuel exhaustion produces a
  `FaultInfo` / terminal `InstanceStatus` rather than a hang. An unbounded
  `logs()` `Vec` is itself a memory-exhaustion channel a guest controls — check
  for a cap.
- **Lifecycle.** `LifecyclePhase` / `initialize` / `shutdown`: verify a fault in
  one phase cannot leave the instance usable, that `shutdown` is idempotent, and
  that a trapping guest is quarantined rather than retried in a loop.
- **Untrusted input at compile time.** `SandboxRuntime::compile` takes arbitrary
  bytes. Verify a malformed component yields `SandboxError`, not a panic — this
  crate's whole point is that hostile input is expected.

## Procedure

1. Grep all `unsafe` in `crates/` + `byroredux/` (`.rs`); note the renderer mass
   and the SAFETY-comment gap (Dimension 4).
2. Audit the live FFI crossings — `fsr3-sys` then Ruffle/wgpu — and confirm the
   cxx bridge is still a no-pointer placeholder (Dimension 1).
3. Audit the cached-pointer ECS contract + repr(C) GPU structs + NIF POD reads +
   sfmaterial decode (Dimension 2).
4. Walk the leak inventory and the three drop-ordering regression guards — Rapier
   release, deferred-destroy drain, AllocatorResource removal (Dimension 3).
5. Sweep unsafe-block discipline; batch the comment-less blocks (Dimension 4).
6. Vulkan-spec pass — run validation layers, report emitted errors verbatim; frame
   barrier/layout claims invisible to cargo test as "needs RenderDoc verification"
   (Dimension 5).
7. R1 material layout pins (Dimension 6), IOR/glass guards (7), NPC/anim spawn (8),
   NIFAL NaN boundary (9), debug-ui teardown (10), mod-runtime trust boundary (11).
8. Dedup against open/closed issues (`_audit-common` Deduplication) — most items
   above are regression guards; recast a confirmed-intact guard as PASS, not a NEW
   finding.
9. Save the report to `docs/audits/AUDIT_SAFETY_<TODAY>.md` (see `_audit-common`
   Report Finalization).
