# Performance Audit — 2026-07-16

**Scope**: Full 9-dimension sweep (`--depth deep`, no `--focus` restriction).
**Method**: Static analysis / code reading across 9 parallel dimension passes
(max 3 concurrent per the skill's orchestrator architecture). No engine run —
all findings are sourced from reading current code, existing tests, and
shipped `triangle.frag.spv` symbol inspection; no wall-clock bench was
collected this pass.

## Executive Summary

The engine's performance posture is **mature and healthy**. Across all 9
dimensions, every regression guard from Session 46 (#1371-#1379), the R1
material/GpuInstance work, the M29.x skinning/BLAS series (#1194-#1197,
#1791/#1796/#1797/#1811/#1812), the #1799 legacy-WRS compile gate, and the
#1792 mid-batch BLAS eviction fix was **re-derived independently from current
source and found intact** — zero erosions. Dimension 2 (draw-call/instancing)
had already been swept yesterday (2026-07-15) and both its LOW findings were
filed and fixed same-day (#1994, commit `56019cdf`); today's pass re-confirmed
health and found nothing new.

**15 findings total: 0 CRITICAL, 0 HIGH, 4 MEDIUM, 11 LOW.** No finding rises
above MEDIUM — nothing here is an urgent regression. The four MEDIUM findings
are all real, quantifiable inefficiencies with bounded blast radius:
- A bindless texture-descriptor slot leak on cell revisit (Dim 3) — not a VRAM
  leak (GPU memory is reclaimed), but a monotonic leak of the finite
  descriptor-slot *index* space, degrading to checkerboard-fallback after
  enough cumulative cell transitions in one process lifetime.
- Partial-dirty-frame bone-palette/bone-world re-upload cost in the skinning
  pipeline (Dim 6) — a documented residual of already-closed #1794, not a new
  regression.
- 14x redundant AI-package resolution walks per NPC spawn (Dim 7), compounding
  the still-open #1798 interior-spawn-cost gap.
- A missed migration in Starfield's `BSGeometryMeshData` skin-weight parse
  (Dim 8) that never got converted to the bulk `read_pod_vec` path despite its
  own doc comment saying it should be.

**Bench-of-record**: No bench was run this pass (static analysis only, per
methodology). ROADMAP.md's bench-of-record (`R6a-stale-14`, HEAD `1c26bc25`,
2026-06-03) was already flagged 613 commits stale as of the Session 56
closeout (2026-07-15) and has grown one session staler since; no perf-audit
finding in this report changes that staleness assessment or supplies a fresh
number. R6a-stale-15 (a fresh 300-frame three-scene GPU bench) still gates any
current FPS claim.

## Hot Path Analysis

Not applicable in the traditional GPU-timer-table sense — this was a static
pass, and Dimension 9 (the dimension that would source `gpu_timers`/
`ScratchTelemetry` live numbers) confirmed the instrumentation itself is
healthy but did not capture a live snapshot. Positive structural findings
worth recording as the closest analogue to a hot-path table:

| Path | Verified property |
|---|---|
| GPU timer readback (`gpu_timers.rs`) | Fence-guaranteed prior-cycle read; `active_bits`-gated WAIT is a host no-op, never blocks the current frame |
| Per-instance camera-relative rebase (`draw.rs` build loop) | Single-pass, folded into the existing `gpu_instances.push`; 3 subtractions/instance, no second O(instances) pass |
| TAA/SVGF history across cell-boundary origin snap | Preserved via one `translation(O2-O1)` matrix multiply in `origin_corrected_prev_view_proj`; hot-path early-return on the common (non-crossing) frame |
| Volumetrics inject/integrate | O(froxels), 160×90×128 fixed grid, does not scale with mesh count |
| Bloom pyramid | O(pixels), 5 down + 4 up mips |
| TLAS build vs refit | Refits dominate; barrier from AS-build to shader-read confirmed present and correctly placed |
| Legacy WRS (16-reservoir) shader path | Confirmed compiled OUT of the shipped `triangle.frag.spv` (`ENABLE_LEGACY_WRS=0`); default path streams O(lights), not O(lights×16) |
| Skinned BLAS refit / dispatch-dirty gates | First-sight invariant + pose-hash dirty gate confirmed live; early-return rollback (#1791/#1796) confirmed ordering-correct |
| SSBO per-frame uploads (instances/materials/indirect) | All O(live data), content-hash dirty-gated; only `upload_lights` lacks the sibling gate (LOW finding) |
| `pre_parse_cell` two-phase streaming pipeline | Serial header-extract → parallel/serial body-parse split (#877) intact; small-model serial fast-path (#1262) intact |

## Regression Guards Verified Intact (not re-proposed)

All of the following were independently re-derived from current source across
the 9 dimension passes and found **un-eroded**:

- **Dim 1**: #1371 (`drain_dirty_into` capacity preservation), #1372/#1725
  (animation system persistent scratch), #1374 (billboard camera-static skip),
  #1376 (debug-UI snapshot clone gated on visibility), #1379 (bone-pool
  `next_slot` contraction), #1794 (`bone_world` steady-state reuse — CPU-side
  half only, see DIM6-01), #1803 (dead `GlobalTransform` probe removed from
  `emit_particles`), #1802 (`BYRO_PROFILE` cached via `OnceLock`).
- **Dim 2**: #1377/#1805 (GT-presence hoist), #1804 (two-sided blend split
  `z_write` gate). Yesterday's DIM2-01/DIM2-02 (LOW) both fixed same-day in
  `56019cdf` / #1994 — confirmed resolved, not re-proposed.
- **Dim 3**: dynamic BLAS budget floor, #1792 mid-batch `pending_bytes` fold,
  64-build eviction check interval, LRU-by-oldest-tick, scratch/TLAS shrink
  reserve floors, `MeshRegistry` soft/hard pool caps, #1430 BGSM/BGEM
  half-eviction, `NifImportRegistry` 2048-entry LRU, deferred-destroy N+2
  countdown. #1793 (documented-not-fixed BLAS gaps) confirmed still
  as-documented, not re-reported.
- **Dim 4**: `GpuInstance` 112 B / std430 / no re-expansion (all three pinning
  tests read and confirmed present); PBR resolved once at import
  (`classify_pbr_keyword` has zero draw-loop call sites); material dedup
  O(1)-amortized intern + O(unique) upload. #1794/#1808/#1809 all confirmed
  CLOSED with fixes present.
- **Dim 5**: #1799 legacy-WRS compile-time gate (confirmed via shipped-SPIR-V
  `strings` inspection — no `resLight`/`resWSel` symbols present); CPU-side
  `inv_vp` precompute (no shader `inverse()`); TLAS build→read AS barrier
  present; TLAS refit-vs-rebuild logic correct; PBR lobe gating confirmed.
- **Dim 6**: all 7 required guards (dedicated palette compute pass,
  dispatch-dirty gate + first-sight invariant, BLAS refit gate + FAST_BUILD +
  600-frame rebuild ceiling, descriptor-rewrite skip, #1194 instrumentation,
  #1791/#1796 early-return rollback ordering, #1797 shared-scratch ceiling
  deferred-not-fixed) verified intact.
- **Dim 7**: two-phase `pre_parse_cell` (#877), small-model serial fast-path
  (#1262), NIF import cache LRU + #1854 ordering fix, shutdown drain,
  exterior per-frame spawn budget, CSG/CDB session-lifetime caching,
  `npc_spawn_wall` telemetry (#1798).
- **Dim 8**: bulk `read_pod_vec` routing (#833), `bytemuck` confirmed NOT a
  workspace dependency (correcting a stale claim from a past audit),
  `allocate_vec`'s `#[must_use]` gate (#831), #832 counter `get_mut`/`insert`
  split, import-time-only typed particle emitter extraction, `pre_parse_cell`
  split (cross-confirms Dim 7).
- **Dim 9**: no per-frame GPU-timer stall, single-pass per-instance origin
  rebase, TAA/SVGF history preservation across cell-boundary crossings
  (#1489), `ScratchTelemetry` row-Vec reuse (no per-frame realloc).

## Findings

### MEDIUM

#### MEM-D3-01: Bindless texture slots leak on every cell revisit
- **Severity**: MEDIUM (HIGH-adjacent on GPUs with a smaller
  `maxPerStageDescriptorUpdateAfterBindSampledImages` limit, or very long
  streaming sessions)
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/texture_registry.rs:452-459,1024-1054,1063-1083`;
  `check_slot_available` `:221-229`; drop call site `byroredux/src/cell_loader/unload.rs:170`
- **Status**: NEW
- **Description**: `TextureRegistry` is strictly grow-only — every
  registration takes `self.textures.len()` as a fresh index. On cell unload,
  `drop_texture` purges the `path_map` entry once `ref_count` hits 0, so
  re-entering a previously-unloaded cell re-registers its textures as **new**
  slots rather than hitting the dedup cache. GPU image memory *is* reclaimed
  via deferred destroy — this is not a VRAM leak — but the finite
  bindless-array slot index (ceiling: `min(maxPerStageDescriptorUpdateAfterBindSampledImages, 65535)`)
  is never reclaimed. `check_slot_available` gates on cumulative `textures.len()`,
  including dead slots.
- **Impact**: Slow-motion, session-length exhaustion. At ~150 unique
  textures/cell, ~430 cell transitions exhausts the 65535 ceiling on the dev
  card (fewer on constrained devices). Degrades gracefully to a checkerboard
  fallback — no crash/corruption — but the degradation is total and permanent
  until process restart, with no telemetry surfacing the slot high-water mark.
- **Related**: #372 (handle-stability rationale for the grow-only design);
  mesh-registry analog carries the same shape but a practically-unreachable
  16M-slot ceiling (see MEM-D3-02).
- **Suggested Fix**: Add a generational free-list (recycle a dropped index
  after a deferred-destroy fence proves no live `GpuInstance.texture_index`
  references it), or track `live_count` separately and gate
  `check_slot_available` on it plus a periodic compaction pass at cell-unload
  boundaries. At minimum, surface the slot high-water in `ctx.scratch` /
  telemetry and document the ceiling in `docs/engine/memory-budget.md`.

#### DIM6-01: `bone_world` upload + palette dispatch are not per-slot dirty-gated
- **Severity**: MEDIUM
- **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:197-226`
  (`upload_bone_worlds`), `:235-272` (`record_bone_world_copy`),
  `crates/renderer/src/vulkan/context/draw.rs:2850-2874,2913-2949`
- **Status**: Existing: #1794 (CLOSED; this is the residual explicitly
  documented as deferred in that closure, not a fresh regression)
- **Description**: The `pose_dirty` set gates only the per-entity
  `skin_vertices` dispatch and the skinned-BLAS refit. The upstream
  `bone_world` staging memcpy + `cmd_copy_buffer` and the `skin_palette.comp`
  dispatch are gated only by the whole-chain clean-streak gate
  (`skip_skin_gpu_refresh`, #1811), which only engages when **every** skinned
  pose has been static for `MAX_FRAMES_IN_FLIGHT + 1` frames. On any
  partial-dirty frame — one moving actor in an otherwise-idle crowd — the full
  `(max_used_slot+1) × MAX_BONES_PER_MESH` range is re-uploaded and
  recomputed, not just the dirty slots.
- **Impact**: Host-write + flush + PCIe transfer scales with *total resident
  skinned slots*, not *moving* actors. At the cited ~260-entity FNV workload
  this is ~2.4 MB/frame (~144 MB/s at 60 fps) on essentially every gameplay
  frame in a populated cell, since the all-clean gate rarely fires when any
  NPC is animating. GPU palette compute itself is cheap; the transfer is the
  real cost. No correctness impact.
- **Related**: #1794 (padding-tail angle, CLOSED — this is its documented
  "remaining two-thirds"), #1811 (all-clean gate), #1195/#1196 (the per-entity
  gates this path bypasses).
- **Suggested Fix**: Build per-slot `vk::BufferCopy` regions covering only
  `pose_dirty` slots' real bone-count prefixes, or move to a persistent
  incrementally-written device-side `bone_world` SSBO (mirroring
  `bind_inverses`). Requires plumbing per-slot bone counts across the
  byroredux→renderer `FrameInputs` boundary, which doesn't exist yet.
  Quantify via the existing #1194 `skin.coverage` brackets on a moving-crowd
  bench before spending the risk budget.

#### PERF-D7-01: NPC spawn re-resolves the same active AI package 14 times
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/npc_spawn.rs:1443-1626` (fourteen
  `active_package_is_*`/`active_*_location`/`active_*_target` call sites),
  `crates/plugin/src/esm/records/misc/ai.rs:288-296` (`active_package`, the
  shared walk every call re-invokes from scratch)
- **Status**: NEW
- **Description**: Since M42.2-M42.8 landed, `spawn_npc_entity`'s tail
  independently calls a package-is-X check plus a location/target getter for
  each of the seven procedures (Sandbox/Wander/Travel/Follow/Escort/Guard/
  Patrol) — 14 calls total, each re-running `active_package()`'s
  `find()`-with-CTDA-evaluation walk over `npc.ai_packages` from scratch. An
  NPC's active package is a single winning `PackRecord` by construction (an
  invariant independently confirmed by `AUDIT_ECS_2026-07-16.md`), so all 14
  calls converge on the same answer — the walk-and-CTDA-evaluate work runs up
  to 14x more than necessary, and `condition_met` is not cheap (closes over
  the M47.1 scripting evaluator per CTDA entry on every rejected package).
- **Impact**: Bounded by `ai_packages.len()` (small on vanilla NPCs) × CTDA
  length × 14. Already fully captured inside the existing `npc_spawn_wall`
  metric (#1798) — this finding directly compounds that still-open gap (no
  per-frame interior spawn throttle exists).
- **Related**: #1798 (the throttle/timing finding this compounds);
  `AUDIT_ECS_2026-07-16.md`'s mutual-exclusivity finding (confirms the premise
  that all 14 calls converge on one package).
- **Suggested Fix**: Resolve `active_package(...)` once at the top of the
  spawn tail, then match its `procedure_type` against the seven `PROCEDURE_*`
  constants to build the one `Behavior` component directly — collapses 14
  walks into 1. ~40-60 LOC, behavior-preserving, low risk.

#### D8-01: `BSGeometryMeshData::parse` skin-weight loop bypasses the bulk `read_pod_vec` path its own doc comment specifies
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/bs_geometry.rs:466-489` (loop),
  struct + doc comment at `:293-305`
- **Status**: NEW (predates and was simply never migrated by the #833/#873/
  #1589 sweeps, not a regression of any of them)
- **Description**: `BoneWeight` is `#[repr(C)]` + POD-marked
  (`unsafe impl AnyBitPattern`), and its own doc comment states the parser
  should bulk-read it via `read_pod_vec::<BoneWeight>`. The actual parse body
  instead does `allocate_vec` + a per-element loop of two `read_u16_le()`
  calls — the exact pre-#873/#1589 pattern those sweeps were written to
  eliminate. Two sibling reads 20 lines later in the same function
  (`meshlets`, `cull_data`) already use the correct bulk path; #1589 converted
  4 near-identical sites in the same cycle but missed this one.
- **Impact**: CPU throughput cost only (no correctness/memory-safety impact —
  `allocate_vec`'s budget guard still bounds the allocation). Every skinned
  Starfield `BSGeometry` mesh pays thousands of extra small reads/pushes
  instead of one bulk `read_exact`, on the cell-load / streaming-worker parse
  path. **Not caught by existing dhat `heap_allocation_bounds*` gates** — the
  loop performs zero extra allocations vs. the bulk path (capacity is
  pre-reserved either way), so an allocation-count assertion can't
  distinguish the two; a call-count or wall-clock benchmark is needed instead.
- **Related**: #833 (NIF-PERF-02), #873 (NIF-PERF-09), #1589 (fixed 4 sibling
  sites, missed this one).
- **Suggested Fix**: Replace the inner loop with
  `stream.read_pod_vec::<BoneWeight>(outer_len * weights_per_vert as usize)?`
  then `.chunks_exact(weights_per_vert).map(|c| c.to_vec()).collect()` —
  must read `outer_len * weights_per_vert`, not `n_total_weights`, to
  byte-for-byte preserve today's truncating-division stream-position
  behavior. Guard with a `criterion` wall-clock benchmark or a
  `#[cfg(test)]` call-count assertion, since dhat allocation bounds can't
  catch this class of regression.

### LOW

#### PERF-D1-2026-07-16-01: M42 AI-package systems allocate a fresh per-frame decision `Vec`
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/{wander,travel,follow,escort,guard,patrol}.rs`
  (one `Vec::new()` each) and `sandbox.rs:152,169,171` (two Vecs + a HashMap)
- **Status**: NEW
- Each of the seven M42 AI-package runtimes allocates
  `let mut decisions: Vec<Decision> = Vec::new()` fresh every frame instead of
  the closure-captured persistent-scratch pattern `make_animation_system`/
  `make_billboard_system` already use. **Opt-in only** — all seven are
  registered behind per-behavior env-var gates in `boot.rs:721-754`, never in
  the default scheduler, so this costs nothing in normal play. No dhat
  coverage exists for this class of render/ECS-adjacent site.
- **Suggested Fix**: Convert each to a `make_*_system()` factory capturing
  persistent scratch reused via `clear()`. Low priority given opt-in gating.

#### PERF-D1-2026-07-16-02: `collect_lights` recomputes `gi_priority_score` on both sides of every sort comparison
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/render/lights.rs:206-207`
- **Status**: NEW
- Compute-only (not allocation); point-light counts are small
  (streaming-RIS-capped, typically <50). Not worth a change unless a
  hundreds-of-lights cell ever materializes.

#### MEM-D3-02: Stale `MeshRegistry` doc comment claims freed-slot reuse that doesn't exist
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/mesh.rs:39-43` vs upload path `:295-314`, drop doc `:587-589`
- **Status**: NEW
- The `MAX_MESH_SLOTS` doc claims "re-uses freed slots via drop-and-push";
  the actual upload path is grow-only (same shape as MEM-D3-01, but with a
  practically-unreachable 16M-slot ceiling). Doc-only; contradicts the
  correct statement at the `drop_mesh` doc site 550 lines away.

#### PERF-D4-01: `upload_lights` is the one per-frame SSBO without a content-hash dirty gate
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:19-84`
- **Status**: NEW
- Instances (#1134), materials (#878), and indirect draws (#1809) all gained
  a hash-gate skip; lights did not. Blast radius is small (light buffers are
  a few KB/frame) and the gate would frequently miss anyway on
  flickering-torch content — a consistency/hardening gap, not a hot-path cost.

#### GPU-D5-01: Bloom `upload_params` rewrites construction-invariant UBOs every frame
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/bloom.rs:451-488`
- **Status**: NEW
- All 9 down/upsample param UBOs are pure functions of `self.extent`, fixed
  at construction, yet rewritten every frame. ~144 bytes of redundant host
  memcpy/frame; no extra barrier. Fix: write once at `BloomFrame::new` (and
  on resize, which already recreates the pipeline).

#### GPU-D5-INFO-01: Volumetrics casts ~1.8M per-froxel shadow rays/frame with no temporal reprojection yet
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:916-919`,
  `crates/renderer/shaders/volumetrics_inject.comp:177-189`
- **Status**: NEW (informational — a documented M55 Phase 5 roadmap gap, not
  a regression; **not actionable as a bug** today)
- O(froxels), correctly not scaling with mesh count, but paid in full every
  frame because temporal reprojection is an explicitly-future phase. Recorded
  for the record as the largest per-froxel cost lever once Phase 5 lands.

#### PERF-D7-02: Cell-transition orchestrator discards warm material/texture caches on every door transition
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/app_step.rs:255-298` (`step_cell_transition`),
  `byroredux/src/save_io.rs:610-614`
- **Status**: NEW
- `build_material_provider`/`build_texture_provider` are called fresh on
  every transition, discarding the BGSM/BGEM template cache, `csg_cache`, and
  `sf_cdbs`. Currently low-impact: `PendingCellTransition` is only queued by
  the `door.teleport` **console command** — interactive door activation
  (Stage 4) hasn't shipped yet. Will become a real per-door gameplay cost
  once it does. No urgency before then; worth a design note now.

#### PERF-D9-01: GPU timer brackets use `TOP_OF_PIPE` start, risking pass-cost misattribution
- **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:355-760`
- **Status**: NEW
- Diagnostic-accuracy-only: a bracket's reported ms can absorb queue-wait
  from prior in-flight work, so per-bracket timings are an upper bound and
  must not be summed to a "total GPU ms" without caveat.

#### PERF-D9-02: GPU timer readback issues up to 12 separate driver round-trips per frame
- **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:245-345`
- **Status**: NEW
- A single `get_query_pool_results(pool, 0, 24, ...)` batched read would
  replace up to 12 per-bracket calls. Minor host-side driver overhead only.

#### PERF-D9-03: `ScratchTelemetry` doc says "5 today"; producer now emits 9 rows
- **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/core/src/ecs/resources/mod.rs:421-426` vs
  `crates/renderer/src/vulkan/context/mod.rs:2785-2864`
- **Status**: NEW
- Pure doc-rot; the "bounded, reused Vec" invariant the comment documents
  still holds at runtime.

#### PERF-D9-04: Render origin is snapped twice per frame from independently-passed `camera_pos`
- **Dimension**: Telemetry & Origin Cost
- **Location**: `byroredux/src/render/camera.rs:160`,
  `crates/renderer/src/vulkan/context/draw.rs:2583-2584`
- **Status**: NEW
- No measurable CPU cost (a Vec3 floor/multiply, twice). The finding is
  fragility, not performance: the "both call sites must receive the same
  un-jittered `camera_pos`" invariant is enforced only by convention; a
  future refactor that jitters one path could desync the two origins by one
  cell-width at the boundary. Latent today, not active.

## Prioritized Fix Order

Nothing here is urgent — the sweep found a healthy, well-guarded engine.
Ordered by cheapest-fix-for-real-value first:

1. **PERF-D7-01** (MEDIUM, ~40-60 LOC, behavior-preserving) — collapse 14
   redundant AI-package resolution walks to 1 in `npc_spawn.rs`. Directly
   reduces the already-tracked #1798 interior-spawn cost.
2. **D8-01** (MEDIUM, small diff) — migrate `BSGeometryMeshData`'s skin-weight
   loop to `read_pod_vec`, matching its own doc comment and its two sibling
   reads in the same function. Needs a wall-clock/call-count guard, not a
   dhat bound.
3. **MEM-D3-01** (MEDIUM, moderate design work) — add slot reclamation or at
   minimum telemetry + documentation for the bindless texture-slot ceiling.
   No urgency on the 12 GB dev card at normal session lengths, but the
   failure mode (permanent-until-restart checkerboard fallback) is worth
   catching before a long-session playtest hits it blind.
4. **DIM6-01** (MEDIUM, needs `FrameInputs` plumbing) — per-slot bone-world
   copy/dispatch gating. Quantify via `skin.coverage` on a moving-crowd bench
   before spending the risk budget; this is real but the #1794 closure
   already scoped it as follow-up debt, not a surprise.
5. **GPU-D5-01** (LOW, trivial) — hoist bloom's invariant UBO writes to
   construction time. Safe filler for the next cleanup pass.
6. **PERF-D4-01, MEM-D3-02, PERF-D9-03** (LOW, doc/consistency) — fold into
   the next doc-rot bug-bash alongside DIM2-02's already-fixed precedent.
7. Everything else (PERF-D1-01/02, PERF-D7-02, PERF-D9-01/02/04,
   GPU-D5-INFO-01) — no action needed now; several are explicitly
   not-yet-actionable (Stage 4 door activation, M55 Phase 5 temporal
   volumetrics) and should be revisited when their gating milestone lands.

---
Full per-dimension detail (guard verification tables, evidence, disproof
attempts) is preserved in the 9 dimension traces this report was merged from.
