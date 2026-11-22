# #7154 — PAR-D2-2026-11-21-01: a CDB whose `DBFileIndex` class declares no `Objects` field builds an `Ok` index with no keyed objects

Labels: medium,import-pipeline,bug,game:starfield

- **Severity**: MEDIUM (silently wrong output: every `.mat` lookup misses; the #5320 class)
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs` `stream_db_file_index` (the `for field in chunk_fields` match; the #7124 gate `if pid_misses == keyable_rows` sits inside the `"Objects"` arm, ~l.852)
- **Status**: NEW (sibling of #7124, closed)
- **Trigger Input**: a mod `materialsbeta.cdb` whose `BSComponentDB2::DBFileIndex` class declaration has no `Objects` field. That is schema-valid, and the class table is file-controlled.
- **Description**: the gate only runs when the field loop reaches `"Objects"`. Without the field, `objects_by_key` stays empty and no error is raised. `Components` still has its `RowInstanceMismatch` backstop, but nothing guards the key table. A smaller issue at the same site: `NoKeyedObjects { rows: keyable_rows }` prints "0 rows" when every row lacked a DBID, even though the table had rows.
- **Evidence**: verified by reading lines 770–870; no `saw_objects` and no post-loop `objects_by_key.is_empty()` check exist. No synthetic CDB was built.
- **Impact**: every Starfield material from that CDB falls back to the PBR presence-only path, and nothing is logged. Vanilla is unaffected (the five real-CDB tests pass).
- **Related**: #7124, #5320, #5899
- **Suggested Fix**: after the field loop, return `NoKeyedObjects` (or a new `MissingObjectsList`) when `objects_by_key.is_empty()`. Carry the total row count beside `keyable_rows` in the error. Add a fixture whose class omits `Objects`.

### LOW

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
