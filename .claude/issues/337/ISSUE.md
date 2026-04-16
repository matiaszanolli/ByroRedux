# D4-NEW-01: NiStencilProperty stencil state not mapped to Vulkan

## Finding: D4-NEW-01 (LOW)

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`
**Dimension**: Property → Material Mapping
**Games Affected**: Oblivion, FO3, FNV (pre-Skyrim meshes with stencil effects)
**Location**: `crates/nif/src/blocks/properties.rs:906-973` (parsed), `crates/nif/src/import/material.rs:472-477` (only `is_two_sided()` consumed)

## Description

NiStencilProperty is fully parsed with all fields (stencil_enabled, stencil_function, stencil_ref, stencil_mask, fail/z_fail/pass actions, draw_mode). However, only `is_two_sided()` is consumed by the importer. The actual stencil test/write parameters are discarded. No Vulkan stencil pipeline variant exists.

## Impact

>95% of NiStencilProperty usage is for two-sided rendering (which works). Stencil-masked decals, portals, and stencil shadow volumes in some Oblivion interiors render incorrectly.

## Suggested Fix

When stencil shadow volumes or portal effects are targeted: extract stencil params into Material, create Vulkan stencil pipeline variants. Low priority given the rarity.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`._
