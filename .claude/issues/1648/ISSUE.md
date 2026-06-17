# REN-D3-03: Authoritative docs still document the removed 7th (reservoir) G-buffer attachment

- **Issue**: #1648
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (doc divergence)
- **Source audit**: docs/audits/AUDIT_RENDERER_2026-06-16.md
- **Labels**: low, pipeline, documentation
- **Location**: `docs/engine/shader-pipeline.md` (L85, L97); `docs/engine/memory-budget.md` (L189); `crates/renderer/src/vulkan/context/helpers.rs` (L44)

## Description
`shader-pipeline.md` still lists a Reservoir R32G32B32A32_UINT location-6 G-buffer
row and "Seven colour attachments + depth"; `memory-budget.md` says "7 attachments
× 2 FIF". The reservoir was removed under `218b425b`/#1583; code is now 6.

## Evidence
Confirmed at HEAD: `shader-pipeline.md:85` + `:97`, `memory-budget.md:189`,
`helpers.rs:44` ("eight…the seven"). `218b425b` did not touch these docs.

## Impact
None functional. A reader sizing VRAM over-counts one 16 B/px × 2 FIF attachment
and hunts for a non-existent location-6 output.

## Suggested Fix
Delete the Reservoir row, "Seven" → "Six" in shader-pipeline.md; "7 attachments"
→ "6" + dependent MB figures in memory-budget.md; fix helpers.rs "eight…seven".

## Completeness Checks
- [ ] SIBLING: all three sites updated; no other doc references the location-6 reservoir attachment
