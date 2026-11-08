# ESM-D4-05: FO76 `SubgraphRole::LinkingOnly` for value 3 rests on a conflicting xEdit copy, with no vanilla occurrence

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6793
Labels: low, esm-plugin, bug, game:fo76
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (INFO; zero records affected)
- **Dimension**: 4 · **Status**: NEW, adjacent to closed #6621
- **Location**: `crates/plugin/src/esm/records/actor/race.rs` `SubgraphRole::from_raw`, arm `(3, Fallout76 | Starfield) => LinkingOnly`
- **Description**: #6621's source for the FO76 half is `/mnt/data/tmp/wbDefinitionsFO76.pas:14390` (Sep 30; `{3} 'Linking Only'`). The Oct 31 dev copy `esmaudit/FO76.pas:16329` says `{3} 'Idle'` with a 5-value enum, like FO4. The two files also differ in element order, so they are different upstream snapshots.
- **Evidence**: raw scan of all 157 `SeventySix.esm` `RACE`: `SRAF` roles are {0: 73, 1: 1,415, 2: 969, 4: 2}. **No role-3 entry exists**, so the arm is unreachable on vanilla data.
- **Impact**: none today. Under the "no guessing" rule the FO76 half is unresolved and unmeasurable.
- **Suggested fix**: either note the conflict at the arm, or leave FO76 value 3 as `Other(3)` until a source or a role-3 record settles it. Starfield's `Linking Only` is solid (its xEdit copy plus the 5-7 additions and Starfield data).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
