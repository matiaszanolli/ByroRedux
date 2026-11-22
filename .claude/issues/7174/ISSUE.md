# #7174 — PAR-D2-2026-11-21b-03: a CDB index that keys or describes almost nothing still builds `Ok`

Labels: low,import-pipeline,bug,game:starfield

- **Severity**: LOW (PLAUSIBLE; vanilla is unaffected)
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs`: `stream_db_file_index` (`objects_by_key.insert(key, dbid)` and the #7154 `is_empty()` check), and `MaterialIndex::build`'s `idx.rows.len() != instances` check
- **Status**: NEW (siblings of #7154, closed)
- **Trigger Input**:
  - (a) a mod CDB whose `Objects` rows all carry one `(Dir, Ext)` PersistentID, for example a drift that zeroes both;
  - (b) a `DBFileIndex` with keyed `Objects`, no or empty `Components`, and no instances after it.
- **Description**:
  - (a) key collisions are last-wins with no count, so one surviving key passes the `is_empty()` gate.
  - (b) `0 == 0` passes the row/instance join, so every lookup resolves to an object with no component rows.
  - Both are the #5320 / #7154 silent-degrade class at its edges.
- **Suggested Fix**: count `insert(..).is_some()` into the existing "Objects table decode degraded" warning, and refuse an index with no component rows the way `NoKeyedObjects` does.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
