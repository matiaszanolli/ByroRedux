#version 450

// EXAL ground cover — the Phase 1 distribution view (§9, #4054).
//
// "Debug point rendering of accepted candidates over real terrain. **This is
// where the distribution is judged, before any blade exists.**"
//
// Points are coloured by `d_ground`, not by acceptance, because the two
// questions the view has to answer are different. Whether a point exists
// answers "did the accept test fire here"; what colour it is answers "what did
// the field actually evaluate to", and the second is what tells a patchy
// result apart from a correct field being sampled too sparsely.
//
// Compile with:
//   glslangValidator -V -Icrates/renderer/shaders groundcover_debug.frag -o groundcover_debug.frag.spv

layout(location = 0) in vec3 vWorldPos;
layout(location = 1) in vec3 vWorldNormal;
layout(location = 2) in float vBladeT;
layout(location = 3) in float vDGround;
layout(location = 4) flat in uint vSpecies;
layout(location = 5) in float vColourJitter;

layout(location = 0) out vec4 outColor;
layout(location = 6) out float outFsrReactive;
layout(location = 7) out float outFsrTransparency;

/// Perceptually ordered ramp: dark red (sparse) → amber → green (dense).
/// Deliberately not a rainbow — a rainbow ramp has non-monotonic lightness, so
/// a density *gradient* reads as banding and the patchiness this view exists
/// to detect becomes indistinguishable from the colour map's own artifacts.
vec3 densityRamp(float d) {
    d = clamp(d, 0.0, 1.0);
    vec3 low = vec3(0.35, 0.05, 0.05);
    vec3 mid = vec3(0.85, 0.55, 0.10);
    vec3 high = vec3(0.20, 0.85, 0.25);
    return d < 0.5 ? mix(low, mid, d * 2.0) : mix(mid, high, (d - 0.5) * 2.0);
}

void main() {
    // Emissive by design: the distribution has to be legible in shadow, and a
    // lit debug view would confound "sparse here" with "dark here".
    outColor = vec4(densityRamp(vDGround), 1.0);
    outFsrReactive = 1.0;
    outFsrTransparency = 1.0;
}
