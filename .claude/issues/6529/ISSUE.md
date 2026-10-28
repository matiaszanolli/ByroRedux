# TOOL-D2-2026-10-27-01: #6278's new wire test was inserted under an existing test's doc comment, so the "Phase 1 lockstep pin" doc describes the wrong test

Labels: low, documentation, tech-debt, doc-rot
Source: docs/audits/AUDIT_TOOLING_2026-10-27.md

# TOOL-D2-2026-10-27-01: #6278's new wire test was inserted under an existing test's doc comment, so the "Phase 1 lockstep pin" doc describes the wrong test

**Source**: `docs/audits/AUDIT_TOOLING_2026-10-27.md` finding `TOOL-D2-2026-10-27-01` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `crates/debug-protocol/src/wire.rs:175-183` (doc block) and `:194` (`round_trip_phase1_response_variants`, which the doc block used to sit on).

## Description
The block "Phase 1 - same lockstep pin for the new response variants. Adding a field to `DebugResponse::Metrics` without updating this test ... breaks the build at the missing assertion" documented `round_trip_phase1_response_variants`. 05d94dd61 placed the new `/// #6278 ...` lines and `metrics_payload_without_the_breakdowns_still_deserializes` between that block and its function. Rustdoc concatenates the two blocks onto the new serde-default test, and `round_trip_phase1_response_variants` is left with no doc. A reader is now told a deserialize-only test "breaks the build" when a field is added; that claim belongs to the struct-literal round trip.

## Evidence
`wire.rs:175-179` (old doc), `:180-181` (#6278 doc), `:182-183` (`#[test] fn metrics_payload_without_the_breakdowns_still_deserializes`), `:193-194` (`#[test] fn round_trip_phase1_response_variants` with no doc) - verified at HEAD.

## Impact
The doc gives wrong guidance on which test enforces payload lockstep. No runtime effect.

## Related
#6278, #5692 (same displaced-doc class; see also #6373's spliced-doc guard).

## Suggested Fix
Move lines 175-179 back above `round_trip_phase1_response_variants`, leaving the #6278 lines on the new test. The #6373 spliced-doc scan does not see `debug-protocol` tests; consider extending it (see TD2-2026-10-27-02).

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix

