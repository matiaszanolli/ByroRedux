# #7273: INC-G-02: The Pattern-B table-coverage test matches a label string, not the marker type

- **Severity**: LOW
- **Dimension**: audit-scripting Pattern-B drains / test-gap
- **Location**: `byroredux/src/systems/request_drain_tests.rs` (`every_pattern_b_table_row_has_a_drain_test`)
- **Changed in**: c504ac64f (#7264)
- **Status**: NEW
- **Description**: The test counts a table marker as covered when a whitespace-squashed source contains `drains("<Name>"`. Several things therefore satisfy it without a real drain check:
  - a case whose label names `X` but raises a different marker type;
  - a commented-out `// drains("X", …)`;
  - `OnResetEvent`, where only the presence of a test-function name in `obscript_objects.rs` is checked;
  - a parse that loses up to one of the 21 markers, since the `>= 20` floor still passes.
- **Evidence**: `let covered = cleanup.contains(&call) || here.contains(&call) || elsewhere…`, where `call = format!("drains(\"{marker}\"")`.
- **Impact**: The guard can pass while a listed marker has no behavioural drain test. This is the rot class #2672 / #7264 meant to close.
- **Related**: #7264, #2672, #6688
- **Suggested Fix**: Have both `drains` helpers record `std::any::type_name::<T>()` for the marker they ran, and compare the table against those names. Raise the floor to the exact row count, or derive it.

**Found by**: `docs/audits/AUDIT_INCREMENTAL_2026-11-22b.md`

## Completeness Checks
- [ ] **TESTS**: the guard fails when a drains case raises a different marker type than its label
