// Addressing for the local-grid irradiance field (tracking #6531, Phase 3
// #6535). One image holds two froxel grids side by side, so it is twice the
// froxel width:
//   x in [0, W)   radiance: rgb = summed local-light radiance, a = sun visibility
//   x in [W, 2W)  light direction: xyz = radiance-weighted mean PROPAGATION
//                 direction of those lights (its length is how coherent they
//                 are: 1 = one direction, 0 = they cancel), w unused
// Written by the inject pass, read by composite's fire march. Filtering stays
// inside a half: the sample is inset to the half's outermost texel centres, so
// a bilinear tap at an edge never blends radiance with direction.
#ifndef BYRO_LOCAL_IRRADIANCE_GLSL
#define BYRO_LOCAL_IRRADIANCE_GLSL

// `uvw` is a coordinate in one froxel grid, [0,1]^3; `widthTexels` the image's
// full width (twice the grid's); `halfIndex` 0.0 for radiance, 1.0 for direction.
vec3 localIrradianceAddress(vec3 uvw, int widthTexels, float halfIndex) {
    float texelsPerHalf = float(widthTexels) * 0.5;
    float u = clamp(uvw.x, 0.5 / texelsPerHalf, 1.0 - 0.5 / texelsPerHalf);
    return vec3((u + halfIndex) * 0.5, uvw.yz);
}

#endif
