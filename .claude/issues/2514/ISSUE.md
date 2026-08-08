# REN-D21-2026-08-07-02: subsurface/sheen/sheen_tint/anisotropic are hardcoded to zero in the draw path, so no scene can drive them

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2514
**Finding ID**: REN-D21-2026-08-07-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 21 — Cornell Harness
**Location**: `byroredux/src/render/static_meshes.rs::collect_static_mesh_draws` (lines ~627-633)
**Status**: NEW

## Description
The `DrawCommand` construction writes `subsurface: 0.0, sheen: 0.0, sheen_tint: 0.0, anisotropic: 0.0` as literals with a "when the importer surfaces them" TODO. `GpuMaterial` carries the fields, `hash_gpu_material_fields` hashes them, `include/pbr.glsl` and `lighting.glsl` consume them — but no CPU producer can ever make them non-zero, from game content or from the harness. This is the enabling half of the sibling REN-D21-2026-08-07-01 finding: even if `MAT_FLAG_PBR_BSDF` were set on a Cornell probe, `disneyDiffuseSplit` would run with all three of its distinguishing parameters pinned at zero and degenerate back toward Burley-only.

## Evidence
Literals at `static_meshes.rs:627-633`; the only non-zero writer of `GpuMaterial::subsurface` in the tree is `presets::skin_wax_marble()` in `crates/renderer/src/vulkan/material.rs`, a test/reference fixture with no render-path caller.

## Impact
Three shipped shader features (fake-SSS, sheen, anisotropic GGX) are dead code end-to-end with no runtime signal that they are inert; #1249/#1250 read as delivered from the shader side alone. Blast radius is limited to those lobes.

## Related
REN-D21-2026-08-07-01 (this report — same root gap seen from the harness side); #1249, #1250.

## Suggested Fix
Plumb the four scalars from `Material` (adding the fields if absent) through `DrawCommand`, then expose them via `mat.set` so Cornell can sweep them; until then, mark the shader-side lobes explicitly as unreachable in their doc comments.

## Completeness Checks
- [ ] **SIBLING**: File and fix alongside REN-D21-2026-08-07-01 (this report) — same `mat.set` extension covers both
- [ ] **TESTS**: Cornell harness gains a probe row driving non-zero subsurface/sheen/anisotropic values
