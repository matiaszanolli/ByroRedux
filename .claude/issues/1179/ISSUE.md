# FO4-D4-002: vanilla Fallout4.esm ships zero MOVS; decode path has no real-data coverage

**Labels**: bug, import-pipeline, low

**Source**: [`docs/audits/AUDIT_FO4_2026-05-18.md`](docs/audits/AUDIT_FO4_2026-05-18.md)
**Dimension**: ESM Architecture Records
**Severity**: LOW (test coverage gap)

## Observation

`crates/plugin/tests/parse_real_esm.rs:1065-1071`:

```rust
// MOVS: vanilla ships 0; pin to 0 to catch a future spurious
// population (DLC-only or mod-content additions can lift this
// floor when those harnesses arrive).
assert_eq!(
    index.cells.movables.len(),
    0,
    "MOVS={} (vanilla Fallout4.esm ships 0; non-zero indicates \
     a DLC was loaded — bump the floor when that's expected)",
    index.cells.movables.len(),
);
```

Live run confirms: `movables=0`. The `MovableStaticRecord` decode path (`crates/plugin/src/esm/records/movs.rs:85-131`, captures MODL / LNAM / ZNAM / DEST / VMAD) has no real-data parse coverage.

## Why bug

The first DLC or mod that introduces MOVS will trip the `assert_eq!(movables.len(), 0)` and any decoder regression silently shipped today would only surface then. The pinned-to-zero assertion is intentional regression-detection but means none of the MOVS field plumbing is exercised end-to-end.

## Fix

Either:

- **(a)** Gate a `>= 1` assert on `BYROREDUX_FO4_DATA_DLC` or similar, switch when DLC harness arrives; OR
- **(b)** Add a synthetic-bytes fixture that exercises `parse_movs_group` end-to-end through `parse_esm`, asserting MODL / LNAM / ZNAM / VMAD round-trip into a `MovableStaticRecord`.

Option (b) is independent of game-data availability and recommended.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: confirm SCOL / PKIN / TXST / MSWP also have either real-data OR synthetic-fixture coverage of all sub-record types they parse
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic-bytes fixture exercising `parse_movs_group`

## Related

- #588 — MOVS parser landed
- FO4-D4-004 (separate issue) — `parse_rate_fo4_esm` ignore-gating compounds this gap
