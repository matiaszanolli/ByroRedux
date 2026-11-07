#ifndef BYRO_ALPHA_COMPARE_GLSL
#define BYRO_ALPHA_COMPARE_GLSL

// The NiAlphaProperty / D3D alpha-test comparison table: the ONE definition, shared by the raster path
// (triangle.frag), every secondary ray (ray_hit.glsl) and the froxel sun trace (volumetrics_inject.comp).
// It needs nothing else, so a shader that cannot include bindings.glsl can still use it.
bool alphaComparePass(float alpha, float threshold, uint func) {
    if (threshold <= 0.0 || func == 0u) return true;
    if (func == 1u) return alpha < threshold;
    if (func == 2u) return abs(alpha - threshold) < (1.0 / 255.0);
    if (func == 3u) return alpha <= threshold;
    if (func == 4u) return alpha > threshold;
    if (func == 5u) return abs(alpha - threshold) >= (1.0 / 255.0);
    if (func == 6u) return alpha >= threshold;
    return false; // NEVER
}

#endif
