#version 450
#extension GL_GOOGLE_include_directive : require

#include "include/shader_constants.glsl"

layout(set = 0, binding = 0) uniform sampler2D upscaledScene;

// Stage 1 (RENDERING-PLAN.md) — per-frame exposure, produced by the
// exposure-meter compute pass (fixed or auto-EV100). A 1x1 R32_SFLOAT texel,
// one image per frame in flight; the set is indexed per frame so this is the
// same value the FSR dispatch of this frame normalized against. The old
// push-constant scalar remains only as the `exposure`-lane reserved word —
// see PresentationPushConstants on the host side.
layout(set = 0, binding = 2) uniform sampler2D exposureTex;

// EX-05 / #2736 — pre-tonemap image-health counters.
//
// This pass is the *last* place the scene exists in linear HDR: everything
// after `aces()` below is clamped to [0,1], which is exactly why a PNG
// mean/stddev gate cannot observe an HDR NaN. A non-finite texel here either
// clamps to white or propagates as a black hole, and both read as ordinary
// scene content downstream.
//
// Counting here rather than in a dedicated compute pass is deliberate: this
// shader already runs exactly once per output pixel, so the check costs one
// branch and needs no new pipeline, dispatch or barrier.
layout(set = 0, binding = 1) buffer ImageHealth {
    uint nonFinitePixels;
    uint nonFiniteAlpha;
} health;

layout(push_constant) uniform PresentationParams {
    vec4 underwater;
    // Display-transform selection — ids from `tonemap.rs`
    // (`renderer::tonemap::TONEMAP_OP_*`). `uint`, matching the host's
    // `PresentationPushConstants` (#3578 idiom: no enum in float lanes).
    uint tonemapOp;
    // Layout padding only; keeps the 16-31 byte block scalar-aligned.
    uint reserved;
    // #3578 — `uint`, matching `PresentationPushConstants`. See that struct
    // for why these must not ride in float lanes.
    uint renderDebugFlags;
    uint renderDebugMode;
    vec4 lens;
    vec4 radialCurve;
    vec4 grade;
    vec4 radialCenter;
    vec4 tintColor;
    vec4 fadeColor;
} params;

layout(location = 0) in vec2 fragUV;
layout(location = 0) out vec4 outColor;

vec3 aces(vec3 x) {
    const float a = 2.51;
    const float b = 0.03;
    const float c = 2.43;
    const float d = 0.59;
    const float e = 0.14;
    // #4840 — the Narkowicz fit is not sign-safe: its numerator has a
    // second root at x = -0.012, so negative input maps to a POSITIVE
    // output (reaching 1.0 near x = -0.3) that the output clamp cannot
    // catch. The grade's contrast pivot (contrast > 1 on most FO3/FNV
    // IMGS) and saturation > 1 both produce negative channels, which
    // rendered black as grey. Floor at zero, as `agx()` already does.
    x = max(x, vec3(0.0));
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}

// ---------------------------------------------------------------------------
// AgX — Minimal AgX implementation (c) 2023 Benjamin Wrensch (IOLITE engine,
// https://iolite-engine.com/blog_posts/minimal_agx_implementation), a compact
// port of Troy Sobotka's AgX display transform. MIT licensed; notice in
// THIRD_PARTY_NOTICES.md. The CPU reference and behavior pins live in
// `tonemap.rs`.
//
// Output convention matches `aces()` above: LINEAR display-light — the
// reference EOTF `pow(x, 2.2)` returns the AgX-encoded signal to linear, and
// the B8G8R8A8_SRGB swapchain applies the sRGB OETF on write. Do not add an
// sRGB encode here.
// ---------------------------------------------------------------------------
const mat3 AGX_MAT = mat3(
  0.842479062253094,  0.0423282422610123, 0.0423756549057051,
  0.0784335999999992, 0.878468636469772,  0.0784336,
  0.0792237451477643, 0.0791661274605434, 0.879142973793104);

const mat3 AGX_INV_MAT = mat3(
  1.19687900512017,   -0.0528968517574562, -0.0529716355144438,
 -0.0980208811401368,  1.15190312990417,   -0.0980434501171241,
 -0.0990297440797205, -0.0989611768448433,  1.15107367264116);

// Polynomial fit of AgX's contrast S-curve (gist verbatim).
vec3 agxDefaultContrastApprox(vec3 x) {
    vec3 x2 = x * x;
    vec3 x4 = x2 * x2;
    return + 15.5     * x4 * x2
           - 40.14    * x4 * x
           + 31.96    * x4
           - 6.868    * x2 * x
           + 0.4298   * x2
           + 0.1191   * x
           - 0.00232;
}

vec3 agx(vec3 val) {
    const float min_ev = -12.47393;
    const float max_ev = 4.026069;
    val = AGX_MAT * val;
    // #4578 — clamp in LOG space (Wrensch 2023 minimal AgX, and three.js's
    // port): the old linear [0,1] clamp meant log2 never exceeded 0, the
    // 4.03-EV headroom was dead, and every texel >= 1.0 flattened to
    // ~0.59 display-linear. max() before log2 also avoids log2(0) = -inf,
    // whose inf-inf NaN propagated through the contrast polynomial on
    // exactly-black channels (GLSL FMax's NaN result is undefined).
    val = clamp(log2(max(val, 1.0e-10)), min_ev, max_ev);
    val = (val - min_ev) / (max_ev - min_ev);
    val = agxDefaultContrastApprox(val);
    val = AGX_INV_MAT * val;
    // Reference EOTF back to linear; clamp absorbs the outset matrix's small
    // negatives before pow() can turn them into NaN.
    val = clamp(pow(max(val, 0.0), vec3(2.2)), 0.0, 1.0);
    return val;
}

// Display-transform dispatch — ids from renderer `tonemap.rs`
// (TONEMAP_OP_ACES = 0, TONEMAP_OP_AGX = 1).
vec3 tonemap(vec3 x) {
    return params.tonemapOp == TONEMAP_OP_AGX ? agx(x) : aces(x); // TONEMAP_OP_AGX from shader_constants.glsl
}

vec4 sampleImageSpace(vec2 uv) {
    vec2 texel = 1.0 / vec2(textureSize(upscaledScene, 0));
    vec4 scene = texture(upscaledScene, uv);

    float blurRadius = max(params.lens.x, 0.0);
    if (blurRadius > 0.001) {
        vec2 d = texel * blurRadius;
        vec4 blurred = scene * 4.0;
        blurred += texture(upscaledScene, uv + vec2( d.x, 0.0));
        blurred += texture(upscaledScene, uv + vec2(-d.x, 0.0));
        blurred += texture(upscaledScene, uv + vec2(0.0,  d.y));
        blurred += texture(upscaledScene, uv + vec2(0.0, -d.y));
        blurred += texture(upscaledScene, uv + d);
        blurred += texture(upscaledScene, uv - d);
        blurred += texture(upscaledScene, uv + vec2(d.x, -d.y));
        blurred += texture(upscaledScene, uv + vec2(-d.x, d.y));
        scene = blurred / 12.0;
    }

    float doubleVision = abs(params.lens.y);
    if (doubleVision > 0.001) {
        vec2 offset = vec2(doubleVision * 4.0 * texel.x, 0.0);
        vec4 ghost = 0.5 * (
            texture(upscaledScene, uv + offset) +
            texture(upscaledScene, uv - offset)
        );
        scene = mix(scene, ghost, clamp(doubleVision * 0.35, 0.0, 0.75));
    }

    float motionBlur = abs(params.lens.z);
    if (motionBlur > 0.001) {
        vec2 offset = vec2(motionBlur * 6.0 * texel.x, 0.0);
        vec4 streak = (
            texture(upscaledScene, uv - offset) + scene +
            texture(upscaledScene, uv + offset)
        ) / 3.0;
        scene = mix(scene, streak, clamp(motionBlur * 0.25, 0.0, 0.8));
    }

    float radialStrength = params.lens.w;
    if (abs(radialStrength) > 0.001) {
        vec2 radial = uv - params.radialCenter.xy;
        float radius = length(radial);
        float rampUp = max(params.radialCurve.x, 0.001);
        float envelope = smoothstep(
            params.radialCurve.y,
            params.radialCurve.y + rampUp,
            radius
        );
        float downStart = params.radialCurve.w;
        float rampDown = params.radialCurve.z;
        if (rampDown > 0.001 && downStart > params.radialCurve.y) {
            envelope *= 1.0 - smoothstep(downStart, downStart + rampDown, radius);
        }
        float amount = clamp(radialStrength * 0.02 * envelope, -0.25, 0.25);
        vec4 radialSum = scene;
        for (int i = 1; i <= 6; ++i) {
            float stepAmount = amount * (float(i) / 6.0);
            radialSum += texture(upscaledScene, uv - radial * stepAmount);
        }
        scene = radialSum / 7.0;
    }
    return scene;
}

vec3 normalizedLegacyColor(vec3 color) {
    return max(max(color.r, color.g), color.b) > 1.0 ? color / 255.0 : color;
}

// #5482 — IMGS/IMAD cinematic contrast on EXPOSED radiance. Linear stretch
// about middle grey, handing over below GRADE_CONTRAST_TOE to a power law
// that approaches black instead of crossing it — the Community Shaders
// ISHDR "crushed shadows" form (see shader_constants_data.rs). The plain
// linear stretch floored every shade under `pivot * (1 - 1/contrast)` to
// exactly zero: at Skyrim's weather contrast 1.3 that was all ambient-only
// exterior terrain, which read as black holes in the ground. The Rust
// mirror and behaviour pins live in tonemap.rs (`grade_contrast`).
vec3 gradeContrast(vec3 x, float contrast) {
    vec3 stretched = (x - vec3(GRADE_CONTRAST_PIVOT)) * contrast + vec3(GRADE_CONTRAST_PIVOT);
    vec3 toe = GRADE_CONTRAST_PIVOT
        * pow(max(x / GRADE_CONTRAST_PIVOT, vec3(1.0e-6)), vec3(contrast));
    return mix(toe, stretched, clamp(toe / GRADE_CONTRAST_TOE, 0.0, 1.0));
}

void main() {
    // Sample the raw texel for the health check, *before* the lens/blur path
    // mixes neighbours together — a single NaN would otherwise smear across
    // every pixel its blur kernel touches and inflate the count.
    vec4 raw = texture(upscaledScene, fragUV);
    if (any(isnan(raw.rgb)) || any(isinf(raw.rgb))) {
        atomicAdd(health.nonFinitePixels, 1u);
    }
    if (isnan(raw.a) || isinf(raw.a)) {
        atomicAdd(health.nonFiniteAlpha, 1u);
    }

    // The main/composite passes have already encoded the debug oracle in
    // display-linear [0,1]. Preserve categorical colours, scalar visibility,
    // and isolated lighting energy exactly: no lens kernels, grading,
    // exposure, tone mapping, underwater treatment, or scripted fades.
    uint dbgFlags = params.renderDebugFlags;
    uint debugMode = params.renderDebugMode;
    // #2978 — the "which views are oracles" policy is generated into
    // shader_constants.glsl from DBG_VIZ_RAW_OUTPUT_ANY/_ALL, the same two
    // catalogs shader_constants.rs's debug_viz_requires_raw_output walks. Do
    // not re-spell its clauses here: that is what let the Rust side and both
    // shaders drift apart behind a four-literal subset check.
    bool rawDebug = debugMode == RENDER_DEBUG_LEGACY_FLAGS
        ? DBG_VIZ_REQUIRES_RAW_OUTPUT(dbgFlags)
        : debugMode != RENDER_DEBUG_FINAL;
    if (rawDebug) {
        outColor = vec4(clamp(raw.rgb, 0.0, 1.0), raw.a);
        return;
    }

    vec4 scene = sampleImageSpace(fragUV);
    // #5482 — expose BEFORE the grade. Saturation, brightness and tint are
    // scale-invariant, but the contrast pivot is not: pivoting raw radiance
    // at 0.18 put it at 0.18 × exposure in the metered image (2× the key
    // with the meter at its ceiling), so dark-metered exteriors lost far
    // more of the frame to the stretch than bright ones. Vanilla ISHDR
    // also applies its adaptation first and grades after.
    float exposure = texelFetch(exposureTex, ivec2(0), 0).r;
    vec3 exposed = scene.rgb * exposure;
    float luminance = dot(exposed, LUMA_REC709);
    vec3 graded = mix(vec3(luminance), exposed, max(params.grade.x, 0.0));
    graded = gradeContrast(graded, max(params.grade.z, 0.0));
    graded *= max(params.grade.y, 0.0);
    // Cinematic tint: blend toward the graded luminance carried in the tint
    // hue. Both CK wikis (GECK + Creation Kit, "ImageSpace Modifiers" /
    // Cinematic / Tint): at full alpha it "will render the entire scene in
    // shades of the RGB color", and "will never raise the color level high
    // enough to completely wash out the scene". A plain multiply instead
    // darkened every hue away from the tint (blue sky under the Mojave's
    // amber went near-black) rather than shifting it toward the tint.
    graded = mix(
        graded,
        vec3(dot(graded, LUMA_REC709)) * normalizedLegacyColor(params.tintColor.rgb),
        clamp(params.tintColor.a, 0.0, 1.0)
    );
    // #5154 — EV-dependent chroma compress between the meter and the
    // tonemapper. When the eye adapts to low light, the exposure lift
    // pushes mid-tones into the tone curve's steep region, where
    // per-channel deltas magnify into visible hue shifts — the classic
    // dark-scene saturation blowout (AgX's coercive tables counter it only
    // partially, the Narkowicz ACES fit not at all). Compress chroma by
    // 2^(-falloff * lift), lift measured in stops above the meter's
    // neutral output (EV100 = 0 -> 1.2): one stop of adaptation costs a
    // quarter stop of chroma — about 0.88 chroma at the 2× envelope cap
    // (#5158); `exposure ev` compensation can lift further. `graded` is
    // already exposed (#5482), and chroma ratios are scale-invariant
    // anyway. The Rust mirror and behaviour pins live in tonemap.rs.
    float lift_stops = max(log2(max(exposure, 1.0e-6) / EXPOSURE_METER_NEUTRAL), 0.0);
    float chroma = exp2(-ADAPTATION_SAT_FALLOFF * lift_stops);
    float graded_luma = dot(graded, LUMA_REC709);
    vec3 compressed = mix(vec3(graded_luma), graded, chroma);
    vec3 presented = tonemap(compressed);

    if (params.underwater.w > 0.0) {
        // The app packs the authored WATR fog ramp into this channel as a
        // Beer–Lambert extinction value. Do not apply a second fixed-distance
        // curve here: that erased per-water fog_near/fog_far differences.
        float extinction = clamp(params.underwater.w, 0.0, 0.85);
        // The fog tint rides the same exposure and the same tone curve, so
        // it takes the same chroma compress (#5154).
        vec3 underwater_color = mix(
            vec3(dot(params.underwater.xyz, LUMA_REC709)),
            params.underwater.xyz,
            chroma
        );
        vec3 underwaterTone = tonemap(underwater_color * exposure);
        presented = mix(presented, underwaterTone, extinction);
    }

    presented = mix(
        presented,
        normalizedLegacyColor(params.fadeColor.rgb),
        clamp(params.fadeColor.a, 0.0, 1.0)
    );

    outColor = vec4(presented, scene.a);
}
