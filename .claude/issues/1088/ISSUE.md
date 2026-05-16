# Issue #1088

**Audit**: AUDIT_RENDERER_2026-05-15
**Status**: FIXED (2026-05-16)

## Summary

After #992 migrated `MESH_ID_FORMAT` from `R16_UINT` to `R32_UINT`, the
`ALPHA_BLEND_NO_HISTORY` flag moved from bit 15 to bit 31 (mask
`0x80000000u`; instance-id mask `0x7FFFFFFFu`). The code paths were
updated, but two narrative comments still referenced the pre-#992 "bit 15"
position, contradicting the surrounding code.

## Changes

1. `crates/renderer/shaders/caustic_splat.comp` (line 121)
   - `// Mask off the ALPHA_BLEND_NO_HISTORY flag (bit 15) that`
   - `// Mask off the ALPHA_BLEND_NO_HISTORY flag (bit 31) that`

2. `crates/renderer/shaders/svgf_temporal.comp` (line 80)
   - `// Alpha-blend fragments (mesh_id bit 15 set by triangle.frag) take`
   - `// Alpha-blend fragments (mesh_id bit 31 set by triangle.frag) take`

The historically-accurate reference in `caustic_splat.comp` line 132
("Pre-#992 this was an R16_UINT format with bit 15 as the alpha-blend
marker, capping the instance count at 32767") was deliberately left
untouched - it describes the previous encoding, not the current one.

## Verification

- Recompiled both shaders to SPIR-V via `glslangValidator -V` - no
  errors.
- `cargo check` clean (only pre-existing unrelated warnings).
