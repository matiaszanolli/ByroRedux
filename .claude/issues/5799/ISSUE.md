# TD9-2026-10-15-01: About 218 of the 322 ignored tests run in no automated lane, and at least 45 of them pin fixes for closed HIGH/CRITICAL issues

Labels: medium,test-gap,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD9-2026-10-15-01`).

- **Severity**: MEDIUM (severity table: `#[ignore]`d test that guards a fix from a closed CRITICAL/HIGH issue)
- **Dimension**: 9 - Test Hygiene
- **Location**: `.github/workflows/real-data-gates.yml:139-146,177-231,244-246` (what the nightly covers) vs the ignored tests in `crates/plugin` (92), `byroredux` bin (78, excluding `tests/{golden_frames,cornell_rt_oracle,upscaler_quality}.rs`), `crates/scripting` (16), `crates/nif` (13 outside the three corpus binaries), `crates/audio` (9), `crates/spt` (4), `crates/ui` (3), `crates/pex`, `crates/renderer` (1 each)
- **Status**: NEW (related open: #5784; #4492 / #4619 closed the same shape for the exterior harnesses and the NIF corpus)
- **Age**: structural; the +23 of this window are all of this class
- **Effort**: medium
- **Description**: the nightly lane runs `byroredux-nif` `parse_real_nifs` / `per_block_baselines` / `block_coverage_baselines`, the `bsa`, `bgsm`, `sfmaterial`, `hkx`, `facegen`, `menuxml` suites, and one named `byroredux-scripting` test; `rt-correctness.yml` (manual dispatch) runs cornell, golden and the FSR preset gate. Everything else ignored is reachable only by a hand-run `--ignored` or by an audit skill's instructions. The workflow comment at `:232-233` says the `plugin` suites stay out deliberately (memory: one `parse_real_esm.rs` run spikes past 20 GB), and no scoped alternative was added. Tests in the uncovered set that cite a closed HIGH/CRITICAL issue in their doc comment or body (token scan of the 60 lines after each `#[ignore]` against `gh issue list --label critical|high --state all`): 45, among them `fnv_corpses_key_on_base_health_not_xrgd`, `fo4_starts_unconscious_and_xrgd_poses_match_the_census`, `skyrim_xrgd_poses_match_the_census_and_nothing_is_unconscious`, `fo3_fnv_script_killed_corpses_match_the_measured_census` (#5005), `race_skeletons_match_authored_paths_on_real_skyrim_data` (#5487), `starfield_cydonia_pack_ins_instance_their_template_cells` (#5489), `exterior_precombine_instances_are_world_absolute` (#5228), `distant_water_mesh_is_per_cell_on_real_fnv` (#5243), `fnv_actor_value_roster_and_health_resolve_on_shipped_master` and `fo3_fnv_named_npc_health_matches_the_authored_curve` (#5238), the three `*_ruleset_resolves_against_the_real_master` (#2986), `parse_rate_oblivion_esm` (#533, CRITICAL) and `cross_game_translation_completeness` (#1277/#1292, named in `audit-nifal` and `audit-renderer`).
- **Evidence**: `/tmp/audit/tech-debt/d9_ignguard.py` (lane map: `covered` prefixes from the workflow files; 103 covered, 219 not, one of the 219 being the scripting test the nightly does run). Caveat: an issue number in a test's doc is a pointer to the fix, not proof the test is its only guard; the 45 is an upper bound on "pins a closed HIGH fix".
- **Impact**: the last two weeks fixed #5005, #5228, #5238, #5243, #5487, #5489 and added their real-data pins; each of those regressions would be found only when an audit happens to run the right `--ignored` filter by hand on a machine with the game installed.
- **Related**: #5784, #4492, #4619, #3850 (`BYROREDUX_REQUIRE_GAME_DATA`), `plugin_ignored_tests_oom` (memory).
- **Suggested Fix**: add a scoped step to `real-data-gates.yml` that runs the plugin/bin/scripting/audio real-data tests per game in separate processes (`--test-threads=1`, `-- --ignored <filter>` per title, `BYROREDUX_REQUIRE_GAME_DATA=1`, with the existing "refuse a green no-op" count check); that removes the OOM objection because each process loads one master. Start with the 45 closed-HIGH pins.


## LOW Findings

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
