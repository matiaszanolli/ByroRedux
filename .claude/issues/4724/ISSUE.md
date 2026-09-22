# UI-D7-2026-09-21-04: the HUD drivers have no unit tests, FNV has no gate, and the HUD smokes report missing data as FAIL

**Issue**: #4724
**Severity**: LOW
**Labels**: low,ui,bug,test-gap

## Description
`byroredux/src/hud.rs` (773 lines), `byroredux/src/scaleform_hud.rs` (422 lines) and `byroredux/src/commands/hud.rs` (235 lines) contain zero `#[test]` functions, and running the bin test harness filtered on `hud`/`scaleform` matches nothing. Yet substantial pure logic in these files is device-free and directly testable: `hud_archive_args`/`hud_scaleform_args` discovery (the exact function UI-D7-2026-09-21-03's bug lives in), `fraction`/`bar_fractions`, `hash_signature` and the upload throttle, `hud.values`/`hud.heading` parsing, and the push table's prefix skip.

Separately, the four `m48-*-hud.sh` HUD smoke scripts `exit 1` ("FAIL: missing …") when the required game data isn't present locally, unlike `m48-menu-load.sh` which `exit 77`s (SKIP) on the same condition — and only `m48-menu-load.sh` is wired into `scripts/check-playable-smoke-contracts.sh`. No smoke or test gates FNV at all.

## Evidence
Verified at HEAD `ee6d3fb39`: `grep -c '#\[test\]'` is 0 in all three files (`hud.rs`, `scaleform_hud.rs`, `commands/hud.rs`); the bin harness run with `-- hud scaleform` filters selects 0 tests. `docs/smoke-tests/m48-4-oblivion-hud.sh`, `m48-5-fo3-hud.sh`, `m48-6-skyrim-hud.sh`, `m48-7-fo4-hud.sh` all `exit 1` with `"FAIL: missing $DATA/$f"` on absent data files; `scripts/check-playable-smoke-contracts.sh` references `m48-menu-load.sh` but not the four HUD-specific smokes.

## Impact
Coverage gap only. UI-D7-2026-09-21-03 (FO3/FNV discovery choosing the wrong archive) and `CHAR-2026-09-21-D1-01` (wrong bar-source FormIDs) are exactly the class of defect these missing unit tests would have caught at the function level, well before an end-to-end smoke run. The FAIL-on-missing-data smoke behavior also means these scripts cannot run as an unconditional CI gate without real game data staged, so nothing currently exercises this code path in CI at all.

## Related
- UI-D7-2026-09-21-03 (this report) — a concrete bug this coverage gap let ship.
- CHAR-2026-09-21-D1-01 — another concrete bug in the same driver code, filed by the character-audit track.
- PAR-D4-2026-09-21-03 (issue #4665) — already tracks the missing FNV MenuXml *corpus* test; this finding is the driver-level (unit test) gap plus the smoke/contract-checker gap.

## Suggested Fix
Add unit tests for the listed pure functions, including an FNV-DLC discovery case (`--master FalloutNV.esm --esm HonestHearts.esm`) and a player-vs-NPC `fraction` case. Change the four `m48-*-hud.sh` smokes to `exit 77` on missing data (matching `m48-menu-load.sh`) and add them to `scripts/check-playable-smoke-contracts.sh`. Add an FNV twin of `crates/menuxml/tests/fo3_corpus.rs`.

## Completeness Checks
- [ ] **TESTS**: Unit tests land for `hud_archive_args`/`hud_scaleform_args`, `fraction`/`bar_fractions`, `hash_signature`, and `hud.values`/`hud.heading` parsing
- [ ] **TESTS**: The four HUD smokes SKIP (exit 77) rather than FAIL on missing data, and join the contract checker

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D7-2026-09-21-04)
