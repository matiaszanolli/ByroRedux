# PERF-2026-10-31-D7-02: FO4 interior-to-exterior first door re-parses the whole load order on the main thread (1.7-2.9 s, ~1.2 GB RSS peak)

Labels: low,performance,esm-plugin,game:fo4,bug
Source: docs/audits/ (2026-10-31 report)

**Severity**: LOW
**Dimension**: Performance / Streaming & Cells
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-31.md` PERF-2026-10-31-D7-02

## Location
`byroredux/src/cell_loader/load_order.rs` - `needs_exterior_processing` (~:607-614) and `SharedParseCache::get_or_parse` (~:697-727); the exterior leg is `build_exterior_world_context` called from `byroredux/src/app_step.rs`. Introduced by `977936719` (the #6464 FO4 Use Land Data change; #6464 is closed).

## Description
`needs_exterior_processing` now includes `GameKind::Fallout4`, and the shared parse's `exterior_ready` reads the same predicate. An index parsed for an interior is therefore not served to an exterior session on FO4 (`EsmIndex` is not `Clone`, so it cannot be processed in place) and the whole order is parsed again. Before #6464, FO4 interior-to-exterior reused the resident index, which was #6123's point. The audit-performance premise names only Oblivion and FO3/FNV as re-parsing "by design". The cost is paid once: after the re-parse the slot is `exterior_ready`, and later exterior-to-interior transitions reuse it.

## Evidence
`matches!(game, Oblivion | Fallout3NV | Fallout4)`; `let ready = shared.exterior_ready || !exterior; if shared.files == files && ready { return reuse }`, else `parse(plugin_paths)` then `apply_exterior_processing`. The test `the_cache_and_the_processing_step_share_one_gate` pins that FO4 now reparses.

## Impact
A 1.7-2.0 s (`Fallout4.esm`) to about 2.85 s (FO4 + 7 DLC) synchronous parse, with a parse-time RSS peak of roughly 1.1-1.4 GB on top of the still-resident interior index, at the first door out of an FO4 interior when the session began in an interior. Sessions that begin in an exterior (`--grid`) are unaffected. The correctness gain (29 LAND-less cells now stand on terrain) is real, so this is a cost to bound, not to revert.

## Suggested Fix
Apply `inherit_parent_landscapes` in place when the slot's `Arc<EsmIndex>` is uniquely held (`Arc::get_mut` after the interior session drops its clone), or run the inheritance on the interior parse too (it is idempotent and only fills LAND-less cells) so one parse serves both session kinds.

## Related
#6123, #6407, #5505, #6464

## Completeness Checks
- [ ] **SIBLING**: Oblivion / FO3 / FNV / Skyrim (once added) interior-to-exterior behaviour checked for the same cost
- [ ] **TESTS**: `the_cache_and_the_processing_step_share_one_gate` updated to pin the chosen reuse behaviour
