# ByroRedux Performance Audit — 2026-07-01

**HEAD**: `1b4e8e84` (post-Session-53 closeout — CHARAL character abstraction
layer + ~35-fix audit bug-bash). **Depth**: deep, all 9 dimensions.
**Hardware target**: RTX 4070 Ti (12 GB) + Ryzen 7950X (16c/32t); RT VRAM
minimum 6 GB; RT budget total under ~4 GB. A CPU bottleneck on this machine
is a bug, not a tuning gap.

Prior performance audit: `docs/audits/AUDIT_PERFORMANCE_2026-06-23.md` (1 LOW,
zero regressions — the healthiest prior baseline on record). This audit
re-verifies every guard that report and its predecessors established, across
Session 53's ~35 intervening fixes, and searches for new issues introduced
since — including the Session 49 RT-denoiser overhaul (`6b061120`, #1662)
which had not yet received a dedicated shader-level performance pass.

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 |
| MEDIUM   | 9 |
| LOW      | 13 |
| **Total**| **23** |

Zero CRITICAL findings. One HIGH: a real-but-narrow correctness/perf hole in
the skinning pool commit protocol (D6-01). The remaining 22 findings are
MEDIUM/LOW efficiency gaps — none are regressions of a landed guard; every
guard enumerated by the skill across all 9 dimensions was re-verified INTACT
against current code. This is a **higher finding count than the last three
performance audits** (2026-06-23: 1; 2026-06-16: 2; 2026-06-14: 2) — not
because the codebase regressed, but because this pass is the first dedicated
performance sweep of the Session 47 Cornell/GI/camera-origin work and the
Session 49 RT-denoiser overhaul (Dimension 5), plus the first pass to trace
the skinning pool's commit ordering against `draw_frame`'s early-return paths
(Dimension 6) and the interior NPC-spawn path against the exterior budget
precedent (Dimension 7). All of these are genuinely new code surfaces, not
previously-audited surfaces that eroded.

### Bench-of-Record delta (observed vs ROADMAP — not absolute FPS)

Per the skill's Bench-of-Record policy, no FPS/ms/fence numbers are
hardcoded here. ROADMAP.md's **Bench-of-record** block (R6a-stale-14, HEAD
`1c26bc25`, 2026-06-03) is self-flagged as **437 commits stale** as of this
session's closeout — the most stale it has ever been — because Session 47
(Cornell glass/GI/caustics + camera-relative render origin) and Session 49
(#1662 RT denoiser overhaul: multi-scatter energy compensation, SVGF à-trous,
ReSTIR-DI temporal reservoirs, PCG-hash sampling) both landed after the last
recorded bench and are exactly the passes Dimension 5 of this audit found
the most (and only) new material findings in. The canonical control benches
remain **Prospector** (FNV, glass-heavy interior), **WhiterunBanneredMare**
(Skyrim, steady-state), and **MedTekResearch01** (FO4, CSG-precombine-heavy).
**R6a-stale-15 (a fresh three-scene 300-frame GPU bench) is now overdue** —
it is the only way to confirm whether the Session 49 denoiser changes moved
per-pass GPU cost, and several findings below (D5-NEW-01 through 04) directly
bear on that pass's cost profile. This is a process recommendation, not a
code finding.

---

## Hot Path Analysis — Guard Verification Matrix

Quantitative instrumentation exists and should be used for any future "this
is expensive" claim: GPU per-pass timestamp pairs
(`crates/renderer/src/vulkan/gpu_timers.rs`), the CPU per-phase wall-clock
split (`log_stats_system`'s `cpu_ms:` line, `byroredux/src/systems/debug.rs`),
`ScratchTelemetry` (`crates/core/src/ecs/resources.rs`), and the skin
coverage counters (`SkinCoverageFrame`, `crates/renderer/src/vulkan/skin_compute.rs`,
reachable via `skin.coverage` / `bench-stats --break-down skin`). Every
finding below that estimates a cost either cites one of these instruments or
explicitly states no quantitative guard exists for the site (per-frame
render/ECS hot paths have no dhat coverage; the profiler is a process
singleton).

### Dimension 1 — CPU Per-Frame Allocations & Hot Paths
| Guard | Status | Evidence |
|---|---|---|
| `drain_dirty_into` (#1371) preserves dirty-set capacity | INTACT | `crates/core/src/ecs/packed.rs:73`, test at `:693` |
| `make_animation_system()` persistent scratch (#1372), extended to text-events (#1725) | INTACT | `byroredux/src/systems/animation.rs:322-338,786-791` |
| `make_billboard_system()` camera-static skip (#1374) | INTACT | `byroredux/src/systems/billboard.rs:23,56-59` |
| `build_debug_ui_snapshot` clone gated on visibility (#1376) | INTACT | `byroredux/src/main.rs:1629-1633` |
| `SkinSlotPool` idle-slot sweep contracts `next_slot` (#1379) | INTACT | `crates/core/src/ecs/resources.rs:902-913`, test `:1067` |
| Scheduler timing tracker gated on resource presence (#1647, supersedes 2026-06-16 D1-NEW-01) | INTACT | `crates/core/src/ecs/scheduler.rs:473-476,87` |

### Dimension 2 — Draw-Call & Instancing Efficiency
| Guard | Status | Evidence |
|---|---|---|
| GT-presence hoist before sibling probes (#1377) | INTACT | `byroredux/src/render/static_meshes.rs:147` |
| Parallel-sort gate at ≥2000 commands matches 7950X crossover | INTACT | `byroredux/src/render/mod.rs:398-421` |
| `draw_sort_key` 10-tuple orders pipeline/state before depth | INTACT | `byroredux/src/render/mod.rs:192-240` |
| Batch merge SSBO-contiguity requirement | INTACT | `crates/renderer/src/vulkan/context/draw.rs:3100-3113` |
| Per-draw state fully change-gated | INTACT | `draw.rs:996-1102` (post-#1748 `record_geometry_pass`) |
| Additive-particle mesh-before-depth sort (2026-06-16 D2-NEW-01) | **RESOLVED** | Fixed by #1649, commit `067a8354`; regression tests in `draw_sort_key_tests.rs` |

### Dimension 3 — GPU Memory Pressure & Eviction Thrash
| Guard | Status | Evidence |
|---|---|---|
| Dynamic BLAS budget = `device_local/3` floored at 256 MB | INTACT | `crates/renderer/src/vulkan/acceleration/predicates.rs:547-554` |
| LRU victim = smallest last-used tick, routed through deferred destroy | INTACT | `blas_static.rs:1134-1151,1175` |
| Mid-batch eviction trigger @ 90% + 64-build interval | **Trigger INTACT, effect ERODED** | See PERF-D3-NEW-01 — callee gate makes it a structural no-op |
| Scratch/TLAS shrink honors reserve floors + slack | INTACT | `memory.rs:42-104,157`; `constants.rs:21,30,39,47,54` |
| MeshRegistry soft/hard caps fire (warn/error) | INTACT | `mesh.rs:29-34,55-71` |
| BGSM/BGEM half-eviction, not full flush (#1430) | INTACT | `byroredux/src/asset_provider/material.rs:437-443` |
| NifImportRegistry 2048-entry LRU | INTACT | `byroredux/src/cell_loader/nif_import_registry.rs:187-190` |
| Deferred-destroy countdown = 2, ticked post-fence-wait | INTACT | `deferred_destroy.rs:34`; `draw.rs:2166-2189` |

### Dimension 4 — SSBO Sizing & Per-Frame Upload
| Guard | Status | Evidence |
|---|---|---|
| `GpuInstance` 112 B, per-draw-only, 3 layout tests | INTACT | `gpu_instance_layout_tests.rs:26,71,91` |
| PBR resolved once at import, no per-draw `classify_pbr_keyword` | INTACT | `crates/core/src/ecs/components/material.rs:217,223,449,638` |
| `MAX_INSTANCES`/`MAX_INDIRECT_DRAWS`/`MAX_MATERIALS` match memory-budget.md | INTACT | `scene_buffer/constants.rs:134,157,184` |
| Instance/material dirty gates (#1134/#878), post-flush hash stamping | INTACT | `scene_buffer/upload.rs:500-503,558-561` |
| Upload = O(live data), `flush_range` not full-buffer (2026-06-14 F8) | **RESOLVED** | All 5 upload sites migrated, e.g. `upload.rs:76,204,518,577,625` |
| `MaterialTable::intern` O(1) amortized | INTACT | `crates/renderer/src/vulkan/material.rs:922,1077-1083` |

### Dimension 5 — GPU Pipeline & Pass Efficiency
| Guard | Status | Evidence |
|---|---|---|
| Volumetrics/bloom/SVGF/TAA/SSAO dispatch O(pixels)/O(froxels) | INTACT | `volumetrics.rs:864-867`, `bloom.rs:72,546-602`, `svgf.rs:1214-1288` |
| TLAS build→read barrier | INTACT | `context/draw.rs:2766-2774` |
| G-buffer 6 attachments ×2 FIF, reservoir attachment retired | INTACT | `draw.rs:2263-2265`; `gbuffer.rs:37-59` |
| `inv_vp` CPU-computed once, no per-invocation `inverse()` | INTACT | `ssao.comp:24`, `cluster_cull.comp:60` |
| Disney BSDF lobes gated by material flags | INTACT | `lighting.glsl:120-161`; `triangle.frag:514-693,1867-1918` |
| Stale "7th reservoir attachment" comment (2026-06-16 D5-NEW-01) | **RESOLVED** | `draw.rs:2263-2265` now correct |
| Streaming-RIS reservoir loop count vs divergence | **Architecture changed — see PERF-D5-NEW-01** | Default path now single-reservoir ReSTIR-DI (4 shadow rays, down from 16); legacy 16-slot WRS arm stays compiled-in |

### Dimension 6 — Skinning & BLAS Cost (M29.x)
| Guard | Status | Evidence |
|---|---|---|
| Bone-palette multiply is dedicated compute pass (M29.5) | INTACT | `skin_compute.rs:850`; `skin_palette.comp:78` |
| `bind_inverses` persistent SSBO, first-sight-only upload | INTACT (commit-path hole, see D6-01) | `upload.rs:270-277`; `context/draw.rs:2643` |
| Dispatch-dirty gate (#1195), `pose_dirty` on `SkinSlotPool` | INTACT (staleness hole, see D6-02) | `draw.rs:1711-1714,1732`; `resources.rs:724,949,965` |
| BLAS refit gate (#1196), `SKINNED_BLAS_FLAGS = FAST_BUILD` | INTACT | `draw.rs:1860-1864`; `acceleration/constants.rs:68,112-114` |
| Descriptor-rewrite skip (#1197) | INTACT | `skin_compute.rs:532-559,872-899`, test `:1102` |
| Instrumentation (#1194) GPU timers + `dispatches_skipped` | INTACT | `skin_compute.rs:136-144`; `gpu_timers.rs:353-408` |

### Dimension 7 — World Streaming & Cell Transitions (M40)
| Guard | Status | Evidence |
|---|---|---|
| Two-phase `pre_parse_cell` (#877) | INTACT | `byroredux/src/streaming.rs:634-665` |
| Small-model serial fast-path (#1262) | INTACT | `streaming.rs:659-665` (threshold 8) |
| NIF import cache is process-lifetime, 2048-entry LRU | INTACT | `byroredux/src/cell_loader/nif_import_registry.rs:145-205` |
| Exterior per-frame cell-spawn budget (#1586/F7) | INTACT | `byroredux/src/main.rs:1181,1212-1227` |
| Interior NPC spawn budget | **ABSENT — see D7-NEW-01** | No equivalent cap in `load_references`/`spawn_npc_entity` |
| Shutdown drain joins worker, no leak | INTACT | `streaming.rs:310-334,381-409` |
| Starfield CDB parsed once per session | INTACT | `crates/sfmaterial/src/reader.rs:30-90`; `byroredux/src/asset_provider/material.rs:74-125,246` |

### Dimension 8 — NIF Parse Performance
| Guard | Status | Evidence |
|---|---|---|
| `read_pod_vec<T>` collapses double-allocation (#833) | INTACT | `crates/nif/src/stream.rs:350,391-482` |
| `bytemuck` NOT a workspace dep; hand-rolled `AnyBitPattern` | INTACT | zero `bytemuck` hits repo-wide; `stream.rs:24-26,47` |
| `allocate_vec::<T>` is `#[must_use]` (#831) | INTACT | `stream.rs:252`; all 20 call sites bind |
| Per-block counters use `get_mut`/`insert` split (#832) | INTACT | `crates/nif/src/lib.rs:342-365` |
| `NiPSysEmitter*` parsed at import time only | INTACT | `crates/nif/src/import/walk/mod.rs:687,786` via `streaming.rs:510` |
| dhat bound tests green | INTACT (CI gap tracked) | `crates/nif/tests/heap_allocation_bounds{,_geometry}.rs`, 3/3 pass — see Existing #1763 below |

### Dimension 9 — Telemetry & Camera-Relative Origin Cost
| Guard | Status | Evidence |
|---|---|---|
| GPU timer readback after fence wait, no mid-frame stall | INTACT | `context/draw.rs:2069-2096` |
| `read_and_reset` bracket-gated, no WAIT-block on unwritten query | INTACT | `gpu_timers.rs:243-309` |
| `cpu_ms:` CPU-phase breakdown intact | INTACT | `byroredux/src/systems/debug.rs:59-142` |
| `ScratchTelemetry` capacity-vs-used tracking (now 7 rows) | INTACT | `context/mod.rs:2639-2716` |
| Render origin snaps to 4096-unit grid, moves only on crossing | INTACT | `scene_buffer/constants.rs:336,347-348` |
| Per-instance rebase stays inside existing O(visible-instances) loop | INTACT | `draw.rs:2884-3130` |
| `origin_corrected_prev_view_proj` preserves TAA/SVGF history (#1489) | INTACT | `draw.rs:3836-3850`, 2 regression tests pass |

---

## Findings

Findings are grouped by severity, CRITICAL first. IDs preserve each
dimension agent's own numbering for traceability back to `/tmp` working
notes (now cleaned up).

### HIGH

#### D6-01: First-sight `bind_inverses` are drained from the pool before `draw_frame` commits them — an early return permanently corrupts the entity's skinning palette
- **Severity**: HIGH
- **Dimension**: Skinning & BLAS
- **Location**: `byroredux/src/main.rs:1759-1785` (drain + hand-off) vs `crates/renderer/src/vulkan/context/draw.rs:2030-2032,2118` (early returns) vs `draw.rs:2643-2665` (actual upload)
- **Status**: NEW (related: #1192 fixed a different loss vector — capped-drain inside `upload_pending_bind_inverses`; this is upstream of that fix, at the pool-drain call site)
- **Description**: `render_one_frame` calls `self.skin_slot_pool.drain_pending(...)` and materializes `pending_with_data` *before* invoking `ctx.draw_frame(...)`. `drain_pending` irrevocably removes the entries from `SkinSlotPool::pending_uploads` and the pool has no re-queue API. `draw_frame` has multiple early-return paths preceding the bind-inverse upload at line 2643: empty framebuffers (`Ok(false)` at 2030), `ERROR_OUT_OF_DATE_KHR` on acquire (`Ok(true)` at 2118 — fires on every window resize / mode change), and fence/error propagation paths. On any of these, the drained first-sight `bind_inverses` are dropped: the persistent SSBO region for those slots is never written (the buffer is `create_device_local_uninit`), yet `entity_to_slot` keeps the slot resident so `allocate()` never re-queues the upload.
- **Evidence**: Call path `main.rs:1759 drain_pending` → `main.rs:1806 draw_frame(...)` → `draw.rs:2118 return Ok(true)` (upload at 2643 never reached) → `pending_with_data` dropped; the `Ok(needs_recreate)` handling arm (`main.rs:1829-1877`) performs no re-queue. Every subsequent frame, `skin_palette.comp` computes `palette[slot] = bone_world[slot] × <uninitialized>` for the affected slots, consumed by both the raster path (`triangle.vert` set 1 binding 3) and `skin_vertices.comp`, and the resulting garbage skinned vertices feed the per-entity BLAS and TLAS.
- **Impact**: Skinned entity (NPC body part) renders as garbage geometry in **both** raster and RT, and pollutes the TLAS with degenerate world-space triangles, for the entity's remaining lifetime in the cell — recovery only via despawn + 3-frame pool sweep + respawn. Trigger requires a first-sight frame (NPC spawn / cell load) to coincide with a swapchain-out-of-date frame (resize, fullscreen toggle) — a real but narrow window, more likely during startup cell loads when window setup and streaming can overlap.
- **Related**: #1192 (the sibling loss vector, already fixed), D6-02 (same root cause — pool state advances before `draw_frame` commits).
- **Suggested Fix**: Make the drain transactional: move `drain_pending` inside `draw_frame` past the last early return (pass `&mut SkinSlotPool` or a drain closure), or have `draw_frame` report "skin section reached" and re-queue the drained `(slot, entity)` pairs into `pending_uploads` on any path that returned before the upload.

---

### MEDIUM

#### PERF-D3-NEW-01: Mid-batch BLAS eviction (#510) is structurally a no-op — `evict_unused_blas` gate ignores the batch's pending bytes
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:547-567` (mid-batch trigger) + `blas_static.rs:1115-1117` (callee gate); `predicates.rs:382-390`
- **Status**: NEW (defect in the #510 fix; distinct from #740, which fixed the frame-counter aspect of the same mechanism)
- **Description**: The mid-batch check fires `should_evict_mid_batch(static_blas_bytes, pending_bytes, budget)` at projected footprint ≥90% of budget, but `evict_unused_blas` early-returns unless `static_blas_bytes` **alone** exceeds **100%** of budget — the batch's freshly created result buffers aren't added to `static_blas_bytes` until Phase 7. Since `frame_counter` bumps once per `build_blas_batched` call, the eviction candidate set is frozen for the whole batch, identical to what pre-batch eviction already saw. Either the pre-batch pass already brought `static ≤ budget` (mid-batch always early-returns) or it couldn't (no idle-eligible candidates remain, so mid-batch re-scans an empty set). Either way, mid-batch eviction frees zero bytes, always.
- **Evidence**: `docs/engine/memory-budget.md` documents the trigger ("runs pre-batch and mid-batch, triggered at 90% of BLAS budget") but not the (nil) effect; `should_evict_mid_batch`'s #510 doc comment describes the intended pause-and-evict behavior the callee gate defeats.
- **Impact**: A single large batch (initial exterior grid load, FO4 precombine-heavy cell) allocates its full result-buffer footprint above budget with no mid-batch relief. On the RT-minimum 6 GB device (budget = 2 GB) the intended pause never lands; failure mode is allocator pressure / a graceful cell-load bail (cleanup verified), not device loss. Unreachable on the 12 GB dev card with vanilla content.
- **Related**: #510 (introduced the mechanism), #740 (frame-counter no-op fix), #915 (single-shot pre-build guard), PERF-D3-NEW-02.
- **Suggested Fix**: Thread `pending_bytes` into `evict_unused_blas` (or add `evict_down_to(target_bytes)`) so both the early-return gate and the loop's break condition test `static + pending` against the 90% line the trigger already computes.

#### PERF-D3-NEW-02: Budget eviction has no rebuild path — evicted static BLAS drop out of RT permanently, and multi-cell load bursts age not-yet-drawn BLAS into candidacy
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:149-160` (missing rigid BLAS → count + skip only); `blas_static.rs:425-431` (per-batch `frame_counter` bump) + `blas_static.rs:1129-1151` (`idle ≥ 3` candidacy)
- **Status**: NEW
- **Description**: Two coupled gaps. (1) No recovery: static BLAS build only at load sites; only skinned BLAS have a lazy first-sight rebuild inside `draw_frame`. When budget eviction takes a BLAS whose mesh is still drawn, `build_tlas` increments `missing_rigid_blas`, warns (rate-limited), and skips — forever; the mesh keeps rasterizing but permanently vanishes from shadows/reflections/GI until its cell unloads and reloads. (2) Burst aging makes this reachable in the current scene: a synchronous multi-cell load (`--grid` radius 3 = 49 batched calls before the first frame) leaves cell #1's just-built, never-yet-drawn entries at `idle = 48` on the first `build_tlas` — prime LRU victims if cumulative `static_blas_bytes` crosses budget mid-load, since eviction picks oldest-first.
- **Evidence**: `tlas.rs:92`'s own comment: "an LRU eviction got something the draw still references; should be near-zero in steady state" — the counter (#1228) exists precisely because there is no re-acquire path.
- **Impact**: Silent RT-correctness degradation (missing occluders → wrong shadows/GI), gated on `static_blas_bytes > budget` — unreachable on the 12 GB dev card with vanilla content, plausible on 6-8 GB devices with heavy exteriors / mod load-orders. Crash-safe (deferred destroy, #1449); recovery requires a cell round-trip. Steady-state single-cell-per-frame streaming is protected (drawn entries idle ≤2 <3); exposure is multi-batch bursts between frames.
- **Related**: #920 (skinned/static budget split), #740 (introduced burst bumping), #1228 (telemetry split), #1449 (deferred destroy), PERF-D3-NEW-01.
- **Suggested Fix**: On a `missing_rigid_blas` hit in `build_tlas`, queue the mesh handle for a lazy `build_blas_batched` next frame (mirroring the skinned first-sight path). Cheaper stopgap: stamp the batch's own entries with a post-batch tick so an in-burst load cannot age its own cells into victims.

#### PERF-D4-NEW-01: Per-frame `bone_world` fill + upload is fixed-stride O(used_slots × 144) — pays the full `MAX_BONES_PER_MESH` reservation per skinned mesh every frame; the code comment's referenced fix points at an already-closed milestone
- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:178-256` (`upload_bone_worlds` + `record_bone_world_copy`), `byroredux/src/render/skinned.rs:128-170`, `byroredux/src/render/mod.rs:308`, `scene_buffer/constants.rs:50-56`
- **Status**: NEW (untracked debt: the in-code comment defers to "variable-stride packing (M29.5)", but ROADMAP.md marks M29.5 **Closed** with a narrower scope — GPU palette dispatch only — so no live tracker owns the packing work)
- **Description**: Each `SkinnedMesh` entity's slot occupies a fixed `MAX_BONES_PER_MESH` (144) × 64 B = 9216 B stride in `bone_world`, regardless of actual bound-bone count. Per-frame cost is three-fold, all O(used_slots × 144): CPU `bone_world.clear()` + `resize(required_slots, IDENTITY)` re-fills the whole array from empty every frame; host-visible staging memcpy + flush of the full range; GPU `cmd_copy_buffer` staging→device of the same bytes plus a `skin_palette.comp` dispatch sized from it. The unused-tail-is-free claim in `constants.rs` holds for slots past `max_used_slot`, but the 144-slot stride tail of every *used* slot is paid in CPU fill, PCIe write, flush, and transfer copy every single frame.
- **Evidence**: `skinned.rs:131-133` — `required_slots = (max_used_slot+1) * MAX_BONES_PER_MESH`; resize always fills from empty since `bone_world` is cleared per-frame at `render/mod.rs:308`; `upload.rs:190` sizes the byte count from the full strided array length.
- **Impact**: Scales with skinned-entity density, not bone count. At the project's own measured 260-entity FNV workload (FreesideAtomicWrangler): ≥261 slots × 9216 B ≈ 2.4 MB/frame ≈ 144 MB/s sustained host-write + flush + GPU copy at 60 fps, most of it identity padding beyond each mesh's actual bound-bone prefix. Full-pool worst case (1365 slots) ≈ 12.6 MB/frame ≈ 755 MB/s. No dirty gate applies (animation legitimately changes every frame), so unlike instances/materials this cost is unconditional.
- **Related**: #1284 (`MAX_TOTAL_BONES` history), M29.5 (closed, narrower scope), memory-budget.md bone-palette row (residency is fine; this is about per-frame traffic).
- **Suggested Fix**: Variable-stride packing — allocate prefix-summed offsets sized by each mesh's actual bone count (or quantized buckets, e.g. 16-bone granularity), uploading only the live prefix per slot. Cheaper interim: build per-slot `vk::BufferCopy` regions covering only `skin.bones.len()` matrices instead of one whole-range copy. File a live issue either way so the debt stops pointing at a closed milestone.

#### D2-NEW-02: Per-particle unquantized LERPed color defeats `MaterialTable` dedup — one fresh 300-B `GpuMaterial` per live particle per frame
- **Severity**: MEDIUM
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/particles.rs:74-83,138-139,207-209`; `crates/renderer/src/vulkan/material.rs:1051` (`intern_by_hash`)
- **Status**: NEW
- **Description**: Each particle's color is LERPed against its age every frame (unquantized `t = age/life`) and folded into `emissive_color`/`emissive_mult`, both material-table (post-R1) fields. `material_hash` therefore differs for virtually every particle every frame, so `intern_by_hash` takes the miss path per particle: full `GpuMaterial` construction + FxHashMap insert + table push + upload, inverting the ~97% dedup-hit rate the #781 fast path assumes. Instancing itself is unaffected (`material_id` is per-instance data), so this is the residual cost left after #1649 fixed the depth-vs-mesh sort ordering.
- **Evidence**: `particles.rs:77-83` computes continuous `t`; `:138-139` writes to `emissive_mult`/`emissive_color`; `:208-209` `intern_by_hash(cmd.material_hash(), ...)`. `hash_gpu_material_fields` covers the emissive fields, so distinct colors never dedup.
- **Impact**: Scales with live particle count. FX-heavy scenes (20-30 emitters, 96-256 particle caps each) can reach ~5-8K unique materials/frame ≈ 1.5-2.3 MB/frame upload plus CPU churn, stacking toward the `MAX_MATERIALS = 16384` cap where overflow silently routes particles to the neutral material id 0 (wrong color). Also permanently depresses the dedup-ratio telemetry, masking real dedup regressions elsewhere.
- **Related**: #1649 (resolved the instancing half), #781 (hit-path design), #780 (dedup telemetry), #797 (cap behavior).
- **Suggested Fix**: Quantize the fade parameter before the color LERP (e.g. 32 steps — imperceptible on additive billboards; size stays smooth since it lives in the per-instance model matrix). Same-emitter particles then collapse to ≤32 materials, restoring the dedup-hit rate.

#### D6-02: Pose hash is committed at `build_render_data` time — a `draw_frame` early return freezes the RT skinned pose while the dirty gate reads "clean"
- **Severity**: MEDIUM
- **Dimension**: Skinning & BLAS
- **Location**: `byroredux/src/render/skinned.rs:179-180` (hash commit) + `crates/renderer/src/vulkan/context/draw.rs:2118` (early return) + `draw.rs:1711-1714/1860-1864` (gates consuming the stale verdict)
- **Status**: NEW (related: #1195 introduced the gate; this is a commit-ordering hole in it, not a regression of the predicate itself)
- **Description**: `try_mark_pose_dirty(entity, hash)` records the new pose hash the moment `build_skinned_palettes` runs — CPU-side, before `draw_frame`. The scheme assumes the frame's skin dispatch will consume the dirty bit. If `draw_frame` early-returns before `record_skinned_blas_refit` (swapchain out-of-date, empty framebuffers), the dispatch never runs but the hash baseline has already advanced. Sequence: frame N-1 dispatches pose P1 (hash H1); frame N computes pose P2 → hash H2 recorded dirty; `draw_frame` N early-returns; frame N+1 the NPC stops moving → hash H2 matches stored H2 → gate reads "not dirty" → dispatch and refit both skipped with `has_populated_output == true`. The slot output buffer and skinned BLAS stay at P1 indefinitely while the raster palette (recomputed every frame from `bone_world`) shows P2.
- **Evidence**: `skinned.rs:180` runs unconditionally in `build_render_data`; nothing rolls back `last_pose_hash` when `draw_frame` fails to reach the skin section.
- **Impact**: RT shadows/reflections/GI of the affected NPC freeze at a pose one-plus frames stale relative to the rasterized body, persisting through the idle period after the lost frame. Self-healing on next movement; no crash, no leak.
- **Related**: D6-01 (same root cause: pool state advances before `draw_frame` commits), #1195.
- **Suggested Fix**: Same transactional shape as D6-01 — stage the frame's pose hashes and fold them into `last_pose_hash` only after `draw_frame` confirms the skin section ran, or re-insert the frame's dirty set on early-return paths.

#### D6-03: All skinned BLAS builds/refits in a frame are serialized on one shared scratch buffer — zero build overlap on multi-NPC frames
- **Severity**: MEDIUM
- **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:417` (per-refit barrier), `:278-283` (per-build barrier in first-sight batch); consumed at `context/draw.rs:1835-1899`
- **Status**: NEW (the barrier *correctness* chain — #642/#644/#983/#1095/#1140/#1300 — is complete and intact; nothing tracks the throughput cost of the serialization itself)
- **Description**: `blas_scratch_buffer` is a single allocation sized to the max single-build demand. Because every skinned BLAS build/refit reuses the same scratch address, the Vulkan spec requires an AS_WRITE→AS_WRITE barrier between each pair — N dirty skinned entities produce N fully serialized AS builds per frame, each self-emitting the barrier. Small skinned BVHs (5-15K triangles per body part) individually underutilize the GPU; back-to-back serialization with full-pipe AS-stage drains between them prevents any overlap.
- **Evidence**: `refit_skinned_blas`'s first statement is `record_scratch_serialize_barrier`; the refit loop calls it once per dirty entity; scratch sizing is grow-to-max-single-build, not per-build slots.
- **Impact**: GPU skin-chain time scales linearly with dirty-entity count with no overlap. On crowd scenes (market/plaza-class cells) the `gpu_skin_blas_refit_ms` bracket absorbs the full serial sum plus per-barrier drain cost. Idle crowds are already saved by #1195/#1196 (dispatch/refit gates skip entirely); this is the moving-crowd ceiling only.
- **Confidence**: Quantify before fixing — the #1194 GPU timer brackets exist precisely for this (`skin.coverage` → `gpu_skin_blas_refit_ms` vs `refits_attempted`).
- **Related**: #642, #983, #1300 (correctness chain), #1194 (measurement hook).
- **Suggested Fix**: Sub-allocate the scratch buffer into K aligned slots and round-robin builds across slots, emitting the serialize barrier only every K builds; falls back to K=1 (current behavior) under memory pressure.

#### D7-NEW-01: Interior NPC/REFR spawn loop has no per-frame or per-NPC budget, unlike the exterior streaming path
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/references.rs:75,224` (`load_references`) → `byroredux/src/npc_spawn.rs:524` (`spawn_npc_entity`); driven from `byroredux/src/cell_loader/transition.rs:237-276` (`load_interior_cell`)
- **Status**: NEW
- **Description**: The exterior streaming path has an explicit, documented per-frame spawn budget (`MAX_CELLS_SPAWNED_PER_FRAME = 2`) specifically to avoid frame-time spikes from bulk main-thread spawning. No equivalent exists for interior cell transitions: `load_references` iterates every `PlacedRef` in one synchronous call, and every NPC hit calls `spawn_npc_entity` inline — no batching, no yield point, no cap. `spawn_npc_entity` makes ~28 separate NIF-load call sites per NPC (skeleton, body, head, hair, eyes, per-slot armor), each a synchronous BSA-extract + NIF-parse + import on the main thread. A crowded interior (Dragonsreach-class, 6000+ entities, dozens of NPCs) pays the full N × (skeleton+outfit load) cost inside the single frame that triggers the transition, before that cell's first frame can even render.
- **Evidence**: `main.rs:1178-1181` documents the exterior budget's rationale but it is never applied to `load_references`/`spawn_npc_entity`; no `Instant::now()`/timing instrumentation exists anywhere in `cell_loader/load.rs` or `references.rs` to even measure the resulting stall.
- **Impact**: An unmeasured multi-hundred-ms-to-multi-second frame-time spike on every interior transition into an NPC-dense cell (door walk-in, save-load-apply reload, fast travel). Architecturally distinct from and precedes the already-open #1698 (a *post-load* Rapier/ECS-scheduler settle-storm confirmed by `docs/audits/AUDIT_RUNTIME_2026-06-26.md`) — the two compound on entry to a crowded cell (load-time freeze, then post-load stall) but are separate mechanisms.
- **Related**: Existing: #1698 (adjacent, not a duplicate — #1698 is post-load).
- **Suggested Fix**: Either extend the `MAX_CELLS_SPAWNED_PER_FRAME`-style budget to interior NPC spawning by chunking `load_references`'s ref loop across frames with a resumable cursor, or at minimum add `Instant::now()` timing around the NPC-spawn portion so the cost becomes visible in the existing telemetry, providing a concrete number before investing in the more invasive chunking fix.

#### PERF-D5-NEW-01: Legacy 16-slot WRS reservoir arrays stay live on the default ReSTIR path — dead per-thread storage in the frame's hottest shader
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/triangle.frag:1967-1983` (declaration + unconditional init), `:2237-2250` (legacy streaming writes), `:2558-2673` (legacy pass-2 reads)
- **Status**: NEW (related: #1369 — CLOSED, the precedent this re-erodes)
- **Description**: #1369 retired a larger reservoir array specifically because per-thread reservoir storage was "the dominant per-thread footprint suppressing WRS occupancy," landing at 128 B (`resLight[16]` + `resWSel[16]`). Session 49 then made ReSTIR-DI (a single scalar reservoir) the default shadow path, but kept the 16-slot legacy WRS arm compiled in for a runtime A/B toggle (`DBG_DISABLE_RESTIR`, a dynamically-uniform branch, not a compile-time constant). The arrays are declared and zero-initialized *before* `useRestir` is even computed, and must survive the entire cluster-streaming loop through pass 2, so the compiler must budget their registers/local memory on every invocation — including the ~100% of production frames that take the ReSTIR path and never read them.
- **Evidence**: `NUM_RESERVOIRS = 16` at `:1967`; unconditional init loop at `:1980-1983`; `useRestir` computed at `:1998` from a runtime-uniform flag; only the `!useRestir` code path touches the arrays afterward.
- **Impact**: Up to ~32 extra live registers (or spilled local bytes) per fragment thread in a shader that already carries the full RT uber-path (ray queries, ReSTIR, GI, glass) and is almost certainly register-bound — silently re-eroding a portion of the #1369 occupancy win, now on a path that gets zero benefit from it. Blast radius: every lit fragment, every frame, every game.
- **Confidence**: MEDIUM — the storage-lifetime analysis is code-verified; the *magnitude* of the occupancy hit needs Nsight/RenderDoc SASS-level confirmation. ALU/register-only, no pipeline-state or barrier change, so the speculative-Vulkan caveat does not block acting on it — bench before/after.
- **Related**: #1369 (the footprint precedent), `DBG_DISABLE_RESTIR` toggle.
- **Suggested Fix**: Promote the legacy-WRS arm to a compile-time toggle through the existing generated-constants channel (the mechanism #1758 used for skin workgroup size); A/B then costs a shader recompile instead of taxing every production frame.

#### PERF-D5-NEW-02: One-bounce-GI hit irradiance samples the first 8 lights in upload order, not the 8 relevant to the hit point
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/include/lighting.glsl:176-226` (`giHitIrradiance`), `include/shader_constants.glsl:39` (`GI_HIT_LIGHT_CAP = 8`); light upload order `byroredux/src/render/lights.rs:73-160`
- **Status**: NEW (landed with "true one-bounce GI"; not covered by the 2026-06-16 or 2026-06-23 audits)
- **Description**: `giHitIrradiance` loops over `min(lightCount, GI_HIT_LIGHT_CAP)` of the global light SSBO **prefix**. `collect_lights` uploads the cell directional first, then point lights in arbitrary ECS sparse-set iteration order — proximity-blind. In any cell with more than 8 lights (taverns, Whiterun/Dragonsreach-class interiors), the GI bounce permanently ignores every light past index 8, while still firing up to 8 shadow rays against a fixed prefix that may be entirely out of range of the hit point. The primary-fragment path solved this exact problem with clustered culling + RIS; the bounce path regressed to an unsorted prefix.
- **Evidence**: `lighting.glsl:178` `count = min(lightCount, GI_HIT_LIGHT_CAP)`; index order = upload order, confirmed by `lights.rs`'s plain ECS-iteration push loop.
- **Impact**: Two-sided: quality (bounce lighting in >8-light cells is systematically wrong/dim and can flicker across cell reloads as ECS iteration order changes which 8 lights exist for GI) and efficiency (up to 8 ray-query traces per lit fragment — the single largest per-pixel ray budget in the frame — spent on an unprioritized set). The per-pixel cap itself is doing its perf job correctly; this is about spending that capped budget on the wrong lights.
- **Confidence**: HIGH on the premise (both sides code-verified); impact magnitude is scene-dependent.
- **Related**: `GI_HIT_LIGHT_CAP` comment (the cap's own intent).
- **Suggested Fix**: Prioritize the prefix CPU-side (sort `gpu_lights[1..]` by intensity·radius, one small sort per frame) so "first 8" approximates "8 most influential"; or select per-hit by distance with an early-out after 8 contributing lights instead of 8 indexed lights. Keep the ray-count cap unchanged.

---

### LOW

#### PERF-D1-NEW-01: `about_to_wait` runs a full MeshHandle+TextureHandle dedup walk every frame for on-demand-only telemetry
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/main.rs:2262-2286`
- **Status**: NEW
- **Description**: Every frame, `about_to_wait` iterates the entire `MeshHandle` and `TextureHandle` storages and inserts each non-zero handle into a persistent `HashSet` (allocation-free since #1584) to compute `meshes_in_use`/`textures_in_use` dedup counts. The only consumers are the `stats` console command and the debug-server entity evaluator — both on-demand, neither per-frame; `log_stats_system` (1 Hz) doesn't print these fields.
- **Evidence**: `main.rs:2266-2281` runs unconditionally before the scheduler each frame.
- **Impact**: CPU cost scaling linearly with mesh-entity count — plausibly approaching ~1 ms/frame on a dense exterior grid or Skyrim city, a double-digit share of the frame budget at the 300+ FPS bench rates this engine targets, spent on a stat nobody reads that frame. No quantitative guard exists for this site.
- **Related**: #1584 (made the walk allocation-free but kept it per-frame), #637 (introduced the telemetry).
- **Suggested Fix**: Throttle to the diagnostics cadence (e.g. every-16-frames or 1 Hz, matching `log_stats_system`), or compute lazily inside the two on-demand consumers.

#### PERF-D1-NEW-02: Per-frame process-environment lookups in hot paths, contrary to the codebase's own `OnceLock` convention
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/main.rs:2242-2245`, `byroredux/src/render/mod.rs:333`, `byroredux/src/render/static_meshes.rs:138`
- **Status**: NEW
- **Description**: Three hot-path sites query the process environment every frame (`BYROREDUX_FIXED_DT`, `BYRO_PROFILE`, `BYRO_NO_CULL`) instead of caching once, unlike the sibling `apply_fog_overrides` (`render/mod.rs:52-73`) which explicitly caches via `OnceLock` with the comment "so the hot path doesn't `getenv` per frame." Environment variables cannot change mid-process, so caching all three is semantics-preserving.
- **Impact**: Sub-µs each in steady state; one heap `String` per frame in fixed-dt runs. Consistency/hardening more than a measurable bottleneck. No quantitative guard exists for these sites.
- **Related**: `apply_fog_overrides`'s `OnceLock` pattern — the in-repo precedent.
- **Suggested Fix**: Hoist each into a `OnceLock`, mirroring `apply_fog_overrides`.

#### PERF-D1-NEW-03: `emit_particles` acquires GlobalTransform and performs a dead per-emitter probe every frame
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/render/particles.rs:48-55`
- **Status**: NEW
- **Description**: `emit_particles` hard-requires a `GlobalTransform` query and then, per emitter per frame, executes a discarded `gtq.get(entity)` lookup with a comment claiming the transform is "sampled by the system at spawn" — `gtq` has no other use in the function.
- **Impact**: Micro (emitter counts are small); primarily misleading dead code. No quantitative guard exists for this site.
- **Related**: `particle_system` (`byroredux/src/systems/particle.rs:317-325`) is the real transform consumer.
- **Suggested Fix**: Delete the `gtq` acquisition and the dead probe; take only the `ParticleEmitter` query.

#### D2-NEW-03: Two-sided glass split runs on additive particle batches — 2× draws + a fully-culled vertex pass with zero compositing benefit
- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1086-1088,1159-1172`; `byroredux/src/render/particles.rs:96`
- **Status**: NEW
- **Description**: Particles emit `two_sided: true` + `alpha_blend: true`, so every particle batch hits `needs_split = is_blend && two_sided` and dispatches twice (FRONT-cull then BACK-cull), excluded from indirect grouping. The split stabilizes TAA depth-winner flips on volumetric glass — a rationale requiring depth writes and order-dependent compositing. Particles have `z_write: false` and (post-#1649) the dominant presets are additive (order-independent); billboards are camera-facing, so the FRONT-cull pass rasterizes ~nothing while still shading the whole instanced batch.
- **Impact**: 2× draw calls and 2× vertex invocations for all live particles; batch counts are small post-#1649 so absolute cost is minor, but the first pass is provably dead work for the additive/no-depth-write case.
- **Related**: #1649, glass-split design (Tier C plan).
- **Suggested Fix**: Narrow the split predicate, e.g. `needs_split = is_blend && two_sided && z_write`, or exclude order-independent blends (`dst == ONE`).

#### D2-NEW-04: Static-mesh hot loop pays a redundant GlobalTransform re-probe and a late IsFxMesh gate
- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/static_meshes.rs:147,175,280`
- **Status**: NEW
- **Description**: Two skip-ordering inefficiencies in the per-entity draw enumeration: the #1377 hoist probes `tq.get(entity).is_none()` then re-fetches the same component at line 175, two storage lookups per drawn entity where one binding would do; and the `IsFxMesh` skip fires only after ~12 optional-component gets and the frustum-sphere test, all discarded for FX entities.
- **Impact**: One extra storage get per rendered entity per frame (tens of µs at FO4 MedTek scale) plus ~12 wasted gets per FX entity. Micro-scale but this is the single hottest CPU loop in `build_render_data`.
- **Related**: #1377 (the original hoist), #1136 (FX marker precompute).
- **Suggested Fix**: Replace the two-step probe with a single `let Some(transform) = tq.get(entity) else { continue; };`, and hoist the `fx_q` gate immediately after the visibility skip.

#### D2-NEW-05: `draw_sort_key` omits the `wireframe` pipeline axis — sort key and `PipelineKey` no longer in lockstep
- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/mod.rs:192-240`; `crates/renderer/src/vulkan/context/draw.rs:3072-3082`
- **Status**: NEW
- **Description**: #869 added `wireframe` to `PipelineKey`, making it a batch-merge and pipeline-bind boundary, but the sort key was never extended with a matching tuple slot. A wireframe draw interleaved among fill draws lands mid-run, splitting the instanced batch and forcing extra pipeline binds.
- **Impact**: Near-zero on shipped content (`NiWireframeProperty` is essentially absent from real assets) — a lockstep/hardening gap that becomes real if wireframe content (debug modes, mods) ever coexists with fill geometry.
- **Related**: #869 (wireframe pipeline variant), #1581 (`group_state` lockstep discipline).
- **Suggested Fix**: Fold `wireframe` into an existing u8 slot (e.g. pack with `two_sided`), and extend the sort-key/merge-axis lockstep test.

#### PERF-D3-NEW-03: `memory-budget.md` links the BGSM cache section to a path deleted by the `asset_provider` module split
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `docs/engine/memory-budget.md:149`
- **Status**: NEW
- **Description**: The doc links `byroredux/src/asset_provider.rs`, which no longer exists — the module split into `byroredux/src/asset_provider/{mod,archive,material,script,texture,tests}.rs`. All documented values remain correct; this is pure link rot.
- **Impact**: Doc-only — future work following the authoritative doc lands on a 404 path.
- **Related**: Session 34/35 module-split memory notes.
- **Suggested Fix**: Point the link at `byroredux/src/asset_provider/material.rs`, and sweep the doc's other relative links while there.

#### PERF-D4-NEW-02: `upload_lights` silently truncates past `MAX_LIGHTS` (512) — no overflow warn, no proximity prioritization
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:25`; producer `byroredux/src/render/lights.rs:73-177`
- **Status**: NEW
- **Description**: `collect_lights` appends every `LightSource` with no cap, sort, or camera-proximity priority; `upload_lights` clamps to 512 silently. Every sibling overflow path (instances, indirect draws, terrain tiles, material intern) warns; lights only get a once-per-session info-log on the first frame, before dense cells load.
- **Impact**: On content with >512 live lights (plausible on Skyrim radius-3+ exterior grids), lights past index 511 in storage-iteration order vanish with zero telemetry — a light adjacent to the camera can be the one dropped, and diagnosing "this torch doesn't light" costs a debugging session.
- **Related**: #279 (instance-overflow warn precedent), #797 (material cap-and-warn).
- **Suggested Fix**: Add the same `log::warn!` pattern on `lights.len() > MAX_LIGHTS`; optionally sort by distance-to-camera before the clamp.

#### PERF-D4-NEW-03: `upload_indirect_draws` lacks the dirty gate both its siblings have
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:594-626`; caller `crates/renderer/src/vulkan/context/draw.rs:3225-3238`
- **Status**: NEW
- **Description**: Instances (#1134) and materials (#878) both got content-hash dirty gates justified by "static interiors produce byte-identical slices each frame." The indirect command list, derived from the same batches, is byte-identical under the exact same conditions but is memcpy'd + flushed unconditionally every frame.
- **Impact**: Small in absolute terms (worst realistic ≈160 KB/frame ≈ 10 MB/s at 60 fps) — a consistency/completeness gap in the established pattern rather than a measurable frame cost on the dev box.
- **Related**: #878, #1134.
- **Suggested Fix**: Reuse the existing FxHash-over-slice + per-FIF `last_uploaded_hash` pattern (~15 lines mirroring `upload_instances`).

#### PERF-D4-NEW-04: Stale byte-math in scene_buffer comments — `GpuLight` quoted at 48 B (is 64 B), `GpuInstance` quoted at 72 B (is 112 B, twice)
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:10`; `upload.rs:492-494`; `descriptors.rs:292-295`
- **Status**: NEW
- **Description**: `GpuLight` is actually 64 B (matching the correct `memory-budget.md`), and `GpuInstance` is 112 B (pinned by the layout test) — three in-code comments still quote the pre-R1 sizes, understating live PCIe traffic estimates.
- **Impact**: Doc-rot only — sizes that matter (buffer allocation, flush ranges, tests) all derive from `size_of`, not the comments.
- **Related**: memory-budget.md (already correct); #1134, #1587.
- **Suggested Fix**: Update the three comments to 64 B / 112 B and recompute the quoted per-frame figures.

#### D6-04: Fixed per-frame skinning costs run even on fully-clean frames
- **Severity**: LOW
- **Dimension**: Skinning & BLAS
- **Location**: `byroredux/src/render/mod.rs:308,328`; `byroredux/src/render/skinned.rs:131-134`; `crates/renderer/src/vulkan/context/draw.rs:2630-2732`
- **Status**: NEW (distinct from #1379, which fixed the monotonic high-water range, not the per-frame recurrence; #1195 gated only the vertex-skinning dispatch + refit)
- **Description**: When `pose_dirty` is empty and no bind-inverse uploads are pending, the frame still pays: CPU identity refill of the whole live `bone_world` range, full-range staging memcpy, staging→device copy, and a full-range `skin_palette.comp` dispatch recomputing byte-identical palettes. None of this chain consults `pose_dirty`.
- **Impact**: For S live slots, ≈S × 9.2 KB per frame for each sub-step — well under a millisecond at realistic slot counts (LOW), becoming relevant only on dense crowd cells.
- **Related**: #1379, #1195, #1284.
- **Suggested Fix**: Track frames-since-last-dirty and skip upload+copy+dispatch when ≥`MAX_FRAMES_IN_FLIGHT` with no pending uploads; stop clearing `bone_world` every frame, re-seeding identity only for freshly (re)allocated slots.

#### D6-05: First-sight entities pay a redundant BLAS UPDATE immediately after their fresh BUILD in the same command buffer
- **Severity**: LOW
- **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1848-1870,1878`
- **Status**: NEW
- **Description**: A first-sight entity is always dirty, so the refit-gate condition is false and the loop proceeds to `refit_skinned_blas` — a full UPDATE against the identical vertex data the BUILD consumed moments earlier in the same command buffer. The block comment asserts the fall-through is harmless, but `refit_skinned_blas` has no "freshly built this frame" short-circuit and records a real UPDATE, also inflating `refits_attempted`/`refits_succeeded` on spawn frames.
- **Impact**: One redundant AS UPDATE + barrier per skinned entity per spawn frame only; steady-state unaffected. Minor telemetry skew on spawn frames.
- **Related**: #911 (batched first-sight builds), #1196.
- **Suggested Fix**: Track entities built this frame and `continue` past the refit for them; fix the stale comment either way.

#### PERF-D5-NEW-03: SVGF à-trous recomputes the 5×5 spatial-variance estimate in all 5 iterations
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/svgf_atrous.comp:108-150`; dispatch loop `crates/renderer/src/vulkan/svgf.rs:1258-1288`
- **Status**: NEW (Session 49 addition)
- **Description**: Each of the 5 à-trous iterations re-derives a 5×5 local luminance variance and re-runs the 3×3 temporal-variance prefilter before the 25-tap edge-stopped filter. The spatial estimate is a legitimate iteration-0 concern (catches converged-but-noisy pixels), but iterations 1-4 run it against already-filtered color whose local variance shrinks monotonically, mostly duplicating work with diminishing contribution.
- **Impact**: Constant-factor bandwidth/ALU on 5 full-screen dispatches (~460M extra, heavily L2-cached, texel fetches/frame at 1440p); the pass remains strictly O(pixels).
- **Confidence**: HIGH on the cost; the safety of computing spatial-variance once and propagating it needs a visual A/B against the dark-floor moiré regression scene before shipping.
- **Related**: #1662 / Session 49 denoiser overhaul.
- **Suggested Fix**: Compute the spatial-variance estimate in iteration 0 only, propagate through the unused moments-image channel, falling back to temporal-variance-only weight in later iterations.

#### PERF-D5-NEW-04: ReSTIR reservoir SSBOs (~130-530 MB screen-dependent) are absent from `memory-budget.md` and all VRAM telemetry
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/restir.rs:34-69`; `docs/engine/memory-budget.md` (no entry)
- **Status**: NEW
- **Description**: Session 49 added two device-local, screen-sized reservoir buffers (one per FIF slot) — ~127 MB at 1080p, ~236 MB at 1440p, ~531 MB at 4K — the largest single VRAM addition of the denoiser overhaul, but the authoritative per-pass VRAM ledger has no row for it and no telemetry attributes it.
- **Impact**: Budget-accounting drift only — no leak (create-once + recreate-on-resize with fenced destroy is correct). At 4K this is >13% of the ~4 GB engine budget going untracked, the same class of gap that has historically preceded budget regressions.
- **Confidence**: HIGH (arithmetic + grep).
- **Related**: RT VRAM budget baseline (~4 GB target); #1583/#1590 (removed the per-pixel reservoir G-buffer attachment this SSBO pair replaced, inheriting no doc entry).
- **Suggested Fix**: Add a "ReSTIR reservoirs" row to memory-budget.md with the W×H×64 B formula, and include both buffers in the renderer's memory-usage log line.

---

## Existing Issues Re-confirmed (not re-filed)

| Issue | Note |
|---|---|
| #1698 (OPEN, HIGH, performance) | Skyrim Dragonsreach ECS scheduler stalls ~140 ms/frame for ~28 s — confirmed as a **post-load** Rapier/ECS-scheduler settle-storm (`docs/audits/AUDIT_RUNTIME_2026-06-26.md`). D7-NEW-01 above is the **load-time** freeze that precedes and compounds with it — related, not a duplicate. |
| #1763 / TD9-001 (OPEN, LOW, tech-debt) | NIF heap-allocation dhat regression tests never run in CI (`dhat-heap` feature dormant). Re-confirmed via `.github/workflows/ci.yml` — still accurate, both bound test files ran green locally this session. |

## Resolved Since Last Audit (verified, not re-filed as findings)

| Prior finding | Resolution |
|---|---|
| PERF-2026-06-23-01 (LOW) — AnimationPlayer-path text-event Vec allocated fresh per frame | **RESOLVED** via #1725: `player_events` now lives in the persistent `AnimScratch` struct, reused via `clear()`. |
| D1-NEW-01 (2026-06-16, LOW) — scheduler `run()` allocates ~25 Strings + a Vec unconditionally | **RESOLVED** via #1647: the timing tracker is `Option`-gated on `SchedulerSystemTimings` resource presence; `None` arm skips both the `Instant::now()` probe and the `String` allocation. |
| D2-NEW-01 (2026-06-16, MEDIUM) — additive particle billboards never instance-batch | **RESOLVED** via #1649 (commit `067a8354`): transparent branch of `draw_sort_key` special-cases additive blend (mesh dominates depth); regression tests added. |
| D5-NEW-01 (2026-06-16, LOW) — stale "7th reservoir attachment" comment | **RESOLVED**: comment now correctly documents 6 color attachments + depth. |
| F8 (2026-06-14) — full-allocation `flush_if_needed` | **RESOLVED** via #1587: all five upload sites migrated to range-limited, atom-aligned `flush_range`. |

---

## Prioritized Fix Order

Quick wins (scratch reuse, preallocation, gate restoration) before architectural changes, per `_audit-common.md` convention:

1. **D6-01 (HIGH)** — make the `SkinSlotPool` bind-inverse drain transactional against `draw_frame`'s early-return paths. This is the one finding with a real (if narrow) correctness blast radius — garbage skinned geometry surviving into both raster and the BLAS/TLAS.
2. **D6-02 (MEDIUM)** — same transactional fix pattern covers the pose-hash commit-ordering hole; do both in one pass since they share root cause and touch the same call sites.
3. **PERF-D3-NEW-01 (MEDIUM, quick win)** — thread `pending_bytes` into `evict_unused_blas`'s gate; small, well-scoped change restoring the mid-batch eviction the #510 mechanism was built to provide.
4. **D2-NEW-02 (MEDIUM, quick win)** — quantize the particle fade parameter before the color LERP (~5-line change); restores the ~97% material-dedup hit rate #781 assumed.
5. **PERF-D5-NEW-01 (MEDIUM)** — compile-time-gate the legacy 16-slot WRS arrays behind the existing generated-constants channel; recovers register/occupancy headroom on every production frame at the cost of a shader recompile for the A/B path.
6. **PERF-D5-NEW-02 (MEDIUM)** — sort or distance-select the GI-hit light prefix; fixes both a quality bug (wrong bounce color in dense cells) and wasted ray budget in one change.
7. **PERF-D4-NEW-01 (MEDIUM)** — file a live issue for variable-stride bone_world packing (the debt comment currently points at a closed milestone); scope as its own follow-up given the packing-scheme design work involved.
8. **D7-NEW-01 (MEDIUM, quick win first)** — add `Instant::now()` timing around interior NPC spawn to quantify the stall before deciding whether to invest in chunking.
9. **PERF-D3-NEW-02 (MEDIUM)** — add a lazy re-acquire path for evicted-but-still-drawn static BLAS, mirroring the skinned first-sight build.
10. **D6-03 (MEDIUM)** — measure via `gpu_skin_blas_refit_ms` on a crowd scene before investing in scratch sub-allocation; likely sub-priority pending that number.
11. All LOW findings — batch as a single hygiene pass (env-var caching, redundant probes, stale comments, missing warns, doc-link fix); each is independently trivial.
12. **Process recommendation**: run a fresh R6a-stale-15 three-scene GPU bench. ROADMAP's bench-of-record is now 437 commits stale and predates both the Session 47 camera-origin work and the Session 49 RT-denoiser overhaul that produced most of this audit's Dimension 5 findings — no current FPS claim is possible without it.

No finding in this audit requires reverting a Vulkan render-pass, barrier, or pipeline-state change speculatively; PERF-D5-NEW-01/02/03 are ALU/register/algorithm-level and explicitly flagged with their confidence level rather than asserted as certain wins.
