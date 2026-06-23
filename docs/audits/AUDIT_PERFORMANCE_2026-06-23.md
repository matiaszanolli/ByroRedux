# Performance Audit — 2026-06-23

**Scope**: GPU/CPU performance — CPU per-frame allocations & hot paths, draw-call
& instancing efficiency, GPU memory pressure & eviction, SSBO sizing & upload, GPU
pipeline & pass efficiency, skinning & BLAS cost, world streaming, NIF parse, and
telemetry/camera-relative-origin cost. All 9 dimensions, `deep` depth.

**Method**: Inline (no nested sub-agents). Each Session 46 perf-batch (#1371–#1379),
Session 47 precision (#1489–#1498), and the R1 / NIFAL / M29.x guards named in the
skill were treated as **regression guards to verify still hold**, not as findings to
re-propose. Each finding (and each guard) was re-read against the live tree and an
attempt was made to disprove it before inclusion. Anchored on symbols, not line numbers.

**Hardware target**: RTX 4070 Ti (12 GB) + Ryzen 7950X (16c/32t). RT VRAM minimum 6 GB.

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 1 |

**One LOW finding (NEW), zero regressions.** Every Session 46/47 perf guard and every
R1/NIFAL/M29.x invariant the skill enumerates is **present and intact** in the current
tree. The performance posture is healthy: per-frame CPU allocation is amortized via
caller-owned scratch, draw state-changes are change-gated, SSBO uploads are O(live data),
GPU memory pressure uses the dynamic VRAM/3 budget with smooth LRU, the skinning
dispatch/refit/descriptor-skip gates fire, the TLAS build→read barrier is correct, and
the camera-relative origin stays inside the existing instance loop with TAA history
preserved across cell crossings.

### Bench-of-Record delta (observed vs ROADMAP — not absolute FPS)

ROADMAP's **Bench-of-record** block (R6a-stale-14, HEAD `1c26bc25`, 2026-06-03) is
self-flagged as **332 commits stale** and explicitly does not gate. Per the skill, no
FPS/ms/fence numbers are copied or asserted here. The canonical control benches remain
**Prospector** (FNV glass-heavy interior), **WhiterunBanneredMare** (Skyrim steady-state),
and **MedTekResearch01** (FO4 CSG-precombine-heavy). The two open structural items ROADMAP
itself records — Prospector's unrecovered pre-collider baseline (entity-count growth origin
under investigation) and MedTek's −28% FPS being the M49-CSG-correct new larger baseline —
are **scene-correctness consequences, not code regressions**, and are out of scope for this
code-level audit. A fresh R6a-stale-15 three-scene GPU bench (gated on game data + a Vulkan
device) is the prerequisite for any current FPS claim and is the recommended next action.

---

## Hot Path Analysis — Guard Verification Matrix

Quantitative instrumentation exists and should be used for any future "this is expensive"
claim: GPU per-pass timestamp pairs (`crates/renderer/src/vulkan/gpu_timers.rs`), the CPU
per-phase wall-clock split (`log_stats_system`, `byroredux/src/systems/debug.rs`,
`cpu_ms:` line), `ScratchTelemetry` (`crates/core/src/ecs/resources.rs`), and the skin
coverage counters (`SkinCoverageFrame`, `crates/renderer/src/vulkan/skin_compute.rs`).

### Dimension 1 — CPU Per-Frame Allocations & Hot Paths
| Guard | Status | Evidence |
|-------|--------|----------|
| `drain_dirty_into` over `take_dirty` (#1371) | INTACT | `transform_propagation_system` (`crates/core/src/ecs/systems.rs:93`) and world-bound prop (`byroredux/src/systems/bounds.rs:62`) both call `storage_mut().drain_dirty_into(&mut …)` into a persistent scratch; `take_dirty` only in tests. Test `drain_dirty_into_preserves_storage_capacity` present. |
| `make_animation_system` persistent scratch (#1372) | INTACT | `byroredux/src/systems/animation.rs` — `make_animation_system()` captures `entities_scratch` + `playback_scratch`, reused via `clear()+extend()`; production wires it at `byroredux/src/main.rs:754`. |
| `make_billboard_system` camera-move gate (#1374) | INTACT | `byroredux/src/systems/billboard.rs:56` — `if last_cam == Some((cam_pos, cam_forward)) { return }` skips the whole `get_mut` loop when the camera hasn't moved. |
| `build_debug_ui_snapshot` visibility gate (#1376) | INTACT | `byroredux/src/main.rs:1572` — deep-clone gated on `debug_ui … visible`; boot-default hidden = ~0 cost. |
| `SkinSlotPool` `next_slot` contraction (#1379) | INTACT | `crates/core/src/ecs/resources.rs` `sweep()` sorts `free_list` and tail-pops while top == `next_slot-1`; test `sweep_contracts_next_slot_when_tail_is_freed`. |

Particle integration (`integrate_force_fields`) is allocation-free per particle; the only
`collect()` (`convert_force_fields_zup_to_yup`) is import-time, not per-frame. Render scratch
(`draw_commands`/`gpu_lights`/`bone_world`/`skin_offsets`/`material_table`) is caller-owned
and `clear()`ed in `build_render_data` (`byroredux/src/render/mod.rs:305`).

### Dimension 2 — Draw-Call & Instancing Efficiency
| Guard | Status | Evidence |
|-------|--------|----------|
| `draw_sort_key` pipeline/material/texture-before-depth ordering | INTACT | `byroredux/src/render/mod.rs:192` 10-tuple; additive-blend (`dst_blend==ONE`) clusters by mesh for indirect-merge, true alpha-over keeps depth dominant. |
| Parallel-sort gate at ≥2000 commands | INTACT | `byroredux/src/render/mod.rs:417` `if draw_commands.len() >= 2000 { par_sort_unstable_by_key } else { sort_unstable_by_key }`. Threshold matches the typical Bethesda cell range (control cells 1224–1299 draws → serial; FO4 MedTek 14535 → parallel). |
| Per-draw state-change minimization | INTACT | `crates/renderer/src/vulkan/context/draw.rs` draw loop change-gates pipeline (`last_pipeline_key`, 2718), depth-bias (`last_render_layer`, 2763), and z_test/z_write/z_function (2775–2787). No unconditional `cmd_set_depth_bias` per draw. |
| GT-presence hoist (#1377) | INTACT | `byroredux/src/render/static_meshes.rs:147` `if tq.get(entity).is_none() { continue; }` before vis/wb sibling probes. |
| CPU instance→batch merge counters (#1258) | INTACT | `last_draw_call_stats.batch_count` snapshot at `draw.rs:2708`. |

### Dimension 3 — GPU Memory Pressure & Eviction
| Guard | Status | Evidence |
|-------|--------|----------|
| Dynamic BLAS budget `VRAM/3` floored at 256 MB | INTACT | `crates/renderer/src/vulkan/acceleration/predicates.rs:551-553` `(device_local_bytes / 3).max(MIN_BLAS_BUDGET_BYTES)`; `MIN_BLAS_BUDGET_BYTES = 256 MB` (`constants.rs:61`). No static "1 GB". |
| 90% pre/mid-batch eviction, interval 64 | INTACT | `predicates.rs:373-388` (90% projected); `BATCH_EVICTION_CHECK_INTERVAL = 64` (`constants.rs:74`). |
| Scratch shrink floors | INTACT | `MIN_TLAS_INSTANCE_RESERVE = 8192`, `WORKING_SET_FLOOR = MIN_TLAS_INSTANCE_RESERVE` (`constants.rs:47,54`); `BLAS_REBUILD_SLACK_BYTES`/`TLAS_*_SLACK_BYTES` headroom present. |
| BGSM/BGEM + failed-path **half-eviction** (#1430) | INTACT | `byroredux/src/asset_provider.rs:1076,1109` drop oldest N/2 via insertion-order `VecDeque` (`MAX_BGEM_CACHE_ENTRIES`/`MAX_FAILED_PATHS = 1024`). No full-flush thundering-herd path. |
| Deferred-destroy countdown = `MAX_FRAMES_IN_FLIGHT` | INTACT | `crates/renderer/src/deferred_destroy.rs:27` countdown expressed as `MAX_FRAMES_IN_FLIGHT as u32` (currently 2). |

### Dimension 4 — SSBO Sizing & Per-Frame Upload
| Guard | Status | Evidence |
|-------|--------|----------|
| Upload size O(live data) not O(capacity) | INTACT | `crates/renderer/src/vulkan/scene_buffer/upload.rs` — every `upload_*` computes `count = data.len().min(MAX_*)` then `byte_size = size_of::<T>() * count` (`upload_instances:480-507`, lights/camera/bones/materials/indirect/terrain all the same). No full-capacity memcpy. |
| `MAX_INSTANCES = 0x40000`, `MAX_INDIRECT_DRAWS = MAX_INSTANCES`, `MAX_MATERIALS = 16384` | INTACT | `scene_buffer/constants.rs:134,157,184`; 24-bit `instance_custom_index` static-assert present (`constants.rs:141`). |
| `GpuInstance` 112 B carrying per-DRAW data only (R1) | INTACT | `gpu_types.rs:108` (offset 108→total 112); tests `gpu_instance_is_112_bytes_std430_compatible` / `…field_offsets_match_shader_contract` / `…does_not_re_expand_with_per_material_fields`. |
| PBR resolved ONCE at import (no per-draw classify) | INTACT | `Material::metalness`/`roughness` plain `f32` (`crates/core/src/ecs/components/material.rs:217,223`); `resolve_pbr()` (which calls `classify_pbr_keyword`) is invoked only at the NIFAL boundary `byroredux/src/material_translate.rs:160`. Draw loop reads `GpuMaterial` indices — no `classify_pbr_keyword` call in `draw.rs`. |
| `MaterialTable::intern` O(1) amortized, dedup-skip closure | INTACT | `crates/renderer/src/vulkan/material.rs:1051` `intern_by_hash` (`FxHashMap<u64,u32>`); `to_gpu_material` skipped on the ~97% dedup-hit path (#781). |

### Dimension 5 — GPU Pipeline & Pass Efficiency
| Guard | Status | Evidence |
|-------|--------|----------|
| Volumetrics dispatch O(froxels) not O(meshes) | INTACT | `crates/renderer/src/vulkan/volumetrics.rs:922-978` inject/integrate dispatch dims = `FROXEL_{WIDTH,HEIGHT,DEPTH}.div_ceil(WORKGROUP_*)`. |
| Bloom pyramid O(pixels) | INTACT | `crates/renderer/src/vulkan/bloom.rs` — fixed `BLOOM_MIP_COUNT` down + up pyramid, 4-tap bilinear; no mesh-count scaling. |
| `inv_vp` computed CPU-side, passed via UBO (no per-invocation `inverse()`) | INTACT | `ssao.comp:24`, `cluster_cull.comp:60`, `include/bindings.glsl:176` all declare `invViewProj` as a precomputed UBO field; the only `inverse(` occurrences in shaders are in comments describing that precompute. |
| TLAS build → shader read barrier | INTACT | `crates/renderer/src/vulkan/context/draw.rs:1762-1770` AS_BUILD_WRITE → FRAGMENT\|COMPUTE SHADER + ACCELERATION_STRUCTURE_READ_KHR after `build_tlas`. COMPUTE→AS_BUILD input barriers also present (1490-1500, 1643-1650). |

### Dimension 6 — Skinning & BLAS Cost (M29.x)
| Guard | Status | Evidence |
|-------|--------|----------|
| Dispatch-dirty gate (#1195) | INTACT | `crates/renderer/src/vulkan/context/draw.rs:1453-1457` `is_dirty = pose_dirty.contains(&entity_id); if slot.has_populated_output && !is_dirty { dispatches_skipped += 1; continue; }`. First-sight (`has_populated_output==false`) falls through; `pose_dirty`/`try_mark_pose_dirty`/`clear_pose_dirty` live on `SkinSlotPool` (`crates/core/src/ecs/resources.rs`); call sites in `byroredux/src/render/skinned.rs:152,180`. |
| BLAS refit gate (#1196) | INTACT | `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` `refit_skinned_blas`; `SKINNED_BLAS_REFIT_THRESHOLD` present. |
| `SKINNED_BLAS_FLAGS = PREFER_FAST_BUILD` (not FAST_TRACE) | INTACT | `crates/renderer/src/vulkan/acceleration/constants.rs:112-114` `PREFER_FAST_BUILD`; TLAS stays `PREFER_FAST_TRACE` (constants.rs:96/134). Flip-back caveat documented inline (R6a-prospector-regress). |
| Descriptor-rewrite skip (#1197) + instrumentation (#1194) | INTACT | `dispatches_skipped` on `SkinCoverageFrame` (`skin_compute.rs:136`); per-pass GPU timers wired. |

### Dimension 7 — World Streaming & Cell Transitions (M40)
| Guard | Status | Evidence |
|-------|--------|----------|
| Two-phase pre-parse (#877): serial extract → rayon body parse | INTACT | `byroredux/src/streaming.rs:603-656` — Phase 1 serial BSA extract (one mutex acquire per NIF), Phase 2 `extracted.into_par_iter().map(parse_one_nif).collect()`. |
| Small-model serial fast-path (#1262) | INTACT | `byroredux/src/streaming.rs:640-656` drops to serial iteration below the size threshold, keeping rayon for larger batches. |
| Panic-safe worker + shutdown drain | INTACT | `pre_parse_cell_panic_safe` (`streaming.rs:445`); closure shared between serial/parallel branches (`streaming.rs:482`). |

### Dimension 8 — NIF Parse Performance
| Guard | Status | Evidence |
|-------|--------|----------|
| Bulk arrays through `read_pod_vec<T>` (#833) | INTACT | `crates/nif/src/stream.rs:350`; big-endian `compile_error!` gate at `stream.rs:22`. |
| `allocate_vec` / `read_pod_vec` `#[must_use]` (#831) | INTACT | `stream.rs:252,349`. |
| Per-block counters via `entry().get_mut()/insert` (no `or_insert(to_string)`) (#832) | INTACT | confirmed no throwaway-string `or_insert` on the parse path. |
| Typed emitter params extracted at import only | INTACT | `extract_emitter_params`/`extract_emitter_rate` are import-side (NIFAL particles slice); the per-frame side (`apply_emitter_params`) only reads pre-converted data. |
| dhat alloc bounds (#1381) | PRESENT | `crates/nif/tests/heap_allocation_bounds.rs` + `…_geometry.rs` cover node/geometry/particle parse paths. |

### Dimension 9 — Telemetry & Camera-Relative Origin Cost
| Guard | Status | Evidence |
|-------|--------|----------|
| Render origin snapped to 4096-unit grid; per-instance rebase inside existing loop | INTACT | `crates/renderer/src/vulkan/context/draw.rs:806` `snap_render_origin`; the `m[12..14] - render_origin` subtraction is inline in the single O(visible-instances) build loop (draw.rs:2024-2026), not a second pass. |
| `origin_corrected_prev_view_proj` preserves TAA/SVGF history on cell crossing (#1489) | INTACT | `draw.rs:3747` — returns `prev_vp` verbatim (bitwise) when ΔO==0 (hot path), right-multiplies by `translation(O₂−O₁)` only on a crossing. Tests `unchanged_origin_returns_matrix_verbatim` + the Markarth-scale crossing test present. No per-crossing history reset. |
| GPU timers don't stall (prior-frame readback) | INTACT | `gpu_timers.rs` `read_and_reset` pattern; CPU `cpu_ms:` split + `ScratchTelemetry` available for attribution. |

---

## Findings

### PERF-2026-06-23-01: Player-path animation text-event Vec re-allocates each frame
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/animation.rs` (`animation_system_inner`, the
  AnimationPlayer text-key block, ~`let mut events: Vec<AnimationTextKeyEvent> = Vec::new();`
  preceding `for ps in playback_scratch.iter()`)
- **Status**: NEW
- **Description**: The `make_animation_system` factory (#1372) captures `entities_scratch`
  and `playback_scratch` and reuses them across frames, and the **AnimationStack** path's
  text-event scratches (`events`, `seen_labels`, `channel_names_scratch`, `updates_scratch`)
  were deliberately hoisted to closure scope under #828 so they keep their high-water
  capacity. The **AnimationPlayer** path's text-event `events` Vec, however, is allocated
  fresh with `Vec::new()` *inside* `animation_system_inner` on every call — it is `clear()`ed
  and reused across entities *within* a frame, but the backing allocation is dropped at the
  end of the function and re-grown 0→N the next frame. This is the same 0→N regrowth pattern
  #828/#1372 eliminated everywhere else in this system.
- **Evidence**: The stack path uses outer-scope `let mut events: Vec<AnimationTextKeyEvent>
  = Vec::new();` reused via `events.clear()` per entity (documented `#828` rationale).
  The player path declares its own `events` Vec scoped to the text-key emit block, not
  captured by the `make_animation_system` closure, so it is reborn each frame.
- **Impact**: One heap allocation + grow per frame, bounded by the count of distinct text
  events firing across all AnimationPlayer entities in a frame (typically small — most
  KF clips fire 0–few text keys per tick). Negligible in absolute terms; flagged for
  consistency with the established scratch-persistence pattern, not because it is a measured
  hot spot. **No quantitative guard exists for this site** — per-frame render/ECS hot paths
  have no dhat coverage (the profiler is a process singleton; the live loop is smoke-test
  territory), so this cannot be bounded by a test today.
- **Related**: #828 (stack-path scratch hoist), #1372 (`make_animation_system` scratch).
- **Suggested Fix**: Thread a third reusable buffer (`text_events_scratch:
  Vec<AnimationTextKeyEvent>`) through `animation_system_inner` and capture it in
  `make_animation_system` alongside `entities_scratch`/`playback_scratch`; the `#[cfg(test)]`
  `animation_system` wrapper passes a fresh `Vec::new()` as it already does for the other two.

---

## Prioritized Fix Order

1. **PERF-2026-06-23-01 (LOW, quick win)** — hoist the player-path text-event Vec into the
   `make_animation_system` closure scratch. ~5-line change mirroring the existing #828/#1372
   pattern; zero risk, restores full scratch-persistence consistency across both animation
   apply paths.
2. **R6a-stale-15 refresh (process, not code)** — run the three-scene 300-frame GPU bench
   (Prospector / WhiterunBanneredMare / MedTekResearch01) on the 4070 Ti and refresh
   ROADMAP's Bench-of-record. This is the gating prerequisite for any current FPS claim and
   the only way to confirm the 332-commit churn (Session 47 Cornell/GI/caustics + Session 49
   RT denoiser overhaul #1662) hasn't moved the per-pass GPU cost — none of which is visible
   to `cargo test`.

No architectural changes recommended. The perf-critical surfaces audited are in good shape.
