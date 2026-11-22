# #7264: Pattern-B drain-pin test cannot run binary-crate consumers; ReleasePackageHold's row is unguarded by the table test

**Severity**: LOW · **Found by**: /audit-sync 2026-11-22 (scripting group)

## Description
`crates/scripting/src/cleanup.rs`'s Pattern-B table documents each self-draining marker and its consumer, and `every_documented_pattern_b_marker_drains_in_its_named_consumer` proves the drain behaviourally — but only for consumers inside `byroredux-scripting`. A marker whose consumer lives in the binary crate cannot be run there. `ReleasePackageHold` (#7243) is such a row: its consumer is `byroredux::npc_spawn::ai_package::ambient_ai_package_system`, and its drain is asserted only by `an_explicit_evaluate_package_ends_a_must_complete_hold` (`byroredux/src/npc_spawn/ai_package.rs`). `EvaluatePackageRequest`, also drained by that system, is tested only against `scene_package_system`. Nothing ties the doc table's bin-crate rows to a test, so a future row can be added with no drain check at all.

## Impact
Test gap: the documented contract for bin-crate Pattern-B consumers is checked per-marker by hand, not by the table guard (#2672's rot class).

## Suggested Fix
Add a bin-crate counterpart (e.g. beside `byroredux/src/systems/request_drain_tests.rs`) that runs each bin-crate consumer the `cleanup.rs` table names, and a check that every table row is covered by one of the two tests.

## Completeness Checks
- [ ] **TESTS**: every Pattern-B row in the `cleanup.rs` table is exercised by a drain test in one crate or the other
