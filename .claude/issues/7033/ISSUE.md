# REN-D8-12: `ripple_amplitude` (#7018) split `authored_wave_height_over_bed`'s rustdoc off its function

Labels: low, physics, water, documentation, doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/7033

**Severity**: LOW
**Source**: `docs/audits/AUDIT_RENDERER_2026-11-16.md` (renderer audit, delta since 2026-11-09)
**Location**: `crates/physics/src/water.rs` (`ripple_amplitude`, `authored_wave_height_over_bed`)
**Status**: NEW (validated against HEAD; no matching open issue)

## Description
`ripple_amplitude` was inserted between `authored_wave_height_over_bed`'s doc block and the public function, so the private helper carries both doc blocks and the public fn has none. Cross-owner finding (`/audit-physics`).

## Suggested Fix
Move the helper above the original doc block.

Related: #7018.
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
