# #6993: TD9-2026-11-15-04: nine data-gated ignores in four crates run in no lane, and one `ui` test skips green under the strict-data gate

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6993
- **Labels**: low,speedtree,ui,test-gap,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 9 (ignore triage / lanes)
- **Location**: `crates/spt/tests/parse_real_spt.rs:146,174,200,271`; `crates/ui/src/avm1_host/tests.rs:255-262`, `crates/ui/src/host/tests.rs:652,670`; `crates/pex/tests/r5_fidelity.rs:153`; `crates/video/src/lib.rs:819`. Not run by: `real-data-gates.yml` parsers lane (`bsa bgsm sfmaterial hkx facegen menuxml`), the corpus job (nif only), the pin lanes (no row).
- **Age**: various; `video` test 2026-11-12
- **Effort**: small
- **Labels**: `tech-debt`, `test-gap`
- **Status**: NEW. The `video` row is already recorded in `audit-parsers/SKILL.md:85` ("absent from `real-data-gates.yml`"), so it is listed for completeness only.
- **Description**: by crate: `spt` 4 (`parse_rate_*_spt`, `walker_stops_on_true_tlv_boundary`, citing closed #3808/#4122), `ui` 3 (`installed_skyrim_host_calls_are_all_cataloged`, citing closed #2966/#3103; plus two installed-corpus protocol tests), `pex` 1 (`da10_main_door_decompiles_to_the_r5_reference_shape`), `video` 1 (the Bink movie sweep, which also reads its own `BYROREDUX_STEAM_COMMON` instead of the `BYROREDUX_*_DATA` variables). `audio` adds 9 device-gated ignores (hardware, expected). The `spt` file does honour `BYROREDUX_REQUIRE_GAME_DATA` (`parse_real_spt.rs:52-63`). The `ui` catalog test does not: `installed_skyrim_host_calls_are_all_cataloged` re-inlines `/mnt/data/SteamLibrary/...` as its default and, when `BsaArchive::open` fails, prints "skipping" and returns green (`avm1_host/tests.rs:257-263`), the #6184 pattern that #3850 closed elsewhere.
- **Evidence**: `avm1_host/tests.rs:261-264`: `let Ok(archive) = BsaArchive::open(&archive_path) else { eprintln!("skipping: …"); return; };`
- **Impact**: the cited issues are MEDIUM or unlabelled, not HIGH, so this stays LOW; the decoders they cover (SpeedTree TLV walker, AVM1 host catalog, `.pex` decompiler) have no automated real-corpus gate, and a missing archive cannot fail the ui catalog test even under the strict flag (its two siblings `expect` the archive and do fail).
- **Related**: #5905, #6184, #3850, #6182.
- **Suggested Fix**: add `spt ui pex video` to the `parsers` lane crate list, route the `ui` catalog test through `byroredux_plugin::esm::test_paths` (or a `require_or_skip` equivalent that honours `BYROREDUX_REQUIRE_GAME_DATA`), and give `video` the standard `BYROREDUX_*_DATA` variables.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
