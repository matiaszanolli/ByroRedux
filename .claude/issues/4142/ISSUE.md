### SAVE-D2-2026-09-11-03: no save/load round-trip test exercises `Effect::SetLocked`/`Effect::SetLockLevel` — the specific v22 shape change — through an actual serialize/deserialize cycle

- **Severity**: LOW
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: Test-gap only.
- **Location**: `byroredux/src/save_io/round_trip_tests.rs:757` (`fragment_execution_queue_survives_save_load_round_trip_and_resumes`)
- **Status**: NEW
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: The one existing `FragmentExecutionQueue` round-trip test queues `Effect::Wait`/`ProviderCall`/`SetHudCartMode` — never `SetLocked`/`SetLockLevel`, the two variants that motivated the v22 bump. Confirmed via `grep -n "SetLocked\|SetLockLevel" byroredux/src/save_io/round_trip_tests.rs` — no hits.

**Impact**: Low — both are plain-data variants using already-proven-serializable types, so a serde failure is unlikely, but the checklist's round-trip-coverage bar is unmet for the exact shape change `FORMAT_MAJOR` exists to protect.

**Related**: SAVE-D2-2026-09-11-01 (same commit/variants).

**Suggested Fix**: Add `SetLocked`/`SetLockLevel` values to the existing round-trip test or a sibling.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — a `FragmentExecutionQueue` round-trip test queuing `Effect::SetLocked`/`Effect::SetLockLevel` and asserting the restored values match
