---
date: 2026-05-10
audit: performance
focus: dimensions 1 (GPU Pipeline), 2 (GPU Memory), 3 (Draw Calls), 6 (CPU Allocations), 8 (Material/SSBO)
depth: deep
defers: dimensions 4 (ECS Queries), 5 (NIF Parse), 7 (TAA & Skinning), 9 (World Streaming) — recently audited
---

# Performance Audit — 2026-05-10

Companion follow-up to the `2026-05-06` / `2026-05-06b` performance audits. Scope shifts to the dimensions most affected by the M55 (volumetric lighting) + M58 (bloom pyramid) + M-LIGHT v1 work that landed in commits `33f48b5` (2026-05-09) and `f62d4bd` (2026-05-09). Dimensions 4, 5, 7, 9 are deferred to their recent canonical audits — no material change in those areas since.

## Executive Summary

| Severity | Count | Files Touched |
|---|---|---|
| CRITICAL | 0 | |
| HIGH | 2 | volumetrics.rs + draw.rs (waste); render.rs (skin diagnostic) |
| MEDIUM | 3 | pipeline.rs, bloom.rs, systems.rs |
| LOW | 4 | render.rs, volumetrics.rs (subordinate), composite.rs, render.rs |
| INFO | 5 | composite.rs, acceleration.rs, render.rs, volumetrics.rs |
| **Total** | **14** | |

**Headline (Dim 1 + Dim 2)**: PERF-GP-01 — the M55 volumetrics pipeline dispatches its full inject + integrate compute (~1.84M ray-query traces / frame for sun shadow visibility, ~28 MB of write+read bandwidth) **even though composite multiplies the result by 0.0**. The shader gate (`combined += vol.rgb * 0.0`) was added when the per-froxel banding was discovered on Prospector but the host-side dispatch was not gated in lockstep. Estimated waste: **10–20 ms/frame** at Prospector scale. Single largest avoidable cost in the renderer today.

**Headline (Dim 6)**: PERF-CPU-01 — diagnostic Vec allocation in the M41 skinning hot path (`render.rs:335`, dropout-detection scaffold) allocates 8–16 B per skinned mesh per frame post-cold-start. Intentional, but should be `cfg(debug_assertions)`-gated.

**Headline (Dim 8)**: Verified clean against the 2026-05-06 baseline. Two open items from that audit (#878 SSBO dirty-gate, #781 dedup-hit skip) closed in the window. M55/M58 added zero per-instance material fields; `GpuInstance` still 112 B.

**Estimated FPS impact**: PERF-GP-01 is the single material lever — gating the wasted dispatch should recover ~10-20 ms / frame on cells where volumetric is currently spending compute (i.e., everywhere with a TLAS). On Prospector at 23 FPS / 44 ms today, this could move the needle visibly. Other findings are sub-ms or quality-only.

**Profiling-infrastructure gap (recurring)**: Same dhat / alloc-counter regression-test gap flagged in the 2026-05-04 + 2026-05-06 audits. Both PERF-CPU-01 and PERF-CPU-02 are estimated allocation counts with no enforced regression. GPU-side cost estimates (PERF-GP-01) are CPU back-of-envelope; should be measured under RenderDoc/Nsight before claiming the magnitude.

## Hot Path Analysis (per frame, Prospector at 1280×720)

| Stage | Pipelines | Dispatches/Draws | Barriers | Wall-clock estimate | Status |
|---|---|---|---|---|---|
| TLAS update | 1 (REFIT) | 1 | 1 | <0.1 ms | ✅ refit dominates (PERF-GP-06) |
| G-buffer + RT main pass | 2 opaque + N blend | many indexed (M31 batched) | per-pass | ~3 ms | ✅ healthy; PERF-GP-02 says one of the 2 opaque pipelines is redundant |
| SVGF temporal | 1 | 1 | 3 | ~0.3 ms | ✅ |
| Bloom pyramid | 2 (down + up) | 9 | **19** | ~0.5–1 ms | ⚠ PERF-GP-03 (barrier overhead) |
| Volumetrics inject + integrate | 2 | 28 800 + 216 wg | 6 + 1 | **~10–20 ms (WASTED)** | 🔴 PERF-GP-01 |
| SSAO | 1 | 1 | 2 | ~0.2 ms | ✅ |
| TAA | 1 | 1 | 3 | ~0.3 ms | ✅ |
| Composite | 1 | 1 fullscreen | 0 | ~0.4 ms | ✅ PERF-GP-05 |
| Caustic | 1 (gated) | 1 | 2 | ~0.2 ms | ✅ |

## Findings

### HIGH

#### PERF-GP-01 — Volumetrics dispatches full pipeline; composite multiplies result by 0.0
**Dimension**: GPU Pipeline + GPU Memory (cross-ref PERF-GM-02)
**Files**:
- `crates/renderer/src/vulkan/context/draw.rs:1694-1738`
- `crates/renderer/shaders/composite.frag:362`
- `crates/renderer/src/vulkan/volumetrics.rs:117-119,799-855`

**Evidence**: Composite shader gates the read OFF (`combined += vol.rgb * 0.0;`) but draw.rs still dispatches both passes unconditionally. Inject does ~1.84M (160×90×128) thread invocations, each tracing a sun-shadow ray query against the TLAS; integrate Z-marches 128 slices per (x,y) column. The texture sample in composite is kept alive purely so SPIR-V reflection still sees binding 6.

**Impact**: ~10–20 ms/frame estimated GPU cost (high uncertainty — sun-shadow ray-query cost on RTX 4070 Ti is ~5-15 ns × 1.84M ≈ 10-28 ms; plus 28 MB/frame of write+read bandwidth on the integrated volume). Lower bound is "non-zero on a render fighting for budget."

**Fix**: Wrap `vol.dispatch()` in `draw.rs:1694-1738` with a `VOLUMETRICS_ENABLED` const matching the composite gate. When M-LIGHT (multi-tap soft shadows) lands, flip both gates together. Keep `write_tlas` as-is (cheap, descriptor stays valid).

**Disprove attempt**: Driver might fold `* 0.0` to dead code in the FS, but that doesn't help — the dispatches are CPU-issued via Vulkan, they always run.

#### PERF-CPU-01 — Diagnostic Vec allocation in skinning hot path (M41 dropout detection)
**Dimension**: CPU Allocations
**File**: `byroredux/src/render.rs:335`

**Evidence**: `let mut dropout_slots: Vec<(usize, bool)> = Vec::new();` allocated per-skinned-mesh once `frame_count >= 60`. Gated only by `call_once` log writes — the Vec itself is always allocated.

**Impact**: ~8–16 bytes alloc per skinned mesh per frame post-cold-start. Loaded cell with 50+ skinned NPCs ≈ 0.5–1 KB/frame of throwaway allocations. Below FPS-signal threshold but shows up in dhat profiles.

**Fix**: Wrap in `cfg!(debug_assertions)` or remove entirely if M41.0 Phase 1b.x dropout investigation is closed. ~2 lines.

**Note**: Estimated allocation count — no dhat regression test wired (recurring infrastructure gap).

### MEDIUM

#### PERF-GP-02 — `pipeline_two_sided` is redundant with dynamic CULL_MODE
**Dimension**: GPU Pipeline
**Files**: `crates/renderer/src/vulkan/pipeline.rs:220-237,287`, `context/draw.rs:1322-1351`, `byroredux/src/render.rs:233-243`

**Evidence**: Both opaque pipelines declare CULL_MODE dynamic (pipeline.rs:287). Their *only* baseline difference is `cull_mode(BACK)` vs `cull_mode(NONE)` in static state — overridden at every draw via `cmd_set_cull_mode`. The opaque sort key in `render.rs:233` orders by `(0, render_layer, two_sided, …)`, so `two_sided` flips force a redundant `cmd_bind_pipeline` 2-8 times per frame on typical interior content.

**Impact**: <0.2 ms/frame steady-state. **Larger blast radius**: deleting one of the two pipelines also halves the `(src, dst, two_sided)` blend pipeline cache axis to `(src, dst)`, halving its size.

**Fix**: Delete `pipeline_two_sided`, collapse `(src, dst, two_sided)` blend cache key to `(src, dst)`, keep dynamic CULL_MODE state. Two-sided alpha-blend ordering split (draw.rs:1486-1489) stays.

**Disprove attempt**: Vulkan spec: when CULL_MODE is dynamic, the pipeline static value is *ignored* at draw time. Pipelines compile to identical machine code on every desktop driver.

#### PERF-GP-03 — Bloom emits 19 image barriers per frame; collapses to ~3 with single mip-chain image
**Dimension**: GPU Pipeline
**File**: `crates/renderer/src/vulkan/bloom.rs:543-660`

**Evidence**: 5 down + 4 up = 9 dispatches; each surrounded by pre+post ImageMemoryBarrier pairs. Plus the HOST→COMPUTE UBO barrier. Total: **19 pipeline barriers + 9 dispatches + 2 pipeline binds**. Each mip is a separate `vk::Image`, so the barrier API can't see same-image read/write boundaries.

**Impact**: ~60 µs CPU + 100–300 µs GPU L2 stalls per frame. Below FPS-signal threshold today; ratchets up if BLOOM_MIP_COUNT grows.

**Fix**: Pack pyramid into single `vk::Image` with `mip_levels = BLOOM_MIP_COUNT` and use `subresource_range` per barrier. Inter-dispatch barriers collapse to a single COMPUTE→COMPUTE *whole-image* barrier between down and up chains — 19 barriers → ~3. Standard FidelityFX SPD layout. Larger refactor (~150 LOC); document as M58 follow-up rather than blocking.

**Disprove attempt**: Read-after-write on adjacent mips needs L1 invalidation regardless of image identity — true, but a single whole-image COMPUTE→COMPUTE barrier achieves the same flush in *one* call.

#### PERF-CPU-02 — Per-frame triggers Vec in footstep_system (M44 Phase 3.5)
**Dimension**: CPU Allocations
**File**: `byroredux/src/systems.rs:874`

**Evidence**: `let mut triggers: Vec<Vec3> = Vec::new();` allocated unconditionally every frame in footstep_system (commit 3987ecd, M44 Phase 3.5).

**Impact**: ~24 bytes per footstep trigger per frame. Loaded cells with 5–10 walking NPCs: typically 0–50 B/frame, peak 500+ B in worst case.

**Fix**: Pre-allocate with `with_capacity(32)` and `clear()` each frame to reuse the allocation. Footstep triggers are bounded by NPC count; capacity is stable.

**Note**: Estimated. Same dhat infrastructure gap as PERF-CPU-01.

### LOW

#### PERF-GP-04 — Volumetrics integration has no early-out on saturated transmittance
**Dimension**: GPU Pipeline
**Files**: `crates/renderer/src/vulkan/volumetrics.rs:851-855`, `crates/renderer/shaders/volumetrics_integrate.comp:29`

**Evidence**: Integration dispatch is 18×12×1 workgroups, each thread Z-marching all 128 slices unconditionally. No early-out when `T_cum < ~1e-5`.

**Impact**: ~0.5–1 ms saved on dense-fog scenes. **Subordinate to PERF-GP-01** — if the whole pass is gated off, this becomes moot.

**Fix**: Add `if (T_cum < 1e-5) break;` near the bottom of the per-slice loop. ~3 lines.

**Disprove attempt**: GPU thread divergence inside a workgroup — but a workgroup is 8×8=64 threads and adjacent froxel columns saturate together (same density). Divergence loss small.

#### PERF-GM-02 — Volumetrics dispatch wastes ~28 MB/frame of memory bandwidth
**Dimension**: GPU Memory
**Files**: `volumetrics.rs::dispatch` + `composite.frag:362`

**Evidence**: Cross-ref of PERF-GP-01 from a memory-bandwidth perspective. The integrated volume's 14 MB write per frame produces output that composite multiplies by 0.

**Impact**: ~28 MB/frame of memory bandwidth (write+read of integrated volume). Bandwidth, not VRAM occupancy.

**Fix**: Subordinate to PERF-GP-01 — same gating change in `draw.rs::draw_frame` resolves this.

**Disprove attempt**: Verified `combined += vol.rgb * 0.0` in composite.frag:362 — vol.rgb result is unused; integration write is discarded.

#### PERF-DC-01 — `par_sort_unstable_by_key` may be slower than serial on 800-draw inputs
**Dimension**: Draw Call Overhead
**File**: `byroredux/src/render.rs:1101`

**Evidence**: `draw_commands.par_sort_unstable_by_key(draw_sort_key);` at typical Prospector scale (~811 draws). Rayon fork-join overhead can exceed the wall-clock cost of a single-threaded sort. Crossover for parallel sort on 64-bit comparable keys is typically 10K–100K elements; we're at <1K.

**Impact**: ~50–200 µs/frame at typical cell counts. 1-2% of the frame budget at 5-10 ms.

**Fix**: Microbenchmark `par_sort` vs serial. If serial wins at <2K draws, add a threshold:
```rust
if draw_commands.len() > 2000 {
    draw_commands.par_sort_unstable_by_key(draw_sort_key);
} else {
    draw_commands.sort_unstable_by_key(draw_sort_key);
}
```

**Disprove attempt**: Rayon's `par_sort` has internal adaptive thresholds, but tuned for primitive types, not key closures.

#### PERF-GP-05 — Composite descriptor set has 8 bindings; verified not thrashing (audit hint disproved)
**Dimension**: GPU Pipeline
**File**: `crates/renderer/src/vulkan/composite.rs:465-510`

**Evidence**: 7 sampler + 1 UBO bindings, well within VkPhysicalDeviceLimits. Set is populated once at construction (composite.rs:636-672), not per-frame. Only per-frame state is the UBO write into HOST_COHERENT memory — normal pattern.

**Impact**: None measurable today. Watch if binding count grows past ~16.

**Fix**: None required.

### INFO

#### PERF-GP-06 — TLAS REFIT dominates (verified)
**File**: `crates/renderer/src/vulkan/acceleration.rs:210-252`. `decide_use_update` returns UPDATE/REFIT when `tlas_last_gen == current_gen` AND BLAS-address vector unchanged. Static cells = REFIT every frame after the first. **Confirmation, not a finding.**

#### PERF-GM-01 — New M55 + M58 GPU memory footprint
**Files**: `bloom.rs:75-86`, `volumetrics.rs:103-115`. Calculation at 1280×720 baseline, `MAX_FRAMES_IN_FLIGHT=2`:
- Bloom: ~5 MB total (down + up pyramid × 2 frames).
- Volumetrics: ~56 MB total (2 volumes × 14 MB × 2 frames).
- **Combined new addition**: ~61 MB.

Negligible on RTX 4070 Ti target. At 4K, bloom ~40 MB, total ~96 MB additional.

#### PERF-GM-03 — Volumetrics RGBA16F is the right format despite 2× cost vs R11G11B10F
**File**: `volumetrics.rs:115`. The alpha channel stores transmittance, which the integration recurrence reads/writes in a tight feedback. R11G11B10F has no alpha; switching would need a separate volume = same memory + extra descriptor.

#### PERF-DC-02 — Glass single-sided override (NEW) is metadata-only; no draw-state cost
**File**: `byroredux/src/render.rs:714-742`. `material_kind == MATERIAL_KIND_GLASS → two_sided = false` is per-entity classification. Pipeline state set dynamically per-draw via `cmd_set_cull_mode`. No new pipeline; possibly 1 fewer pipeline cache lookup per glass mesh.

#### PERF-DC-03 — Sort key 9-tuple is large but cache-friendly
**File**: `byroredux/src/render.rs:219-244`. 9-tuple = 27 B/key. 811 keys × log₂(811) ≈ 8.3 KB sort working set; fits L1. Lexicographic short-circuit on slot 0 (opaque/blend) optimal. Packing to u128 would add encode/decode cost.

## Prioritized Fix Order

1. **PERF-GP-01** (HIGH, **single highest-impact win**) — gate `vol.dispatch()` behind a const matching the composite gate. ~5 LOC, recovers ~10-20 ms/frame estimated. Pair with M-LIGHT v2 work since they re-enable together.
2. **PERF-CPU-01** (HIGH, code-hygiene) — `cfg!(debug_assertions)` on the M41 dropout-detection Vec. ~2 LOC.
3. **PERF-CPU-02** (MEDIUM, code-hygiene) — pre-allocate footstep triggers with capacity-32. ~2 LOC.
4. **PERF-GP-02** (MEDIUM, code-quality + small CPU) — delete `pipeline_two_sided`, collapse cache axis. ~30 LOC.
5. **PERF-GP-04** (LOW, subordinate to GP-01) — `if (T_cum < 1e-5) break;` in integrate shader. ~3 LOC.
6. **PERF-DC-01** (LOW) — microbenchmark `par_sort` vs serial; add threshold if serial wins.
7. **PERF-GP-03** (MEDIUM, follow-up) — bloom mip-chain refactor. ~150 LOC; not blocking.

## Dimensions Deferred (no material change since recent audits)

| Dim | Last audited | Reason for deferral |
|---|---|---|
| 4 — ECS Query Patterns | 2026-05-04 | Baseline locked (#823–#828); no system changes since |
| 5 — NIF Parse | 2026-05-04 | NIF parser stable; no new block types or readers |
| 7 — TAA & GPU Skinning | 2026-05-06b | Architecture snapshot all-pass; M29.3 deferred deliberately |
| 9 — World Streaming | 2026-05-06b | CRITICAL CELL-PERF-01/02/03 trio still open from that audit (separate work track) |

## Cross-References

- Prior canonical: `docs/audits/AUDIT_PERFORMANCE_2026-05-06.md` (Dim 5 + 8) and `AUDIT_PERFORMANCE_2026-05-06b.md` (Dim 7 + 9)
- Recent commits in scope: `33f48b5` (M55+M58+M-LIGHT v1+golden tests, 2026-05-09), `f62d4bd` (bloom 0.20→0.15, glass single-sided, fresnel rim, 2026-05-09), `4d54f47` (glass diffuse mip-bias, 2026-05-09)
- Related infrastructure gaps:
  - dhat / alloc-counter regression coverage NOT wired (recurring; flagged in 2026-05-04 audit)
  - GPU-side cost estimates in this audit are CPU back-of-envelope; RenderDoc/Nsight measurement recommended before claiming PERF-GP-01 magnitude

---

*Audit run by orchestrator + 4 dimension agents (Dim 1, 6, 8 via specialist agents; Dim 2 + 3 written by orchestrator from direct read-through). Total: 14 findings across 5 dimensions; 4 dimensions deferred to recent audits.*
