# Performance Audit — ByroRedux — 2026-06-04

**Command**: `/audit-performance` (all 10 dimensions, `--depth deep`)
**Method**: 10 dimension agents (renderer / ECS / general specialists), read-only, hot-path-traced with exact line citations. Cross-dimension duplicates merged.
**Dedup baseline**: 44 open GitHub issues; prior `AUDIT_PERFORMANCE_*` reports through 2026-05-31.
**Context since last audit (2026-05-31)**: 89 commits. Key changes:
- Multiple prior MEDIUM findings fixed: M4/M5 (#1371/#1372), M7/M8/M9 (#1374/#1375/#1376), L1/L3/L6 (#1377/#1378/#1379), M3/M6 (#1370/#1373)
- `H1` (QueryRead downcast, #1367) and `M1` (SipHash→FxHash, #1368) both CONFIRMED FIXED
- `M2` (#1369, WRS `resRadiance[16]`) **partially addressed**: `resRadiance[16]` array (192B) was retired; per-thread WRS local storage dropped 320B→128B. Issue remains open for `NUM_RESERVOIRS` spec-constant and loop-invariant hoist
- New feature: **ReSTIR-DI reservoir** (`9abbe510`) — 7th G-buffer attachment, write-only this session
- New feature: autostep physics (`99af1f79`)
- New feature: PBR classification overhaul (`83d6a155`)
- Today's fix: decal/render-layer classification (`92fb1693`)

---

## Executive Summary

| Severity | Count (deduplicated) |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 6 |

**The engine is in excellent shape.** Every must-not-regress baseline across all 10 dimensions was verified intact. The H1 (QueryRead downcast, ~1.2–2.3 ms/frame) and M1 (SipHash, ~0.8–1.5 ms/frame) fixes from the last audit are confirmed in-tree — the single largest prior CPU-side recovery (~2–4 ms/frame combined) is live. All streaming baselines, NIF parse baselines, skinning/TAA gates, and material dedup are intact. The M2 WRS occupancy finding made meaningful progress (320B→128B local storage) but is not fully closed.

**The two new MEDIUM findings** are both low-urgency:
- The **ReSTIR-DI reservoir** attachment (66MB VRAM at 1080p, 16B/px) is write-only — no resample pass reads it yet. It is functioning as planned infrastructure for Phase 2, but the VRAM cost is live today with no current reader.
- **`light_anim.rs`** still allocates a fresh `Vec<LightUpdate>` every frame and acquires a dead Transform write lock (Pass 3, Phase 19.5 disabled, `translation == None` unconditional). Confirmed open (#1380) with a new observation about the dead lock.

---

## Hot-Path Analysis (per-frame, r8 / ~23K draws)

| Per-frame operation | Est. cost | Status |
|---|---|---|
| `QueryRead::get` downcast | ~~1.2–2.3 ms~~ → **~0 (field read)** | ✅ H1 FIXED #1367 |
| `material_hash` SipHash | ~~0.3–0.6 ms~~ → **~0.04–0.08 ms (FxHash)** | ✅ M1 FIXED #1368 |
| material/instance dirty-gate SipHash | ~~0.45 ms~~ → **~0.05 ms (FxHash slice)** | ✅ M1 FIXED #1368 |
| `take_dirty` dirty-set reallocs | ~~2 allocs/frame~~ | ✅ M4 FIXED #1371 |
| animation entity-list `collect()` | ~~3 allocs/frame~~ | ✅ M5 FIXED #1372 |
| `billboard_system` unconditional write | ~~N_billboards dirty/frame~~ | ✅ M7 FIXED #1374 |
| `build_debug_ui_snapshot` clone (hidden) | ~~3 heap clones~~ | ✅ M9 FIXED #1376 |
| LOD-ring boot fence-waits | ~~1000–1200~~ | ✅ M3 FIXED #1370 |
| LOD streaming Slice 2 | ~~stale ring + teleport leak~~ | ✅ M6 FIXED #1373 |
| ReSTIR reservoir write (~16 B × ~23K px/dispatch) | 66 MB VRAM, no reader | **N1** |
| WRS streaming loop (128B local, was 320B) | occupancy-improved (#1369) | ⚠️ M2 partial |
| `animate_lights_system` Vec + dead lock | ~1–2 allocs/frame + useless lock | **N2** (#1380) |
| TAA / SVGF / bloom / volumetrics | O(pixels)/O(froxels) — clean | — |

---

## Findings (deduplicated, grouped by severity)

### MEDIUM

#### N1 — ReSTIR-DI reservoir attachment: 66 MB VRAM write-only (no current reader)
- **Severity**: MEDIUM
- **Dimensions**: GPU Pipeline + GPU Memory (merged `PERF-D1-NEW-01` + `PERF-D2-NEW-01`)
- **Location**: `crates/renderer/shaders/triangle.frag:58` (`outReservoir`), `crates/renderer/src/vulkan/gbuffer.rs` (attachment 6, `RESERVOIR_FORMAT = R32G32B32A32_UINT`)
- **Status**: NEW
- **Estimated impact**: 16 B/px × 1920×1080 × 2 FIF = **~66 MB VRAM** continuously allocated. Write-only today: `outReservoir` is written every rendered fragment but no pass reads it. The reservoir stores `(lightIdx u32, M u32, wSum f32, W f32)` for Phase 2 ReSTIR-GI spatial/temporal resampling.
- **Description**: `9abbe510` added the reservoir as a 7th G-buffer color attachment (R32G32B32A32_UINT). The write path in `triangle.frag:2702-2995` (WRS streaming pass) initializes and exports the reservoir correctly. However, the Phase 2 resample compute pass does not yet exist — the attachment is write-only dead VRAM for the current session. At 1080p this is manageable (66 MB < 6 GB RT minimum), but at 1440p/4K it grows to ~118 MB/~264 MB. No resample descriptor binding exists in `composite.rs` or `draw.rs`.
- **Fix**: No immediate action required — this is planned Phase 2 infrastructure. Flag for the VRAM budget monitor (`feedback_vram_baseline` < 4 GB target): until Phase 2 lands, consider reducing format from `R32G32B32A32_UINT` (16 B/px) to `R32G32_UINT` (8 B/px, packing wSum+W into 64 bits) to halve the reservation. Alternatively, gate the attachment creation on a `rt_reservoirs_enabled` feature flag. Reference: `docs/diagnostics/metal-reflection-svgf-fix-plan.md` for the prior attachment-count lockstep experience (5 sites must stay in sync).

#### N2 — `animate_lights_system`: per-frame `Vec<LightUpdate>` alloc + dead Transform lock (Pass 3)
- **Severity**: MEDIUM
- **Dimensions**: ECS Query Patterns + CPU Allocations (merged `PERF-D4-NEW-01` + `#1380`)
- **Location**: `byroredux/src/systems/light_anim.rs:83` (`Vec::new()`), `:76-188` (3-pass lock cycling); specifically Pass 3 lock acquisition at ~line 150 despite Phase 19.5 disable (`translation == None` unconditional)
- **Status**: OPEN (#1380) + **new observation**: Pass 3's `GlobalTransform` write lock is acquired **unconditionally** even though `translation` is hardcoded `None` (Phase 19.5 disabled), making it a dead lock acquisition every frame.
- **Estimated impact**: 1 `Vec<LightUpdate>` alloc/frame whenever any animated light entity exists; + 1 useless RwLock write-acquire/release per light-animation tick. The lock acquisition itself is cheap but it's unnecessary write contention on `GlobalTransform` that defeats parallel query scheduling.
- **Fix**: (a) Add a closure-captured `Vec<LightUpdate>` scratch (same pattern as M5/M7 fixes). (b) Guard Pass 3's entire `query_write::<GlobalTransform>` block on `translation.is_some()` — since Phase 19.5 is disabled this removes the dead lock entirely. Both fixes are independent. **Note dhat-infra gap** (`#1381`): alloc savings are not regression-tested today.

---

### LOW

#### L1 — Dead `IsCollisionOnly` query in `static_meshes.rs` (stale post-`83d6a155`)
- **Severity**: LOW
- **Dimension**: Draw Call Overhead (`PERF-D1-NEW-02`)
- **Location**: `byroredux/src/render/static_meshes.rs:135,201-202`
- **Status**: NEW
- **Description**: Commit `83d6a155` restructured how collision-only entities are handled — they are now **ghost physics entities** (no MeshHandle) spawned alongside the render entity (`spawn.rs:1082-1090`). The render entity re-enters the TLAS; the ghost does not. As a result, the `IsCollisionOnly` query in `collect_static_mesh_draws` is never true on a render entity and can never fire. The query performs a SparseSet probe on every entity (cache-cold) for a condition that is structurally unreachable.
- **Fix**: Remove the `IsCollisionOnly` guard from `collect_static_mesh_draws` and add a comment documenting that collision-only behavior is now handled by the ghost entity pattern.

#### L2 — Player-path text-key `events` Vec not closure-captured (asymmetric with M5 fix)
- **Severity**: LOW
- **Dimension**: CPU Allocations (`PERF-D4-NEW-02`)
- **Location**: `byroredux/src/systems/animation.rs:437`
- **Status**: NEW
- **Description**: Commit `22b5f558` (M5 fix, #1372) captured the `events`/`seen_labels` scratches for the **stack path** at `animation.rs:558`. The **player path** `events` Vec at `:437` was missed — it still allocates fresh per frame for entities with active `AnimationPlayer`s (the common NPC case). One alloc/frame per animated entity.
- **Fix**: Add this Vec to the M5 closure-captured scratch bundle (same `animation.rs` closure, extend the captures). **Dhat-infra gap** applies.

#### L3 — `AnimationStack` inner scratch Vecs not closure-captured
- **Severity**: LOW
- **Dimension**: CPU Allocations (`PERF-D4-NEW-03` + `DIM6-NEW-01` merged)
- **Location**: `byroredux/src/systems/animation.rs:555-559` (4 `Vec::new()` per-stack-entity-per-frame)
- **Status**: NEW
- **Description**: The `events`/`seen_labels` hoisted by M5 are the outer loop scratches. Four additional `Vec::new()` allocations at `:555-559` (blend targets, weight accum, layer indices, root-motion scratch) are not captured — each stack-entity frame pays fresh allocations. These grow transiently (bounded by layer count, typically 2–4) and are dropped each tick.
- **Fix**: Extend the captured scratch bundle with these four Vecs, `clear()` before use. Pairs with L2. **Dhat-infra gap** applies.

#### L4 — Autostep diagnostic: 2 atomic RMW + `log::info!` every 60th airborne frame (release builds)
- **Severity**: LOW
- **Dimension**: Per-frame Translation & UI Overlay (`PERF-D10-NEW-02`)
- **Location**: `byroredux/src/systems/character.rs:257-308` (autostep block added by `99af1f79`)
- **Status**: NEW
- **Description**: The autostep implementation includes a diagnostic block that runs two atomic read-modify-write operations every frame in character mode, plus `log::info!` on the first 5 frames and every 60th frame while airborne. This fires in release builds unconditionally. In an open-world exterior with NPCs and an airborne player, the log-spam timer fires ~1 log/second — non-trivial on slow terminals.
- **Fix**: Wrap the diagnostic block in `#[cfg(debug_assertions)]` or gate on a `BYROREDUX_AUTOSTEP_DIAG` env var. The atomics themselves are cheap; the log macro is the cost on emission frames.

#### L5 — `build_conversation_tree` O(N²) scan (not yet on production path)
- **Severity**: LOW
- **Dimension**: World Streaming (`PERF-D9-NEW-01`)
- **Location**: `crates/plugin/src/esm/records/misc/ai.rs:465,525` (commit `c82bea9c`)
- **Status**: NEW
- **Description**: `build_conversation_tree` uses `infos.iter().find(|i| i.previous_info == current)` at two points (`:465`, `:525`) — O(N) per chain step → O(N²) for a linear chain of N INFO records. The function already builds a `HashMap<FormId, &InfoRecord>` earlier in the body; the `.find()` calls should use the map. Not yet on any production cell-load code path (test-only callers), but will hit when conversation trees are used for NPC AI processing.
- **Fix**: Replace both `.find()` calls with `HashMap` lookups on the already-built map. O(N²) → O(N).

#### L6 — dhat ECS/render coverage still unwired (carry-over #1381)
- **Severity**: LOW
- **Dimension**: CPU Allocations (carry-over `L4`)
- **Status**: STILL OPEN (#1381). **Partial progress**: a `crates/nif/tests/heap_allocation_bounds.rs` CI test now instruments the NIF parser path under `dhat`. The ECS query and render-data build paths (the highest-value targets for findings L2/L3/N2) remain untracked. All alloc findings in this report continue to rely on code review + estimate rather than measurement.
- **Fix**: Add `--features dhat-heap` CI path for `byroredux-core` with a cell-load → unload net-retained-alloc assertion, targeting the `animation_system` and `animate_lights_system` hot paths.

---

## Verified-Clean (no findings — baseline confirmations)

**H1 FIXED**: `QueryRead::get` caches storage pointer in `new()` (`query.rs:32`, `storage()` at `:57-64` = bare field read). `QueryWrite` mirror at `:100/111-115`, `ComponentRef` at `:233/257-261`. Confirmed engine-wide, no vtable dispatch per access.

**M1 FIXED**: `FxHashMap`/`FxHasher` throughout `material.rs` (`:29,793,918,961`), `descriptors.rs` hash_material_slice/hash_instance_slice both use `FxHasher`. No `DefaultHasher` remaining.

**M2 PARTIAL** (#1369 still open): `resRadiance[16]` (192B) retired; Pass 2 recomputes radiance via `shadowableLightRadiance()`. Per-thread WRS storage: 320B → 128B (`uint[16]+float[16]` = 128B). `NUM_RESERVOIRS` still 16; spec-constant and loop-invariant hoist not yet landed.

**M3/M6 FIXED**: `terrain_lod.rs:449` uses `upload_scene_mesh_global_only` (not `with_one_time_commands`). `main.rs:1120` calls `stream_lod_blocks` inside `step_streaming`.

**M4/M5/M7/M8/M9 FIXED**: All confirmed in-tree with exact lines.

**L1 (static_meshes gate) FIXED (#1377)**: GlobalTransform presence gate hoisted to `static_meshes.rs:138-146`.

**L3/L6 FIXED (#1378/#1379)**: LOD pool-cap compile assertions at `terrain_lod.rs:100-107`; `SkinSlotPool::sweep` tail-contraction at `resources.rs:860-879`.

**GPU baselines — all intact**:
- TLAS build-vs-refit + AS_BUILD→FRAGMENT|COMPUTE barrier: intact (`draw.rs:1450-1457`)
- Per-batch (not per-draw) dynamic state: intact
- Skinning M29.5/M29.6/#1194-#1197: all 6 sub-baselines VERIFIED OK with exact line citations
- TAA O(pixels): intact (`taa.comp`, single `cmd_dispatch`)
- Volumetrics O(froxels), bloom O(pixels): intact

**Material/SSBO — all intact**:
- `GpuMaterial` 300B (unchanged; `68c23d3f` was a test rename only)
- `GpuInstance` 112B (3 layout tests pinned)
- NIFAL pin: no per-draw `classify_pbr_keyword`, `metalness`/`roughness` plain `f32` at `material.rs:216,222`
- Material upload: O(0) PCIe in steady state (content-hash dirty gate at `upload.rs:552-555`)

**NIF parse — all 4 baselines intact** (verified with exact lines):
- #830 rayon `pre_parse_cell` + `PRE_PARSE_RAYON_MIN=8` serial fast-path
- #831 `allocate_vec` `#[must_use]`
- #832 `entry().get_mut()/insert` per-block counters
- #833 `read_pod_vec<T>` bulk-array readers (grown from 6 to 11 methods)

**ECS baselines — all 5 intact**: lock_tracker debug-gated, NameIndex clear+insert, transform_propagation cache key, animation_system scratch hoists, `World::despawn` type_names table.

**Streaming baselines — all intact**: #877 two-phase, #1262 threshold, #1263/#1265 bulk-read paths. CDB once-per-load via `Arc<ComponentDatabaseFile>`.

---

## Prioritized Fix Order

**Quick wins (no architecture change):**
1. **L1** — Remove dead `IsCollisionOnly` guard from `collect_static_mesh_draws`. One-line delete + comment. Cleans up cache-cold SparseSet probe.
2. **N2** — Guard `animate_lights_system` Pass 3 GlobalTransform lock on `translation.is_some()`. One conditional — removes dead lock acquisition. Also add closure-captured scratch for the `Vec<LightUpdate>`.
3. **L2/L3** — Extend animation scratch captures to the player-path `events` Vec (`:437`) and the 4 `AnimationStack` inner Vecs (`:555-559`). Pairs with the M5 fix pattern already established.
4. **L5** — Replace O(N²) `.find()` in `build_conversation_tree` with existing HashMap. Two-line fix before conversation trees land on the production path.

**Medium effort:**
5. **N1** — Evaluate ReSTIR reservoir format shrink: `R32G32_UINT` (pack wSum+W, 8B/px → ~33 MB) vs keeping `R32G32B32A32_UINT` until Phase 2. Requires shader + Rust lockstep change (5-site attachment contract per `metal-reflection-svgf-fix-plan.md`). **Do not speculate — measure Phase 2 timeline first** (per `feedback_speculative_vulkan_fixes`).
6. **L4** — `#[cfg(debug_assertions)]` gate on autostep diagnostic block.

**Process / hardening:**
7. **L6** — Wire dhat for `animation_system` / `animate_lights_system` under `--features dhat-heap`. Unblocks regression-locking L2/L3/N2 alloc fixes. Recurring since #1381.
8. **M2** — Spec-constant `NUM_RESERVOIRS` + `resRadiance` already retired; remaining: hoist loop-invariant noise offsets. RenderDoc occupancy capture required before shipping (#1369).

**Out of scope but flagged**: `physics_sync_system` remains the dominant scheduler cost (~7–8 ms @ r8), unchanged from last audit. Warrants a dedicated pass.
