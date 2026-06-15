**Severity**: MEDIUM · **Dimension**: GPU Memory Pressure · **Status**: RE-AFFIRMED carry-over (F2/PERF2-03 2026-06-11, N1 2026-06-04) — never filed
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-14.md` (F3)

## Description
The 7th G-buffer attachment `gb_reservoir` (`R32G32B32A32_UINT`, `crates/renderer/src/vulkan/gbuffer.rs:60-63`; allocated `:231/:281/:444`) is written every fragment (`crates/renderer/shaders/triangle.frag:58,3473-3479`) but never read — the ReSTIR-DI resample compute pass that would consume it does not exist (`triangle.frag:3078-3080` states it is a separate milestone). `grep` across `draw.rs`/`svgf.rs`/`composite.rs` finds no reservoir read binding.

## Evidence
Verified live: `RESERVOIR_FORMAT = R32G32B32A32_UINT` with the comment confirming it matches `uvec4 outReservoir` in `triangle.frag` (`packReservoir` writes `floatBitsToUint(wSum/W)`). No consumer pass binds it.

## Impact
~66 MB @1080p, ~265 MB @4K of resident VRAM plus per-frame ROP write bandwidth for data with no consumer. Works against the < 4 GB VRAM target at 1440p+.

## Suggested Fix
Gate the attachment behind a feature flag (drop from framebuffer + render pass when off) until the resample pass lands, OR land the resample pass. If kept, consider `R32G32_UINT` (8 B/px) packing.

## Related
#1562 (doc-only: shader-pipeline.md omits the 7th attachment) — distinct from this VRAM finding.

## Completeness Checks
- [ ] **DROP**: If the attachment is removed from the framebuffer + render pass, the Drop/teardown stays reverse-order correct and FIF image count matches
- [ ] **SIBLING**: Confirm no other write-only G-buffer attachment is paying the same dead-VRAM cost
- [ ] **TESTS**: Pin the render-pass color-attachment count / fragment-output count match (cross-link #1564)
