# NIFAL-D6-2026-08-07-02: docs/engine/nifal.md particle slice contradicts itself and current code on initial_radius

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2488
**Finding ID**: NIFAL-D6-2026-08-07-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 6 — NIFAL Material
**Location**: `docs/engine/nifal.md` §2 "Particles — emitter base params converged (2026-05-28)" vs `byroredux/src/systems/particle.rs::apply_emitter_params`
**Status**: NEW

## Description
The spec's first particle bullet states that `initial_radius` is deliberately **not** applied and that size stays owned by the preset. Two paragraphs later the *same section* states the opposite — that size is authored as `initial_radius × base_scale`. The code implements the second version. The first bullet is stale text left in place when the size work landed, and it is the paragraph an auditor/agent reads first.

## Evidence
Spec, first bullet: "`initial_color` (shipped as the white nif.xml default) and `initial_radius` (default 1.0) are **intentionally not applied** — colour stays owned by the `color_curve` override, size by the preset". Spec, later paragraph: "Particle **size** is authored too ... the translate sets a **constant** `start_size = end_size = initial_radius × base_scale`". Code (`systems/particle.rs:39-41`):
```rust
let size = p.initial_radius * p.base_scale.unwrap_or(1.0);
preset.start_size = size;
preset.end_size = size;
```
Only the `initial_color` half of the stale bullet is still true.

## Impact
Documentation-only, but on the authoritative NIFAL spec that reviewers and audit dimensions treat as the contract. A future change that "restores the documented invariant" by removing the size override would regress FNV oasis smoke back to ~7× oversized particles (the exact defect the `base_scale` work fixed).

## Related
#1434 (base_scale sanity), #1775 (radius_variation).

## Suggested Fix
Delete `initial_radius` from the "intentionally not applied" bullet (leave `initial_color`) and cross-reference the size paragraph below it.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
