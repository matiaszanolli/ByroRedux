# #7250: INC-2026-11-22-08: `FoldProvenance::note_plugin` resolves the slot owner with a linear scan per record

**Labels**: low,esm-plugin,performance,bug

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-B-04**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: LOW
- **Dimension**: Performance (audit-performance Dim 7, parse cost)
- **Location**: `crates/plugin/src/esm/records/provenance.rs:46` (`owner_of(form_id)`), closure at `byroredux/src/cell_loader/load_order.rs:1008-1012` (`slots.iter().position(..)`)
- **Changed in**: both files (884627235)
- **Status**: NEW
- **Description**: `owner_of` is evaluated for every record a plugin carries that is not yet in `overridden` (it is computed before the `merged.record_types.contains_key` test), and the closure does `slots.iter().position(|owner| *owner == slot)`, which is O(plugins). The fold is serial, after the parallel walk. LMC-ID-004 requires profiles with no ceiling (more than 254 plugins must load).
- **Evidence**: the commit measures 0.02 s of provenance in a 0.48 s fold on 10 plugins. Cost scales as records x average owner position: at 250+ plugins and millions of mod records it grows to hundreds of millions of enum compares on the load thread. I did not measure it.
- **Impact**: seconds-scale worst case on very large profiles, load-time only. Not a per-frame cost.
- **Related**: LMC-ID-004, #6670.
- **Suggested Fix**: build a `HashMap<GlobalSlot, usize>` once (or index regular slots directly), and compute `owner` lazily only in the two branches that use it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
