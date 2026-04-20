# Issue #454

FO3-REN-M3: BSShaderNoLightingProperty decal detection skips ALPHA_DECAL_F2 (0x00200000)

---

## Severity: Medium

**Location**: `crates/nif/src/import/material.rs:660`

## Problem

`BSShaderNoLightingProperty` decal detection (line 660):
```rust
if shader.shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0 {
    info.is_decal = true;
}
```

Omits the `ALPHA_DECAL_F2` (flag2 bit 21, 0x00200000) check. The PPLighting branch at line 646-650 has the same code plus the flag2 test:
```rust
if shader.shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0
    || shader.shader.shader_flags_2 & ALPHA_DECAL_F2 != 0
{
    info.is_decal = true;
}
```

Same flag vocabulary for both shader subclasses — inconsistent handling.

## Impact

Rare on FO3 (NoLighting is mostly skybox / UI), but real for blood-splat meshes that use NoLighting + alpha-decal-only flag declaration.

## Fix

Mirror the flag2 clause:
```rust
if shader.shader.shader_flags_1 & (DECAL_SINGLE_PASS | DYNAMIC_DECAL) != 0
    || shader.shader.shader_flags_2 & ALPHA_DECAL_F2 != 0
{
    info.is_decal = true;
}
```

## Completeness Checks

- [ ] **TESTS**: Synthetic NoLighting block with only flag2 ALPHA_DECAL set → importer flags `is_decal`
- [ ] **SIBLING**: Confirm PPLighting and NoLighting decal logic stay in sync going forward — consider a shared helper `fn is_decal(flags1, flags2) -> bool`

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-REN-M3)
