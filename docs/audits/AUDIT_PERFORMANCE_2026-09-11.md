# Performance Audit — 2026-09-11

- **Scope**: `/audit-performance` (no `--focus`, i.e. all 9 dimensions), `--depth deep`.
- **Orchestration**: 9 dimension Task agents run in 3 batches of ≤3 concurrent
  (per the skill's max-3-concurrent constraint) — batch 1: Dimensions 1–3
  (`general-purpose` for D1, `renderer-specialist` for D2/D3); batch 2:
  Dimensions 4–6 (`renderer-specialist` ×3); batch 3: Dimensions 7–9
  (`general-purpose` for D7/D8, `renderer-specialist` for D9). Each wrote
  `/tmp/audit/performance/dim_{1..9}.md`, consolidated here per Phase 3 of the
  skill.
- **HEAD**: `b3db49fa`, branch `main`, 2026-09-11.
- **Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux --limit 200
  --json number,title,state,labels` → `/tmp/audit/performance/issues.json`,
  **56 entries, all `state == "OPEN"`** (the export carries no CLOSED rows this
  time, so "CLOSED → verify fix landed" was exercised by reading the code
  directly against issue numbers named in the skill/prior reports, not by a
  GitHub CLOSED match). Plus `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` for
  guard/finding continuity (Dimensions 1 and 3 only, its prior scope).
- **Method**: static analysis + source reading only. No engine process was
  launched (*feedback_no_parallel_engine_launch*) and no bench was run this
  session.

---

## Observed-vs-ROADMAP bench delta

**Not measured this session.** ROADMAP's current Bench-of-record is the
stepped-camera refresh at HEAD `4c9a5b36` (2026-09-09) — full 5-scene × 5-config
× 3-run matrix, 75 runs, zero rejections (`docs/audits/BENCH_stepped-camera_4c9a5b36.tsv`).
This audit is static analysis only (no `--bench-frames` run), so no FPS/frame-time
delta is reported against it — per the skill, no number is fabricated. None of
the findings below are large enough, individually, to be separable from that
matrix's own run-to-run noise; the two largest (PERF-D6-2026-09-11-01,
PERF-D3-2026-09-11-01) are both estimated in the tens-of-microseconds /
budget-accounting-only class respectively, not multi-millisecond.

---

## Executive Summary

**22 findings: 0 CRITICAL, 0 HIGH, 8 MEDIUM, 14 LOW.**

| Dimension | Findings | Severity breakdown |
|---|---|---|
| 1 — CPU Hot Paths | 2 | 1 MEDIUM (NEW), 1 LOW (Existing, partially fixed) |
| 2 — Draw & Instancing | 5 | 5 LOW (all NEW) |
| 3 — GPU Memory Pressure | 3 | 2 MEDIUM (NEW), 1 LOW (NEW, telemetry) |
| 4 — SSBO Sizing & Upload | 3 | 1 MEDIUM (NEW), 2 LOW (NEW) |
| 5 — GPU Pipeline | 2 | 2 LOW (NEW, both doc/code-quality) |
| 6 — Skinning & BLAS | 2 | 1 MEDIUM (NEW), 1 LOW (NEW) |
| 7 — Streaming & Cells | 0 | — (all guards intact, nothing new) |
| 8 — NIF Parse | 2 | 2 MEDIUM (NEW) |
| 9 — Telemetry & Origin Cost | 3 | 1 MEDIUM (NEW), 2 LOW (NEW) |

**Zero CRITICAL, zero HIGH.** No memory corruption, no in-flight-GPU-memory
free, no missing AS barrier, no wrong SSBO indexing, no TAA/SVGF history
break on a render-origin crossing, and no timestamp readback stall were found
anywhere in the nine dimensions — all of these specific HIGH/CRITICAL-class
failure modes named in the skill's checklists were traced to source this
session and are confirmed absent.

**Regression-guard posture: clean.** Every named Session-46/75/76/#3660-series
guard checked by each dimension agent (see per-dimension sections below) is
**intact** — nothing that previously landed has eroded. Three of the four
Dimension-3 items and all five Dimension-1 items carried over from
`AUDIT_PERFORMANCE_2026-09-05.md` are now **fixed** in current code (see
"Prior-audit re-verification" under each dimension); the residual open item
from that report is re-reported here as `PERF-D3-2026-09-11-01` (its
reachability gap, not its accounting, which is now fixed).

**Dedup note**: the `issues.json` dump for this run contained only OPEN
issues (56, none CLOSED), so several "verify the fix landed" checks were done
by reading the code directly against the issue number cited in the skill text
rather than against a GitHub CLOSED record. Two dimensions (6, 8) additionally
re-confirmed pre-existing OPEN issues (#4050, #4049, #3813) are still present
in current code — these are **not** counted in the finding total above (per
dedup protocol, an OPEN match is skipped, not re-filed) but are listed in each
dimension's section for continuity.

---

## Hot Path Analysis

Per-frame CPU + per-pass GPU cost, sourced from the dimension agents' direct
reads of `gpu_timers.rs` / `ScratchTelemetry` / the constants they gate, not
re-measured this session:

| Site | Per-frame cost (estimated from code, not measured) | Class | Finding |
|---|---|---|---|
| Bone-palette compute dispatch extent | O(all resident skinned entities × 144 mat4), not O(dirty) — ~30-50 µs waste on a single-dirty-actor frame at the Freeside baseline (~97.5K palette slots) | GPU dispatch scaling with population, not motion | PERF-D6-2026-09-11-01 (MEDIUM) |
| Mid-batch BLAS eviction / #3979 admission gate | Unreachable on the production per-REFR `build_blas_batched` caller (batches are 1–20 meshes, gate needs `idx % 64 == 0`) — unbounded *real* BLAS residency growth during an over-budget synchronous interior load | Structural budget-gate reachability gap | PERF-D3-2026-09-11-01 (MEDIUM) |
| `flush_pending_uploads` staging residency | Peak = sum of one flush batch's staging sizes; batching threshold is a texture *count* (64), not a byte budget — ~11 MB to ~1.4 GB depending on texture size for the same policy decision | Unbilled transient VRAM/BAR spike | PERF-D3-2026-09-11-02 (MEDIUM) |
| Two eagerly-allocated scene SSBOs (`instance_buffers`, `previous_model_buffers`) | ~117.5 MB resident at `MAX_INSTANCES` (262144) ceiling vs ~3.3 MB needed at the densest measured cell (7359 instances) | Residency over-allocation (not upload — upload is already O(live)) | PERF-D4-2026-09-11-01 (MEDIUM) |
| `decode_bs_vertex_stream` tangent array | Grows via repeated `Vec::push` (default doubling) instead of pre-sized `allocate_vec`, for every normal-mapped Skyrim SE+/FO4+/Starfield mesh — `log2(num_vertices)` extra realloc+copy cycles per mesh block | NIF parse allocation churn | PERF-D8-2026-09-11-01 (MEDIUM) |
| `pre_parse_cell` per-REFR model-path loop | 2-3 throwaway string allocations per REFR (not per unique model), on a documented ~95% cache-hit / ~10× refs-to-unique-models ratio | Streaming-worker allocation churn ahead of dedup check | PERF-D8-2026-09-11-02 (MEDIUM) |
| `atw_post_ms` nesting contract | Doc-only: field doc + `cpu_breakdown` both describe stale/wrong containment; `atw_post` actually ⊇ the entire `render_one_frame` call since Phase 14 | Stall mis-attribution (numbers correct, contract wrong) | PERF-D9-2026-09-11-01 (MEDIUM) |
| Six AI-locomotion systems' `NavPath` write-back | One `VecDeque<Vec3>` clone + copy per actively-pathing entity per frame, across follow/escort/guard/patrol/travel/wander | CPU per-frame allocation on a live (not dead) write path | PERF-D1-2026-09-11-01 (MEDIUM) |
| Material-hash key production (`material_hash`) | ~110 `FxHasher::write_u32` calls per `DrawCommand` per frame (~0.2 ms at 7359 draws), even on frames where every upload gate skips | CPU key-production cost separate from the (already-O(1)) dedup itself | PERF-D4-2026-09-11-03 (LOW) |
| FSR reactive/transparency mask attachments under `--upscaler taa` | ~7.4 MB/frame ROP write traffic + ~14 MB VRAM for two attachments nothing reads on the non-default TAA path | Unconditional attachment on a conditional pass | PERF-D5-2026-09-11-02 (LOW) |

No CRITICAL/HIGH-class GPU pass cost was found anywhere — every per-pass
dispatch verified this session (bloom, SSAO, volumetrics, cluster cull, TLAS
build, SVGF, TAA) is O(pixels)/O(froxels), matches its documented shape, and
carries correct AS/barrier sequencing.

---

## Findings

Grouped by severity, CRITICAL first. All 22 are listed; "Existing" items note
their prior report/issue rather than being re-argued in full.

### MEDIUM (8)

#### PERF-D1-2026-09-11-01: Six AI-locomotion systems clone `NavPath` on write-back when the source is already dead
- **Severity**: MEDIUM · **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/follow.rs:311`, `escort.rs:442`, `guard.rs:299`, `patrol.rs:230`, `travel.rs:359`, `wander.rs:400`
- **Status**: NEW
- **Description**: All six AI-locomotion systems share a Pass-1/Pass-2 shape. Pass 2 writes `d.nav_path` into `NavPath` component storage via `path.clone()` even though `scratch.decisions` (which owns it) is fully cleared and repopulated next frame and nothing reads `d.nav_path` again after this loop. The input side of the same functions already eliminated the analogous clone via `mem::take` with an explicit comment ("hand the list over instead of cloning it every tick") — the write-back side was not given the same treatment.
- **Evidence**: `follow.rs:267` documents the already-fixed input-side elimination; the write-back loop at `follow.rs:311` (and the five siblings) still does `nq.insert(d.entity, path.clone())`.
- **Impact**: One `VecDeque<Vec3>` heap allocation + element-wise copy per actively-pathing entity per frame, recurring every tick for the path's lifetime (not just on repath), across six systems, scaling with concurrently-active NPC AI in a populated exterior cell.
- **Related**: Same class as the already-fixed input-side clone in the same six files; conceptually related to `PERF-D1-2026-09-11-02` (unnecessary clone of an about-to-be-dropped scratch value), distinct code path.
- **Suggested Fix**: `for d in &mut scratch.decisions { match d.nav_path.take() { Some(path) => nq.insert(d.entity, path), None => nq.remove(d.entity) } }` — behavior-preserving since `scratch.decisions` is rebuilt from scratch next frame with no intervening read.
- **Confidence**: High.

#### PERF-D3-2026-09-11-01: The mid-batch BLAS eviction check and #3979's admission gate are unreachable on the production static-BLAS path
- **Severity**: MEDIUM · **Dimension**: GPU Memory Pressure & Eviction Thrash
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:434-508`, `constants.rs:81` (`BATCH_EVICTION_CHECK_INTERVAL = 64`), `byroredux/src/cell_loader/spawn.rs:708,748-751`, `byroredux/src/cell_loader/references/mod.rs:284-303`, `cell_loader/load.rs:588`
- **Status**: NEW
- **Description**: The mid-batch eviction trigger and the #3979 admission gate are both reachable only through `idx % BATCH_EVICTION_CHECK_INTERVAL (64) == 0` inside `build_blas_batched`'s Phase-1 loop, and the admission gate additionally needs `eviction_can_still_reclaim`, which is written *only* inside that same interval-gated block. The dominant production caller, `spawn_placed_instances`, submits one batch **per placed reference** (a single NIF's sub-meshes — single digits to low tens), never approaching `idx == 64`. So on interior and exterior loads alike, only the pre-batch eviction (evaluated against the accounting figure, not the resident one, since resident free lags `DEFAULT_COUNTDOWN` frames behind `tick_deferred_destroy`) ever runs. Worst case: the synchronous interior load path (`FrameTimeBudget::unlimited()`) spawns every REFR with no intervening `draw_frame`, so once a cell crosses budget, each subsequent per-REFR batch pre-evicts the paper figure back down while the evicted BLAS stay resident — true residency grows monotonically for the rest of the load.
- **Evidence**: `blas_static.rs:425` (`eviction_can_still_reclaim = true` init), `:434-436` (interval gate), `:470` (sole writer, inside the gated block); `spawn.rs:3,708,749` ("Per REFR placement the loader calls `spawn_placed_instances`").
- **Impact**: Unbounded real BLAS residency growth during an over-budget synchronous cell load, on exactly the small-VRAM (6 GB RT-minimum) configuration the budget machinery exists for. Not reachable on the 12 GB dev card. Failure mode is a logged `Batched BLAS build failed` (missing RT geometry for the cell), not corruption.
- **Related**: #3979 (the gate this neuters), #3840 (`pending_destroy_static_bytes`), #510 (the interval), #1792/#1793 (distinct, documented-not-fixed siblings), #3540.
- **Suggested Fix**: Decouple `blas_admission_exhausted` from the loop index (it's three integer ops — cheap to evaluate every iteration); arm `eviction_can_still_reclaim` from the pre-batch eviction's paper-delta too. Keep the 64-interval on the `evict_unused_blas` *call* itself only.
- **Confidence**: High on mechanism; Medium on magnitude (not reproduced on hardware — unreachable on the 12 GB dev card).

#### PERF-D3-2026-09-11-02: `flush_pending_uploads` holds every staged texture's staging buffer live at once, and the flush trigger is a texture count, not a byte budget
- **Severity**: MEDIUM · **Dimension**: GPU Memory Pressure & Eviction Thrash
- **Location**: `crates/renderer/src/texture_registry/upload.rs:384-390,448-473,540-544`, `byroredux/src/cell_loader/references/mod.rs:779-788` (`YIELDED_TEXTURE_UPLOAD_BATCH_MIN = 64`), `crates/renderer/src/vulkan/buffer.rs:115-124`, `texture_registry/mod.rs:76-85,258`
- **Status**: NEW (partial prior acknowledgement — `memory-budget.md`'s "Not yet ledgered" section already names the retention-vs-in-flight gap generally, but not this specific mechanism)
- **Description**: Peak host-visible staging residency for one flush is the **sum** of that batch's staging sizes (each `StagedUpload`'s guard is returned to the pool only after submit+fence completes). `StagingPool`'s budget is enforced on `release`, not `acquire`, so an over-budget batch simply allocates fresh buffers. The batching policy (`should_flush_pending_cell_textures`) fires at a texture *count* of 64, a floor not a ceiling — 64 × a 4096² BC7 mip chain (~22 MB) is ~1.4 GB of simultaneous staging plus ~1.4 GB of retained `dds_bytes` in host RAM, versus 64 × a 512² BC1 (~0.17 MB) at ~11 MB — two orders of magnitude under the same policy decision. Two forced flush call sites have no threshold at all.
- **Evidence**: `upload.rs:384-389` (`StagedUpload` accumulation), `:506-545` (release only post-submit/fence); `buffer.rs:115-116` ("the budget only bounds *retained* size, not in-flight allocations"); `references/mod.rs:779-785`.
- **Impact**: A transient host-visible VRAM/BAR spike proportional to a cell's texture *bytes*, unbilled in `memory-budget.md`'s VRAM roll-up, competing with the BLAS budget's `screen_scaled_reservation_bytes` (which doesn't know about it) on a 6 GB card during a texture-heavy cell load. Fails gracefully (dropped upload → checkerboard texture), not a crash.
- **Related**: #881/CELL-PERF-03, #239, #1922, `memory-budget.md` "Not yet ledgered".
- **Suggested Fix**: Add a byte accumulator to the enqueue path; make the flush trigger `count >= 64 || bytes >= BUDGET` sized against `DEFAULT_STAGING_BUDGET_BYTES`. Chunk the forced-completion flush into bounded byte-sized sub-batches.
- **Confidence**: High on mechanism; Medium on magnitude (no measured per-cell texture-byte histogram taken).

#### PERF-D4-2026-09-11-01: The two biggest scene SSBOs are eagerly allocated at a 5× worst-case capacity no measured cell approaches
- **Severity**: MEDIUM · **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/buffers.rs:495-497,565-576`, `constants.rs:97-141`, `docs/engine/memory-budget.md:92-93`
- **Status**: NEW
- **Description**: `instance_buffers` and `previous_model_buffers` are allocated at context init at the full `MAX_INSTANCES` (262144) ceiling — 262144 × 160 B and 262144 × 64 B, × 2 FIF = 83.9 MB + 33.6 MB = **117.5 MB resident**, roughly half the entire scene-buffer budget. `MAX_INSTANCES` itself is deliberately sized with ~5× headroom over the densest observed city cells (~50K REFRs); at the densest measured real workload (MedTek, 7359 instances) only ~3.3 MB is needed. The remaining ~95 MB is never touched by any measured workload. This is a residency finding, not an upload one — upload is already correctly O(live data).
- **Evidence**: `buffers.rs:495-497` — unconditional `MAX_INSTANCES`-sized allocation inside the `0..MAX_FRAMES_IN_FLIGHT` loop, no resize path.
- **Impact**: ~95 MB of VRAM held for a case no shipped content reaches (~2.4% of the 4 GB budget ceiling, ~5% of the doc's ~1.91 GB typical-FNV-interior total). Not a frame-time cost.
- **Related**: `memory-budget.md:92-93,109,793`; #992 (mesh-ID encoding, unaffected).
- **Suggested Fix**: Allocate at a working capacity (e.g. 64K instances = 28.7 MB for the pair) and grow by doubling up to `MAX_INSTANCES` at a frame/cell-load boundary, rewriting the two descriptor writes on grow. Author explicitly notes this must not ship without on-device measurement (`feedback_speculative_vulkan_fixes`); if the grow machinery is judged not worth the risk, the alternative is documenting the 117.5 MB as an accepted cost rather than an unexamined default.
- **Confidence**: Medium-High on the numbers; Medium on the fix being worth its risk.

#### PERF-D6-2026-09-11-01: The bone-palette compute pass still dispatches the full dense slot range every frame, after #3665 narrowed its own input upload to dirty slots
- **Severity**: MEDIUM · **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:301-303`, `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs:142-188`, `crates/renderer/shaders/skin_palette.comp:73-79`
- **Status**: NEW
- **Description**: Three of the four stages of the skin-preparation chain are dirty-narrowed (staging memcpy + `cmd_copy_buffer` walk `bone_world_copy_regions[frame]`, built from `dirty_slot_offsets` by #3665). The palette dispatch that consumes them was not narrowed with them: its extent is `bone_world_dispatch_bytes`, unconditionally the whole high-water array length (`(pool.max_used_slot() + 1) * MAX_BONES_PER_MESH`), regardless of how many entities actually moved. The only existing skip is all-or-nothing (`skip_skin_gpu_refresh` requires the *entire* pose-dirty set empty for `MAX_FRAMES_IN_FLIGHT` consecutive frames) — one twitching NPC anywhere in loaded cells re-arms the full-range recompute for every resident skinned entity.
- **Evidence**: `upload.rs:301-303` (`count` = high-water length, independent of dirty set); `render/skinned.rs:148-149` (sizes at `(max_used_slot()+1) * MAX_BONES_PER_MESH`); `skin_palette.comp:75` (`if (slot >= bone_count) return;` — the only bound check).
- **Impact**: On the Freeside baseline (~677 live skinned entities → ~97.5K palette slots), a frame with a single dirty actor still moves ~18 MB and issues ~1.5K workgroups — order 30-50 µs of the `skin_palette_ms` bracket, ~100% waste in the common case (few actors animating, most idle/off-frustum). Scales with *population*, the wrong axis for a streaming open-world renderer; blunts the payoff of the #3665/#1811 state machine that would make narrowing cheap.
- **Related**: #3665 (the state machine to reuse), #1811 (all-idle skip), #1195 (per-entity gate this pass doesn't consume), #3676 (`skin_palette` timer that would measure the win).
- **Suggested Fix**: Reuse `bone_world_copy_regions[frame]` as the palette work list instead of the dense extent — either one `cmd_dispatch` per region (few-regions case) or an index SSBO of region slot-bases (many-regions case), with the existing dense path kept behind a region-count threshold as a fallback.
- **Confidence**: High on mechanism; Medium on magnitude (not measured on-device).

#### PERF-D8-2026-09-11-01: `decode_bs_vertex_stream`'s tangent array grows by repeated push instead of the pre-sized allocation its sibling decoder already uses
- **Severity**: MEDIUM · **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:1138` (compare `crates/nif/src/import/mesh/sse_recon.rs:341-345`)
- **Status**: NEW
- **Description**: `decode_bs_vertex_stream` — the shared packed-vertex decoder for essentially all Skyrim SE+/FO4+/Starfield static and skinned geometry — pre-sizes `vertices`/`uvs`/`normals`/`vertex_colors` unconditionally and `bone_weights`/`bone_indices` conditionally on `is_skinned` (both correct, gating on a value known before the loop). `tangents` is left `Vec::new()` regardless of `vertex_attrs`, even though whether it will be pushed on every iteration is exactly as knowable in advance (`VF_VERTEX && VF_TANGENTS && VF_NORMALS`, fixed for the whole call) as the skinning case. The sibling SSE-reconstruction decoder for the *same on-disk format* (`sse_recon.rs`, #559) already applies this exact conditional pre-size for `tangents` — the fix is known and landed elsewhere but wasn't carried to the more heavily-hit inline `BsTriShape::parse` path.
- **Evidence**: `bs_tri_shape.rs:1138` (`Vec::new()`, never resized) vs `:1251` (push gated by the four `Option`s reducing to the uniform `vertex_attrs` condition); `sse_recon.rs:341-345` (`if has_tangent_quad { Vec::with_capacity(num_vertices) } else { Vec::new() }`). Existing dhat coverage (`heap_allocation_bounds.rs`'s `bs_tri_shape_block_with_vertices` fixture) explicitly omits `VF_TANGENTS` per its own comment, so this path has never been under a heap-allocation bound.
- **Impact**: For every normal-mapped Skyrim SE+/FO4+/Starfield mesh — the dominant case, since `VF_TANGENTS` is set pervasively wherever a mesh authors a normal map — `tangents` grows via default doubling-capacity reallocation instead of one reservation: `log2(num_vertices)` extra realloc+copy cycles per mesh block. Bounded (amortized O(1), not O(n²)) but exactly the regression class #3691 fixed elsewhere in the same file family.
- **Related**: #3691 (skin buffer pre-sizing, sibling fix); `#2114 / D8-02` (the dhat fixture with the coverage gap this finding identifies).
- **Suggested Fix**: Hoist the `has_tangent_quad` check before the loop and pre-size via `stream.allocate_vec(nv_u32)?`, mirroring the `is_skinned` gate and `sse_recon.rs` verbatim. Extend the dhat fixture with `VF_TANGENTS` set so a future revert trips the existing gate.
- **Confidence**: High.

#### PERF-D8-2026-09-11-02: `pre_parse_cell`'s per-REFR model-path loop allocates two-to-three throwaway strings before the dedup check that would make most of them unnecessary
- **Severity**: MEDIUM · **Dimension**: NIF Parse (pre-parse/streaming path)
- **Location**: `byroredux/src/streaming.rs:1430-1472`, `byroredux/src/cell_loader/nif_import_registry.rs:49-56` (`canonical_model_path_key`)
- **Status**: NEW
- **Description**: For **every** REFR in a cell (not just unique models), the loop does a full-string `.to_ascii_lowercase()` purely to test `.ends_with(".spt")` (discarded), then calls `canonical_model_path_key` — itself a second `to_ascii_lowercase().replace(...)` plus, conditionally, a `format!` allocation — **before** the cache/batch dedup check that determines whether the computed key is even used. The function's own comments document ~95% cache-hit rate on a 7×7 exterior grid and heavy per-cell model-path duplication (chairs/lanterns/rocks sharing one path each) — meaning these 2-3 allocations per REFR run essentially every time regardless of outcome.
- **Evidence**: `streaming.rs:1452,1460`; `nif_import_registry.rs:50,54`.
- **Impact**: Bounded, load-path (streaming-worker thread, not render-frame) allocation churn proportional to `cell.references.len()` rather than the unique-model count, which the surrounding comments document as roughly an order of magnitude smaller. Cell-load latency waste, not a frame-time regression.
- **Related**: None (distinct from #3038's correctness fix and #877/#830/#1262's phase-split work, unaffected by this finding).
- **Suggested Fix**: Non-allocating suffix test for `.spt`; memoize `canonical_model_path_key` per-call keyed on the raw `&str`, or check the dedup sets with a borrowed lowercase compare before committing to an owned `String`.
- **Confidence**: Medium-High (code/comment evidence direct; wall-clock cost not profiled — a dhat fixture on a high-reference-count cell would make it measurable).

#### PERF-D9-2026-09-11-01: The CPU-phase nesting contract is documented wrong in both places an operator reads it
- **Severity**: MEDIUM · **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/core/src/ecs/resources/mod.rs:835-840` (`atw_post_ms` doc), `byroredux/src/systems/debug.rs:114-120` (`cpu_breakdown`), producer `byroredux/src/app_events.rs:819,893,1328`
- **Status**: NEW
- **Description**: `atw_post_ms`'s field doc lists only its Phase-10-era contents (`step_streaming`, `step_debug_loads`, `step_cell_transition`, window title). Phase 14 moved `render_one_frame` *inside* that same bracket (opens `:819`, calls render at `:893`, closes `:1328`), so in steady state `atw_post ⊇ rof_pre_draw + rof_draw_call + rof_post_draw` — i.e. the entire render path, `draw_frame`, fence wait, submit. `cpu_breakdown`'s paragraph then asserts the *opposite and false* containment ("`atw_post` ⊇ `atw_pre`/`atw_scheduler`'s sibling work" — the three `atw_*` brackets are actually strictly disjoint and sequential), occupying the exact sentence where the true `atw_post ⊇ rof_*` containment should be.
- **Evidence**: Bracket line numbers in `app_events.rs` (805/808/812/819/892-894/1328) directly contradict the doc text quoted verbatim in both files.
- **Impact**: Stall mis-attribution — this bucket's entire purpose. The `cpu_breakdown` triage rule stays *accidentally* safe because it conditions on `rof_*` being small, but the field doc read alone (as the debug-UI Metrics panel and `byro-dbg` do) would point an investigator at exterior streaming for a frame whose cost is actually `draw_frame`. No runtime cost — the numbers are right, the contract describing them is not.
- **Related**: #3692, #3674, #2731, Phase 14.
- **Suggested Fix**: Two doc-only edits — extend `atw_post_ms`'s doc to state it also contains `render_one_frame` in its entirety; replace `cpu_breakdown`'s false clause with the true `atw_post ⊇ rof_*` statement and note the three `atw_*` brackets are disjoint siblings. Optionally pin with a source-order assertion like `between_frames_capture_ordering_tests`.
- **Confidence**: High.

### LOW (14)

#### PERF-D1-2026-09-11-02: `scene_trigger_actor_approach_system` still deep-clones every `ScenePlayer`'s unused heap fields every frame
- **Severity**: LOW · **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/cinematic.rs:452-458`
- **Status**: Existing: PERF-D1-2026-09-05-05 (prior report; partially fixed — the outer-container reallocation is gone, closure-captured scratch now in place; the per-entity deep clone of `active_actions: Vec<...>` / `completed_actions: HashSet<u32>` remains, and neither is read downstream — only three `Copy` scalar fields are)
- **Description/Evidence/Impact/Suggested Fix**: see `/tmp/audit/performance/dim_1.md` for full detail (preserved in the merge working set; summary: clone only the three scalar fields needed, or iterate the query directly for the three consumer passes).
- **Confidence**: Medium-High.

#### PERF-D2-2026-09-11-01: `no_sorter` occupies sort-key slot 3 on the opaque and additive branches, partitioning two populations it has no ordering meaning for
- **Severity**: LOW · **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/mod.rs:846-849` (opaque), `:755-758` (additive)
- **Status**: NEW
- **Description**: Slot 3 is load-bearing only for the true-alpha-over branch (separating depth-sorted from state-clustered-opt-out draws). The opaque/additive branches also write `cmd.no_sorter as u8` into the same slot "for free" — but it sits above `render_layer`/`two_sided`/`pack_depth_state`/`mesh_handle`, so any opaque/additive population containing one `NoSorter` draw is cut into two blocks, doubling the layer/cull/depth-state ladder traversal. `no_sorter` is reachable on opaque draws: an alpha-tested `NiAlphaProperty` with bit 13 set produces an *opaque* `DrawCommand` carrying the marker (confirmed via #3797's own landing-commit census: 74/76 Oblivion meshes, concentrated in Ayleid ruins/shipwrecks/clutter; 0 on FO3/FNV).
- **Impact**: Does not split same-mesh instanced runs (the flag derives from the shape's own property, uniform per mesh) — only duplicates the state ladder and adds a `group_state` boundary (one extra indirect draw call) per affected cell. Small; author explicitly tried to make this bigger and could not.
- **Suggested Fix**: Write `0u8` into slot 3 on the opaque and additive branches; correct the doc block claiming this is "harmless".
- **Confidence**: High on mechanism; Low-Medium on magnitude (needs an Oblivion Ayleid-ruin capture to quantify).

#### PERF-D2-2026-09-11-02: `preserve_opaque_gbuffer` is a `PipelineKey` axis with no corresponding `draw_sort_key` slot
- **Severity**: LOW · **Dimension**: Draw & Instancing
- **Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:474-486` vs `byroredux/src/render/mod.rs:752-841`
- **Status**: NEW (sibling of OPEN #2764, which covers the batch-merge-key half of the same field)
- **Description**: `PipelineKey::Blended` has four axes (`src`, `dst`, `wireframe`, `preserve_opaque_gbuffer`), each mapping to a distinct `VkPipeline`, but `draw_sort_key` only carries slots for the first three (wireframe was added by #1806 for exactly this reason). Two draws agreeing on every sorted axis but differing in `preserve_opaque_gbuffer` can interleave and ping-pong `cmd_bind_pipeline`. Narrow in practice: the dominant alpha-blend population takes the depth-primary branch where this wouldn't help; only the additive/`no_sorter` branches would benefit, and their affected population (`is_refractive_glass && (additive || NoSorter)`) is plausible but unmeasured.
- **Suggested Fix**: Add `is_refractive_glass(cmd) as u32` after `dst_blend` in the additive and `no_sorter` branches only, not the depth-primary branch.
- **Confidence**: High that the axis is absent; Low that it costs measurable frame time on tested content.

#### PERF-D2-2026-09-11-03: `build_instance_map` heap-allocates a fresh `Vec<Option<u32>>` per frame — the one per-frame render buffer with no scratch amortization
- **Severity**: LOW · **Dimension**: Draw & Instancing
- **Location**: `crates/renderer/src/vulkan/context/begin_frame_recording.rs:149-157`, allocation at `acceleration/predicates.rs:310`
- **Status**: NEW
- **Description**: Every neighboring per-frame buffer on this path (`gpu_instances_scratch`, `batches_scratch`, `indirect_draws_scratch`, `terrain_tile_scratch`, etc.) is a persistent field taken via `mem::take` and restored — the #243 amortization convention. `instance_map` is the outlier: allocated fresh and dropped every frame, sized to the full draw-command count.
- **Impact**: ~32 KB malloc/free pair per frame at the FO4 baseline (3949 draw commands); microseconds, reported because the violated convention is explicit and enforced everywhere else on the same function.
- **Suggested Fix**: Add `instance_map_scratch` to `VulkanContext`, give `build_instance_map` an `out: &mut Vec<_>` variant, thread it through `BeginFrameOutput` as a borrow or taken/returned scratch.
- **Confidence**: High.

#### PERF-D2-2026-09-11-04: The raster sort permutes ~480-byte records and recomputes the 12-tuple key O(n log n) times
- **Severity**: LOW · **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/mod.rs:891-897` (`sort_draw_commands`), key at `:696-860`
- **Status**: NEW
- **Description**: `sort_unstable_by_key(draw_sort_key)` doesn't cache keys, so the branchy 12-tuple key build is re-evaluated on every comparison, and `DrawCommand` itself is a ~480-byte struct being memmoved on every swap. The measured crossover table is therefore mostly memmove-and-rekey cost, not comparison cost.
- **Impact**: Nil on every committed baseline (max `bench_draws_raster_cmds` 283, well under any measurable threshold); matters only on exterior grids reaching the 3000-command parallel-sort gate.
- **Suggested Fix**: Do not change the `3000` constant. Extend the existing bench harness with a `(key, index)`-pair-sort + apply-permutation variant and re-run the sweep; only act on the measured result.
- **Confidence**: High that both costs are paid; unmeasured whether the indirection wins at relevant sizes — hence "measure", not "change".

#### PERF-D2-2026-09-11-05: The FO4 runtime baseline carries `bench_draws_batches` (296) > `bench_draws_raster_cmds` (256) — a pair that cannot come from one capture
- **Severity**: LOW (measurement integrity) · **Dimension**: Draw & Instancing
- **Location**: `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv:84-85`
- **Status**: NEW
- **Description**: `batches <= raster_cmds` holds structurally (a `DrawBatch` is only created from a command surviving the raster-prefix filter). The FO4 TSV violates it because two rows were regenerated 12 days apart from different `entities_total` states (a #3660 partial regen held the older #3006 `batches` row). The other four committed TSVs are self-consistent.
- **Impact**: No engine defect — a stale-baseline-merge risk: this dimension's own checklist instructs cross-checking the parallel-sort threshold against `bench_draws_raster_cmds`, which cannot be sanity-checked against its own sibling on this row.
- **Suggested Fix**: Re-capture all four FO4 `bench_draws_*` rows in one run; add an invariant check (`gpu_calls <= batches <= raster_cmds <= cmds`) to `/audit-runtime` Phase 3.
- **Confidence**: High.

#### PERF-D3-2026-09-11-03: The static-BLAS batch log line says "Cell BLAS batch" for a per-REFR batch
- **Severity**: LOW (telemetry) · **Dimension**: GPU Memory Pressure & Eviction Thrash
- **Location**: `byroredux/src/cell_loader/spawn.rs:748-751`
- **Status**: NEW
- **Description**: `log::info!("Cell BLAS batch: {built}/{} meshes", ...)` runs once per placed reference, not once per cell; a cell load emits hundreds of these. An operator diagnosing BLAS residency reads them as per-cell totals — this mislabeling is what made `PERF-D3-2026-09-11-01`'s reachability gap non-obvious.
- **Suggested Fix**: Re-label to the REFR scope; accumulate a real per-cell figure in the caller if wanted.
- **Confidence**: High.

#### PERF-D4-2026-09-11-02: `memory-budget.md`'s scene-buffer total (225 MB) is exactly one `GpuInstance`-growth stale
- **Severity**: LOW (doc rot) · **Dimension**: SSBO Sizing & Upload
- **Location**: `docs/engine/memory-budget.md:109,793`
- **Status**: NEW
- **Description**: The page's own row table was updated for #3231's `GpuInstance` 128→160 B growth, but the two totals below it (225 MB, ~223 MB) were not — they reproduce exactly the pre-#3231 sum. Current row sum is ≈242.3 MB (delta = 262144 × 32 B × 2 FIF = 16.8 MB, reproduced exactly).
- **Impact**: Under-counts resident scene VRAM by ~17 MB (7%) for any reader of the doc, including this audit's own skill instructions ("~223–225 MB").
- **Suggested Fix**: Update both totals to ≈242 MB; state the MiB/MB unit convention once in the table header (the table currently mixes both under one "MB" label).
- **Confidence**: High.

#### PERF-D4-2026-09-11-03: Material interning re-hashes ~110 fields per draw per frame even on frames where every upload gate skips
- **Severity**: LOW · **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:596-745` (`material_hash`)
- **Status**: NEW
- **Description**: `intern_by_hash` itself is correctly O(1)/O(unique materials); producing its *key* is not — `material_hash` walks ~110 fields with `FxHasher::write_u32` per `DrawCommand`, every frame, ~800K hash steps (~0.2 ms) at the MedTek workload's 7359 draws. A cache on the `Material` component is not safe as-is: the hash covers animated lanes (shader color/float, alpha, UV offset) that legitimately change per frame with no existing invalidation signal, so a naive cache risks silent mis-dedup (a correctness hazard worse than the cost it would save).
- **Suggested Fix**: Only if profiling shows this on critical path: split static vs. animated lanes, caching the static half behind the existing `resolve_pbr` import hook.
- **Confidence**: Medium (mechanism certain; ~0.2 ms is an instruction-count estimate, not measured).

#### PERF-D5-2026-09-11-01: `CLAUDE.md` still attributes ACES tone mapping to the composite pass
- **Severity**: LOW (doc/code quality) · **Dimension**: GPU Pipeline
- **Location**: `CLAUDE.md:146,159`
- **Status**: NEW
- **Description**: Composite now emits render-resolution linear HDR; exposure + ACES moved to `presentation.frag` downstream of the upscale boundary (confirmed: `composite.frag`/`composite.rs` contain no ACES/tonemap symbol; `presentation.frag:43,162` defines and applies `aces()`; `docs/engine/fsr3-upscaler-integration-plan.md:133` records the move). `CLAUDE.md`'s two lines still describe the old shape.
- **Impact**: Actively misleading for the frame's most safety-critical ordering question — bloom runs *after* composite on the (correctly) linear-HDR image; a reader trusting the stale `CLAUDE.md` line would file that ordering as a MEDIUM defect and be wrong (a plausible false positive for every future pass over this area).
- **Suggested Fix**: Reword both lines to describe composite as linear-HDR reassembly and attribute exposure/ACES to `presentation.vert/frag` (which currently has no `CLAUDE.md` entry at all).
- **Confidence**: High.

#### PERF-D5-2026-09-11-02: The two FSR mask attachments are unconditional render-pass attachments, so `--upscaler taa` clears/writes 2 B/px/frame nothing samples
- **Severity**: LOW (inefficient pipeline state on a non-default path) · **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:178-198,276-277,389-390`, `triangle.frag:69-70`, `gbuffer.rs:88`
- **Status**: NEW
- **Description**: The FSR3 reactive/transparency masks (attachments 6/7) are built unconditionally into the fixed 8-attachment main render pass; their sole reader is `record_upscale_pass`, which under `UpscalerMode::Taa` never dispatches FSR. `triangle.frag` declares both outputs with no upscaler-mode specialization. The TAA path is reachable in production (not just via an explicit flag) — it's also the FSR-construction-failure fallback (#2480).
- **Impact**: ~7.4 MB/frame ROP traffic + ~14 MB VRAM at 2560×1440 on the non-default path; does not affect the shipped default (`Fsr3(Quality)`).
- **Suggested Fix**: Document only (recommended, zero risk) — do not restructure the render pass speculatively; a real fix (a second render-pass variant) touches every pipeline/framebuffer/shader-output-location in lockstep and must not land without RenderDoc + `BYRO_VALIDATION=1` evidence on both upscaler modes.
- **Confidence**: High on the observation; the bandwidth figure is an arithmetic estimate, not measured.

#### PERF-D6-2026-09-11-02: The per-entity skin dispatch re-binds the same compute pipeline once per skinned entity
- **Severity**: LOW · **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs:701-728`, called from `skinned_blas_refit.rs:501-570`
- **Status**: NEW
- **Description**: `SkinComputePipeline::dispatch` opens with `cmd_bind_pipeline` and is called once per non-skipped entity in the dispatch loop; the pipeline handle is immutable for the renderer's lifetime, so every bind after the first is redundant (descriptor set + push constants genuinely vary per slot; the pipeline bind does not).
- **Impact**: One extra recorded command per dirty skinned entity per frame; sub-microsecond each, zero GPU cost — a small constant, not a scaling hazard.
- **Suggested Fix**: Split a `bind()` off `dispatch()`, hoist above the loop; keep the timer bracket ordering unchanged.
- **Confidence**: High on the redundancy; Low-Medium that it's worth a standalone commit (bundle with a future edit to the same loop).

#### PERF-D9-2026-09-11-02: `about_to_wait`'s two screenshot-handshake early returns skip the `atw_*` write
- **Severity**: LOW · **Dimension**: Telemetry & Origin Cost
- **Location**: `byroredux/src/app_events.rs:1254,1279` vs `:1328-1334`
- **Status**: NEW
- **Description**: Two `return`s (CLI-screenshot claim path, screenshot poll deadline) fire after `render_one_frame` has already run, but before the `atw_*` field write at the handler tail — so those frames print fresh `rof_*`/`between_frames` values next to the *previous* frame's stale `atw_*` values.
- **Impact**: Confined to `--screenshot`/`byro-dbg`-screenshot frames — rare, but exactly when an operator is capturing evidence; can produce an apparently-impossible `atw_post < rof_pre_draw + rof_draw_call` reading that looks like nesting corruption rather than a skipped write.
- **Suggested Fix**: Hoist the three-field write into a helper called before each early return, or restructure to fall through to the tail.
- **Confidence**: High on mechanism; LOW severity because the affected frames are rare/non-representative.

#### PERF-D9-2026-09-11-03: `gpu_timers.rs` still documents itself as a 16-bracket/32-query pool after #4052 made it 17/34
- **Severity**: LOW (doc rot) · **Dimension**: Telemetry & Origin Cost
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:5,441,449,451`
- **Status**: NEW
- **Description**: #4052 added the ground-cover-bench bracket and correctly bumped `QUERIES_PER_FRAME` to 34, `active_bits` to `u32`, and added `BIT_GROUNDCOVER_BENCH` — but four prose counts in the same file's comments still say "32"/"sixteen" and now contradict the code beside them.
- **Impact**: Documentation only — runtime paths key off `QUERIES_PER_FRAME`/the bitmask correctly. Risk is a future reader trusting the stale prose while sizing a new bracket, in a file whose central hazard is reading a query slot that was never written.
- **Suggested Fix**: Replace the four literals with current counts, or word them to reference `QUERIES_PER_FRAME` directly so they can't rot; add a `const _: () = assert!(...)` cross-check.
- **Confidence**: High.

---

## Regression-Guard Verification (all dimensions)

Every named guard from the skill's per-dimension checklists was checked
against current source by its assigned agent. **None has eroded.** Summary:

| Dimension | Guards checked | Result |
|---|---|---|
| 1 — CPU Hot Paths | `drain_dirty_into` vs `take_dirty` (#1371), `make_animation_system`/`make_billboard_system` scratch (#1372/#1374), `build_debug_ui_snapshot` gating (#1376), `SkinSlotPool::sweep` contraction (#1379), `bone_world` steady-state reuse (#1794), dead-probe removal (#1803) | All 6 INTACT |
| 2 — Draw & Instancing | GT-presence hoist (#1377), opaque depth bucket (#3663), two-sided blend split (#1804/#2165), indirect-leader split-loss hazard, per-draw descriptor/push-constant/depth-bias churn, instanced batching collapse, parallel-sort threshold | All INTACT / disproven-as-hazard |
| 3 — GPU Memory Pressure | Deferred-destroy countdown/ordering (CRITICAL-class), dynamic BLAS budget, #1792 mid-batch gate, LRU smoothness, scratch reserve floors, mesh pool caps, BGSM/BGEM half-eviction, `NifImportRegistry` LRU | All INTACT |
| 4 — SSBO Sizing & Upload | `GpuInstance` 160 B / field-offset / no-re-expansion tests, `GpuLight` 64 B + GLSL lockstep, import-time-only PBR resolution, dirty bone-slot upload (#3665) | All 4 INTACT |
| 5 — GPU Pipeline | Legacy WRS compile-time gate (#1799, verified in shipped SPIR-V via `strings`), `NUM_RESERVOIRS` scope, CPU-precomputed `inv_vp`, depth-history copy gating (#3667, strengthened by #3836) | All 4 INTACT |
| 6 — Skinning & BLAS | Dedicated palette pass, persistent `bind_inverses`, dispatch-dirty gate (#1195), BLAS refit gate + jitter (#1196/#3669), `FAST_BUILD` flags, descriptor-rewrite skip (#1197), instrumentation (#1194/#3676), early-return rollback (#1791/#1796/#3569, evolved by #3991) | All 8 INTACT (one strengthened) |
| 7 — Streaming & Cells | Two-phase pre-parse (#877), small-batch fast-path (#1262), cross-request batch dedup (#3670), worker-thread mesh import (#3672), NIF/CDB cache hit-rate, NPC-spawn measurement (#1798), exterior + interior sub-cell budgets, batched exterior teardown | All 8 INTACT — zero findings this dimension |
| 8 — NIF Parse | `read_pod_vec` bulk-array collapse (#833) + big-endian gate, `allocate_vec` `#[must_use]` (#831), per-block counter allocation (#832), import-only particle extraction, skin-buffer pre-sizing (#3691) | All INTACT (one sibling gap found — see PERF-D8-2026-09-11-01) |
| 9 — Telemetry & Origin Cost | Non-blocking GPU timer readback, TAA/SVGF history survival across origin crossing, single-pass per-instance rebase, `between_frames_ms` anchor (#3674), scratch telemetry coverage | All 5 INTACT |

## Prior-Audit Re-Verification (`AUDIT_PERFORMANCE_2026-09-05.md`)

| Prior ID | Prior severity | Current status |
|---|---|---|
| `PERF-D1-2026-09-05-02` (combustion decode before `had_grid`) | LOW | **Fixed** (#3835) |
| `PERF-D1-2026-09-05-03` (`scene_has_effect_soft_material` ungated scan) | LOW | **Fixed** (#3836) |
| `PERF-D1-2026-09-05-04` (`frame_lights_scratch` error-path capacity loss) | LOW | **Fixed** (#3837) |
| `PERF-D1-2026-09-05-05` (`scene_trigger_actor_approach_system` deep-clone) | LOW | **Partially fixed** — outer-container reallocation gone; per-entity `ScenePlayer` clone remains → re-reported as `PERF-D1-2026-09-11-02` |
| `PERF-D1-2026-09-05-01` (VolumetricsPipeline full-array upload) | MEDIUM | Out of this session's Dimension-1 scope (Dim 1 agent's brief); not independently re-checked here |
| `PERF-D3-2026-09-05-01` (BLAS budget blind to resolution-scaled floor) | MEDIUM | **Fixed** (#3839/#3988/#3992) — `blas_budget_for_heap` now subtracts `screen_scaled_reservation_bytes`, re-derived at init and every swapchain recreate |
| `PERF-D3-2026-09-05-02` (mid-batch eviction accounting) | MEDIUM | **Fixed in substance** (#3840/#3979) — `pending_destroy_static_bytes` now exists and balances correctly; residual **reachability** gap re-reported as `PERF-D3-2026-09-11-01` |
| `PERF-D3-2026-09-05-03` (stale `shrink_tlas_to_fit` doc) | LOW | **Fixed** |
| `PERF-D3-2026-09-05-04` (`compute_blas_budget` doc orphaned) | LOW | **Fixed** (`fa5c4191` split) |

**8 of 9 carried-over items are fully or substantively fixed** — the codebase's
per-frame-allocation and GPU-memory-accounting discipline held up well between
2026-09-05 and 2026-09-11.

## Pre-existing OPEN issues re-confirmed present (not counted in the finding total)

- **#4050** (`SkinSlotPool::sweep` doesn't purge `pending_uploads`) — confirmed still present, Dimension 6.
- **#4049** (un-gated per-frame `warn!` on `bind_inverses` retry) — confirmed still present, Dimension 6.
- **#3813** (parallelize per-plugin ESM parsing) — confirmed still architecturally accurate, Dimension 8; no new angle to add.
- **#2764** (`order_dependent_glass` fragments opaque MultiLayerParallax batches) — same field as `PERF-D2-2026-09-11-02`, different consumer; not closed by this session's finding.
- **#3572** (TAA resolves only pre-composite direct HDR) — touches the same frame tail as `PERF-D5-2026-09-11-01`/`-02`, distinct claim, not addressed here.

## Coverage Gaps (stated explicitly, not silently skipped)

- **Dimension 5** deliberately did not read `aoUV`'s derivation (SSAO's 2-frame temporal lag could be raw screen-space UV vs. reprojected — a potential MEDIUM denoiser-ghosting finding if raw, but filing it without reading the derivation would itself violate the audit's no-unverified-premise rule). Also not reached: SVGF à-trous spatial-filter internals, and `build_and_upload_instances`/`assemble_camera_and_lights` internals beyond their `draw.rs` call sites. No GPU timings were taken this session — every bandwidth/ALU figure in Dimensions 1-9 is arithmetic from source, not measurement.
- **Dimension 1** did not independently re-check `PERF-D1-2026-09-05-01` (VolumetricsPipeline full-array upload) — out of this session's Dim-1 brief; status unknown, not re-verified either way.

---

## Prioritized Fix Order

**Tier 1 — real budget/correctness-adjacent gaps (do first)**

1. **Decouple BLAS mid-batch eviction/admission from the loop index** (`PERF-D3-2026-09-11-01`) — the only finding with a genuine (if unreproduced-on-dev-hardware) unbounded-growth failure mode on small-VRAM configurations.
2. **Byte-budget the texture-upload flush trigger** (`PERF-D3-2026-09-11-02`) — unbilled transient VRAM spike competing with the BLAS budget on exactly the hardware class that matters.
3. **Narrow the bone-palette dispatch to dirty regions** (`PERF-D6-2026-09-11-01`) — the highest quantified-waste finding (~100% waste in the common case), and it blunts an already-paid-for state machine (#3665/#1811).
4. **Pre-size `decode_bs_vertex_stream`'s tangent array** (`PERF-D8-2026-09-11-01`) — trivial fix, known-good pattern one file over, affects the dominant normal-mapped-mesh case across three games' worth of content.

**Tier 2 — cheap, local, measurable**

5. Fix the `NavPath` write-back clone across all six AI-locomotion systems (`PERF-D1-2026-09-11-01`) — one-line change × 6 files.
6. Reorder `pre_parse_cell`'s per-REFR allocations after the dedup check (`PERF-D8-2026-09-11-02`).
7. Correct the `atw_post`/`cpu_breakdown` nesting-contract docs (`PERF-D9-2026-09-11-01`) — doc-only, but it is the exact contract an operator uses to triage a stall.
8. Zero the `no_sorter` sort-key slot on opaque/additive branches (`PERF-D2-2026-09-11-01`).
9. Amortize `build_instance_map`'s per-frame `Vec` (`PERF-D2-2026-09-11-03`).
10. Lazily-size the two eagerly-allocated scene SSBOs, or record the 117.5 MB as an accepted cost (`PERF-D4-2026-09-11-01`) — do not ship the grow-machinery half without on-device measurement.
11. Complete `scene_trigger_actor_approach_system`'s clone elimination (`PERF-D1-2026-09-11-02`).
12. Hoist the redundant per-entity `cmd_bind_pipeline` in the skin dispatch loop (`PERF-D6-2026-09-11-02`) — bundle with the Tier-1 #3 change since it's the same loop.
13. Fix the screenshot-handshake early-return telemetry gap (`PERF-D9-2026-09-11-02`).

**Tier 3 — doc hygiene (cheap, and these are the premises the next audit will trust)**

14. Update `memory-budget.md`'s scene-buffer totals to ≈242 MB (`PERF-D4-2026-09-11-02`).
15. Fix `CLAUDE.md`'s stale ACES/composite attribution (`PERF-D5-2026-09-11-01`) — highest-value doc fix in this tier: it manufactures a plausible false-positive MEDIUM finding for the next auditor.
16. Update `gpu_timers.rs`'s bracket-count prose (`PERF-D9-2026-09-11-03`).
17. Re-label the per-REFR "Cell BLAS batch" log line (`PERF-D3-2026-09-11-03`) — this is what made finding #1 in this list non-obvious; fix alongside it.
18. Re-capture the FO4 baseline's inconsistent `bench_draws_*` pair and add the cross-row invariant to `/audit-runtime` (`PERF-D2-2026-09-11-05`).

**Tier 4 — structural completeness gaps, narrow/unmeasured impact (defer pending measurement)**

19. `PERF-D2-2026-09-11-02` (`preserve_opaque_gbuffer` sort-key axis) — re-evaluate after #2764 is fixed.
20. `PERF-D2-2026-09-11-04` (raster-sort key-caching) — measure with the existing bench harness before changing anything.
21. `PERF-D4-2026-09-11-03` (material-hash key production cost) — only act if profiling shows it on critical path; the naive fix is a correctness hazard.
22. `PERF-D5-2026-09-11-02` (unconditional FSR mask attachments under TAA) — document only; a real fix needs RenderDoc + validation-layer evidence on both upscaler modes before it can ship.
