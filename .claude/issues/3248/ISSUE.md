# 3248: D11-01: composite.rs descriptor-layout comment says '7 bindings', array declares 9

**Severity**: LOW · **Report**: `docs/audits/AUDIT_RENDERER_2026-08-24.md` (D11-01)

## Description

The comment above `ds_bindings` reads "7 bindings — HDR, indirect, albedo, params UBO, depth, caustic, volumetric (M55 Phase 4)" and enumerates exactly 7 items, but the array declares bindings 0-8 (9 total) — bindings 7 (bloom, `#2796`) and 8 (water-side caustic accumulator, `#1257`) were added later without updating the summary.

## Location

`crates/renderer/src/vulkan/composite.rs:702-703`

## Impact

Documentation drift only — `validate_set_layout()`, called immediately after, cross-checks against SPIR-V reflection at pipeline creation and fails fast on any real mismatch.

## Suggested Fix

Update the comment to "9 bindings" and add bloom + water caustic to the list, or drop the itemized list in favor of "see per-binding comments below."

## Completeness Checks
- [ ] **TESTS**: N/A — comment-only fix, no behavior change
