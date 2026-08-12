# PERF-D3-03: memory-budget.md screen-sized ledger understates VRAM by 32 B/px

**Issue**: #2679
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`


- **Severity**: MEDIUM
- **Dimension**: 3 — GPU Memory Pressure
- **Location**: [memory-budget.md](docs/engine/memory-budget.md) — "Glass + Water Caustics" and "SVGF (indirect-lighting denoiser)" tables plus the "VRAM Rough Budget" rows; ground truth in [caustic.rs](crates/renderer/src/vulkan/caustic.rs) (`CAUSTIC_COLOR_LAYERS`, `CausticPipeline::create_slot`) and [svgf.rs](crates/renderer/src/vulkan/svgf.rs) (`SvgfPipeline::new_inner`, `atrous_color`)
- **Status**: NEW (no baseline issue covers either site; the closest precedent, #1814/#1872, created these very sections)
- **Description**: Two separate ledger gaps, same remedy.
  1. **Glass caustics tripled yesterday and the ledger did not follow.** Commit
     `610cb170` (2026-08-11) turned the glass accumulator into a three-layer R32_UINT
     array for RGB radiance: `const CAUSTIC_COLOR_LAYERS: u32 = 3;` and
     `.array_layers(CAUSTIC_COLOR_LAYERS)` in `create_slot`, pinned by
     `caustic_accumulator_spans_rgb_array_layers`. `water_caustic.rs` stayed at
     `.array_layers(1)`. The doc still says "each own a full-resolution R32_UINT
     (4 B/px) atomic accumulator image, double-buffered per FIF — two independent
     accumulators, 16 B/px combined". Actual: glass 3 × 4 B × 2 FIF = 24 B/px, water
     4 B × 2 FIF = 8 B/px → **32 B/px**.
  2. **SVGF's à-trous ping-pong pair was never ledgered.** The doc enumerates
     "two double-buffered history images per slot: `indirect_history` (4 B/px) and
     `moments_history` (8 B/px) … 24 B/px total". `SvgfPipeline::new_inner` also
     allocates, inside the same per-FIF loop, **two** full-resolution
     `INDIRECT_HIST_FORMAT` à-trous colour images per frame-in-flight
     (`for pp in 0..2 { … partial.atrous_color.push(at); }`), consumed by the
     `ATROUS_ITERATIONS` (currently 3) spatial pass. That is 4 × 4 B/px = 16 B/px
     never counted → actual **40 B/px**, not 24.
- **Evidence**: `CAUSTIC_COLOR_LAYERS = 3` + `caustic_subresource_range().layer_count == 3`
  (test-pinned); `svgf.rs` `atrous_color` is `Vec<HistorySlot>` indexed
  `frame * 2 + pp` and destroyed alongside the two histories in `destroy()`.
- **Impact**: Doc-only defect, but the doc is the *authoritative* input to every VRAM
  decision (this audit is required to cite it rather than re-derive). At the doc's own
  reference resolutions the understatement is **+66 MB at 1080p** (SVGF 49.8 → 82.9;
  caustics 33.2 → 66.4) and **+265 MB at 4K** (SVGF 199.1 → 331.8; caustics
  132.7 → 265.4) — ~6.6 % of the < 4 GB engine budget at 4K, on top of a peak table
  that is already the tightest part of that budget. Real-world numbers are lower than
  the doc's headings imply because all of these are allocated at `render_extent`
  (verified: `context/mod.rs` passes `render_extent.width/height` to
  `CausticPipeline::new` and `SvgfPipeline::new`), which under the shipped FSR 3.1
  Quality default is below output resolution — but that makes the *labels* wrong in the
  other direction, not the ratios.
- **Related**: #1814 (ReSTIR ledger entry + attributing telemetry), #1872 (the sweep
  that added this whole section and grepped for exactly these omissions). Both are the
  precedent for treating ledger drift as a finding. **Cross-audit**: overlaps REN-D5-03 /
  REN-D14-01 in the renderer report; the SVGF à-trous half is this report's unique
  contribution.
- **Suggested Fix**: Recompute both tables and the "VRAM Rough Budget" rows against
  `CAUSTIC_COLOR_LAYERS` and the à-trous pair; add the one-line `log::info!`
  size report at `CausticPipeline::new`/`create_slot` and `SvgfPipeline::new_inner`
  that #1814 established as the attribution mechanism, so the next resolution or
  layer-count change is self-reporting.

---


---
*Filed from [`docs/audits/AUDIT_PERFORMANCE_2026-08-12.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `PERF-D3-03`.*

## Completeness Checks
- [ ] **SIBLING**: Every listed site corrected, not just the first
- [ ] **TESTS**: Where a doc pins a numeric contract, a test asserts the number
