### SAVE-D2-2026-09-11-04: `ReferenceEnableState` is registered (with a correct `ValidateFn`) but has no save/load serde round-trip test — only a source-order text-scan test for restore ordering

- **Severity**: LOW
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: Test-gap only.
- **Location**: `byroredux/src/save_io.rs:501` (registration); `byroredux/src/save_io/live_reload_tests.rs:407` (the #3789 ordering test — a text-position check, not a data round trip)
- **Status**: NEW
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: No test constructs a populated `ReferenceEnableState`, saves it, decodes it, and asserts the restored value matches. `crates/save/tests/round_trip.rs`'s crate-level fixture doesn't include it either. Confirmed via `grep -n "ReferenceEnableState" byroredux/src/save_io/round_trip_tests.rs byroredux/src/save_io/live_reload_tests.rs` — the only hits are in `live_reload_tests.rs`, all doc-comment/ordering-check text, none a data-level assertion.

**Impact**: Low — no evidence of an actual serde defect, but a `FormId`-keyed map is exactly the shape class worth extra scrutiny per the Dimension 2 checklist.

**Related**: None.

**Suggested Fix**: Add a data-level round-trip test mirroring the pattern used for `FragmentExecutionQueue`/`PapyrusProviderContinuationQueue`/`CinematicPresentationState`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — populate `ReferenceEnableState`, run it through save/decode, assert equality
