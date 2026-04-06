# Gamebryo v3.2 Documentation Analysis — NIF Parser Findings

Analyzed 4559 HTML docs across Object_Systems, NiParticle, NiMesh, NiMaterial,
NiShader, Texturing, Frame_Rendering_System, NiFloodgate, and Learn sections.

---

## Confirmed Correct (No Changes Needed)

1. **Three-phase load** — LoadBinary → LinkObject → PostLinkObject. Our collapsed phases 1+2 valid for read-only.
2. **String table format** — version threshold at 20.1.0.3. Inline before, string table after.
3. **BlockRef as i32** with -1 sentinel for null.
4. **Object groups** — read and discard (runtime memory optimization).
5. **Block type names** match RTTI class names exactly.
6. **Sequential block parsing** in index order is correct.
7. **Particle systems not serialized** — opaque byte-consumption is the right strategy.
8. **NiPSysModifierBase** field layout: name + order + target + active.
9. **Texture slot numbering** 0-7 (base through parallax) with decals at 8+.
10. **Skin partition structure** — vertex map, bone indices as u8, per-vertex weights.
11. **Bump texture extra fields** — luma scale, luma offset, 2x2 matrix.

## Fixed This Session

1. **Endianness validation** — reject big-endian NIFs instead of silent corruption.
2. **bhkCompressedMeshShape vertex count** — /3 for u16 component→triple.
3. **bhkCompressedMeshShape dequantization** — offset + vertex * error, not /1000.
4. **bhkCompressedMeshShape indices** — /3 for pre-multiplied vertex references.

## Action Items for Future Work

### HIGH Priority

1. **Property inheritance during import** — Material properties on parent NiNodes should
   propagate to child geometry. Current import only reads properties from geometry nodes.
   Location: `crates/nif/src/import/material.rs`

2. **Skin partition strip→triangle conversion** — Currently skips strip data.
   Location: `crates/nif/src/blocks/skin.rs` line ~270

### MEDIUM Priority

3. **Morph before skinning** — ECS pipeline must apply morph targets before skeletal
   deformation. Noted for M29.

4. **BSTriShape vertex_desc decoding** — The u64 bitfield determines which vertex
   components exist and their half-float packing. Already handled in tri_shape.rs but
   skin partition's vertex_desc is skipped.

5. **BLENDINDICES BGRA swizzle** — Some NIFs store bone indices in NORMUINT8_4_BGRA
   format (swizzled). Need to check BSTriShape skinning data for this.

### LOW Priority

6. **NiDataStream RTTI delimiter** — Type names can contain a delimiter character for
   polymorphic class arguments. Not relevant for Bethesda NIFs.

7. **PostProcessFunction callbacks** — Can convert object types at load time. Explains
   legacy type name appearance in newer files.

8. **Texture combination order** — Standard material applies: base * dark * detail + 
   lighting + glow + decals, env map modulated by gloss. Relevant for shader fidelity.

9. **NiPS* (new particle system)** — v2.5+ replacement for NiPSys*. Only needed for
   non-Bethesda Gamebryo content (Civilization IV, etc.).

## Key Texture Slot Reference

| Idx | Slot | Blend Mode |
|-----|------|-----------|
| 0 | Base (Diffuse) | Modulate with vertex color |
| 1 | Dark | Multiply base (baked lightmap) |
| 2 | Detail | Multiply 2x (close-up sharpening) |
| 3 | Gloss | Modulates env map + specular |
| 4 | Glow | Additive (self-illumination) |
| 5 | Bump | Bump environment mapping |
| 6 | Normal | Tangent-space normal map |
| 7 | Parallax | Height-based UV offset |
| 8+ | Decals | Overlay (up to 4) |

## Source Files Referenced

- Object_Systems/Streaming.htm, Streaming_Internals.htm
- General_Topics/How_NIF_File_I_O_Works.htm, BackgroundLoading.htm
- NiParticle/ (all files)
- NiMesh/ (mesh modifier pipeline, data streams, skin partitions)
- NiMaterial/ (standard material, texture slots, property inheritance)
- Texturing/ (NiTexturingProperty, Apply Mode, Coordinate Transform)
- NiShader/ (NSF/FX, shader constants)
- Frame_Rendering_System/ (accumulator sorting)
