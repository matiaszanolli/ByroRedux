# LC-D6-NEW-01: StringPool::intern allocates on every call — doc claims zero-allocation

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/894
**Source audit**: docs/audits/AUDIT_LEGACY_COMPAT_2026-05-07_DIM6.md
**Severity**: MEDIUM
**Dimension**: String Interning — allocation contract

## Location
- crates/core/src/string/mod.rs:5 (module doc claim)
- crates/core/src/string/mod.rs:31-34 (intern allocation)
- crates/core/src/string/mod.rs:46-49 (get allocation)

## Root Cause
`s.to_ascii_lowercase()` runs unconditionally before each `get_or_intern` / `get`, allocating a new String even when the symbol already exists. The module-level + method doc-comments promise "zero allocation after first intern" — promise is false.

## Real-world cost
Megaton cell load: ~4,400 unique strings interned, but ~60,000 intern calls. 55,600 throwaway lowercase allocations per cell. Hard to spot in flamegraphs because every intern site shows `to_ascii_lowercase`.

## Suggested Fix (pick one)

1. **Stack-buffer fast path** (~10 LOC): `[u8; 64]` stack array, lowercase byte-by-byte, fall back to String for >64 chars. Eliminates the allocation entirely for ≥99% of calls.
2. **Lookup-first fast path** (~5 LOC): try case-sensitive `get` first; lowercase only on miss. Same string interned with the same case → no allocation.

Either fix must update the misleading doc-comments.

## Verification

```bash
cargo bench --bench string_pool_intern   # add the bench if absent
# Before fix: ~55k allocations on a 60k-intern workload
# After fix: ≤ unique-string-count allocations
```

## Related
- #882 (CLOSED): CELL-PERF-05 — same code-path family
- #866 (OPEN): registry case-handling drift
