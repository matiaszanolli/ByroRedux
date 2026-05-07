# Issue #885 (OPEN): CELL-PERF-09: stamp_cell_root uses entry.push per entity — replace with extend(first..last)

URL: https://github.com/matiaszanolli/ByroRedux/issues/885

---

## Description

`stamp_cell_root` (`byroredux/src/cell_loader.rs:104-112`) reserves `span + 1` capacity, then pushes each entity ID one at a time via `entry.push(eid)`. The body is a contiguous range; `entry.extend(first..last)` is the more idiomatic and trivially faster pattern.

## Evidence

```rust
// cell_loader.rs:104-112
if let Some(mut idx) = world.try_resource_mut::<CellRootIndex>() {
    let entry = idx.map.entry(cell_root).or_insert_with(Vec::new);
    let span = last.saturating_sub(first) as usize;
    entry.reserve(span + 1);
    for eid in first..last {
        entry.push(eid);
    }
    entry.push(cell_root);
}
```

Capacity is already reserved, so there's no allocation difference — but `extend` over a known-size iterator can elide bounds checks per push and lets the compiler inline the iteration as a typed memcpy when the source range is `Copy`.

## Proposed Fix

```rust
if let Some(mut idx) = world.try_resource_mut::<CellRootIndex>() {
    let entry = idx.map.entry(cell_root).or_insert_with(Vec::new);
    let span = last.saturating_sub(first) as usize;
    entry.reserve(span + 1);
    entry.extend(first..last);
    entry.push(cell_root);
}
```

Trivial. The behavior is identical; the codegen is at least as good and typically better.

## Cost Estimate

Marginal. Per-cell-load cost; capacity already reserved so zero new allocations. Cosmetic + minor branch overhead reduction.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit other `entry.reserve(N); for ... { entry.push(...) }` patterns; the same shape often appears elsewhere in cell_loader_*.rs siblings
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing cell-load tests must produce bit-identical `CellRootIndex` contents

## dhat Gap

Negligible alloc impact (capacity already reserved); zero new allocations. Per audit-performance command spec: warrants follow-up "wire dhat" issue for completeness, but no quantitative baseline shift expected from this fix alone.

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06b.md` (CELL-PERF-09)
