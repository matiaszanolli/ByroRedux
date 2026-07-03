# Renderer Audit — 2026-07-03

Deep audit of the Vulkan deferred + ray-traced renderer across all 21 skill
dimensions (AS correctness, SSBO/RT ray-query plumbing, GPU-struct layout,
sync/barriers, GPU memory/lifecycle, NIFAL material translation, material table,
denoiser/composite, GPU skinning, camera-relative precision, pipeline/render
pass, command-buffer recording, TAA, caustics, water, volumetrics/bloom, Disney
BSDF/soft shadows, sky/weather, tangent-space, debug/telemetry, Cornell harness).

- **Branch**: main · **HEAD**: `8498e55921bea7d43784442b2a3bae43df124962`
- **Depth**: deep — delta-focused. The prior full 21-dimension deep sweep
  (`AUDIT_RENDERER_2026-07-01.md`) ran against `1b4e8e84`. This session audits
  the **renderer code delta** since that sweep (929 insertions across 19 files),
  then re-confirms the two carried-forward LOW findings against the live tree.
- **Authoritative references**: `docs/engine/shader-pipeline.md`,
  `docs/engine/memory-budget.md`, `docs/engine/nifal.md`.
- **Dedup baseline**: `gh issue list` (open issues incl. #1860/#1861) + prior
  reports `AUDIT_RENDERER_2026-07-01.md` / `AUDIT_RENDERER_2026-07-02.md`.
- **Test baseline**: `cargo test -p byroredux-renderer --lib` → **346 passed,
  0 failed** (fresh run this session; up from 337 on 07-01/07-02 — the +9 delta
  is the new regression tests landed with the fixes audited below, not a
  layout-pin change).

## Methodology Note

Unlike the 07-02 confirmation pass (which audited an unchanged tree), the tree
HAS moved since the last deep sweep. `git diff --stat 1b4e8e84 HEAD` over
`crates/renderer/`, `byroredux/src/render/`, and the shader sources shows **19
files / 929 insertions / 68 deletions** — a burst of just-landed fixes:
#1782 (deferred BLAS-scratch destroy), #1790 (skinned-BLAS scratch barrier
AS_READ bit), #1791 (requeue drained bind_inverses on early return), #1792
(mid-batch BLAS eviction `pending_bytes`), #1793/#1797 (documented-not-fixed
gaps annotated at their sites), #1794 (bone_world per-frame fill elimination),
#1795 (particle color-fade quantization), #1796 (pose-hash rollback on early
return), #1799 (compile-time gate for the legacy WRS reservoir arrays), #1803
(dead GlobalTransform probe removal), #1804 (two-sided blend split gated on
z_write), #1823 (FO4 BGSM/BGEM blend-factor swap revert).

Each was audited by reading the full diff, tracing the invariant it relies on,
and cross-checking the accompanying regression tests. The adversarial focus was
the two highest-risk changes: #1794 (a per-frame GPU-buffer fill elimination
that leaves stale bone-matrix data resident) and #1782 (a use-after-free fix
that adds a new deferred-destroy queue whose shutdown drain must be complete).

## Executive Summary

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 2 | REN-2026-07-03-L01 (Existing: #1860), REN-2026-07-03-L02 (Existing: #1861) |

The renderer remains in **excellent** condition. Every fix landed since the
07-01 deep sweep was verified correct and behavior-preserving where claimed;
no fix introduced a regression, and all accompanying regression tests are green
(346/346). The only two open findings are the pre-existing LOW items already
filed as GitHub issues #1860 and #1861 — both re-confirmed live, both bounded
and non-blocking. **No new findings.**

## RT Pipeline Assessment

**BLAS/TLAS (Dim 1)** — clean, and materially hardened since 07-01:

- **#1782 deferred BLAS-scratch destroy** — verified complete. The retired
  `blas_scratch_buffer` at the two `about_to_wait`-reachable grow sites
  (`build_blas`, `build_blas_batched` in `blas_static.rs`) and both
  `shrink_blas_scratch_to_fit` sites (`memory.rs`) now route through the new
  `pending_destroy_scratch: DeferredDestroyQueue<GpuBuffer>` instead of an
  immediate `destroy`. The queue is declared in `acceleration/mod.rs`,
  initialized in `new()`, ticked in `tick_deferred_destroy`, drained in
  `drain_pending_destroys`, and — critically — the shutdown path
  `AccelerationManager::destroy()` calls `drain_pending_destroys` (mod.rs:279)
  **before** destroying the live `blas_scratch_buffer` (mod.rs:326). No leak, no
  UAF. The `build_skinned_blas_batched_on_cmd` grow site in `blas_skinned.rs`
  correctly stays immediate (runs after that frame's own fence wait) and is
  documented as a deliberate non-sibling.
- **#1790 scratch-serialize barrier** — spec-correct. `record_scratch_serialize_barrier`
  now carries `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR`
  in the dst mask (was WRITE-only). An UPDATE-mode refit reads
  `srcAccelerationStructure`; on a first-sight frame the same command buffer
  records the BUILD immediately before the refit with only this barrier between
  — WRITE-only was an unmade-visible RAW. Fix matches VUID expectations.
- **#1792 mid-batch eviction** — the new `blas_over_budget(static, pending,
  budget)` predicate threads `pending_bytes` into `evict_unused_blas`; all four
  call sites updated (`0` for no-batch callers, real accumulated bytes for the
  mid-batch caller). On a fresh cell load (`static_blas_bytes == 0`) the callee
  is no longer structurally blind to the batch's own in-flight allocations.
  Batch entries aren't yet in `blas_entries`, so eviction can't false-touch
  them. Correct.
- **#1793 / #1797 documented-not-fixed gaps** — the permanently-missing rigid
  BLAS (no per-frame rebuild path) and the single-scratch build serialization
  throughput ceiling are annotated at their exact sites (`tlas.rs`,
  `blas_skinned.rs`) and are the two known gaps the SKILL.md Dim-1 checklist
  explicitly says to recast, not re-report. Both gated behind budget pressure
  unreachable on the 12 GB dev card. Not re-reported.

**SSBO indexing & ray queries (Dim 2)** — clean. The `#1799` compile-time gate
around the legacy 16-slot WRS reservoir arrays in `triangle.frag` is
structurally sound: paired `#if ENABLE_LEGACY_WRS` / `#endif` around the array
declarations, the streaming-write `else` arm, and the entire pass-2 shadow-ray
`else` block; `useRestir` collapses to `rtEnabled` when disabled;
`RESERVOIR_W_CLAMP` (shared with the ReSTIR finalize path) correctly stays
outside the gate. `triangle.frag.spv` shrank 185008→177840 B, consistent with
the dead arm being preprocessed out. The `ENABLE_LEGACY_WRS = 0` default is
test-pinned (`legacy_wrs_arm_defaults_to_disabled`) and the gate placement is
test-pinned (`triangle_frag_legacy_wrs_arrays_are_compile_time_gated`). The
ReSTIR-DI spatial normal-cone guard and BC1 punch-through guard are untouched.

## GPU-Struct & Memory Assessment

**Bone-world per-frame fill elimination (#1794, Dim 9)** — the highest-risk
change this cycle; verified correct. `build_render_data` no longer
`bone_world.clear()`s each frame; it re-seeds only slot-0 element 0 to identity
and lets `build_skinned_palettes`'s Pass-2 `resize` grow-or-shrink the buffer in
place. The load-bearing invariant — *a vertex's bone-weight index is bounded by
its own mesh's bone count at import time, so it can never read a slot's padding
tail (beyond the occupant's bone count) or a reused/unallocated slot's stale
content* — holds: Pass 3 rewrites the full `0..skin.bones.len()` used range of
every allocated slot every frame (fresh even across slot reuse into a larger
bone count), bind_inverse for the used range is re-uploaded on (re)allocation,
and slot 0's `[1..MBPM)` tail stays identity from the first resize (slot 0 is
never a Pass-3 target and `max_used_slot()` keeps the buffer ≥ MBPM). Three new
regression tests pin steady-state overwrite without a prior clear, tail-sentinel
survival across frames, and grow-then-shrink resize. The remaining fixed-stride
staging memcpy + GPU copy in `upload_bone_worlds` is documented as deferred
follow-up (needs per-slot bone-count plumbing across the crate boundary), not a
correctness defect.

**Pose-hash rollback on early return (#1796)** — correct and fully wired. The
new `skin_dispatch_ran` flag on `VulkanContext` is reset `false` before both
`draw_frame` early-return guards (empty framebuffers, `ERROR_OUT_OF_DATE_KHR`)
and flipped `true` only when `record_skinned_blas_refit` runs. The consumer in
`main.rs:1863` calls `SkinSlotPool::rollback_pending_pose_commits()` (and #1791's
`requeue_pending`) when the flag reads `false`, undoing the premature
`try_mark_pose_dirty` commit that `build_render_data` performed before
`draw_frame` was called. Ordering is source-pinned by
`skin_dispatch_ran_is_reset_before_both_early_return_guards`. Both callee
functions exist in `crates/core/src/ecs/resources.rs`.

**Layout pins / sync / lifecycle** — no `#[repr(C)]` GPU-struct delta this
cycle; the 112/336/300 B pins and reflection tests are green. Two-sided blend
split now gated on `z_write` (#1804) via `needs_two_sided_blend_split`, removing
a dead FRONT-cull pass on particle batches without touching the depth-writing
glass case; four unit tests pin the truth table. Particle color-fade
quantization (#1795) restores MaterialTable dedup by snapping only the color
LERP to 32 steps (size LERP stays continuous). All verified.

## Findings

### LOW

#### REN-2026-07-03-L01: `DBG_BITS` test catalog still covers only 13 of 17 `DBG_*` constants
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/shader_constants.rs` :: `DBG_BITS`; constants in `crates/renderer/src/shader_constants_data.rs`; hand-written emits in `crates/renderer/build.rs`
- **Status**: Existing: #1860 (open) — re-confirmed live this session
- **Description**: `shader_constants_data.rs` declares **17** `pub const DBG_*`
  constants (re-verified `grep -c "^pub const DBG_"` → 17), but the `DBG_BITS`
  catalog array — the shared iteration source for both the header value-pin test
  and the shader no-redeclare guard — still enumerates only **13** entries
  (`DBG_BYPASS_POM` 0x1 … `DBG_LEGACY_LIGHT_ATTEN` 0x1000). The four newest bits
  (`DBG_DISABLE_MULTISCATTER` 0x2000, `DBG_DISABLE_ATROUS` 0x4000,
  `DBG_DISABLE_RESTIR` 0x8000, `DBG_DISABLE_SPATIAL` 0x10000) are emitted into
  the generated GLSL header via separate hand-written `writeln!` calls in
  `build.rs` and carry neither a value-pin nor a no-redeclare guard. The in-code
  doc comment above the catalog still asserts "All 13 DBG_* bits", now stale.
- **Evidence**: `shader_constants.rs` `DBG_BITS` body = 13 entries (read this
  session, unchanged from the 07-01/07-02 citation); `build.rs` hand-writes the
  four newer `#define`s plus the new `ENABLE_LEGACY_WRS` define this cycle.
- **Impact**: Latent, not live. No shader currently shadow-redeclares the four
  uncovered bits and the generated header values are correct today; the risk is
  a future shader/`build.rs` edit on those four bits shipping undetected past
  `cargo test`.
- **Related**: #1482 (original catalog fix); #1860 (open issue).
- **Suggested Fix**: Add the four missing entries to `DBG_BITS`, route their
  header emit through the catalog loop, and add a
  `dbg_bits_catalog_covers_every_dbg_constant` count-parity test so the parallel
  list cannot drift again. (Note: this same file gained the `ENABLE_LEGACY_WRS`
  pins this cycle — the catalog itself was not extended.)

#### REN-2026-07-03-L02: `with_one_time_commands_inner` leaks fence/command-buffer on the post-recording error paths
- **Severity**: LOW (fires only on an already-failing GPU submit/wait)
- **Dimension**: Sync/Barriers (error-path resource lifecycle)
- **Location**: `crates/renderer/src/vulkan/texture.rs` :: `with_one_time_commands_inner`
- **Status**: Existing: #1861 (open) — carried, unchanged (file untouched this cycle)
- **Description**: Three fallible calls (`reset_fences?`, `queue_submit?`,
  `wait_for_fences?`) propagate via `?` before the fence-destroy /
  `free_command_buffers` cleanup tail runs. On error the owned fence (if
  `owned == true`) and/or the one-time command buffer are abandoned. The
  `wait_for_fences` case is the most likely to fire (device-loss mid-wait) and
  fires after a successful submit, so the GPU may still be mid-execution against
  the abandoned `cmd`.
- **Evidence**: `git diff --stat 1b4e8e84 HEAD -- crates/renderer/src/vulkan/texture.rs`
  is empty — the function is byte-identical to the 07-01/07-02 citation.
- **Impact**: Bounded. Only fires when a one-time submit is already failing
  (device-loss / OOM territory); the reusable-fence path (`owned == false`,
  common case post-init) leaks no fence, only a command buffer reclaimed at pool
  destruction. No per-frame accumulation.
- **Related**: #1861 (open issue), #302 (reusable fence), #1713 (adjacent, race-free).
- **Suggested Fix**: Capture the three `Result`s, run the destroy/free cleanup
  unconditionally in all error arms (or wrap `cmd` + owned fence in an RAII drop
  guard), then propagate. Verify no double-destroy of the reusable fence on the
  `owned == false` path.

## Prioritized Fix Order

1. **REN-2026-07-03-L01 / #1860** (LOW, test) — extend `DBG_BITS` to all 17
   constants + add the count-parity test. One-file test edit.
2. **REN-2026-07-03-L02 / #1861** (LOW, error path) — cleanup-before-propagate in
   `with_one_time_commands_inner`.

Neither is urgent; both are pre-existing, bounded, non-blocking, and already
filed.

## Needs-RenderDoc

No new sync/barrier finding requires capture-based verification. Per standing
guidance, no speculative Vulkan change is proposed. The #1790 barrier fix was
verified against the Vulkan spec (UPDATE-mode refit reads `srcAccelerationStructure`)
and its author cites live validation-layer confirmation; the #1782 deferred
scratch destroy is a lifecycle change with no barrier semantics. The two
completeness caveats carried from 07-01 (the #1748 `draw_frame`-extraction
live-capture confirmation, and `water.frag`'s absent shader-side `sceneFlags.x`
early-out) remain owed but are not suspected defects.

## Disproved / Not Reported

- **#1794 stale bone-matrix corruption** — investigated as the top regression
  candidate; disproved. The used range of every allocated slot is rewritten
  fresh every frame (Pass 3, full `0..bones.len()`), bind_inverse is
  re-uploaded on (re)allocation, and the vertex-index-bounded-by-own-bone-count
  invariant means the never-refilled tail/reused slots are structurally
  unreadable. Three regression tests pin this.
- **#1782 scratch-queue leak on shutdown** — disproved. `destroy()` →
  `drain_pending_destroys` drains `pending_destroy_scratch` before the live
  buffer is freed.
- **#1799 WRS compile-gate breakage** — disproved. The `.spv` compiled at
  `ENABLE_LEGACY_WRS = 0` (smaller binary), reflection tests green, gate
  placement source-pinned.
- Cornell metalness-vs-lighting confound and glass-stipple / IGN refraction
  jitter — known open observations per project memory; not re-reported.
