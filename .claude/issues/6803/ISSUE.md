# ESM-D7-03: `esm-records.md` says 115 `categories()`; the pinned count is 117

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6803
Labels: low, esm-plugin, documentation, doc-rot
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW
- **Location**: `docs/engine/esm-records.md:54` vs `crates/plugin/src/esm/records/tests.rs:2020` (`assert_eq!(EsmIndex::categories().len(), 117)`)
- **Status**: NEW
- **Description**: baseline 114 (`670f81917`), 115 after #6564's own refresh (`b89c1e59e`), now 117: `god_rays` (GDRY) landed in `3bfcbd58b` this morning and `volumetric_lighting` (VOLI) just before. `awk '/pub struct EsmIndex/,/^}/' index.rs | grep -c 'pub [a-z_0-9]*: HashMap<'` = 110 (the rest are non-map categories such as `cells`/`globals`). `docs/engine/memory-budget.md` deliberately states no count (#4083), so it cannot drift here.
- **Suggested fix**: replace the transcribed number with the re-derive instruction the memory-budget page uses, or bump to 117.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
