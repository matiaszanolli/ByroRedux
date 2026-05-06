# NIF-PERF-01: per-block to_string() in parse loop for recovery / drift / size-cache counters

## Description

The per-block parse loop uses `recovered_by_type.entry(type_name.to_string()).or_insert(0)` and `drifted_by_type.entry(type_name.to_string()).or_insert(0)` to bump per-type counters. Each `to_string()` allocates a fresh `String` even when the entry already exists in the map — that allocation is unconditional because `HashMap::entry(K)` takes `K` by value.

The same pattern repeats on `parsed_size_cache.entry(type_name.to_string())` (line 362), which fires on **every successful parse** on Oblivion-no-block-sizes files (8000+ NIFs in vanilla Oblivion).

`type_name: &str` here is borrowed from the header's block-type table, so an `Arc::clone` would suffice — and `HashMap<Arc<str>, _>::entry` accepts `Arc<str>` cheaply (atomic increment instead of a heap copy).

## Location

- `crates/nif/src/lib.rs:332` (drifted)
- `crates/nif/src/lib.rs:362` (size cache, hot success path)
- `crates/nif/src/lib.rs:410, 448, 473` (recovered)

## Evidence

```rust
*drifted_by_type.entry(type_name.to_string()).or_insert(0) += 1;          // line 332
parsed_size_cache.entry(type_name.to_string()).or_default().push(...);    // line 362
*recovered_by_type.entry(type_name.to_string()).or_insert(0) += 1;        // lines 410, 448, 473
```

## Impact

On a typical Oblivion cell load (~150 NIFs × ~50 blocks/NIF = 7500 blocks), the success-path cache insertion at line 362 fires once per block — **7500 `String::from(&str)` allocations per cell** for type names averaging ~20 chars. ~150 KB of throwaway short-string allocations per cell. On Skyrim Meshes0 archive walks the drifted/recovered paths fire thousands of times. Estimated 0.5-1 ms per cell load, plus heap fragmentation.

## Suggested Fix

Two-stage:

1. **Quick fix** — `raw_entry_mut().from_key(type_name).and_modify(...).or_insert_with(|| (type_name.to_string(), 0))`. Keeps `HashMap<String, _>` but pays the `to_string()` only on insert (cold path).
2. **Architectural** — bundled with NIF-PERF-05: promote `NifHeader.block_types` from `Vec<String>` to `Vec<Arc<str>>`. Then keys become `HashMap<Arc<str>, _>` and lookups use `Arc::clone` (atomic increment) on insert. Closes both issues with one change.

## Related

- NIF-PERF-05 (companion finding — same Arc<str> promotion fix)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check all `HashMap<String, _>::entry(...to_string())` patterns across `crates/nif/`
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Allocation-counter test (e.g. `dhat`) over a 150-NIF Oblivion cell parse, assert no `String::from` allocations on the steady-state path

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM5.md` — NIF-PERF-01