# 2180: D5-N2: FSR reactive/transparency masks are unconditional main-pass attachments, written even under --upscaler taa fallback

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2180
**Labels**: bug, renderer, low

---

## Severity
LOW

## Dimension
GPU Pipeline (Dim 5) — `/audit-performance` 2026-07-25

## Status note
Low confidence, estimated impact per the report — flagged for awareness rather than as a confirmed high-value fix.

## Location
`crates/renderer/src/vulkan/gbuffer.rs` (FSR reactive/transparency mask attachments)

## Description
The FSR 3.1 reactive-mask and transparency-and-composition-mask G-buffer attachments are allocated and written unconditionally in the main geometry pass, even when the engine is running under the `--upscaler taa` fallback path where FSR never consumes them.

## Impact
Estimated ~7 MB/frame of wasted attachment memory + write bandwidth on the non-default TAA fallback path only. Low confidence on the exact magnitude; a second render-pass permutation to conditionally omit these attachments is likely a poor trade for this modest saving.

## Related
D5-N1 (filed separately, same FSR-attachment area).

## Suggested Fix
Not recommended as-is per the report's own assessment (a second render-pass permutation is probably not worth ~7 MB/frame on a non-default path) — noting for awareness; revisit only if TAA-fallback VRAM pressure becomes a real complaint.

## Completeness Checks
- [ ] N/A — measurement-gated, no action recommended without further justification
