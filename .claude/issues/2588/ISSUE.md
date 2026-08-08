# SK-D6-03: BSTreeNode wind-bone lists are imported but have no consumer outside the NIF crate

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2588
**Finding ID**: SK-D6-03

**Severity**: LOW
**Dimension**: Specialty Blocks + Real-Data Rendering
**Location**: `crates/nif/src/import/walk/mod.rs:1589-1600`; `import/types.rs:161`
**Status**: NEW (informational — forward scope, same class as the VWD note this dimension was told not to re-file)

## Description
`BSTreeNode`'s two trailing `NiNode` ref lists (SpeedTree wind rig) are parsed correctly and surfaced onto `ImportedNode.tree_bones` by both walkers, but nothing outside `crates/nif`/`crates/spt` reads the field.

## Impact
None today (Skyrim trees render static). Recorded so the parse-vs-consume gap is on record rather than rediscovered as "the parser drops it."

## Suggested Fix
None required now — ready hook for when SpeedTree wind lands.

## Completeness Checks
- [ ] **TESTS**: N/A — forward-scope note, no action required
