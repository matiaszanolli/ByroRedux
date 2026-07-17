# DIM3-OBL-02: NpcRecord/ClasRecord.flags_oblivion parsed and real-data-verified but has no downstream consumer

- **Severity**: LOW
- **Labels**: low, import-pipeline, bug
- **Location**: `crates/plugin/src/esm/records/actor.rs:467` (field), `:1078` (populated in `parse_clas`)

## Description
`flags_oblivion: Option<u32>` decodes correctly from Oblivion's 60-byte CLAS `DATA` and is verified against real `Oblivion.esm` (`clas_oblivion_knight_against_vanilla`), but repo-wide grep shows no production consumer (e.g. no leveling/spellmaking-eligibility gate).

## Evidence
`grep -rn "flags_oblivion"` across `crates/` returns exactly 6 hits: field declaration (`actor.rs:467`), an initializer to `None` (`:1034`), the parse site inside `parse_clas` (`actor.rs:1078`, `record.flags_oblivion = r.u32().ok();`), two in-file unit test assertions, and one real-data integration test assertion in `crates/plugin/tests/parse_real_esm.rs`. No production/non-test code anywhere reads the field.

## Impact
None today — reads as intentional sequencing ahead of CHARAL (the per-game character-rules abstraction layer, `docs/engine/charal.md`, PROPOSED) reaching its Oblivion class-flag pass, not a bug.

## Related
CHARAL (`docs/engine/charal.md`)

## Suggested Fix
No action needed now; flag for CHARAL's Oblivion pass so it isn't rediscovered as a "surprise" gap later.

## Completeness Checks
- [ ] **TESTS**: When CHARAL's Oblivion class-flag pass lands, verify `flags_oblivion` gets a real consumer and this issue can close
