# DIM3-OBL-01: XESP doc comments mislabel the sub-record "(Skyrim+)" — it is Oblivion-era

- **Severity**: LOW
- **Labels**: low, import-pipeline, documentation
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:855-856`, `crates/plugin/src/esm/cell/mod.rs:517`

## Description
Both doc comments label `XESP` (REFR/ACHR/ACRE enable-parent gating) as a Skyrim+ feature, contradicting the parser's own comment three lines above the match arm and a sibling test's docstring, both of which correctly state XESP is present on Oblivion (`walkers.rs:788-791`; `crates/plugin/src/esm/cell/tests/refr.rs:583-588`).

## Evidence
`walkers.rs:855-856` reads `// XESP — enable-parent gating (Skyrim+).` while `walkers.rs:788-791` correctly documents that ACRE (Oblivion-only) shares "NAME/DATA/XSCL/XESP" wire layout with ACHR on Oblivion. `crates/plugin/src/esm/cell/mod.rs:517` has the identical `(Skyrim+)` mislabel on the `EnableParent` struct doc. The `b"XESP"` match arm itself (`walkers.rs:861`) has no `GameKind` guard — it already parses unconditionally, so this is a pure doc-label bug, not a live parsing defect.

## Impact
None today. Risk is a future contributor reading the "(Skyrim+)" label and adding an incorrect `if game != GameKind::Oblivion` guard, which would reintroduce the #349/#396-class bug (Ayleid ruin / Oblivion gate / dungeon creature placements silently skipped).

## Related
#349, #396, #471 (all closed, none regressed)

## Suggested Fix
Drop the "(Skyrim+)" qualifier from both comments, or replace with "present since Oblivion".

## Completeness Checks
- [ ] **SIBLING**: Grep the rest of `crates/plugin/src/esm/cell/` for other sub-record comments with era-mislabels of the same shape
