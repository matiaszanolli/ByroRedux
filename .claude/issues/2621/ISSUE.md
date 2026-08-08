# SF-D3-04: material provider --bsa arm skips numeric-sibling archive expansion, missing DLC/Creation CDBs

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2621
**Finding ID**: SF-D3-04

**Severity**: MEDIUM
**Dimension**: 3 (CDB Material Database)
**Location**: `byroredux/src/asset_provider/material.rs:194-205` vs `byroredux/src/asset_provider/texture.rs:166-172`
**Status**: NEW

## Description
The texture provider opens mesh archives via `open_with_numeric_siblings`
(`Foo01.ba2` → `Foo02..09.ba2`, added specifically for Starfield's
zero-padded series). The material provider's `--bsa` arm calls bare
`Archive::open(path)` with no sibling expansion, so an invocation naming
only `Meshes01.ba2` gets `Meshes02…09` auto-loaded for meshes/textures but
**never scanned** for `materials\creations\<plugin>\materialsbeta.cdb`.

## Evidence
`byroredux/src/asset_provider/texture.rs:166-172` uses
`open_with_numeric_siblings`; `byroredux/src/asset_provider/material.rs:194-205`
(`build_material_provider`'s `--bsa` arm) calls bare `Archive::open` with no
equivalent expansion.

## Impact
This is precisely #1571's original failure mode ("a missed DLC CDB")
reappearing one level up, at archive selection rather than path selection.
Silent — `sf_cdb_count` just stays lower, and if no CDB is found at all,
every `.mat` mesh in the cell falls through to NIF-default rendering.

## Suggested Fix
Route the material provider's `--bsa` arm through the same
`open_with_numeric_siblings`, into a scratch `Vec<Archive>` scanned and
dropped. Also worth documenting (LOW, same site): a loose
`Data\materials\materialsbeta.cdb` (the natural mod-override shape) is
never discovered at all — not a regression, just undocumented.

## Related
#1571 (CLOSED — the path-matching half; this is the archive-selection
half).

## Completeness Checks
- [ ] **TESTS**: A fixture with `Meshes01.ba2` + `Meshes02.ba2` (the latter carrying the CDB) asserts the sibling is scanned
