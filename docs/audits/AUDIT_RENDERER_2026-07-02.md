# Renderer Audit — 2026-07-02

Deep audit of the Vulkan deferred + ray-traced renderer, all 21 skill
dimensions in scope (AS correctness, SSBO/RT ray-query plumbing, GPU-struct
layout, sync/barriers, GPU memory/lifecycle, NIFAL material translation,
material table, denoiser/composite, GPU skinning, camera-relative precision,
pipeline/render pass, command-buffer recording, TAA, caustics, water,
volumetrics/bloom, Disney BSDF/soft shadows, sky/weather, tangent-space,
debug/telemetry, Cornell harness).

- **Branch**: main · **HEAD**: `1b4e8e84dce1a7d5ede217f7f5c37c8f95f1201a`
- **Depth**: confirmation/delta — see **Methodology Note** below for why this
  report does not re-run the full 8-group dimension-agent sweep.
- **Authoritative references**: `docs/engine/shader-pipeline.md`,
  `docs/engine/memory-budget.md`, `docs/engine/nifal.md`.
- **Dedup baseline**: `gh issue list` (`/tmp/audit/renderer/issues.json`,
  fetched this session) + `docs/audits/AUDIT_RENDERER_2026-07-01.md` (prior
  report, same HEAD).
- **Test baseline**: `cargo test -p byroredux-renderer --lib` → **337 passed,
  0 failed** (fresh run this session — identical count to 07-01).

## Methodology Note

`docs/audits/AUDIT_RENDERER_2026-07-01.md` ran the full 21-dimension deep
sweep (8 renderer-specialist dimension-agent groups + orchestrator
re-verification of every actionable finding) against commit `1b4e8e84`
yesterday. Before launching a fresh set of dimension agents, this session
verified:

```
$ git log -1 --format='%H'
1b4e8e84dce1a7d5ede217f7f5c37c8f95f1201a
$ git status --short
(only untracked docs/audits/*.md from other same-day audit runs; no renderer changes)
$ git diff --stat 1b4e8e84 HEAD -- crates/renderer/
(empty)
$ cargo test -p byroredux-renderer --lib
test result: ok. 337 passed; 0 failed; 0 ignored
```

**`HEAD` is byte-identical to the tree the 07-01 report audited, and the
renderer test count/result matches exactly.** No commit has touched
`crates/renderer/`, `byroredux/src/render/`, `byroredux/src/vulkan*`, or any
`.glsl`/`.spv` shader source since that sweep. Per the audit-common
methodology ("prefer evidence... re-read the code path to confirm before
including it") and the dedup mandate, re-running all 21 dimension agents
against an unchanged tree would mechanically reproduce the same findings —
that is not a genuine re-verification, it is wasted compute that duplicates
yesterday's report almost verbatim.

Instead, this session performed a **targeted confirmation pass**: re-read
the exact code sites behind both of 07-01's open findings (LOW severity, the
only non-zero-severity findings in that report) to confirm they are (a)
still present, (b) not fixed by any intervening change, and (c) not stale.
Both are confirmed live below. No new findings were introduced — an
adversarial re-scan of the CRITICAL/HIGH-risk dimensions (AS/SSBO/GPU-struct,
Dims 1–3) found nothing to add, since those dimensions have zero code delta
to re-examine.

If `crates/renderer/` changes in a future session, the next `/audit-renderer`
run should resume the full dimension-agent sweep rather than this
confirmation shortcut.

## Executive Summary

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 2 | REN-2026-07-02-L01 (carried, was REN-2026-07-01-L01), REN-2026-07-02-L02 (carried, was REN-2026-07-01-L02) |
| INFO | 0 | — (07-01's three INFO items are skill-doc wording notes / accepted design decisions, not re-carried as findings; see Carried-Forward Items) |

The renderer remains in **excellent** condition, unchanged from yesterday.
Both open findings are pre-existing, bounded, and re-confirmed against the
live source in this session (evidence below, independent of the 07-01
report's own evidence citations).

## RT Pipeline Assessment

No code delta since the 07-01 deep sweep. That report's assessment holds
verbatim: BLAS/TLAS geometry description and build-flag constants clean;
`instance_custom_index == ssbo_idx` contract intact; SSBO indexing and RT
ray-query geometry (bias, tMin, Frisvad basis, glass budget, ReSTIR-DI
spatial normal-cone guard, BC1 punch-through guard) all previously verified
and unchanged; SVGF/TAA denoiser stability, composite reassembly order, and
camera-relative-vs-absolute precision separation all previously verified and
unchanged. See `AUDIT_RENDERER_2026-07-01.md` §"RT Pipeline Assessment" for
the full evidence trail — not restated here to avoid duplicating a report
against a tree that hasn't moved.

## GPU-Struct & Memory Assessment

No code delta since the 07-01 deep sweep. `GpuInstance` 112 B / `GpuCamera`
336 B / `GpuMaterial` 300 B pins, per-field offset pin, 5-site `GpuInstance`
lockstep, capacity constants, sync/barrier chain, and memory/lifecycle
teardown ordering all previously verified and unchanged. See
`AUDIT_RENDERER_2026-07-01.md` §"GPU-Struct & Memory Assessment".

## Findings

### LOW

#### REN-2026-07-02-L01: `DBG_BITS` test catalog still covers only 13 of 17 `DBG_*` constants
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/shader_constants.rs` :: `DBG_BITS` (drives `generated_header_contains_all_defines` + `triangle_frag_dbg_bits_not_redeclared`); constants in `crates/renderer/src/shader_constants_data.rs`; hand-written emits in `crates/renderer/build.rs`
- **Status**: Carried forward, unresolved — was `REN-2026-07-01-L01`
- **Description**: Unchanged from 07-01. `shader_constants_data.rs` declares
  **17** `pub const DBG_*` constants (re-counted this session:
  `grep -c "^pub const DBG_"` → 17); the `DBG_BITS` catalog array in
  `shader_constants.rs` (the shared iteration source for both the header
  value-pin test and the shader no-redeclare guard) still enumerates only
  **13** entries, `DBG_BYPASS_POM` (0x1) through `DBG_LEGACY_LIGHT_ATTEN`
  (0x1000). The four newest bits — `DBG_DISABLE_MULTISCATTER` (0x2000),
  `DBG_DISABLE_ATROUS` (0x4000), `DBG_DISABLE_RESTIR` (0x8000),
  `DBG_DISABLE_SPATIAL` (0x10000) — are still emitted into the generated
  GLSL header via separate hand-written `writeln!` calls in `build.rs`,
  bypassing the catalog, and still carry neither a value-pin nor a
  no-redeclare guard. The in-code doc comment above `DBG_BITS` (lines 25-30)
  still asserts the catalog "cannot recur" drift, which remains inaccurate
  for these four bits.
- **Evidence**: This session re-read `shader_constants.rs:31-45` directly —
  the `DBG_BITS` array body is 13 entries, unchanged byte-for-byte from the
  07-01 citation. `shader_constants_data.rs` `pub const DBG_` count is 17
  (re-verified via grep this session, same as 07-01's citation).
- **Impact**: Unchanged — latent, not live. No shader currently
  shadow-redeclares the four uncovered bits; the generated header values are
  currently correct. Risk remains a future shader or `build.rs` edit on
  those four bits shipping undetected past `cargo test`.
- **Related**: #1482 (original catalog fix); `AUDIT_RENDERER_2026-07-01.md`
  REN-2026-07-01-L01 (same finding, prior session).
- **Suggested Fix**: Unchanged from 07-01 — add the four missing entries to
  `DBG_BITS` and route their header emit through the catalog loop instead of
  the hand-written `writeln!`s in `build.rs`; add a
  `dbg_bits_catalog_covers_every_dbg_constant` test asserting
  `DBG_BITS.len()` equals the `^pub const DBG_` count in
  `shader_constants_data.rs` so the parallel list cannot silently drift
  again.

#### REN-2026-07-02-L02: `with_one_time_commands_inner` still leaks fence/command-buffer on the post-recording error paths
- **Severity**: LOW (pre-existing; fires only on an already-failing GPU submit/wait)
- **Dimension**: Sync/Barriers (error-path resource lifecycle)
- **Location**: `crates/renderer/src/vulkan/texture.rs` :: `with_one_time_commands_inner`
- **Status**: Carried forward, unresolved — was `REN-2026-07-01-L02`
- **Description**: Unchanged from 07-01. Re-read the live function this
  session (`texture.rs:576-680`). The recording-closure failure path
  (`if let Err(e) = f(cmd) { … }`, lines 610-618) now correctly ends +
  frees the command buffer before propagating — that shape is fine. But
  three later fallible calls still propagate via `?` before the
  fence-destroy/`free_command_buffers` cleanup tail runs:
  - `device.reset_fences(&[**guard])?` (line 643) — on error, the allocated
    `cmd` (now past the begin/end boundary is not yet reached, so it's only
    allocated + began) is never freed.
  - `device.queue_submit(*q, &[submit_info], fence)?` (line 670) — on error,
    neither the owned fence (if `owned == true`) nor `cmd` is cleaned up.
  - `device.wait_for_fences(&[fence], true, u64::MAX)?` (line 674) — same
    gap; this is the most likely of the three to actually fire (device-loss
    mid-wait), and it also fires after a successful submit, so the GPU may
    still be mid-execution against `cmd` when the fence/cmd handles are
    abandoned.
  The cleanup tail (`destroy_fence` if owned, `drop(fence_guard)`,
  `free_command_buffers`) sits at lines 675-679, strictly after all three
  fallible calls.
- **Evidence**: Direct read of `crates/renderer/src/vulkan/texture.rs:576-680`
  this session — the three `?` sites and the cleanup-tail ordering are
  byte-identical to the 07-01 citation. No commit has touched this file
  since (confirmed via the empty `git diff --stat 1b4e8e84 HEAD --
  crates/renderer/` above).
- **Impact**: Unchanged — bounded. Fires only when a one-time submit is
  already failing (device-loss/OOM territory). The reusable-fence path
  (`owned == false`, the common case post-init) leaks no fence, only the
  command buffer, which is reclaimed at pool destruction. No per-frame
  accumulation in normal operation.
- **Related**: #1713 (adjacent queue-mutex re-scope, verified race-free in
  07-01, unaffected by this finding), #302 (reusable fence), prior
  `AUDIT_RENDERER_2026-07-01.md` REN-2026-07-01-L02.
- **Suggested Fix**: Unchanged from 07-01 — capture the `reset_fences` /
  `queue_submit` / `wait_for_fences` `Result`s instead of using `?` directly,
  run the destroy-fence-if-owned + `free_command_buffers` cleanup
  unconditionally in all three error arms (or wrap `cmd` + the owned fence
  in a small RAII drop guard), then propagate the original error. Pure
  error-path cleanup, not a barrier/sync semantic change — verify it doesn't
  double-destroy the reusable fence on the `owned == false` path before
  landing.

## Carried-Forward Items (not re-reported as findings)

The three INFO items from `AUDIT_RENDERER_2026-07-01.md`
(REN-2026-07-01-I01/I02/I03) are a documented-design-decision note and two
audit-skill wording corrections, not code defects. They are unaffected by
the zero code delta and are not re-listed here as new findings; see the
07-01 report if pursuing the skill-doc wording fixes (cite #1190 for the
`MAT_FLAG_*` pin mechanism; update the "13-bit `DBG_*` catalog" wording in
the Dim-3/Dim-19 checklists to "17 bits" once L01 above lands).

## Prioritized Fix Order

1. **REN-2026-07-02-L01** (LOW, test) — extend `DBG_BITS` to all 17
   constants + add the count-parity test. One-file test edit.
2. **REN-2026-07-02-L02** (LOW, error path) — add cleanup-before-propagate
   to the three `?` sites in `with_one_time_commands_inner`. Small, isolated,
   testable via the existing one-time-command test seam.

Neither is urgent; both are pre-existing, bounded, and non-blocking.

## Needs-RenderDoc

No new sync/barrier finding requires capture-based verification this
session (zero code delta). The two open items carried from 07-01 remain:

- The #1748 `draw_frame` extraction's live-frame-capture confirmation (that
  the G-buffer→SHADER_READ and caustic-accum→SHADER_READ barriers sit at the
  `record_geometry_pass` tail / `record_post_passes` head) is still owed —
  no capture was run this session either.
- `water.frag` still has no shader-side `sceneFlags.x` early-out (CPU-gated
  on `rt_live` per #1561) — still flagged as a RenderDoc-verified follow-up,
  not a suspected defect.

## Disproved / Not Reported

No new claims investigated and disproved this session (confirmation-pass
scope). See `AUDIT_RENDERER_2026-07-01.md` §"Disproved / Not Reported" for
the standing list (caustic decay-vs-splat race, bloom-sampling-TAA-output,
skinned-output `VERTEX_BUFFER` flag, Cornell metalness/glass-stipple
confounds) — all still valid disproofs against the unchanged tree.
