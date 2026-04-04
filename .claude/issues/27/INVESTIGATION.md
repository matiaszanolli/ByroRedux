# Investigation: Issue #27

## Root Cause
pipeline.rs lines 105-121: both `rasterizer` and `rasterizer_no_cull` have
`cull_mode(CullModeFlags::NONE)`. The comment (lines 98-102) says culling was
disabled pending NIF winding verification.

## Winding Confirmed
camera.rs lines 40-41 confirm: "CW triangles (NIF/D3D) appear CCW after [Y-flip],
matching our front face setting." So COUNTER_CLOCKWISE front face + BACK culling
is correct for the standard rasterizer.

## Fix
- `rasterizer`: change NONE → BACK (backface culling for normal meshes)
- `rasterizer_no_cull`: keep NONE (for two-sided meshes like foliage, glass)
- Update the comment to note winding is now confirmed

## Scope
1 file: pipeline.rs. 2 lines changed + comment update.
