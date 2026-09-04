# Concurrency & Synchronization Audit — 2026-09-04 (scoped: water-deep)

**Scope note**: this is a SCOPED slice of `/audit-concurrency` run as part
of the `water-deep` audit-suite preset. It covers only **Dimension 1**
(Vulkan Queue & Acceleration-Structure Sync) and **Dimension 2**
(Compute → AS → Fragment Chains), with Dimension 2 specifically tracing
the water-caustic per-FIF `R32_UINT` accumulator chain as its
water-relevant slice. Dimensions 3–7 (ECS lock ordering, scheduler access
declarations, RwLock/physics patterns, GPU teardown ordering, worker
threads) are **out of scope** for this run — do not read the summary
below as a full 7-dimension concurrency sweep.

HEAD at audit time: `040e7a33`. Dedup baseline: the most recent full
sweep, `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` (committed
`64f64480`). Per the mandatory dedup protocol, every checklist item below
was re-verified against current code rather than inherited, and every
candidate finding was checked against `gh issue list` (72 open issues at
fetch time, saved to `/tmp/audit/concurrency/issues.json`) before being
reported.

**Headline result: zero new findings.** All 6 findings from the
2026-08-30 baseline (3 in each dimension) are CLOSED with their fixes
confirmed present at HEAD. In between, this exact surface underwent a
~4,700/-3,200 line restructuring (`draw_frame` split into 5 phase files
under #3282/#3246/#3247, `recreate_screen_passes` split into 5 phase
methods under #3738) that preserved every barrier, lock-order, and
recovery invariant this dimension's checklist tracks — including one
same-day regression (#3636, introduced by the #3738 split, fixed the same
day it was found, before this audit ran) that is verified fixed below
rather than re-reported.

---

## Dimension 1 — Vulkan Queue & Acceleration-Structure Sync

**Entry points re-read at HEAD**: `crates/renderer/src/vulkan/context/draw.rs`,
the five #3282 phase files (`sync_and_acquire_frame.rs`,
`begin_frame_recording.rs`, `assemble_camera_and_lights.rs`,
`dispatch_skin_and_cluster.rs`, `build_and_upload_instances.rs`),
`crates/renderer/src/vulkan/sync.rs`,
`crates/renderer/src/vulkan/acceleration/{blas_static,blas_skinned,tlas,predicates,memory}.rs`,
`crates/renderer/src/vulkan/context/resize.rs` (now 1,850 lines, 5-phase
split under #3738).

### Dedup: prior findings, verified fixed

| Prior finding | Severity | Fixed by | Verified at HEAD |
|---|---|---|---|
| CONC-D1-2026-08-30-01 — #870 remediation list named only the depth image, missed 4 other resources riding the both-slots wait | MEDIUM | #3643 (`c9158b5d`) | `sync.rs`'s block now enumerates all 5 non-per-FIF dependents (scratch buffer, depth-capture staging, terrain-tile buffer, screenshot/depth-capture readback, morph weight buffer) plus a source-pinned regression test (`frames_in_flight_contract_names_every_dependent_resource`) that greps the live code for each name |
| CONC-D1-2026-08-30-02 — `with_one_time_commands` doc still described the pre-#1713 lock scope | LOW | #3644 (`a80b1a80`) | Duplicate/stale summary line removed |
| CONC-D1-2026-08-30-03 — `images_in_flight` doc's stated deadlock rationale was false at HEAD (#952 moved `reset_fences`) | LOW | #3645 (`f13cdbd3`) | Doc now correctly frames the aliasing guard as a redundant-wait skip, not a deadlock preventer |

### Regression-guard re-verification (checklist items 1–9)

1. **Single-Mutex queue submission — CLEAN.** Submit (`draw.rs:1997-2035`)
   and present (`draw.rs:2113-2128`) both bind the `MutexGuard` across the
   call, with an explicit `drop()` on every error arm before recovery.
   One-time commands unchanged since #1713.
2. **Frame-in-flight discipline — CLEAN**, now living in
   `sync_and_acquire_frame.rs:50-62`: the dual-fence wait
   (`in_flight[frame]` + `in_flight[(frame+1)%N]`) is unchanged; `reset_fences`
   is still immediately before `queue_submit` (`draw.rs:1978-1995`).
3. **Acquire→submit `?`-propagation window — re-verified under the new
   file split.** The prior report's "zero `?` operators in this window"
   claim is now literally false — `draw_frame` calls
   `begin_frame_recording(...)?` and `assemble_camera_and_lights(...)?`
   after the acquire (`draw.rs:1705`, `:1729-1744`). Traced both callees:
   every fallible call inside each (`reset_command_buffer` /
   `begin_command_buffer` in `begin_frame_recording.rs:49-76`;
   `build_fsr_frame_parameters` in `assemble_camera_and_lights.rs:217-236`)
   explicitly calls `recreate_image_available_for_frame` before its own
   `return Err`, so the outer `?` never actually leaks the acquire
   semaphore — recovery happens inside the callee. Invariant holds; the
   "zero `?`" phrasing is now doc-stale relative to the file split
   (documentation follow-up, not a finding).
4. **AS build → read barrier — CLEAN.** `skinned_blas_refit.rs`'s publish
   barrier retains `COMPUTE_SHADER` in its dst mask (see D2 below); the
   TLAS build's `AS_WRITE→AS_READ` barrier (now
   `dispatch_skin_and_cluster.rs:301-309`) still fires on both the build
   success and failure arms (#2931).
5. **Deferred BLAS-scratch destruction (#1782) — CLEAN.**
   `blas_skinned.rs`'s immediate-free SAFETY comment was strengthened
   (not weakened) by #3643 to name the both-slots wait as the actual
   guarantee.
6. **Swapchain recreate sync — CLEAN, with one same-day regression
   already found and fixed by the project.** `recreate_swapchain_core`
   still opens with `device_wait_idle`. The #3738 split
   (`66003aa3`, 2026-09-03) left a latent hazard — `PresentationPipeline`'s
   descriptors reference `FrameUpscaler`'s output views, and
   `FrameUpscaler::recreate` unconditionally destroys+recreates those
   views with no `rebind_upscaled_views` sibling to
   `composite.rs::rebind_hdr_views` — that #3636 (`040e7a33`, the tip
   commit at fetch time) fixed the same day. Verified in place:
   `resize.rs:1074-1120` destroys `presentation` → recreates the upscaler
   → constructs a fresh `PresentationPipeline` against the new views, and
   a static-source-order test
   (`presentation_is_retired_before_the_upscaler_view_handoff_it_depends_on`,
   `resize.rs:1585`) pins that ordering against the live source string.
   Not re-reported (CLOSED, fix confirmed) — recorded here so it is
   verified rather than rediscovered.
7. **Water-caustic per-FIF accumulator resize — CLEAN.**
   `recreate_gbuffer_dependent_passes` (`resize.rs:685-752`) recreates
   `water_caustic_accum` under the outer `device_wait_idle` and
   unconditionally rebinds `WaterPipeline`'s set 2 (to the accumulator or
   a placeholder sink, #2142) on either failure arm — matches
   `water_caustic_rebind_is_not_gated_on_accumulator_presence` (`resize.rs:1621`).
8. **AS build INPUT flag, deferred AS destruction, one-time submits** —
   no commits since the baseline touch these access/stage masks.
   `predicates.rs`'s new `sort_tlas_instances_by_blas_address` (#3666)
   and `skinned_blas_refit_limit` per-entity jitter (#3669) are pure
   CPU-side scheduling decisions (rebuild-vs-refit timing, instance
   order for UPDATE-mode stability) with no barrier involvement.

### New findings

**None.**

### Coverage gaps

- No validation-layer or RenderDoc run performed this pass (source-level
  analysis only). Nothing here proposes a new barrier/stage/layout
  change, so nothing requires fresh `BYRO_VALIDATION=1` confirmation —
  #3636's fix is a source-ordering change (destroy→recreate→rebuild),
  not a mask change, and ships with its own static-order test.
- The five new #3282 phase files (5,631 combined lines) were read close
  to fully; passes with no queue/AS/semaphore interaction (material
  interning, blend-pipeline warm-up) were skimmed only.

---

## Dimension 2 — Compute → AS → Fragment Chains

**Entry points re-read at HEAD**: `crates/renderer/src/vulkan/skin_compute.rs`,
`crates/renderer/src/vulkan/context/{skinned_blas_refit,dispatch_skin_and_cluster,build_and_upload_instances,post_passes}.rs`,
`crates/renderer/src/vulkan/{caustic,water_caustic,volumetrics,bloom,material}.rs`,
`crates/renderer/src/vulkan/scene_buffer/upload.rs`.

**Water focus**: the water-caustic per-FIF `R32_UINT` accumulator
(`water_caustic.rs`) was traced end to end as this run's water-relevant
slice, per the task brief.

### Dedup: prior findings, verified fixed

| Prior finding | Severity | Fixed by | Verified at HEAD |
|---|---|---|---|
| CONC-D2-2026-08-30-01 — `caustic_splat.comp` dereferenced the skinned-vertex SSBO from COMPUTE with no COMPUTE_SHADER-visible publish (regression of the #2403 class) | HIGH | #3582 (`5c9d584f`) | `skinned_blas_refit.rs`'s publish barrier dst mask at HEAD is `ACCELERATION_STRUCTURE_BUILD_KHR \| FRAGMENT_SHADER \| COMPUTE_SHADER` (`:502-510`), with an in-line comment citing #3582 and the exact `caustic_splat.comp` inline-deref provenance the finding described. (Landed before the 2026-08-30 report's own commit despite appearing there as NEW; re-verified directly against live code, not against the old report's text.) |
| CONC-D2-2026-08-30-02 — `CausticPipeline::clear_for_skip`'s `TRANSFER_WRITE` never in the source scope of the next visit's decay read | MEDIUM | #3646 (`1889585a`) | Fix landed same day as the baseline report |
| CONC-D2-2026-08-30-03 — `VolumetricsPipeline::record_neutral_frame`'s clear not in `pre_int_write`'s source scope (WAW sibling of -02) | LOW | #3647 (same `1889585a` commit) | "Fix #3646 as well — same defect shape on the volumetrics side, landed in one pass as both issues ask" |

### Regression-guard re-verification

1. **Skin chain, palette half — CLEAN.** `dispatch_skin_and_cluster.rs`'s
   palette barrier (`:185-202`) still targets
   `COMPUTE_SHADER | VERTEX_SHADER`, matching its two consumers
   (`skin_vertices.comp`, `triangle.vert` inline skinning).
2. **Skin chain, skin → AS → TLAS → ray-query half — CLEAN.** Full walk
   intact: `COMPUTE/SHADER_WRITE → AS_BUILD|FRAGMENT|COMPUTE/SHADER_READ`
   (`skinned_blas_refit.rs`, #3582 fix) → shared-scratch
   `AS_WRITE→AS_WRITE|AS_READ` self-emitted per build/refit
   (`blas_skinned.rs`, #1790 guard) → the single frame `AS_WRITE→AS_READ`
   publish on both success/failure arms (`dispatch_skin_and_cluster.rs:301-309`,
   #2931 guard, comment updated but semantics unchanged).
3. **#3665 "upload only dirty bone slots" — checked for a partial-copy
   barrier gap. None found.** `upload_bone_worlds` now issues one
   `vk::BufferCopy` region per dirty slot instead of a whole-buffer copy,
   but `record_bone_world_copy` still emits exactly one
   `TRANSFER_WRITE→SHADER_READ` buffer barrier per frame whose
   `[0, barrier_size)` envelope (computed from the highest dirty region's
   end offset) is a strict superset of every sparse region recorded that
   frame — it cannot under-cover. Slots clean this frame keep whatever a
   *prior* frame's copy+barrier already published to that per-FIF device
   buffer; correctness of "every per-FIF buffer saw the dirty write at
   least once" is tracked by a per-slot bitmask
   (`bone_world_slot_states`) that is unit-tested
   (`dirty_slot_is_refreshed_once_into_each_frame_buffer`,
   `a_new_pose_rearms_both_frame_buffers`). Same pattern class as the
   pre-existing `#1811`/`clean_skin_frames` guarantee, one level more
   granular.
4. **Volumetrics gate (#1105) — CLEAN.** `tlas_written` set/reset
   symmetry unaffected by the split; #3685's
   `volumetrics_cleared_on_skip` reset (`resize.rs:1141`) mirrors the
   pre-existing `caustic_cleared_on_skip` reset, keeping both latches'
   resize-invariants identical.
5. **Water-caustic per-FIF `R32_UINT` accumulator — CLEAN, and
   structurally immune to the CONC-D2-02/03 defect class.** Full
   lifecycle trace:
   - `clear_pre_render_pass` is called **unconditionally every frame**
     whenever `water_caustic_accum` is `Some`
     (`build_and_upload_instances.rs:943-946`) — there is no skip-clear
     / history-valid / decay branch analogous to `CausticPipeline`'s.
     Its internal sequence is self-contained and atomic:
     `FRAGMENT_SHADER→TRANSFER` pre-clear barrier → `cmd_clear_color_image`
     → `TRANSFER→FRAGMENT_SHADER` post-clear barrier, all in one call, so
     there is no separate "next visit's read" barrier that could omit
     the clear's source scope the way `caustic.rs`'s `pre_decay` did.
   - `water.frag` atomic-adds into the accumulator during the main
     render pass, between the post-clear barrier and the barrier below.
   - `barrier_post_render_pass` runs unconditionally as the first
     statement of `record_svgf_pass` right after the main render pass
     ends (`post_passes.rs:320-322`, unmoved by the #3282 split),
     publishing `GENERAL/write → GENERAL/read` for `composite.frag`, the
     accumulator's only consumer.
   - Net: clear → write → publish → read is one linear per-frame chain
     with no cross-frame ping-pong and no conditional skip path, so it
     cannot exhibit the "clear's write never reaches the next visit's
     read" hazard that #3646/#3647 fixed in `caustic.rs` /
     `volumetrics.rs`. Confirms Dimension 1's / the prior report's
     "consumer is composite.frag only" note still holds post-split.
6. **Cross-frame ping-pong indexing (SVGF/TAA/ReSTIR/volumetrics)** — no
   commits since the baseline touch the `(f+1)%N`-vs-previous-slot
   indexing in `svgf.rs`/`taa.rs`/`restir.rs`/`volumetrics.rs` (the only
   `volumetrics.rs` diffs are the #3646/#3647 barrier fix and unrelated
   weather/wind shader-parameter additions, confirmed to touch only
   `GpuFogVolume`/`VolumetricsParams` fields and shader math). Relying on
   the 2026-08-30 CLEAN verdict here rather than a full re-derivation —
   see coverage gaps.
7. **Master ordering (`record_post_passes`)** — untouched code; only its
   caller-side wiring moved under #3282.

### New findings

**None.**

### Coverage gaps

- No validation-layer or RenderDoc run performed. Nothing in this pass
  proposes a new barrier/mask change.
- Item 6 (ping-pong indexing) was confirmed by diff absence, not a full
  re-read against the checklist — acceptable for a regression-guard-scoped
  run but due a full re-read in the next non-scoped concurrency audit.
- FSR/`frame_upscaler.rs` internals remain opaque to source reading, same
  as the prior report.

---

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |

No new findings in either dimension. All 6 findings from the
2026-08-30 baseline are closed with fixes confirmed in place. One
same-day regression (#3636), introduced by the #3738 resize-path split
and unrelated to any prior finding, was found and fixed by the project
before this audit ran — verified fixed, not re-reported.

**Suggested next step**: none — nothing here is publishable via
`/audit-publish` (zero findings). If a full 7-dimension sweep is wanted,
run `/audit-concurrency` without `--focus 1,2`.
