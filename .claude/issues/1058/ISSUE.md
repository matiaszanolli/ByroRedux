# Tech-Debt: 25 hardcoded Steam paths in #[ignore]'d integration tests — extract STEAM_DATA_ROOT helper

**Labels**: low, tech-debt
**Status**: Open

## Description

25 ignored integration tests across `crates/plugin/`, `crates/nif/`, `byroredux/` hardcode \`/mnt/data/SteamLibrary/steamapps/common/<Game>/Data/\` paths. Today only the audit author can run them; CI doesn't, no test ever rotates against a different layout, and the path-string copy-paste was the actual source of [#1041](https://github.com/matiaszanolli/ByroRedux/issues/1041)'s "FNV.esm not found" misleading error in the FO3 baseline test.

Concrete sites (`crates/plugin/src/esm/cell/tests/integration.rs`, the post-Session-36 split target):
- `:13` FNV
- `:85` Oblivion
- `:171` Oblivion
- `:226` FO3
- `:255` Skyrim SE
- `:342` FO4
- `:393` FO4 (PKIN)
- `:435` Oblivion
- `:487` FO3
+ ~16 more across `byroredux/tests/skinning_e2e.rs`, `byroredux/tests/golden_frames.rs`, `crates/nif/tests/parse_real_nifs.rs`, `crates/nif/tests/mtidle_motion_diagnostic.rs`, the per-block-baselines integration test, and the SF smoke harness.

## Severity rationale

**LOW** (default tech-debt). No amplification trigger fires today. The 2026-05-14 audit Dim 6 flagged it as part of "hardcoded resource paths," which would be a HIGH if the resource binding silently overflowed; here it's just developer convenience friction. Promote to MEDIUM **only if** a CI runner gets configured with different data paths, at which point every one of these 25 tests breaks in lockstep.

## Proposed fix

Add a single helper to `crates/plugin/src/esm/cell/tests/mod.rs` (visible to every sibling via `pub(super)`):

\`\`\`rust
/// Resolve a game-data path through the BYRO_GAME_DATA env var when set,
/// falling back to the audit author's Steam layout for unconfigured local
/// runs. Tests that need a real-disk ESM call \`fnv_data().join(\"FalloutNV.esm\")\`
/// etc.
pub(super) fn fnv_data() -> std::path::PathBuf { /* … */ }
pub(super) fn oblivion_data() -> std::path::PathBuf { /* … */ }
// + skyrim_se_data, fo3_data, fo4_data, fo76_data, starfield_data
\`\`\`

Roll out incrementally — 9 sites in the cell-tests directory are the most concentrated cluster; sweep them first, then propagate the helper module to the other 5 test entry points via a shared crate-local utility.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: 25 sites across 6 test files — track via a checkbox per file in the PR description
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: tests must continue to pass with no env-var set (default Steam path matches today's behaviour)

## Dedup notes

Distinct from **#1050** (open — test hygiene batch). This is the focused actionable subset that the audit Quick Wins called out separately; #1050 stays as the umbrella but the actual fix lives here.
Status: Closed (94a9011)
