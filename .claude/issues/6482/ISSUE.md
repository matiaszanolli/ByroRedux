# PAR-D4-02: Two FO76 strict-lane gates are red after FO76's 2026-10-24 patch. The bgsm material count (29,991 pinned, 30,078 shipped) and the BA2 archive census (40 pinned, 43 shipped) are exact pins on a live-service corpus

Labels: low, import-pipeline, game:fo76, test-gap, bug
Source: docs/audits/AUDIT_PARSERS_2026-10-27.md

**Source**: `docs/audits/AUDIT_PARSERS_2026-10-27.md` finding `PAR-D4-02` (LOW)
**Location (re-mapped to current tree)**: `crates/bgsm/tests/parse_all.rs:~476` (`parse_rate_fo76_materials_corpus`, pinned c5a0b98c6 #5904); `crates/bsa/tests/ba2_real.rs:~724` (`fo76_ba2_corpus_opens_and_lists`, pinned ce008af1c #5903)

**Severity**: LOW (test-gap; both gates are red and each masks its own later assertions). Dimension: Corpus Gates. Game: FO76.

## Description
28 FO76 archives on the audit install were rewritten on 2026-10-24.
- **bgsm.** The parse is fully clean (30,076 clean, 2 allowed JSON text files, 0 failures) but the exact-count assert runs first and fails (`left: 30078, right: 29991`). Every later assertion (diagnostic, over-ceiling) can no longer report.
- **BA2.** `fo76_ba2_corpus_opens_and_lists` opens 43 archives vs the pinned 40 (`left: 43, right: 40`, after 420 s). The entry-count assert (679,240) and the "every entry extracted" assert come after it and never run, so it is unknown whether the patched archives extract cleanly.
- `fo76_dx10_texture_extract_round_trips` passes.
FO76 is a live-service title, so an exact pin on its corpus breaks on every patch.

## Evidence
`assert_eq!(stats.total, 29_991, ...)` at `parse_all.rs:477`; `assert_eq!(opened, 40, "FO76 BA2 archive census changed")` at `ba2_real.rs:724` (both verified at HEAD). Run via `BYROREDUX_REQUIRE_GAME_DATA=1 cargo test --release -p <crate> -- --ignored`.

## Impact
Strict real-data lane red; parse/extract regressions on FO76 are hidden behind the stale count asserts until re-pinned.

## Related
#5903, #5904.

## Suggested Fix
Re-measure and re-pin both. Better: run the clean-rate and extract-all checks *before* the count asserts, and make the FO76 counts floors (or key them on archive sizes/hashes) so a patch reads as "corpus changed, re-measure" instead of hiding whether the parse still holds.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix

