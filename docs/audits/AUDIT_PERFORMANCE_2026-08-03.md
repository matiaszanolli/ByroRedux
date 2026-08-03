# Performance Audit — 2026-08-03

Scope: full 9-dimension `/audit-performance` sweep (deep). One leg of a
`comprehensive` audit-suite sweep run the same day as `/audit-ecs`
(`docs/audits/AUDIT_ECS_2026-08-03.md`, 0 findings) and `/audit-renderer`
(`docs/audits/AUDIT_RENDERER_2026-08-03.md`, 0 CRITICAL/HIGH, 3 MEDIUM
carried-forward visual defects). This audit runs against the same HEAD
(`1ae86f62`) and does **not** re-derive ground already covered today by those
two sibling audits — see "Cross-Audit Coverage" below for exactly what was
skipped and why.

## Method

Read the prior performance audit (`docs/audits/AUDIT_PERFORMANCE_2026-07-25.md`)
to establish baseline: that pass found 3 HIGH + 6 MEDIUM findings, most already
unfiled-but-actionable. `git log --oneline --since="2026-07-25"` shows 122
commits since — a full session's worth of work, including a large streaming
rearchitecture (foreground-first exterior loading, resumable cell application
with deadline budgeting, resumable NPC assembly, shared LOD reconciliation)
that landed *after* the 2026-07-25 report and before either of today's ECS/
Renderer sweeps touched it. That rearchitecture (Dimension 7) got the deepest
new investigation in this pass; Dimensions 1-6, 8-9 were verified by direct
code read against every named regression guard in the skill, with fixes to
the 2026-07-25 findings independently re-confirmed in the live tree (not
trusted from commit messages alone).

No GPU device bench re-run was performed this session (see "Bench-of-Record
Staleness" below) — findings are code-verified, not FPS-measured, consistent
with how today's sibling Renderer audit also scoped its verification
(`cargo test` + shader-artifact check, no live bench).

## Executive Summary

**0 CRITICAL, 1 HIGH, 2 MEDIUM, 2 LOW — all 5 NEW.** Every regression guard
enumerated in the skill (18 distinct guards across Dimensions 1-6, 9) was
re-verified against live code this session and found **intact** — including
three that were found *eroded* in the 2026-07-25 report (`camera_cut`
misfiring, the two-sided blend-split `z_write` regression, particle/entity-ID
motion-history collision) and have since been correctly fixed with dedicated
regression tests, not just reverted. Two items from 2026-07-25 (PERF-D1-01
scheduler-timing gate, D2-03/PERF-D4-03 `HashMap`→`FxHashMap`) are also
confirmed fixed.

The one real gap: the new resumable/budgeted streaming architecture (Session
62-63, commits `67081437`/`484893de`/`9926fa50`/`9bf4c493`) is sound in its
core design — cursor-based resumption, no re-scan-to-find-cursor pattern, no
double-spawn risk — but has one real hole (**PERF-D7-01**, HIGH) where a
pre-existing synchronous load path bypasses the whole budgeting system it was
built to replace, plus two smaller CPU-waste items introduced alongside it.

## Cross-Audit Coverage (what this report deliberately did not re-derive)

| Area | Owner today | What it confirmed (cited, not repeated) |
|---|---|---|
| `GpuInstance` (128 B) / `GpuMaterial` (348 B) / `GpuFogVolume` (64 B) lockstep | `/audit-renderer` 2026-08-03 | All three unchanged/tested this session; new `GpuFogVolume` gained the `offset_of!` lockstep test it was missing yesterday |
| BLAS/TLAS build constants, mid-batch eviction gate, deferred-destroy queues | `/audit-renderer` 2026-08-03 | "Untouched by this session's [shader/shadow-mask/refactor] work" — independently re-confirmed here via direct read of `constants.rs` (Dimension 3/6 below) |
| ECS lock ordering, storage capacity guards, `drain_dirty_into`/`take_dirty`, transform-propagation fast path, `resolve_pbr` NIFAL boundary | `/audit-ecs` 2026-08-03 | Zero findings, all guards intact — this report cites rather than re-walks `world.rs`/`packed.rs`/`systems.rs` |
| `build_tlas` / `record_post_passes` mechanical splits (#2258/#2259) | `/audit-renderer` 2026-08-03 | Flagged as needing a RenderDoc/live-run smoke pass (cargo test alone can't confirm no barrier reordering) — this report does not re-litigate, just notes it belongs in the same "needs hardware validation" bucket as PERF-D7-01 below |

## Findings

### PERF-D7-01: Worldspace-persistent-cell load bypasses the new resumable/budgeted streaming architecture entirely
- **Severity**: HIGH
- **Dimension**: World Streaming & Cell Transitions (7)
- **Location**: `byroredux/src/scene/world_setup.rs:511-523` → `cell_loader/exterior.rs:111` (`load_worldspace_persistent_cell`) → `cell_loader/references/mod.rs:158` (`FrameTimeBudget::unlimited()`)
- **Status**: NEW
- **Description**: `world_setup.rs`'s "foreground-first" bootstrap comment (lines 492-502) claims it "blocks only for the center cell," but before `stream_initial_radius` even computes the load deltas, it synchronously runs `load_worldspace_persistent_cell` for every globally-persistent REFR within `radius_load` cells of the spawn point. That call reaches `load_references(..)`, which still hard-codes `FrameTimeBudget::unlimited()` — the same unbudgeted primitive the new resumable-NPC-assembly work (`9bf4c493`) was built specifically to replace. Persistent-cell REFRs include NPCs (Whiterun's guards, tavern keepers, etc.), which now route through the brand-new cooperative `NpcSpawnJob` state machine, but driven to completion with zero yields.
- **Evidence**: `resumable.rs`'s own module doc calls this class of stall ("the largest remaining EXAL apply outlier") the exact motivation for the new machinery, yet this specific call site was not migrated to it. `git show 9bf4c433 --stat -- byroredux/src/scene/world_setup.rs` touches the file but the `load_worldspace_persistent_cell` call site itself is unchanged.
- **Impact**: Cost scales with the streaming radius footprint (persistent NPC density across N cells), not the single arrival cell — the render thread stalls for the full synchronous cost of every persistent-cell NPC assembly at boot/fast-travel, on exactly the class of exterior scene (dense Bethesda settlement) the "foreground-first" work targeted.
- **Related**: Session 62-63 streaming rearchitecture; conceptually adjacent to the confirmed-open #1798 below (both are "budget primitive exists, one call site doesn't use it" gaps).
- **Suggested Fix**: Thread a real `FrameTimeBudget` through `load_worldspace_persistent_cell`, or dispatch it through the same async worker/apply pipeline used for regular streamed cells instead of running it inline during bootstrap.

### PERF-D7-02: `tag_descendants_as_actor` re-walks the whole attached subtree from scratch after every NPC part attach, not once per NPC
- **Severity**: MEDIUM
- **Dimension**: World Streaming & Cell Transitions (7) / NPC Spawn
- **Location**: `byroredux/src/npc_spawn/resumable.rs:1126-1133` (`parent_part`, new in `9bf4c493`), plus the `Finalize` call at `:782`/`:1092`; walked function at `byroredux/src/npc_spawn.rs:856-885`
- **Status**: NEW
- **Description**: Pre-`9bf4c493`, `tag_descendants_as_actor` ran exactly once, at the end of each NPC's spawn (`git show 9bf4c493^:byroredux/src/npc_spawn.rs` confirms single call site). The resumable rewrite now calls it after **every** skeleton/body-piece/head/hair/brow/eye/armor attach (roughly 11-16 calls for a typical FaceGen actor with body + head + hair/brow + eyes + several armor pieces), plus once more at `Finalize`. Each call does a fresh BFS from the actor root over every currently-attached descendant — two fresh ECS queries and a freshly-allocated `Vec` queue — re-tagging entities a previous call already tagged. This runs on every streamed NPC now, not just occasionally.
- **Impact**: Real, avoidable multiplier on the streaming hot path: roughly quadratic in part count per actor (bounded — actor subtrees are small, ~10-20 entities — so not catastrophic, but it is wasted CPU reintroduced by this rewrite that did not exist before it).
- **Suggested Fix**: Have each `parent_part` call site tag only the specific entity/subtree it just attached (the caller already knows exactly what was added), or defer all tagging to the single `Finalize` call.

### PERF-D7-03: SCOL/PKIN child-placement expansion is recomputed from scratch on every resumed tick that yields mid-REFR
- **Severity**: MEDIUM
- **Dimension**: World Streaming & Cell Transitions (7)
- **Location**: `byroredux/src/cell_loader/references/mod.rs:418-433` (`expand_pkin_placements`/`expand_scol_placements` call), `:450` (skip-forward via `synth_idx < job.next_synth`)
- **Status**: NEW
- **Description**: The expanded `synth_refs: Vec<...>` for a SCOL/PKIN's children is a **local** variable inside the per-REFR processing step, not stored on the resumable `ReferenceLoadJob`. If `budget.should_yield()` fires partway through a large static-collection's children, the next resumed tick re-enters the same `next_ref` and recomputes the *entire* expansion (re-walking `scol.parts`/`.placements`, recomposing every child transform via `GlobalTransform::compose_trs`) before skip-forwarding past already-processed entries using `job.next_synth`.
- **Impact**: For an unusually large SCOL (clutter/rock/tree collections can carry hundreds of placements) split across several budget-limited frames, this is O(k·m) work instead of O(k), where k is the SCOL's child count and m is the number of resume ticks needed. Narrow blast radius (only large SCOLs that straddle a budget boundary), but it is CPU work that did not exist when this code ran synchronously to completion, so it is a real cost introduced by the resumable rewrite.
- **Suggested Fix**: Cache the expanded `synth_refs` (and any overlay data) on `ReferenceLoadJob` alongside `next_ref`/`next_synth` so a mid-REFR yield resumes without re-expanding.

### PERF-D9-01: `gpu_timers.rs` has no per-bracket "ran this frame" flag — an inactive pass and a genuinely-instantaneous one both read back `0.0`
- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost (9)
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:55-70` (doc comment above `read_and_reset`)
- **Status**: NEW (self-documented gap, not previously filed)
- **Description**: The module's own doc comment states plainly: "There is currently no per-bracket 'ran this frame' flag exposed to consumers; `0.0` is ambiguous between 'inactive' and 'genuinely instantaneous.'" `read_and_reset` builds a fresh `GpuTimerSnapshot::default()` each call and only fills fields whose `active_bits` bit was set, but `active_bits` itself isn't surfaced to the `bench-stats`/`skin.coverage` consumers this audit's own instrumentation guidance points findings at (Dimension 6/9 guidance: "quantify via these hooks, don't estimate").
- **Impact**: Telemetry-correctness only, no runtime cost — but it undermines the "cite the GPU timer, don't guess" instruction this skill gives every other dimension, since a `0.0` for e.g. the skin-dispatch bracket on a frame with no skinned draws is indistinguishable from a broken timer.
- **Suggested Fix**: Expose `active_bits` (or a per-field `Option<f32>`/bool pair) on `GpuTimerSnapshot` so `bench-stats` can print "n/a (didn't run)" instead of `0.0`.

### PERF-D-DOC-01: `ROADMAP.md` Bench-of-record predates ~90 commits of substantial rendering/streaming work
- **Severity**: LOW
- **Dimension**: Cross-cutting (documentation/process)
- **Location**: `ROADMAP.md:71` ("Bench-of-record (LIVE) — R6a-stale-17 refresh (2026-07-26, HEAD `3a02b02d`)")
- **Status**: NEW
- **Description**: The pinned bench-of-record HEAD (`3a02b02d`) sits roughly 90 commits behind the current tree (`1ae86f62`). The intervening work is not cosmetic: the full procedural volumetric-fog rewrite (froxel V-buffer, temporal reprojection, new `composite.frag`/`volumetrics_inject.comp` GPU cost), the entire resumable/budgeted streaming rearchitecture (Dimension 7 above), a materials-pipeline refactor (`ImportedMaterial`/`MaterialTextureSet<T>`), and the Scaleform/FSR3 host-bridge work have all landed since. ROADMAP already flags the block as stale in its own text, so this is not new information, but the gap has grown large enough (a full session, several GPU-cost-relevant features) that the next `/session-close` bench refresh is now overdue rather than optional.
- **Impact**: No functional impact — this is a documentation/process gap. It does mean any FPS claim made against the current tree cannot be sanity-checked against a comparably-recent baseline right now.
- **Suggested Fix**: Run `scripts/fsr-bench-matrix.sh 3 300` against current HEAD at the next opportunity with GPU access and refresh the ROADMAP block.

## Existing / Re-Confirmed (not counted as new findings)

- **#1798** (interior cell load has no per-frame NPC-spawn budget) — **re-confirmed still open**. `byroredux/src/cell_loader/load.rs:404` (`load_cell_with_masters`) still calls `load_references(..)` → `FrameTimeBudget::unlimited()`. `git show 9bf4c493 --stat -- byroredux/src/cell_loader/load.rs` is empty — this session's resumable-NPC-assembly work did not touch the interior path. The interior path *does* now route NPCs through the new cooperative `NpcSpawnJob` machinery (no logic duplication was introduced), it simply never yields. Distinct from PERF-D7-01 above (that one is the *exterior* persistent-cell path; this is the *interior* cell-load path) — both share the same root cause (a call site that never adopted the new budget primitive) but are two different call sites.
- **#2215** (RT-1: indirect-draw grouping regression, fnv/oblivion/fo4 `gpu_calls` still elevated) — status-noted only per `/audit-renderer` 2026-08-03's own re-check (still OPEN, no commit since 2026-08-02 touches the implicated batch-key logic). Not re-investigated in this pass; cross-referenced because it is a Draw & Instancing (Dimension 2) finding at heart.
- **PERF-D1-03** (2026-07-25): draw-sort parallel threshold (`DRAW_SORT_PARALLEL_THRESHOLD = 3000`, `byroredux/src/render/mod.rs:429`) calibration predates the 10→11-tuple sort-key widening. Re-verified: constant is unchanged at 3000. Not re-elevated — no new evidence this session that the threshold is miscalibrated for the current tuple width, carried forward as an open low-priority question only.

## Regression Guards Verified Intact (no erosion found)

All checked by direct code read against the exact symbols the skill names, not assumed from commit messages:

| Guard | Location | Verdict |
|---|---|---|
| `drain_dirty_into` capacity preservation (#1371) | `crates/core/src/ecs/packed.rs` | Intact — also independently confirmed by today's `/audit-ecs` |
| Animation system persistent scratch (`entities_scratch`/`playback_scratch`, #1372) | `byroredux/src/systems/animation.rs:377-521` | Intact, clear+extend pattern preserved |
| Billboard camera-move skip (`last_cam`, #1374) | `byroredux/src/systems/billboard.rs:21-59` | Intact |
| Debug-UI snapshot deep-clone gated on `visible` (#1376) | `byroredux/src/main.rs:501-511` | Intact |
| `bone_world` resize-not-clear reuse (#1794) | `byroredux/src/render/skinned.rs:137-198` | Intact — no unconditional `.clear()` found |
| Particle dead-probe removal (#1803, no unused `GlobalTransform` query) | `byroredux/src/render/particles.rs` | Intact — zero `GlobalTransform` references in the file |
| GT-presence hoist (#1377/#1805) | `byroredux/src/render/static_meshes.rs:151-187` | Intact, and further hardened (FX-skip hoisted right after the visibility gate too) |
| Two-sided blend split gate (#1804) | `crates/renderer/src/vulkan/context/draw.rs:1063-1066` | **Previously eroded** (2026-07-25 found the `z_write` proxy regressed by `883f57cd`) — now **correctly fixed**: gate uses `b.order_dependent_glass` (material-kind-derived), not the `z_write` proxy that broke twice in opposite directions. Dedicated doc comment explains both prior failure modes. |
| `camera_cut` / origin-history preservation (#1489, PERF-D9-NEW-01 from 07-25) | `crates/renderer/src/vulkan/context/draw.rs:79-350` (`is_camera_cut` + `camera_cut_tests`) | **Previously eroded**, now fixed — extracted into a standalone function with a dedicated test module including the exact "crossing + walking ⇒ no cut" regression case the prior audit called for |
| Particle/entity-ID motion-history isolation (PERF-D4-01 from 07-25) | `crates/renderer/src/vulkan/context/draw.rs:2267` (`uses_rigid_history && !camera_cut`) | Fixed per commit `11ae4a35` |
| BLAS mid-batch eviction gate (#1792) / dynamic budget constants | `crates/renderer/src/vulkan/acceleration/constants.rs` | Untouched this session — also independently confirmed by today's `/audit-renderer` |
| `SKINNED_BLAS_FLAGS` = `PREFER_FAST_BUILD` (not FAST_TRACE) | `crates/renderer/src/vulkan/acceleration/constants.rs:93-112` | Intact |
| `SKINNED_BLAS_REFIT_THRESHOLD` = 600 | same file | Intact |
| Skin dispatch-dirty gate (`pose_dirty`, #1195) | `crates/core/src/ecs/resources/skin_slot_pool.rs`, `byroredux/src/render/skinned.rs:157-198` | Intact |
| Descriptor-rewrite skip (#1197) | `crates/renderer/src/vulkan/skin_compute.rs:553-576, 902-925` | Intact — both the skin-dispatch and BLAS-refit descriptor arrays gate on a live-key comparison |
| `skin_dispatch_ran` rollback on bailed frame (#1791/#1796) | `crates/renderer/src/vulkan/context/draw.rs:1212-1214, 3712-3758` | Intact, with a dedicated ordering-regression test (`skin_dispatch_ran_ordering_tests`) |
| `GpuInstance` 128 B / `GpuMaterial` 348 B lockstep | `crates/renderer/src/vulkan/scene_buffer/` | Unchanged — also independently confirmed by today's `/audit-renderer` |
| PBR resolve-once, no per-draw `classify_pbr_keyword` re-entry | `crates/core/src/ecs/components/material.rs` | Unchanged — also independently confirmed by today's `/audit-ecs` |
| `upload_instances` O(live data), content-hash dirty gate (#1134/PERF-D8-NEW-01 from a prior audit) | `crates/renderer/src/vulkan/scene_buffer/upload.rs:538-560` | Intact — `count = instances.len().min(MAX_INSTANCES)`, hash-gated skip on unchanged frames |
| `ENABLE_LEGACY_WRS` shipped default 0 (#1799) | `crates/renderer/src/shader_constants_data.rs:632` | Intact |
| `inv_view_proj`/`invViewProj` computed once on CPU, no shader-side `inverse()` | `crates/renderer/shaders/{ssao.comp,cluster_cull.comp,composite.frag,volumetrics_inject.comp}` | Intact — every shader consumes a precomputed UBO field, no per-invocation `inverse()` call found |
| `origin_corrected_prev_view_proj` history preservation on cell-boundary crossing (#1489) | `crates/renderer/src/vulkan/context/draw.rs:3262-3310` | Intact, with round-trip + non-trivial-offset tests |
| `froxel_extent` resolution-derived (not fixed grid) | `crates/renderer/src/vulkan/volumetrics.rs:437-447` | Intact, pinned by `froxel_extent_uses_render_resolution_and_configured_divisor` |
| dhat allocation-bound test coverage on NIF parse path | `crates/nif/tests/heap_allocation_bounds.rs`, `heap_allocation_bounds_geometry.rs` | Both present |
| Resumable-state carried forward, not reallocated per resumed tick (new architecture) | `ReferenceLoadJob`, `NpcSpawnJob`'s `RuntimeNpcState`/`PrebakedNpcState`, `WorldStreamingState::active_apply` | Verified fine (Dimension 7 sub-investigation) |
| Budget-check granularity (once per REFR/synth-child/assembly-unit, never sub-loop, never too coarse) | `references/mod.rs:340,453`; `resumable.rs:177`; `streaming_helpers.rs:424` | Verified fine |
| LOD reconciliation diffs against previous mask, doesn't recompute wholesale | `cell_loader/terrain_lod.rs:313-349` | Verified fine |
| No double-spawn / leaked half-built entity on interrupted resumable NPC assembly | `byroredux/src/npc_spawn/resumable.rs` (`RuntimePhase` cursor enum) | Verified fine — phase cursor advances exactly once per completed unit, no re-entrant path |

## Hot Path Analysis

No live GPU-timer / `ScratchTelemetry` capture was taken this session (no bench
re-run — see PERF-D-DOC-01). The per-pass GPU cost table and CPU wall-clock
breakdown (`cpu_ms:` line, `byroredux/src/systems/debug.rs`) remain the
correct instruments per the skill's own guidance; this audit verified the
instrumentation itself is trustworthy (gpu_timers reads the prior frame's
completed queries, not a blocking stall — confirmed via the module doc) but
did not capture fresh numbers.

## Prioritized Fix Order

1. **PERF-D7-01** (HIGH) — thread a real `FrameTimeBudget` (or dispatch through
   the async cell-load pipeline) into `load_worldspace_persistent_cell`. This
   is the one place the new "foreground-first" architecture's own stated goal
   is currently violated.
2. **PERF-D7-02** (MEDIUM) — narrow `tag_descendants_as_actor` to the
   newly-attached subtree at each `parent_part` call site, or defer to
   `Finalize` only. Quick, mechanical, no design change.
3. **PERF-D7-03** (MEDIUM) — cache `synth_refs`/`next_synth` state on
   `ReferenceLoadJob` so a mid-REFR yield doesn't re-expand a SCOL/PKIN's
   children from scratch.
4. **PERF-D9-01** (LOW) — expose `active_bits` on `GpuTimerSnapshot` so
   telemetry consumers can distinguish "didn't run" from "ran in 0 ms."
5. **#1798** (Existing, HIGH per its own tracking, not re-elevated here) —
   thread the same budget primitive into the *interior* `load_references`
   call site (`cell_loader/load.rs:404`); natural companion fix to #1 above
   since both are the same primitive missing from a different call site.
6. **PERF-D-DOC-01** (LOW) — refresh `ROADMAP.md`'s Bench-of-record at the
   next GPU-accessible session; ~90 commits of rendering/streaming-relevant
   work have landed since the pinned HEAD.

---

*Generated by `/audit-performance` (deep, all 9 dimensions), as one leg of a
`comprehensive` audit-suite sweep alongside `/audit-ecs` and `/audit-renderer`
(both same-day, same HEAD `1ae86f62`). Dimension 7 (World Streaming) was
delegated to a foreground sub-agent per this session's tooling constraints
(background sub-agents are not retrievable in this harness); its findings
were reviewed and cross-checked by the primary agent before inclusion, not
taken on trust. Dedup checked against a 47-issue `gh issue list` pull. No
GitHub issues created — suggested next step:
`/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-08-03.md`.*
