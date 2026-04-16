# N1-01: NiGeometryData 'Group ID' read 1 version too early

## Finding: N1-01 (LOW)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md` / Dimension 1
**Games Affected**: Non-Bethesda Gamebryo pre-10.1.0.114 (Civ IV era). Target games (Oblivion+) unaffected.
**Location**: `crates/nif/src/blocks/tri_shape.rs:607-609`

## Description

`parse_geometry_data_base` gates `Group ID` on `version >= 0x0A000100` (10.0.1.0). nif.xml gates it `since="10.1.0.114"` (0x0A010072). Files in [10.0.1.0, 10.1.0.113] read 4 phantom bytes, misaligning every NiGeometryData afterward.

Parser:
```rust
if stream.version() >= NifVersion(0x0A000100) { let _group_id = stream.read_i32_le()?; }
```

nif.xml:3882: `<field name="Group ID" type="int" since="10.1.0.114">Always zero.</field>`

## Suggested Fix

Change threshold to `NifVersion(0x0A010072)`.


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
