# ByroRedux Performance Audit — 2026-07-02

**HEAD**: `1b4e8e84` (post-Session-53 closeout — CHARAL character abstraction
layer + audit bug-bash). **Depth**: deep, all 9 dimensions.
**Hardware target**: RTX 4070 Ti (12 GB) + Ryzen 7950X (16c/32t); RT VRAM
minimum 6 GB; RT budget total under ~4 GB. A CPU bottleneck on this machine
is a bug, not a tuning gap.

## Scope note — re-verification pass against an unchanged tree

The immediately prior performance audit — `docs/audits/AUDIT_PERFORMANCE_2026-07-01.md`
— ran against **this exact commit** (`1b4e8e84`); `git diff 1b4e8e84 HEAD` is
empty. This audit is therefore not a fresh divergent sweep (the source it would
sweep is byte-identical) but a **re-verification pass**: every finding the
2026-07-01 audit raised was independently re-derived from live source by three
parallel dimension agents (D1/D6, D3/D4, D2/D5/D7) plus direct spot-checks,
with the explicit goal of *disproving* each finding per `_audit-common.md`
methodology. Two prior findings did not survive that scrutiny unchanged (one
downgraded to PARTIAL, one whose original STALE-verdict during this pass was
itself a false negative and is re-confirmed). Every Session 46 / R1 / NIFAL /
skinning-gate guard enumerated by the skill was re-confirmed **INTACT**. No new
findings beyond the 2026-07-01 set were discovered — expected, since the code
has not moved.

The net actionable set carried forward from 2026-07-01, re-verified here, is
**23 findings** (1 HIGH, 9 MEDIUM, 13 LOW), with the two corrections noted
above folded in.

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 |
| MEDIUM   | 9 |
| LOW      | 13 |
| **Total**| **23** |

Zero CRITICAL. One HIGH (**D6-01**): a real-but-narrow correctness/perf hole in
the skinning-pool bind-inverse commit protocol. The other 22 are MEDIUM/LOW
efficiency gaps. **No landed guard has eroded** — every guard the skill lists
across all 9 dimensions was re-confirmed present against current code
(evidence table below). The finding set is unchanged from 2026-07-01 because
the tree is unchanged; the value of this pass is the independent
re-confirmation and the two premise corrections.

### Corrections to the 2026-07-01 report (applied here)

| Finding | 2026-07-01 verdict | This pass | Basis |
|---|---|---|---|
| **PERF-D1-NEW-02** (per-frame `env::var`) | LOW, "hot paths" (3 sites) | **LOW, PARTIAL** — 2 live per-frame sites (`render/mod.rs:333`, `static_meshes.rs:138`), both `var_os` (no alloc). The `render/mod.rs:57` site is *already* `OnceLock`-cached (`apply_fog_overrides`) → not a violation. | Direct read of all three cited sites. |
| **D6-02** (pose-hash commit ordering) | MEDIUM, cites `draw.rs:1711-1732` as the "hash commit" | **MEDIUM, CONFIRMED — line pointers corrected**: the hash commit is CPU-side in `render/skinned.rs:180` (`try_mark_pose_dirty`); `draw.rs:1711` is the *consumer* (skip gate), not a commit. No `last_pose_hash` write exists anywhere in `crates/renderer/src`. | grep for `last_pose_hash` in renderer = 0 hits. |

> During this pass one agent initially flagged **PERF-D1-NEW-03** STALE
> ("`emit_particles` doesn't exist") — this was a false negative from grepping
> only `byroredux/src/systems/`. `emit_particles` lives in
> `byroredux/src/render/particles.rs:37`; the dead probe `let _ = gtq.get(entity);`
> at `particles.rs:55` is present and `gtq` is used nowhere else in the
> function. **PERF-D1-NEW-03 is re-confirmed NEW/LOW.**

### Bench-of-Record delta (observed vs ROADMAP — not absolute FPS)

Per the skill's Bench-of-Record policy, no FPS/ms/fence numbers are hardcoded.
ROADMAP.md's **Bench-of-record** block (R6a-stale-14, HEAD `1c26bc25`,
2026-06-03) is self-flagged stale and predates both the Session 47
camera-origin work and the Session 49 RT-denoiser overhaul (#1662) — the
passes Dimension 5 finds the most cost in. **R6a-stale-15 (a fresh three-scene
300-frame GPU bench on Prospector / WhiterunBanneredMare / MedTekResearch01)
is overdue** — it is the only way to confirm whether the Session 49 denoiser
changes moved per-pass GPU cost, and several D5 findings bear directly on that
pass. This is a process recommendation, not a code finding.

---

## Guard Re-Verification (all INTACT)

| Guard | Status | Evidence |
|---|---|---|
| `drain_dirty_into` preserves storage capacity (#1371) | INTACT | `crates/core/src/ecs/packed.rs:73`, regression test `:693` |
| Animation scratch Vecs reused via clear+extend (#1372) | INTACT | `byroredux/src/systems/animation.rs:435-444` |
| Billboard `last_cam` short-circuit (#1374) | INTACT | `byroredux/src/systems/billboard.rs:23,56-59` |
| Bone-pool `next_slot` contracts on sweep (#1379) | INTACT | `crates/core/src/ecs/resources.rs:697-777` |
| `pose_dirty` dispatch gate (#1195) on `SkinSlotPool` | INTACT (commit-ordering hole = D6-01/D6-02) | `resources.rs:724,949,965`; consumer `draw.rs:1711` |
| GpuInstance = 112 B, offsets pinned (R1) | INTACT (no drift) | `gpu_types.rs:57-110`; `gpu_instance_layout_tests.rs:26,71` |
| PBR resolved once at import, not per-draw (NIFAL) | INTACT | `material.rs:638` `resolve_pbr`; `classify_pbr_keyword:449` not in draw loop |
| `read_pod_vec` / `allocate_vec` `#[must_use]` (#831/#833) | INTACT | `crates/nif/src/stream.rs:252,349` |
| `upload_instances` / `upload_materials` content-hash dirty gate | INTACT | `upload.rs:500-503,558-561` |
| Parallel-sort gate at `>= 2000` draw commands | INTACT | `byroredux/src/render/mod.rs:417` |
| GT-presence hoist in static-mesh loop (#1377) | INTACT | `byroredux/src/render/static_meshes.rs:147` |
| Additive-particle mesh-before-depth sort (#1649) | INTACT | `draw_sort_key` transparent branch; `draw_sort_key_tests.rs` |
| Mid-batch eviction TRIGGER at 90% / 64-build interval | Trigger INTACT, **effect eroded** (see PERF-D3-NEW-01) | `blas_static.rs:551-567` vs callee gate `:1115` |
| Deferred-destroy countdown = MAX_FRAMES_IN_FLIGHT (2) | INTACT | `crates/renderer/src/vulkan/acceleration/` (#1449) |

The single "eroded" row (mid-batch eviction *effect*) is **not a regression of
a landed guard** — the trigger has never functioned end-to-end; it is captured
as the NEW finding PERF-D3-NEW-01, not a guard erosion.

---

## Findings

### HIGH

#### D6-01: First-sight `bind_inverses` are drained from the pool before `draw_frame` commits them — an early return permanently corrupts the entity's skinning palette
- **Severity**: HIGH
- **Dimension**: Skinning & BLAS
- **Location**: `byroredux/src/main.rs:1759` (drain) → `main.rs:1806` (draw_frame call) vs `crates/renderer/src/vulkan/context/draw.rs:2031,2118,2148,2243,2259` (early returns preceding the commit) vs `draw.rs:2643-2662` (actual upload)
- **Status**: NEW (related: #1192 fixed a different loss vector inside `upload_pending_bind_inverses`; this is upstream, at the pool-drain call site)
- **Description**: `render_one_frame` calls `self.skin_slot_pool.drain_pending(...)` at `main.rs:1759`, which irrevocably removes entries from `SkinSlotPool::pending_uploads` (`crates/core/src/ecs/resources.rs:850-853`, `drain(..n).collect()`), *before* invoking `ctx.draw_frame(...)` at `main.rs:1806`. `draw_frame` has multiple early returns preceding the bind-inverse upload at `draw.rs:2643`: empty framebuffers (`Ok(false)` @2031), `ERROR_OUT_OF_DATE_KHR` on acquire (`Ok(true)` @2118 — fires on every resize / mode change), and fence/reset/begin error arms (@2148/2243/2259). On any of these, the drained first-sight `bind_inverses` are dropped; the caller's `Ok(true)/Ok(false)` arms (`main.rs:1828-1882`) perform no re-queue. `entity_to_slot` keeps the slot resident, so `allocate()` never re-queues the upload, and the persistent SSBO region for those slots is never written.
- **Evidence**: Call path `main.rs:1759 drain_pending` → `main.rs:1806 draw_frame` → `draw.rs:2118 return Ok(true)` (upload at 2643 unreached) → `pending_with_data` dropped, no re-queue. `skin_palette.comp` then computes `palette[slot] = bone_world[slot] × <uninitialized>` for the affected slots, consumed by both `triangle.vert` (set 1 binding 3) and `skin_vertices.comp`; the garbage skinned vertices feed the per-entity BLAS and TLAS.
- **Impact**: Skinned entity (NPC body part) renders as garbage geometry in **both** raster and RT and pollutes the TLAS with degenerate triangles for the entity's remaining lifetime in the cell — recovery only via despawn + 3-frame pool sweep + respawn. Trigger requires a first-sight frame (NPC spawn / cell load) to coincide with a swapchain-out-of-date frame (resize, fullscreen toggle) — narrow but real, most likely during startup cell loads where window setup and streaming overlap.
- **Related**: #1192 (sibling loss vector, fixed), D6-02 (same root cause).
- **Suggested Fix**: Make the drain transactional — move `drain_pending` inside `draw_frame` past the last early return, or have `draw_frame` report "skin section reached" and re-queue the drained `(slot, entity)` pairs into `pending_uploads` on any path that returned before the upload.

---

### MEDIUM

#### PERF-D3-NEW-01: Mid-batch BLAS eviction (#510) is structurally a no-op — `evict_unused_blas` gate ignores the batch's pending bytes (OOM-on-first-huge-cell risk)
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:551-567` (mid-batch trigger, correctly fed `pending_bytes`) + `blas_static.rs:1115,1156` (callee gates, blind to `pending_bytes`); `predicates.rs:382-390`
- **Status**: NEW (defect in the #510 fix; distinct from #740, which fixed the frame-counter aspect)
- **Description**: The mid-batch trigger `should_evict_mid_batch(total_live_bytes, pending_bytes, budget)` correctly projects `live + pending` against 90% of budget. But `evict_unused_blas` (`:1106-1195`) gates purely on committed state: `if self.static_blas_bytes <= self.blas_budget_bytes { return; }` (`:1115`) and the per-candidate break (`:1156`) test the same committed-only compare. `pending_bytes` is a local in `build_blas_batched`, never threaded into `evict_unused_blas`. The batch's result buffers are created inline (`:621`) but only added to `static_blas_bytes` in Phase 7 (`:1029-1030`), after the batch completes. So mid-batch: either the previous cells were already under budget (evict early-returns, freeing nothing) or they weren't (pre-batch pass at `:533` already handled them). **On a fresh single large cell load (`static_blas_bytes == 0` at start), a batch that individually overshoots the whole budget allocates every result buffer with zero intervening eviction** — the budget is only ever enforced retroactively on the *next* cell.
- **Evidence**: `should_evict_mid_batch`'s #510 doc comment describes the intended pause-and-evict; the callee's committed-only gate defeats it. `memory-budget.md` documents the trigger but not the (nil) effect.
- **Impact**: A single large batch (initial exterior grid load, FO4 precombine-heavy cell) allocates its full result-buffer footprint above budget with no mid-batch relief. On the RT-minimum 6 GB device (budget ≈ 2 GB) the intended pause never lands; failure mode is allocator pressure / a graceful cell-load bail (cleanup verified), not device loss. Unreachable on the 12 GB dev card with vanilla content.
- **Related**: #510, #740, #915, PERF-D3-NEW-02.
- **Suggested Fix**: Thread `pending_bytes` into `evict_unused_blas` (or add `evict_down_to(target_bytes)`) so both the early-return gate and the loop break test `static + pending` against the 90% line the trigger already computes.

#### PERF-D3-NEW-02: Budget eviction has no rebuild path — evicted static BLAS drop out of RT permanently, and multi-cell load bursts age not-yet-drawn BLAS into candidacy
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:150-158` (missing rigid BLAS → count + skip only); `blas_static.rs:425-431` (per-batch `frame_counter` bump) + `:1129,1156-1175` (`idle ≥ MIN_IDLE_FRAMES` candidacy + slot clear)
- **Status**: NEW
- **Description**: Two coupled gaps. (1) No recovery: `build_blas_batched` is invoked only from cell-load / scene-load sites (`cell_loader/exterior.rs:349`, `cell_loader/spawn.rs:1179`, `scene/nif_loader.rs:1043`, `cornell.rs`, `scene.rs:371`) — never per-frame. In `build_tlas` a rigid draw whose `blas_entries[mesh_handle]` is `None` hits `missing_rigid_blas += 1; … continue;` — warns (rate-limited) and skips forever; the mesh keeps rasterizing but permanently vanishes from shadows/reflections/GI until its cell unloads and reloads. (2) Burst aging: a synchronous multi-cell load (`--grid` radius 3 = 49 batched calls before the first frame) leaves cell #1's just-built, never-yet-drawn entries at high idle on the first `build_tlas` — prime LRU victims if cumulative `static_blas_bytes` crosses budget mid-load, since eviction picks oldest-first.
- **Evidence**: `tlas.rs`'s own comment — "an LRU eviction got something the draw still references; should be near-zero in steady state" — the #1228 counter exists precisely because there is no re-acquire path.
- **Impact**: Silent RT-correctness degradation (missing occluders → wrong shadows/GI), gated on `static_blas_bytes > budget` — unreachable on the 12 GB dev card with vanilla content, plausible on 6-8 GB devices with heavy exteriors / mod load-orders. Crash-safe (deferred destroy, #1449); recovery requires a cell round-trip. Steady-state single-cell-per-frame streaming is protected (drawn entries idle ≤2 < MIN_IDLE); exposure is multi-batch bursts between frames.
- **Related**: #920, #740, #1228, #1449, PERF-D3-NEW-01.
- **Suggested Fix**: On a `missing_rigid_blas` hit in `build_tlas`, queue the mesh handle for a lazy `build_blas_batched` next frame (mirroring the skinned first-sight path). Cheaper stopgap: stamp the batch's own entries with a post-batch tick so an in-burst load cannot age its own cells into victims.

#### PERF-D4-NEW-01: Per-frame `bone_world` fill + upload is fixed-stride O(used_slots × 144) — pays the full `MAX_BONES_PER_MESH` reservation per skinned mesh every frame; the code comment defers to an already-closed milestone
- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:178-256` (`upload_bone_worlds` + `record_bone_world_copy`), `byroredux/src/render/skinned.rs:131-181`, `byroredux/src/render/mod.rs:308`, `crates/core/src/ecs/components/skinned_mesh.rs:52` (`MAX_BONES_PER_MESH = 144`)
- **Status**: NEW (untracked debt: the in-code comment defers to "variable-stride packing (M29.5)", but ROADMAP marks M29.5 Closed with a narrower scope — GPU palette dispatch only — so no live tracker owns the packing work)
- **Description**: Each `SkinnedMesh` slot occupies a fixed 144 × 64 B = 9216 B stride in `bone_world`, regardless of actual bound-bone count. Per-frame cost is three-fold, all O(used_slots × 144): CPU `bone_world.clear()` + `resize(required_slots, IDENTITY)` re-fills the whole array from empty every frame (`render/mod.rs:308`, `skinned.rs:131`); host-visible staging memcpy + flush of the full range (`upload.rs:190`); GPU `cmd_copy_buffer` of the same bytes plus a `skin_palette.comp` dispatch sized from it. Per-entity *writes* are bounded by `min(skin.bones.len(), 144)`, but the allocation, upload byte-count, and copy are the full 144-stride per used slot.
- **Evidence**: `skinned.rs:131` — `required_slots = (max_used_slot()+1) * MAX_BONES_PER_MESH`; resize always fills from empty since `bone_world` is cleared at `render/mod.rs:308`; `upload.rs:190` sizes byte count from the full strided array length.
- **Impact**: Scales with skinned-entity density, not bone count. At the project's own measured 260-entity FNV workload: ≥261 slots × 9216 B ≈ 2.4 MB/frame ≈ 144 MB/s sustained host-write + flush + GPU copy at 60 fps, most of it identity padding. Full-pool worst case (1365 slots) ≈ 12.6 MB/frame ≈ 755 MB/s. No dirty gate applies (animation legitimately changes every frame), so unlike instances/materials this cost is unconditional.
- **Related**: #1284, M29.5 (closed, narrower scope), memory-budget.md bone-palette row.
- **Suggested Fix**: Variable-stride packing — prefix-summed offsets sized by each mesh's actual bone count (or quantized buckets). Cheaper interim: build per-slot `vk::BufferCopy` regions covering only `skin.bones.len()` matrices. File a live issue so the debt stops pointing at a closed milestone.

#### D2-NEW-02: Per-particle unquantized LERPed color defeats `MaterialTable` dedup — one fresh `GpuMaterial` per live particle per frame
- **Severity**: MEDIUM
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/particles.rs:74-82,138-139,208-209`; `crates/renderer/src/vulkan/context/mod.rs:509,521-523` (`material_hash` over raw `emissive_*` bits)
- **Status**: NEW
- **Description**: Each particle's color is LERPed against `t = age/life` (unquantized, `particles.rs:74`) and folded into `emissive_color`/`emissive_mult` (`:138-139`), both material-table fields post-R1. `material_hash` hashes `emissive_mult.to_bits()` + `emissive_color[..].to_bits()` — raw f32 bits, zero quantization (`context/mod.rs:521-523`) — so `intern_by_hash` (`:208`) takes the miss path per particle every frame: full `GpuMaterial` build + FxHashMap insert + table push + upload, inverting the ~97% dedup-hit rate the #781 fast path assumes. Instancing is unaffected (`material_id` is per-instance); this is the residual after #1649 fixed the depth-vs-mesh sort ordering.
- **Evidence**: `particles.rs:74` continuous `t`; `:138-139` emissive writes; `:208` intern; `context/mod.rs:521-523` raw-bits hash covers the emissive fields, so distinct colors never dedup.
- **Impact**: Scales with live particle count. FX-heavy scenes (20-30 emitters, 96-256 particle caps each) can reach ~5-8K unique materials/frame ≈ 1.5-2.3 MB/frame upload plus CPU churn, stacking toward the `MAX_MATERIALS = 16384` cap where overflow silently routes particles to neutral material id 0 (wrong color). Also permanently depresses dedup-ratio telemetry, masking real dedup regressions elsewhere.
- **Related**: #1649, #781, #780, #797.
- **Suggested Fix**: Quantize the fade parameter before the color LERP (e.g. 32 steps — imperceptible on additive billboards). Same-emitter particles then collapse to ≤32 materials.

#### D6-02: Pose hash is committed at `build_render_data` time — a `draw_frame` early return freezes the RT skinned pose while the dirty gate reads "clean"
- **Severity**: MEDIUM
- **Dimension**: Skinning & BLAS
- **Location**: `byroredux/src/render/skinned.rs:152,180` (clear + mark = the hash commit) + `crates/renderer/src/vulkan/context/draw.rs:2118` (early return) + `draw.rs:1711-1715` (consumer skip gate)
- **Status**: NEW (related: #1195 introduced the gate; this is a commit-ordering hole)
- **Description**: `try_mark_pose_dirty(entity, hash)` records the new pose hash the moment `build_skinned_palettes` runs — CPU-side, in `build_render_data`, before `draw_frame`. **Corrected line pointers:** the commit is `render/skinned.rs:180` (into `last_pose_hash` on the ECS `SkinSlotPool`, `resources.rs:949-966`); `draw.rs:1711` is the *consumer* (`pose_dirty.contains(entity)` skip gate), and there is **no** `last_pose_hash` write anywhere in `crates/renderer/src`. If `draw_frame` early-returns before the skin dispatch (swapchain out-of-date @2118, empty framebuffers), the dispatch never runs but the hash baseline has already advanced. Sequence: frame N-1 dispatches pose P1 (H1); frame N computes P2 → H2 recorded dirty; `draw_frame` N early-returns; frame N+1 the NPC stops → H2 matches stored H2 → gate reads "not dirty" → dispatch + refit skipped with `has_populated_output == true`. The slot output and skinned BLAS stay at P1 while the raster palette (recomputed every frame from `bone_world`) shows P2.
- **Evidence**: `skinned.rs:180` runs unconditionally in `build_render_data`; grep for `last_pose_hash` in the renderer crate = 0 hits, so nothing rolls it back when `draw_frame` fails to reach the skin section.
- **Impact**: RT shadows/reflections/GI of the affected NPC freeze at a pose one-plus frames stale relative to the rasterized body, persisting through the idle period after the lost frame. Self-healing on next movement; no crash, no leak.
- **Related**: D6-01 (same root cause), #1195.
- **Suggested Fix**: Same transactional shape as D6-01 — stage the frame's pose hashes and fold into `last_pose_hash` only after `draw_frame` confirms the skin section ran, or re-insert the frame's dirty set on early-return paths.

#### D6-03: All skinned BLAS builds/refits in a frame are serialized on one shared scratch buffer — zero build overlap on multi-NPC frames
- **Severity**: MEDIUM
- **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:417` (per-refit barrier), `:278-283` (per-build barrier in first-sight batch); consumed at `context/draw.rs:1835-1899`
- **Status**: NEW (the barrier *correctness* chain — #642/#644/#983/#1095/#1140/#1300 — is complete and intact; nothing tracks the throughput cost of the serialization)
- **Description**: `blas_scratch_buffer` is a single allocation sized to the max single-build demand. Because every skinned BLAS build/refit reuses the same scratch address, the Vulkan spec requires an AS_WRITE→AS_WRITE barrier between each pair — N dirty skinned entities produce N fully serialized AS builds per frame, each self-emitting the barrier. Small skinned BVHs (5-15K triangles per body part) individually underutilize the GPU; back-to-back serialization with full-pipe AS-stage drains prevents overlap.
- **Evidence**: `refit_skinned_blas`'s first statement is `record_scratch_serialize_barrier`; the refit loop calls it once per dirty entity; scratch sizing is grow-to-max-single-build, not per-build slots.
- **Impact**: GPU skin-chain time scales linearly with dirty-entity count with no overlap. On crowd scenes the `gpu_skin_blas_refit_ms` bracket absorbs the full serial sum plus per-barrier drain. Idle crowds are already saved by #1195/#1196; this is the moving-crowd ceiling only.
- **Confidence**: Quantify before fixing — the #1194 GPU timer brackets exist for this (`skin.coverage` → `gpu_skin_blas_refit_ms` vs `refits_attempted`).
- **Related**: #642, #983, #1300 (correctness chain), #1194 (measurement hook).
- **Suggested Fix**: Sub-allocate the scratch buffer into K aligned slots, round-robin builds, emit the serialize barrier only every K builds; K=1 fallback under memory pressure.

#### D7-NEW-01: Interior NPC/REFR spawn loop has no per-frame or per-NPC budget, unlike the exterior streaming path
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/references.rs:224` (`load_references` ref loop) → `byroredux/src/npc_spawn.rs` (`spawn_npc_entity` @319, `spawn_prebaked_npc_entity` @354); driven from `byroredux/src/cell_loader/load.rs:301` (`load_cell_with_masters`) and `cell_loader/transition.rs:237` (`load_interior_cell`)
- **Status**: NEW
- **Description**: The exterior streaming path has an explicit per-frame *cell-count* budget (`MAX_CELLS_SPAWNED_PER_FRAME = 2`, `main.rs:1181`, enforced `main.rs:1212-1227`) to avoid frame-time spikes from bulk main-thread spawning. No equivalent exists for interior transitions: `load_references` iterates every `PlacedRef` in one synchronous `for placed_ref in refs` pass, spawning each static + NPC inline — no batching, no yield, no cap. **Nuance surfaced this pass**: even the exterior "budget" is cell-granularity — each individual cell's `load_references` call (also reached via `cell_loader/exterior.rs:403`) is itself an unbudgeted burst. So the true gap is "no *sub-cell* spawn budget anywhere"; the interior path additionally lacks even the cell-level throttle because it is a single-cell blocking load. `spawn_npc_entity` makes ~28 synchronous NIF-load call sites per NPC.
- **Evidence**: `main.rs` documents the exterior budget's rationale but it is never applied to `load_references`/`spawn_npc_entity`; no `Instant::now()` timing exists in `cell_loader/load.rs` or `references.rs` to even measure the stall. **Same-root-cause sibling**: `references.rs` ends the cell load with a single synchronous batched texture flush + fence-wait (`flush_pending_uploads`, ~`references.rs:969-987`) — an intentional batching win (#881), but on top of the unbudgeted spawn loop it means a large interior cell pays its entire NIF-parse + spawn + BLAS-build + texture-upload cost in one frame with a hard fence stall and no yield.
- **Impact**: An unmeasured multi-hundred-ms-to-multi-second frame-time spike on every interior transition into an NPC-dense cell (door walk-in, save-load-apply reload, fast travel). Architecturally distinct from and precedes #1698 (a *post-load* Rapier/ECS-scheduler settle-storm confirmed by `docs/audits/AUDIT_RUNTIME_2026-06-26.md`) — the two compound on entry to a crowded cell (load-time freeze, then post-load stall) but are separate mechanisms.
- **Related**: Existing: #1698 (adjacent, not a duplicate — post-load); #881 (the batched texture flush).
- **Suggested Fix**: Extend a `MAX_CELLS_SPAWNED_PER_FRAME`-style budget to interior NPC spawning by chunking `load_references`'s ref loop across frames with a resumable cursor; at minimum add `Instant::now()` timing around the NPC-spawn portion so the cost becomes visible before investing in chunking.

#### PERF-D5-NEW-01: Legacy 16-slot WRS reservoir arrays stay live on the default ReSTIR path — dead per-thread storage in the frame's hottest shader
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/triangle.frag:1967-1983` (declaration + unconditional init), `:2237-2250` (legacy streaming writes), `:2558-2673` (legacy pass-2 reads)
- **Status**: NEW (related: #1369 — CLOSED, the precedent this re-erodes)
- **Description**: #1369 retired a larger reservoir array (dropping the third `resRadiance` array, 320 B → 128 B) because per-thread reservoir storage was "the dominant per-thread footprint suppressing WRS occupancy," landing at `resLight[16]` + `resWSel[16]`. Session 49 then made ReSTIR-DI (a single scalar reservoir) the default shadow path, but kept the 16-slot legacy WRS arm compiled in for a runtime A/B toggle (`DBG_DISABLE_RESTIR`, a dynamically-uniform branch, not a compile-time constant). The arrays are declared and zero-initialized *before* `useRestir` is even computed (`:1980-1983` init vs `:1998` compute), so the compiler must budget their registers/local memory on every invocation — including the ~100% of production frames that take the ReSTIR path and never read them.
- **Evidence**: `NUM_RESERVOIRS = 16` at `:1967`; unconditional init loop `:1980-1983`; `useRestir` at `:1998` from a runtime-uniform flag; only the `!useRestir` path touches the arrays afterward.
- **Impact**: Up to ~32 extra live registers (or spilled local bytes) per fragment thread in a shader that already carries the full RT uber-path — silently re-eroding a portion of the #1369 occupancy win on a path that gets zero benefit. Blast radius: every lit fragment, every frame, every game. (Footprint smaller than a naive read: #1369 already halved the array set.)
- **Confidence**: MEDIUM — storage-lifetime analysis is code-verified; the *magnitude* of the occupancy hit needs Nsight/RenderDoc SASS confirmation. ALU/register-only, no pipeline-state/barrier change, so the speculative-Vulkan caveat does not block acting on it — bench before/after.
- **Related**: #1369, `DBG_DISABLE_RESTIR` toggle.
- **Suggested Fix**: Promote the legacy-WRS arm to a compile-time toggle through the existing generated-constants channel (the mechanism #1758 used for skin workgroup size); A/B then costs a shader recompile instead of taxing every production frame.

#### PERF-D5-NEW-02: One-bounce-GI hit irradiance samples the first 8 lights in upload order, not the 8 relevant to the hit point
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/include/lighting.glsl:178-192` (`giHitIrradiance` fixed-prefix loop + post-hoc `dist > radius` skip), `include/shader_constants.glsl:39` (`GI_HIT_LIGHT_CAP = 8u`); light upload order `byroredux/src/render/lights.rs:73-160`
- **Status**: NEW (landed with "true one-bounce GI"; not covered by the 2026-06-16 or 2026-06-23 audits)
- **Description**: `giHitIrradiance` loops `count = min(lightCount, GI_HIT_LIGHT_CAP)` over the global light SSBO **prefix** in upload order. `collect_lights` uploads the cell directional first, then point lights in arbitrary ECS sparse-set iteration order — proximity-blind. A `dist > radius` skip exists but only *after* the fixed-prefix selection, so lights past index 8 are never considered. In any cell with >8 lights (taverns, Whiterun/Dragonsreach-class interiors), the GI bounce permanently ignores every light past index 8 while still firing up to 8 shadow rays against a fixed prefix that may be entirely out of range of the hit point. The primary-fragment path solved this exact problem with clustered culling + RIS; the bounce path regressed to an unsorted prefix.
- **Evidence**: `lighting.glsl:178` `count = min(lightCount, GI_HIT_LIGHT_CAP)`; index order = upload order, confirmed by `lights.rs`'s plain ECS-iteration push loop.
- **Impact**: Two-sided — quality (bounce lighting in >8-light cells is systematically wrong/dim and can flicker across cell reloads as ECS iteration order changes which 8 lights exist for GI) and efficiency (up to 8 ray-query traces per lit fragment — the single largest per-pixel ray budget in the frame — spent on an unprioritized set). The per-pixel cap is doing its perf job; this is about spending that budget on the wrong lights.
- **Confidence**: HIGH on the premise (both sides code-verified); impact magnitude is scene-dependent.
- **Related**: `GI_HIT_LIGHT_CAP` comment.
- **Suggested Fix**: Prioritize the prefix CPU-side (sort `gpu_lights[1..]` by intensity·radius, one small sort/frame) so "first 8" approximates "8 most influential"; or select per-hit by distance with an early-out after 8 contributing lights. Keep the ray-count cap unchanged.

---

### LOW

#### PERF-D1-NEW-01: `about_to_wait` runs a full MeshHandle+TextureHandle dedup walk every frame for on-demand-only telemetry
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/main.rs:2262-2286`
- **Status**: NEW
- **Description**: Every frame, `about_to_wait` iterates the entire `MeshHandle` and `TextureHandle` storages and inserts each non-zero handle into a persistent `HashSet` (allocation-free since #1584) to compute `meshes_in_use`/`textures_in_use` dedup counts. The only consumers are the `stats` console command and the debug-server entity evaluator — both on-demand, neither per-frame; `log_stats_system` (1 Hz) doesn't print these fields.
- **Evidence**: `main.rs:2266-2281` runs unconditionally before the scheduler each frame.
- **Impact**: CPU cost scaling linearly with mesh-entity count — plausibly ~1 ms/frame on a dense exterior grid or Skyrim city, a double-digit share of the frame budget at the 300+ FPS bench rates this engine targets, spent on a stat nobody reads that frame. No quantitative guard exists for this site.
- **Related**: #1584, #637.
- **Suggested Fix**: Throttle to the diagnostics cadence (every-16-frames or 1 Hz, matching `log_stats_system`), or compute lazily inside the two on-demand consumers.

#### PERF-D1-NEW-02: Per-frame process-environment lookups in two hot-path sites (PARTIAL)
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/render/mod.rs:333` (`BYRO_PROFILE`), `byroredux/src/render/static_meshes.rs:138` (`BYRO_NO_CULL`)
- **Status**: NEW (PARTIAL — narrower than the 2026-07-01 framing)
- **Description**: Two live per-frame sites call `std::env::var_os(...)` instead of caching once, unlike the sibling `apply_fog_overrides` (`render/mod.rs:52-71`) which caches via `OnceLock` "so the hot path doesn't `getenv` per frame." Both are `var_os(...).is_some()` — no heap allocation. **Correction vs 2026-07-01**: the third originally-cited site (`render/mod.rs:57`) is *inside* `apply_fog_overrides` and already `OnceLock`-cached — not a violation. The `BYROREDUX_FIXED_DT` reference is at the `about_to_wait` timing site, not these two. Env vars cannot change mid-process, so caching both is semantics-preserving.
- **Impact**: Sub-µs each; consistency/hardening more than a measurable bottleneck. No quantitative guard exists for these sites.
- **Related**: `apply_fog_overrides`'s `OnceLock` pattern.
- **Suggested Fix**: Hoist both into a `OnceLock`, mirroring `apply_fog_overrides`.

#### PERF-D1-NEW-03: `emit_particles` acquires GlobalTransform and performs a dead per-emitter probe every frame
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/render/particles.rs:48-55`
- **Status**: NEW (re-confirmed this pass after an initial false-negative STALE call — see the correction note in the summary)
- **Description**: `emit_particles` (`render/particles.rs:37`, NOT `systems/particle.rs`) hard-requires a `GlobalTransform` query (`:48-53`) and then, per emitter per frame, executes a discarded `let _ = gtq.get(entity);` at `:55` with a comment claiming the transform is "sampled by the system at spawn." `gtq` is used **nowhere else** in the function (verified: only two references, the query bind and the discarded probe) — `emit_particles` reads particle world positions directly from `em.particles.positions`.
- **Impact**: Micro (emitter counts are small); primarily misleading dead code + a wasted per-emitter SparseSet get. No quantitative guard exists for this site.
- **Related**: `particle_system` (`byroredux/src/systems/particle.rs:317-325`) is the *real* transform consumer (there the `get` is not dead).
- **Suggested Fix**: Delete the `gtq` acquisition and the dead probe; take only the `ParticleEmitter` query.

#### D2-NEW-03: Two-sided glass split runs on additive particle batches — 2× draws + a fully-culled vertex pass with zero compositing benefit
- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1086-1088,1159-1172`; `byroredux/src/render/particles.rs:96`
- **Status**: NEW
- **Description**: Particles emit `two_sided: true` + `alpha_blend: true` (`particles.rs:96`), so every particle batch hits `needs_split = is_blend && two_sided` and dispatches twice (FRONT-cull then BACK-cull), excluded from indirect grouping. The split stabilizes TAA depth-winner flips on volumetric glass — a rationale requiring depth writes + order-dependent compositing. Particles have `z_write: false` and (post-#1649) the dominant presets are additive (order-independent); billboards are camera-facing, so the FRONT-cull pass rasterizes ~nothing while still shading the whole instanced batch.
- **Impact**: 2× draw calls and 2× vertex invocations for all live particles; batch counts are small post-#1649 so absolute cost is minor, but the first pass is provably dead work for the additive/no-depth-write case.
- **Related**: #1649, glass-split design (Tier C plan).
- **Suggested Fix**: Narrow the split predicate, e.g. `needs_split = is_blend && two_sided && z_write`, or exclude order-independent blends (`dst == ONE`).

#### D2-NEW-04: Static-mesh hot loop pays a redundant GlobalTransform re-probe and a late IsFxMesh gate
- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/static_meshes.rs:147,175,280`
- **Status**: NEW
- **Description**: Two skip-ordering inefficiencies in the per-entity draw enumeration: the #1377 hoist probes `tq.get(entity).is_none()` (`:147`) then re-fetches the same component at `:175`, two storage lookups per drawn entity where one binding would do; and the `IsFxMesh` skip fires only after ~12 optional-component gets and the frustum-sphere test, all discarded for FX entities.
- **Impact**: One extra storage get per rendered entity per frame (tens of µs at FO4 MedTek scale) plus ~12 wasted gets per FX entity. Micro-scale but this is the single hottest CPU loop in `build_render_data`.
- **Related**: #1377, #1136.
- **Suggested Fix**: Replace the two-step probe with a single `let Some(transform) = tq.get(entity) else { continue; };`, and hoist the `fx_q` gate immediately after the visibility skip.

#### D2-NEW-05: `draw_sort_key` omits the `wireframe` pipeline axis — sort key and `PipelineKey` no longer in lockstep
- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/mod.rs:192-240`; `crates/renderer/src/vulkan/pipeline.rs:114,121` (`Opaque { wireframe }`, `Blended { …, wireframe }`)
- **Status**: NEW
- **Description**: #869 added `wireframe` to `PipelineKey`, making it a batch-merge and pipeline-bind boundary, but `draw_sort_key` (a 10-tuple in both the alpha and opaque branches) was never extended with a matching slot — neither branch references `cmd.wireframe`. A wireframe draw interleaved among fill draws lands mid-run, splitting the instanced batch and forcing extra pipeline binds.
- **Impact**: Near-zero on shipped content (`NiWireframeProperty` is essentially absent from real assets) — a lockstep/hardening gap that becomes real if wireframe content (debug modes, mods) ever coexists with fill geometry.
- **Related**: #869, #1581.
- **Suggested Fix**: Fold `wireframe` into an existing u8 slot (e.g. pack with `two_sided`), and extend the sort-key/merge-axis lockstep test.

#### PERF-D3-NEW-03: `memory-budget.md` links the BGSM cache section to a path deleted by the `asset_provider` module split
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `docs/engine/memory-budget.md:149`
- **Status**: NEW
- **Description**: The doc links `byroredux/src/asset_provider.rs`, which no longer exists — the module split into `byroredux/src/asset_provider/{mod,archive,material,script,texture,tests}.rs`. All documented values remain correct; pure link rot.
- **Impact**: Doc-only.
- **Related**: Session 34/35 module-split notes.
- **Suggested Fix**: Point the link at `byroredux/src/asset_provider/material.rs`, and sweep the doc's other relative links.

#### PERF-D4-NEW-02: `upload_lights` silently truncates past `MAX_LIGHTS` (512) — no overflow warn, no proximity prioritization
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:25`; producer `byroredux/src/render/lights.rs:73-177`
- **Status**: NEW
- **Description**: `collect_lights` appends every `LightSource` with no cap, sort, or camera-proximity priority; `upload_lights` clamps to 512 silently (`let count = lights.len().min(MAX_LIGHTS);`). Every sibling overflow path (instances, indirect draws, terrain tiles, material intern) warns; lights only get a once-per-session info-log on the first frame, before dense cells load.
- **Impact**: On content with >512 live lights (plausible on Skyrim radius-3+ exterior grids), lights past index 511 in storage-iteration order vanish with zero telemetry — a light adjacent to the camera can be the one dropped.
- **Related**: #279, #797.
- **Suggested Fix**: Add the same `log::warn!` pattern on `lights.len() > MAX_LIGHTS`; optionally sort by distance-to-camera before the clamp.

#### PERF-D4-NEW-03: `upload_indirect_draws` lacks the dirty gate both its siblings have
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:594-626`; caller `crates/renderer/src/vulkan/context/draw.rs:3225-3238`
- **Status**: NEW
- **Description**: Instances (#1134) and materials (#878) both got content-hash dirty gates justified by "static interiors produce byte-identical slices each frame." The indirect command list, derived from the same batches, is byte-identical under the exact same conditions but is `copy_nonoverlapping` + `flush_range`'d unconditionally every frame (`upload.rs:617-625`). No `last_uploaded_indirect_hash` field exists.
- **Impact**: Small (worst realistic ≈160 KB/frame ≈ 10 MB/s at 60 fps; indirect entries are ~20 B vs 112 B for instances) — a consistency/completeness gap in the established pattern.
- **Related**: #878, #1134.
- **Suggested Fix**: Reuse the existing FxHash-over-slice + per-FIF `last_uploaded_hash` pattern (~15 lines mirroring `upload_instances`).

#### PERF-D4-NEW-04: Stale byte-math in scene_buffer comments — `GpuLight` quoted at 48 B (is 64 B), `GpuInstance` quoted at 72 B (is 112 B)
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:10`; `upload.rs:492-494`; `descriptors.rs:292-295`
- **Status**: NEW
- **Description**: `GpuLight` is 64 B and `GpuInstance` is 112 B (pinned by the layout test), but three in-code comments still quote the pre-R1 sizes, understating live PCIe traffic estimates.
- **Impact**: Doc-rot only — sizes that matter (buffer allocation, flush ranges, tests) all derive from `size_of`, not the comments.
- **Related**: memory-budget.md (already correct); #1134, #1587.
- **Suggested Fix**: Update the three comments to 64 B / 112 B and recompute the quoted per-frame figures.

#### D6-04: Fixed per-frame skinning costs run even on fully-clean frames
- **Severity**: LOW
- **Dimension**: Skinning & BLAS
- **Location**: `byroredux/src/render/mod.rs:308,328`; `byroredux/src/render/skinned.rs:153-186`; `crates/renderer/src/vulkan/context/draw.rs:2630-2732`
- **Status**: NEW (distinct from #1379, which fixed the monotonic high-water range; #1195 gated only the vertex-skinning dispatch + refit)
- **Description**: The `pose_dirty` gate (`draw.rs:1712`) guards only the per-entity GPU compute dispatch. When `pose_dirty` is empty and no bind-inverse uploads are pending, the frame still pays, ungated: CPU identity refill of the whole live `bone_world` range + full per-entity bone-matrix reconstruction (`gt_q.get` + `to_matrix` per bone, `skinned.rs:153-181`), the pose-hash recompute over that slice (inherently un-gateable — it *is* the dirtiness signal), the full-range staging memcpy + device copy, and a full-range `skin_palette.comp` dispatch. Pass-4 `pool.sweep` (`skinned.rs:186`) also runs every frame.
- **Impact**: For S live slots, ≈S × 9.2 KB per frame per sub-step — well under a millisecond at realistic slot counts (LOW), relevant only on dense crowd cells. The avoidable part is the matrix rewrite + upload + copy + dispatch on clean frames (the hash recompute is fundamental to the scheme).
- **Related**: #1379, #1195, #1284.
- **Suggested Fix**: Track frames-since-last-dirty and skip upload+copy+dispatch when ≥`MAX_FRAMES_IN_FLIGHT` with no pending uploads; stop clearing `bone_world` every frame, re-seeding identity only for freshly (re)allocated slots.

#### D6-05: First-sight entities pay a redundant BLAS UPDATE immediately after their fresh BUILD in the same command buffer
- **Severity**: LOW
- **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1848-1870,1878`
- **Status**: NEW
- **Description**: A first-sight entity is always dirty, so the refit-gate condition is false and the loop proceeds to `refit_skinned_blas` — a full UPDATE against the identical vertex data the BUILD consumed moments earlier in the same command buffer. The block comment asserts the fall-through is harmless, but `refit_skinned_blas` has no "freshly built this frame" short-circuit and records a real UPDATE, also inflating `refits_attempted`/`refits_succeeded` on spawn frames.
- **Impact**: One redundant AS UPDATE + barrier per skinned entity per spawn frame only; steady-state unaffected. Minor telemetry skew on spawn frames.
- **Related**: #911, #1196.
- **Suggested Fix**: Track entities built this frame and `continue` past the refit for them; fix the stale comment either way.

#### PERF-D5-NEW-03: SVGF à-trous recomputes the 5×5 spatial-variance estimate in all 5 iterations
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/svgf_atrous.comp:134-150` (unconditional 5×5 luminance loop); dispatch loop `crates/renderer/src/vulkan/svgf.rs:88` (`ATROUS_ITERATIONS = 5`), `:1258-1288`
- **Status**: NEW (Session 49 addition)
- **Description**: Each of the 5 à-trous iterations re-derives a 5×5 local luminance variance (`spatialVar`, `svgf_atrous.comp:134-150`, no iteration-index gate) and re-runs the 3×3 temporal-variance prefilter (`:108-123`) before the 25-tap edge-stopped filter. The spatial estimate is a legitimate iteration-0 concern (catches converged-but-noisy pixels), but iterations 1-4 run it against already-filtered color whose local variance shrinks monotonically, mostly duplicating work with diminishing contribution. (Distinct from the shader's own header note about not filtering variance *alongside* color — that is a separate deferred refinement.)
- **Impact**: Constant-factor bandwidth/ALU on 5 full-screen dispatches (~460M extra, heavily L2-cached, texel fetches/frame at 1440p); the pass remains strictly O(pixels).
- **Confidence**: HIGH on the cost; the safety of computing spatial-variance once and propagating it needs a visual A/B against the dark-floor moiré regression scene before shipping.
- **Related**: #1662 / Session 49 denoiser overhaul.
- **Suggested Fix**: Compute the spatial-variance estimate in iteration 0 only, propagate through the unused moments-image channel, falling back to temporal-variance-only weight in later iterations.

#### PERF-D5-NEW-04: ReSTIR reservoir SSBOs (~130-530 MB screen-dependent) are absent from `memory-budget.md` and all VRAM telemetry
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/restir.rs:36-83`; `docs/engine/memory-budget.md` (no entry)
- **Status**: NEW
- **Description**: Session 49 added two device-local, screen-sized reservoir buffers (one per FIF slot, `width * height * RESERVOIR_STRIDE`) — ~127 MB at 1080p, ~236 MB at 1440p, ~531 MB at 4K — the largest single VRAM addition of the denoiser overhaul, but the authoritative per-pass VRAM ledger has no row for it and no telemetry attributes it.
- **Impact**: Budget-accounting drift only — no leak (create-once + recreate-on-resize with fenced destroy is correct). At 4K this is >13% of the ~4 GB engine budget going untracked, the same class of gap that has historically preceded budget regressions.
- **Confidence**: HIGH (arithmetic + grep).
- **Related**: RT VRAM budget baseline (~4 GB target); #1583/#1590 (removed the per-pixel reservoir G-buffer attachment this SSBO pair replaced).
- **Suggested Fix**: Add a "ReSTIR reservoirs" row to memory-budget.md with the W×H×stride formula, and include both buffers in the renderer's memory-usage log line.

---

## Existing Issues Re-confirmed (not re-filed)

| Issue | Note |
|---|---|
| #1698 (OPEN, HIGH, performance) | Skyrim Dragonsreach ECS scheduler stalls ~140 ms/frame for ~28 s — a **post-load** Rapier/ECS-scheduler settle-storm (`docs/audits/AUDIT_RUNTIME_2026-06-26.md`). D7-NEW-01 is the **load-time** freeze that precedes and compounds with it — related, not a duplicate. |
| #1763 / TD9-001 (OPEN, LOW, tech-debt) | NIF heap-allocation dhat regression tests never run in CI (`dhat-heap` feature dormant). Still accurate; both bound test files run green locally. |

## Resolved Since a Prior Audit (verified, not re-filed)

| Prior finding | Resolution |
|---|---|
| PERF-2026-06-23-01 (LOW) — AnimationPlayer text-event Vec allocated per frame | **RESOLVED** via #1725: `player_events` lives in persistent `AnimScratch`, reused via `clear()`. |
| D1-NEW-01 (2026-06-16, LOW) — scheduler `run()` allocates ~25 Strings + a Vec | **RESOLVED** via #1647: timing tracker `Option`-gated on `SchedulerSystemTimings` presence. |
| D2-NEW-01 (2026-06-16, MEDIUM) — additive particle billboards never instance-batch | **RESOLVED** via #1649 (`067a8354`): transparent branch of `draw_sort_key` special-cases additive blend; regression tests added. |
| D5-NEW-01 (2026-06-16, LOW) — stale "7th reservoir attachment" comment | **RESOLVED**: comment now documents 6 color attachments + depth. |
| F8 (2026-06-14) — full-allocation `flush_if_needed` | **RESOLVED** via #1587: all five upload sites use range-limited, atom-aligned `flush_range`. |

---

## Prioritized Fix Order

Quick wins (scratch reuse, preallocation, gate restoration) before architectural changes:

1. **D6-01 (HIGH)** — make the `SkinSlotPool` bind-inverse drain transactional against `draw_frame`'s early-return paths. The one finding with a real (if narrow) correctness blast radius — garbage skinned geometry surviving into both raster and BLAS/TLAS.
2. **D6-02 (MEDIUM)** — same transactional fix pattern covers the pose-hash commit-ordering hole; do both in one pass (shared root cause, same call sites).
3. **PERF-D3-NEW-01 (MEDIUM, quick win)** — thread `pending_bytes` into `evict_unused_blas`'s gate; restores the mid-batch eviction the #510 mechanism was built to provide and closes the OOM-on-first-huge-cell risk.
4. **D2-NEW-02 (MEDIUM, quick win)** — quantize the particle fade parameter before the color LERP (~5-line change); restores the ~97% material-dedup hit rate #781 assumed.
5. **PERF-D5-NEW-01 (MEDIUM)** — compile-time-gate the legacy 16-slot WRS arrays behind the existing generated-constants channel; recovers register/occupancy headroom every production frame at the cost of a shader recompile for the A/B path.
6. **PERF-D5-NEW-02 (MEDIUM)** — sort or distance-select the GI-hit light prefix; fixes both a quality bug and wasted ray budget in one change.
7. **PERF-D4-NEW-01 (MEDIUM)** — file a live issue for variable-stride `bone_world` packing (the debt comment points at a closed milestone); scope as its own follow-up.
8. **D7-NEW-01 (MEDIUM, quick win first)** — add `Instant::now()` timing around interior NPC spawn to quantify the stall before deciding whether to invest in chunking.
9. **PERF-D3-NEW-02 (MEDIUM)** — add a lazy re-acquire path for evicted-but-still-drawn static BLAS, mirroring the skinned first-sight build.
10. **D6-03 (MEDIUM)** — measure via `gpu_skin_blas_refit_ms` on a crowd scene before investing in scratch sub-allocation.
11. **All LOW findings** — batch as a single hygiene pass (env-var caching, redundant probes, stale comments, missing warns, doc-link fix, ReSTIR/memory-budget doc rows); each is independently trivial.
12. **Process recommendation** — run a fresh R6a-stale-15 three-scene GPU bench. ROADMAP's bench-of-record predates both the Session 47 camera-origin work and the Session 49 RT-denoiser overhaul that produced most of this audit's Dimension 5 findings; no current FPS claim is possible without it.

No finding requires reverting a Vulkan render-pass, barrier, or pipeline-state change speculatively; PERF-D5-NEW-01/02/03 are ALU/register/algorithm-level and flagged with their confidence level rather than asserted as certain wins.

---

## Methodology Note

This report is a re-verification of `AUDIT_PERFORMANCE_2026-07-01.md` against
the identical HEAD (`1b4e8e84`; `git diff` empty). Three parallel dimension
agents plus direct spot-checks independently re-derived every finding from live
source with the goal of disproving it. Outcome: 21 of 23 findings survived
unchanged; **PERF-D1-NEW-02** was narrowed to PARTIAL (2 live sites, not 3);
**D6-02**'s line pointers were corrected (the hash commit is in `render/skinned.rs`,
not `draw.rs`); **PERF-D1-NEW-03**'s premise was re-confirmed after an initial
false-negative STALE call (the function lives in `render/particles.rs`, not
`systems/particle.rs`). No landed guard has eroded. Because the tree is
unchanged, no findings beyond the prior set were expected or found — the next
substantive performance sweep should follow the overdue R6a-stale-15 bench and
any Session 54+ code.
