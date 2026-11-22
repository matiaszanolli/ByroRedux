# #6999: UI-D2-2026-11-15-02: the real-Ruffle adapter guards self-skip green under the strict-data gate, and no lane runs them with data

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6999
- **Labels**: low,ui,test-gap,bug
- **Source**: docs/audits/AUDIT_UI_2026-11-15.md

From the UI audit `docs/audits/AUDIT_UI_2026-11-15.md` (HEAD 8473ae994).

- **Severity**: LOW
- **Dimension**: AVM2 Adapter (also Profile & Bridge)
- **Profile**: both Scaleform profiles
- **Existing**: none. #6993 (closed) fixed only `avm1_host/tests.rs::installed_skyrim_host_calls_are_all_cataloged` and moved `host/tests.rs` to `test_paths::data_dir`.
- **Location**: `crates/ui/tests/fallout4_hudmenu_protocol.rs:19-34`, `crates/ui/tests/hudmenu_protocol.rs:23-40`, `crates/ui/src/avm2_host.rs:1409-1423` (`fallout4_interface_archive`, five callers)
- **Status**: NEW
- **Description**: these tests are not `#[ignore]`d and each carries a private data resolver that returns on a missing archive (`eprintln!("skipping ...")`; `fallout4_interface_archive` reads `BYROREDUX_FO4_DATA` itself). The real-data lane (`.github/workflows/real-data-gates.yml`, parsers job) runs `cargo test --release -p byroredux-ui -- --ignored`, which executes exactly three tests (I ran it: `3 passed`, the two protocol binaries report `running 0 tests`), and the ordinary CI job runs without game data. So the FO4 protocol pin (the only test that drives the injected adapter through a real Ruffle `hudmenu` lifecycle and asserts destruction acknowledgement) and the five `avm2_host` corpus readers (`all_installed_fallout4_swfs_round_trip_through_injection`, `installed_fallout4_host_calls_are_all_forwarded`, `installed_menus_without_destroy_trait_report_the_partial_lifecycle`, `re_injecting_already_patched_bytes_is_a_no_op`, `patching_an_already_patched_root_constructor_is_rejected`) run only when a developer happens to run `cargo test` on a machine with the games installed. `BYROREDUX_REQUIRE_GAME_DATA=1` changes nothing for them: with the variable set and the data path pointed at an empty directory they still return green (the `data_dir`/`fallout4_interface_archive` helpers never consult `test_paths::game_data_required`).
- **Evidence**: file/line above; `cargo test --release -p byroredux-ui -- --ignored --test-threads=1` output (`3 passed; 119 filtered out`, `running 0 tests` for `fallout4_hudmenu_protocol` and `hudmenu_protocol`); `grep -n '#\[ignore' crates/ui/tests/*.rs` is empty; `grep -rn _protocol .github/workflows scripts` is empty.
- **Impact**: the SKILL names these as the Dim 2 guards. A regression in injection that only shows on real movies would pass every CI-reachable lane. This is the #6184/#4660 "green skip" shape.
- **Suggested Fix**: replace the two private `data_dir` helpers and `fallout4_interface_archive` with `byroredux_plugin::esm::test_paths::{data_dir, FO4_ENV, FO4_DEFAULT, SKYRIM_SE_ENV, ...}` plus `require_or_skip(..)` (panic when `BYROREDUX_REQUIRE_GAME_DATA` is set), or mark them `#[ignore = "needs ... game data"]` so the parsers lane's `--ignored` run picks them up (and the lane's `count-passed-tests.sh` guard then counts them).
- **Completeness Checks**: SIBLING `avm1_host/tests.rs` and `host/tests.rs` already use `test_paths`; TESTS add a strict-mode assertion mirroring `test_paths`'s own env test.
- **Merged**: UI-D3-2026-11-15-04 reported the same gap from the catalog side: each protocol pin keeps a private `data_dir()` that skips green, and is in no strict lane. Its probe: with `BYROREDUX_SKYRIMSE_DATA=/nonexistent` the Skyrim pin panics, so a wrong path is loud, but with the variable unset and no install it passes green. One issue covers both.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
