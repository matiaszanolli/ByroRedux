# SK-D5-LZ4-LOW-01: open_with_numeric_siblings has no de-dup guard against explicitly re-listing an auto-loaded sibling

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2584
**Finding ID**: SK-D5-LZ4-LOW-01

**Severity**: LOW
**Dimension**: BSA v105 (LZ4)
**Location**: `byroredux/src/asset_provider/archive.rs:306-331`, called from `texture.rs:151-192`
**Status**: NEW

## Description
`build_texture_provider` opens each `--bsa`/`--textures-bsa` occurrence independently with no tracking of already-opened paths. A user who still explicitly lists every Skyrim archive (e.g. both `Meshes0.bsa` and `Meshes1.bsa`) gets `Meshes1.bsa` opened twice — once explicitly, once as the auto-loaded sibling of `Meshes0.bsa`. Caps at one duplicate per redundantly-listed archive (mid-series digits don't re-expand).

## Evidence
Confirmed directly: `open_with_numeric_siblings` (`archive.rs:306-331`) pushes each opened archive unconditionally with no `HashSet` of already-opened canonical paths.

## Impact
Wasted memory (duplicated directory `HashMap` + file handle) and non-deterministic archive lookup order between the two copies. Not a correctness bug — both copies are identical content. No evidence this fires in shipped smoke-test scripts or README examples.

## Suggested Fix
Track already-opened canonical paths in a `HashSet<String>` inside `build_texture_provider`, checked before both the primary open and each sibling open.

## Completeness Checks
- [ ] **TESTS**: A regression test opens an archive with an explicitly-listed sibling and confirms only one copy is opened
