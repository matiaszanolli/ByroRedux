# #601: FO4-DIM5-01: nif_stats block histogram is top-20 only — per-type NiUnknown rate invisible

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/601
**Labels**: nif-parser, low, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 5)
**Severity**: LOW
**Location**: `crates/nif/examples/nif_stats.rs:138-142` prints `sorted.iter().take(20)`

## Description

`nif_stats` emits a top-20 block histogram; blocks outside the top 20 are counted only in the tail `({N} distinct block types)` line with no per-type NiUnknown rate. For the FO4 audit-checklist question "does it report per-block-type coverage so you can see at a glance whether `BSConnectPoint::Parents/Children` / `BSBehaviorGraphExtraData` / `BSClothExtraData` / `BSInvMarker` are hit?", the answer is "only if they're in the top 20."

Per ROADMAP R3 open work: `NiUnknown` soft-fail path means a per-block parser regression shows up as missing geometry, not a parse failure. `MIN_SUCCESS_RATE = 1.0` catches file-level; per-block-type it doesn't.

## Evidence

```rust
// nif_stats.rs:138-142
for (name, count) in sorted.iter().take(20) {
    println!("  {:>40}: {}", name, count);
}
println!("({} distinct block types)", sorted.len());
```

## Impact

A FO4-specific block type could quietly fall into `NiUnknown` recovery on every FO4 NIF without this audit catching it.

## Suggested Fix

Extend `nif_stats` with:
1. `--all` or `--full` flag that prints the whole histogram (not just top 20).
2. A parallel `NiUnknown rate per block type` accumulator — track for each seen type: `(clean_count, unknown_recovery_count)` and emit as a second table.
3. Optional `--min-count N` to trim the tail.

Call-site user can then spot a block type that's silently recovering instead of parsing.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: `full_histogram.rs` example for cross-archive aggregation
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Snapshot test for histogram format with known-good output.

## Related

- ROADMAP R3 (per-block-type coverage) open item.
