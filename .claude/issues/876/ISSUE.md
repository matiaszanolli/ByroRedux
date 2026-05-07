# Issue #876 (OPEN): NIF-PERF-12: Header block_type_indices / block_sizes use per-element push-loops

URL: https://github.com/matiaszanolli/ByroRedux/issues/876

---

## Description

`crates/nif/src/header.rs:174-177` and `:200-203` populate POD `Vec<u16>` / `Vec<u32>` arrays via per-element push-loops, where a single `read_exact` into a typed Vec (the same pattern `read_pod_vec` uses internally) would suffice.

The header parses *before* the `NifStream` wrapper exists, so it can't directly call `read_u16_array` / `read_u32_array`. A small refactor that exposes the cursor-side primitive solves it.

## Evidence

```rust
// header.rs:174-177 — block_type_indices
let mut indices = Vec::with_capacity(num_blocks as usize);
for _ in 0..num_blocks {
    indices.push(read_u16_le(&mut cursor)?);
}
```

```rust
// header.rs:200-203 — block_sizes
let mut sizes = Vec::with_capacity(num_blocks as usize);
for _ in 0..num_blocks {
    sizes.push(read_u32_le(&mut cursor)?);
}
```

Both are POD arrays with bounds-checked count and no version-dependent layout per element.

## Why it matters

Per-file cost is negligible (~5 µs on 1000 blocks), but cell loads parse 100–300 NIF headers per cell — total ~1–2 ms shaved off cell-streaming critical path.

Not high-priority on its own; rolled in for completeness while the header storage is being touched for #834 (NIF-PERF-07 — `block_types: Vec<Arc<str>>` promotion).

## Proposed Fix

Lift `read_pod_vec`'s body into a standalone `read_pod_vec_from_cursor<T: Copy + Default>(cursor: &mut Cursor<&[u8]>, count: usize) -> io::Result<Vec<T>>` that the header parser can also call. Both header arrays then become:

```rust
let indices = read_pod_vec_from_cursor::<u16>(&mut cursor, num_blocks as usize)?;
// ...
let sizes = read_pod_vec_from_cursor::<u32>(&mut cursor, num_blocks as usize)?;
```

Single allocation, single read.

## Cost Estimate

Defer until #834 touches the header anyway — both changes share the same file and review surface.

## Completeness Checks

- [ ] **UNSAFE**: New cursor-side helper reuses `read_pod_vec`'s existing unsafe POD cast
- [ ] **SIBLING**: Audit other free-function `read_*_le` push-loops in `header.rs`
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing per-game header parse tests must produce bit-identical block_types / block_sizes

## dhat Gap

Expected savings are estimates; no quantitative regression guard exists today. This finding warrants a follow-up "wire dhat for header parse" issue.

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06.md` (NIF-PERF-12)
- Pairs with: #834 (NIF-PERF-07 — same file, header storage promotion)
