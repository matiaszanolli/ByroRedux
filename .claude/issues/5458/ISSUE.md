# #5458: SAVE-D1-2026-10-08-01: Eat/Sleep allowlist reasons claim "the seated pose … carries via the registered Seated restore", but `Seated` is excluded from the live overlay

**Labels**: low,save-load,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5458

**Source**: `docs/audits/AUDIT_SAVE_2026-10-08.md` — `SAVE-D1-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: rows 568-570 of `registry_completeness_tests.rs` still cite the "registered Seated restore"; `Seated` is still absent from `MUTABLE_DELTA_COLUMNS`.

- **Severity**: LOW
- **Dimension**: Snapshot Completeness & the Two Lists
- **Data-Loss Class**: none (stale reason; cosmetic redo)
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:568-570`; `byroredux/src/save_io.rs:113-128` (`Seated` deliberately absent from `MUTABLE_DELTA_COLUMNS`) and `:435-453` (register comment: the full round trip is `restore_world`-only).
- **Status**: NEW. Distinct from GAME-D5-2026-10-08-03, which concerns `EatSleepState`'s "idempotent" destination claim; this one is the `Seated` claim on all three rows.
- **Description**: `Seated` is registered but never replayed by `execute_pending_save_loads`, the only production load path. A diner or sleeper seated at save time therefore respawns standing. It re-walks and re-seats through `eat_sleep_system`, which is exactly the "silently redo its Seat behavior" outcome that the `Seated` registration comment says registration prevents. The guard checks only that a reason exists, so the false mechanism stays green.
- **Suggested Fix**: Restate the three reasons as "re-derived: the actor re-walks and re-seats after a live load (`Seated` is `restore_world`-only)". Alternatively, add a FormID-keyed seated ledger if the redo is unwanted.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other allowlist rows that cite a `restore_world`-only registration as a live-load mechanism)
- [ ] **TESTS**: A regression test pins this specific fix
