# Performance Audit — ByroRedux — 2026-05-31

**Command**: `/audit-performance` (all 10 dimensions, `--depth deep`)
**Method**: 10 dimension agents (renderer / ECS / general specialists), read-only, hot-path-traced with per-frame cost estimates. Cross-dimension duplicates merged below.
**Dedup baseline**: 18 open GitHub issues (none performance-tagged); prior `AUDIT_PERFORMANCE_*` reports through 2026-05-24.
**Context**: this session landed three perf-relevant commits — the distant-terrain **LOD ring** (`cell_loader/terrain_lod.rs`), the **terrain UV-tiling** fix, and **incremental `world_bound_propagation`** (`GlobalTransform`/`LocalBound` change-tracking). The audit covered the current tree including these.

---

## Executive Summary

| Severity | Count (deduplicated) |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 9 |
| LOW | 13 |

**The engine is in good shape.** Every must-not-regress baseline across all 10 dimensions was verified intact (NIF parse, GPU memory discipline, skinning dispatch/refit/descriptor gates, material dedup, streaming worker, lock-tracker gating). No correctness regressions, no GPU-crash/UB/leak findings, no Vulkan spec violations. The incremental-bounds change landed this session was verified **structurally sound** (sole dirty-set drainer, no leak, cycle-safe, all four ECS baselines intact).

**The cost is CPU-side, in the per-frame render-data build**, and almost all of it is *type-erasure and hashing overhead* with localized fixes that do **not** require the M40 RenderExtract-stage refactor:

- **One HIGH**: `QueryRead::get` re-runs a `downcast_ref` on every single component access — ~391K type-erased downcasts/frame at radius-8 (~1.2–2.3 ms of the ~8.84 ms `brd_ms`). Engine-wide, fixed by caching the storage pointer once at query construction.
- **The SipHash cluster** (MEDIUM): `material_hash` runs ~75 SipHash-1-3 `write_u32` per draw (even on the ~97% dedup-hit path), and the whole-table/instance dirty-gates re-SipHash the full buffer each frame. One `DefaultHasher → FxHash/ahash` swap fixes all three sites (~0.8–1.5 ms/frame combined).
- **LOD-ring boot cost** (MEDIUM, self-inflicted this session): ~600 blocks each upload via `with_one_time_commands(staging_pool: None)` → ~1000–1200 serialized GPU fence-waits + ~1250 tiny device-local sub-allocations, all as one-time boot stall. And the ring is not yet wired into per-frame streaming (Slice 2).

**Estimated frame-time recoverable from the HIGH + SipHash MEDIUM alone: ~2–3.5 ms/frame** of the render-data build window, with no architectural change. The WRS-reservoir GPU finding is occupancy-bound and must be RenderDoc-gated before shipping.

---

## Hot-Path Analysis (per-frame, radius-8 / ~23K draws baseline)

| Per-frame operation | Est. cost | Finding |
|---|---|---|
| `QueryRead::get` downcasts (~17 × 23K ≈ 391K) | **~1.2–2.3 ms** | H1 |
| `material_hash` SipHash (~75 write_u32 × 23K) | ~0.3–0.6 ms | M1 |
| material/instance dirty-gate SipHash (full buffer) | ~0.45 ms | M1 |
| `world_bound_propagation` (incremental, this session) | ~0.23 ms (was ~2.0 ms) | ✅ fixed |
| `take_dirty` dirty-set reallocs (2/frame on motion) | 2 allocs | M4 |
| `animation_system` collect() Vecs (3/frame, animated cells) | 3 allocs | M5 |
| `build_debug_ui_snapshot` clone (even when overlay hidden) | ~µs + 3 heap clones | M9 |
| WRS streaming reservoir loop (GPU, per fragment in clustered arm) | occupancy-bound | M2 |
| TAA / SVGF / volumetrics / bloom dispatch | O(pixels)/O(froxels) — clean | — |

`physics_sync_system` (measured ~7–8 ms at radius-8, now the dominant scheduler cost) is **out of this audit's dimension scope** (it is the Rapier sync subsystem, not the render/ECS/NIF/streaming surfaces audited here) but is flagged here as the highest-value next target for a dedicated pass.

---

## Findings (deduplicated, grouped by severity)

### HIGH

#### H1 — `QueryRead::get` re-runs `downcast_ref` on every access (~391K downcasts/frame)
- **Severity**: HIGH
- **Dimensions**: Draw Call Overhead + ECS Query Patterns (merged `PERF-D3-NEW-06` + `PERF-D4-NEW-03`)
- **Location**: `crates/core/src/ecs/query.rs:44-53` (`storage()`/`get()`), `:98-119` (`QueryWrite` mirror); hot caller `byroredux/src/render/static_meshes.rs:133-648`
- **Status**: NEW
- **Estimated impact**: ~391K downcasts/frame at r8/23K (~17 optional gets × 23K entities) → **~1.2–2.3 ms/frame** of the ~8.84 ms `brd_ms`. Pure overhead producing no data.
- **Description**: `QueryRead<T>` holds the `RwLockReadGuard<Box<dyn DynStorage>>` for the whole loop, so the concrete `&T::Storage` is invariant once acquired — but `get()` calls `storage()`, which re-runs `guard.as_any().downcast_ref::<T::Storage>().expect(...)` (a non-inlinable vtable dispatch + a 16-byte TypeId compare + a never-taken `.expect` branch) on **every** access. The same downcast result is recomputed ~17× per entity across 23K entities. This is the single largest *avoidable* hot-loop cost engine-wide (it taxes every `get`/`get_mut`/`contains` in every system, not just the static-mesh collector).
- **Fix** (local, no architecture change, lands before M40 RenderExtract): cache the downcast once in `QueryRead::new` / `QueryWrite::new` — store a `*const T::Storage` resolved from the guard at construction and return it from `storage()` (sound: the guard field keeps the lock held and the box address stable for the struct's lifetime; the box is never reallocated under a live guard). `storage()` becomes a field read; `SparseSetStorage::get`/`PackedStorage::get` become directly inlinable. This fixes EVERY `get` caller. Lower-leverage alternative: hoist `q.storage()` into a `&T::Storage` local once at the top of `collect_static_mesh_draws` and index it directly.
- **Validation**: benchmark `brd_ms` before/after at r8; the 1979+ test suite must not regress (this touches the hottest ECS surface).

---

### MEDIUM

#### M1 — SipHash on the render hot path: per-draw `material_hash` + whole-buffer dirty-gates
- **Severity**: MEDIUM
- **Dimensions**: Draw Call Overhead + Material Table & SSBO (merged `PERF-D3-NEW-07` + `PERF-D8-NEW-01` + `PERF-D8-NEW-02`)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:503-620` (`DrawCommand::material_hash`, ~75 `write_u32`), called `byroredux/src/render/static_meshes.rs:647` + `render/particles.rs:204`; dirty-gates `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:218-231` (`hash_material_slice`) + `:243-254` (`hash_instance_slice`); table probe `MaterialTable::index` (SipHash key)
- **Status**: NEW
- **Estimated impact**: per-draw hash ~0.3–0.6 ms/frame at 23K draws (mostly wasted — the #781 fast path skips `to_gpu_material()` on dedup hit but the *key* is computed unconditionally before the probe). Instance dirty-gate ~0.45 ms/frame on a 7359-instance scene (full-buffer SipHash on the CPU critical path; the upload it saves is async DMA). **Combined ~0.8–1.5 ms/frame.**
- **Description**: every SipHash site here uses `std::collections::hash_map::DefaultHasher` (SipHash-1-3, DoS-resistant — irrelevant for an internal render key never exposed/persisted). The per-draw `material_hash` is the dominant arithmetic cost after H1's downcasts.
- **Fix**: (a) **immediate** — add `rustc-hash` (FxHash) or `ahash` and swap `DefaultHasher` for it across `material_hash`, `hash_gpu_material_fields`, `hash_material_slice`, `hash_instance_slice`, and the `MaterialTable::index` map (`FxHashMap`/`ahash::RandomState`). ~5–10× faster for short keys, behavior-identical (collision resistance irrelevant; the debug-only collision assert at `material.rs:1055` stays). One-line dep + a few swaps, all behind the existing lockstep test (`material_hash_matches_gpu_material_field_hash`). (b) **deferred (M40)** — carry a stable interned material id on the `Material` component (content-addressed at `translate_material`) so the per-draw 75-field walk is skipped entirely except on first sight.

#### M2 — WRS streaming reservoir loop is O(cluster.count × 16) + 320 B/fragment cuts occupancy
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline (`PERF-D1-NEW-01`)
- **Location**: `crates/renderer/shaders/triangle.frag:2684` (`NUM_RESERVOIRS = 16`), `:2690-2698` (arrays), `:2702-2995` (streaming pass), `:3004-3103` (shadow-ray pass)
- **Status**: NEW
- **Estimated impact**: after the 8→16 bump, ~192 `interleavedGradientNoise` evals + 192 conditional array writes per fragment at cluster.count≈12 (double the prior); and `uint[16]+float[16]+vec3[16]` = **320 B/thread local storage** (up from 160 B) suppresses occupancy across the *entire* clustered-lighting branch (BSDF + GI included), not just the reservoir code. The shadow-ray count itself is correctly bounded ≤16/fragment — this is occupancy/ALU/divergence, not a ray blowup.
- **Fix** (do NOT ship blind — RenderDoc/Nsight occupancy capture before/after, per `feedback_speculative_vulkan_fixes`): (1) make `NUM_RESERVOIRS` a spec-constant clamped to `min(16, cluster.count)`; (2) **drop `resRadiance[16]`** (the biggest array, 192 B) and recompute radiance in pass 2 (which already recomputes `L`) — single highest-leverage occupancy fix; (3) hoist the loop-invariant per-`s` noise offsets out of the `ci × s` double loop.

#### M3 — LOD-ring boot upload: ~1000–1200 serial fence-waits + ~1250 tiny device-local sub-allocations
- **Severity**: MEDIUM (one-time boot/initial-radius stall; large magnitude, non-recurring)
- **Dimensions**: GPU Memory + World Streaming (merged `PERF-D2-NEW-01` + `PERF-D9-NEW-01`)
- **Location**: `byroredux/src/scene/world_setup.rs:753-761` → `byroredux/src/cell_loader/terrain_lod.rs:79-320` (`spawn_lod_ring`/`spawn_lod_block`) → `crates/renderer/src/mesh.rs:244-296` (`upload`/`upload_scene_mesh`)
- **Status**: NEW (introduced this session by the LOD ring)
- **Estimated impact**: ~500–600 non-hole blocks, each calling `upload_scene_mesh(staging_pool: None)` → `create_vertex_buffer` + `create_index_buffer`, each routing through `with_one_time_commands` which **creates a fresh fence and blocks on `wait_for_fences` per submission** = **2 synchronous GPU round-trips per block (~1000–1200 serialized fence-waits)** + ~1250 tiny (~6–29 KB) device-local sub-allocations. Multi-hundred-ms to multi-second one-time stall, *after* all full-detail cells already loaded. Byte budget is fine (~22 MB, ~4–6% of pool soft caps); the cost is the fence-wait serialization and sub-allocation count/fragmentation.
- **Fix**: (1) accumulate all block geometry into the global `pending_vertices`/`pending_indices` pool and rely on a single `rebuild_geometry_ssbo` (LOD draws read the global SSBO, so the per-mesh buffers are unused anyway) — collapses ~1250 sub-allocations → ~2 and ~1200 fence-waits → ~1; OR use `with_one_time_commands_reuse_fence` (`texture.rs:569`, which exists for exactly this pattern) + a shared `StagingPool`. (2) Defer the ring to a few frames after first present (distant geometry; 1–2 frame pop-in is invisible) so the window appears before the ring finishes. (3) Optionally move block mesh-gen onto the cell-stream worker (CPU-only; upload stays main-thread). Pairs with M6.

#### M4 — `take_dirty()` reallocates the dirty-set Vec every frame (capacity NOT reused; doc comment wrong)
- **Severity**: MEDIUM
- **Dimension**: CPU Allocations (`PERF-D6-NEW-01`)
- **Location**: `crates/core/src/ecs/packed.rs:56-58` (`take_dirty`); consumers `crates/core/src/ecs/systems.rs:87` (transform prop) + `byroredux/src/systems/bounds.rs:55-60` (bound prop, this session)
- **Status**: NEW
- **Estimated impact**: 2 heap allocations/frame (one per dirty-tracked storage) on any frame ≥1 entity moves; each re-grows from zero capacity (0→1→2→4→…→N). Bounded by the moved-entity count, not unbounded. Steady-state static cell: zero.
- **Description**: `take_dirty()` is `std::mem::take(&mut self.dirty)` which swaps in a **zero-capacity** Vec — the next `mark_dirty` re-grows from 0. **The doc comment claims "the backing capacity is reused next frame" — this is factually wrong** (`mem::take` hands the capacity to the caller, which drops it at end of frame).
- **Fix**: add `drain_dirty_into(&mut self, out: &mut Vec<EntityId>) { out.clear(); out.append(&mut self.dirty); }` (`Vec::append` empties `self.dirty` *keeping its capacity*), and let `bounds.rs`/`systems.rs` own a persistent `dirty_scratch: Vec<EntityId>` reused across frames (bounds already owns `dirty_roots`/`post_order`/`stack` this way). **Correct the doc comment regardless** of whether the alloc fix lands. The capacity-retention property is unit-testable today (assert `storage.dirty.capacity()` retained across a re-mark cycle); the allocs/frame reduction needs dhat.

#### M5 — `animation_system` allocates 3 unconditional per-frame `Vec`s via `collect()`
- **Severity**: MEDIUM
- **Dimension**: CPU Allocations (`PERF-D6-NEW-02`)
- **Location**: `byroredux/src/systems/animation.rs:382` (`entities_with_players`), `:394` (`playback_states`), `:527` (`stack_entities`)
- **Status**: NEW
- **Estimated impact**: 3 fresh heap allocs/frame whenever any `AnimationPlayer`/`AnimationStack` exists (every animated-NPC cell). Freed and re-collected from scratch each frame.
- **Description**: the system already reuses scratch for the stack inner loop (`#828`) and refills `NameIndex` in place (`#824`), but the three top-of-phase entity-list collections are allocated fresh. The collect-then-drop-lock pattern is needed (can't hold the query lock across apply); the *buffer* can persist.
- **Fix**: promote the three Vecs to closure-captured scratch (mirroring `make_world_bound_propagation_system`'s captured scratch), `clear()`+refill each frame. Behavior unchanged (covered by `animation_system_e2e_tests`); alloc reduction needs dhat.

#### M6 — LOD ring not wired into per-frame streaming — never follows the player + teleport leak (Slice 2)
- **Severity**: MEDIUM (streaming correctness + latent VRAM growth)
- **Dimension**: World Streaming (`PERF-D9-NEW-02`)
- **Location**: `byroredux/src/main.rs:948-1085` (`step_streaming` — no LOD call), `byroredux/src/scene/world_setup.rs:753` (only call site), `byroredux/src/cell_loader/terrain_lod.rs:16-18` (Slice-2 TODO)
- **Status**: NEW (known/pending — reported for tracking)
- **Estimated impact**: 0 ms/frame steady-state today, but distant terrain becomes spatially stale as the player walks (the hole-out stays centred on the spawn cell → near terrain can z-fight/gap against the stale ring), and any re-entry to `stream_initial_radius` (M40 scripted teleport) re-spawns a fresh ~600-block ring with **no unload of the prior one** (LOD blocks are bare entities, untracked in `state.loaded`, never reclaimed).
- **Fix** (Slice 2): track LOD blocks by block-coord in `HashMap<(i32,i32), LodBlock>` on `WorldStreamingState`; in `step_streaming`, on cell-boundary crossing recompute the desired block set around the new player block, load/unload the delta, and regenerate the boundary blocks whose hole pattern changed (16-bit hole mask). Frees the teleport leak too.

#### M7 — `billboard_system` arms one `GlobalTransform` dirty entry per billboard per frame — defeats the static-cell bounds fast path
- **Severity**: MEDIUM
- **Dimension**: ECS Query Patterns (`PERF-D4-NEW-02`)
- **Location**: `byroredux/src/systems/billboard.rs:47-55`; consumed at `bounds.rs:118-130`
- **Status**: NEW
- **Estimated impact**: N_billboards dirty pushes/frame → bounds pass-1 recomputes N leaf bounds/frame even with the camera parked. In a billboard-heavy cell (vegetation impostors, sprite quads) this defeats the incremental-bounds fast path landed this session, regressing it toward its old per-frame cost.
- **Description**: `billboard_system` runs every frame with no change-detection gate and `get_mut`s every billboard's GlobalTransform (writing `rotation`), each push arming the TRACK_CHANGES dirty Vec. For a sphere bound the rotation is irrelevant to center/radius unless `local.center != ZERO`.
- **Fix**: gate `billboard_system` on camera motion (cache last cam_pos/forward, early-return if unchanged — billboards only need re-rotation when the camera moved, exactly when transform_propagation already marks the camera dirty), OR skip the dirty-mark when `LocalBound.center == ZERO`. Camera-motion gating is cleanest.

#### M8 — Late-stage `GlobalTransform` writers leave bounds one frame stale (latent trap)
- **Severity**: MEDIUM
- **Dimension**: ECS Query Patterns (`PERF-D4-NEW-01`)
- **Location**: `byroredux/src/systems/character.rs:426-433` (camera_follow), `byroredux/src/systems/audio.rs:300/337/397`; drain at `bounds.rs:55-60`
- **Status**: NEW
- **Estimated impact**: benign today (camera + audio emitters have no LocalBound, so pass-1 skips them), but a latent correctness trap: bounds drains GlobalTransform's dirty set in PostUpdate, while camera_follow/audio re-arm it in Stage::Late (after the drain) — the moment a Late-stage system writes GlobalTransform on a *bounded* entity, that entity's WorldBound silently lags one frame.
- **Fix**: document+assert the invariant "no Late-stage system may write GlobalTransform on a LocalBound-bearing entity", OR move the bounds drain to Stage::Early of the next frame so it captures the prior frame's complete write set. Pin the PostUpdate ordering contract (propagation → billboard → bounds) in a `main.rs` comment block.

#### M9 — `build_debug_ui_snapshot` clones the metrics snapshot every frame even when the overlay is hidden
- **Severity**: MEDIUM
- **Dimension**: Per-frame Translation & UI Overlay (`PERF-D10-NEW-01`)
- **Location**: `byroredux/src/main.rs:1321-1325` (unconditional call) + `:2245-2284` (body); resource `crates/core/src/ecs/metrics.rs:28-79`
- **Status**: NEW
- **Estimated impact**: a few hundred ns to low-µs/frame **on every frame including when the overlay is hidden** (the boot default) — deep-clones two `BTreeMap<String,f32>` + a `Vec<(String,f32)>`, re-allocating every pass-name `String` each frame, then discards it when `!visible`. The exact "debug overlay that costs when hidden" pattern. (The egui GPU/Vulkan path *is* correctly gated — verified clean.)
- **Fix**: gate the snapshot build on `self.debug_ui.visible` — return a `PanelSnapshot::default()` (metrics:None, entities:None) when hidden; `ui.run` already early-returns on `!visible` and ignores the snapshot.

---

### LOW

| ID | Title | Location | Source |
|---|---|---|---|
| L1 | static_meshes per-entity probes precede the transform presence gate; cache-cold unsorted SparseSet gets | `static_meshes.rs:136-158`, `sparse_set.rs:112` | D3-NEW-08 |
| L2 | WRS pass-2 recomputes IGN/`L` (correct storage-vs-recompute trade; fold into M2 if K reduced) | `triangle.frag:3021-3027` | D1-NEW-02 |
| L3 | LOD pool-cap headroom verified safe but not compile/test-asserted (retune could erode silently) | `mesh.rs:27-32`, `terrain_lod.rs:41-53` | D2-NEW-02 |
| L4 | **dhat/alloc-counter regression coverage still unwired** (recurring since 2026-05-04) — every alloc finding here is estimate-only | workspace-wide; NIF fixture is one bare NiNode | D2-NEW-03, D5 coverage, D6 caveat |
| L5 | SVGF/G-buffer/TAA history reallocated wholesale on swapchain resize (~5–15 ms one-shot hitch @4K) | `gbuffer.rs:77`, `taa.rs:793-826` | D2-NEW-04, D7-NEW-02 |
| L6 | Skinning bone-world upload + palette dispatch sized to monotonic `max_used_slot` high-water (never contracts after a scene shrinks) | `resources.rs:876`, `upload.rs:164`, `draw.rs:728` | D7-NEW-01 |
| L7 | `GpuMaterial` 300 B — each Disney axis is a fresh dedup-distinguishing field (hit-rate watch-item, no action now) | `material.rs:38-271` | D8-NEW-03 |
| L8 | `animate_lights_system` per-frame `Vec<LightUpdate>` alloc + 3-pass lock cycling | `light_anim.rs:76-188` | D4-NEW-04 |
| L9 | render scratch buffers start `Vec::new()` — first frame pays growth reallocs (cold-start only) | `main.rs:867-884` | D6-NEW-04 |
| L10 | `AnimationTextKeyEvents(events.clone())` allocates per fired event (intrinsic; SmallVec would inline) | `animation.rs:433/668` | D6-NEW-03 |
| L11 | NIF `bs_tri_shape` uvs/vertex_colors/triangles clones — **prior NEW-04 premise corrected: genuinely-required copies (borrowed source), recommend wont-fix** | `bs_tri_shape.rs:72/81/167` | D5-NEW-01 |
| L12 | NIF double determinant/degeneracy check (parse sanitize + import re-check); SVD correctly one-time-gated | `stream.rs:624/647`, `coord.rs:47-60` | D5-NEW-02 |
| L13 | Initial-radius bootstrap finishes import+spawn+upload serially on main thread (worker throttled to 1-ahead; known by-design) | `world_setup.rs:660-745` | D9-NEW-03 |

---

## Verified-Clean (no findings — baseline confirmations)

- **GPU pipeline**: per-batch (not per-draw) dynamic state; global VB/IB bound once; multi-draw-indirect grouping; correct TLAS build-vs-refit + AS_BUILD→FRAGMENT|COMPUTE barrier; volumetrics O(froxels); bloom O(pixels). Distant-terrain LOD ring batches cleanly (in_tlas=false, no per-draw waste/log-spam).
- **GPU memory**: host-visible/device-local discipline enforced; StagingPool + budget eviction; BLAS/TLAS scratch grow-then-shrink hysteresis live; coherent-flag-cached flushes; `upload_scene_mesh(rt_enabled=false)` correctly skips RT BLAS-input buffers for LOD meshes.
- **NIF parse**: all 7 baselines intact (#830/#831/#832/#833, #1262/#1263/#1265); 5 of 7 prior NEW findings fixed in-tree; emitter walks verified import-side-only (zero per-frame callers); unknown blocks `block_size`-skipped.
- **Skinning/TAA**: all of M29.5/M29.6/#1195/#1196/#1197 verified live in the current draw loop; TAA O(pixels); skin dispatch per-skinned-entity only (`bone_offset==0` static skip); BLAS refit dominates.
- **Material table**: dedup real, upload O(unique) not O(draws), `GpuInstance` 112 B (3 layout tests), no SSBO index mismatch, NIFAL pin intact (no per-draw `classify_pbr_keyword`, metalness/roughness plain f32).
- **Streaming**: #877 two-phase split, #1262 fast-path, NIF cache, off-thread worker, shutdown-join, BLAS budget (VRAM/3, 256 MB floor), Starfield CDB once-per-load — all good.
- **Incremental bounds (this session)**: sole GlobalTransform-dirty drainer, unconditional drain (no leak), cycle-safe parent-walk, all four ECS baselines (#823/#824/#825/#828) intact.
- **Per-frame/UI**: `apply_emitter_params` build-time/zero-alloc; `particle_system` in-place SoA; egui overlay GPU path fully gated when hidden (no tessellation/upload/pass).

---

## Prioritized Fix Order

**Quick wins (localized, no architecture change, immediate frame-time):**
1. **H1** — cache the storage pointer in `QueryRead::new`/`QueryWrite::new`. ~1.2–2.3 ms/frame, engine-wide. *Highest leverage.*
2. **M1** — add FxHash/ahash, swap `DefaultHasher` at the 5 render-hash sites. ~0.8–1.5 ms/frame, one dep + a few swaps.
3. **M4** — `drain_dirty_into` + persistent scratch (and fix the wrong doc comment). Helps transform-prop + the new bound-prop.
4. **M9** — gate `build_debug_ui_snapshot` on overlay visibility. Trivial; removes per-frame clones in the (default) hidden state.

**Self-inflicted this session (the LOD ring):**
5. **M3** — collapse LOD-block uploads into the global pool + single SSBO rebuild (or reuse-fence/staging-pool); defer ring past first present. Kills the boot stall.
6. **M6** — wire the LOD ring into `step_streaming` (Slice 2, 16-bit hole-mask delta). Closes the stale-ring + teleport-leak.

**Medium effort:**
7. **M5** — animation entity-list collects → captured scratch.
8. **M7** — gate `billboard_system` on camera motion (protects the new bounds fast path in billboard-heavy cells).
9. **M8** — pin the PostUpdate ordering contract + bounded-entity-Late-write invariant.

**GPU (measure-gated):**
10. **M2** — drop `resRadiance[16]` + spec-constant `NUM_RESERVOIRS`. **RenderDoc/Nsight occupancy capture required before shipping** (per `feedback_speculative_vulkan_fixes`).

**Process / hardening:**
11. **L4** — wire `dhat-rs` behind `--features dhat-heap` with one cell-load→unload net-retained-alloc assertion. Unblocks regression-locking every alloc finding above. (Recurring gap since 2026-05-04.)
12. Remaining LOWs as opportunistic cleanup.

**Out of scope but flagged**: `physics_sync_system` (~7–8 ms @ r8) is now the dominant scheduler cost and warrants a dedicated audit pass (Rapier sync / query_pipeline.update over static colliders / kinematic-player wake).
