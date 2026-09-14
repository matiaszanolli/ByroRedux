// Shared participating-medium optics for weather clouds, fog, and combustion.
// Density/velocity/temperature belong to each producer. This layer consumes
// optical coefficients only; it knows no game, weather type, or volume shape.
// Hillaire, Physically Based and Unified Volumetric Rendering in Frostbite
// (SIGGRAPH 2015), sections 5.1 and 5.3.
#ifndef BYRO_MEDIUM_TRANSPORT
#define BYRO_MEDIUM_TRANSPORT

float medium_henyey_greenstein(float cos_theta, float g) {
    // Runtime-tunable g must remain inside the open interval (-1, 1).
    float g_safe = clamp(g, -0.999, 0.999);
    float g2 = g_safe * g_safe;
    float denom = 1.0 + g2 - 2.0 * g_safe * clamp(cos_theta, -1.0, 1.0);
    return (1.0 - g2) / (12.566370614359172 * pow(max(denom, 1e-4), 1.5));
}

// x = transmittance; y = integral of transmittance along the homogeneous slab.
// sigma_t and distance must use reciprocal units (metres OR world units).
// Multiplying y by source radiance per distance integrates scattering AND
// thermal emission, including an emitting vacuum with sigma_t == 0.
vec2 medium_slab(float sigma_t, float distance) {
    float sigma = max(sigma_t, 0.0);
    float ds = max(distance, 0.0);
    float tau = sigma * ds;
    float transmittance = exp(-tau);
    // Cancellation-safe vacuum limit; the quadratic term also keeps the
    // boundary continuous to floating-point precision at tau = 0.001.
    float integral = tau < 1e-3
        ? ds * (1.0 - 0.5 * tau + tau * tau / 6.0)
        : (1.0 - transmittance) / sigma;
    return vec2(transmittance, integral);
}
#endif
