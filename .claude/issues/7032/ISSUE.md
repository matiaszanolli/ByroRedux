# REN-D7-01: `shader-pipeline.md` still says `CompositeParams` ends in the aperture array

Labels: low, renderer, documentation, doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/7032

**Severity**: LOW
**Source**: `docs/audits/AUDIT_RENDERER_2026-11-16.md` (renderer audit, delta since 2026-11-09)
**Location**: `docs/engine/shader-pipeline.md` (the "aperture UBO lives in composite" bullet, ~lines 972-977); `CompositeParams` in `crates/renderer/src/vulkan/composite.rs`
**Status**: NEW (validated against HEAD; no matching open issue)

## Description
The doc says `CompositeParams` ends in `sky_aperture_count` plus the `CompositeSkyAperture` array and that only the prefix is written with the 12 KB tail unflushed. It now ends in `view_proj` (after all 256 apertures), which `composite_upload_ranges` always writes (#6836).

## Suggested Fix
Update the bullet: struct tail is `view_proj`; upload is two ranges (prefix through the live apertures + the `view_proj` tail).

Related (closed): #6836.
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
