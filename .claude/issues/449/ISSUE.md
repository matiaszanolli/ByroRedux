# Issue #449

FO3-2-01: genhash_file high-word algorithm wrong — 119k debug warnings per archive open

---

## Severity: Medium (pre-existing, also fires on FNV)

**Location**: `crates/bsa/src/archive.rs:86-99`

## Problem

`genhash_file` folds the extension bytes on top of the stem-middle rolling hash via one sequential multiplication. BSArch / libbsarch / BSArchPro reference computes them **independently and adds**:

```
high = rolling(stem[1..len-2]) + rolling(ext_full)
```

The low 32 bits are correct (HashMap lookup is keyed by path, not hash), but every file with stem length > 3 produces a wrong high word.

## Evidence

- `meshes\armor\raiderarmor01\f\glover.nif`: stored `0xc86aec30_6706e572`, computed `0xd91bd930_6706e572`. Low matches, high differs.
- Python port: `rolling("lov") = 0x359da633`, `rolling(".nif") = 0x92cd45fd`, sum `= 0xc86aec30` ✓.
- Current code produces `0xd91bd930` (sequential fold).

119k `debug_assertions` warnings per FO3 archive open; 125k on FNV Meshes.bsa. The `#[cfg(debug_assertions)]` validation from #361 has never actually validated a single real file — it just isn't wired to a regression test.

## Impact

- Functional lookup unaffected (path-keyed HashMap).
- Debug log noise drowns real diagnostics.
- Future BSA-writing tooling would emit archives rejected by Bethesda tools.

## Fix

```rust
let hash_low = (hash as u32) ^ ext_xor;
let mut hash_ext = 0u32;
for &c in ext_bytes {
    hash_ext = hash_ext.wrapping_mul(0x1003f).wrapping_add(c as u32);
}
let hash_high = ((hash >> 32) as u32).wrapping_add(hash_ext);
hash = ((hash_high as u64) << 32) | (hash_low as u64);
```

## Completeness Checks

- [ ] **TESTS**: Regression test pinning `genhash_file` against a known stored hash from FNV Meshes.bsa (e.g. `meshes\clutter\food\beerbottle01.nif`)
- [ ] **SIBLING**: Verify folder hash (`genhash_folder`) already matches — audit sampled 1,885 folders with 0 mismatches
- [ ] **DOCS**: Reference BSArchPro GenHash (Pascal) in the fixed code comment

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-2-01)
