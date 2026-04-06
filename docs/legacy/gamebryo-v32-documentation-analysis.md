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

### HIGH Priority (Animation)

6. **BSpline interpolator parsing** — NiBSplineCompTransformInterpolator and friends.
   Required for Oblivion/Skyrim compressed animations. Compact decompression:
   `value = offset + (short / 32767) * half_range`. Degree 3 only.

7. **Blending algorithm review** — Only top 2 priority levels participate, not all.
   Spinner (ease-in/out) is separate from weight. Additive mode exists.

8. **"NonAccum" root motion detection** — Accumulation root identified by child named
   `parent_name + " NonAccum"`. Needed for proper root motion extraction.

### LOW Priority

9. **NiDataStream RTTI delimiter** — Type names can contain a delimiter character for
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

## Animation System Findings

### Blending Algorithm (Critical)
- Only top TWO priority levels participate in blending (not all priorities)
- "Spinner" (ease-in/out ramp 0-1) is a separate multiplier from weight
- Additive blending exists: computes (current - reference_frame) delta, weights NOT normalized
- Higher priority additive sequences applied first (rotation order matters)

### BSpline Compressed Animation (Missing Parser)
- Open uniform B-Spline degree 3 (cubic), uniform time spacing
- Two modes: float (full precision) and compact (16-bit short)
- Compact decompression: `value = offset + (short_value / 32767.0) * half_range`
- NiBSplineData stores control point arrays, NiBSplineBasisData stores basis params
- Types: NiBSplineCompTransformInterpolator (translation/rotation/scale channels)
- Used in Oblivion and Skyrim compressed animations

### Transform Conventions (Confirmed)
- NiMatrix3 rotations are CW (clockwise), NiQuaternion is CCW (counter-clockwise)
- Matrix/quaternion chirality mismatch is intentional — conversion must account for this
- Uniform scale only (scalar, not per-axis)

### KF File Format Details
- KF files are standard NIF streams with NiControllerSequence as root
- "NonAccum" naming convention identifies accumulation root for root motion
- Per-axis accumulation flags (X, Y, Z) for rotation and translation
- Text keys: "morph:" prefix for sync points, arbitrary event strings

### Morph System (Two Generations)
- Legacy: NiGeomMorpherController + NiMorphData (Oblivion/FNV — we parse these)
- Modern: NiMorphMeshModifier + NiMorphWeightsController (v2.5+ — NIF block exists)
- Targets can be absolute or relative (offsets from base mesh)
- Weights for ALL targets exported, even 0.0 — affects blending normalization

## Source Files Referenced

- Object_Systems/Streaming.htm, Streaming_Internals.htm
- General_Topics/How_NIF_File_I_O_Works.htm, BackgroundLoading.htm
- NiParticle/ (all files)
- NiMesh/ (mesh modifier pipeline, data streams, skin partitions)
- NiMaterial/ (standard material, texture slots, property inheritance)
- Texturing/ (NiTexturingProperty, Apply Mode, Coordinate Transform)
- NiShader/ (NSF/FX, shader constants)
- Frame_Rendering_System/ (accumulator sorting)
