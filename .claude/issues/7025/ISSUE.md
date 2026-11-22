# REN-D2-01 / REN-D6-01: docs and a test comment still state the old normal-alpha fallback rule

Labels: low, renderer, nifal, documentation, doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/7025

**Severity**: LOW
**Source**: `docs/audits/AUDIT_RENDERER_2026-11-16.md` (renderer audit, delta since 2026-11-09)
**Location**: rustdoc of `normal_alpha_spec_applies` (`byroredux/src/material_translate.rs`: "normal alpha is also the fallback reflection mask"); doc and in-body comment of `normal_alpha_masks_specular_intensity_not_roughness` (`crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`)
**Status**: NEW (validated against HEAD; no matching open issue)

## Description
Found independently by two dimensions. The new test comment says "materials that do not author their own environment strength", but only kind 11 is exempt and kind 1 authors a strength too; the rustdoc has no kind-11 carve-out.

## Suggested Fix
Add the kind-11 carve-out to the rustdoc; reword the test comment to "MultiLayerParallax only".

Related: REN-D2-02.
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
