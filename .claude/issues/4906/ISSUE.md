# Issue #4906

**Title:** EXT-D3-2026-09-27-01: Phase C ignores the authored LTEX→GRAS association — every load-order GRAS is placed everywhere by climate keyword, and its density is diluted by the record count
**State:** OPEN
**Labels:** bug, medium, terrain-exterior, shaders

**Severity**: MEDIUM (visual)
**Dimension**: Ground-cover pipeline
**Tier Violated**: no-fabrication
**Game Affected**: Oblivion, FO3, FNV, Skyrim, FO4
**Status**: NEW. `exal-groundcover.md:1645-1651` documents only the localisation part.
**Location**:
- `byroredux/src/groundcover_translate.rs:354-372,384` (`resolve_authored_cover` over `record_index.grasses`; `classify_species_name`).
- `byroredux/src/components.rs:458-469` (`authored_grass`, `#[allow(dead_code)]`).
- `crates/renderer/shaders/groundcover_models.comp:280,305`.
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
The widened carrier has no consumer. Main-context re-check: `authored_grass` is read only by its producer and tests. Three consequences:
- **(a) No localisation.** Species follow worldspace climate only.
- **(b) Load-order-wide mix.** DLC and mod GRAS appear in every worldspace.
- **(c) Diluted density.** A candidate first picks one record from the 256-entry table (`h >> 24`), then accepts with probability `rec.density × field × fade`.
  - Every added record therefore thins all the others.
  - Records whose model failed to load keep their table share and place nothing.

## Evidence
`groundcover_models.comp:280` (`gcRecordTable[h >> 24]`) and `:305` (`rec.density * field * fade`).

## Impact
- The wrong species grows per region.
- Species bleed across DLCs.
- Visible density depends on how many GRAS records the load order carries.

## Suggested Fix
- Resolve per-lane GNAM lists into `AuthoredCover` indices at the translate, and carry them in the cell data.
- Weight record selection by lane splat × climate.
- Evaluate density per record, or normalise by selection share.

## Related
#4642, #4413, EXT-D2-03 (the map is also empty at runtime, so fix that first)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **SPIR-V**: Edited shaders recompiled and `scripts/check-shader-artifacts.sh` is clean
- [ ] **TESTS**: A regression test pins this specific fix

