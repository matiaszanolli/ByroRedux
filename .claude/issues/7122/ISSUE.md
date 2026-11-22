# #7122 — ESM-D8-05: 30 `#[ignore]`d plugin real-data tests return `ok` on a missing master under `BYROREDUX_REQUIRE_GAME_DATA` (regression of #6184)

Labels: low,esm-plugin,test-gap,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D8-05)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D8-05**

- **Severity**: LOW
- **Dimension**: Real-Data Validation
- **Location**: `crates/plugin/src/equip.rs`, `crates/plugin/src/esm/records/actor_value_derive.rs`, `crates/plugin/src/esm/records/tests.rs` (the `load()` helper and its five ruleset callers, plus 7 direct sites), `crates/plugin/src/esm/records/actor/tests.rs` (3 sites), `crates/plugin/src/esm/cell/tests/integration.rs` (12 sites)
- **Status**: Regression of #6184. This is the 2026-11-08 report's ESM-D8-02, which was never filed; still present at HEAD.
- **Description**: #6184 (`3e4bfab51`) made `BYROREDUX_REQUIRE_GAME_DATA` turn a missing install into a failure on the strict lane, through `esm/test_paths.rs` (`require_or_skip`). These `#[ignore]`d real-data tests keep their own `eprintln!("Skipping…"); return;` shape, so on the strict lane they pass silently when the master is absent.
- **Evidence**: A fresh scan at f3713bf5e found 25 sites. Line numbers at the time: `equip.rs:2741`; `actor_value_derive.rs:1519,1941`; `records/tests.rs:213,906,1024,1058,1094,1774,1818`; `actor/tests.rs:2175,2226,2351`; `cell/tests/integration.rs:14,76,141,227,282,311,390,450,501,543,595,634`. The shared `load()` helper at `records/tests.rs:78` has the same shape and is called by five more `#[ignore]`d ruleset tests (`records/tests.rs:89-181`), for 30 in all. Every real-data test added since 11-08 (`bow_real_data.rs`, `package_schedule_census.rs`, `lod_family_census.rs`, `parse_real_esm.rs` edits) already uses `require_or_skip` / `require_game_data`.
- **Impact**: A strict-lane run on a box missing a master reports these tests green without running them, the exact gap #6184 closed for the other real-data tests.
- **Suggested Fix**: Route every site through `test_paths::require_or_skip` (or have `load()` call it). Consider a source-scan guard (`source_scan::production_text` is not needed here; scan the test files) that rejects `eprintln!("Skipping` in an `#[ignore]`d test that does not call `require_or_skip`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
