# NIFAL-D6-08: NiTriStripsData.normals not cross-checked by resolve_tri_strips_data_refs, unlike sibling packed_triangle_winding check

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: LOW
**Dimension**: Collision · **Tier Violated**: parked-not-leak
**Location**: `crates/nif/src/import/collision/shape.rs` (`resolve_tri_strips_data_refs`), vs. `packed_triangle_winding` (`shape.rs:457`, gated for `BhkPackedNiTriStripsShape`)
**Status**: NEW

## Description

`NiTriStripsData.normals` (per-vertex, parsed) is never cross-checked by the
`bhkNiTriStripsShape`-derived collision path (`resolve_tri_strips_data_refs`),
unlike the sibling `packed_triangle_winding` check `c4481c78` added for
`BhkPackedNiTriStripsShape`. Explicitly **not** a fix for open issue `#2193`
— that issue's own investigation already hand-checked this for the actual
repro entity and found zero winding/normal disagreements across all 913
triangles. Documented asymmetry only.

## Evidence

`packed_triangle_winding` exists at `shape.rs:457` and is invoked from the
packed-mesh resolve path (`shape.rs:434`); no equivalent normal-vs-winding
cross-check exists in `resolve_tri_strips_data_refs`.

## Impact

Asymmetric robustness: one of the two structurally similar tri-strip
collision paths (`BhkPackedNiTriStripsShape`) gets a winding-vs-authored-normal
sanity check that the other (`bhkNiTriStripsShape` via
`resolve_tri_strips_data_refs`) does not. No known live defect — `#2193`'s own
investigation found no disagreements on its repro content — but the asymmetry
means a future corrupt/hand-edited NIF on this path wouldn't get the same
defense-in-depth check its sibling already has.

## Suggested Fix

Add the same normal-vs-winding cross-check to `resolve_tri_strips_data_refs`
that `packed_triangle_winding` provides for the packed-mesh path, for
consistency — low priority given no known live trigger.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`packed_triangle_winding`'s existing check)
- [ ] **TESTS**: A regression test pins this specific fix

## Filed as

GitHub issue #2302, labels: low, nif-parser, bug.
