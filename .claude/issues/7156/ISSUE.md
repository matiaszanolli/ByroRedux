# #7156 — PAR-D1-2026-11-21-03: `sorted_entries` collects a whole directory listing before the loose budget is charged

Labels: low,import-pipeline,bug

- **Severity**: LOW
- **Dimension**: Size Discipline
- **Location**: `byroredux/src/asset_provider/archive.rs:107` (`sorted_entries`), called from `walk` and `index_bounded`
- **Status**: NEW (residual of #7141, closed)
- **Trigger Input**: one loose directory under `Data\` holding millions of files.
- **Description**: `read_dir(dir).flatten().collect()` materialises every `DirEntry` before `MAX_LOOSE_ENTRIES` is charged, so memory is bounded by the directory, not by the budget. `sort_by_key(DirEntry::file_name)` also allocates an `OsString` on every comparison.
- **Suggested Fix**: keep only the `budget - examined` smallest names (a bounded max-heap), or at least use `sort_by_cached_key`; charge the top-level listing the same way.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
