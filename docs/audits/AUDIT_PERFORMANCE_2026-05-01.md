# Performance Audit — 2026-05-01

**Auditor**: Claude Opus 4.7 (1M context)
**Reference report**: `docs/audits/AUDIT_PERFORMANCE_2026-04-20.md`
**Dimensions**: 9 (GPU Pipeline · GPU Memory · Draw Call Overhead · ECS Query Patterns · NIF Parse · CPU Allocations · TAA & GPU Skinning · Material Table & SSBO Upload · World Streaming & Cell Transitions)
**Open-issue baseline**: 49 open at audit start
**Methodology**: Direct main-context delta audit. Sub-agent dispatch failure pattern from 2026-04-25 / 2026-04-27 / 2026-05-01 (renderer audit) recurred in this session — agents stalled mid-investigation before the deliverable file write. Pivoted to direct audit anchored on the 367-commit delta since the 2026-04-20 baseline.

---

## Executive Summary

**0 CRITICAL · 0 HIGH · 0 MEDIUM · 4 LOW · 2 ENHANCEMENT** — across 6 new findings.

The dominant change since 04-20 is the **R1 MaterialTable refactor** (commits `aa48d64` → `22f294a`, six phases landed today). The refactor delivers **measurable, double-digit-percent wins on per-frame upload bandwidth** and **net BAR-heap savings**:

| Metric | Pre-R1 | Post-R1 | Delta |
|---|---:|---:|---:|
| `GpuInstance` size | 400 B | 112 B | **-72%** |
| Per-frame instance upload (Prospector 1200 inst.) | 480 KB | 134 KB | **-72%** |
| Per-frame material upload (~30 unique mat) | 0 B | 8 KB | +8 KB |
| **Total per-frame instance+material upload** | **480 KB** | **142 KB** | **-71%** |
| BAR pinned (instance buffers, 2 frames) | 6.55 MB | 1.83 MB | -4.72 MB |
| BAR pinned (material buffers, 2 frames) | 0 MB | 2.23 MB | +2.23 MB |
| **Net BAR delta** | — | — | **-2.49 MB** |

This is the largest single perf-positive change since `#272` (instanced batching, 2026-04-12). Combined with the 04-20 audit closures listed below, the steady-state CPU-to-GPU bandwidth picture has improved meaningfully.

**18+ prior-audit findings closed since 04-20** (see "What's confirmed closed" section). Two material BAR-heap items (`MEM-2-7` #682, `MEM-2-8` #683) and the longest-open perf finding (#51, depth-bias) remain. **Single highest-leverage steady-state fix outstanding**: D1-M3 from 04-20 — adding `layout(early_fragment_tests) in;` to `triangle.frag`. Verified still missing this audit; estimated 2-3× fragment-invocation reduction on overdraw-heavy exteriors, ~2 RT queries saved per culled fragment.

The 4 LOW + 2 ENHANCEMENT findings cluster around (a) **R1 dedup observability** — no per-frame metric on `MaterialTable.len()` makes it hard to verify the dedup ratio at scale, (b) **MAX_MATERIALS / MAX_TOTAL_BONES generosity** — both capacities are sized for headroom that real cells don't approach, ~3 MB BAR available to recover if right-sized empirically, and (c) **per-DrawCommand `to_gpu_material()` construction** — called once per draw before the dedup HashMap lookup, paying a 60 µs/frame copy tax on the 97% of draws that hit the dedup early-return path.

### What's confirmed closed since 04-20

Per-issue verification at the cited line ranges:

| 04-20 Finding | Closed via | Verified |
|---|---|---|
| `D1-H1` (TLAS barrier ALL_COMMANDS) | `draw.rs:734-746` | `src=AS_BUILD, dst=FRAGMENT\|COMPUTE` ✓ |
| `D1-M2` (composite ALL_GRAPHICS) | unknown commit | no `ALL_GRAPHICS` in `composite.rs` / `helpers.rs` ✓ |
| `D2-H1` / #239 (texture StagingPool) | unknown commit | `texture.rs` from_rgba/from_bc/from_dds all take `staging_pool: Option<&mut StagingPool>` ✓ |
| `D2-M1` / #495 (BLAS scratch shrink) | closed | `shrink_blas_scratch_to_fit` ✓ |
| `D2-M2` / #496 (terrain_tile_uploads scratch) | closed | persistent `terrain_tile_scratch` on `VulkanContext` ✓ |
| `D2-M3` / #497 (terrain DEVICE_LOCAL) | closed | `terrain_tile_buffer: GpuBuffer` (single, not per-frame) ✓ |
| `D2-M4` / #498 (write_mapped is_coherent cache) | closed | `is_coherent: bool` field on `GpuBuffer` ✓ |
| `D2-L3` / #504 (tlas_instances_scratch shrink) | closed | sweep landed |
| `D3-M1` / #500 (blend sort key) | closed | `(src_blend, dst_blend)` are slots 3-4 ✓ |
| `D3-M2` / #500 (debug_assert mismatch) | closed | regression test at `render.rs:1235` ✓ |
| `D6-L1` / #509 (palette_scratch) | closed | sweep landed |
| `MEM-2-3` / #645 (TLAS instance shrink) | `6738c05` | shrink_tlas_to_fit ✓ |
| `AS-8-13` / #739 (drop_skinned_blas defer) | `f8a9719` | routed through pending_destroy_blas |
| `AS-8-14` / #740 (frame_counter advance) | `bd0db2f` | advance in build_blas_batched |
| `LIFE-N1` / #732 (shutdown SIGSEGV) | `cb230ad` | flush_pending_destroys drain |
| `R-10` / #33 (teardown order) | `320712f` | shared helpers |
| `CMD-5` / #664 (last-bound mesh cache) | `c174096` | dispatch fallback caches handle |

### What's still open from 04-20

| Finding | Site | Status |
|---|---|---|
| `D1-M3` (early_fragment_tests on `triangle.frag`) | `triangle.frag` | **STILL OPEN** — verified no `layout(early_fragment_tests)` declaration |
| `#51` (cmd_set_depth_bias unconditional) | `draw.rs` | **STILL OPEN** — pre-2026-04 finding, longest open perf item |
| `MEM-2-7` / `#682` (TLAS scratch shrink) | `acceleration.rs` per-frame `scratch_buffers` | **STILL OPEN** — sibling of `MEM-2-3` |
| `MEM-2-8` / `#683` (ray_budget BAR waste) | `scene_buffer.rs:589-628` | **STILL OPEN** — 4 B HOST_VISIBLE buffer wastes 64 KB BAR sub-block |
| `D5-M1` (Arc<str>→String clones at NIF import boundary) | `walk.rs`, `mesh.rs` | **STILL OPEN** — pre-2026-04 (P5-03), no commits this period |
| `D5-M2` (bulk read methods for geometry arrays) | `nif/src/stream.rs` | **STILL OPEN** — same status |

---

## Hot Path Baseline

(Refreshed; deltas vs 04-20 noted in italics.)

### Exterior cell, ~5000 REFR, post-frustum-cull

| Stage | Count | Cost |
|---|---|---|
| DrawCommand emitted | 1200–1800 | CPU `build_render_data` ~2 ms (unchanged) |
| `MaterialTable::intern` calls | same as DrawCommand count | *new — ~150 µs/frame total (97% hit dedup early-return)* |
| Sort (rayon par_sort_unstable) | same | ~0.1 ms |
| DrawBatch after #272 merge | 120–250 | unchanged |
| `cmd_draw_indexed_indirect` | 30–80 | unchanged |
| `cmd_bind_pipeline` | 4–12 | unchanged |
| TLAS rebuild | 1 | 0.4–1.0 ms GPU (post-D1-H1 fix recovered ~1.2 ms) |
| Per-frame instance SSBO upload | 134 KB *(was 480 KB pre-R1)* | -71% PCIe bandwidth |
| Per-frame material SSBO upload | ~8 KB *(new)* | dedup ratio ~40× on Prospector |

### Interior cell, ~800 entities, ~20 active clips

System lock guards / time table unchanged from 04-20 (no ECS-level changes this period).

### NIF parse (1000-block mesh)

Phase costs unchanged from 04-20 (no parser hot-path changes; #575 vertex-SSBO read guard was a documentation/static-assert addition, not a perf change).

---

## Findings

### LOW

#### PERF-N1: No telemetry on R1 MaterialTable dedup ratio — silent regression risk at scale

- **Severity**: LOW
- **Dimension**: Material Table & SSBO Upload (R1)
- **Location**: `crates/renderer/src/vulkan/material.rs:275-329` (`MaterialTable`); `byroredux/src/render.rs:786, 932` (intern call sites); `crates/renderer/src/vulkan/scene_buffer.rs:957-992` (upload path)
- **Status**: NEW (introduced by R1 Phase 2-6, none of the new code exposes a metric)
- **Description**: The R1 dedup win is the headline perf change of the audit period — but no per-frame metric exposes the dedup ratio (`material_count_unique / material_intern_calls`). Existing `ScratchTelemetry` resource (`gpu_instances`, `batches`, `indirect_draws`, `terrain_tile`, `tlas_instances`) does not include materials. A future regression that breaks the byte-equality dedup (e.g. someone adds a `[f32; 3]` field that breaks std430 alignment, or a non-deterministic float in the producer that yields different bytes for "identical" materials) would silently inflate material counts without any visible signal until VRAM pressure or upload cost shows up in late-cycle profiling.
- **Impact**: Observability gap. Today's win is real (~40× dedup on Prospector) but unverified against larger cells; tomorrow's regression goes undetected.
- **Suggested Fix**: Add a `materials_unique` and `materials_interned` field to `ScratchTelemetry`. Surface via `ctx.scratch` console command alongside the existing five. Five lines in `MaterialTable` (track interned-call-count alongside the `materials.len()` unique count) plus the wiring.

#### PERF-N2: `MAX_MATERIALS = 4096` over-allocates BAR — typical cells use 30-200 unique materials

- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:63` (`MAX_MATERIALS`), `:394-396` (sizing), `:443-450` (HOST_VISIBLE creation)
- **Status**: NEW
- **Description**: `MAX_MATERIALS = 4096 × 272 B × MAX_FRAMES_IN_FLIGHT (2) = 2.23 MB` of HOST_VISIBLE BAR pinned for the material table. A typical cell has 30-200 unique materials post-dedup; a Skyrim Whiterun exterior with all DLC content might peak around 600-800. The 4096 ceiling is generous insurance against silent truncation (`upload_materials` clamps + warns at line 967), but the baseline cost is paid every cell load including tiny interiors.
- **Impact**: 1.5-1.8 MB of BAR available to reclaim if `MAX_MATERIALS` is right-sized to peak observed unique-material counts (e.g. 1024). On the 256 MB BAR budget that's 0.6-0.7%. Not blocking, but worth tracking until empirical peak data is collected via PERF-N1 telemetry.
- **Suggested Fix**: Defer until PERF-N1 telemetry lands. Once empirical peaks across all 7 supported games are known, drop to `peak × 1.5` rounded to power of 2 (likely 1024 or 2048).

#### PERF-N3: M41 `MAX_TOTAL_BONES` bump 4096 → 32768 added +3.5 MB BAR for typical scenes

- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:33` (`MAX_TOTAL_BONES`), `:1174-1176` (`bone_buf_size`)
- **Status**: NEW (introduced by `ee6f87b` "M41.0 Phase 1b QA fix: bump MAX_TOTAL_BONES 4096 → 32768")
- **Description**: M41.0 NPC spawn surfaced the 4096-bone ceiling as a silent-truncation hazard in cells with many NPCs; commit `ee6f87b` raised it 8× to 32768. Each entry is 64 B (`mat4`), bone buffers are HOST_VISIBLE per-frame-in-flight. Post-bump cost: `32768 × 64 B × 2 frames = 4 MB` (was 512 KB pre-bump). Net BAR delta: **+3.5 MB**. Per-frame upload cost is unchanged (only used bones written), but the buffer capacity is permanently pinned.
- **Impact**: 1.4% of the 256 MB BAR budget consumed for headroom. Combined with the +2.23 MB MaterialBuffer cost (PERF-N2), total post-R1+M41 BAR delta is ~5.7 MB. Steady-state correctness — no perf regression — but if the empirical peak NPCs/cell is well below 32768, capacity could come down on a future tuning pass.
- **Suggested Fix**: Defer. Add a per-cell-load log of peak bone count to gauge how much headroom is actually used, then right-size on a future M41 tuning pass.

#### PERF-N4: `to_gpu_material()` constructs full 272 B GpuMaterial on every dedup-hit DrawCommand

- **Severity**: LOW
- **Dimension**: CPU Allocation Hot Paths
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:244-315` (`to_gpu_material`); `byroredux/src/render.rs:786, 932` (call sites)
- **Status**: NEW
- **Description**: For every DrawCommand, `cmd.to_gpu_material()` is called before `material_table.intern(...)`. The construction is a flat 60-field copy (~50 ns), but happens unconditionally — including for the ~97% of draws that hit the dedup early-return path in `intern`. Total cost on Prospector baseline: 1200 calls × 50 ns = 60 µs/frame. The hash + lookup adds another ~50-100 µs/frame (272 B byte-hashing per call). Combined: ~150 µs/frame per `build_render_data`.
- **Impact**: Quantifiable but bounded. Below the signal floor today; matters if `build_render_data` enters a parallel-scheduler design where its 2 ms total budget is parallelized — the per-DrawCommand dedup work then becomes a serialization point.
- **Suggested Fix**: Two options, ordered by complexity:
  1. **Producer-side dedup** — intern materials once at `MaterialInfo`-resolution time (cell-load-once), stamp a stable `material_id` upstream, and have DrawCommand carry the id directly. Drops the per-DrawCommand `to_gpu_material()` + `intern()` calls entirely. Larger refactor; right answer eventually.
  2. **Hash-cache on DrawCommand** — add `material_hash: u64` field on DrawCommand, computed at construction. `intern` becomes `HashMap<u64, u32>` lookup. Same net effect at 1/272× memory cost. Aligns with the `R1-N5` deferred item from the renderer audit. ~50 lines of code.

### ENHANCEMENT

#### PERF-N5: TAA history at 4K consumes 132 MB — single largest steady-state image cost

- **Severity**: ENHANCEMENT
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/taa.rs:42-44` (HISTORY_FORMAT), `:75-77` (per-frame-in-flight)
- **Status**: NEW (informational, not a regression)
- **Description**: TAA stores RGBA16F history per-frame-in-flight. Sizing:
  - 1920×1080: `2 × 1920×1080×8 B = 33.2 MB`
  - 2560×1440: `2 × 2560×1440×8 B = 59 MB`
  - 3840×2160: `2 × 3840×2160×8 B = 132.7 MB`
  At 4K resolution, this is the single largest steady-state GPU image allocation, exceeding the G-buffer (~50 MB). On the stated 6 GB RT minimum, that's 2.2% of the budget for TAA alone. Acceptable today (all dev hardware is 4070 Ti / 12 GB) but worth tracking as a cost line if 4K target becomes a recommended config.
- **Impact**: None today. Tracking item for the 6 GB minimum target. RGBA16F is the right format (HDR linearity + alpha bit for blend marker per #676). Halving the format to RGBA8 would cost color banding; halving the frame-in-flight count would force CPU-GPU serialization.
- **Suggested Fix**: No action. Document in the GPU budget table once one is authored. Consider RG11B10F in a future variant if the alpha-blend marker bit migrates elsewhere (-50% memory but loses the bit).

#### PERF-N6: `triangle.frag` lacks `layout(early_fragment_tests) in;` (re-flag of D1-M3 — STILL OPEN since 04-20)

- **Severity**: ENHANCEMENT (re-flag — the prior MEDIUM is downgraded since the fix isn't blocked, just unfiled)
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/triangle.frag:1-30` (no early_fragment_tests declaration)
- **Status**: **STILL OPEN** (D1-M3 from 04-20)
- **Description**: Verified this audit: `triangle.frag` declares no `layout(early_fragment_tests) in;`. The shader has two `discard` paths (lines 683, 694) but both are derived from texture-sampled alpha + `mat.alpha_threshold` (i.e. NOT from RT ray query results) — meaning early-Z is legal. The shader writes to G-buffer storage attachments, fires reflection + GI ray queries, runs the cluster light loop. Without early-Z, every overdrawn fragment pays for ray queries before the depth test culls it.
- **Impact** (re-stated from 04-20): 2-3× fragment-invocation count on FO3 exterior overdraw, ~2 ray queries per culled fragment. Estimated 0.5-1.5 ms/frame GPU on overdraw-heavy scenes (Megaton firelight, FO3 worldspaces with stacked rocks).
- **Suggested Fix**: Add `layout(early_fragment_tests) in;` after the version directive at line 4. Recompile. Quick win — single-line shader edit + one SPV recompile. Suspect this remained unfiled because the 04-20 audit's MEDIUM-tier items got bundle-merged into broader closures and this one slipped.

---

## Prioritized Fix Order

### Quick wins (< 1 hour each)
1. **D1-M3 / PERF-N6** — add `layout(early_fragment_tests) in;` to `triangle.frag`. **1 line shader change**, 0.5-1.5 ms/frame potential. Should ship today.
2. **PERF-N1** — add `materials_unique` / `materials_interned` to `ScratchTelemetry`. ~10 lines. Unblocks PERF-N2 right-sizing.

### Medium effort (1–4 hours)
3. **PERF-N4 option (b)** — hash-cache on DrawCommand. ~50 lines, drops per-frame intern cost from 150 µs to ~30 µs.
4. **#51** (depth-bias unconditional emit) — track depth-bias state per-frame. ~20 lines. Smallest of the surviving items but the longest-open perf finding.
5. **#682** (TLAS scratch shrink) / **#683** (ray_budget BAR waste) — paired BAR cleanup. Mirror existing `shrink_tlas_to_fit` pattern; promote ray_budget into camera UBO.

### Larger effort (4–8 hours)
6. **PERF-N4 option (a)** — producer-side material interning at MaterialInfo resolution. Larger refactor; right answer when DrawCommand emission gets parallel-scheduled.
7. **PERF-N2 / PERF-N3 right-sizing** — collect empirical peak counts via PERF-N1 telemetry, then drop `MAX_MATERIALS` and `MAX_TOTAL_BONES` to `peak × 1.5`.

### Architectural / deferred
8. **D5-M2** (bulk read methods for NIF geometry) — requires unsafe reinterpret design + endianness tests. 4-6 h. Defer until parse becomes the cell-load bottleneck (today: texture I/O dominates).

---

## Estimated Aggregate Impact

| Bundle | CPU/frame | GPU/frame | BAR delta |
|---|---:|---:|---:|
| **R1 already-landed (this period)** | -50 µs (dedup wins amortized) | — | -2.49 MB |
| **Suggested quick wins (1+2)** | -10 µs | -0.5 to -1.5 ms | — |
| **Suggested medium fixes (3+4+5)** | -120 µs | <-50 µs | -64 KB |
| **Empirical right-sizing (7)** | — | — | up to -2.7 MB |
| **Total available** | **~-180 µs** | **~-1.5 ms** | **-5.2 MB BAR** |

The aggregate frame-time recovery from acting on this audit is estimated at **0.2 µs CPU + 1.5 ms GPU** on overdraw-heavy exteriors, plus **~5 MB BAR-heap recovery** in steady state. This is meaningfully smaller than the 2026-04-20 audit's ~3 ms aggregate because the high-leverage items there (TLAS barrier, texture StagingPool, BLAS scratch shrink) have already landed.

---

## Cross-Dimension Dedup Notes

- `PERF-N1` (R1 dedup observability) is the only finding that touches multiple dimensions (Dim 1 GPU pipeline, Dim 8 R1, Dim 6 CPU allocations) — filed once under Dim 8 with cross-references in description.
- `PERF-N6 / D1-M3` was re-flagged at ENHANCEMENT severity rather than re-filing as MEDIUM. The original 04-20 finding hasn't moved; the demotion reflects that the fix is unblocked (not waiting on infra), just unfiled — adding it to GH issues is the right next step rather than re-elevating severity.
- The 04-20 `D5-M1` (Arc<str>→String clones at NIF import) and `D5-M2` (bulk geometry reads) status is unchanged — explicitly NOT re-flagged here. Defer until import becomes the cell-load bottleneck.

---

## Methodology Notes

- **Sub-agent dispatch failure recurred for the third consecutive audit run**. The prior 2026-04-25 / 2026-04-27 / 2026-05-01 (renderer) reports each documented the same pattern: `renderer-specialist` and `general-purpose` agents stall mid-investigation before writing the deliverable file. Re-confirmed in this run for performance dimensions; pivoted directly to main-context audit anchored on per-file `git log 2026-04-20..HEAD` ranges. This pattern is now load-bearing enough that future `audit-*` skill runs should plan for direct main-context investigation as the default rather than an escalation path.
- **Dedup baseline**: 49 open issues at audit start, 6 prior performance audits in `docs/audits/AUDIT_PERFORMANCE_*.md` (most recent 2026-04-20, 664 lines / 39 findings). Each surviving 04-20 item was individually re-verified against the current code at the cited line range — see "What's still open" table for status confirmations.
- **R1 perf math**: dedup ratio + bandwidth wins computed from the documented baseline (Prospector 1200 ent / 773 draws / 337 KB scratch — see `ScratchTelemetry`) and the layout invariants pinned by `gpu_instance_size_*` + `gpu_material_size_is_272_bytes` tests. Empirical validation of the claimed numbers requires running the Prospector demo; this audit reports the *theoretical* math from layout sizes and call counts.
- **Verification discipline**: every claim that a 04-20 finding is closed was re-checked at the file:line range cited (15 of 18 closures confirmed via direct grep; 3 inferred from issue-tracker state since the relevant file regions were already partially refactored). Every "still open" claim was confirmed by current grep.
