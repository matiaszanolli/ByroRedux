# #5070: FO3-D3-01: ROADMAP's FO3 Megaton repro command runs 0 tests, and the record-count row still cites the retired 44 657

**Labels**: documentation, low, legacy-compat, game:fo3, esm-plugin, doc-rot, test-gap

**Source report**: `docs/audits/AUDIT_FO3_2026-09-29.md`
**Severity**: LOW
**Dimension**: Cell Loading (validation / doc-rot)

## Location
- `ROADMAP.md` "How to run" table: the "Megaton interior parse-side 929 REFRs" row and the "Full ESM record counts" row.
- The test lives at `crates/plugin/src/esm/cell/tests/integration.rs` (`parse_real_fo3_megaton_cell_baseline`), not in `crates/plugin/tests/parse_real_esm.rs`.

## Description
- **Megaton row.** It backs "Megaton interior parse-side 929 REFRs" with `cargo test -p byroredux-plugin --release --test parse_real_esm parse_real_fo3_megaton_cell_baseline -- --ignored`. The test is in the plugin lib, so the filter matches nothing in `parse_real_esm` and the command passes green having run 0 tests.
- **Record-count row.** It states "FO3 44 657 = 37 459 structured + 7 198 NAVMs" — the figure #3756 retired (an index-sum, not a record count). Live `index.total()` is 44 718; the file holds 718 952 records. The FNV half ("77 825") is likewise stale (live 78,575).
- That row's command is the whole-file `--test parse_real_esm -- --ignored` run, which `/audit-fo3` warns can spike >20 GB RSS.

## Evidence
The Megaton command, run verbatim at HEAD, prints `running 0 tests` and `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 35 filtered out`. The working command, `cargo test -p byroredux-plugin --release --lib -- --ignored --exact esm::cell::tests::integration::parse_real_fo3_megaton_cell_baseline`, prints `MegatonPlayerHouse: 929 REFRs`.

Validated at HEAD 9fcfdc3fc: both ROADMAP rows are unchanged; `grep -n megaton crates/plugin/tests/parse_real_esm.rs` is empty and the test fn is at `crates/plugin/src/esm/cell/tests/integration.rs`.

## Impact
Anyone re-verifying the Megaton claim from ROADMAP gets a green run that checked nothing. The count row asserts a retired number and points at a command that can OOM the machine.

## Related
- #3756 (CLOSED; corrected the test comment only).

## Suggested Fix
- Point the Megaton row at the `--lib … --ignored --exact` command.
- Restate the record-count row with the #3756 floors (`index.total()` ≥ 44 000, placed refs ≥ 573 000, exterior cells ≥ 41 900) and one named `--exact` test per game instead of the whole-file run.

## Completeness Checks
- [ ] **SIBLING**: Other ROADMAP repro commands checked for the same `--test` vs `--lib` mismatch
- [ ] **TESTS**: Each ROADMAP repro command actually runs ≥ 1 test

