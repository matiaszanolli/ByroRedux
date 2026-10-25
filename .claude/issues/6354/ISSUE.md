# TD9-2026-10-25-01: five closed-HIGH real-data guards are in `real-data-pins.txt`'s blind spot, and four of them run in no lane at all

Labels: medium, test-gap, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6354
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 9 (label `test-gap`) - **Effort**: small - **Age**: guards landed 2026-09-29 .. 2026-10-19; none are in the 4a3e89d56..HEAD window
- **Location**: `.github/real-data-pins.txt`; `scripts/run-real-data-pins.sh:70-75` (lane map); `byroredux/src/workspace_hygiene_tests.rs:1053` (`real_data_pin_lane_sources`)
- **What**: each test below is `#[ignore]`d, cites a closed `high` issue in its own doc comment, and is absent from the pin list (`grep -c <fn> .github/real-data-pins.txt` = 0):

  | fn | file:line | closed-HIGH | other lane today |
  |---|---|---|---|
  | `fnv_patch04_quest_script_targets_its_own_scro_quests` | `crates/scripting/src/obscript_quests.rs:1715` | #5629 (ObScript refs resolve against SCRO-only list) | none. Its own doc says the sibling gate "was blind to references resolving against the SCRO-only list", so this is the only discriminating test |
  | `authored_actor_value_indices_resolve_in_the_vanilla_masters` | `crates/scripting/src/condition/tests.rs:2073` | #5989 (GetActorValue enum code read as FormID) | none |
  | `oblivion_starts_dead_bases_and_their_placements_match_the_audit_census` | `crates/plugin/tests/parse_real_esm.rs:5221` | #5013 (787 Oblivion corpses spawn alive) | none (the plugin `--ignored` suite is excluded from the `parsers` job, `real-data-gates.yml:242-246`) |
  | `installed_starfield_npcs_and_races_decode_acbs_and_skeletons` | `crates/plugin/src/esm/records/actor/tests.rs:2614` | #6120 (Starfield NPC_ ACBS matches no arm) | none |
  | `fo3_corpses_key_on_base_health_not_xrgd` | `crates/plugin/tests/parse_real_esm.rs:5416` | #5005 (FO3 leg; the FNV twin `fnv_corpses_key_on_base_health_not_xrgd` IS pinned, the doc calls this "the FO3 twin") | none |

  Also in the same class but already covered by a manual lane (the `parsers` job runs the whole hkx crate `--ignored`):
  `fallout_4_behavior_graph_census_is_exact` (`crates/hkx/src/corpus_tests.rs:135`, #6211, re-measured by 7e2f21482/fbccdcbff).
  Pinning it is optional; no lane gap.
- **Why it was missed**: #6224 swept only its audit window. These are older guards; the #6224/`/fix-issue` PINS line only
  catches fixes landing after it. Window check for this audit: of the 11 commits that added/changed an `#[ignore]` since
  4a3e89d56, none guards a closed-HIGH (#6089, #6090, #5753, #5750, #6329, #6252, #6251, #6241, #6350-refs are medium/low/open;
  `skeever_ragdoll_survives_six_hundred_floor_ticks`, #6185, is already pinned). The 9 closed-HIGH fixes in the window
  (#6343, #6214, #6213, #6212, #6211, #6210, #6209, #6208, #6207) all carry hosted non-ignored tests.
- **Fix**: append `plugin-esm oblivion_starts_dead_bases_and_their_placements_match_the_audit_census`, `plugin-esm fo3_corpses_key_on_base_health_not_xrgd`,
  `plugin-lib installed_starfield_npcs_and_races_decode_acbs_and_skeletons` (3.4 GB peak, one process per pin is already the
  runner's bound). The two scripting guards need a new lane: add `scripting-lib) target="-p byroredux-scripting --lib"` to the
  `case` at `run-real-data-pins.sh:70-75` and the matching `add_dir("crates/scripting/src")` arm in
  `real_data_pin_lane_sources` (workspace_hygiene_tests.rs:1058-1062) so `every_real_data_pin_names_an_ignored_test_in_its_lane`
  keeps checking them; then add the two `scripting-lib` rows. Verify with `scripts/run-real-data-pins.sh --list`.
- **Dedup**: precedent #6224 (closed; different four rows), #6183 (closed), #6182/#5905 (closed, no runner - the local run is the only run).


### LOW (by dimension)


#### Dimension 1 — Complexity (6)

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD9-2026-10-25-01`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
