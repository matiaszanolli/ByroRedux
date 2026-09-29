# #5088: OBL-2026-09-29-D5-01: The M47.3 real-data gate silently passes without Oblivion.esm, and no CI lane runs it

**Labels**: bug, low, legacy-compat, scripting, game:oblivion, test-gap

**Source report**: `docs/audits/AUDIT_OBLIVION_2026-09-29.md`
**Severity**: LOW (test-gap)
**Dimension**: Gameplay & UI Data Slice (M47.3)

## Location
- `crates/scripting/src/obscript_quests.rs` (real-data test): resolves `BYROREDUX_OBLIVION_DATA` by hand and does `if !path.is_file() { return; }`, with no strict-mode failure.
- `.github/workflows/real-data-gates.yml`: the strict parser lane loops `for crate in bsa bgsm sfmaterial hkx facegen menuxml`, not `scripting`.

## Description
The M47.3 quest-script corpus gate returns green when `Oblivion.esm` is absent, even under `BYROREDUX_REQUIRE_GAME_DATA=1`, and the scheduled strict lane never builds `byroredux-scripting`. It is the only real-data guard over the 255 vanilla quest scripts (ObScript framing and command ids).

Same class as PAR-D4-2026-09-29-01 (the Oblivion EGM test in the facegen crate), but a different crate and test.

## Evidence
Validated at HEAD 9fcfdc3fc: `obscript_quests.rs` reads `BYROREDUX_OBLIVION_DATA` and returns on `!path.is_file()` with no `BYROREDUX_REQUIRE_GAME_DATA` check; `real-data-gates.yml` crate loop does not include `scripting`.

## Impact
A regression in ObScript quest-script decoding lands green on any run without the data, including CI.

## Suggested Fix
- Resolve the path via `byroredux_plugin::esm::test_paths::oblivion_esm()`.
- Under `BYROREDUX_REQUIRE_GAME_DATA`, fail instead of returning, as the per-crate `require_game_data` helpers do (`crates/bsa/tests/bsa_real.rs`, `crates/facegen/tests/parse_real_facegen.rs`).
- Add the single named test to the strict lane (~160 MB resident, unlike the whole-crate plugin `--ignored` run).

## Completeness Checks
- [ ] **SIBLING**: Other `--ignored` real-data tests in `byroredux-scripting` checked for the same silent-skip shape
- [ ] **TESTS**: The strict lane runs the named test and fails when the data is missing

