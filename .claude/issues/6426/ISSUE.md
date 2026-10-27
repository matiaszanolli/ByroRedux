# CHAR-2026-10-26-L-04: Test gaps: the `crime` and `radiation_gauge` rows, `install_crime_model`, the `build_vats_model` FO3 leg, and the `plugin_files` wiring

- **Severity**: LOW
- **Dimension**: Ruleset Seam / Coverage
- **Game**: all (oblivion and skyrim for crime; fo3 and fnv for the rest)
- **Location**: `profile.rs` (`crime` rows, `radiation_gauge` rows; test `per_game_rule_rows_are_stated_on_every_profile` covers only the #5921 columns); `byroredux/src/crime.rs:59-67`; `byroredux/src/inventory.rs:384-391`; `cell_loader/load_order.rs:908`
- **Status**: NEW
- **Description**:
  - Nothing asserts that only Oblivion and Skyrim carry a `CrimeModel`, or that exactly FO3 and FNV carry `("RAD","RadiationRads")`.
  - `install_crime_model` (profile → `with_gmst` overlay → resource, and `remove_resource` on `None`) is exercised by no test. Crime tests hand-build the resource.
  - `install_catalog_resolves_per_game_vital_keys` asserts only `["HP","AP"]`.
  - Nothing pins `merged.plugin_files = load_order.clone()` (`load_order.rs:908`), the one line carrying the #6227 fix. Deleting it leaves every test green and silently returns FNV to a cap of 30.
- **Impact**: Swapping the Oblivion and Skyrim crime rows, dropping the rad gauge, leaving a stale `CrimeModel` after a `None` profile, or regressing #6227 would pass every test.
- **Suggested Fix**: Add `crime` and `radiation_gauge` columns to the row pin. Add one `install_crime_model` test (installs plus overlay on Oblivion, clears on FO3/FNV/FO4). Assert the FNV `RAD` entry in the catalog test. Assert `index.plugin_files == order` in `load_order_parallel_tests`.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-04`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

