# Safety Audit — 2026-08-10

Scope: `unsafe` blocks, FFI boundaries, memory-lifecycle code, Vulkan spec
compliance, and the safety-relevant regression guards across the ByroRedux
workspace, per `.claude/commands/audit-safety/SKILL.md` (10 dimensions) and
the shared protocol in `_audit-common.md` / `_audit-severity.md`.
HEAD at audit time: `6f93b565`.

Method: all ten dimensions were run as independent parallel workers, each
re-deriving its findings from current source (file:line citations) rather
than trusting the skill doc's own prose or a prior report's summary, and each
running the mandatory dedup check against a freshly-fetched
`/tmp/audit/issues.json` (`gh issue list --limit 200`). The prior pass
(`docs/audits/AUDIT_SAFETY_2026-08-07.md`, 3 days earlier) is the diff
baseline; several dimensions explicitly re-verified that pass's regression
guards rather than re-deriving from scratch, and Dimension 7 specifically
traced the effect of the intervening `5798e467` adaptive-ray-budget refactor.

**Result: one CRITICAL, zero HIGH, three MEDIUM, nine LOW.** The CRITICAL is
a genuine new defect (Dimension 5) — a write-once "TLAS is valid" latch that
never resets on a failed TLAS rebuild, leaving a destroyed acceleration
structure bound and ray-traced against for the rest of the session. Every
other dimension's guard set — FFI lifetime contracts (cxx-bridge placeholder
+ fsr3-sys `# Safety` contracts), the ECS cached-pointer contract, `#[repr(C)]`
GPU-struct layout (both `gpu_types.rs` and the 348-byte `GpuMaterial`), NIF
bulk-POD-read overflow guards, the three drop-ordering leak guards (Rapier
cell-unload release, deferred-destroy drain timing, `AllocatorResource`
ordering incl. panic-unwind), the glass/IOR refraction guards, NPC/animation
spawn safety (B-spline sentinel, `AnimationClipRegistry` dedup, bone-palette
overflow fallback), the NIFAL NaN/Inf material boundary, and debug-ui
teardown ordering — was re-verified intact. Zero uncommented `unsafe` blocks
exist anywhere in the workspace (`crates/renderer/src/lib.rs`'s
`#![deny(clippy::undocumented_unsafe_blocks)]`, landed under #1904, makes
this a structural guarantee, not a one-time sweep result).

---

## Total Severity Counts

| Severity | Count |
|---|---|
| **CRITICAL** | **1** |
| **HIGH** | **0** |
| **MEDIUM** | **3** |
| **LOW** | **9** |
| **Total live findings** | **13** |

(Counts include both NEW findings and pre-existing OPEN issues that were
independently re-confirmed reproducible against current HEAD this pass — see
each finding's **Status** line. Findings marked purely "Existing — verified
FIXED, not re-opened" are excluded from this table; they are recorded in the
Regression Guards section for completeness.)

| Dimension | CRITICAL | HIGH | MEDIUM | LOW | PASS items |
|---|---|---|---|---|---|
| 1. FFI Lifetime Safety | 0 | 0 | 0 | 0 | 3 |
| 2. Memory Corruption / UB | 0 | 0 | 0 | 0 | 6 |
| 3. Memory & Resource Leaks | 0 | 0 | 0 | 0 | 5 |
| 4. Unsafe-Block Discipline | 0 | 0 | 0 | 1 | workspace-wide (0 uncommented blocks) |
| 5. Vulkan Spec Compliance | 1 | 0 | 1 | 0 | 12 |
| 6. Material Table Layout | 0 | 0 | 0 | 2 | 10 |
| 7. RT IOR-Refraction Safety | 0 | 0 | 0 | 2 (+1 existing) | 9 |
| 8. NPC/Animation Spawn Safety | 0 | 0 | 0 | 0 | 4 |
| 9. NIFAL NaN/Inf Boundary | 0 | 0 | 1 (existing) | 0 | 4 |
| 10. debug-ui Teardown | 0 | 0 | 1 | 3 | 10 |

---

## Findings

### SAFE-2026-08-10-01: `SceneBuffers::tlas_written` is a sticky latch never cleared on TLAS destruction — a failed TLAS resize leaves a destroyed acceleration structure bound and ray-traced against for the rest of the session
- **Severity**: CRITICAL
- **Dimension**: 5 (Vulkan Spec Compliance) — also Dimension 3 (leak/lifetime) class
- **Location**: `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:175` (sole writer, `write_tlas`); `crates/renderer/src/vulkan/scene_buffer/buffers.rs:185,909` (`tlas_written: Vec<bool>`); `crates/renderer/src/vulkan/acceleration/tlas.rs:695-939` (`ensure_tlas_state`, destroy-then-rebuild); `crates/renderer/src/vulkan/context/draw.rs:1621-1633,2186-2199` (`rt_flag` gate + swallowed `Err`); `crates/renderer/src/vulkan/context/geometry_pass.rs:490-495` (second consumer of the same flag)
- **Status**: NEW
- **Description**: `tlas_written[frame]` is written `true` in exactly one place
  (`write_tlas`) and never written `false` anywhere in the crate. `ensure_tlas_state`
  destroys the previous frame's TLAS *before* constructing the replacement
  (`tlas.rs:719-732`: `take()` → `device_wait_idle` → destroy), then runs five
  fallible allocation steps (`tlas.rs:773,784,847,862,912`). If any of those
  fails, `build_tlas`'s error propagates to `draw.rs:2192`, which **swallows it
  with `log::warn!` and continues rendering** — `write_tlas` never runs on that
  arm, so descriptor binding 2 keeps pointing at the already-freed
  `VkAccelerationStructureKHR`. Because the latch is never reset, both
  `rt_flag` (`draw.rs:1629`) and the geometry-pass RT gate
  (`geometry_pass.rs:495`) continue to read "enabled," so the frame still
  records and submits ray-query draws against a destroyed AS. This is a
  descriptor-validity spec violation (a bound descriptor must reference a
  live object) compounded by a GPU-side use-after-free (the freed device
  memory may already have been re-handed to a subsequent allocation). Contrast
  `VolumetricsPipeline`, which uses the identically-named latch but **does**
  reset it every dispatch (`volumetrics.rs:1492`) — the fix pattern already
  exists elsewhere in the codebase, it simply was not applied to
  `SceneBuffers`.
- **Evidence**:
  ```
  $ rg -n "tlas_written" crates/renderer/src
  scene_buffer/buffers.rs:185:    pub tlas_written: Vec<bool>,
  scene_buffer/buffers.rs:909:            tlas_written: vec![false; MAX_FRAMES_IN_FLIGHT],
  scene_buffer/descriptors.rs:175:        self.tlas_written[frame_index] = true;     <-- only writer, never reset
  context/draw.rs:1629:  if self.device_caps.ray_query_supported && self.scene_buffers.tlas_written[frame] {
  context/geometry_pass.rs:495:  self.device_caps.ray_query_supported && self.scene_buffers.tlas_written[frame];
  ```
  ```rust
  // context/draw.rs:2192-2199 — the failure arm that never invalidates the latch
  if let Err(e) = accel.build_tlas(&self.device, alloc, cmd, draw_commands, &instance_map, frame) {
      log::warn!("TLAS build failed: {e}");
  } else {
      // ...only this arm calls self.scene_buffers.write_tlas(...)
  }
  ```
- **Impact**: Device lost / GPU hang, or silently garbage ray-query results,
  on any frame where a TLAS resize allocation fails. `ensure_tlas_state` is
  entered on ordinary TLAS growth during exterior-cell streaming, and its
  allocations are host-visible/BAR memory (`tlas.rs:768`) — the scarcest heap
  class on the documented 6 GB RT floor. A realistic BAR-exhaustion transient
  is enough to trigger this, not a contrived OOM. Once triggered, the
  corruption is **permanent for the remainder of the session**: every
  subsequent frame keeps the stale binding and `rt_flag = 1.0`, converting one
  transient allocation failure into a persistent use-after-free rather than a
  single bad frame. `log::warn!("TLAS build failed")` is the only symptom
  visible in the log.
- **Related**: Nearest neighbours `#1227`/REN-D8-NEW-21 (success-path lag) and
  `#1390` (resize `device_wait_idle`, confirmed still present — see Regression
  Guards) cover adjacent ground but not this failure path. No open issue
  matches; not a duplicate.
- **Suggested Fix**:
  1. Add `SceneBuffers::invalidate_tlas(&mut self, frame_index: usize)` that
     clears `tlas_written[frame_index]`, called from `draw.rs`'s `Err` arm
     before the `log::warn!`, so `rt_flag`/the geometry-pass gate fall back to
     the already-existing non-RT path instead of dereferencing the dead handle.
  2. In `ensure_tlas_state`, build the replacement `TlasState` into a local and
     only swap + destroy the old state once construction fully succeeds —
     removes the "destroyed old, no new" window entirely (defense in depth on
     top of fix 1).
  3. Mirror `VolumetricsPipeline`'s pattern with a `debug_assert!(tlas_written[frame])`
     immediately before the geometry pass records RT draws, so a future
     refactor that reintroduces the gap trips in debug builds.
  4. Verify with fault injection in the shape of the existing
     `BYRO_FSR_FORCE_DISPATCH_FAIL` pattern (e.g. `BYRO_FORCE_TLAS_ALLOC_FAIL=1`
     forcing the `tlas.rs:847` create to fail) under `BYRO_VALIDATION=1`,
     confirming no "destroyed acceleration structure" validation error is
     emitted and the frame degrades to the non-RT path.

### SAFE-2026-08-10-02: `mat.set` console command writes raw NaN/Inf into canonical `Material` PBR scalars with no finite guard
- **Severity**: MEDIUM
- **Dimension**: 9 (NIFAL Boundary — NaN/Inf on the GPU)
- **Location**: `byroredux/src/commands/scene.rs:541-579` (`MatSetCommand::execute`, `set_scalar`/`set_vec3` closures and all twelve scalar field arms)
- **Status**: Existing: #2489 ("NIFAL-D6-2026-08-07-03") — OPEN, independently
  re-confirmed reproducible against current HEAD this pass (empirical probe:
  `"nan".parse::<f32>()` → `Ok(NaN)`, `"inf"`/`"-inf"` likewise parse
  successfully). Note the tracker issue is labeled `low`; this audit's
  independent severity assessment is MEDIUM per `_audit-severity.md`'s
  "Wrong/divergent Material out of NIFAL translate boundary" guidance — flag
  the discrepancy for triage rather than silently overriding the label.
- **Description**: `set_scalar` writes `*slot = v[0]` verbatim after
  `s.parse::<f32>()` with no `is_finite()` check. `Material.metalness`/
  `.roughness` are plain (non-`Option`) `f32` fields whose only production-path
  NaN gate is `Material::resolve_pbr()`, called exactly once at
  `translate_material` spawn time. `mat.set` mutates the already-spawned
  `Material` component directly and never re-invokes `resolve_pbr()`, so
  `mat.set <id> metalness nan` writes the sentinel straight through to
  `GpuMaterial` → the per-frame SSBO → the GGX/Disney BRDF terms in
  `triangle.frag`, and can poison SVGF/TAA temporal history for that entity.
- **Evidence**: `scene.rs:543` — `*slot = v[0];  // no is_finite() check`.
- **Impact**: Reachable only via the embedded TCP debug server (`byro-dbg`,
  port 9876) — a developer/tooling surface, not shipped-game player input.
  Blast radius is the edited entity's draw, but can poison shared denoiser
  temporal history until the entity is despawned or overwritten again.
- **Related**: #2489 (open, unfixed); sibling precedent `#1535`
  (`normal_alpha_spec_roughness`'s finite-glossiness guard) shows the
  codebase's usual pattern for this class of gap.
- **Suggested Fix**: Already scoped by #2489 — reject/clamp non-finite input
  in `set_scalar` before the write, or call `Material::resolve_pbr()` after
  every `mat.set` mutation touching `metalness`/`roughness`.

### SAFE-2026-08-10-03: `EguiPass` rebuild on swapchain format change discards the egui-ash-renderer texture map, which egui never re-sends — debug overlay dies permanently with per-frame log spam
- **Severity**: MEDIUM
- **Dimension**: 10 (debug-ui Teardown & Shared-Allocator Safety)
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:894-930`; consequence in `crates/renderer/src/vulkan/egui_pass.rs:186-273`
- **Status**: NEW (side effect of the #2475 fix, which remains correct and
  CLOSED for the render-pass/`srgb_framebuffer` half it targeted)
- **Description**: On a swapchain **surface-format change**, the resize path
  tears down the whole `EguiPass` and reconstructs it via `EguiPass::new`,
  which builds a brand-new `egui_ash_renderer::Renderer` with empty
  `managed_textures`/`textures` maps. The CPU-side `egui::Context` is not
  rebuilt and has no way to know this happened — egui's `TextureManager` only
  emits a `TexturesDelta` entry for a texture that was *created or changed*,
  so the already-uploaded font atlas (`TextureId::Managed(0)`) is never
  re-sent. Every subsequent `cmd_draw`/`set_textures` lookup against that
  `TextureId` is a hard `RendererError::BadTexture`.
- **Evidence**: `resize.rs:907-918` (`pass.destroy(...)` → `EguiPass::new(...)`,
  no callback into `DebugUiState`/`egui::Context`);
  `egui-ash-renderer-0.11.0/src/renderer/mod.rs:585-589`
  (`self.textures.get(&m.texture_id).ok_or(RendererError::BadTexture(..))?`);
  `context/draw.rs:3072-3074` swallows the resulting `Err` with
  `log::error!` every frame.
- **Impact**: After any HDR toggle / monitor move / driver format
  renegotiation, the debug overlay renders nothing for the rest of the
  session plus one `log::error!` per frame. No Vulkan violation and no UB —
  the render-pass begin/end balance guard still runs and the error path
  returns before any handle dereference — so this is a functional regression
  and log spam, not memory corruption. Blast radius is the debug overlay
  only; rare trigger.
- **Related**: #2475 (the rebuild this rides on, CLOSED/correct), #2247
  (pending-output merge).
- **Suggested Fix**: After a successful `EguiPass::new` on the format-change
  arm, force egui to re-upload its font atlas — rebuild `DebugUiState`'s
  `egui::Context` or call the egui API that invalidates it (re-setting
  `Style`/`FontDefinitions`), or have `EguiPass` expose a `texture_state_lost`
  flag the binary polls before its next `run()`.

### SAFE-2026-08-10-04: `vkGetBufferDeviceAddress` re-queried every frame per skinned draw for an immutable value
- **Severity**: LOW (hygiene/CPU-cost, not a spec violation — the SAFETY comment and its invariant are correct)
- **Dimension**: 5 (Vulkan Spec Compliance)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2499-2512`
- **Status**: NEW
- **Description**: Inside the per-`draw_cmd` instance-upload loop, every
  skinned draw (`bone_offset != 0`) issues a fresh `get_buffer_device_address`
  driver dispatch. A `VkBuffer`'s device address is immutable for the
  buffer's lifetime, established at bind-memory time. Every other
  device-address query in the renderer is hoisted to resource-creation time
  (`skin_compute.rs:412-431`, `blas_static.rs:206-222`, `tlas.rs:786`); this
  is the sole steady-state per-frame caller.
- **Impact**: One redundant driver round-trip per skinned instance per frame
  — measurable on NPC-dense cells, no incorrectness (documented
  CPU-bottleneck-is-a-bug stance, `user_hardware`).
- **Suggested Fix**: Cache `device_address: u64` on `SkinSlot` at
  `create_slot` time (reuse the address already queried there) and read
  `slot.device_address` in the draw loop.

### SAFE-2026-08-10-05: `GLASS_RAY_BUDGET`'s Rust/GLSL lockstep pin now certifies a dead value — `AdaptiveRayBudget` tiers gate the real limit since `5798e467`
- **Severity**: LOW
- **Dimension**: 7 (RT IOR-Refraction Safety)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/ray_budget.rs:96-139`; `crates/renderer/src/shader_constants_data.rs:135-144`; `crates/renderer/shaders/include/shader_constants.glsl:52-54`; `crates/renderer/shaders/triangle.frag:1514-1529`; `crates/renderer/shaders/include/bindings.glsl:325-334`
- **Status**: NEW
- **Description**: `GLASS_RAY_BUDGET` (Rust `2_097_152` / GLSL `2097152u`)
  remains in exact lockstep and passes its pin test, but since commit
  `5798e467`'s adaptive-ray-budget refactor, `triangle.frag` gates against
  `rayBudget.glassRayLimit`, an SSBO word written from
  `AdaptiveRayBudget::settings()`'s four hard-coded tiers
  (`262_144`/`524_288`/`1_048_576`/`2_097_152`), none derived from or asserted
  against `GLASS_RAY_BUDGET`. No runtime shader references the `#define`
  anymore (`GLASS_RAY_COST` is still live); the only remaining mention is now
  a false statement in a `bindings.glsl` comment.
- **Impact**: No safety regression — the recursion guard
  (`REFRACT_PASSTHRU_BUDGET`) is unrelated and intact, and the atomic gate
  still bounds the per-frame ray flood. Three hygiene consequences: the
  lockstep pin now gives false confidence about what's enforced; tightening
  `GLASS_RAY_BUDGET` to bound a runaway would have zero effect; and the
  engine's actual default startup cap (`1_048_576`) and GPU-pressure floor
  (`262_144`) are 2×/8× below the documented figure, so the stipple artifact
  `6efe1706` raised the budget to avoid could reappear at the low tiers —
  a visual claim that needs RenderDoc/live verification, not asserted here.
- **Related**: #1438 (atomicAdd overshoot, separate/accepted); `5798e467`.
- **Suggested Fix**: Derive the tier ladder from `GLASS_RAY_BUDGET`
  (e.g. right-shifted fractions) or pin `AdaptiveRayBudget`'s top tier equal
  to it via a unit test; correct the `bindings.glsl` comment.

### SAFE-2026-08-10-06: No test asserts the 24 `DBG_*` debug-flag bits are distinct/single-bit
- **Severity**: LOW
- **Dimension**: 7 (RT IOR-Refraction Safety — debug-flag catalog)
- **Location**: `crates/renderer/src/shader_constants_data.rs:556-581`; `crates/renderer/src/shader_constants.rs:52-70,187,397`
- **Status**: NEW
- **Description**: `DBG_VIZ_GLASS_PASSTHRU = 0x80` has not collided — all 24
  `DBG_*` constants are currently distinct single-bit values — but the three
  existing guards check catalog coverage, Rust↔GLSL value equality, and
  no-shader-redeclaration; none checks that two entries don't share a bit. A
  future `pub const DBG_FOO: u32 = 0x80;` would pass all three while silently
  aliasing the glass-passthru diagnostic. The catalog is actively growing (22
  → 24 since the prior audit).
- **Impact**: Diagnostic-only blast radius — two `BYROREDUX_RENDER_DEBUG`
  bisect paths would fire together, wasting a renderer-bisect session. No
  production-frame correctness effect.
- **Related**: #1482, #1860 (prior catalog-drift fixes).
- **Suggested Fix**: Extend the coverage test with
  `assert!(seen.insert(value))` and `assert_eq!(value.count_ones(), 1)` over
  `DBG_BITS`.

### SAFE-2026-08-10-07: Two live doc comments quote superseded `GpuMaterial` figures ("75 scalar fields", "260-byte construction")
- **Severity**: LOW
- **Dimension**: 6 (R1 Material Table Layout Soundness)
- **Location**: `crates/renderer/src/vulkan/material.rs:1143`; `byroredux/src/render/static_meshes.rs:734`
- **Status**: NEW (sibling drift sites already tracked: #2415 for
  `gpu_instance_layout_tests.rs:939,990`, #2483 for `gpu_types.rs`/
  `constants.rs:168` — neither names these two files)
- **Description**: `intern_by_hash`'s collision-policy doc reasons about hash
  quality "over 75 scalar fields" (struct has 87); `static_meshes.rs`'s
  intern-skip justification cites a "260-byte construction" (current size is
  348 B, two growths past #804's 260 B).
- **Impact**: No runtime effect (the size/offset pin tests are authoritative
  and green) — a reader auditing the dedup-hash-quality argument gets a wrong
  denominator, and the perf-justification comment understates saved work by
  34%.
- **Related**: #2415, #2483 (same drift class, different files).
- **Suggested Fix**: Update both literals, or drop the numerals entirely so
  future growth can't re-stale them; fold into the same pass as #2415/#2483.

### SAFE-2026-08-10-08: Skill doc understates `upload_materials`'s over-cap guard as a `debug_assert` when it is a release-visible `assert!`
- **Severity**: LOW
- **Dimension**: 6 (R1 Material Table Layout Soundness) — doc-rot in `.claude/commands/audit-safety/SKILL.md`
- **Location**: `.claude/commands/audit-safety/SKILL.md:213-215` vs `crates/renderer/src/vulkan/scene_buffer/upload.rs:646-652`
- **Status**: NEW
- **Description**: The skill's Dimension-6 checklist says `upload_materials`
  "`debug_assert`s `len <= MAX_MATERIALS`". The code actually carries an
  unconditional release `assert!` (deliberately hardened under #1064 so a
  cap-breaking refactor cannot silently truncate uploads in release builds),
  in addition to the `.min()` clamp. An auditor reading only the skill text
  could wrongly conclude release builds are unguarded and file a phantom
  HIGH "no release-mode SSBO over-index guard" finding — exactly the
  stale-premise class `feedback_audit_findings.md` warns about.
- **Related**: #797, #1064, #807.
- **Suggested Fix**: Change "`debug_assert`s" to "`assert!`s (release-visible,
  #1064)" in the skill's Dimension-6 bullet.

### SAFE-2026-08-10-09: `dispatch`'s early `?` returns can drop that frame's egui texture free-list, stranding textures + descriptor sets
- **Severity**: LOW
- **Dimension**: 10 (debug-ui Teardown & Shared-Allocator Safety)
- **Location**: `crates/renderer/src/vulkan/egui_pass.rs:203-225` vs `:270`
- **Status**: NEW
- **Description**: `self.pending_free = output.textures_delta.free;` is
  `dispatch`'s last statement; two earlier steps (`free_textures(&drained)?`,
  `set_textures(..)?`) propagate errors with `?`. On either error, `output`
  is consumed and the arriving frame's free-list is dropped — egui never
  re-emits those `TextureId`s, so the corresponding image/memory/sampler/
  descriptor set stay resident until `EguiPass` is destroyed.
- **Impact**: Bounded in steady state (these calls don't normally fail); it
  becomes material only in combination with SAFE-2026-08-10-03, where
  `set_textures` fails every frame. Overlay-only VRAM, released at teardown.
- **Related**: SAFE-2026-08-10-03; #1427 (teardown-side flush of the same field).
- **Suggested Fix**: Stash `output.textures_delta.free` into `self.pending_free`
  before step 1 runs (or re-append `drained` on the step-1 error) so no error
  path can strand a free-list.

### SAFE-2026-08-10-10: egui `pending_free` one-frame-defer safety silently depends on `MAX_FRAMES_IN_FLIGHT == 2`; the sanctioned bump route in `sync.rs` doesn't account for it
- **Severity**: LOW (latent — not a live defect today)
- **Dimension**: 10 (debug-ui Teardown & Shared-Allocator Safety)
- **Location**: `crates/renderer/src/vulkan/egui_pass.rs:10-16,82-88,200-208`; `crates/renderer/src/vulkan/sync.rs:8-36`; `crates/renderer/src/vulkan/context/draw.rs:1346-1360`
- **Status**: NEW
- **Description**: The egui defer's safety argument ("the fence wait at the
  top of `draw_frame` ensures the prior frame's command buffer has
  GPU-completed") is only true because the wait covers both slots and there
  are exactly two. `sync.rs`'s `const_assert` pinning `MAX_FRAMES_IN_FLIGHT ==
  2` is documented entirely against a *different* consumer (#870, the shared
  depth image) and lists route (a) "making the depth image per-frame" as an
  acceptable bump path — but route (a) does not extend the fence wait. At
  `MAX_FRAMES_IN_FLIGHT == 3`, the immediately-preceding frame's command
  buffer could still be executing when `free_textures` destroys images it may
  reference — a GPU use-after-free invisible to `cargo test`.
- **Impact**: Zero today (compile-time pinned at 2). The exposure is a
  *sanctioned* future refactor closing #870 via route (a) silently re-arming
  a use-after-free in the overlay.
- **Related**: #870, #1427.
- **Suggested Fix**: Add the egui `pending_free` defer as a second consumer
  in the `sync.rs` const_assert comment that mandates route (b) (extend the
  fence wait), and reference `MAX_FRAMES_IN_FLIGHT` explicitly in
  `egui_pass.rs`'s field doc.

### SAFE-2026-08-10-11: Graphics-queue Mutex held across `egui-ash-renderer`'s internal `vkQueueWaitIdle` mid-frame-recording
- **Severity**: LOW
- **Dimension**: 10 (debug-ui Teardown & Shared-Allocator Safety)
- **Location**: `crates/renderer/src/vulkan/egui_pass.rs:220-225`; upstream `egui-ash-renderer-0.11.0/src/renderer/vulkan.rs:580-590`
- **Status**: NEW (residual of CLOSED #1713/CONC-D1-01; noted in prose in `docs/audits/AUDIT_CONCURRENCY_2026-06-23.md` but never filed)
- **Description**: #1713's fix correctly narrowed the lock to the
  `set_textures` call (confirmed still intact). Inside that call, upstream's
  `execute_one_time_commands` does `queue_submit` followed by a full
  `queue_wait_idle` — not a fence wait — held under the same Mutex, in the
  middle of the main frame command buffer's recording.
- **Impact**: No correctness defect (`vkQueueWaitIdle` requires exactly the
  external synchronisation the Mutex provides, and the prior both-slot fence
  wait means the queue is already drained). Cost is a hard CPU stall on any
  frame with a non-empty `textures_delta.set` (overlay open / font-atlas
  growth / DPI change — not steady state) and a serialisation point for any
  future second graphics-queue producer.
- **Related**: #1713 (CLOSED), #1421 (CLOSED).
- **Suggested Fix**: No engine-side fix without forking `egui-ash-renderer`;
  add a one-line note that the upstream wait is a `vkQueueWaitIdle`, and
  consider a debug_assert/log if `textures_delta.set` is non-empty across
  many consecutive frames (would indicate SAFE-2026-08-10-03).

### SAFE-2026-08-10-12: Skill doc's renderer SAFETY-comment ratio ("~9 in 10") is stale — actual ratio is 10/10, structurally enforced
- **Severity**: LOW
- **Dimension**: 4 (Unsafe-Block Discipline) — doc-rot in `.claude/commands/audit-safety/SKILL.md`
- **Location**: `.claude/commands/audit-safety/SKILL.md:27-28`
- **Status**: NEW
- **Description**: The line predates (or was never updated after)
  `#![deny(clippy::undocumented_unsafe_blocks)]` landing in
  `crates/renderer/src/lib.rs` under #1904. Verified live this pass via
  `cargo clippy -p byroredux-renderer --lib -- -W
  clippy::undocumented_unsafe_blocks` (zero warnings) plus an injected
  uncommented-`unsafe` probe (correctly failed the lint, then reverted). The
  actual ratio is 10/10, structurally enforced — not "roughly nine in ten"
  from a one-time sweep.
- **Impact**: Wastes future Dimension-4 audit time hunting for a gap that no
  longer exists in `renderer/src`; no code-correctness impact.
- **Related**: #2274 (sibling Dimension-3 doc-rot finding, same class).
- **Suggested Fix**: Update the "Scale of the surface" paragraph to state the
  `#![deny(...)]` gate exists and point future audits at the clippy command
  as the fast, authoritative check, reserving manual sampling for
  invariant-soundness (which the gate cannot verify).

---

## Existing Issues Re-confirmed Live (not re-reported as new)

| Issue | Dimension | Description | Status this pass |
|---|---|---|---|
| #2482 (REN-D2-2026-08-07-03) | 7 | Refraction passthru loop never decrements `tMax`, ~3× documented reach | Confirmed still live in current source |
| #2274 (SAFE-2026-08-03-04) | 3, 4 | SKILL doc-rot in Dimension 3's leak-inventory prose | Confirmed still open/unfixed |

## Regression Guards Re-verified Fixed (no action needed)

| Item | Dimension | Original issue | Status |
|---|---|---|---|
| fsr3-sys `vulkan_context_smoke.rs` example — unsafe/SAFETY gap | 4 | prior audit finding (2026-08-07) | FIXED — 23/23 sites now commented |
| `unload.rs::finish_unload_batch` BLAS-scratch shrink call — SAFETY comment gap | 4 | prior audit finding (2026-08-07) | FIXED — comment restored |
| `synthesize_packed_havok_proxy` unbounded/infinite collider from unclamped REFR scale | 9 | SAFE-2026-08-07-01 / #2543 (HIGH) | FIXED — finite check + clamp to `RT_ABSOLUTE_PRECISION_CEILING` after the scale multiply |
| fsr3-sys `unsafe fn` `# Safety` gaps | 1 | SAFE-2026-08-07-02 / #2544 (MEDIUM) | FIXED — confirmed still fixed |

---

## Dimension-by-Dimension PASS Summary (regression guards confirmed intact)

**Dimension 1 — FFI Lifetime Safety**: cxx-bridge remains a no-pointer
placeholder (`native_hello() -> String` only); `fsr3-sys`'s two `unsafe fn`s
(`Context::create`/`dispatch`) both carry `# Safety` doc sections matched by
call-site behavior; `#[repr(C)]` ABI layout cross-checked field-for-field
against the native shim; Drop soundness confirmed (unique `NonNull`, no
`Clone`/`Send`/`Sync`); sole production call site (`frame_upscaler.rs`)
upholds the device-idle-before-destroy contract, pinned by a static-source
test.

**Dimension 2 — Memory Corruption / UB**: `World::get`/`ComponentRef` cached-
pointer contract holds (guard field outlives every deref, no interior
mutability bypassing `RwLock`); `#[repr(C)]` GPU structs in `gpu_types.rs`
have zero `[f32;3]` fields; NIF `read_pod_vec`/`read_pod_vec_from_cursor`
both gate on `checked_mul` overflow + a 256 MB alloc cap before any unsafe
cast; `sfmaterial::BuiltinType::from_u32` is a checked match with an `Err`
arm, no transmute; `pex::OpCode::from_u8`'s transmute is gated by a
range-check against contiguous `#[repr(u8)]` discriminants `0..51`, full-range
tested; block-walk/collision-shape recursion is depth-capped (128/64) with
cycle detection on the collision side.

**Dimension 3 — Memory & Resource Leaks**: Rapier body/collider/joint release
on cell unload intact for both single-body and ragdoll paths (7 tests
passing); deferred-destroy tick runs after the dual-fence wait, shutdown
sweep drains every queue; `AllocatorResource` removal-before-drop is now a
real `impl Drop for App`, covering the panic-unwind path the field-order-only
approach missed; full GPU allocation inventory (BLAS/TLAS, G-buffer, SVGF,
TAA, caustic/water-caustic, SkinSlots, MaterialBuffer, volumetrics, bloom)
verified wired into `VulkanContext::Drop`; `AnimationClipRegistry` dedup +
release confirmed at 5 production eviction call sites, `MaterialTable` is
a bounded per-frame structure (cleared every `build_render_data`), not a
leak risk (contra stale skill-doc prose already tracked as #2274).

**Dimension 4 — Unsafe-Block Discipline**: zero uncommented `unsafe` blocks
anywhere in the workspace (`crates/` + `byroredux/`, incl. `examples/`);
`crates/renderer/src`'s ratio is structurally 10/10 via
`#![deny(clippy::undocumented_unsafe_blocks)]` (#1904); ~25 renderer FFI/
unsafe-cast sites spot-checked for invariant soundness beyond mere comment
presence, all held; unsafe token counts across all crates matched the skill
doc's expected figures.

**Dimension 5 — Vulkan Spec Compliance**: TLAS UPDATE-mode primitive-count
match (VUID-pInfos-03708) guarded in both grow/shrink directions; the #1390
resize `device_wait_idle` is present; skinned-BLAS refit vertex/index/flag
parity validated against the original BUILD (VUID-pInfos-03667); AS-build →
shader-read barriers correct on the success path with proper `AS_READ|
AS_WRITE` src scope on UPDATE-mode rebuilds; `VK_KHR_ray_query` and
`buffer_device_address` gated in lockstep between device features and the
gpu-allocator; per-image semaphores + acquire/submit leak-recovery paths
intact with unit-test coverage; `ERROR_OUT_OF_DATE_KHR` correctly returns
before `acquire_next_image` signals; `MAX_FRAMES_IN_FLIGHT == 2` const-
asserted; `VOLUMETRIC_OUTPUT_CONSUMED` gate is read (not assumed) by its
caller; `initialize_layouts` UNDEFINED→GENERAL coverage confirmed complete
across gbuffer/svgf/taa/caustic/water_caustic/bloom/volumetrics, including
resize re-invocation; SPIR-V reflection descriptor-drift test present and
wired. **Caveat**: this pass was static-analysis only — no validation-layer
or RenderDoc run was performed (per the no-parallel-engine-launch policy),
so per-frame barrier/layout correctness beyond `initialize_layouts` coverage
is explicitly left unasserted rather than claimed clean.

**Dimension 6 — Material Table Layout Soundness**: `gpu_material_size_is_348_bytes`
and `gpu_material_field_offsets_match_shader_contract` both pass
(`cargo test -p byroredux-renderer --lib material`: 44/44); mechanically
re-derived 87 flat scalar fields × 4 B = 348 B on both the Rust and GLSL
sides; zero `[f32;N]`/vec3 fields anywhere in `GpuMaterial`; no uninitialized
pad bytes (`Default` is a full struct literal, no `..Default::default()`
tail); intern-cap (16384) and upload-truncation are in lockstep and stronger
than documented (a release `assert!`, not just `debug_assert` — see SAFE-2026-08-10-08);
`GpuInstance.material_id` proven in-range end-to-end (only two writers, both
via `intern_by_hash`); `ui.vert` declares no `MaterialBuffer` at all (reads
`inst.textureIndex` directly), so the #785 regression class structurally
cannot recur; all five GLSL `GpuInstance` mirrors diffed byte-identical;
GLSL↔Rust field order machine-checked, not eyeballed.

**Dimension 7 — RT IOR-Refraction Safety**: `REFRACT_PASSTHRU_BUDGET = 2`
loop cap present and structurally sound (max 3 traces, no spin path); glass-
passthru identity check keyed on `materialKind == MATERIAL_KIND_GLASS`
(#789), not texture equality; #1438's atomicAdd-overshoot nuance confirmed
still the accepted behavior, not a new bug; Frisvad/Duff orthonormal basis
confirmed the sole basis builder (no naive `cross(N, up)` construction
remains, denominator provably never zero); interior IOR miss fallback uses
cell ambient, not global sky tint (#1125 intact); `DBG_VIZ_GLASS_PASSTHRU =
0x80` has not collided (24 distinct single-bit values, script-verified);
committed SPIR-V postdates the `.frag` source; secondary-ray (GI-path) glass
interaction is bounded by `MAX_PATH_SEGMENTS`, not unbounded.

**Dimension 8 — NPC/Animation Spawn Safety**: B-spline pose-fallback
`FLT_MAX` sentinel gate (#772) intact across all four fallback producers plus
the mainline `is_key_value_sane` guard; `AnimationClipRegistry` lowercased-
path dedup (#790) confirmed at the production NPC-spawn call site, second
`add()` call site in `cell_loader/partial.rs` confirmed safe (cache-hit
gated, not a per-cell leak); no era-based gating rules out
`NiBSplineCompTransformInterpolator` — block-name dispatch is unconditional,
confirmed reachable on FO3/FNV; `SkinSlotPool` overflow guard confirmed
(one-shot warn + cumulative counter, `None` on cap rather than truncation,
caller drops the entity, draw loop falls back to bind pose via slot 0,
dedicated test file covers both at-capacity and over-capacity cases).

**Dimension 9 — NIFAL NaN/Inf Boundary**: `translate_material` is the sole
producer of the deliberate NaN sentinel and always calls `resolve_pbr()`
before returning; every renderer-bound `Material` producer traced (both
`translate_material` call sites, the no-Material static-mesh fallback, all
seven Cornell-harness constructors, the M45 save/load restore path — the
last empirically confirmed to reject NaN/Inf via `serde_json`'s null-
coercion plus a hard deserialize error) constructs finite values or runs
`resolve_pbr()`; collision translate (`BhkMultiSphereShape`/
`BhkConvexListShape`) is finite-gated per-primitive with bounded recursion
(depth 64 + cycle detection) and length-checked bulk allocation; the prior
audit's HIGH finding on the packed-Havok proxy collider (unbounded
half-extents) is confirmed fixed (#2543 — finite check + clamp after the
scale multiply); particle emitter extraction is finite/positive-gated at
both extraction and consumption, with a hard engine-preset spawn cap
independent of authored NIF data. The one open gap is the `mat.set` console
command (SAFE-2026-08-10-02 / Existing #2489) — a live-mutation bypass of
the boundary, not a construction-time bypass.

**Dimension 10 — debug-ui Teardown & Shared-Allocator Safety**:
`DebugUiState` confirmed CPU-only, holds no Vulkan handle; `Option<EguiPass>`
teardown confirmed to run ~330 lines ahead of `vkDestroyDevice`, with
`EguiPass`'s own `Drop` firing while the device and allocator are still live;
the allocator `Arc` clone is released before the `Arc::try_unwrap` strong-
count check, so the overlay cannot trip the leak-guard arm; #1427's
pending-free-flush-before-destroy guard intact; the one-frame `pending_free`
defer is sufficient today because it's backed by the dual-fence wait and the
`MAX_FRAMES_IN_FLIGHT == 2` const-assert (see SAFE-2026-08-10-10 for the
latent fragility); allocator mutex held only per-call, never across
`queue_submit`/`queue_wait_idle`; #1713/CONC-D1-01's queue-lock narrowing
confirmed intact (see SAFE-2026-08-10-11 for the residual upstream-internal
wait); render-pass begin/end balance on error (#1491/#1637) intact;
transfer-pool queue-family legality (#1420) intact; resize path leaks
nothing on either the framebuffer or format-change arm.

---

## Deduplication

Every dimension worker independently fetched/grepped
`/tmp/audit/issues.json` (`gh issue list --repo matiaszanolli/ByroRedux
--limit 200`) and scanned `docs/audits/` for prior coverage before filing.
Cross-dimension check performed at aggregation time: no finding in this
report duplicates another dimension's finding (the closest adjacency —
Dimension 5's TLAS latch vs. Dimension 3's leak/lifetime guards — is a single
finding correctly attributed to Dimension 5, cross-referenced rather than
double-filed). Two pre-existing OPEN issues (#2482, #2274) were independently
re-confirmed live by two different workers each pass and are recorded once
above, not duplicated per-dimension.

---

## Summary for Triage

The single actionable priority from this pass is **SAFE-2026-08-10-01**
(CRITICAL): the TLAS-latch-never-resets bug is a real, reachable use-after-
free / descriptor-validity violation with a small, well-scoped fix (invalidate
the latch on the `Err` arm; the codebase already has the correct pattern in
`VolumetricsPipeline` to copy from). Everything else found this pass is
MEDIUM or below: two console/live-edit NaN-boundary and texture-lifecycle
regressions reachable only through developer tooling (debug server, format-
change resize), and a cluster of LOW-severity doc-rot / hygiene items with no
runtime effect. The overall safety posture is strong — ten dimensions'
worth of regression guards (FFI lifetime, ECS pointer-cache soundness,
`#[repr(C)]` GPU-struct layout, NIF POD-read bounds, three drop-ordering leak
guards, TLAS/BLAS build discipline outside the one latch bug, glass/IOR
guards, NPC-spawn safety, the NIFAL NaN boundary, and debug-ui teardown
ordering) were independently re-derived from current source this pass and
found intact, and the workspace has **zero** uncommented `unsafe` blocks,
structurally enforced by a clippy deny-lint.
