# N1-06: NiExtraData legacy 'Next Extra Data' threshold mismatch

## Finding: N1-06 (LOW)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md` / Dimension 1
**Games Affected**: Non-Bethesda Gamebryo in [4.2.2.1, 10.0.0.x].
**Location**: `crates/nif/src/blocks/extra_data.rs:44-46, 108`

## Description

Code comment says "Pre-Gamebryo (v < 5.0.0.1): NiExtraData does NOT inherit NiObjectNET. Format: next_extra_data_ref + bytes_remaining + subclass data." But the gate is `version < 0x0A000100` (10.0.1.0), not `< 0x05000001`. nif.xml says `Next Extra Data` is `until="4.2.2.0"` and `Num Bytes` is `since="4.0.0.0" until="4.2.2.0"`.

Files in [4.2.2.1, 10.0.0.x] fall through to `parse_legacy`, which reads `next_extra_data_ref` and `bytes_remaining` that don't exist there.

## Suggested Fix

Either tighten gate to `version <= NifVersion(0x04020200)` (matches nif.xml), or add a third branch for [4.2.2.1, 10.0.0.x] that reads only the subclass body.


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
