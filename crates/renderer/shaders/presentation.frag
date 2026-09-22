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
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}

// ---------------------------------------------------------------------------
// AgX — Minimal AgX implementation (c) 2023 Benjamin Wrensch (IOLITE engine,
// https://iolite-engine.com/blog_posts/minimal_agx_implementation), a compact
// port of Troy Sobotka's AgX display transform. MIT licensed; notice in
// THIRD_PARTY_NOTICES.md. Behavioural mirror + licence pin: renderer's
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
    val = clamp(val, 0.0, 1.0);
    val = log2(val);
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
    float luminance = dot(scene.rgb, LUMA_REC709);
    vec3 graded = mix(vec3(luminance), scene.rgb, max(params.grade.x, 0.0));
    graded = (graded - vec3(0.18)) * max(params.grade.z, 0.0) + vec3(0.18);
    graded *= max(params.grade.y, 0.0);
    graded = mix(
        graded,
        graded * normalizedLegacyColor(params.tintColor.rgb),
        clamp(params.tintColor.a, 0.0, 1.0)
    );
    float exposure = texelFetch(exposureTex, ivec2(0), 0).r;
    vec3 presented = tonemap(graded * exposure);

    if (params.underwater.w > 0.0) {
        // The app packs the authored WATR fog ramp into this channel as a
        // Beer–Lambert extinction value. Do not apply a second fixed-distance
        // curve here: that erased per-water fog_near/fog_far differences.
        float extinction = clamp(params.underwater.w, 0.0, 0.85);
        vec3 underwaterTone = tonemap(params.underwater.xyz * exposure);
        presented = mix(presented, underwaterTone, extinction);
    }

    presented = mix(
        presented,
        normalizedLegacyColor(params.fadeColor.rgb),
        clamp(params.fadeColor.a, 0.0, 1.0)
    );

    outColor = vec4(presented, scene.a);
}
