#ifndef BYRO_COVERAGE_ALPHA_GLSL
#define BYRO_COVERAGE_ALPHA_GLSL

// Where a coverage test gets its alpha (SMM-OPA-001/002, #7421). A material
// with an opacity map (`GpuMaterial.opacityMapIndex != 0`) takes it from that
// map's red channel; every other material keeps the base-colour alpha passed
// in. The one definition, shared by the visible surface (triangle.frag), every
// traced hit (ray_hit.glsl) and the froxel sun trace (volumetrics_inject.comp).
//
// Apply it after the base-colour alpha is final (the BC1 punch-through pin) and
// before material alpha, vertex alpha, decals and the alpha test.
//
// Include after the shader declares `textures[]`.

float coverageAlphaGrad(uint opacityMap, float baseAlpha, vec2 uv, vec2 dx, vec2 dy) {
    if (opacityMap == 0u) return baseAlpha;
    return textureGrad(textures[nonuniformEXT(opacityMap)], uv, dx, dy).r;
}

// A ray has no derivatives: it reads the finest resident level, as the
// base-colour coverage sample does.
float coverageAlphaLod(uint opacityMap, float baseAlpha, vec2 uv) {
    if (opacityMap == 0u) return baseAlpha;
    return textureLod(textures[nonuniformEXT(opacityMap)], uv, 0.0).r;
}

#endif
