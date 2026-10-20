# TD9-2026-10-19-02: The `closed-high-pins` list drifted from the code: two entries name the wrong cargo lane (red on arrival), three named closed-HIGH pins are absent, and nothing on a hosted runner checks the list

Labels: medium,test-gap,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: MEDIUM (closed-HIGH pins #5489 / #5005 / #5238 unlisted; the check is a hosted-runnable gap)
- **Dimension**: 9 - Test Hygiene
- **Location**: `.github/workflows/real-data-gates.yml:367-368` (`"bin dark_texture_role_population_matches_the_documented_census"`, `"bin real_archive_torch_meshes_surface_particle_emitters"`) vs `crates/nif/tests/translation_completeness.rs:975` and `crates/nif/tests/parse_real_nifs.rs:675`; absent: `byroredux/src/cell_loader/pkin_expansion_tests.rs:579` (`starfield_cydonia_pack_ins_instance_their_template_cells`, #5489), `byroredux/src/cell_loader/reference_state.rs:910` (`fo3_fnv_script_killed_corpses_match_the_measured_census`, #5005), `crates/plugin/tests/parse_real_esm.rs:360` (`fo3_fnv_named_npc_health_matches_the_authored_curve`, #5238)
- **Age**: `ceca6eaa8` (2026-10-15, #5799) — the 2 wrong rows were added by the #5784 follow-up in the same commit family
- **Effort**: trivial
- **Description**: the `bin` lane runs `cargo test -p byroredux --bin byroredux <title> -- --ignored`; the two `bin` rows above live in `byroredux-nif` integration tests (`translation_completeness`, `parse_real_nifs`), so on a runner they match nothing and the lane's own "matched nothing (green no-op)" check turns the job red permanently. The `nif-corpus` target list also omits `--test parse_real_nifs` (needed for the torch test). #5799's finding named three closed-HIGH pins (#5489, #5005, #5238 cases above); they are not in the 50-row array (`grep -c <name> real-data-gates.yml` = 0 for each). The line `if: github.event_name == 'schedule' || ...` on the job (`:283`) refers to a trigger the file no longer has. I verified the other 48 rows: each `fn` exists, is `#[ignore]`d, and sits in the cargo target its lane names.
- **Evidence**: `grep -rn "fn dark_texture_role_population_matches_the_documented_census" crates byroredux` -> `crates/nif/tests/translation_completeness.rs:975` only.
- **Impact**: the first run on a registered runner would fail on two stale rows while 3 HIGH pins still go unrun; and because the YAML cannot execute today nobody sees the drift.
- **Suggested Fix**: relabel the two rows `nif-corpus` and add `--test parse_real_nifs` to that lane; append the three missing pins; add a hosted-runnable test in `byroredux/src/workspace_hygiene_tests.rs` that parses the pins list (or the data file from TD9-01) and asserts each title is a `fn` carrying `#[ignore]` inside the source tree its lane names (same style as `every_ignore_attribute_carries_a_reason`); delete the dead `schedule` clause.

Related: #5799 (closed).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
