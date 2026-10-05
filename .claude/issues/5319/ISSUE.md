# #5319: PAR-D2-2026-10-05-01: `cdb_material_index` drops archive open/extract errors with `.ok()?` and memoises the failure silently

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5319
- **Labels**: low,import-pipeline,nifal,bug,game:starfield
- **Source**: `docs/audits/AUDIT_PARSERS_2026-10-05.md` (PAR-D2-2026-10-05-01)

_From `docs/audits/AUDIT_PARSERS_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `byroredux/src/asset_provider/material/cdb.rs:111-113`; log strings at `cdb.rs:117` and `cdb.rs:124`
- **Status**: NEW (introduced by `224a19372` / `18fce7e43`)
- **Trigger Input**: a discovered `materialsbeta.cdb` whose archive cannot be re-opened, or whose entry fails to re-extract, at first `.mat` lookup. Examples: an archive replaced mid-session, an I/O error, or a corrupt chunk past the header that the discovery probe read.
- **Description**:
  - The lazy build runs `let archive = Archive::open(source).ok()?; let bytes = archive.extract(inner).ok()?;`. This is the raw-`.ok()` shape #4658 removed from the providers.
  - Only the `MaterialIndex::build` `Err` arm warns. The `None` is then cached for the process, so every `.mat` lookup against that CDB silently degrades to the Phase-1 PBR fallback with no log line.
  - Separately, both of the function's log messages contain 22 embedded spaces (`"…material index built                      ({} keyed objects)"`), because a `\` line continuation was lost.
- **Evidence**: `cdb.rs:111-131`; `cat -A` of lines 117 and 124.
- **Impact**: an operator cannot tell "this CDB failed to load" from "this material is not in any CDB". The effect is diagnostic only: rendering falls back, and nothing crashes.
- **Related**: #4658, #3398, PERF-D3-2026-10-05-01
- **Suggested Fix**: match the open and extract results and `log::warn!` naming the source, path and error before memoising `None`, as `Archive::extract_or_warn` does. Restore the `\` continuations in both format strings.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
