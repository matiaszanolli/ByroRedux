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

// ── Local gusts ─────────────────────────────────────────────────────────
//
// Line-for-line mirror of `groundcover::local_gust` in core (see the constants'
// block there for the model); the SpeedTree sway reads the Rust copy, so the
// two must stay in step.
uint byroGcGustMix(uint x) {
    x ^= x >> 16;
    x *= 0x7feb352du;
    x ^= x >> 15;
    x *= 0x846ca68bu;
    x ^= x >> 16;
    return x;
}

float byroGcGustUnit(ivec2 cell, uint salt) {
    uint h = byroGcGustMix(uint(cell.x) ^ byroGcGustMix(uint(cell.y) ^ (salt * 0x9E3779B9u)));
    return float(h >> 8) * (1.0 / 16777216.0);
}

/// The local gust at `baseXZ` and `timeS`, in [0, 1]; 0 = no gust here now.
float byroGcLocalGust(vec4 wind, vec2 baseXZ, float timeS) {
    vec2 dir = wind.xy;
    float along = dot(baseXZ, dir);
    float across = -baseXZ.x * dir.y + baseXZ.y * dir.x;
    float drift = max(wind.z, GROUNDCOVER_GUST_MIN_DRIFT_SPEED);
    float u = along - drift * GROUNDCOVER_WIND_ADVECTION_SCALE * timeS;
    vec2 q = vec2(u, across) / GROUNDCOVER_GUST_CELL_UNITS;
    ivec2 cell = ivec2(floor(q));
    vec2 f = q - vec2(cell);

    float clock = timeS / GROUNDCOVER_GUST_PERIOD_S
        + byroGcGustUnit(cell, GROUNDCOVER_GUST_PHASE_SALT);
    float slot = floor(clock);
    float phase = clock - slot;
    uint salt = uint(int(slot)) * 4u;
    if (byroGcGustUnit(cell, salt) >= GROUNDCOVER_GUST_PROBABILITY) {
        return 0.0;
    }
    float seconds = phase * GROUNDCOVER_GUST_PERIOD_S;
    if (seconds >= GROUNDCOVER_GUST_DURATION_S) {
        return 0.0;
    }
    vec2 centre = vec2(
        0.3 + 0.4 * byroGcGustUnit(cell, salt + 1u),
        0.3 + 0.4 * byroGcGustUnit(cell, salt + 2u));
    float d = length(f - centre) / GROUNDCOVER_GUST_RADIUS_FRACTION;
    float spatial = max(1.0 - d * d, 0.0);
    spatial *= spatial;
    float envelope = sin(3.14159265358979 * seconds / GROUNDCOVER_GUST_DURATION_S);
    float amplitude = 0.6 + 0.4 * byroGcGustUnit(cell, salt + 3u);
    return clamp(envelope * envelope * spatial * amplitude, 0.0, 1.0);
}

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
    // Local gust: a short, localised surge on top of the steady strength, so one
    // patch bends harder than its neighbours for a moment. It scales the
    // steady strength plus a calm-air floor (`gusted_strength` in core), so
    // every weather in every game gets gusts and only their intensity varies.
    float localGust = byroGcLocalGust(wind, baseXZ, timeS);
    float steadyStrength = clamp(windSpeed / GROUNDCOVER_MAX_WIND_SPEED, 0.0, 1.0);
    float gustedStrength = clamp(
        steadyStrength * (1.0 + GROUNDCOVER_GUST_GAIN * localGust)
            + GROUNDCOVER_GUST_CALM_FLOOR * localGust,
        0.0, 1.0);
    float bendFraction = gustedStrength
        * mix(GROUNDCOVER_WIND_FLOW_FLOOR, 1.0, clamp(flow, 0.0, 1.0))
        * gust * (1.0 - GROUNDCOVER_WIND_STIFFNESS_ATTENUATION * stiffness);
    return clamp(bendFraction, 0.0, 1.0) * GROUNDCOVER_WIND_MAX_BEND;
}

#endif // BYRO_GROUNDCOVER_WIND_GLSL
