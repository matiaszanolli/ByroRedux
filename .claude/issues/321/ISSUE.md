# Issue #321 — Caustics pass

- **Severity**: MEDIUM (visual quality, missing pipeline feature) | **URL**: https://github.com/matiaszanolli/ByroRedux/issues/321

Glass/water/refractive surfaces project zero caustic light onto their surroundings. Three approaches:

- **Option A (recommended)**: screen-space caustic splat — refract cluster lights through refractive surface normals, single ray per refractive pixel per important light, splat into a R11G11B10F screen-space caustic_buffer, composite reads it into `direct`. Sparse, uses existing G-buffer + SVGF infrastructure.
- **Option B**: photon-style emissive caustics into world-space hash texture. More expensive, more correct.
- **Option C**: image-based fake (Voronoi). Cheap, fake.

Touch points: new `caustic_splat.comp`, new `vulkan/caustic.rs` attachment, composite.frag addition, draw.rs dispatch. Reuses `isGlass`/`isWindow` classification from triangle.frag.
