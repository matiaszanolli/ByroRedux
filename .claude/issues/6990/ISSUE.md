# #6990: TD9-2026-11-15-01: `parse_rate_fo4_esm`, the only guard for closed-HIGH #819 (and #817's five categories), is `#[ignore]`d, in no pin lane and in no workflow

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6990
- **Labels**: medium,esm-plugin,game:fo4,test-gap,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: MEDIUM (skill trigger: an `#[ignore]`d test that guards a fix from a closed HIGH issue)
- **Dimension**: 9 (test hygiene)
- **Location**: `crates/plugin/tests/parse_real_esm.rs:3361-3363` (test); absent from `.github/real-data-pins.txt` (70 rows, none names it); the lane that would run it is `plugin-esm` (`scripts/run-real-data-pins.sh`, `-p byroredux-plugin --test parse_real_esm`)
- **Age**: closed 2026-05-04 (#819); the pin list's last additions were #6354/#6460/#6554
- **Effort**: trivial
- **Labels**: `tech-debt`, `test-gap`
- **Status**: NEW (same class as closed #6224 and #6354; those added other rows)
- **Description**: `.github/real-data-pins.txt` is the only list the strict real-data runner executes for `parse_real_esm.rs` (`real-data-gates.yml` states the plugin crate's whole-suite run stays out of the lane deliberately, because one process loading every master spikes past 20 GB). Its siblings are pinned (`parse_rate_oblivion_esm`, `parse_rate_fnv_pack_pldt_location`), but the FO4 parse-rate floor is not. The corpus job's filters (`fallout_4 fo4_all_meshes`) run only inside `-p byroredux-nif`, so they do not reach this plugin test. The test's doc comment says what it guards: "Closes #819 ... Floors specifically lock in the 5 FO4-architecture categories that #817 added to `EsmIndex::categories()` (texture_sets / scols / packins / movables / material_swaps)". #819 is labelled `high` and closed; the test is the fix. `parse_rate_fnv_esm` / `parse_rate_fo3_esm` are also unpinned, but cite no closed issue and are not claimed here.
- **Evidence**:
  ```
  parse_real_esm.rs:3361  #[test]
  parse_real_esm.rs:3362  #[ignore = "needs FO4 game data on disk"]
  parse_real_esm.rs:3363  fn parse_rate_fo4_esm() {
  grep -c parse_rate_fo4_esm .github/real-data-pins.txt   -> 0
  ```
  Weaker siblings with the same absence (each also has synthetic or other coverage, so not claimed at MEDIUM): `dlccoast_header_classifies_as_fallout4` (`parse_real_esm.rs:111`, #2904 HIGH; a synthetic `GameKind::from_header(Tes5Plus, 0.95, 131)` check exists at `reader.rs:1937`); `real_skyrim_load_order_preserves_categories_and_resolves_archive_strings` (`byroredux/src/cell_loader/load_order.rs:1388`, #2907 + #2912 HIGH; synthetic localization tests sit beside it); `lod_descriptor_tiles_the_shipped_quad_names` (`byroredux/src/cell_loader/lod_support.rs:1104`, #2586 HIGH, "reopened" on the real corpus; synthetic origin tests `:981-1085` exist).
- **Impact**: a regression that wipes an FO4 category (the #817 shape: SCOL/PKIN/MSWP/TXST maps dropping) trips nothing in CI and in no scheduled lane; it surfaces only if someone runs the ignored test by hand.
- **Related**: #819, #817, #5799, #6224, #6354, #6183 (the `every_real_data_pin_names_an_ignored_test_in_its_lane` guard would accept the new row).
- **Suggested Fix**: append `plugin-esm parse_rate_fo4_esm` to `.github/real-data-pins.txt` and run `scripts/run-real-data-pins.sh parse_rate_fo4_esm`; add the three weaker rows under `plugin-esm` / `bin` at the same time if the maintainer wants the real-corpus legs.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
