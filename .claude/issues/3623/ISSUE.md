# #3623: OBL-D1-02: dark/detail/gloss/glow TexDesc presence gated on texture_count, but nif.xml gates none of them

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 1 (NIF Version Handling)
**Severity**: LOW
**Location**: `crates/nif/src/blocks/properties.rs` — `NiTexturingProperty::parse`, the dark / detail / gloss / glow TexDesc reads

## Description

The dark, detail, gloss and glow texture slots are gated on `texture_count > 1..>4`. nif.xml
gates **none** of them — their `Has …` presence bools are unconditional; only Bump
(`> 5`), Normal (`> 6`), Parallax (`> 7`) and the Decals carry `Texture Count` conditions.

## Evidence

Current code (verified 2026-08-30):

```rust
let dark_texture   = if texture_count > 1 { Self::read_tex_desc(stream)? } else { None };
let detail_texture = if texture_count > 2 { ... };
let gloss_texture  = if texture_count > 3 { ... };
let glow_texture   = if texture_count > 4 { ... };
let bump_texture   = if texture_count > 5 { ... };
```

nif.xml (lines 5237-5245):

```xml
<field name="Has Dark Texture"   type="bool" />
<field name="Has Detail Texture" type="bool" />
<field name="Has Gloss Texture"  type="bool" />
<field name="Has Glow Texture"   type="bool" />
<field name="Has Bump Map Texture" type="bool" since="3.3.0.13" cond="Texture Count #GT# 5" />
```

The Bump/Normal/Parallax/Decal conditions the code applies are correct and spec-backed; the
four above are not in the spec.

**Measured exposure on Oblivion: zero — `texture_count == 7` on all 30,121
`NiTexturingProperty` instances in the archive** (a single-valued histogram), so all four
gates always pass.

## Impact

Latent, but the divergence from spec is real: a file with `texture_count < 5` would skip the
presence bools entirely, giving 1 byte of drift per skipped slot and total misalignment
downstream. Reachable via mod content or non-Bethesda NetImmerse assets.

## Suggested Fix

Read the four `Has …` bools unconditionally as nif.xml declares, and keep the
`Texture Count` conditions only where the spec puts them (Bump, Normal, Parallax, Decals).

## Related

OBL-D1-01 (the missing `since` bound in the same parser). #2565 (`OBL-D1-04: Two latent
TexDesc version gaps`) is a different pair of gaps in the TexDesc readers, not this one.

## Completeness Checks
- [ ] **SIBLING**: the Decal 0-3 arms carry version-split conditions (`until=20.2.0.4` vs `since=20.2.0.5`) — confirm both bands are transcribed while touching this function
- [ ] **TESTS**: a regression test pins a synthetic `texture_count < 5` property parsing to the right stream position
