# NIF-D3-2026-09-29-01: #3461's FO76 GeneratedMeshes fix has no corpus pin — parse_rate_fo76_all_meshes still carries the pre-fix floors (0.945 / 0.0)

**Labels**: low,bug,nif-parser,nif,test-gap,game:fo76

**Source**: `docs/audits/AUDIT_NIF_2026-09-29.md`
**Severity**: LOW (test-coverage gap; the code is correct today; the dispatch arm itself is unit-pinned)
**Dimension**: Block Dispatch Coverage (baseline harness)
**Game Affected**: Fallout 76 (bsver 155..=167)
**Location**: `crates/nif/tests/parse_real_nifs.rs` — the FO76 module note ("`GeneratedMeshes02` is **0.00% clean — all 2,049 of its NIFs truncate**") and the two `ArchiveSpec`s for `SeventySix - GeneratedMeshes01.ba2` / `GeneratedMeshes02.ba2` in `parse_rate_fo76_all_meshes`.

## Description
#3461 (closed 2026-09-02) gave `BSDistantObjectExtraData` a dispatch arm and cleared the FO76 distant-LOD truncation tail. The gate was never updated:
- `GeneratedMeshes01` is still floored at `min_clean: 0.945`;
- `GeneratedMeshes02` is still floored at `min_clean: 0.0`, next to a comment reading "Raise it the moment the tail is fixed, or the fix has nothing pinning it";
- the module note still says "0.00% clean — all 2,049 of its NIFs truncate".

First reported as NIF-2026-09-04-D3-01 but never published.

## Evidence
- The audit's corpus run reports `GeneratedMeshes01` 20,270/20,270 clean and `GeneratedMeshes02` 2,055/2,055 clean.
- The per-type gates cannot see these archives: `Game::mesh_archives` for FO76 is `&["SeventySix - Meshes.ba2"]` (`crates/nif/tests/common/mod.rs`), and no baseline TSV contains `BSDistantObjectExtraData`.
- The only pin is `dispatch_tests/extra_data.rs`, which catches deletion of the dispatch arm but not a layout regression.

## Impact
A regression that makes these 22,325 NIFs truncate again (e.g. a mis-read field in `BsDistantObjectExtraData` or any other `GeneratedMeshes`-only block) would pass every gate: truncation counts as "recoverable", and the clean floor is 0.

## Related
#3461, #3466 (closed); #4628 (same gate's archive list is stale); #4619 (the lane that would run this).

## Suggested Fix
Raise both floors to 0.995 (measured value minus the usual 0.5%) and rewrite the module note and inline comments to the measured state. Consider doing it with #4628's FO76 archive-list refresh.

Validated at HEAD 9fcfdc3fc: `parse_real_nifs.rs` still has `min_clean: 0.945` (GeneratedMeshes01) and `min_clean: 0.0` (GeneratedMeshes02) with the "Raise it the moment the tail is fixed" comment and the "0.00% clean" module note.

## Completeness Checks
- [ ] **SIBLING**: other `ArchiveSpec` floors whose tails have since been fixed
- [ ] **TESTS**: the raised floors are confirmed green against the real FO76 corpus
