# FO4-D3-M1: No SLSF1_/SLSF2_ named bitflags — FO4 flag bits tested via raw hex literals

**Issue**: #414 — https://github.com/matiaszanolli/ByroRedux/issues/414
**Labels**: bug, nif-parser, renderer, medium

---

## Finding

`grep -r 'SLSF1_\|SLSF2_' crates/` returns **zero files**. The decal and alpha-test checks in the FO4 importer use raw hex literals — `shader_flags_2 & 0x10` and a single named constant `ALPHA_DECAL_F2` at `crates/nif/src/import/material.rs:452`.

## Flag bits not currently tested

`BSLightingShaderProperty.shader_flags_1` / `.shader_flags_2` are two distinct u32 masks on FO4 (correctly read per Dim 3 L-03). FO4 uses several bits beyond decal:
- Skinned (SLSF1 bit)
- Glow (SLSF1 bit)
- Cast shadows (SLSF1 bit)
- Alpha test (SLSF1 bit)
- Window environment mapping (SLSF2 bit)
- Refraction (SLSF2 bit)
- Parallax (SLSF1 bit)
- Facegen (SLSF1 bit)
- Subsurface scattering (SLSF1/SLSF2 — FO4 repurposes bits for PBR vs Skyrim)

The importer surfaces two distinct u32s (good) but only interprets one decal bit on SLSF2. **Skinned meshes, glow-mapped materials, and alpha-tested vegetation are currently miscategorized as opaque-static**, feeding the RT BLAS with wrong `cull_mode` and shading model.

## Impact

- **Alpha-test vegetation** (grass, tree leaves, chain-link fences) renders as opaque — visible regression on FO4 exteriors.
- **Glow-mapped weapons and terminals** don't light up in dark interiors.
- **Skinned meshes classified as static** — likely collides with the rigid-vs-skinned geometry routing.

## Fix

Introduce per-game `bitflags` enums. FO4 bit positions differ from Skyrim in several places; cross-reference nif.xml `#BS_FO4#` gating on `SLSF1`/`SLSF2`:

```rust
bitflags! {
    pub struct Slsf1Fo4: u32 {
        const SKINNED               = 0x0000_0002;
        const DECAL                 = 0x0400_0000;
        const ALPHA_TEST            = 0x0000_1000;
        const GLOW_MAP              = 0x0000_0400;
        const SPECULAR              = 0x0000_0001;
        // ... all FO4-documented bits ...
    }
}
```

Same for SLSF2. Replace raw hex with named bit tests in material.rs.

Skyrim has its own `Slsf1Skyrim` / `Slsf2Skyrim` with different bit positions for some fields (e.g. FO4 repurposes Skyrim subsurface bits for PBR semantics).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Requires Dim 3 M-02 doc note on BGSM stopcond boundary to stay consistent — flags apply to both branches.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Per-flag unit tests against a synthetic BSLightingShaderProperty at BSVER=130 with each bit set individually — verify the importer flips the expected ECS `Material` field.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 3 M-01.
