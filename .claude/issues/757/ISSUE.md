# #757: SF-D3-04: Stopcond captures Name unmodified; downstream re-lowercases per block (allocation cosmetic)

URL: https://github.com/matiaszanolli/ByroRedux/issues/757
Labels: nif-parser, medium, performance

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 3, SF-D3-04)
**Severity**: MEDIUM (cosmetic; no correctness impact)
**Status**: NEW

## Description

The stopcond captures `net.name` verbatim into the stub. `material_path_from_name` (`crates/nif/src/import/mesh.rs:750-761`) does its own `to_ascii_lowercase` on the already-captured string. Two issues:

1. The lowercase check produces a 2nd allocation per shader block during import, just to test the suffix — `.eq_ignore_ascii_case` against `"bgsm"` / `"bgem"` on the last 5 bytes is allocation-free.
2. Tests at `mesh_material_path_capture_tests.rs:248-253` cover `.BGSM` / `.bgsm` round-trip but NOT `.MAT` / `.mat` (because there's no Starfield branch yet — closed by SF-D3-01).

## Suggested Fix

Use `eq_ignore_ascii_case` on the last 5 bytes; allocation-free. Track alongside SF-D3-01's gate-tightening so both land in one PR.

```rust
fn has_material_suffix(name: &str) -> bool {
    let trimmed = name.trim_end_matches('\0').trim_end();
    let bytes = trimmed.as_bytes();
    let len = bytes.len();
    if len < 4 { return false; }
    let last5 = &bytes[len.saturating_sub(5)..];
    last5.eq_ignore_ascii_case(b".bgsm")
        || last5.eq_ignore_ascii_case(b".bgem")
        || last5[1..].eq_ignore_ascii_case(b".mat")
}
```

## Completeness Checks

- [ ] **TESTS**: Extend `material_path_from_name_helper_accepts_both_suffixes` with `.MAT` / `.mat` cases.
- [ ] **SIBLING**: Verify all callers of `material_path_from_name` switch to the allocation-free helper.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.

## Related

- SF-D3-01 (#749)
