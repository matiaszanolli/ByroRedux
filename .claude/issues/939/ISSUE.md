# NIF-D3-NEW-03: parse_nif drift detector masks bugs; add per-block-type drift histogram

**Severity**: MEDIUM (architectural / observability)
**Source audit**: `docs/audits/AUDIT_NIF_2026-05-10.md` (Dim 3)

## Location

`crates/nif/src/lib.rs` — `parse_nif` block-walk loop; per-block `stream.skip(remaining)` recovery.

## Why it's a bug

When a block parser under-/over-reads relative to declared `block_size`, the outer loop pads/skips. Parse rates LOOK clean (no error) while quietly papering over byte-level bugs. There's no log-level distinction between "consumed exactly `block_size`" and "padded 1 byte."

The known `NiTexturingProperty` 1-byte shortfall has been "tracked separately" precisely because there's no automated way to root-cause it. NIF-D3-NEW-01 is one candidate root cause but only telemetry will confirm.

## Impact

Parser correctness regressions go unnoticed. The compat matrix's "recovered" column counts NIFs but not bytes-skipped. The next regression slips.

## Fix

Per-block-type drift histogram. Inside `parse_nif`, log `block_type=X, declared=N, consumed=M, drift=N-M` at debug level. Aggregate into a `--drift-histogram` CLI flag on the `nif_stats` example. Reuse the existing #832 `bump_counter` (`get_mut/insert` split) pattern to avoid alloc churn.

~40 LOC.

## Completeness Checks

- [ ] **TESTS**: Verify histogram entries for known-good NIFs are zero; verify a synthetic drift-injecting NIF produces the expected entry
- [ ] **PERF**: Confirm no measurable per-frame cost on Megaton load (use existing `cargo run --release --bench-frames` flow)
