# #5231 — FO4-D4-02: PKIN expander models CNAM as a content base; every vanilla CNAM is a template CELL and 0 vanilla REFRs place a PKIN

https://github.com/matiaszanolli/ByroRedux/issues/5231

Source: `docs/audits/AUDIT_FO4_2026-10-03.md` (HEAD `32f4450d9`)

**Source**: `docs/audits/AUDIT_FO4_2026-10-03.md` (FO4-2026-10-03-D4-02)

- **Severity**: LOW (0 vanilla reach; doc/data-model rot plus a mod-only silent miss)
- **Dimension**: ESM architecture records + cell expansion
- **Location**: `crates/plugin/src/esm/records/pkin.rs:1-60`, `byroredux/src/cell_loader/refr.rs:529-614`
- **Status**: NEW. #589, #815, #1180, #2611 and #2612 (all closed) assumed the LVLI/CONT/STAT/MSTT/FURN content model.
- **Description**:
  - Docs and expander treat `CNAM` as "the content base record" and emit one synthetic child with `child_form_id = CNAM`.
  - On real data the CNAM is the pack-in's template CELL.
  - The CK bakes pack-in contents into ordinary REFRs at placement time, so vanilla carries no PKIN-based REFR.
- **Evidence**:

  | Plugin | PKIN | CNAM → CELL | REFRs with a PKIN base |
  |---|---|---|---|
  | Fallout4.esm | 872 | 872 | 0 |
  | DLCRobot | 21 | 20 | 0 |
  | DLCCoast | 64 | 64 | 0 |
  | DLCworkshop03 | 6 | 6 | 0 |
  | DLCNukaWorld | 46 | 46 | 0 |

  The census scripts are in `/tmp/audit/fo4/d4/pkin*.py`.
- **Impact**:
  - Vanilla: none. The expander is never reached, and nothing is double-spawned.
  - Mods: a mod-placed PKIN REFR would emit a child whose base is a CELL, miss every base lookup and spawn nothing.
  - `pkin_expansion_tests` encode the wrong model.
- **Related**: #589, #1180, #2611, #2612.
- **Suggested Fix**: correct the docs ("CNAM = pack-in template CELL"); then either instance that CELL's references under the outer transform, or log CELL-typed CNAMs as an explicit miss; add a real-data assert that the CNAMs resolve to CELLs.

## Completeness Checks
- [ ] **TESTS**: a real-data assert that vanilla PKIN CNAMs resolve to CELL records
- [ ] **DOCS**: `pkin.rs` doc + `pkin_expansion_tests` encode CNAM = template CELL
