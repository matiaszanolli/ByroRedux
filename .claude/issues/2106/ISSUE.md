# SF-D7-NEW-02: Starfield's two-digit mesh-archive series (Meshes01/Meshes02) is not covered by numeric-sibling auto-load — silent geometry loss on single-LOD BSGeometry blocks

**Severity**: HIGH
**Labels**: high, import-pipeline, legacy-compat, bug
**Location**: `byroredux/src/asset_provider/archive.rs:333-367` (`numeric_sibling_paths`); documented repro command at `docs/engine/game-compatibility.md:243-249`
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (SF-D7-NEW-02)

## Description
`numeric_sibling_paths` recognizes an FNV-style unsuffixed series and a single-digit zero-based Skyrim-style series, but Starfield's actual naming is a two-digit zero-padded series (`Meshes01`, `Meshes02`). Because `"Meshes01"` ends in `'1'` (not `'0'`), it falls into the mid-series "don't expand" bucket, so zero siblings auto-load. The project's own documented Starfield launch command passes only `--bsa "Starfield - Meshes01.ba2"`, with no explicit `Meshes02`. BSGeometry's external-mesh importer already tries every LOD slot and falls back gracefully when a mesh has multiple slots, but blocks with exactly **one** LOD slot (weapon internals, ship-module panels) have no fallback.

## Evidence
Measured 5/28 (17.9%) unrecoverable sub-meshes on a real weapon (`ar99.nif`) and 1/25 BSGeometry blocks fully lost on a ship cargo-bay module, using only `Starfield - Meshes01.ba2` as documented. Confirmed the missing geometry exists only in `Starfield - Meshes02.ba2`.

## Impact
Any real cell load using the project's own documented Starfield launch command silently drops close-range detail geometry (measured 15-18% in the two gameplay-relevant samples checked) with no warning at normal log level. Same bug class as already-fixed #1292/#1661, but for a naming shape neither fix covers.

## Related
Closed #1292 (same silent-drop symptom, different root cause), Closed #1661 (same function, fixed the single-digit Skyrim case).

## Suggested Fix
Extend `numeric_sibling_paths` with a Starfield-shaped two-digit zero-padded case (`Meshes01` → auto-load `Meshes02..`); update the documented Starfield repro command to pass all 5 archives explicitly; add a unit test for the `Meshes01` → `Meshes02` shape.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
