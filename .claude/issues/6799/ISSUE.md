# ESM-D6-04: REGN `RDMP` decides lstring-vs-text by `len == 4`, never resolves the id, and stores a plugin-local id in a merged index

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6799
Labels: low, esm-plugin, bug
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (dead payload: nothing reads region map names)
- **Dimension**: 6
- **Record/Sub-record**: REGN `RDAT` section `RDMP` "Map Name" (`wbLString`, tes5.pas:13155 area, FO4.pas:15615 area)
- **Location**: crates/plugin/src/esm/records/misc/world.rs:1166-1177, enum doc at :712-715
- **Status**: NEW
- **Description**: (a) The guard is `data.len() == 4`, not the TES4 localized flag; the checklist item "the flag, not 'looks like a small integer', decides" is violated here. An Oblivion/FO3/FNV (non-localized) region whose map name is exactly 3 chars is mis-typed as an id (vanilla data has no 4-byte RDMP before Skyrim: all 10-22 bytes, so no live hit; a mod could). (b) The localized case produces `RegionDataPayload::MapStringId(u32)` whose doc says it is "resolved against the plugin's `.STRINGS` tables", but there is no resolver: the id is only ever stored, and once `EsmIndex::merge_from` folds plugins together the per-plugin table (thread-local, dropped at the end of the walk) is no longer reachable, so it can never be resolved later. `read_lstring_or_zstring` would have resolved it at parse time like every other lstring.
- **Evidence**: Skyrim.esm 7 REGN with 4-byte RDMP, Fallout4.esm 19; `grep MapStringId` shows only the decode site, the remap no-op arm and one test.
- **Impact**: none today (no consumer), but the payload is permanently unresolved text for any future region-name UI.
- **Suggested fix**: replace `Map(String)`/`MapStringId` with one `Map(String)` filled by `read_lstring_or_zstring(data)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
