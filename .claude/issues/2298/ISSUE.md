# NIFAL-D2-01: de-strip dedup incomplete — resolve_compressed_mesh and NiSkinPartition still hand-copy the strip-to-triangle conversion

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: LOW
**Dimension**: Geometry · **Tier Violated**: single-boundary
**Location**: `crates/nif/src/import/collision/shape.rs` (`resolve_tri_strips_data_refs`, uses `NiTriStripsData::to_triangles()` directly) vs. `resolve_compressed_mesh`'s chunk-strip walk (same file) vs. `NiSkinPartition`'s inline destrip (`crates/nif/src/blocks/skin.rs:300-318`)
**Status**: NEW

## Description

The `#2193` de-strip dedup is incomplete: `resolve_tri_strips_data_refs` was
unified to call `NiTriStripsData::to_triangles()` directly, but
`resolve_compressed_mesh`'s chunk-strip walk and `NiSkinPartition`'s inline
destrip remain separate hand-copies of the same jagged-strip-to-triangle-list
conversion (same OpenGL/Vulkan CCW winding + degenerate-skip convention,
reimplemented three times). All three are verified orientation-equivalent
today — latent, not live. Notably, `resolve_compressed_mesh`'s copy *did*
diverge to the wrong convention until an unrelated bug-fix pass (`3b9227341`)
silently corrected it — small evidence the drift risk is real, not
theoretical.

## Evidence

`resolve_tri_strips_data_refs` (shape.rs:385-397) comments "de-strip through
`NiTriStripsData::to_triangles`, the same ... strip-parity rule"; the
`blocks/skin.rs:300-318` block independently reimplements the identical
even/odd-index CCW winding logic with its own `#1549` comment citing
`NiTriStripsData::to_triangles`'s convention as the source of truth it must
match, rather than calling it.

## Impact

Latent drift risk only, not a live defect — all three copies currently agree.
The prior silent divergence in `resolve_compressed_mesh` (caught and fixed
incidentally by `3b9227341`, not by design) shows the risk materializes in
practice when the copies aren't kept in lockstep by hand.

## Suggested Fix

Extract the shared de-strip helper (even/odd CCW winding + degenerate skip)
once and call it from all three sites, eliminating the hand-copy risk
entirely rather than relying on comments to keep them in sync.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all three de-strip call sites)
- [ ] **TESTS**: A regression test pins this specific fix

## Filed as

GitHub issue #2298, labels: low, nif-parser, tech-debt, bug.
