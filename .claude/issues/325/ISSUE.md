# M4: NiGeometryData 'Has UV' version window too wide (breaks Morrowind compat)

## Finding: N1-03 (MEDIUM)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md`
**Dimension**: Block Parsing
**Games Affected**: NIFs with version in [4.0.0.3, 20.0.0.3] — early Gamebryo titles, Morrowind-adjacent hybrids. Target games (Oblivion 20.0.0.4+) safe.
**Location**: `crates/nif/src/blocks/tri_shape.rs:671-676`

## Description

nif.xml gates `Has UV` on `until="4.0.0.2"`. The parser reads it for any `version < 0x14000004` (20.0.0.4), i.e., for versions 4.0.0.3 through 20.0.0.3 where the field does not exist.

```rust
let has_uv = if stream.version() >= NifVersion(0x14000004) {
    num_uv_sets > 0
} else {
    stream.read_byte_bool()?   // reads 1 phantom byte for [4.0.0.3, 20.0.0.3]
};
```

The phantom read consumes 1 byte and proceeds to read N UV sets from `data_flags & 0x3F`. May succeed by luck but leaves the stream 1 byte misaligned.

## Impact

Misalignment on early-Gamebryo titles and Morrowind-adjacent hybrid content. Breaks Morrowind aspirational compatibility noted in `compatibility_roadmap.md`.

## Suggested Fix

Gate `has_uv` read on `version <= NifVersion(0x04000002)` only. For the intermediate window, derive `has_uv` from `num_uv_sets` (already read earlier from pre-Gamebryo branch at tri_shape.rs:665) or from `data_flags & 0x3F`.


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
