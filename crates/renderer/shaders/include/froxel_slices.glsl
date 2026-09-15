// M55 volumetrics — the hybrid linear/exponential froxel Z-slice mapping.
//
// NON-STANDALONE shader fragment. Included by volumetrics_inject.comp,
// volumetrics_integrate.comp and composite.frag.
//
// #4346 — the mapping was copied four times across those three shaders, with
// only the uniform member names differing. The host mirror is
// `hybrid_slice_distance` / `hybrid_slice_coordinate` in
// `crates/renderer/src/vulkan/volumetrics.rs`, which uses the same floors as
// this file (it used to floor at 1e-4, so it could not catch a shader drift).
//
// `grid` = (far distance, linear-depth floor, fraction of the slices assigned
// to the linear region), all distances in world units.
#ifndef BYRO_FROXEL_SLICES_GLSL
#define BYRO_FROXEL_SLICES_GLSL

// Normalized slice coordinate in [0, 1] -> distance along the view ray.
float froxelSliceDistance(float normalizedSlice, vec3 grid) {
    float u = clamp(normalizedSlice, 0.0, 1.0);
    float farDistance = max(grid.x, 1.0);
    float linearDepth = clamp(grid.y, 1.0, farDistance);
    float linearFraction = clamp(grid.z, 1.0e-4, 0.9999);
    if (u <= linearFraction) {
        return linearDepth * (u / linearFraction);
    }
    float q = (u - linearFraction) / (1.0 - linearFraction);
    return linearDepth * pow(farDistance / linearDepth, q);
}

// Distance along the view ray -> normalized slice coordinate in [0, 1].
float froxelSliceCoordinate(float distanceAlongRay, vec3 grid) {
    float farDistance = max(grid.x, 1.0);
    float linearDepth = clamp(grid.y, 1.0, farDistance);
    float linearFraction = clamp(grid.z, 1.0e-4, 0.9999);
    float d = clamp(distanceAlongRay, 0.0, farDistance);
    if (d <= linearDepth) {
        return linearFraction * (d / linearDepth);
    }
    return linearFraction + (1.0 - linearFraction)
        * log(d / linearDepth) / log(farDistance / linearDepth);
}

#endif // BYRO_FROXEL_SLICES_GLSL
