# N1-02: NiGeometryData 'Keep/Compress Flags' version threshold off by one minor

## Finding: N1-02 (LOW)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md` / Dimension 1
**Games Affected**: Non-Bethesda Gamebryo in [10.0.1.0, 10.0.1.x]. Target games unaffected.
**Location**: `crates/nif/src/blocks/tri_shape.rs:611-614`

## Description

Parser gates `keep_flags` / `compress_flags` on `version >= 0x0A000100` (10.0.1.0). nif.xml says `since="10.1.0.0"` (0x0A010000). For files in the 10.0.1.x gap window, 2 phantom bytes are read in `NiGeometryData` base.

## Suggested Fix

Change threshold to `NifVersion(0x0A010000)`.


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
