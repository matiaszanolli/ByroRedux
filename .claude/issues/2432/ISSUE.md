# TD9-001: an_unrecognized_pex_is_a_silent_miss discards its own result, asserting nothing

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2432
**Finding ID**: TD9-001 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 9 — Test Hygiene
**Location**: `crates/scripting/tests/pex_recognize_e2e.rs:120-131`
**Status**: NEW

## Description
`an_unrecognized_pex_is_a_silent_miss`'s own comment states the contract it exists to verify (a vanilla script should translate to `None`), then computes `translate_pex(...)` and discards it with `let _ = ...`. If `ObjectReference.pex` ever became accidentally recognized by an overly broad future recognizer — the exact regression the test's name says it guards against — it would still pass silently.

## Suggested Fix
`assert!(got.is_none(), ...)` on the actual return value.

## Age
~5 weeks.

## Completeness Checks
- [ ] **TESTS**: The fixed assertion actually fails if a recognizer is deliberately broadened to match `ObjectReference.pex` (spot-check by temporarily broadening a recognizer and confirming red)
