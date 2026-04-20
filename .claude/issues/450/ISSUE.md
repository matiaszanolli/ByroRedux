# Issue #450

FO3-NIF-M1: NiTexturingProperty parallax slot 7 read + discarded

---

## Severity: Medium

**Location**: `crates/nif/src/blocks/properties.rs:287-293`

## Problem

```rust
if is_v20_2_0_5_plus && texture_count > 7 {
    let parallax = Self::read_tex_desc(stream)?;
    if parallax.is_some() {
        let _parallax_offset = stream.read_f32_le()?;
    }
}
```

The parallax `TexDesc` is read into a local binding and immediately dropped. `_parallax_offset` is discarded. `NiTexturingProperty` has no `parallax_texture` field (contrast `normal_texture` at line 280 which IS persisted).

Stream alignment is fine — the slot is consumed. This is a **capture gap**, not a corruption bug.

## Impact

FO3 meshes that keep `NiTexturingProperty` alongside `BSShaderPPLightingProperty` (rarer on FO3 than Oblivion but non-zero on ported/mixed clutter) lose their parallax height map.

## Reference

- nif.xml `NiTexturingProperty.Parallax Texture` (since 20.2.0.5)
- Gamebryo 2.3 `NiTexturingProperty::LoadBinary` preserves the parallax map.

## Fix

1. Add `pub parallax_texture: Option<TexDesc>` and `pub parallax_offset: f32` to `NiTexturingProperty`.
2. Persist the read values (currently dropped).
3. Wire through `crates/nif/src/import/material.rs` alongside `normal_texture` extraction.

## Completeness Checks

- [ ] **TESTS**: Synthetic v20.2.0.5 `NiTexturingProperty` with parallax slot → struct holds the TexDesc
- [ ] **SIBLING**: Verify `normal_texture` import path flows all the way to `MaterialInfo` — parallax path mirrors it
- [ ] **DOCS**: nif.xml reference note at the fix site

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-NIF-M1)
