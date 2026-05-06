# Investigation — #832

**Domain**: nif

## Code path

5 hot-path sites in `crates/nif/src/lib.rs` use `map.entry(type_name.to_string()).or_*(...)` to bump per-type counters or push to a per-type cache:

- L332: `*drifted_by_type.entry(type_name.to_string()).or_insert(0) += 1;`
- L361-364: `parsed_size_cache.entry(type_name.to_string()).or_default().push(...)`
- L410, L448, L473: `*recovered_by_type.entry(type_name.to_string()).or_insert(0) += 1;`

`HashMap::entry(K)` takes `K` by value, so `to_string()` allocates *every* time even when the key exists. On Oblivion-no-block-sizes files (8000+ NIFs) the L361 cache insert fires once per block — ~7500 throwaway `String::from(&str)` per cell.

## Choice: quick fix vs architectural

The issue suggests two paths:

1. **Quick fix** (this PR) — keep `HashMap<String, _>`; replace `entry(K)` with `if let Some(v) = map.get_mut(k) { ... } else { map.insert(k.to_string(), ...) }`. Hot path (key exists) does ONE hash lookup, ZERO allocations. Cold path (first-seen) does TWO hash lookups + ONE `to_string`.

2. **Architectural** (bundled with #834) — promote `NifHeader.block_types: Vec<String>` to `Vec<Arc<str>>`, propagate `Arc<str>` through `block_type_name` and the parse loop. Then `HashMap<Arc<str>, _>::entry(Arc::clone(&...))` is an atomic increment.

I'm doing the **Quick fix only** since:

- It eliminates the audit's quoted ~150 KB/cell of throwaway allocations entirely on the hot path.
- It doesn't change any public API (`NifHeader.block_types` stays `Vec<String>`, `block_type_name` stays returning `Option<&str>`).
- No new dependencies (`raw_entry_mut` is still unstable on `std::HashMap`, and adding `hashbrown` for one optimization is overkill).
- #834 is a separate issue; the architectural change can land independently and won't double-fix the same hot path.
- User asked for #832 specifically. Bundling #834 here would add Arc<str> threading through the entire parse loop signature — significantly larger scope than the user requested.

## Pattern

```rust
// Pre:
*recovered_by_type.entry(type_name.to_string()).or_insert(0) += 1;

// Post:
if let Some(c) = recovered_by_type.get_mut(type_name) {
    *c += 1;
} else {
    recovered_by_type.insert(type_name.to_string(), 1);
}
```

For the `parsed_size_cache` site (which uses `or_default().push(...)`), same pattern with `vec![first_value]` on insert.

## Scope

1 file: `crates/nif/src/lib.rs`. Behavior unchanged; perf improvement is invisible to existing tests but verifiable via dhat. No new tests for the perf change itself; existing parse tests cover counter correctness.
