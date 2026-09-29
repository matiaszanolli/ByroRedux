# #5060 — SAVE-D5-2026-09-29-03: `restore_resident` (#4695) and `reseat_ambient_packages_after_restore` (#4815) are order-critical load steps with no source-order pin

**Labels**: low,save-load,test-gap,bug

**Source report**: `docs/audits/AUDIT_SAVE_2026-09-29.md`
**Severity**: LOW
**Dimension**: Live Load-Apply & Frame Boundary — data-loss class: none (test gap)

## Location
- `byroredux/src/save_io.rs` — load drain: `restore_resources` → `restore_resident` → `reseat_ambient_packages_after_restore` → `build_form_id_remap` → `apply_deltas`
- `byroredux/src/save_io/live_reload_tests.rs` — `saved_resources_are_restored_before_the_cell_reload`

## Description
- If `restore_resident` moved above the post-reload `restore_resources`, it would be a silent no-op (store absent), re-opening #4695's item duplication.
- `reseat_ambient_packages_after_restore` must precede `apply_deltas` (#4815).
- Only unit tests of the two functions exist (`reference_state.rs`, `ai_package.rs`). The existing source-order pin covers only the pre-reload subset and the post-reload `restore_resources`.

## Evidence
`grep -rn 'restore_resident\|reseat_ambient' byroredux/src/save_io/` finds only a comment in `round_trip_tests.rs` — no order assertion.

## Impact
A refactor that reorders the drain silently reintroduces #4695 / #4815 with green tests.

## Related
#4695, #4815; SAVE-D5-2026-09-29-01 (#5054).

## Suggested Fix
Extend `saved_resources_are_restored_before_the_cell_reload` (or add a sibling) to assert, inside the drain body via `source_scan`-style production text: `restore_resources` < `restore_resident` < `reseat_ambient_packages_after_restore` < `build_form_id_remap` < `apply_deltas`.

Validated at HEAD 9fcfdc3fc: no test under `byroredux/src/save_io/` references `restore_resident` or asserts `reseat_ambient_packages_after_restore` ordering.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
