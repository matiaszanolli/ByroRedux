# 2179: D5-N1: gbuffer.rs leak-guard doc comment undercounts attachments (5/30 vs actual 7/42, post-FSR)

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2179
**Labels**: bug, renderer, low

---

## Severity
LOW

## Dimension
GPU Pipeline (Dim 5) — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/gbuffer.rs`

## Description
The G-buffer leak-guard doc comment still cites the pre-FSR attachment count (5 attachments / 30 something-count). FSR 3.1 added two new attachments (reactive + transparency/composition masks per `5c56e311`/`5c7acfe2`), making the actual current count 7 attachments (42 in whatever unit the comment tracks).

## Impact
Doc-rot only; a maintainer reading the comment while adding an 8th attachment would miscalculate the expected leak-guard count.

## Related
D5-N2 (filed separately, same FSR-attachment area); PERF-D9-NEW-04 (filed separately, same doc-rot class in `gpu_timers.rs`).

## Suggested Fix
Update the comment's attachment/count figures to 7/42 (or whatever the correct post-FSR unit count is), confirmed against the current struct definition.

## Completeness Checks
- [ ] N/A — documentation-only fix
