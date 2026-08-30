# #3545: SK-D2-02: three shader_flags.rs doc comments mis-attribute Skyrim SLSF2 bit 21 as Cloud_LOD — nif.xml says bit 20

**Source**: `docs/audits/AUDIT_SKYRIM_2026-08-30.md` — Dimension 2 (Shader-Type Dispatch)
**Severity**: LOW
**Location**: `crates/nif/src/shader_flags.rs` — `fo4_slsf2` module doc, `fo4_slsf2::ANISOTROPIC_LIGHTING` doc, and `fo3nv_f2::ALPHA_DECAL` doc

## Description

Three doc comments in `shader_flags.rs` attribute Skyrim SLSF2 **bit 21** to `Cloud_LOD`.
Per nif.xml `SkyrimShaderPropertyFlags2`, bit 20 is `Cloud_LOD` and **bit 21 is
`Anisotropic_Lighting`** — the same semantic FO4 has. Only FO3/FNV diverges at bit 21
(`Alpha_Decal`).

## Evidence

Verified against current code (2026-08-30):

- `fo4_slsf2` module doc: *"Bit 21 is `Anisotropic_Lighting` on FO4 (Skyrim: `Cloud_LOD`,
  FO3/FNV F2: `Alpha_Decal` — **three different semantics on the same bit across games**)"*
- `fo4_slsf2::ANISOTROPIC_LIGHTING` doc: *"Distinct from `Cloud_LOD` (Skyrim) and
  `Alpha_Decal` (FO3/FNV) at the same numeric value."*
- `fo3nv_f2::ALPHA_DECAL` doc: *"**Warning**: bit 21 on Skyrim SLSF2 is `Cloud_LOD`, NOT
  decal"*

The **constants are correct** and contradict all three comments:
`skyrim_slsf2::CLOUD_LOD = 0x0010_0000` (bit 20) and
`skyrim_slsf2::ANISOTROPIC_LIGHTING = 0x0020_0000` (bit 21). The in-file test
`f2_bit_21_alpha_decal_legacy_vs_anisotropic_modern` asserts exactly the correct mapping and
its own comment states it correctly — so the code, the constants and the test all agree, and
only these three prose comments dissent.

Corpus context: measured over 85,104 `BSLightingShaderProperty` blocks in
`Skyrim - Meshes0/1.bsa`, every constant in `skyrim_slsf1` / `skyrim_slsf2` matches nif.xml
bit-for-bit (SLSF1 bits 4, 5, 12, 15, 16, 22, 26, 27, 30, 31; SLSF2 bits 4, 6, 17, 20, 21,
25, 26, 27, 30).

## Impact

Doc-rot on a comment block whose entire purpose is to be **the** cross-game flag reference.
The #414 conclusion the docs support — "a legacy `is_decal_from_shader_flags` that tests
`flags2 & 0x0020_0000` must not run on a Skyrim+/FO4 property" — still holds, but for a
two-way, not three-way, reason. A future reader trusting the prose would look for Skyrim
`Cloud_LOD` behaviour at the wrong bit.

## Suggested Fix

Correct all three comments: bit 21 is `Anisotropic_Lighting` on **both** Skyrim and FO4;
only FO3/FNV F2 diverges (`Alpha_Decal`). Skyrim's `Cloud_LOD` is bit 20.

## Related

#414 (cross-game F2 flag divergence).

## Completeness Checks
- [ ] **SIBLING**: sweep the other flag modules for the same three-way phrasing
- [ ] **TESTS**: `f2_bit_21_alpha_decal_legacy_vs_anisotropic_modern` already pins the correct mapping — confirm the corrected prose matches it
