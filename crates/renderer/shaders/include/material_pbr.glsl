#ifndef BYRO_MATERIAL_PBR_GLSL
#define BYRO_MATERIAL_PBR_GLSL

// A surface's base roughness and metalness (SMM-PBR-001/002/004, #7422). A
// material with a roughness or metalness map (`GpuMaterial.roughnessMapIndex`,
// `metalnessMapIndex` != 0) takes that value from the map's red channel; every
// other material keeps its scalar. The one definition, shared by the visible
// surface and every traced hit that shades with the two values
// (triangle.frag). A mapped roughness obeys the floor the CPU resolve puts on
// the scalar.
//
// Include after the shader declares `textures[]` and `GpuMaterial`.

// The visible surface: implicit derivatives, as its other maps are sampled.
void applyMaterialPbrMaps(
    GpuMaterial mat, vec2 uv, inout float roughness, inout float metalness
) {
    if (mat.roughnessMapIndex != 0u) {
        float s = texture(textures[nonuniformEXT(mat.roughnessMapIndex)], uv).r;
        roughness = clamp(s, MATERIAL_MIN_ROUGHNESS, 1.0);
    }
    if (mat.metalnessMapIndex != 0u) {
        metalness = texture(textures[nonuniformEXT(mat.metalnessMapIndex)], uv).r;
    }
}

// A traced hit: the maps at the hit's texture coordinate, written into the
// caller's copy of the record so all later hit shading reads them. A ray has no
// derivatives: it reads the finest resident level, as its other hit samples do.
void applyRayHitPbrMaps(inout GpuMaterial mat, vec2 uv) {
    if (mat.roughnessMapIndex != 0u) {
        float s = textureLod(textures[nonuniformEXT(mat.roughnessMapIndex)], uv, 0.0).r;
        mat.roughness = clamp(s, MATERIAL_MIN_ROUGHNESS, 1.0);
    }
    if (mat.metalnessMapIndex != 0u) {
        mat.metalness = textureLod(textures[nonuniformEXT(mat.metalnessMapIndex)], uv, 0.0).r;
    }
}

#endif
