# D5-NEW-01: Stale render-pass attachment comment claims a 7th "reservoir" attachment

**Issue**: #1643
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-16.md`
**Severity**: LOW · **Dimension**: GPU Pipeline
**Labels**: low, renderer, documentation, tech-debt

## Location
- `crates/renderer/src/vulkan/context/draw.rs:641-643`

## Description
The comment above the `clear_values` array still reads "7 color attachments + depth… 5 albedo, 6 reservoir, 7 depth", but the render pass is now 6 color + depth with no reservoir attachment (removed by #1583 / commit `218b425b`). The `clear_values` array itself correctly has 7 entries (HDR + 5 G-buffer + depth) — only the comment is stale.

## Evidence
- `gbuffer.rs` `struct GBuffer` has exactly 5 attachments (normal, motion, mesh_id, raw_indirect, albedo).
- `context/helpers.rs:78` confirms "6 color attachments + depth".
- No `gb_reservoir` / `reservoir` symbol exists anywhere in `crates/renderer/src/`.

## Impact
Zero runtime cost; maintenance hazard only.

## Suggested Fix
Update the comment to "6 color attachments + depth" with the correct 0–6 index list.

## Related
- #1583 (recommended for closure — attachment already removed by `218b425b`).
