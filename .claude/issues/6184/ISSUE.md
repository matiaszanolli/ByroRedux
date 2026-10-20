# TD9-2026-10-19-03: ~35 real-data test files re-inline `/mnt/data/SteamLibrary/...` defaults and skip silently, bypassing `test_paths` and `BYROREDUX_REQUIRE_GAME_DATA` — including the #5489 and #5005 pins

Labels: medium,test-gap,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: MEDIUM (guards closed-HIGH #5489 and #5005; also Dim 7 hardcoded constant)
- **Dimension**: 9 - Test Hygiene (and 7: an environment-specific path literal copied 127 times)
- **Location**: `byroredux/src/cell_loader/pkin_expansion_tests.rs:579-585` (Starfield; reads `BYROREDUX_STARFIELD_DATA` or `/mnt/data/SteamLibrary/...`, `eprintln!("skipping")`, `return`), `byroredux/src/cell_loader/reference_state.rs:926-947` (FO3/FNV `continue`), `byroredux/src/cell_loader/load_order.rs:1220-1230`, `byroredux/src/cell_loader/legacy_lod_index_tests.rs:35-39,91-99,127-134`, `crates/nif/tests/ragdoll_import.rs:23-33,139-146,179-186,202-212,221-230`, `crates/pex/tests/r5_fidelity.rs:155`, `byroredux/src/asset_provider/texture.rs:1136-1143`, `byroredux/src/inventory.rs` (6 literals), `byroredux/src/save_io/consumable_tests.rs` (4), `crates/audio/src/tests.rs` (7), and about 25 more
- **Age**: structural; `pkin_expansion_tests.rs` 2026-10-14 (#5489 pin), `legacy_lod_index_tests.rs` / `ragdoll_import.rs` touched 2026-10-19
- **Effort**: small (mechanical)
- **Description**: `crates/plugin/src/esm/test_paths.rs` (`pub mod`, `data_dir(env, default)` + the per-game `*_data_dir()/*_esm()`) already centralises the default install paths and makes a set override binding (#3850); `crates/hkx`, `crates/nif/tests/common`, `crates/menuxml/tests/common` and `crates/scripting/tests/common` add the `BYROREDUX_REQUIRE_GAME_DATA` fail-instead-of-skip. `grep -rn /mnt/data/SteamLibrary` finds 127 occurrences in 68 files outside `test_paths.rs` (35 of them test files, excluding examples/game-detect/boot). These tests carry their own default, a typo'd `BYROREDUX_*_DATA` override silently skips, and an absent install reports `ok` even in the strict lane (#3850's exact false green, re-grown after #5784/#5855 fixed three sites).
- **Evidence**: `let Ok(bytes) = std::fs::read(&path) else { eprintln!("[Starfield PKIN] skipping: {path:?} unavailable"); return; };` over a hardcoded `unwrap_or_else(|_| "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data".into())`.
- **Impact**: moot until a lane runs them (TD9-01), then a false green on exactly the tests the lane exists for; the hardcoded path is also the dev machine's layout.
- **Existing**: #5784 and #5855 (both closed, three sites each), #3850 (closed).
- **Suggested Fix**: add `test_paths::require_or_skip(what)` (panic when `BYROREDUX_REQUIRE_GAME_DATA` is set, else eprintln) beside `data_dir`, and convert these sites to `test_paths::{fnv,fo3,skyrim_se,starfield}_data_dir()` + that helper; start with the five files above (the two closed-HIGH pins). A one-line source scan in `workspace_hygiene_tests` ("no `/mnt/data/SteamLibrary` in a test file outside `test_paths.rs`") keeps it closed.

Related: #5784, #5855, #3850 (closed).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
