# TD9-2026-10-25-03: Regression of #6184: #6184 class re-grown inside the allowlist: `animation.rs` hard-codes the Steam root and silently skips four times

Labels: low, test-gap, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6401
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 9 (`test-gap`) - **Effort**: small - **Age**: new site 051179b7b (#6241, 2026-10-23); other three older
- **Location**: `byroredux/src/asset_provider/animation.rs:768-775` (`fallout_4_turn_in_place_clip_carries_its_turn_as_root_yaw`, new),
  `:1262`, `:1353`, `:1425` (three Skyrim tests)
- **What**: all four resolve `BYROREDUX_*_DATA` with a literal `/mnt/data/SteamLibrary/...` fallback and, on a missing archive, `return`
  (or `let Ok(..) else { return }`) with no `require_game_data` / `test_paths::require_or_skip`. Under `BYROREDUX_REQUIRE_GAME_DATA=1`
  they report `ok` for an absent install: the #3850 false green. `no_test_region_hardcodes_the_dev_machine_game_path`
  (workspace_hygiene_tests.rs:1370) cannot see it because the ratchet is **per file** and `animation.rs` is on
  `HARDCODED_GAME_PATH_ALLOWLIST`; since base, 4 more literals were added to allowlisted files (animation.rs +1, npc_spawn/tests.rs +1,
  ragdoll_pose_tests.rs +1; of these only animation.rs's skips silently, the other two assert under REQUIRE).
- **Fix**: use `test_paths::fnv_data_dir()`-style helpers (`fo4_data_dir()`, `skyrim_se_data_dir()` exist in `crates/plugin/src/esm/test_paths.rs`)
  and `require_or_skip` in the four tests, then drop `animation.rs` from the allowlist (the guard's `stale` arm forces that). Longer term make
  the ratchet per-occurrence (count per file) so allowlisted files cannot grow.
- **Dedup**: Existing #6184 (closed 3e4bfab51; the ratchet is its remedy). This is a regression of the class, not of the ratchet.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD9-2026-10-25-03`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
