# #4389 — TD9-001: #3848 wired the Skyrim ruleset, so the ignored CHARAL real-data gate now panics on Skyrim and never reaches the Oblivion case

**Labels**: low, character, game:skyrim, tech-debt, bug, test-gap
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4389

- **Severity**: LOW · **Dimension**: 9
- **Location**: `crates/plugin/tests/parse_real_esm.rs:339` (`derived_rows: None`), `:509-518` (`None =>` arm asserting `ruleset.is_none()`), `:524` · **Status**: NEW · **Age**: `e13985dfc` (09-12) · **Effort**: trivial · **Kind**: test-gap · **Related**: TD3-007
- **Finding**: `crates/core/src/character/profile.rs:140,203` now always builds `Some(skyrim_ruleset(..))`; the non-ignored `skyrim_profile_builds_a_ruleset_and_actually_calls_gmst` already `.expect`s it. The expected value is measured at `crates/plugin/src/esm/records/tests.rs:153` (`derived_row_len() == 2`). Not promoted: #3848 was `medium`. Not run (plugin `--ignored` OOM rule); concluded from the code path.
- **Suggested Fix**: `derived_rows: Some(2)`, re-measured once with the GMST overlay active.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
