# N1-04: NiAVObject bounding-volume branch triggers outside legitimate window

## Finding: N1-04 (LOW)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md` / Dimension 1
**Games Affected**: Non-Bethesda NIFs in [4.2.2.1, 10.0.0.x]. No target game affected.
**Location**: `crates/nif/src/blocks/base.rs:108-113`

## Description

nif.xml gates `Has Bounding Volume` / `Bounding Volume` on `since="3.0" until="4.2.2.0"`. Parser reads/skips a bounding volume for any `version < 0x0A000100` (10.0.1.0), covering the 4.2.2.1–10.0.0.x window where neither `Has Bounding Volume` nor `Collision Object` is present. Phantom bool + optional volume body corrupts all downstream NiAVObject reads in that window.

## Suggested Fix

Gate the bounding-volume skip on `version <= NifVersion(0x04020200)` and add a third branch that reads neither for the [4.2.2.1, 10.0.0.x] gap window.


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
