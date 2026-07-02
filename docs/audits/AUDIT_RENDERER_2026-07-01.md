# Renderer Audit — 2026-07-01

Deep audit of the Vulkan deferred + ray-traced renderer across all 21 skill
dimensions (AS correctness, SSBO/RT ray-query plumbing, GPU-struct layout,
sync/barriers, GPU memory/lifecycle, NIFAL material translation, material table,
denoiser/composite, GPU skinning, camera-relative precision, pipeline/render
pass, command-buffer recording, TAA, caustics, water, volumetrics/bloom, Disney
BSDF/soft shadows, sky/weather, tangent-space, debug/telemetry, Cornell harness).

- **Branch**: main · **HEAD**: `1b4e8e84`
- **Depth**: deep — orchestrator + 8 dimension-agent groups (renderer-specialist),
  symbol-anchored verification, adversarial per-finding disproof, orchestrator
  re-verification of every actionable finding against the live tree.
- **Authoritative references**: `docs/engine/shader-pipeline.md`,
  `docs/engine/memory-budget.md`, `docs/engine/nifal.md`.
- **Dedup baseline**: `gh issue list` (21 open) + prior reports, notably the
  clean comprehensive sweep `AUDIT_RENDERER_2026-06-26.md` and the clean focused
  passes `AUDIT_RENDERER_2026-06-28_DIM3_DIM6.md` /
  `AUDIT_RENDERER_2026-06-28_DIM12_DIM14.md` (baseline commit `f9cc691b`).
- **Test baseline**: `cargo test -p byroredux-renderer --lib` → **337 passed,
  0 failed** (fresh run this session; up from 335 on 06-26 — the delta is
  `queue_guard_released_before_one_time_fence_wait` (#1713) and related
  additions, not a layout-pin change).

## Executive Summary

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 2 | REN-2026-07-01-L01, REN-2026-07-01-L02 |
| INFO | 3 | REN-2026-07-01-I01, -I02, -I03 |

The renderer remains in **excellent** condition. The audit's primary risk
surface was the burst of code-motion refactors landed since the 2026-06-26/06-28
clean baselines — `9b294048`/`b03fd78d` (#1748 `draw_frame` →
`record_geometry_pass` / `record_skinned_blas_refit` / `record_post_passes`),
`0409b6d6` (#1670 `App::new` + #1671 `recreate_swapchain` 3-phase splits),
`26439046` (#1749 `build_core_device` extraction), `806ba7af` (#1713 queue-mutex
re-scope), `69a19c55` (#1751 compute-pipeline helper), `71ba4a04` (#1752
descriptor-write helpers), `c75f3991` (#1759 flush-alignment guard),
`fd483a2f` (#1758 skin workgroup constant), `9009dc66` (#1780 lockstep test).
**Every one of these was verified behavior-preserving by diff**: no barrier
moved across a helper boundary, no begin/end unbalanced, no lifecycle ordering
disturbed, no pipeline/descriptor parameter drifted, no GPU-timer bracket split.

The two LOW findings are a **test-coverage hole** (the `DBG_BITS` value-pin
catalog drifted to a 13-of-17 subset — the exact failure mode #1482 was created
to prevent) and a **bounded error-path resource leak** in the one-time-command
helper (pre-existing; fires only on an already-failing submit). Neither affects
a rendered pixel today.

Notable resolution confirmed: the prior report's only open finding,
**D14-LOW-01** (constants-header lockstep test missing `caustic_splat.comp` +
`water.frag`), is **RESOLVED** by `9009dc66` (#1780) — the allow-list now
enumerates all 16 header-consuming shaders.

## RT Pipeline Assessment

**BLAS/TLAS (Dim 1)** — clean. Geometry description (`R32G32B32_SFLOAT` @ 0,
`UINT32`, `OPAQUE`) identical across all four build/refit sites; the three
build-flag constants match `docs/engine/memory-budget.md` and remain test-pinned
(including the deliberate `SKINNED_BLAS_FLAGS = FAST_BUILD | ALLOW_UPDATE`).
The load-bearing AS/SSBO contract holds: `instance_custom_index == ssbo_idx`
(`Packed24_8::new(ssbo_idx, 0xFF)`), guarded by the per-call `debug_assert!`
and the `MAX_INSTANCES < 1<<24` const-assert. `decide_use_update` keys on
`last_blas_addresses` with the count-mismatch demote (VUID-03708); refit
validates both VUID-03667 halves (`validate_refit_flags` /
`validate_refit_counts`). Deferred BLAS destruction routes every drop/evict
through `pending_destroy_blas`; no immediate `destroy_acceleration_structure`
at any eviction site.

**SSBO indexing & ray queries (Dim 2)** — clean. RT hit path reads
`rayQueryGetIntersectionInstanceCustomIndexEXT` → `instances[]` →
`materials[materialId]`, round-tripping with the Dim-1 CPU index. Shadow /
reflection / GI ray geometry (bias, tMin, TerminateOnFirstHit, Frisvad basis,
interior-ambient miss fill) all verified; `GLASS_RAY_BUDGET` overshoot-by-design
doc intact with no CPU read; ReSTIR-DI spatial normal-cone guard
(`SPATIAL_NORMAL_COS = 0.906`, geometric normal, packed `pad0`) intact; BC1
punch-through guard (#ae285062) intact end-to-end. `.spv`-vs-source staleness
checked: the only includes newer than `triangle.frag.spv` are comment/#define-only
edits that don't change its compiled output.

**Denoiser stability (Dims 8/13)** — clean. SVGF ping-pong, motion convention
(`prevUV = uv − motion`), bit-31-masked mesh-ID disocclusion, firefly clamp
ahead of the `hasHistory` branch (REG-07), first-frame bootstrap gate all hold.
TAA Halton(2,3) jitter advances per frame with un-jittered motion
reconstruction, YCoCg 3×3 clamp, parked-camera α floor (#1497), and a fully
skipped disable path.

**Composite reassembly (Dim 8)** — correct order: `direct + indirect×albedo +
caustic` (both accumulators float-promoted before the add, #1575), bloom added
pre-ACES, ACES after reassembly, display-space fog on direct only, SSAO
modulating indirect/ambient only.

**Precision (Dim 10)** — the two coordinate conventions are never mixed.
Adversarial greps for a derivative consumer on the absolute varying and for a
second absolute world-space varying both came back empty; the soft-particle
rebase (#1642), volumetrics/SSAO origin handling, and
`RT_ABSOLUTE_PRECISION_CEILING = 2^20` debug-assert all hold.

## GPU-Struct & Memory Assessment

**Layout pins (Dim 3)** — locked, with one guard-coverage hole (L01, below).
`GpuInstance` 112 B / `GpuCamera` 336 B / `GpuMaterial` 300 B pins green;
exhaustive per-field offset pin (#806) + GLSL field-order pin (#1657) green;
`GpuInstance` declared at exactly 5 shader sites, byte-identical;
`GpuMaterial` scalar-only with the byte-hash dedup invariant intact; capacities
match `docs/engine/memory-budget.md`; over-cap intern → id 0 + warn-once.

**Sync (Dim 4)** — clean. Per-swapchain-image `render_finished` (548c1b69),
both-slot fence waits, `images_in_flight` tracking, AS-build INPUT barriers
using `SHADER_READ` at the build stage (#507945d8/#1436), skin-compute →
AS-build → fragment chain preserved verbatim through the #1748 extraction,
egui EXTERNAL dependencies (#1433), and the #1671 resize split all verified.
The #1713 queue-mutex re-scope is **race-free**: the reusable fence stays under
its own Mutex across the whole submit→wait cycle; the queue lock covers only
the submit call (exactly what VUID-vkQueueSubmit-queue-00893 requires).

**Memory/lifecycle (Dim 5)** — zero-defect. `AllocatorResource` removed from
the `World` before `renderer.take()` in `impl Drop for App` (structural on
panic-unwind; survived the #1670 split — the resource is inserted in
`resumed()`, not a constructor phase). `VulkanContext` Drop: allocator-independent
block hoisted (#1483), reverse-order teardown, allocator via `Arc::try_unwrap`
before `destroy_device`. TLAS resize waits idle before freeing (#1390).
Deferred-destroy countdown = `MAX_FRAMES_IN_FLIGHT`, ticked post-fence-wait.
Pool caps, shrink slacks (16 MB / 256 KB / 1 MB), LRU caps and half-eviction
(#1430) all match `docs/engine/memory-budget.md`.

**Material translation & table (Dims 6/7)** — clean. `translate_material` still
has exactly two production callers; `resolve_pbr` resolve-once + idempotent; the
renderer never reads `EmissiveSource`; no per-game branch between `Material` and
`MaterialTable::intern`; byte-hash dedup with `DrawCommand::material_hash`
mirror pin; over-cap and telemetry guards intact. The one in-range import-side
commit (`61b8cf7f`, #1333 particle transform) does not touch the boundary.

**Per-feature passes (Dims 9, 11–16, 18–21)** — all clean; every regression
guard re-verified by symbol (see the per-dimension coverage records retained in
the audit working notes). Highlights: skin dispatch math and shader
`local_size` now share the single generated `SKIN_WORKGROUP_SIZE` constant
(#1758, byte-identical `.spv`); GPU-timer bracket pairing (12 start/end pairs)
survived the draw_frame extractions with no bracket straddling a helper
boundary; the Cornell harness still routes `mat.*` edits through the production
`MaterialTable::intern` path.

## Findings

### LOW

#### REN-2026-07-01-L01: `DBG_BITS` test catalog covers only 13 of 17 `DBG_*` constants — the 4 newest debug bits have no value-pin or no-redeclare guard
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/shader_constants.rs` :: `DBG_BITS` (drives `generated_header_contains_all_defines` + `triangle_frag_dbg_bits_not_redeclared`); constants in `crates/renderer/src/shader_constants_data.rs`; hand-written emits in `crates/renderer/build.rs`
- **Status**: NEW (latent drift against the intent of closed #1482)
- **Description**: `shader_constants_data.rs` declares **17** `DBG_*` constants,
  but the `DBG_BITS` catalog — the iteration source for both the header
  value-pin and the shader no-redeclare guard — enumerates only **13**
  (`DBG_BYPASS_POM` 0x1 … `DBG_LEGACY_LIGHT_ATTEN` 0x1000). The four newest
  bits (`DBG_DISABLE_MULTISCATTER` 0x2000, `DBG_DISABLE_ATROUS` 0x4000,
  `DBG_DISABLE_RESTIR` 0x8000, `DBG_DISABLE_SPATIAL` 0x10000) are emitted into
  the generated header by separate hand-written `writeln!` calls in `build.rs`
  and are consumed by production shaders (`triangle.frag`,
  `include/lighting.glsl`, `svgf_atrous.comp`) — yet get neither guard. The
  in-test comment still claims the catalog pins "all 13 … so this value-pin can
  never again cover a subset (#1482)"; the subset it was written to prevent has
  recurred because the catalog is a manually-maintained parallel list.
- **Evidence**: `grep -c "^pub const DBG_" shader_constants_data.rs` = 17;
  `DBG_BITS` array body = 13 entries; `build.rs` lines ~332–339 hand-write the
  four newer `#define`s; generated `crates/renderer/shaders/include/shader_constants.glsl`
  carries correct values (8192u/16384u/32768u/65536u) today. Orchestrator-verified.
- **Impact**: Latent, not live — no shader currently shadow-redeclares the four
  bits and the header values are correct, so no pixel is wrong today. The risk
  is a future shader adding `const uint DBG_DISABLE_RESTIR = …;` (or a
  `build.rs` typo on those four) passing `cargo test` undetected — a debug-viz
  bit silently controlling the wrong RT/denoise feature.
- **Related**: #1482 (original catalog fix); REN-2026-07-01-I03 (stale "13-bit"
  wording in checklists).
- **Suggested Fix**: Add the four missing entries to `DBG_BITS` and route their
  header emit through the catalog loop instead of hand-written `writeln!`s;
  additionally add a `dbg_bits_catalog_covers_every_dbg_constant` test that
  counts `^pub const DBG_` declarations in `shader_constants_data.rs` and
  asserts equality with `DBG_BITS.len()` so the parallel list cannot drift
  again.

#### REN-2026-07-01-L02: `with_one_time_commands_inner` leaks owned fence + one-time command buffer on `queue_submit` / `wait_for_fences` error
- **Severity**: LOW (pre-existing; fires only on an already-failing GPU path)
- **Dimension**: Sync/Barriers (error-path resource lifecycle)
- **Location**: `crates/renderer/src/vulkan/texture.rs` :: `with_one_time_commands_inner`
- **Status**: NEW (pre-existing — confirmed NOT introduced by #1713 / `806ba7af` by diffing the parent commit)
- **Description**: When `queue_submit(...)?` or `wait_for_fences(...)?` returns
  `Err`, the `?` propagates out of the `unsafe` block before the cleanup tail
  runs: `if owned { device.destroy_fence(fence, None) }` and
  `device.free_command_buffers(pool, &[cmd])` are skipped. On the per-call
  fence path (`owned == true`, early-init callers without a persistent
  `transfer_fence`) a `VkFence` leaks; on both paths the one-time
  `VkCommandBuffer` leaks until its pool is destroyed at shutdown. The earlier
  `reset_fences(...)?` error arm has the same shape for the command buffer.
- **Evidence**: Orchestrator-read of the live function: `queue_submit` inside
  the scoped queue-lock block with `?`, `wait_for_fences` with `?`, and the
  fence-destroy / `free_command_buffers` tail strictly after both. The
  `fence_guard` MutexGuard itself is RAII-dropped, so the reusable fence is not
  poisoned/stuck — only the allocations leak.
- **Impact**: Bounded. The path only fires when a one-time submit is already
  failing (device-loss / OOM territory, typically followed by teardown); the
  common reusable-fence path leaks no fence, and leaked command buffers are
  reclaimed at pool destruction. No accumulation in normal operation — hence
  LOW, not the per-frame-leak HIGH floor.
- **Related**: #1713 (adjacent, verified race-free), #302 (reusable fence).
- **Suggested Fix**: Capture the submit/wait `Result`s, run the
  destroy/free cleanup unconditionally, then propagate (or use a small drop
  guard for `cmd` + owned `fence`). Pure error-path cleanup — not a
  barrier/sync semantic change. Verify against the `fence_guard` reuse contract
  (no double-destroy of the reusable fence) before landing.

### INFO

#### REN-2026-07-01-I01: #1759 `NON_COHERENT_ATOM_SIZE` guard is debug-only; release aligns to 256 unconditionally (documented design decision)
- **Severity**: INFO
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/device.rs` :: the `#[cfg(debug_assertions)]` atom-size check; `crates/renderer/src/vulkan/buffer.rs` :: `aligned_flush_range` + `NON_COHERENT_ATOM_SIZE = 256`
- **Status**: NEW (verification record for `c75f3991`)
- **Description**: `aligned_flush_range` rounds non-coherent host flushes to a
  hardcoded 256; the #1759 guard pinning that assumption
  (`debug_assert!(atom <= NON_COHERENT_ATOM_SIZE)`) only runs in debug builds.
  A hypothetical device reporting `nonCoherentAtomSize > 256` in a release
  build would under-align flushes (VUID-VkMappedMemoryRange-size-01390). No
  known GPU reports > 256 (typically 64), the spec requires a power of two,
  coherent allocations skip the flush entirely, and debug/CI would catch it
  first — a defensible, documented decision. Logged so future audits don't
  re-flag it and the release-mode assumption is on record.
- **Suggested Fix**: None required. Optional defense-in-depth: plumb the
  device-reported atom size through `DeviceCapabilities` and round to
  `max(reported, 1)`.

#### REN-2026-07-01-I02: Dim-17 checklist cites #1357 for the flag-bit source-of-truth; the actual enforcing mechanism is #1190
- **Severity**: INFO (audit-skill wording; no code defect)
- **Dimension**: Disney BSDF / audit-skill maintenance
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dim-17 checklist); live code at `crates/renderer/src/shader_constants_data.rs` :: `MAT_FLAG_*`
- **Status**: NEW
- **Description**: The invariant holds — shader-side `MAT_FLAG_*` values are
  pinned equal to the Rust constants by `material_flag_bits_match_material_consts`
  — but the in-code anchor is **#1190** (build.rs emit + pin test), not the
  #1357 migration the checklist cites. A future auditor chasing #1357 would
  find canonicalization history, not the pin mechanism.
- **Suggested Fix**: Cite #1190 in the Dim-17 checklist bullet. No code change.

#### REN-2026-07-01-I03: "13-bit `DBG_*` catalog" wording in the audit-skill checklists is stale — the live catalog spans 17 bits
- **Severity**: INFO (audit-skill wording; no code defect)
- **Dimension**: GPU-Struct Layout / Tangent-Space / audit-skill maintenance
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dim-3 + Dim-19 checklists); live defines in `crates/renderer/shaders/include/shader_constants.glsl`
- **Status**: NEW (sibling of REN-2026-07-01-L01)
- **Description**: Dim-3 ("the 13 `DBG_*` bits (`0x1`…`0x1000`)") and Dim-19
  ("the 13-bit `DBG_*` catalog pinned") both describe the pre-ReSTIR catalog.
  The live set spans 17 contiguous bits, `0x1`…`0x10000`. The count in the
  checklist is what let the L01 test-catalog drift pass three audits unnoticed.
- **Suggested Fix**: Update both checklist bullets to "17 `DBG_*` bits
  (`0x1`…`0x10000`)" once L01 lands (so skill text and test catalog agree). No
  code change.

## Resolved Since Last Report (regression guards, not findings)

- **D14-LOW-01** (2026-06-28: `caustic_splat.comp` / `water.frag` absent from
  the constants-header lockstep test) — **RESOLVED** by `9009dc66` (#1780): the
  allow-list now enumerates all 16 header-consuming shaders, including
  `ssao.comp`, `svgf_atrous.comp`, `svgf_temporal.comp`, `taa.comp`. Dropping
  the `#include` from any of them now fails `cargo test`.
- **#1713 queue-mutex re-scope** — verified race-free (fence-guard held across
  the full submit→wait cycle; queue lock scoped to the submit call only),
  pinned by `queue_guard_released_before_one_time_fence_wait`.
- All previously-resolved guards re-verified to hold: #1575 caustic float
  promotion, REG-07 firefly hoist, #1497 TAA α floor, #1642 soft-particle
  rebase, #786 tangent decode, #1433 egui dependencies, #1390 TLAS resize wait,
  #a476b256 deferred BLAS destroy, #ae285062 BC1 punch-through, #d523b9b3
  ReSTIR normal cone, #1234/#1098 caustic source gate, #928 volumetrics gate,
  #931 bloom barrier removal, #1200 interior fill, #1482 (13 of 17 — see L01).

## Prioritized Fix Order

1. **REN-2026-07-01-L01** (LOW, test) — extend `DBG_BITS` to all 17 constants +
   add the count-parity test. One-file test edit; closes the #1482-class hole
   for the four RT-denoise/ReSTIR debug bits. Do first.
2. **REN-2026-07-01-L02** (LOW, error path) — add cleanup-before-propagate to
   `with_one_time_commands_inner`. Small, isolated, testable via the existing
   one-time-command test seam.
3. **REN-2026-07-01-I02 / -I03** (INFO, skill maintenance) — two checklist
   wording fixes in the audit skill (cite #1190; "17 bits"). Zero-risk doc
   edits.
4. **REN-2026-07-01-I01** — no action required; optional capability plumbing if
   ever desired.

## Needs-RenderDoc

No sync/barrier finding requires capture-based verification, and per standing
guidance no speculative Vulkan change is proposed. Two completeness caveats are
recorded (both pre-documented, neither a suspected defect):

- The #1748 `draw_frame` extractions were verified behavior-preserving by
  static diff + identical recording call counts; the commit itself notes a
  live-frame capture is still owed. If one is run, confirm the
  G-buffer→SHADER_READ and caustic-accum→SHADER_READ barriers sit at the
  `record_geometry_pass` tail / `record_post_passes` head as read.
- `water.frag` has no shader-side `sceneFlags.x` early-out; the draw is
  CPU-gated on `rt_live` (documented in-code at the water dispatch site, #1561)
  with the shader-side gate flagged as a RenderDoc-verified follow-up.

## Disproved / Not Reported

- **Caustic decay-vs-splat "race"** — disproved: decay is a separate
  `decayOnly` dispatch that RMWs each pixel's own slot and returns before the
  splat; the two dispatches are sequenced, never concurrent.
- **Bloom sampling TAA output** — disproved: `hdr_image_views` is populated
  only at creation/resize from the raw HDR attachment; `rebind_hdr_views`
  swaps only the composite descriptor.
- **Skinned-output `VERTEX_BUFFER` usage flag "missing"** — documented Phase-2
  state (raster reads skinned verts via the global vertex SSBO); not a
  regression.
- Cornell metalness-vs-lighting confound and glass-stipple / IGN refraction
  jitter on opaque glass — known open observations per project memory; not
  re-reported.
- Test-count delta 335→337 attributed to `9009dc66` in the working brief —
  bookkeeping correction: `9009dc66` added no test fns (array extension only);
  the delta comes from `806ba7af`'s lock-scope test and siblings.
