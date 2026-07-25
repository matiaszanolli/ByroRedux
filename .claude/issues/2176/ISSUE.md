# 2176: PERF-D3-01: memory-budget.md has no FSR 3.1 entry; existing screen-sized tables keyed to the wrong resolution axis post-5c7acfe2

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2176
**Labels**: bug, low, performance

---

## Severity
LOW

## Dimension
GPU Memory Pressure (Dim 3) — `/audit-performance` 2026-07-25

## Location
`docs/engine/memory-budget.md`

## Description
FSR 3.1's new GPU resources (upscaler outputs, reactive/transparency masks, SDK working memory) are leak-free and FIF-correct (verified this sweep), but none of them appear in `memory-budget.md`. Additionally, the doc's existing screen-sized tables are keyed to the output resolution axis, which is no longer the same as the render resolution now that FSR 3.1 Quality (rendering at a lower internal resolution and upscaling) is the shipped default (`5c7acfe2`) — the tables need a render-vs-output resolution split to stay accurate.

## Impact
Same class as closed #1814 and closed #1872 (doc rot: new GPU resource families absent from the authority doc) — no live defect, but VRAM planning against the 6 GB RT-minimum target is incomplete without these rows.

## Related
PERF-D4-02 (filed separately, same root cause — the doc hasn't tracked the last two sessions of SSBO/image additions; recommend fixing in one documentation pass with PERF-D3-02 below).

## Suggested Fix
Add an FSR 3.1 resource section (upscaler outputs, reactive/transparency masks, SDK working memory) with both render-resolution and output-resolution figures, and split the existing screen-sized tables along that same axis.

## Completeness Checks
- [ ] N/A — documentation-only fix
