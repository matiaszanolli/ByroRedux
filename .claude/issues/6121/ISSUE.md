# NIF-SD-06: Doc rot in the contract files

Labels: low,nif-parser,nif,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6121

**Severity**: LOW · **Dimension**: Documentation · **Games**: n/a

## Location
`byroredux/src/streaming/pre_parse.rs` (module header and `parse_one_nif` / `pre_parse_cell` docs); `byroredux/src/cell_loader/nif_import_registry.rs` (`collision_authoring_totals_registry_wide` doc, `CachedNifImport::embedded_clip` doc); `byroredux/src/cell_loader/partial.rs` (`insert_cached_import` doc); `byroredux/src/parsed_nif_cache.rs` (module doc)

## Description
1. `pre_parse.rs`: the module header is two `//!` blocks, the second an orphan fragment ("bootstrap uses the same request and payload path..."); `parse_one_nif`'s doc begins with the first half of `pre_parse_cell`'s doc and is cut mid-sentence; `pre_parse_cell` is separated from its `#[tracing::instrument]` attribute by a blank line.
2. `nif_import_registry.rs`: the `collision_authoring_totals_registry_wide` summary paragraph is attached to the `#[cfg(test)]` seeder `test_support_seed_authoring`, so the production method keeps only the `#4684` tail; `CachedNifImport::embedded_clip` still says it is "write-only on the cell-loader path (the spawn doesn't yet attach `Name` components...)" although `synth_child.rs` registers it and the spawn consumes the clip handle.
3. `partial.rs`: `insert_cached_import` says "No-op release when `BYRO_NIF_CACHE_MAX=0` (default unlimited mode)"; the default is 2,048 (#1724 fixed the same claim in `insert`).
4. `parsed_nif_cache.rs`: says `SceneImportCache` adds a `bypass_parses` counter; no such field exists (`grep -rn bypass_parses byroredux/src` returns only that comment - verified at publish time).

## Impact
Misleads the next reader of the pre-parse and cache contracts (NIF-SD-01's premises are exactly what these comments obscure).

## Related
#5472, #5730 (same class, different sites).

## Suggested Fix
Re-attach the doc blocks to their items and delete the stale sentences.

Source: docs/audits/AUDIT_NIF_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
