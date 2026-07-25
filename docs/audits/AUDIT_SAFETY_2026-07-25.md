# Safety Audit — 2026-07-25

**Scope:** `/audit-safety` full sweep (one leg of the 2026-07-25 `comprehensive`
audit-suite run) — unsafe-block invariants, memory-corruption / UB, per-frame /
per-cell leaks, Vulkan spec compliance, FFI lifetimes, R1 material layout, RT
IOR/glass guards, NPC/animation spawn safety, NIFAL NaN boundary, debug-ui
teardown ordering.

**Method:** Grepped all `unsafe` in `crates/` + `byroredux/` (776 tokens: 719
renderer, 34 `fsr3-sys` — 11 in `src/lib.rs`, 23 in `examples/
vulkan_context_smoke.rs` — 11 nif, 6 core, 1 each plugin/pex/facegen/cxx-bridge,
0 byroredux-crate-proper*; 641 `// SAFETY` comment lines, 616 of them in
renderer). Diffed `git log --since=2026-07-16` (the prior safety audit's date)
to scope the delta: five commits (`34e26ca8`, `33d6a18e`, `8d0d170c`, plus two
doc/CI commits) landed a brand-new **`fsr3-sys` crate** (native AMD FidelityFX
FSR 3.1 upscaler FFI) and its renderer integration
(`frame_upscaler.rs`, `presentation.rs` — both new files) — this is the one
subsystem genuinely unreviewed by any prior safety pass, so it got a full
Dimension 1/2/3/4/5 pass rather than a diff-only skim. Re-verified every
regression-guard test the SKILL enumerates
(`rapier_release_tests`, `bone_palette_overflow_tests`, `gpu_material_*`,
`scene_descriptor_reflection_tests`, opcode round-trip test). Deduped against
`gh issue list --state all --limit 200` and `docs/audits/`.

*byroredux binary proper: 2 raw `unsafe` tokens (both reviewed, see PASS list).

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 2 |
| **Total NEW** | **3** |

The headline result this pass isn't a bug in old code — it's that the
`fsr3-sys` crate (landed 2026-07-22 → 07-24, entirely after the 2026-07-16
audit) is a **real, live-pointer FFI boundary**, unlike the still-inert
`cxx-bridge` placeholder Dimension 1 has always described. It audited clean:
every `unsafe fn` carries a `# Safety` doc, every `unsafe` block in
`src/lib.rs` carries an inline `SAFETY` comment, and the renderer's
`FrameUpscaler` teardown is correctly sequenced behind `device_wait_idle` (see
PASS list). The one MEDIUM below is a comment-coverage gap in the *renderer's*
new FSR integration code (not `fsr3-sys` itself), and the two LOWs are
audit-infrastructure doc-rot the fsr3-sys arrival exposed.

---

## Findings

### SAFE-2026-07-25-01: New FSR presentation-pass code has 19 uncommented `unsafe` blocks (batched)

- **Severity**: MEDIUM
- **Dimension**: 4 (Unsafe-Block Discipline)
- **Location**: `crates/renderer/src/vulkan/presentation.rs:417-448`,
  `crates/renderer/src/vulkan/composite.rs:381-407,1144-1162`,
  `crates/renderer/src/vulkan/frame_upscaler.rs:360,470`,
  `crates/renderer/src/vulkan/context/draw.rs:893`,
  `crates/renderer/src/vulkan/context/resize.rs:827`
- **Status**: NEW
- **Description**: All five files are new or newly-extended by the FSR 3.1
  integration (`33d6a18e`, 2026-07-23; `presentation.rs` and
  `frame_upscaler.rs` did not exist before this commit). A heuristic sweep for
  `unsafe {` blocks lacking a `SAFETY`/`Safety` mention within 15 lines above
  or 3 lines below flagged 35 raw hits; manual review discarded most as false
  positives (long multi-line SAFETY comments outside the scan window, e.g.
  `nif/src/stream.rs:369`, `nif/src/header.rs:382`, both genuinely commented).
  What remains as real gaps:
  - `presentation.rs::destroy()` (lines 417-448): six `device.destroy_*` calls
    with no per-call comment. The enclosing fn carries a `# Safety` doc
    ("No in-flight command buffer may reference this pipeline") but the
    per-call SAFETY-comment convention used everywhere else in the renderer
    (e.g. `exposure.rs`, `svgf.rs`) is not followed here.
  - `composite.rs` (lines 381-407, 1144-1162): `create_image` /
    `get_image_memory_requirements` / `bind_image_memory` /
    `create_image_view` for the new "composed scene" images — the exact same
    call sequence as `exposure.rs`'s fully-commented version, but without the
    inline SAFETY comments.
  - `frame_upscaler.rs:360,470`, `draw.rs:893`, `resize.rs:827`: single-line
    delegating calls (`unsafe { self.record_fsr_barriers_before(...) }`,
    `unsafe { presentation.destroy(&self.device) }`) to already-`# Safety`-
    documented private/public fns — lowest-priority instances of the gap
    since the contract is stated once at the callee, but still bare per the
    letter of the Dimension-4 rule.
  - `crates/fsr3-sys/examples/vulkan_context_smoke.rs` (17 sites): a
    standalone ash Vulkan bring-up example (not part of the shipped engine
    binary — it's a `cargo run --example` smoke test) with zero SAFETY
    annotations on any of its raw `entry`/`instance`/`device` calls.
- **Impact**: No invariant is actually violated anywhere I checked — this is
  a hygiene gap (Special Rule: "unsafe block without safety comment ⇒ at
  least MEDIUM"), not a live bug. Blast radius is documentation quality on
  the newest, least-reviewed renderer code.
- **Suggested Fix**: Port the existing SAFETY-comment convention from
  `exposure.rs`/`svgf.rs` onto `composite.rs`'s new composed-scene block and
  `presentation.rs::destroy()`'s per-call sites; for the example file, add a
  short top-of-file comment establishing the bring-up contract once (matches
  how `dispatch_abi_structs_are_plain_and_pointer_width_stable` documents the
  ABI contract in `fsr3-sys/src/lib.rs` today) rather than 17 individual
  comments.

### SAFE-2026-07-25-02: `fsr3-sys` is absent from the audit-safety SKILL's Dimension 1 and the repo's crate inventory

- **Severity**: LOW
- **Dimension**: Audit-infrastructure (doc-rot, same class as the prior
  audit's Dimension-10 DebugUiState note and the TD4-00x SKILL-staleness
  issues)
- **Location**: `.claude/commands/audit-safety/SKILL.md` (Dimension 1 and the
  "Scale of the surface" section), `.claude/commands/_audit-common.md`
  ("Crate count: 21 under `crates/`" paragraph)
- **Status**: NEW
- **Description**: Dimension 1 says "The cxx surface is currently a
  placeholder... There is no raw-pointer exchange... Do NOT report
  speculative... findings against this crate." That framing is still
  accurate for `crates/cxx-bridge`, but `crates/fsr3-sys` (added 2026-07-22,
  three commits before this audit) is now the codebase's *actual* live FFI
  boundary: `extern "C"` functions taking `*mut RawContext`/`*const
  RawCreateDesc`/`*mut RawVersion` etc., a `pub unsafe fn Context::create`
  with documented pointer/lifetime preconditions, and a `Drop` impl that
  calls back into the native shim. `_audit-common.md`'s "21 crates" inventory
  and coverage-sanity list also don't mention it (there are 22 crates under
  `crates/` today, `fsr3-sys` being the addition).
- **Impact**: None to running code — this is a documentation-only gap. Impact
  is to *future* audits: an agent following the SKILL literally would treat
  the cxx-bridge scope-guard as covering "the FFI surface" and never grep
  `fsr3-sys`, exactly as almost happened during this pass (it surfaced only
  because the total-unsafe-token grep in step 1 didn't reconcile against the
  documented per-crate breakdown).
- **Suggested Fix**: Add `fsr3-sys` to `_audit-common.md`'s crate list (22
  crates) and give audit-safety's Dimension 1 a second bullet: "`fsr3-sys`
  (added 2026-07-22) is a real FFI crossing — every `unsafe fn` needs a `#
  Safety` doc and lifetime contract; audit it the way Dimension 1 used to
  reserve for a *hypothetical* live cxx-bridge."

### SAFE-2026-07-25-03: `GLASS_RAY_BUDGET` skill-doc value is stale (1,048,576 vs actual 2,097,152)

- **Severity**: LOW
- **Dimension**: Audit-infrastructure (doc-rot)
- **Location**: `.claude/commands/audit-safety/SKILL.md` Dimension 7;
  actual value at `crates/renderer/src/shader_constants_data.rs:122` and its
  GLSL mirror `crates/renderer/shaders/include/shader_constants.glsl:40`
  (both `2_097_152` / `2097152u` — in lockstep with each other, just not with
  the SKILL's cited figure)
- **Status**: NEW
- **Description**: The SKILL text reads "`GLASS_RAY_BUDGET = 1048576`...
  raised from 8192 in `6efe1706`." Current code has it at `2_097_152` — a
  further doubling landed at some point after the SKILL prose was last
  synced, with the Rust constant and its generated GLSL `#define` still
  correctly matched to each other (verified — this is NOT a lockstep-drift
  bug, just a stale citation).
- **Impact**: None to code correctness. A future auditor citing "1048576" as
  the live budget would be quoting a number at least one revision out of
  date.
- **Suggested Fix**: Update the SKILL's cited value, or better, drop the
  literal number from the prose and point at the constant by name only (the
  lockstep guard between the two files is what actually matters, not the
  magnitude).

---

## Regression Guards Verified (PASS — not findings)

- **Dimension 1 — cxx bridge is still a no-pointer placeholder.**
  `crates/cxx-bridge/src/lib.rs` exposes only `fn native_hello() -> String`
  (owned `cxx::String` return, no `*const`/`&[u8]`/`Box<…>`/Rust-reference-
  taking `extern "C++"`). Unchanged. PASS.
- **Dimension 1 (new surface) — `fsr3-sys` FFI soundness.** Read
  `crates/fsr3-sys/src/lib.rs` in full (461 lines). Every `extern "C"`
  function is POD-in/POD-out (`*mut`/`*const` on `#[repr(C)]` structs or
  primitives, no borrowed slices, no Rust-owned heap objects crossing the
  boundary). `Context::create`/`Context::dispatch` are `pub unsafe fn` with
  `# Safety` doc sections stating the caller contract (device/physical-
  device/proc-addr must outlive the `Context`; dispatch handles must belong
  to the creating device and stay live through submission). `Context::drop`
  documents "the Vulkan-idle requirement is part of `Context::create`'s
  contract" — traced the one production call site
  (`crates/renderer/src/vulkan/context/mod.rs`, `FrameUpscaler` field) and
  confirmed `VulkanContext::Drop` calls `device.device_wait_idle()`
  unconditionally as its first action (`context/mod.rs:3156`), before
  `frame_upscaler.destroy()` runs inside the allocator-`Some` guard
  (`context/mod.rs:3297-3299`) and long before the `VkDevice` is destroyed —
  so the native `byro_fsr3_context_destroy` call, wherever it fires (explicit
  `destroy()` or the natural `Option<Context>` drop), always runs on an idle
  queue. `recreate()` (the resize path) also gates its `destroy()` call behind
  a documented "resize calls this only after `device_wait_idle`" comment.
  PASS.
- **Dimension 2 — ECS cached-pointer contract (#35/#1367).**
  `QueryRead`/`QueryWrite` (`crates/core/src/ecs/query.rs:23-181`) and
  `ComponentRef` (`query.rs:231-291`) all cache a `*const`/`*mut` resolved
  once in `new()`; every deref carries a SAFETY comment tying validity to the
  guard field pinned in the same struct. `&mut *self.storage`
  (`query.rs:143`) is gated behind `&mut self`. No guard field is dropped
  before its cached pointer — field-drop order in a `Drop`-implementing
  struct runs after the explicit `drop()` body, and the guard field itself
  carries no early-drop logic. PASS.
- **Dimension 2 — `#[repr(C)]` GPU-struct layout.** Grepped
  `scene_buffer/gpu_types.rs` and `material.rs` for `[f32; 3]` — the only
  hits are module-doc prose warning against it and constructor-helper
  *inputs* (`fn car_paint(base: [f32; 3])`, converted to scalars before
  storage), never a struct field. Every vec3-shaped field (`position`,
  `color_type`, `sun_direction`, …) is `[f32; 4]`. PASS.
- **Dimension 2 — NIF POD reads.** `read_pod_vec`
  (`crates/nif/src/stream.rs:340-372`) and the header mirror
  `read_pod_vec_from_cursor` (`header.rs:360-385`) both keep the
  `count.checked_mul(size_of::<T>())` overflow guard feeding `check_alloc`,
  and both carry the full inline SAFETY comment (16 and 6 lines
  respectively) justifying the raw-bytes cast. The sealed `unsafe trait
  AnyBitPattern` (`stream.rs:47`) is instantiated only for primitives,
  fixed-arity float/byte arrays, and `NiPoint3` — `read_pod_vec::<bool>`
  still does not compile. PASS.
- **Dimension 2 — sfmaterial `BuiltinType::from_u32`.** Still a fully
  checked `match` with `_ => return Err(Error::UnsupportedBuiltin { raw })`
  (`crates/sfmaterial/src/types.rs:37-55`); no transmute. PASS.
- **Dimension 2 — pex `OpCode::from_u8` transmute.**
  `crates/pex/src/opcode.rs:130-136`: range check (`byte >= MAX_OPCODE`)
  precedes the transmute; `OpCode` is `#[repr(u8)]` with 51 contiguous
  discriminants `0..=50` (verified by direct enum read — no gaps), matching
  `MAX_OPCODE = 51`. `from_u8_round_trips_and_rejects_oob` test still
  asserts both directions. PASS.
- **Dimension 3 — Rapier release on cell unload (#1520, `34c7a218`).**
  `PhysicsWorld::remove_body` (`crates/physics/src/world.rs:149-160`) passes
  `remove_attached_colliders = true` and threads `impulse_joints` +
  `multibody_joints`. `release_victim_rapier_bodies`
  (`byroredux/src/cell_loader/unload.rs:387`) is called from `unload_cell`.
  `rapier_release_tests.rs` (284 lines, 7 tests) still asserts
  post-release emptiness for plain bodies, cascaded colliders, non-victim
  survival, handle-less victims, missing-resource no-op, **and** the
  #1531 ragdoll extension (multibody joints swept alongside bodies +
  colliders). All 7 tests still exist and match the guard's intent. PASS.
- **Dimension 3 — deferred-destroy drain timing.**
  `draw.rs:604-628`: the "Deferred-destroy tick. Runs AFTER `wait_for_fences`"
  comment block still precedes the three `tick_deferred_destroy` calls
  (skin slots / cluster-cull / accel manager), all after the per-frame
  `wait_for_fences` at line 512-520. Shutdown sweep confirmed at
  `context/mod.rs:3245-3268` (`accel.destroy()` drains
  `pending_destroy_blas`/scratch internally per #732). PASS.
- **Dimension 3 — `AllocatorResource` drop ordering (#1406, `299e6a84`).**
  Both teardown paths in `byroredux/src/main.rs` — the explicit
  `WindowEvent::CloseRequested` arm (lines 960-962) and `impl Drop for App`
  (lines 247-255, added for the panic-unwind / non-CloseRequested exit case,
  citing #1640/#1477) — remove `AllocatorResource` from the ECS `World`
  strictly before `self.renderer.take()` drops `VulkanContext`. Idempotent
  on the second call. PASS — this guard is unchanged from the last audit and
  already covers every exit path, including the one the new FSR context
  teardown had to slot into (see Dimension 1 above).
- **Dimension 5 — TLAS resize wait (#1390, `a7e1502b`).**
  `acceleration/tlas.rs:347`: `device.device_wait_idle()` still runs in the
  resize branch before the old scratch allocation is freed. PASS.
- **Dimension 5 — volumetrics dispatch gate.**
  `VOLUMETRIC_OUTPUT_CONSUMED` (`volumetrics.rs:164`, currently `true`) is
  read by both call sites that gate `vol.dispatch()`
  (`context/post_passes.rs:301`, `context/draw.rs:1956`). PASS.
- **Dimension 5 — SPIR-V reflection test.**
  `scene_descriptor_reflection_tests` module still wired into
  `scene_buffer/mod.rs:45`. PASS.
- **Dimension 6 — R1 material layout pins.**
  `gpu_material_size_is_300_bytes` still asserts `size_of::<GpuMaterial>() ==
  300` (`material.rs:1210`); `gpu_material_field_offsets_match_shader_contract`
  (`material.rs:1360`) still pins the swap-sensitive `texture_index` (offset
  48) / `normal_map_index` (offset 52) pair explicitly. `MAX_MATERIALS =
  16384` (`scene_buffer/constants.rs:185`) matches `intern()`'s cap check
  (`material.rs:1108`) and `upload_materials`'s `debug_assert!(len <=
  MAX_MATERIALS)` + `.min(MAX_MATERIALS)` clamp
  (`scene_buffer/upload.rs:647-652`). PASS.
- **Dimension 7 — RT IOR/glass guards.** Glass-passthrough loop guard (#789)
  present in `triangle.frag:1427-1437`; `GLASS_RAY_BUDGET` enforcement
  (`glassIORAllowed = (old + GLASS_RAY_COST <= GLASS_RAY_BUDGET)`,
  `triangle.frag:1322`) intact — value itself has drifted from the SKILL's
  citation, see SAFE-2026-07-25-03 above, but the Rust↔GLSL lockstep holds.
  `DBG_VIZ_GLASS_PASSTHRU = 0x80` has no collision against the other 9
  `DBG_VIZ_*` constants (`shader_constants_data.rs`, values `0x4` through
  `0x400000`, all distinct); the module's own "no-redeclare guard" table
  still lists it once. PASS.
- **Dimension 8 — NPC/animation spawn safety.** `FLT_MAX` sentinel gate
  present throughout `crates/nif/src/anim/bspline.rs` (translation,
  rotation, and scale channels all gated). `AnimationClipRegistry` interns
  via `.to_ascii_lowercase()` (`registry.rs:212`). `MAX_TOTAL_BONES = 196608`
  (`scene_buffer/constants.rs:56`); the M29.6 slot-pool overflow path
  (`crates/core/src/ecs/resources/skin_slot_pool.rs:74-155`) latches a
  one-shot `overflow_warned` bool and logs once; `skinned.rs`'s
  `SKIN_DROPOUT_DUMPED: Once` gate (line 38) covers the render-side dropout
  diagnostic. `bone_palette_overflow_tests.rs` still asserts
  `palette.len() <= MAX_TOTAL_BONES`. PASS.
- **Dimension 9 — NIFAL NaN boundary.** `material_translate.rs:157-158`
  seeds `mesh.metalness_override.unwrap_or(f32::NAN)` /
  `roughness_override.unwrap_or(f32::NAN)`; `Material::resolve_pbr`
  (`crates/core/src/ecs/components/material.rs:741-742`) is the sole
  `is_nan()` check-and-clamp. Grepped for every non-test production caller of
  `resolve_pbr()` — exactly one call site
  (`material_translate.rs:164`), confirming the single-boundary invariant.
  `static_meshes.rs` reads the already-resolved `Material.roughness`/
  `.metalness` fields (post-`resolve_pbr`), never the raw NaN sentinel.
  PASS.
- **Dimension 10 — debug-ui teardown ordering.** Already flagged as SKILL
  doc-rot in the 2026-07-16 audit and unchanged since: `DebugUiState`
  (`crates/debug-ui/src/lib.rs:101-118`) holds only `egui::Context`,
  `egui_winit::State`, `Option<egui::FullOutput>`, `PanelState` — no Vulkan
  handles, no `Drop` impl. The actual Vulkan-owning egui wrapper is
  `EguiPass` (`crates/renderer/src/vulkan/egui_pass.rs`, holds `ash::Device`
  reference, `vk::RenderPass`, `Arc<Mutex<gpu_allocator::vulkan::Allocator>>`
  via `egui_ash_renderer::Renderer::with_gpu_allocator`), a field on
  `VulkanContext` destroyed first thing in `Drop`
  (`context/mod.rs:3162-3164`, immediately after `device_wait_idle`, well
  before the allocator is taken or the device destroyed). Re-verifying
  because this is exactly the kind of interaction the new `FrameUpscaler`
  (also inside `VulkanContext`) had to get right too — confirmed it does
  (see Dimension 1 PASS above). Not re-filing; this is the same doc-rot the
  last audit already flagged for the SKILL maintainer and it hasn't been
  actioned yet (SKILL.md Dimension 10 text is unchanged) — bundled into
  SAFE-2026-07-25-02's suggested-fix scope rather than a fourth finding.

---

## Premises Checked and Disproved (no finding)

- **"The renderer's FSR integration might have skipped the
  `AllocatorResource`-before-device-drop discipline for its own new
  allocations."** Checked: `FrameUpscaler`'s output images/views/allocations
  are destroyed via the same `if let Some(ref alloc) = self.allocator`
  guard block as every other allocator-dependent subsystem
  (`context/mod.rs:3297-3299`), not a separate teardown path — no new
  bypass was introduced.
- **"`fsr3-sys`'s `Drop for Context` might race a still-in-flight FSR
  dispatch since nothing in the type system enforces the documented
  Vulkan-idle precondition."** True in principle (the contract is
  comment-only, not type-enforced), but the one production caller always
  drops `Context` from inside `VulkanContext::Drop`, which calls
  `device_wait_idle()` unconditionally as its very first statement — so the
  precondition is satisfied at the only call site that exists. Not filing a
  finding; flagging as worth a defensive comment if a second call site (e.g.
  a future live upscaler-mode hot-swap outside `recreate()`) is ever added.
- **Vulkan barrier/layout spec claims for the new composite/presentation
  passes.** Per the repo's No-Speculative-Vulkan-Fixes guardrail, no
  barrier/layout finding is asserted without validation-layer or RenderDoc
  evidence. None was gathered this pass — no live engine instance was
  running (checked via `pgrep`) and launching one was avoided per the
  No-Parallel-Engine-Launch policy given other build activity was in
  progress in the working tree at audit time. The new `frame_upscaler.rs`
  barrier sequencing (FSR-boundary barriers before/after dispatch, dispatch-
  failure recovery restoring depth + blitting from `GENERAL`) reads as
  internally consistent on a static review, but this dimension needs
  validation-layer or RenderDoc verification before any pass/fail claim,
  not a static-review-only assertion.
