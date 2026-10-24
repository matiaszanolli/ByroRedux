# PERF-D7-01: #6097's worker skip predicate is wider than the main thread's

- **Issue**: #6342 (snapshot as filed; GitHub is authoritative for live state)
- **Labels**: low, performance, terrain-exterior, game:fo4, bug
- **Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-24.md

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/streaming/pre_parse.rs:677-690` (`bake_absorbed_refs`), `:760-765` (the `has_mesh` predicate), `:776-781` (the skip); versus `byroredux/src/cell_loader/precombined.rs:545-547` (`if count > 0 { self.baked_hashes.push(hash) }`) feeding `effective_absorbed_refs` in `exterior/apply_job.rs`.
- **Description**: `88149ae26` makes the worker skip STAT/SCOL refs of every bake whose `_oc.nif` is present in the archives (`tex_provider.has_mesh(key)`, an archive-table `contains`). The main thread skips a ref only if its bake's hash is in `baked_hashes`, and `PrecombinedSpawnJob` pushes a hash only when the bake spawned `count > 0` entities (#5484: a bake that drew nothing — zero-mesh `_oc.nif`, CSG-deferred fallback — is not a carrier of its refs' geometry). The worker's predicate is a strict superset. For a bake that exists but draws nothing (zero-mesh file, CSG routing deferred, parse/CSG decode failure, or every upload refused by a closed geometry pool, #6106), the refs are rendered per-REFR on the main thread, but the worker never parsed their models, so each unique model takes an inline `extract + parse_and_import_nif` on the main thread, the cost the worker exists to remove.
- **Evidence**: code read (above). Those refs are about 95% of an FO4 exterior cell's placements (the #6097 premise).
- **Impact**: bounded to cells whose bake exists but draws nothing. Vanilla `Fallout4.esm` with its CSG present should be clean; the exposure is an install without the shared-geometry `.csg` (every bake then defers, so every cell takes the synchronous fallback) and the already-degraded #6106 pool-refusal case. Not measured (needs FO4 data and an engine run); no cost claim.
- **Related**: #6097 (closed), #5484, #6106, #6114.
- **Suggested Fix**: derive the worker's absorbed set from bakes the worker actually parsed with at least one mesh (order the ref-model collection after the `_oc.nif` pre-parse, or gate on the same `count > 0` signal), or at least log the worker's `skipped_absorbed` against the main thread's `absorbed_skipped` so a mismatch shows in the cell log. Secondary, doc rot in the same hunk: the `pre_parse_cell` doc block (`pre_parse.rs:652-664`) now sits above `bake_absorbed_refs`' doc, leaving `pre_parse_cell` undocumented and `bake_absorbed_refs` with two stacked blocks.


## Completeness Checks
- [ ] **SIBLING**: the interior/precombine path and `effective_absorbed_refs` callers use one absorption predicate
- [ ] **TESTS**: extend `absorbed_ref_tests` with a bake that exists but spawns 0 meshes (CSG-deferred) and assert its refs are still pre-parsed by the worker
- [ ] **DOC**: re-attach the `pre_parse_cell` doc block above `pre_parse_cell`
- [ ] **FOLLOW-UP**: re-run #6114's FO4 shared-model census after #6097

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-10-24.md`. Not measured (needs FO4 data and an engine run); no cost claim._
