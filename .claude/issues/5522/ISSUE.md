# #5522: FO4-2026-10-09-D1-03: The precombine job collapses the worker's negative cache entry into a miss and re-parses on the main thread, unguarded

**Labels**: bug, game:fo4, legacy-compat, low, safety

**Source**: `docs/audits/AUDIT_FO4_2026-10-09.md` — finding `FO4-2026-10-09-D1-03` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW. This is a defense-in-depth gap with zero vanilla reach (235,082 of 235,082 parse clean).
- **Dimension**: 1, exterior streaming route agreement.
- **Location**:
  - `byroredux/src/cell_loader/precombined.rs:261-264`.
  - Compare `byroredux/src/cell_loader/references/synth_child.rs:594-607` and `byroredux/src/streaming_helpers.rs:663-668`.
- **Status**: NEW.
- **Description**:
  - When the worker's pre-parse of an `_oc.nif` fails, `finish_streaming_import` caches a negative entry
    (`reg.insert(key, None)`). The failure can be a missing file, a parse error, or a parser panic caught by
    `pre_parse.rs:256`.
  - The REFR loader honours that entry as a cached miss.
  - `PrecombinedSpawnJob` reads `reg.get(&path).and_then(|opt| opt.clone())`, which turns `Some(None)` into `None`. It
    then re-runs extract, `parse_nif` and the CSG decode **on the main thread**, where there is no `catch_unwind`, and
    re-inserts the negative entry.
- **Evidence**: `NifImportRegistry::get` returns `Option<&Option<Arc<CachedNifImport>>>`
  (`nif_import_registry.rs:619`). Only the REFR path distinguishes the two `None` levels.
- **Impact**:
  - A parser panic that the worker contained (#854's contract) is replayed on the main thread and takes the engine down.
  - A parse error costs one main-thread parse every time the cell is applied.
  - The skill's checklist requires the two routes to agree on the fail-closed fallback. They agree for owner, key and
    blend, but not for the negative case.
- **Related**: #854, CONC-D7-02 (the worker's pre-parse skip memo), and NIF-D4-2026-10-09-01 (a stack overflow is not
  catchable either way).
- **Suggested Fix**: in `advance`, treat `Some(None)` as a terminal miss (`misses += 1; next_hash += 1; continue`),
  matching the REFR loader. Add a unit test that seeds a negative entry and asserts there is no re-extract.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
