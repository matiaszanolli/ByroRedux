**HEAD**: `f97775ca8` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md` (@ `b3db49fa`, 385 commits ago) · **Audited**: Dims 1–8 (every dimension's `Paths:` had commits since the baseline) · **Unchanged since baseline (skimmed)**: none at the dimension level

# Performance Audit — 2026-09-21

**Command**: `/audit-performance` (default scope, `--depth deep`), one leg of `/audit-suite --preset comprehensive`.
**Severity scale**: `.claude/commands/_audit-severity.md`.

## Method

- **Order.** One auditor ran the eight dimensions sequentially, with no sub-agents. Per-dimension notes
  are in `/tmp/audit/performance/dim_{1..8}.md`. The killed earlier run left only `gitlog_d{1..8}.txt`;
  I re-derived every range with `git log b3db49fa..HEAD -- <Paths>`.
- **Static audit.** No engine launch and no bench. Every frame-time figure below is either:
  - arithmetic from source (marked *est.*),
  - a number a commit message records, or
  - the checked-in bench-of-record TSV.
  The runtime audit owns captures; each finding says what a capture should confirm.
- **Commands run** (all `cargo test -j 4`, `TMPDIR=/mnt/data/tmp`; no workspace build):
  - `byroredux` bin: `draw_sort_key sort_key` (32 pass, 2 manual benches ignored);
    `static_blas_recovery skin_dispatch_ran_rollback bone_palette_overflow` (6 pass);
    `streaming unload despawn_batch pre_parse work_budget interior_cell_apply` (78 pass).
  - `byroredux-renderer` lib: `acceleration static_blas scratch` (134 pass); the Dim 4 set (47 + 3 pass);
    the Dim 6 skinning set (42 pass).
  - `byroredux-nif --features dhat-heap`: the three `heap_allocation_bounds*` tests (4 + 1 + 1 pass).
  - `scripts/check-bench-harness-provenance.sh --strict 4c9a5b36`: "harness byte-stable since this
    record — a re-run is a valid apples-to-apples comparison against it" (exit 0).
- **Dedup sources:**
  - `/tmp/audit/issues.json` (4,458 issues, 94 open), plus `gh issue view 3429`.
  - `docs/audits/`, including the four sibling reports written today (`AUDIT_RENDERER`, `AUDIT_SAFETY`,
    `AUDIT_ECS`, `AUDIT_CONCURRENCY_2026-09-21.md`) and `AUDIT_NIFAL` / `AUDIT_TECH_DEBT_2026-09-21.md`.

## Executive Summary

**NEW + regression: 0 CRITICAL · 0 HIGH · 3 MEDIUM · 10 LOW** (13 findings; no regressions).
**Existing open issues re-confirmed (not re-filed): 3** — #4208, #4209, #3813.

| Dimension | Findings |
|---|---|
| 1 — CPU Hot Paths | 2 MEDIUM, 4 LOW |
| 2 — Draw & Instancing | none (all guards pass) |
| 3 — GPU Memory Pressure | none (#4196's admission fix traced sound) |
| 4 — SSBO Sizing & Upload | 1 LOW |
| 5 — GPU Pipeline | 1 MEDIUM (pre-existing, never costed) |
| 6 — Skinning & BLAS | 1 LOW |
| 7 — Streaming & Parse | 2 LOW |
| 8 — Telemetry & Origin | 2 LOW |

**Observed vs ROADMAP.** Not measured. The live bench-of-record is the stepped-camera refresh at
`4c9a5b36` (2026-09-09), now 449 commits stale (R6a-stale-22). The harness pair is byte-stable
(provenance verdict above), so a re-run is a valid comparison. No FPS delta is claimed here.

**Headlines.**
- **The top-of-frame wait on every frame-in-flight fence makes all of `draw_frame`'s CPU recording
  GPU-idle time (PERF-D5-2026-09-21-01).** At `MAX_FRAMES_IN_FLIGHT = 2`, frame N waits for frame N-1
  before recording anything. The engine therefore overlaps only pre-`draw_frame` work with the GPU.
  The code comment says "cost stays zero in practice". The bench-of-record contradicts it: the GPU-bound
  scenes spend most of each frame in that wait (Prospector TAA 10.59 of 13.08 ms).
  - This is pre-existing. The concurrency leg filed the *safety* side today (CONC-D1-2026-09-21-01: the
    argument is unpinned, and there are nine riders). Nothing has costed the throughput.
- **Two new per-frame CPU costs in the render driver since the baseline.**
  - The ground-cover residency ring rebuilds three std SipHash maps/sets plus about ten fresh Vecs every
    exterior frame, in `byroredux/src/render/`, which the #2923 rule keeps Fx end to end
    (PERF-D1-2026-09-21-01).
  - The MenuXml `--hud` re-rasterizes, copies and synchronously uploads the whole swapchain-sized frame
    on every changed tick, up to 30 Hz while the camera turns. Its budget was set at 720p
    (PERF-D1-2026-09-21-03).
- **The three newest commits are cheap per frame.** The exposure meter is one single-workgroup dispatch
  (fixed mode, the default, writes a constant). AgX adds one cached texel fetch per output pixel. The
  debug views add two uniform compares. Their defects are correctness or telemetry, and the sibling legs
  filed the correctness ones. The one performance item is that the meter has no GPU-timer bracket
  (PERF-D8-2026-09-21-01).
- **Guard posture: nothing eroded.**
  - All 20 closed findings from the 09-11 report hold at HEAD.
  - One fix carries no guard: #4206's commit claims heap-bound coverage its diff does not contain
    (PERF-D7-2026-09-21-02).

## Hot Path Analysis

Nothing here is re-measured. The RT quality tier is unknown, because no bench ran and `AdaptiveRayBudget`
decides it. Rows are ordered by estimated per-frame impact.

| Site | Per-frame cost | Class | Finding |
|---|---|---|---|
| All-slots fence wait (`sync_and_acquire_frame.rs:65`) | GPU idle = post-wait `draw_frame` CPU time (`acquire + tlas_build + ssbo_build + cmd_record` + unbucketed camera/skin recording). Bounded by `wall_ms − fence_ms`: Prospector TAA ≤ 2.49 ms of 13.08, Whiterun TAA ≤ 3.21 of 10.74, Dugout TAA ≤ 6.99 of 11.12 (bench-of-record medians) | CPU/GPU pipelining | PERF-D5-01 |
| MenuXml HUD refresh (`app_frame.rs:873-893`) | Per changed tick (≤ 30 Hz while yawing): full-frame clear + menu eval + raster, `.to_vec()` + staging copy of 8.3 MB @1080p / 14.7 MB @1440p, one-time submit + fence. Raster ~16 ms debug @720p per the code's own doc | periodic main-thread spike (opt-in `--hud`) | PERF-D1-03 |
| Ground-cover residency (`render/groundcover.rs:112-181,313`) | ~135 candidate chunks → ~800-950 SipHash ops + ~10 allocations per exterior frame, *est.* 20-40 µs | CPU churn, #2923 class | PERF-D1-01 |
| Unload capture passes (`unload.rs:245-246`) | Two O(victims) walks with a lock per probe per ring unload, *est.* sub-ms to ~1 ms per crossing; in no `UnloadPhaseTimings` bucket | streaming hitch | PERF-D7-01 |
| Ground-cover detail atlas + species table (`app_frame.rs:43-90`) | ~768 `powf` + ~6 allocations per frame, interiors too (*est.* 15-20 µs); ×32 at the species cap | CPU waste | PERF-D1-02 |
| Exposure meter (`post_passes.rs:294`) | Fixed mode: a few µs. Auto: 4,096 scattered fetches on one workgroup between two stage-wide barriers (*est.* tens of µs). Unbracketed | GPU pass | PERF-D8-01 |
| HUD objective snapshot (`objectives.rs:39-97`) | O(quests) scan + Strings + sort per frame (µs) | CPU churn | PERF-D1-04 |
| `plan_palette_dispatch`, combat systems, NPC KCC capsule | 2 Vecs / 2 Vecs + capacity churn / one `Arc` per walker per frame | CPU churn | PERF-D6-01, PERF-D1-05, PERF-D1-06 |
| Indirect-draw SSBO | 10.5 MB resident; > 99.9 % unused on every baseline | VRAM residency | PERF-D4-01 |

Measured in-commit (not bench-of-record):
- ground-cover 4× candidate budget: `gpu_main_render` 7.46 → 7.64-7.88 ms (Skyrim 2,-4, `673b21458`);
- BFECC / soot / multi-scatter volumetrics: `gpu_volumetrics` flat (0.318 → 0.319 ms; ~0.45 ms);
- SKYAL bake + filter: ~0.16 ms (one Skyrim capture, ROADMAP R6a-stale-22).

---

## Findings

### MEDIUM

#### PERF-D5-2026-09-21-01: The top-of-frame wait on every frame-in-flight fence idles the GPU for all of `draw_frame`'s CPU recording, on every GPU-bound frame
- **Severity**: MEDIUM (throughput; suboptimal pipelining, no correctness impact)
- **Dimension**: GPU Pipeline
- **Location**:
  - `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:40-68` — the wait
    `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)` is at `:65`; the premise comment is at `:59-60`.
  - `crates/renderer/src/vulkan/context/draw.rs:1812` — `draw_frame`'s first step. Recording follows at
    `:1821-2125`, then the single `queue_submit` at `:2197`.
  - `crates/renderer/src/vulkan/sync.rs:20-97` — the rider list.
- **Status**: NEW for the throughput cost. The mechanism predates the baseline (#282; kept as-is at N = 2 by #3442).
- **Description**:
  - With two frames in flight, the "other" slot's fence belongs to frame N-1. Waiting on it when
    `draw_frame` N begins drains the GPU queue.
  - From then until N's `queue_submit`, the GPU has nothing to run. The CPU meanwhile does:
    - swapchain acquire;
    - camera/light assembly;
    - skin and TLAS recording (`tlas_build`);
    - `build_and_upload_instances` (`ssbo_build`);
    - geometry and post-pass recording (`cmd_record`; `cmd_t0` starts at `draw.rs:1952`, disjoint from
      the other buckets);
    - the submit call itself.
  - Frame period is therefore about `max(T_pre, GPU) + T_post`, not `max(T_pre + T_post, GPU)`. The
    engine pipelines one frame at the `draw_frame` boundary, not two.
  - The comment at `:59-60` ("Cost stays zero in practice — the GPU is rarely more than 1 frame behind
    the CPU, so the other fences are almost always signaled") inverts the GPU-bound case. There the
    other fence is the frame the GPU is still executing.
- **Evidence**:
  - `docs/audits/BENCH_stepped-camera_4c9a5b36.tsv` medians put most of each GPU-bound frame in this wait:
    - Prospector TAA: `fence_ms` 10.59 of 13.08 ms wall;
    - Prospector FSR-Q: 7.39 of 9.58;
    - Whiterun TAA: 7.53 of 10.74;
    - MedTek TAA: 16.12 of 37.64.
  - `t.fence_wait_ns` is measured around exactly this call (`sync_and_acquire_frame.rs:61-68`).
- **Impact**:
  - GPU-bound scenes, which are the RT default, lose `T_post` every frame.
  - `T_post` is bounded above by `wall − fence`: 2.49 ms Prospector TAA, 3.21 ms Whiterun TAA. At ~1 ms of
    post-wait CPU that is ~8-10 % throughput.
  - It also turns every CPU cost inside `draw_frame` (instance build, recording) into frame time, even
    on GPU-bound frames.
- **Related**:
  - CONC-D1-2026-09-21-01 (`AUDIT_CONCURRENCY_2026-09-21.md`, MEDIUM) files the safety side: the
    all-slots argument is unpinned, and two unlisted riders take the count to nine. It mentions the lost
    overlap only as context.
  - #282, #870, #3442, #3643, #4516.
- **Suggested Fix**: Measure first: on a GPU-bound TAA bench, the `cpu_ms:` sum
  `acquire + tlas_build + ssbo_build + cmd_record` is the per-frame idle, and an Nsight Systems or
  RenderDoc timeline shows the queue gap. Then:
  - per-FIF or defer-destroy all nine riders;
  - cover the SVGF previous-slot G-buffer read (#282) with an in-command-buffer barrier (a barrier's
    first scope includes earlier submissions on the same queue);
  - only then wait on `in_flight[frame]` alone.

  Changing the wait first is exactly the nine-site UAF CONC-D1 warns about. The speculative-Vulkan rule
  applies: `BYRO_VALIDATION=1` + RenderDoc on both upscaler modes.
- **Confidence**: High on the mechanism (the fence semantics plus `draw_frame`'s order). Medium on the
  magnitude (post-wait share unmeasured).

#### PERF-D1-2026-09-21-01: Ground-cover host collection rebuilds std SipHash maps/sets and ~10 fresh allocations every exterior frame
- **Severity**: MEDIUM (redundant allocation and hashing on the per-frame render path; the #2923 rule class)
- **Dimension**: CPU Hot Paths
- **Location**:
  - `byroredux/src/render/groundcover.rs:14` (`use std::collections::{HashMap, HashSet}`);
  - `:112-181` (`GroundCoverResidency::reconcile`);
  - `:247`, `:268`, `:313`, `:418`;
  - caller `byroredux/src/app_frame.rs:369,727-779` (every frame the pipeline exists).
- **Status**: NEW (introduced by `6f2831d5c`, 2026-09-14, and `fd0cd577c`, 2026-09-15; at `b3db49fa` the
  file had no hash collections)
- **Description**:
  - `reconcile` allocates every frame:
    - `desired: HashMap<ChunkKey, ChunkCandidate>`;
    - `wanted: HashSet<ChunkKey>`, which is redundant with `desired.contains_key`;
    - `resident: HashSet<ChunkKey>`;
    - a `pending` Vec and the returned Vec.
  - `collect_groundcover_frame` adds fresh `resident` and `candidates` Vecs and a std
    `emitted: HashMap<usize, u32>`. `collect_groundcover_disturbers` adds a fresh `found` Vec.
  - All of these are SipHash, on `byroredux/src/render/`. `_audit-common.md` (#2923) says that path stays
    Fx end to end, and that a reintroduced std map there is the regression.
  - The comment at `:308-312` ("Survivors arrive in walk order … only the last emitted cell can match")
    has been stale since the ring. `reconcile` returns slot order, so `emitted` needs a map, just not a
    hashed one.
- **Evidence**:
  - Draw distance 3000 + chunk bound gives ~135 candidates per frame, at ~6 hash operations each.
  - `git show b3db49fa:byroredux/src/render/groundcover.rs | grep HashMap` finds only a test `HashSet`.
- **Impact**: *Est.* 20-40 µs of main-thread time on every exterior frame, from ~800-950 SipHash
  operations plus ~10 heap allocations. The #2923 guard pins only named `VulkanContext` fields, so it
  cannot see this. No quantitative guard exists for this site.
- **Related**: #2923, #3682 (same cluster). PERF-D1-2026-09-21-02 (same function family). PERF-D8-2026-09-21-02
  (these scratches have no telemetry rows).
- **Suggested Fix**:
  - Persistent `desired`, `pending` and result buffers on `GroundCoverResidency`, reused with
    clear + extend.
  - `FxHashMap`, or a grid-indexed Vec over the bounded 15×15 window.
  - Drop `wanted`.
  - Make `emitted` a `Vec<Option<u32>>` indexed by the dense `candidate.cell` ordinal.

#### PERF-D1-2026-09-21-03: The MenuXml HUD re-rasterizes, copies and synchronously uploads the whole swapchain-sized frame on every changed tick (≤ 30 Hz while the camera turns); its budget was set at 720p
- **Severity**: MEDIUM (a periodic main-thread spike on the playable HUD path; opt-in `--hud`)
- **Dimension**: CPU Hot Paths
- **Location**:
  - `byroredux/src/app_frame.rs:873-893` (`tick_hud_overlay`; `.map(<[u8]>::to_vec)` at `:886`);
  - `byroredux/src/hud.rs:423` (the overlay is sized to `ctx.swapchain_extent()`);
  - `hud.rs:553-627` (`render`), `:642-667` (`upload_frame`);
  - `crates/menuxml/src/menu.rs:342-356` (`render_frame`: full `frame.clear` plus a full
    `EvalState::resolve_all` every call);
  - `crates/renderer/src/texture_registry/mod.rs:899` → `crates/renderer/src/vulkan/texture.rs:137-200`
    (`overwrite_rgba_pixels` → `with_one_time_commands`, `:838-990`).
- **Status**: NEW (sibling of open #3429; `ffda4ea95`/`dc306a6a0`, 2026-09-18)
- **Description**:
  - A changed HUD tick happens on any camera yaw (heading is quantised to 0.1°), rate-limited to 33 ms.
    Each one does, on the main thread before `draw_frame`:
    - a clear, menu evaluation and raster at swapchain resolution;
    - `.to_vec()` of the whole RGBA frame into a fresh allocation;
    - a staging memcpy of the same size;
    - a one-time command buffer plus fence create, submit, wait and destroy.
  - The component's own docs size the policy at 720p:
    - raster "~16 ms" in the debug profile, which "pinned a machine" before the rate limit
      (`hud.rs:295-303`);
    - "a bounded ~3.5 MB/33 ms transfer budget";
    - the `.to_vec()` comment (`app_frame.rs:883-885`) prices the copy at 3.5 MB and calls it noise.
  - The buffer actually tracks the output extent: 8.3 MB at 1080p and 14.7 MB at 1440p (×2.25 and ×4).
  - The blocking fence adds less than it looks, because `draw_frame` already waits on every in-flight
    fence (PERF-D5-01). It is still redundant: the 3-texture rotation makes the in-place write race-free,
    so the copy could be recorded into the frame's own command buffer.
- **Evidence**: The code sites above. `write_rgba_inplace` uses `with_one_time_commands`, not the
  reuse-fence variant, so it also creates and destroys a fence per upload.
- **Impact**: While the player turns with `--hud` on, every second frame at 60 fps pays the raster plus
  about three full-frame memory passes. The cost scales with output resolution. The release-profile
  figure is unmeasured.
- **Related**:
  - #3429 (OPEN, MEDIUM) covers the Scaleform `update_rgba` path. `AUDIT_CONCURRENCY_2026-09-21.md`
    re-confirms it with the FO4/Skyrim `--hud` trigger and judges this rotation race-free.
  - #4515, #4516 and #4526 covered this rotation's contract, tripwire and ledger. None of them covers the
    per-refresh cost.
- **Suggested Fix**:
  - Raster only the damaged rects (bars, compass), or raster at a fixed HUD-native resolution and let the
    overlay quad scale it.
  - Remove the `.to_vec()` (split-borrow the renderer's frame, or swap an owned buffer).
  - Upload only the damaged rects, recorded into the frame command buffer.
- **Confidence**: High on the mechanism. Low on the release-build magnitude.

### LOW

#### PERF-D1-2026-09-21-02: The ground-cover detail atlas and species table are rebuilt every frame (interiors too) just to compare an unchanged signature
- **Location**:
  - `byroredux/src/app_frame.rs:43-90` (`publish_groundcover_detail_atlas`; runs every frame on every
    ray-query device, `context/init.rs:717`);
  - `byroredux/src/render/groundcover.rs:488-543`, `:656-739`.
- **Status**: NEW (`1a7a22cf0`, `fd0cd577c`)
- **Description**:
  - `build_groundcover_detail_atlas` allocates `16×16×species×4` bytes and runs `linear_to_srgb8` (a
    `powf`) three times per texel for every species.
  - The caller then compares `atlas.signature`, which hashes only `colour_gradient` bits plus the count,
    and discards the pixels when it is unchanged. That is the steady state.
  - The species table builds five Vecs plus a sort per frame, for a result that changes only with the
    palette or climate.
- **Impact**:
  - Today the palette is always one built-in species (`resolve_palette_for_chain` passes `Vec::new()`):
    ~768 `powf` + ~6 allocations per frame, *est.* 15-20 µs.
  - At the 32-species cap, once the authored tier (#4413) fills it, that grows to ~24.6K `powf` (~0.5 ms)
    per frame.
- **Suggested Fix**: Compute the signature from the palette in O(species) and build pixels only on a
  mismatch, or key both on a palette generation counter. Cache the species table under the same key.

#### PERF-D1-2026-09-21-04: The always-on HUD snapshot rebuilds the objective list every frame with no change key
- **Location**:
  - `byroredux/src/app_frame.rs:157-183` (the debug-UI-hidden branch);
  - `byroredux/src/objectives.rs:39-97`;
  - `byroredux/src/inventory.rs:197-221`.
- **Status**: NEW (`db39fe004`, 2026-09-20)
- **Description**: Each frame does:
  - an O(all quests) scan of `QuestStageState`;
  - a Vec per running quest with objectives;
  - per displayed objective, `quest_display_name` (`to_owned` / `format!`) and `flatten_text`
    (`replace` + `collect`);
  - a sort, a final collect, and a `vitals` Vec.

  The result changes only on quest, objective or actor-value events. In a bench window it is computed
  and then discarded (`app_frame.rs:195-200`).
- **Suggested Fix**: Cache the `Vec<ObjectiveView>` behind a generation counter bumped by
  `QuestStageState` / `QuestObjectiveState` mutation.

#### PERF-D1-2026-09-21-05: The P2 combat systems allocate per frame while combat is active
- **Location**: `byroredux/src/systems/combat_anim.rs:333`, `byroredux/src/systems/combat_ai.rs:57-60`
- **Status**: NEW (`ec3a18d2f`, `f61ea0447`)
- **Description**:
  - `let decisions = std::mem::take(&mut scratch.decisions);` is never restored. The closure-persistent
    scratch therefore regrows from 0 on every frame of an active hit or attack take (the `mem::take`
    churn pattern).
  - `npc_combat_ai_system` is a plain fn with fresh `decisions`/`steps` Vecs per frame.
- **Related**: CONC-D3-2026-09-21-02 covers the same two systems' guard lifetimes. It is a separate
  defect; fix both together.
- **Suggested Fix**: Iterate by index, or put the Vec back after the sound loop. Convert `combat_ai` to a
  factory with a persistent scratch.

#### PERF-D1-2026-09-21-06: `move_character` heap-allocates the capsule shape on every call; M42.10 made that per walking NPC per tick
- **Location**: `crates/physics/src/world.rs:1325` (`SharedShape::capsule_y`, an `Arc<dyn Shape>`); caller
  `byroredux/src/systems/locomotion.rs:175` (every walker in the six procedures, plus `combat_ai`)
- **Status**: NEW (the multiplier arrived with `913fd39d8` / M42.10)
- **Suggested Fix**: Pass a stack `Capsule::new_y(..)` as `&dyn Shape` to `move_shape`. The owner of that
  code is the physics audit; the multiplier sits in this dimension's caller.

#### PERF-D4-2026-09-21-01: The indirect-draw SSBO stayed eagerly sized at `MAX_INSTANCES` after #4199
- **Location**:
  - `crates/renderer/src/vulkan/scene_buffer/constants.rs:220` (`MAX_INDIRECT_DRAWS = MAX_INSTANCES`);
  - `scene_buffer/buffers.rs:547`;
  - `docs/engine/memory-budget.md:96`.
- **Status**: NEW
- **Description**:
  - The buffer is 262,144 × 20 B × 2 FIF = 10.5 MB resident. It holds one command per raster batch, so
    its fill is bounded by raster draws.
  - The baselines peak at 196 batches out of at most 283 raster commands, so more than 99.9 % is never
    written. Upload is already O(live) and hash-gated; the waste is residency only.
- **Suggested Fix**: Ride `ensure_instance_capacity`'s grow path, or record the allocation as deliberate
  in `memory-budget.md`.

#### PERF-D6-2026-09-21-01: `plan_palette_dispatch` allocates two fresh Vecs on every frame with a dirty skinned slot
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs:884-920`; caller
  `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs:183-190`
- **Status**: NEW (`eb7c82043`, the #4204 fix)
- **Description**: The function allocates the `ranges` Vec and the returned `runs` Vec. In a populated
  cell, idle animations change the pose hash, so this happens every frame. Neighbouring per-frame
  renderer buffers are persistent fields (the #243 convention, which #4193 applied to
  `build_instance_map`).
- **Suggested Fix**: Make both persistent fields, or have the planner write into a caller-owned
  `&mut Vec`.

#### PERF-D7-2026-09-21-01: Every cell unload now runs two per-victim capture passes that probe storages one lock at a time, and neither is inside any `UnloadPhaseTimings` bucket
- **Location**:
  - `byroredux/src/cell_loader/unload.rs:245-246`;
  - `byroredux/src/cell_loader/reference_state.rs:96-166` (`capture`, `d8255b2e2`, 2026-09-16);
  - sibling `byroredux/src/cell_loader/stream_snapshot.rs:181-214` (#3299).
- **Status**: NEW
- **Description**:
  - `victims` is every entity of the cell. `capture` calls `world.get::<FormIdComponent>` for each one:
    a TypeId lookup plus a tracked `RwLock` read per call.
  - For placement roots it also does a `FormIdPool` resolve plus `Inventory`, `Dead` and `PickedUp`
    probes before it can skip.
  - The sibling pass does the same walk. A radius-3 crossing unloads a 7-cell ring in one batch.
  - Both calls sit between `timings.ownership_index` (`:238`) and the next `phase_started` (`:255`), so
    their cost is attributed to no phase.
- **Impact**: *Est.* sub-ms to ~1 ms of main-thread time per boundary crossing (unmeasured), invisible
  in the unload phase split.
- **Suggested Fix**: Acquire each pass's queries once, or merge the two passes into one walk over
  placement roots. Time them as a `snapshot_capture` phase.

#### PERF-D7-2026-09-21-02: The #4206 tangent pre-size has no regression guard, although its commit says it added heap-bound coverage
- **Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:1145-1157`; fixture
  `crates/nif/tests/heap_allocation_bounds.rs:252-297` (still says "no `VF_TANGENTS`")
- **Status**: NEW (test gap on closed #4206)
- **Description**:
  - `ab8779a52`'s message says the fix "gives the path the heap-bound coverage the dhat fixture skipped".
  - The commit touches only `bs_tri_shape.rs` (+13/−1) and adds no fixture, unit test or pin.
  - A revert to `Vec::new()` stays green; the dhat suite passes either way.
  - PERF-D8-2026-09-11-01 asked for this fixture explicitly.
- **Suggested Fix**: Add a `VF_TANGENTS | VF_NORMALS` variant of `bs_tri_shape_block_with_vertices` to the
  dhat bound, or a unit test asserting `tangents.capacity() == num_vertices`.

#### PERF-D8-2026-09-21-01: The exposure meter is a per-frame GPU pass with no timer bracket
- **Location**:
  - `crates/renderer/src/vulkan/context/post_passes.rs:290-294`, `:1030-1051`;
  - `crates/renderer/src/vulkan/gpu_timers.rs:1-49` (19 brackets, none for the meter);
  - `gpu_timers.rs:39-40` / `:226` (the presentation label "exposure + ACES" predates the ACES|AgX switch
    and the meter-produced exposure texel).
- **Status**: NEW (`c5663fe39`)
- **Description**:
  - The dispatch and its two stage-wide barriers sit after bloom's end timestamp and before TAA's or
    upscale's start. Its GPU time falls in no bracket, the #3676 / #4315 class.
  - The renderer report lists "the exposure meter has no GPU-timer bracket" only under stale skill
    premises, not as a finding.
  - The `memory-budget.md` / `shader-pipeline.md` gaps for this pass are REN-D4-2026-09-21-01.
- **Suggested Fix**: Add `BIT_EXPOSURE_METER` (38 → 40 queries) around the dispatch and its barriers,
  plumbed through `GpuTimerSnapshot`, the metrics map and the `bench:` line (appended last). Update the
  presentation label.

#### PERF-D8-2026-09-21-02: Six persistent per-frame scratches have no `ScratchTelemetry` row, and the coverage guard pins a fixed name list
- **Location**:
  - `crates/renderer/src/vulkan/context/telemetry.rs:44-58` (the maintenance rule);
  - `context/mod.rs:269-283` (`ScratchBuffers`);
  - `byroredux/src/app_events.rs:700-770`;
  - `byroredux/src/main.rs:532-551`.
- **Status**: NEW
- **Description**:
  - No row exists for the renderer's `instance_map_scratch` (#4193, `fd9d000b3`).
  - No row exists for the App-owned `groundcover_cells`, `groundcover_chunks`, `groundcover_species`,
    `groundcover_species_table` and `groundcover_disturbers`.
  - `fill_scratch_telemetry_covers_all_four_previously_missing_scratches` (`context/mod.rs:2044`) names
    only the four scratches from #3693. Same class as #3693 / #3694 (both LOW).
- **Suggested Fix**: Add the six rows. Make the guard enumerate the `ScratchBuffers` fields from source,
  in the #4050 `sweep_purges_every_entity_keyed_collection` idiom.

### Regression-guard verification (no eroded guards)

| Dim | Guards checked | Result |
|---|---|---|
| 1 | `drain_dirty_into` (take_dirty test-only), animation/billboard scratches, propagation fast path, debug-UI gate, `SkinSlotPool` contraction + #4050 purge scan, `bone_world` no-clear, `SceneEffectSoftCache` | intact |
| 2 | 12-tuple key (#4191 slot 3 = 0 on opaque/additive, #4192 `blend_pipeline_slot`, #3663 bucket), raster-prefix sort @3000, split predicate, #1377 order, #4193 scratch; 32 tests | pass |
| 3 | 134 acceleration/static-BLAS/scratch tests, between-frames recovery, `DEFAULT_COUNTDOWN` = FIF, reserve floors, half-eviction, NIF LRU, 128 MiB sub-batched texture flush | pass |
| 4 | `GpuInstance` 160 B + no re-expansion, hash-gate tests, grow path invalidates both slot hashes, O(live) uploads, one-build-one-hash material intern | pass |
| 5 | `ENABLE_LEGACY_WRS 0`, CPU `inv_vp`, depth-history gate, pipeline-cache persist on variant compile, sky-only cloud march | intact |
| 6 | dirty palette plan + late bind-inverse re-arm, pipeline bound once, FX-hashed skinned BLAS, refit 600 + 60 jitter, rollback scope; 42 tests | pass |
| 7 | 78 streaming/unload tests, dhat bounds 4+1+1, worker-side import, budgeted interior cursor, CDB probe-only | pass (#4206 unguarded → PERF-D7-02) |
| 8 | 38 queries = table, origin correction on grid crossings, single-pass rebase, BLAS restore inside `atw_post` | intact |

### Prior report (2026-09-11) — all published and resolved except two

- **Fixed and verified at HEAD (20):**
  - #4189, #4190 (D1);
  - #4191, #4192, #4193, #4194 (production sort kept, `1127d1e25`), #4195 (all five TSVs consistent) (D2);
  - #4196 (admission traced: the overshoot is now at most ~one per-REFR batch), #4197, #4198 (D3);
  - #4199, #4200 (ledger ≈ 155 MB re-derived), #4201 (D4);
  - #4202, #4203 (documented, `cd29529dc`) (D5);
  - #4204, #4205 (D6);
  - #4206 (fixed but unguarded → PERF-D7-02), #4207 (D7; see note);
  - #4210 (D8).
- **Still open, re-confirmed:** #4208 (`systems/debug.rs:120`, `resources/mod.rs:855-862`) and #4209
  (`app_events.rs:1321,1346` precede the `atw_*` write at `:1399-1401`).
- **#4207 residual (not filed):** the memo's `Insert(key)` arm still clones the key for every duplicate
  REFR of an uncached model: one allocation instead of two or three.

### Existing open issues re-confirmed (not re-filed)
- **#4208** (MEDIUM), **#4209** (LOW): see above.
- **#3813** (LOW): per-plugin ESM parse is still serial (`esm/records/parse.rs:113`, reshaped by a file
  split only).

### Cross-audit citations (not re-reported)
- **REN-D1-2026-09-21-01** (HIGH): blended FX cards became shadow blockers in `f97775ca8`; also the
  capture item below.
- **REN-D11-01/02/03**: AgX clamp, 2τ adaptation, meter lacks the raw-debug gate.
- **REN-D4-01**: meter missing from `shader-pipeline.md` / `memory-budget.md`.
- **SAFE-D7-2026-09-21-01**: the meter divides by one thread's sample count.
- **SAFE-D2-2026-09-21-01**: three staging-pool `release_to` sites record `allocation.size()` (Dim 3 path).
- **CONC-D1-2026-09-21-01**: the all-slots wait's argument is unpinned; nine riders (see PERF-D5-01).
- **CONC-D3-2026-09-21-02**: combat-system guard lifetimes (see PERF-D1-05).
- **CONC-D2-2026-09-21-01**: ground-cover readback memory dependency.
- **#3429**: Scaleform HUD `update_rgba`, trigger widened.
- **TD1-2026-09-21-01**: the renderer-side `groundcover.rs` has crossed 2000 production LOC (a different
  file from PERF-D1-01's).

## Prioritized Fix Order

**Tier 1 — quick, local wins (scratch reuse, hash choice, caching, telemetry):**
1. PERF-D1-01 — `FxHashMap` + persistent scratch in the ground-cover residency ring. Cheapest large one;
   it runs on every exterior frame.
2. PERF-D1-02 — signature-first atlas and cached species table. Do it before #4413 multiplies the cost.
3. PERF-D8-01 — bracket the exposure meter, so auto-exposure and AgX are measurable before either
   becomes a default.
4. PERF-D8-02, PERF-D7-02 — restore observability and the missing #4206 guard.
5. PERF-D7-01 — hoist queries / merge the unload capture passes and time them.
6. PERF-D6-01, PERF-D1-05, PERF-D1-06, PERF-D1-04 — per-frame allocation hygiene.

**Tier 2 — architectural; measure first:**
7. PERF-D5-01 — measure `T_post` on a GPU-bound bench. If it is ~1 ms or more, retire the nine riders
   and move to a per-slot wait. Coordinate with CONC-D1-2026-09-21-01.
8. PERF-D1-03 — damaged-rect HUD raster/upload recorded into the frame command buffer. Pairs with #3429's
   in-frame upload restructure.
9. PERF-D4-01 — indirect SSBO onto the grow path, or document it.

## Capture items for the runtime audit (static findings to confirm)
- PERF-D5-01: on Prospector/Whiterun TAA, log `cpu_ms:` `acquire + tlas_build + ssbo_build + cmd_record`
  (the per-frame GPU idle); ideally take a GPU timeline trace.
- PERF-D1-03: `rof_pre_draw` on alternate frames with `--hud` while yawing vs. still, at 720p and 1440p.
- `f97775ca8` (REN-D1-01) and `d54382415` (`STATIC_PROP` in the conservative light mask), plus FO3/FNV
  lights that now get omnidirectional shadow visibility: take a `main_render_ms` A/B on Prospector at a
  pinned `--rt-test-ray-quality-tier`.
- NPC locomotion: every walking NPC in every loaded cell does a Rapier KCC sweep per tick, serially, with
  no distance tier (M42.10). Read the scheduler timings on a populated exterior.
- Water caustic visibility ray per deposit (`1375abf53`): `main_render_ms` on an exterior water scene.
- Sky-cube bake + prefilter + SH in interiors: `sky_cube_ms` (known premise; quantify before gating).
- The exterior-grid batch count, for PERF-D4-01's sizing.

## Stale skill premises (for the next `/audit-performance` sync)
1. **Pass inventory.** The table lacks the **exposure meter** (`record_exposure_meter_pass`, between bloom
   and TAA, one workgroup, two barriers, no timer). Presentation is now
   `tonemap(graded * exposureTex)` with an ACES|AgX switch, not "exposure + ACES". The bracket count
   (19) needs a 20th once PERF-D8-01 lands.
2. **Dim 5 / new checklist item.** "The top-of-frame all-slots fence wait (`sync_and_acquire_frame.rs`)
   makes `draw_frame`'s post-wait CPU work GPU-idle time; quantify `T_post` (PERF-D5-01) — the comment's
   'cost stays zero' premise is false on GPU-bound frames."
3. **Dim 1 Paths** should add:
   - `byroredux/src/render/groundcover.rs`;
   - `byroredux/src/systems/combat_anim.rs`;
   - `byroredux/src/app_frame.rs`'s ground-cover and HUD collection;
   - `byroredux/src/hud.rs`;
   - `byroredux/src/objectives.rs`.
4. **`_audit-common.md` #2923 guard.** The `must stay FxHashSet (#2923)` assertion
   (`context/mod.rs:1884-1912`) pins named fields only. It could not see PERF-D1-01. A source scan of
   `byroredux/src/render/` for `std::collections::Hash` would enforce the rule as worded.
5. **Dim 7 Paths** should add `cell_loader/reference_state.rs` and `cell_loader/stream_snapshot.rs` (the
   unload capture passes). "`UnloadPhaseTimings` is wired only on the batch path" should also note that
   the capture passes sit in no phase.
6. **Dim 4.** The ledger total is now ≈ 155 MB at starting capacity (≈ 243 MB grown). `MAX_INDIRECT_DRAWS`
   is still eager at `MAX_INSTANCES` (PERF-D4-01).

## Process notes
- Scratch notes for all eight dimensions are in `/tmp/audit/performance/dim_{1..8}.md`, with test logs
  `test_d2_sortkey.log`, `test_d3_accel.log` and `test_d7_dhat.log`. The skill's Phase 4
  `rm -rf /tmp/audit/performance` is left to the suite orchestrator.
- Tests ran only through `cargo test`; no `target/*/deps` binary was invoked directly, and no engine
  process was launched.
- Nothing besides this report was written into the repo.

Publish with: `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-09-21.md`. Labels:
- `performance` on every finding;
- `renderer` on D5-01, D4-01, D6-01, D8-01, D8-02;
- `vulkan` + `sync` on D5-01;
- `ui` on D1-03;
- `terrain-exterior` on D1-01 and D1-02;
- `test-gap` on D7-02 and D8-02;
- `nif-parser` on D7-02;
- `physics` on D1-06;
- `combat` on D1-05.
