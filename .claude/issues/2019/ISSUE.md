# SAVE-D6-04: build_form_id_remap silently drops deltas for a saved FormIdPair no longer present in the reloaded cell

**Labels**: medium, ecs, bug

**Severity**: MEDIUM
**Dimension**: M45.1 Live Load-Apply
**Source**: `docs/audits/AUDIT_SAVE_2026-07-16.md`

## Location
`crates/save/src/driver.rs:143-178` (`build_form_id_remap`)

## Description
A saved `FormIdPair` that doesn't resolve in the reloaded cell (record removed from a plugin, cell content changed between save and load) is silently absent from the remap with zero logging. Every `MUTABLE_DELTA_COLUMNS` row keyed to that entity is then dropped by `ApplyFn`'s `filter_map`, equally silently — `apply_deltas`'s applied-count just comes back smaller, logged only as a bare aggregate number with no per-entity detail. The function's doc comment covers the "no form id at save time" case but not this one.

Verified current: `build_form_id_remap`'s final `filter_map(|(old, pair)| pair_to_live.get(&pair).map(|&live| (old, live)))` silently drops any saved pair with no live match — no `log::warn!` call in this path.

## Impact
A saved moved/customized object silently reverts to its ESM-authored defaults on live load with no trace in the log — undiagnosable without manually diffing the saved `FormIdPair` list against the reloaded cell. Low blast radius (plugin/cell content drift between a save and its later load isn't the common case). Arguably correct behavior (there's no valid target to apply the delta to), which is why this is MEDIUM (diagnosability gap) rather than HIGH (correctness bug).

## Related
Distinct from closed `#1847` (opposite direction — removed objects *reappearing*, not moved-object deltas silently failing to apply).

## Suggested Fix
Log the count (and, bounded, the identities) of saved rows that fail to resolve — mirroring the `log::warn!` already present in the same file's `FormIdComponent` save closure for the symmetric case.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
