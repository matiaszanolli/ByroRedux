# #6296: SAVE-2026-10-21-06: `schema_fingerprint` is row-order-sensitive and no test pins it, while #5802 keeps moving registration rows

**Labels**: low,save-load,test-gap,bug

**Source**: docs/audits/AUDIT_SAVE_2026-10-21.md · **Severity**: LOW · **Dimension**: Format & Schema Discipline · **Location**: `crates/save/src/registry.rs:377-395`

- **Data-Loss Class**: none (a reorder rejects every existing save with `SchemaMismatch`; it does not corrupt one)
- **Location**: `crates/save/src/registry.rs:377-395`; `byroredux/src/save_io/registry.rs:511-558` (the builder chain split at each `register_saved` call)
- **Status**: NEW (related #5802)
- **Description**:
  1. The fingerprint is FNV-1a over component, then resource, names in registration order.
  2. The snapshot itself is a name-keyed `BTreeMap`, so order carries no compatibility meaning. A pure reorder still invalidates every save, with no `FORMAT_MAJOR` bump or ledger entry to explain it.
  3. Each #5802 slice must keep each delegate at its row's former position. `bde3478db` and `e619dbbaf` verified this by printing the fingerprint by hand on two trees, as their commit messages record.
  4. No test pins the value. `SaveRegistry Order-Sensitive` is tracked only as a memory note.
  5. The next slice that moves a delegate one line goes unnoticed in CI.
- **Impact**: every player save becomes unloadable after a refactor that changed no data. It is caught only by a live load.
- **Suggested Fix**: either hash the sorted names (a one-time fingerprint change to fold into the next genuine `FORMAT_MAJOR` bump), or add a `const BASELINE_SCHEMA_FINGERPRINT` test beside `BASELINE_SHAPE_FINGERPRINT`, refreshed with a justification on every column add or remove, so a reorder fails CI.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
