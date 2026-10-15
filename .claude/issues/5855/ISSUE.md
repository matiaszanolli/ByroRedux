# TD9-2026-10-15-03: Three scripting real-data test files skip green when game data is missing, ignoring `BYROREDUX_REQUIRE_GAME_DATA`

Labels: low,test-gap,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD9-2026-10-15-03`).

- **Severity**: LOW
- **Dimension**: 9 - Test Hygiene
- **Location**: `crates/scripting/tests/perk_entry_points_real_data.rs:45-48,103-106,129-132` (`SKIP: FalloutNV.esm not found`), `crates/scripting/tests/location_aliases_real_data.rs:38-41,100-103` (`skipping: game data unavailable`), `crates/scripting/tests/pex_recognize_e2e.rs:40-42,85-87,127-129,152-154` (`SKIP: ... no game data?`)
- **Status**: NEW (sibling of open #5784, which is the same shape in `npc_spawn/tests.rs`)
- **Age**: 6 of the 9 `#[ignore]` tests in these files landed in this window (`perk_entry_points_real_data` x3, `location_aliases_real_data` x2, `pex_recognize_e2e` x1)
- **Effort**: trivial
- **Description**: #3850 made an absent install fail instead of skip when `BYROREDUX_REQUIRE_GAME_DATA` is set (helpers in `crates/plugin/src/esm/test_paths.rs`, `crates/scripting/src/obscript_quests.rs`, `byroredux/src/ragdoll_pose_tests.rs:254`, `cell_loader/tree_lod_tests.rs:263`). These three files consult only `test_paths::*_ENV`/`*_DEFAULT` (or an archive lookup) and `return` on `None`, so an `--ignored` run in a strict lane reports `ok` without having asserted anything.
- **Evidence**: `let Some(index) = fnv_index() else { eprintln!("SKIP: ..."); return; };`
- **Impact**: moot while the tests have no lane (TD9-01), and a false green the moment one is added.
- **Related**: #5784, #3850, TD9-2026-10-15-01.
- **Suggested Fix**: one shared `require_or_skip(path, what)` in `crates/scripting/tests/common` (panic when `BYROREDUX_REQUIRE_GAME_DATA` is set) and call it from the three helpers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
