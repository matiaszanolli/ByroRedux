# OBL-D3-C3: ESM walker hardcodes 24-byte group header, breaks on Oblivion (20-byte)

**Issue**: #391 — https://github.com/matiaszanolli/ByroRedux/issues/391
**Labels**: bug, critical, legacy-compat

---

## Finding

The ESM walker computes group content end as `reader.position() + group.total_size - 24` at ~13 sites across `crates/plugin/src/esm/`. Oblivion's TES4 group header is **20 bytes**, not 24. Skyrim/FO4 are 24.

Affected sites (non-exhaustive):
- `crates/plugin/src/esm/records/mod.rs:114, 232`
- `crates/plugin/src/esm/cell.rs:208, 212, 221, 225, 234, 287, 462, 663, 735, 826, 945, 991`

The correct value is already exposed at `crates/plugin/src/esm/reader.rs:67-72` via `reader.variant().group_header_size()` — it just isn't threaded through the walker.

## Why this works today (and why it's dangerous)

`read_group_header()` advances `pos` variant-correctly, so each new record read starts at the right offset. The off-by-4 is the walker's _termination_ computation — it reads ~1 extra record past the nominal end, which happens to be benign on `Oblivion.esm` because the next group header is self-delimiting. Any parser touching the `total_size - 24` computed value for slicing or bounds-checking is a latent corruption site.

## Impact

- Latent corruption risk on Oblivion ESM walks. No confirmed rendering damage today, but any parser added that treats `group.total_size - 24` as an authoritative slice end (e.g. for LAND / PGRD / VISI nested groups) will read junk bytes on Oblivion.
- Every future variant (FO4+ uses different header sizes too? not currently) needs the same threading.

## Fix

Thread a helper. Either:

```rust
let end = reader.position() + group.content_size();  // new helper
// OR
let end = reader.position() + group.total_size as usize - reader.variant().group_header_size();
```

Mechanical — 13 call sites. Add regression test: parse a synthetic Oblivion group with exactly one record that ends at `total_size - 20` and verify the walker stops at the record boundary.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Grep for any other `- 24` literal in `crates/plugin/src/`.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Variant-parameterized walker test — same input data, compare Oblivion (20) vs Skyrim (24) group-end computation.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 3 C3.
