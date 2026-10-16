# #5899: PAR-D2-2026-10-15-04: `MaterialIndex` drops an `Objects` row whose `PersistentID` columns drift off `{Dir: u32, Ext: u32}` silently

- **Labels**: low,bug,nifal,game:starfield
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: `MaterialIndex` drops an `Objects` row whose `PersistentID` columns drift off `{Dir: u32, Ext: u32}` silently. #5672's degradation counter covers `DBID` and `Parent` but not the key columns #5465 now reads by name

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs:749-760`. The `match (u32_field(pid, "Dir"), u32_field(pid, "Ext"))` falls to `_ => None` with no count. The warning at `:762-768` counts only `dbid_misses` and `parent_misses`.
- **Status**: NEW. It is the residual of #5672 (fa6666053, closed) as reshaped by 30bdbcc3b (#5465, closed). The pre-#5465 `n == 3` arm was equally silent. Searches for "PersistentID" in open and closed issues found only #5465 and #5502.
- **Trigger Input**: a mod or DLC `materialsbeta.cdb` whose `BSResource::ID` class declares `Dir` or `Ext` under another name or type (e.g. `u64`), or whose `ObjectInfo.PersistentID` is not an inline object.
- **Description**:
  - #5672 reasoned that "an unkeyed object only loses its own lookup" and made `DBID` and `Parent` misses warn once per build.
  - A `PersistentID` shape drift unkeys *every* row. `objects_by_key` ends empty, `MaterialIndex::build` returns `Ok`, and every `.mat` lookup misses into `apply_cdb_pbr_fallback`.
  - The only signal is the info-level "material index built (0 keyed objects)" at `byroredux/src/asset_provider/material/cdb.rs:175-179`. That is the wholesale silent degradation #5672 was filed to remove, moved onto the field #5465 just made name-dependent.
  - No test covers it. `degraded_objects_rows_and_parent_walks_warn_instead_of_truncating_silently` re-types `Parent` only.
- **Evidence**: code reading at HEAD (`index.rs:735-769`). Vanilla is clean: the base CDB keys 500,403 objects, which equals the Objects row count in the baseline `cdbprobe`.
- **Impact**: a schema-drifted mod CDB renders every Starfield material it should own as PBR-fallback, with no warning naming the cause.
- **Related**: #5672, #5465, #5320, #5502.
- **Suggested Fix**: count `pid_misses` beside `dbid_misses` and `parent_misses` and include them in the same once-per-build warning. Return a typed error (in the style of `RowInstanceMismatch`) when rows exist but none keyed. Extend the #5672 fixture to re-type `PersistentID.Dir`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
