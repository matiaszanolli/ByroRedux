# M1: NiMaterialProperty -20B under-read — compact_material() gate mis-firing on FNV

## Finding: NIF-D3-04 (MEDIUM)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md`
**Dimension**: Stream Position
**Games Affected**: Fallout NV (likely FO3, possibly FO4 compact-material files)
**Location**: `crates/nif/src/blocks/properties.rs:36-86` (gate at line 44)

## Description

24 `NiMaterialProperty` blocks in FNV consistently report `expected 68 bytes, consumed 48` — exactly 20 bytes (5 floats = ambient + diffuse) short. Parser reads NET header + specular(12) + emissive(12) + shininess(4) + alpha(4) + optional emissive_mult(4) = 36 bytes of body when `compact_material()` returns true (which skips `ambient` and `diffuse`).

The delta is stable at −20 bytes across all 24 occurrences, implying `stream.variant().compact_material()` returns `true` on a subset of FNV files whose on-disk layout actually contains the full `ambient(12) + diffuse(12)` pair.

## Impact

Affected materials default ambient/diffuse to `(0.5, 0.5, 0.5)` instead of file values. Note: distinct from #221, which is about the import layer discarding the colors. This bug is one layer below — the parser loses them entirely.

## Suggested Fix

Instrument `stream.variant().compact_material()` and dump header `bs_version` / `user_version` / variant for the 24 failing blocks. Most likely the gate needs to be tightened (FO4+ BGSM-driven materials only, not FO3/FNV Gamebryo-native NiMaterialProperty). See `crates/nif/src/version.rs`.

## Related

- #221 — ambient/diffuse discarded at import layer


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
