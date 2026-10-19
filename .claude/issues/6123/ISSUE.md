# PERF-SD-2026-10-19-01: Every door transition re-parses the whole ESM load order on the main thread, though the parsed index is already resident

Labels: medium,performance,esm-plugin,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6123

**Severity**: MEDIUM · **Dimension**: Streaming & Cells (Dim 7)

## Location
- `byroredux/src/cell_loader/load.rs` (`InteriorCellApplyJob::begin` -> `parse_record_indexes_in_load_order`), reached from `cell_loader/transition.rs` (`InteriorCellApply::begin`) and `app_step.rs`.
- `byroredux/src/cell_loader/exterior.rs` (`build_exterior_world_context` -> the same parse), reached from `app_step.rs` for any door whose destination is exterior.
- No cache: `cell_loader/load_order.rs` does `std::fs::read` + a fresh rayon walk + `merge_from` on every call.

## Description
A door transition is dispatched by `step_cell_transition`. Interior destination: `unload_current_interior`, then `InteriorCellApply::begin`, whose first act is a full-load-order parse (`phases.esm_parse`). Exterior destination: `build_exterior_world_context` parses it again. The destination's `masters` / `esm_path` come from `LoadedPluginSet`, so within a session they are always the plugin set that built the `Arc<EsmIndex>` already installed as `LoadedCellIndex` (and held in the streaming `wctx.record_index` for exterior sessions). The code already knows a parse is expensive: `app_step.rs` avoids "a second ESM parse" for the preserved persistent root, but the one remaining parse per door is not avoided. The skill's Dim 7 text describes the interior door path as budgeted; the `begin` half is not (the `FrameTimeBudget` only governs `advance`).

## Evidence
```rust
// load.rs, InteriorCellApplyJob::begin, run once per door
let phase_started = Instant::now();
let (index, load_order) = parse_record_indexes_in_load_order(&plugin_paths)?;
phases.esm_parse = phase_started.elapsed();
```
Quoted costs (not re-measured by the audit): `AUDIT_FNV_2026-10-18.md` - FNV interior load `esm_parse 1.54 s, references 0.41 s, finalization 0.07 s, total 2.01 s` (parse is 77 % of the load); FO4 + 7 DLC 2.85 s, Starfield 14 plugins 7.72 s (2026-09-28 parallel-walk measurements). Disproof attempted: no parsed-index cache in `parse_record_indexes_in_load_order`; the loading screen hides the frame, not the block (the parse runs inline in `step_cell_transition`, so no frame is presented and no window events are pumped); the plugin set cannot differ within a session; `LoadedCellIndex` is an immutable `Arc` and `InteriorCellApplyJob` only moves the index into it at `finish`.

## Impact
Per door: FNV/FO3/Oblivion ~1.5 s main-thread block; FO4+DLC ~3 s; Starfield ~8 s. The previous index stays alive in `LoadedCellIndex` while the new one parses, so the transient peak holds two indexes (`memory-budget.md` "ESM Index" prices one). Confidence: high on structure, medium on magnitude per game (quoted, not re-run).

## Related
#3671 (budgeted interior REFR phase), #5288 (archives opened twice per door), #3415, #5715 (also a main-thread hitch in the same step).

## Suggested Fix
When the destination's `(masters, esm_path)` equals the resident `LoadedPluginSet`, pass the existing `Arc<EsmIndex>` + `LoadOrder` into `InteriorCellApplyJob::begin` / `build_exterior_world_context` instead of parsing; fall back to the parse when the sets differ. Pin it with a test that a second interior transition performs zero `parse_record_indexes_in_load_order` calls.

Source: docs/audits/AUDIT_PERFORMANCE_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
