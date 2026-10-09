# #5450: PERF-D7-2026-10-08-01: #5248's script-killed corpse set scans every placed reference in the load order on the main thread at the first reference apply, for all games

**Labels**: low,performance,esm-plugin,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5450

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-08.md` — `PERF-D7-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/reference_state.rs:554-575` (`script_killed_corpse_forms_for_load_order`), `:493-523` (`script_killed_corpse_forms`); `byroredux/src/cell_loader/references/mod.rs:344-357` (the lazy first-call build)
- **Status**: NEW. Arrived with `d4e8c31be` (#5248, 2026-10-07).
- **Description**: the set is built lazily the first time `load_references_budgeted` runs. It iterates every interior cell, every exterior grid cell and every worldspace-persistent cell in the whole `EsmIndex`. Per placement:
  - pass 1: a `linked_refs` check plus a std-HashMap base-script lookup when links exist;
  - pass 2: a `killed.contains` and an `index.npcs.get(&base_form_id)` SipHash lookup, for every placement including statics.
  Only FO3 and FNV can ever match, because `script_is_vanilla` accepts only the twelve names in `VANILLA_KILL_SCRIPT_PLUGINS`. On Oblivion, Skyrim, FO4, FO76 and Starfield the result is always empty, but the scan still covers every placement. Before #5248 the work was proportional to one cell's refs.
- **Evidence**: no `index.game` gate at `references/mod.rs:344-357`; the scan sits on the main thread (`world.insert_resource`) in the first apply.
- **Impact**: a one-time O(total placed references in the load order) pass of hash lookups at first-cell load. The count is large on the big masters, but the pass is one-shot and boot is unbudgeted by design, so LOW. It adds directly to time-to-first-frame on the five games where it cannot match. Unmeasured; no quantitative guard exists.
- **Related**: #5248, #5223, #5304.
- **Suggested Fix**: gate on `record_index.game` (FO3/FNV only), and/or compute it on a worker overlapped with the ESM parse. Alternatively fold the recognizer into the existing ESM index walk so no second pass over placements is needed.

## Completeness Checks
- [ ] **SIBLING**: Other lazily-built whole-load-order resources in `cell_loader/` checked for the same missing game gate
- [ ] **TESTS**: A regression test pins this specific fix
