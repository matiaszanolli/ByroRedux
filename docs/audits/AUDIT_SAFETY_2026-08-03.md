# Safety Audit — 2026-08-03

Scope: `unsafe` blocks, FFI boundaries, memory-lifecycle code across the
ByroRedux workspace, per `.claude/commands/audit-safety.md` (10 dimensions)
and the shared protocol in `_audit-common.md` / `_audit-severity.md`.

Method: this is the 21st `/audit-safety` pass (prior reports run from
2026-04-05 through 2026-07-25). Given that history, the dominant expectation
going in was that most of the ten dimensions are regression guards — confirm
a known fix is still in place — rather than fresh discovery. That expectation
held: **this pass found zero CRITICAL or HIGH findings.** All ten dimensions'
load-bearing invariants (FFI lifetime contracts, the ECS cached-pointer
contract, `#[repr(C)]` GPU-struct layout, the three drop-ordering regression
guards, TLAS/BLAS build discipline, glass/IOR guards, NPC-spawn safety,
NIFAL's NaN boundary, debug-ui teardown) were re-verified against the current
source and found intact. Four LOW findings are recorded below — all
maintainability / coverage gaps, not defects.

Deduplication: checked against `gh issue list` (47 open issues fetched to
`/tmp/audit/issues.json`) — no open issue overlaps any finding in this
report. One open issue (`#2153`, lock-stack safety) is adjacent territory
(CHARAL locking) but out of this audit's `unsafe`/FFI/memory-lifecycle scope.

---

## Findings

### SAFE-2026-08-03-01: No Miri coverage for the ECS cached-pointer aliasing model
- **Severity**: LOW
- **Dimension**: 2 (Memory Corruption / UB)
- **Location**: `crates/core/src/ecs/query.rs:93-144`
- **Status**: NEW
- **Description**: `QueryRead`/`QueryWrite`/`ComponentRef` cache a raw
  `*const T`/`*mut T` resolved once in `new()` from the guard's boxed storage,
  then deref it in the hot path without re-touching the guard. The soundness
  argument is airtight at the type level today (guard is never re-borrowed
  after construction; borrow-checker gates `&`/`&mut` coexistence), but that
  is a *convention*, not something the compiler enforces under Stacked
  Borrows — a future method that reads `self.guard` after construction would
  silently invalidate the cached pointer's tag while still compiling and
  passing `cargo test`. There is no `miri` job anywhere in the repo
  (`grep -rln miri .github/` → empty).
- **Evidence**: `crates/core/src/ecs/query.rs:23-35` (guard declared before
  the cached pointer field; no `Drop` impl touches the pointer), `:64,135,
  143,289` (the four SAFETY-commented unsafe derefs in the crate).
- **Impact**: Latent only — no current defect. A future refactor could
  reintroduce #35/#1367-class unsoundness without any test catching it.
- **Suggested Fix**: Add a `cargo +nightly miri test -p byroredux-core`
  job scoped to the `ecs` module (or replace the `guard` field with
  `PhantomData` + a manually-managed drop so re-deriving is structurally
  impossible).

### SAFE-2026-08-03-02: NIF header POD-read overflow guard is a caller contract, not a construction guarantee
- **Severity**: LOW
- **Dimension**: 2 (Memory Corruption / UB)
- **Location**: `crates/nif/src/header.rs:360-385`
- **Status**: NEW
- **Description**: Unlike `NifStream::read_pod_vec` (`crates/nif/src/stream.rs`,
  which calls `check_alloc` internally before allocating), the header mirror
  `read_pod_vec_from_cursor` deliberately omits an internal allocation-size
  cap (documented at `header.rs:357-359`) and relies on each caller checking
  `remaining bytes` first. Both of the two current call sites
  (`header.rs:195-205` and `:225-235`) do this correctly. But a third caller
  added later without the preceding budget check would allocate up to 4 GB
  (`u32::MAX` blocks × 4 B) before `read_exact` fails on a malformed/adversarial
  NIF header.
- **Evidence**: `header.rs:360-385` (no `check_alloc` call, unlike
  `stream.rs:340-372`'s equivalent).
- **Impact**: No live defect — both callers are correct today. Risk is
  purely in future callers of this private helper.
- **Suggested Fix**: Pass `total_bytes` into `read_pod_vec_from_cursor` and
  call the existing `check_header_alloc` (`header.rs:394`) internally, matching
  `stream.rs`'s pattern, so the guard can't be forgotten by a new call site.

### SAFE-2026-08-03-03: Stale field-count in `MaterialTable::intern`'s collision-policy comment
- **Severity**: LOW
- **Dimension**: 6 (R1 Material Table Layout Soundness)
- **Location**: `crates/renderer/src/vulkan/material.rs:1145`
- **Status**: NEW
- **Description**: The doc comment describing hash-collision odds reads
  "rare on FxHash's 64-bit output over 75 scalar fields, #1368" — but
  `GpuMaterial` has carried 87 fields (348 B) since the 2026-07-27 growth
  that added the twelve supplemental texture-role indices. The size/offset
  pins themselves (`gpu_material_size_is_348_bytes`,
  `gpu_material_field_offsets_match_shader_contract`) are correct and
  up to date — only this prose comment drifted.
- **Evidence**: `material.rs:1145` vs. the 87-field struct at `material.rs:76-291`.
- **Impact**: Cosmetic; no functional effect. Flagged per the repo's
  path/symbol-reference hygiene convention (stale numbers in load-bearing
  comments are worth catching even when harmless).
- **Suggested Fix**: Update the comment to 87 fields, or drop the specific
  count so it can't go stale again on the next field addition.

### SAFE-2026-08-03-04: SKILL doc-rot — `audit-safety.md`'s Dimension 3 leak-inventory descriptions no longer match the code
- **Severity**: LOW
- **Dimension**: 3 (Memory & Resource Leaks) / meta
- **Location**: `.claude/commands/audit-safety.md` (Dimension 3 bullets)
- **Status**: NEW
- **Description**: Two specific claims in the skill's Dimension-3 text are
  stale relative to the current source, though the underlying code is
  correct:
  1. **"CPU-side unbounded growth… The MaterialTable dedup map… [is a]
     known per-cell-growth risk."** In fact `MaterialTable::clear()` is
     called once per frame at the top of `build_render_data`
     (`byroredux/src/render/mod.rs:531`), so the dedup map is rebuilt fresh
     every frame — it cannot grow across cells or across the session at all.
     This is not a risk; it was mis-scoped in the skill text.
  2. **"the `DeferredDestroyQueue<T>` shared by mesh + BLAS + BLAS-scratch
     buffer + texture + skin compute"** — a grep for
     `DeferredDestroyQueue<` across `crates/renderer/src/` finds exactly
     three instantiations: `crates/renderer/src/mesh.rs:169`
     (`(Option<GpuBuffer>, Option<GpuBuffer>)`, i.e. mesh vertex/index
     buffers) and two in `crates/renderer/src/vulkan/acceleration/mod.rs:158,175`
     (`BlasEntry` and the BLAS scratch `GpuBuffer`). No instantiation was
     found for texture or skin-compute resources — those subsystems may use
     a different (and unverified-by-this-audit) deferred-free mechanism, or
     the skill's claim is simply inaccurate.
- **Evidence**: `byroredux/src/render/mod.rs:518-531`; the three
  `DeferredDestroyQueue<` declaration sites above.
- **Impact**: None on running code — both the mesh and BLAS/scratch drain
  paths were independently verified correct this pass (tick runs after fence
  wait in `context/draw.rs:1369-1401`; shutdown drain in
  `acceleration/blas_static.rs:100-141` and `mesh.rs:1391`, both gated on a
  preceding `device_wait_idle` per their `# Safety` docs). Only the *skill's
  own description* is wrong, which risks a future audit chasing a
  non-existent "MaterialTable leak" or over-trusting an unverified
  texture/skin-compute deferred-destroy claim.
- **Suggested Fix**: Update `audit-safety.md` Dimension 3 to (a) drop the
  MaterialTable growth-risk framing, and (b) either verify and name the
  actual texture/skin-compute deferred-free mechanism or narrow the claim to
  the three confirmed `DeferredDestroyQueue<T>` users.

---

## Premises Checked and Disproved (no finding)

- **"The renderer's ~760 `unsafe` occurrences in `crates/renderer/src` carry
  a large SAFETY-comment gap (the skill's own ~676-vs-761 estimate),
  concentrated in the heaviest files."** A whole-workspace scan (745
  `unsafe`-token lines across `crates/{renderer,nif,core,fsr3-sys,pex,plugin,
  sfmaterial,ui,facegen,cxx-bridge}/src` + `byroredux/src`, filtering string/
  comment false positives) with a bidirectional ±12/+4-line context window
  turned up 128 candidates on first pass. Every one of those 128 was a false
  positive from a too-narrow lookback: manual inspection of all 17 that
  survived a wider window showed every single site actually carries either
  an inline `// SAFETY:` comment (sometimes on the line *after* the opening
  `unsafe {`, which the narrower heuristic missed) or a function-level
  `/// # Safety` doc block placed 15-25 lines above the call (common in
  `frame_upscaler.rs`'s FSR barrier-recording helpers and
  `acceleration/{memory,blas_static,blas_skinned}.rs`'s BLAS lifecycle
  methods). Two more were pure heuristic noise: a test-file panic-message
  string containing the word "unsafe" (`gpu_instance_layout_tests.rs:823`)
  and a source-text `.find("pub unsafe fn …")` call used by a regression
  test to grep the shader-barrier function signature
  (`water.rs:993/1024`, `svgf.rs:1811`) — not executable unsafe code at all.
  `crates/ui/src/navigator.rs` and `crates/facegen/src/lib.rs`, both counted
  as having "1 unsafe occurrence" by a naive `grep -ro unsafe`, in fact
  contain **zero** real `unsafe` blocks — the matches are a log-message
  string ("...unsafe Scaleform archive path...") and a doc comment
  ("No `unsafe`. No external deps…") respectively. Net: unsafe-block
  discipline across the sampled/scanned population is excellent, not a
  "haystack" — no batched MEDIUM finding is warranted this pass. (This
  reinforces the repo's own Audit-Finding-Hygiene lesson: verify the
  premise against current code before filing.)
- **Dimension 1 — cxx-bridge still a placeholder.** `crates/cxx-bridge/src/lib.rs`
  is unchanged: one `unsafe extern "C++"` fn, `native_hello() -> String`,
  no raw pointers, no borrowed slices, no `Box<>` handoff. `cpp/native_utils.cpp`
  is a 5-line greeting return. No live-CRITICAL-class FFI surface exists here.
- **Dimension 1 — fsr3-sys `# Safety` contracts.** `Context::create` (`lib.rs:342`),
  `Context::dispatch` (`lib.rs:371`), and `Drop for Context` (`lib.rs:444-452`)
  all carry `# Safety` doc sections stating the device/physical-device/proc-addr
  outlive-the-context and same-device-handle contracts. The sole production
  call site (`frame_upscaler.rs`) satisfies both: `Context::create` is called
  with handles from the live renderer device inside `VulkanContext::new`, and
  `Drop for Context` only ever fires from inside `VulkanContext::drop`, which
  calls `device_wait_idle()` unconditionally as its first statement — the
  precondition Drop's own SAFETY comment leans on.
- **Dimension 2 — ECS cached-pointer contract (#35/#1367).** `World::get`
  returns `ComponentRef<'_, T>` (guard-carrying), not a raw pointer. Field
  order + lifetime elision make a guard-outlives-pointer violation
  impossible without an explicit unsafe escape (see SAFE-2026-08-03-01 for
  the residual Miri-coverage gap on this point).
- **Dimension 2 — `#[repr(C)]` GPU-struct vec3 soundness.** Zero
  `[f32; 3]` fields in `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`
  — every vec3-shaped quantity is either three scalar `f32`s or padded to
  `[f32; 4]`.
- **Dimension 2 — NIF bulk POD reads.** `read_pod_vec`
  (`crates/nif/src/stream.rs:340-372`) gates `T` on a crate-local sealed
  `AnyBitPattern` trait with an explicit, closed type list (`bool` cannot be
  added without a new `unsafe impl`), and overflow-checks `count × size_of::<T>()`
  before allocating. See SAFE-2026-08-03-02 for the header-mirror's narrower
  (but currently correct) variant.
- **Dimension 2 — sfmaterial `BuiltinType::from_u32`.** Still a fully
  checked `match` with an `Err` arm (`crates/sfmaterial/src/types.rs:37-57`)
  — no transmute, despite the module doc's aspirational "transmute into this
  enum" wording.
- **Dimension 2 — pex `OpCode::from_u8` transmute.** Both preconditions hold:
  `#[repr(u8)]` with contiguous discriminants `0..MAX_OPCODE=51`
  (`crates/pex/src/opcode.rs:9-61`), and check-then-transmute ordering
  (early-return before the `unsafe` block). Independently pinned by
  `from_u8_round_trips_and_rejects_oob`.
- **Dimension 2 — NIF traversal recursion depth.** `walk_node_hierarchical`/
  `walk_node_flat` (`crates/nif/src/import/walk/mod.rs`) cap at
  `MAX_NIF_NODE_DEPTH = 128`; `crates/nif/src/import/collision/shape.rs`
  caps at `MAX_COLLISION_SHAPE_DEPTH = 64` with balanced cycle-detection via
  a `visited` set.
- **Dimension 3 — Rapier release on cell unload (#1520).** `unload.rs:359-417`
  still walks `RapierHandles` + `Ragdoll` components on victim entities and
  calls `PhysicsWorld::remove_body` / `remove_ragdoll` for each.
- **Dimension 3 — deferred-destroy tick/drain ordering.** Confirmed the tick
  (`context/draw.rs:1369-1401`) runs after `wait_for_fences`
  (`:1268-1283`), and the shutdown drains (`acceleration/blas_static.rs:116-141`,
  `mesh.rs:1391`) are unconditional and gated on a caller-side
  `device_wait_idle` per their `# Safety` docs — not called from any per-frame
  path.
- **Dimension 3 — AllocatorResource drop ordering (#1406), including the
  panic-unwind path.** `impl Drop for App` (`byroredux/src/main.rs`) removes
  `AllocatorResource` from the ECS `World` before `self.renderer.take()` in
  its explicit `drop()` body — and since this is a real `Drop` impl, it runs
  on *every* unwind path (panic or normal), not just the
  `WindowEvent::CloseRequested` handler (which redundantly does the same
  removal for the ordinary-exit case, explicitly documented as idempotent).
- **Dimension 5 — TLAS resize wait (#1390).** `acceleration/tlas.rs:725`
  still calls `device.device_wait_idle()` in the resize branch before the
  old instance-buffer allocation is retired.
- **Dimension 5 — `VOLUMETRIC_OUTPUT_CONSUMED` gate.** `volumetrics.rs:421`
  is `true`; both `context/post_passes.rs:418` and the (now-dead, per its own
  comment) `context/draw.rs:588` branch read the const rather than assuming
  a value.
- **Dimension 5 — VulkanContext Drop ordering.** `context/mod.rs`'s
  `impl Drop`: `device_wait_idle()` first, then `egui_pass`/`presentation`
  destroy, then allocator-independent device-level teardown (query pools,
  compute pipelines — explicitly hoisted per #1483 so they run on every Drop
  path including allocator-`None`), then allocator-dependent teardown,
  then the allocator itself is dropped (with an `Arc` outstanding-reference
  debug-assert), and `destroy_device` is the final call.
- **Dimension 7 — glass/IOR guards.** `GLASS_RAY_BUDGET = 2_097_152`
  matches between `shader_constants_data.rs:171` and the generated
  `shader_constants.glsl:48`; enforced at `triangle.frag:1528`
  (`glassIORAllowed = (old + GLASS_RAY_COST <= GLASS_RAY_BUDGET)`), with the
  known #1438 atomicAdd-overshoot nuance unchanged. Frisvad orthonormal
  basis is the active construction (`math_common.glsl:102-121`, referenced
  at `triangle.frag:1610`). All nine `DBG_VIZ_*` constants
  (`shader_constants_data.rs:341-569`) have distinct bit values —
  `DBG_VIZ_GLASS_PASSTHRU = 0x80` has no collision.
- **Dimension 8 — NPC/animation spawn safety.** `FLT_MAX`-sentinel gating is
  present throughout `crates/nif/src/anim/bspline.rs` (translation, rotation,
  scale). `AnimationClipRegistry` interns via `.to_ascii_lowercase()`.
  `SkinSlotPool`'s one-shot `overflow_warned` + `overflow_attempt_count`
  overflow path is intact (`skin_slot_pool.rs:77-160`).
- **Dimension 9 — NIFAL NaN boundary.** `material_translate.rs:174-175`
  seeds `f32::NAN` via `unwrap_or(f32::NAN)` for both metalness and
  roughness; `Material::resolve_pbr` (`material_translate.rs:182`) is called
  immediately after construction and is the sole `is_nan()`-detecting
  producer path (only 1 production call site of `resolve_pbr()` in the
  entire workspace, plus test call sites). No other `Material {` construction
  site exists outside `material_translate.rs` and a test file.
- **Dimension 10 — debug-ui teardown.** `DebugUiState` (`crates/debug-ui/src/lib.rs`)
  holds no Vulkan handle. `EguiPass` is destroyed first thing inside
  `VulkanContext`'s `Drop`, immediately after `device_wait_idle()` and before
  any allocator-dependent or device-destroy step. `pending_free` texture IDs
  are deferred and drained through the same per-frame path that leans on the
  fence-wait discipline already verified in Dimension 3.

---

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 4 |
| **Total** | **4** |

All four findings are maintainability/coverage gaps (missing Miri job,
a caller-contract-only bounds guard, a stale field-count in a comment, and
skill-doc drift), not live defects. Every regression guard this dimension
set exists to protect — FFI lifetime contracts, the ECS cached-pointer
contract, GPU struct layout pins, the three drop-ordering invariants,
TLAS/BLAS build discipline, RT glass/IOR guards, NPC-spawn safety nets, the
NIFAL NaN boundary, and debug-ui teardown ordering — was re-verified against
the current source this pass and found intact.

Next: `/audit-publish docs/audits/AUDIT_SAFETY_2026-08-03.md`
