# #7304: SCR-D3-05: `pattern_b_markers_survive_event_cleanup` is a hand-kept list that omits `ReleasePackageHold`

- **Severity**: LOW (test gap)
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/cleanup.rs:286-355`
- **Status**: NEW. #7264 added the drain half for this row, not the survive half.
- **Untrusted-Input**: No
- **Description**: The table has 21 markers and `survives(..)` has 20 calls; `ReleasePackageHold` is missing. If someone adds it to `event_cleanup_system`, the new marker is wiped before `ambient_ai_package_system` runs. `every_pattern_b_table_row_has_a_drain_test` (`request_drain_tests.rs`) matches `drains("X"` only, so it does not enforce the survive half.
- **Suggested Fix**: Add `survives("ReleasePackageHold", byroredux_scripting::package::ReleasePackageHold)`, or extend the table-driven check to require a matching `survives(` call.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
