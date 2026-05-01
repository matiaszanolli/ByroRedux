# Issue #773: FO3-4-PPMAT — BSShaderPPLightingProperty texture_clamp_mode + env_map_scale dropped at MaterialInfo mirror

**Severity**: HIGH · **Domain**: nif-parser, renderer · **Type**: bug
**Source audit**: docs/audits/AUDIT_FO3_2026-05-01.md
**Game affected**: Fallout 3, Fallout NV (shared PPLighting code path)
**Bundles**: FO3-4-01 + FO3-4-02

## Summary

Two single-line MaterialInfo assignments missing at `crates/nif/src/import/material/walker.rs:471-538` (the FO3/FNV PPLighting branch):

1. `info.texture_clamp_mode = shader.texture_clamp_mode as u8;`
2. `info.env_map_scale = shader.shader.env_map_scale;`

Both fields parse correctly on disk (shader.rs:48 + base.rs:180) but never reach the renderer. Visible regression: CLAMP-authored decals render with WRAP edge-bleed; env-cube-mapped surfaces render with `env_map_scale=0.0` (zero reflection intensity even with valid env cube bound).

The Skyrim+ and BSEffectShader paths already capture both fields; only the FO3/FNV PPLighting branch is missing.
