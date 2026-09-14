#ifndef BYRO_SKY_SH
#define BYRO_SKY_SH
// Real orthonormal SH, l <= 2, in renderer world space. Basis/order follows
// Ramamoorthi & Hanrahan (SIGGRAPH 2001), An Efficient Representation for
// Irradiance Environment Maps. Projection and reconstruction share this code.
float sky_sh_basis(uint i, vec3 n) {
    switch (i) {
        case 0u: return 0.2820947918;
        case 1u: return 0.4886025119 * n.y;
        case 2u: return 0.4886025119 * n.z;
        case 3u: return 0.4886025119 * n.x;
        case 4u: return 1.0925484306 * n.x * n.y;
        case 5u: return 1.0925484306 * n.y * n.z;
        case 6u: return 0.3153915653 * (3.0 * n.z * n.z - 1.0);
        case 7u: return 1.0925484306 * n.x * n.z;
        default: return 0.5462742153 * (n.x * n.x - n.y * n.y);
    }
}
// Clamped-cosine convolution divided by PI: these coefficients reconstruct
// the outgoing radiance of a unit-albedo Lambertian, not raw irradiance.
float sky_sh_lambert_band(uint i) {
    return i == 0u ? 1.0 : (i < 4u ? 2.0 / 3.0 : 0.25);
}
#endif
