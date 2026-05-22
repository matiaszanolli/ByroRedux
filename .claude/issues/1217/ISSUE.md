**Severity**: LOW (observability; depends on FIND-1)
**Dimension**: NIF Format Readiness
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` Dim 2 FIND-3

When a `_oc.nif` imports to zero meshes (CSG case), `parse_and_import_nif` returns `Some(arc)` and the precombined loader inserts that arc into `NifImportRegistry`. Subsequent cells in the same load order referencing the same hash get the cached zero-mesh entry on the cache-hit path (`byroredux/src/cell_loader/precombined.rs:85-88`) and re-skip silently — they never even hit the `parse_and_import_nif` log site, so a single `warn!` from FIND-1 only fires once per process per `_oc.nif` path.

Not strictly a bug (cache is content-addressed by `path`), but it weakens FIND-1's fix.

### Suggested Fix
When the cache **hits** on a zero-mesh `CachedNifImport`, fire `log::debug!` (not `warn`) at the `precombined::spawn_precombined_meshes` call site so the operator can see post-mortem how many cells took the CSG-deferred fallback. Pair with `pc_spawned` count already logged at info.

### Completeness Checks
- [ ] **TESTS**: Two-cell load test that re-uses a precombined hash should show one `warn!` (first cell) + one `debug!` (second cell cache-hit).
