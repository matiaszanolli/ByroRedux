# Investigation — Issue #449 (FO3-2-01)

## Domain
BSA — `crates/bsa/src/archive.rs`

## Current code (archive.rs:86-97)

```rust
let hash_low = (hash as u32) ^ ext_xor;

let mut hash_high = (hash >> 32) as u32;
for &c in ext_bytes {
    hash_high = hash_high.wrapping_mul(0x1003f).wrapping_add(c as u32);
}

hash = ((hash_high as u64) << 32) | (hash_low as u64);
```

**Bug**: the ext rolling hash folds on TOP of the stem_high value via sequential multiplication. BSArch/libbsarch reference computes ext rolling hash independently starting from 0 then **adds** the result to stem_high.

## Evidence (from issue body)

For `meshes\armor\raiderarmor01\f\glover.nif`:
- stored hash: `0xc86aec30_6706e572`
- Python port: `rolling("lov")` = `0x359da633`, `rolling(".nif")` = `0x92cd45fd`, sum = `0xc86aec30` ✓
- Current sequential fold produces `0xd91bd930` (wrong high word)
- Low word (`0x6706e572`) is correct in both — HashMap lookup by path works regardless

## Impact
- 119k `debug_assertions` warnings per FO3 archive open (125k on FNV).
- HashMap lookup by path is unaffected (not keyed by hash).
- Future BSA-writing tooling would emit archives Bethesda tools reject.

## Fix

Replace with spec-compliant computation:

```rust
let hash_low = (hash as u32) ^ ext_xor;
let mut hash_ext = 0u32;
for &c in ext_bytes {
    hash_ext = hash_ext.wrapping_mul(0x1003f).wrapping_add(c as u32);
}
let hash_high = ((hash >> 32) as u32).wrapping_add(hash_ext);
hash = ((hash_high as u64) << 32) | (hash_low as u64);
```

## Regression test

Existing tests at lines 531/541 only check behavior ("different extensions produce different hashes") — no value pinning. Add a test matching the algorithm against the `glover.nif` stored hash from a real FNV/FO3 BSA: `0xc86aec30_6706e572`.

## Scope
1 file. Code fix + 1 regression test. No API surface change.
