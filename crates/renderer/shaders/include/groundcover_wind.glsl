#ifndef BYRO_GROUNDCOVER_WIND_GLSL
#define BYRO_GROUNDCOVER_WIND_GLSL

// EXAL ground cover — the shared wind evaluation (§8).
//
// One function turns `WindField` into a bend fraction at a world point and a
// time, so every consumer of the field bends from the same gust wave: the
// blade ribbons (`groundcover_blade.vert`) and the authored-model plants
// (`groundcover_models.comp`, #4921). A second copy would let the two drift
// apart, and a fern would then lean against the blades around it.
//
// Requires `shader_constants.glsl` and `groundcover_density.glsl` (for
// `byroGcFbm`) to be included first.

/// Bend at `baseXZ` and `timeS`, as a fraction of height in
/// `[0, GROUNDCOVER_WIND_MAX_BEND]`.
///
/// `wind` is the push-constant packing every consumer shares: xy = unit
/// direction the wind blows toward, z = speed, w = gust amplitude. The flow
/// noise is sampled at the base *advected downwind*, so neighbours bend
/// together and a gust front travels across the field instead of
/// shimmering. `phase` is the consumer's per-plant offset in radians, which
/// keeps a stand out of lockstep; `stiffness` in [0, 1] resists the bend.
///
/// `secondaryWave` is the harmonic's sine, handed back because the blades
/// derive their lateral sway from it and must not re-derive the clock.
float byroGcWindBendFraction(
    vec4 wind,
    float gustFrequency,
    vec2 baseXZ,
    float phase,
    float stiffness,
    float timeS,
    out float secondaryWave
) {
    vec2 windDir = wind.xy;
    float windSpeed = wind.z;
    vec2 advected = baseXZ - windDir
        * (windSpeed * GROUNDCOVER_WIND_ADVECTION_SCALE * timeS);
    float flow = byroGcFbm(advected / GROUNDCOVER_WIND_NOISE_UNITS);
    float gustClock = timeS * gustFrequency * GROUNDCOVER_TWO_PI;
    float primaryWave = sin(gustClock + phase);
    secondaryWave = sin(
        gustClock * GROUNDCOVER_WIND_HARMONIC_FREQUENCY_MULTIPLIER
        + phase * GROUNDCOVER_WIND_HARMONIC_FREQUENCY_MULTIPLIER);
    float gust = 1.0 + wind.w * (
        primaryWave + GROUNDCOVER_WIND_SECONDARY_AMPLITUDE * secondaryWave);
    float bendFraction = clamp(windSpeed / GROUNDCOVER_MAX_WIND_SPEED, 0.0, 1.0)
        * mix(GROUNDCOVER_WIND_FLOW_FLOOR, 1.0, clamp(flow, 0.0, 1.0))
        * gust * (1.0 - GROUNDCOVER_WIND_STIFFNESS_ATTENUATION * stiffness);
    return clamp(bendFraction, 0.0, 1.0) * GROUNDCOVER_WIND_MAX_BEND;
}

#endif // BYRO_GROUNDCOVER_WIND_GLSL
