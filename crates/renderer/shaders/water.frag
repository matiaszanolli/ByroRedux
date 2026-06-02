#version 460
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : require
#extension GL_GOOGLE_include_directive : require

// Shared shader-side constants (CAUSTIC_FIXED_SCALE, MAT_FLAG_* bits,
// INSTANCE_FLAG_* bits, etc.) — generated from
// `crates/renderer/src/shader_constants_data.rs` by build.rs so the
// Rust + GLSL sides stay byte-equal. Required for #1256's
// `imageAtomicAdd` fixed-point scale match against caustic_splat.comp.
#include "include/shader_constants.glsl"

// ── Water surface fragment shader ─────────────────────────────────────
//
// Renders a transparent water surface for one of four
// `WaterKind` modes selected by `push.meta.x`:
//
//   0 — Calm:     two unbiased scrolling normal maps, fresnel-mixed
//                 RT reflection + RT refraction, mild shoreline foam.
//   1 — River:    Calm + flow-biased UV scroll on the dominant layer.
//   2 — Rapids:   River + flow-aligned foam streaks, secondary high-
//                 frequency normal layer for whitewater chop, brighter
//                 highlights.
//   3 — Waterfall: vertical sheet. Surface tangent (mesh-provided)
//                 is the flow axis; scroll along it at high speed.
//                 No refraction ray (opaque sheet). Foam concentrated
//                 at top + bottom of the sheet.
//
// Ray-tracing strategy (mirrors `triangle.frag::traceReflection`):
//   • Reflection — Schlick fresnel × `reflectivity`. Ray fired along
//     `reflect(-V, N_perturbed)` with `TerminateOnFirstHit`. Distance
//     attenuation matches the rest of the pipeline.
//   • Refraction — fired along `refract(-V, N, 1.0/ior)` with
//     `TerminateOnFirstHit`. Hit distance through water column drives
//     Beer-Lambert absorption: `exp(-hitDist / fog_far) * shallow_color`
//     blended with `deep_color`.
//   • Shoreline — short downward ray from the water surface; hit-dist
//     under `shoreline_width` lights up the foam mask. This avoids
//     plumbing the opaque-pass depth buffer through to the water
//     descriptor set.
//
// Why no SVGF / NRD denoise on water rays:
//   Each fragment fires at most 2 rays (1 reflection + 1 refraction)
//   and the resolved colour is already low-frequency in screen space
//   thanks to the perturbed-normal averaging across the surface. The
//   composite-pass tone-mapper + TAA handles the residual jitter.
//
// Push constants (128 bytes, exactly the Vulkan 1.1 minimum
// `maxPushConstantsSize ≥ 128` — no headroom remains):

layout(push_constant) uniform WaterPush {
    // x = time (seconds since cell load), y = WaterKind enum cast to
    // float, z = foam_strength (0..1), w = ior (1.33 ~ 1.5)
    vec4 timing;
    // xyz = flow direction (unit), w = flow speed (world units / s)
    vec4 flow;
    // rgb = shallow_color (linear), a = fog_near
    vec4 shallow;
    // rgb = deep_color (linear), a = fog_far
    vec4 deep;
    // xy = scroll_a (world units/s), zw = scroll_b
    vec4 scroll;
    // x = uv_scale_a, y = uv_scale_b, z = shoreline_width, w = reserved
    vec4 tune;
    // x = fresnel_f0, y = (unused/reserved), z = normal_map_index (uintBitsToFloat — sample with floatBitsToUint), w = (reserved)
    vec4 misc;
    // rgb = reflection_tint (WATR DATA reflection_color — tints geometry-hit
    // colour in traceWaterRay; #1069 / F-WAT-09). a = reflectivity (0..1,
    // moved from tune.w).
    vec4 tint_reflect;
} push;

// WATER_CALM / WATER_RIVER / WATER_RAPIDS / WATER_WATERFALL now come
// from `include/shader_constants.glsl` (generated from Rust). The
// pre-#1256 local `const uint` declarations were a duplicate of the
// shared #defines; #1256's include directive made the duplicates a
// redefinition error.

layout(location = 0) in vec3 vWorldPos;
layout(location = 1) in vec3 vWorldNormal;
layout(location = 2) in vec3 vWorldTangent;
layout(location = 3) in float vWorldBitangentSign;
// #1036 / F-WAT-08 — `vUV` (loc 4) and `vInstanceIndex` (loc 5)
// were declared as orphan inputs (vertex shader wrote them, this
// fragment shader never read them). Both removed in lockstep with
// `water.vert`. UVs are computed below from world XZ / T-B
// projection; the push-constant block (`WaterPush`) carries every
// per-plane parameter the fragment shader needs, so there's no
// `gl_InstanceIndex`-driven instance lookup on this path.

// Single HDR output — water is a transparent draw, blended onto the
// opaque pass's main colour attachment via standard SRC_ALPHA /
// ONE_MINUS_SRC_ALPHA blending. Bypasses the G-buffer split (no
// normal / motion / mesh-ID writes from water — RT denoising stays on
// the opaque pass).
layout(location = 0) out vec4 outColor;

// Bindless texture array — normal maps + foam mask sample here.
layout(set = 0, binding = 0) uniform sampler2D textures[];

layout(set = 1, binding = 1) uniform CameraUBO {
    mat4 viewProj;
    mat4 prevViewProj;
    mat4 invViewProj;
    vec4 cameraPos;
    vec4 sceneFlags;
    vec4 screen;
    vec4 fog;
    vec4 jitter;
    vec4 skyTint;
    vec4 sunDirection; // xyz = sun world-space direction (light travel), w = intensity. #1210.
    vec4 dofParams;      // x = aperture half-radius, y = focus_dist, zw = reserved. 0.0 = pinhole.
};

layout(set = 1, binding = 2) uniform accelerationStructureEXT topLevelAS;

// #1256 / Phase D of #1210 — water-side caustic accumulator.
// Per-FIF R32_UINT storage image owned by WaterCausticAccum (#1255),
// cleared pre-render-pass each frame in `context::draw::draw_frame`,
// written here via `imageAtomicAdd` (single eta + single bounce per
// REN-D13-NEW-04), sampled by composite (#1257, Phase E) alongside
// the existing causticTex. Bound at set 2 binding 0 per the
// WaterPipeline pipeline-layout shape declared in #1255.
layout(set = 2, binding = 0, r32ui) uniform uimage2D waterCausticAccum;

const float PI = 3.14159265359;
const float REFLECTION_MAX_DIST = 5000.0;
const float REFRACTION_MAX_DIST = 2000.0;
const float SHORELINE_RAY_MAX   = 256.0;
const float DIST_FALLOFF        = 0.0015; // matches triangle.frag

// ── Hash / noise helpers ──────────────────────────────────────────────
//
// 2-D hash adapted from Mark Jarzynski / Marc Olano's "Hash Functions
// for GPU Rendering" (JCGT 2020) — cheap, no tex-fetch, good visual
// quality for foam streak randomisation.

float hash21(vec2 p) {
    p = fract(p * vec2(443.897, 441.423));
    p += dot(p, p.yx + 19.19);
    return fract((p.x + p.y) * p.x);
}

float valueNoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = hash21(i + vec2(0.0, 0.0));
    float b = hash21(i + vec2(1.0, 0.0));
    float c = hash21(i + vec2(0.0, 1.0));
    float d = hash21(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// ── Normal map sampling ───────────────────────────────────────────────
//
// Two scrolling samples blended in tangent-space. When no normal map
// is bound (`normalMapIndex == 0xFFFFFFFF`), we fall back to a pure
// procedural noise gradient so the water still has wave motion — this
// path runs for default-water cells that never had an XCWT.

vec3 sampleScrollingNormal(uint normalMapIndex, vec2 uvBase, vec2 scroll, float scale, float time) {
    if (normalMapIndex == 0xFFFFFFFFu) {
        // Procedural fallback — animated 2-octave value noise gradient.
        // Cheap, doesn't pretend to be real waves but reads as moving
        // water.
        //
        // The gradient → tangent-space normal scaling is tuned so the
        // resulting normal stays within ~15° of straight up: anything
        // larger triggers the crest-foam pass downstream on every
        // fragment, painting the surface white. Pre-fix the multiplier
        // was `*4.0` on top of a sub-1 noise difference, which yielded
        // near-horizontal normals across the whole plane and made
        // `foamCrest()` saturate everywhere — see the May 2026
        // smoke-test where horizontal cell water planes rendered as
        // solid foam.
        //
        // Math: with `eps = 1.0` and noise output in [0,1], the raw
        // difference `(h - h_offset)` is bounded by ±1. The 0.12
        // multiplier puts the tangent-space normal at
        // `(±0.12, ±0.12, 1)` worst case → world tilt < 10°, well
        // under the 23° threshold where crest foam starts firing.
        vec2 uv = uvBase * scale + scroll * time;
        float h0 = valueNoise(uv * 4.0);
        float h1 = valueNoise(uv * 9.0 + 17.0);
        float h  = h0 * 0.65 + h1 * 0.35;
        const float eps = 1.0;
        float hx = valueNoise(uv * 4.0 + vec2(eps, 0.0)) * 0.65
                 + valueNoise(uv * 9.0 + vec2(eps, 0.0) + 17.0) * 0.35;
        float hy = valueNoise(uv * 4.0 + vec2(0.0, eps)) * 0.65
                 + valueNoise(uv * 9.0 + vec2(0.0, eps) + 17.0) * 0.35;
        return normalize(vec3((h - hx) * 0.12, (h - hy) * 0.12, 1.0));
    }
    vec2 uv = uvBase * scale + scroll * time;
    vec3 n = texture(textures[nonuniformEXT(normalMapIndex)], uv).xyz;
    return normalize(n * 2.0 - 1.0);
}

// ── RT reflection / refraction ────────────────────────────────────────
//
// Both helpers use `TerminateOnFirstHit` — water doesn't need closest-
// hit precision and the budget difference is meaningful with two rays
// per fragment. See `traceReflection` in triangle.frag for the design
// rationale.

// `missFallback` is the colour returned on a TLAS miss. Reflection
// callers want the sky tint (light from above the water surface bounces
// back toward the camera). Refraction callers want the cell's deep
// water tint — pre-#1015 a single hardcoded `skyTint` return painted a
// faint sky cast through `absorbWaterColumn`'s ~14% surface-radiance
// term on miss (downward refraction rays escaping the BLAS at cliff
// edges or sparse exterior cells).
vec3 traceWaterRay(vec3 origin, vec3 direction, float maxDist, vec3 missFallback, out float hitDist, out bool hit) {
    rayQueryEXT rq;
    rayQueryInitializeEXT(
        rq, topLevelAS,
        gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFF,
        origin, 0.05, direction, maxDist
    );
    rayQueryProceedEXT(rq);

    if (rayQueryGetIntersectionTypeEXT(rq, true) == gl_RayQueryCommittedIntersectionNoneEXT) {
        hit = false;
        hitDist = maxDist;
        return missFallback;
    }

    hit = true;
    hitDist = rayQueryGetIntersectionTEXT(rq, true);

    // We deliberately do NOT refetch the hit material / texture here.
    // Water rays use a tight budget (≤2 per fragment) and the hit
    // colour is going to be tinted heavily by Beer-Lambert absorption
    // downstream anyway, so the perceptual quality gain from a real
    // texture sample is small. Using a neutral grey biased toward
    // the sky tint keeps the descriptor surface for this pipeline
    // limited to (textures, camera UBO, TLAS, instance buffer) — no
    // material table / vertex SSBO / index SSBO bindings needed,
    // which would otherwise double the descriptor footprint.
    //
    // Deferred work (tracked under closed #1070 — M38 Phase 2 / #1110):
    // Returns a per-WATR constant — the water pipeline does not bind
    // MaterialBuffer / GlobalVertexBuffer / GlobalIndexBuffer. To fetch
    // the real hit albedo, extend WaterPipeline's descriptor set with
    // those three SSBOs and call rayQueryGetIntersectionInstanceCustom
    // IndexEXT to index into them. See also: caustic_splat.comp uses
    // instances[instIdx].avgAlbedoR/G/B as a per-instance proxy that
    // could approximate this without a full SSBO plumb.
    //
    // The per-WATR `tint_reflect.rgb` (sourced from WATR DATA
    // reflection_color, #1069 / F-WAT-09) currently provides water-body-
    // specific tinting. The pre-fix value was a hard-coded neutral grey
    // (`vec3(0.65, 0.7, 0.75)`); the default of `tint_reflect.rgb`
    // matches that fallback for unspecified records.
    return mix(skyTint.xyz, push.tint_reflect.rgb, 0.4);
}

// ── Beer-Lambert through the water column ─────────────────────────────
//
// `hitDist` is the refraction-ray length under water. Attenuation
// uses `fog_far` as the extinction reciprocal: at `fog_far`, the
// refracted radiance reaches 1/e ≈ 37% of its surface value, and the
// deep-water colour fully takes over.
vec3 absorbWaterColumn(vec3 refractedRadiance, float hitDist) {
    float t = clamp(hitDist / max(push.deep.a, 1.0), 0.0, 1.0);
    float absorption = exp(-t * 2.0); // empirical multiplier — tunable
    return mix(push.deep.rgb, refractedRadiance * push.shallow.rgb, absorption);
}

// ── Foam ──────────────────────────────────────────────────────────────
//
// Three independent sources, summed and saturated. The
// `WaterKind`-specific weights live in the call site below.
//
// 1. Shoreline foam — short downward RT ray; mask = 1 - smoothstep
//    over `shoreline_width`. Disabled for waterfalls (the sheet is
//    rarely in contact with ground).
// 2. Flow-aligned streaks — value-noise sampled along the flow
//    tangent at high frequency, scrolled with the current. Drives
//    rapids whitewater.
// 3. Crest foam — fires when the perturbed shading normal points
//    more upward than threshold (high-amplitude wave crests). Drives
//    the "whitecaps on choppy water" look.

float foamShoreline(vec3 worldPos, vec3 surfaceNormal) {
    rayQueryEXT rq;
    rayQueryInitializeEXT(
        rq, topLevelAS,
        gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFF,
        worldPos, 0.05, -surfaceNormal, push.tune.z
    );
    rayQueryProceedEXT(rq);
    if (rayQueryGetIntersectionTypeEXT(rq, true) == gl_RayQueryCommittedIntersectionNoneEXT) {
        return 0.0;
    }
    float depthToGround = rayQueryGetIntersectionTEXT(rq, true);
    return 1.0 - smoothstep(0.0, push.tune.z, depthToGround);
}

float foamFlowStreaks(vec3 worldPos, float time) {
    // Project worldPos onto the flow tangent + a perpendicular tangent
    // to get streak coords. Streaks scroll with the flow.
    vec3 flowDir = push.flow.xyz;
    float speed  = push.flow.w;
    // Build a perpendicular in the surface tangent plane.
    vec3 perp = normalize(cross(vWorldNormal, flowDir));
    float u = dot(worldPos, flowDir) - speed * time;
    float v = dot(worldPos, perp);
    // High-frequency on the streak axis, lower on the perpendicular —
    // gives elongated whitewater streaks aligned to the current.
    float streak = valueNoise(vec2(u * 0.04, v * 0.18));
    // Steepen to thin streaks rather than soft blobs.
    return smoothstep(0.55, 0.8, streak);
}

float foamCrest(vec3 perturbedNormal, vec3 surfaceNormal) {
    // dot(perturbed, surface) ≈ 1 on flat; lower as the perturbed
    // normal tilts. Crest foam sits in the high-tilt band — pick a
    // window so flat regions don't foam and full-vertical sides
    // don't foam either.
    float n = dot(perturbedNormal, surfaceNormal);
    return smoothstep(0.92, 0.78, n); // inverted: lower n = more foam
}

void main() {
    // ── Setup ──
    float time = push.timing.x;
    uint  kind = uint(push.timing.y + 0.5);
    float foamStrength = push.timing.z;
    float ior  = push.timing.w;
    uint  normalMapIndex = floatBitsToUint(push.misc.z);

    vec3 N = normalize(vWorldNormal);
    vec3 T = normalize(vWorldTangent);
    // Re-orthogonalise T against N (Gram-Schmidt) — drops the
    // floating-point drift from interpolation across the quad.
    T = normalize(T - N * dot(T, N));
    vec3 B = normalize(cross(N, T) * vWorldBitangentSign);
    mat3 TBN = mat3(T, B, N);

    vec3 V = normalize(cameraPos.xyz - vWorldPos);

    // ── Wave UVs ──
    // For flat surfaces (Calm/River/Rapids), drive the UV from world
    // XZ so the surface texture is continuous across the cell grid
    // (no seams at quad edges). For waterfalls, the UV runs along
    // the surface tangent (mesh-provided flow axis) so the sheet
    // scrolls down naturally.
    vec2 uvWorld;
    if (kind == WATER_WATERFALL) {
        // Project world position onto the flow tangent (T) for v,
        // and the bitangent for u. The vertex shader sets up T as
        // the mesh tangent — for a waterfall the artist authors that
        // pointing along the fall direction.
        uvWorld = vec2(dot(vWorldPos, B), dot(vWorldPos, T));
    } else {
        // Use world XZ — flat-plane water.
        uvWorld = vWorldPos.xz;
    }

    // Two scrolling normal layers (the "movement on flat surfaces"
    // case). For River/Rapids/Waterfall, layer A's scroll vector is
    // baked from `flow` on the CPU side, so we don't have to branch
    // here. Push constants carry the final scroll vectors.
    vec3 nA = sampleScrollingNormal(normalMapIndex, uvWorld, push.scroll.xy, push.tune.x, time);
    vec3 nB = sampleScrollingNormal(normalMapIndex, uvWorld, push.scroll.zw, push.tune.y, time);

    // Rapids adds a third high-frequency layer scrolled by the flow
    // — gives that chaotic whitewater chop pattern.
    vec3 nMix;
    if (kind == WATER_RAPIDS) {
        vec3 nC = sampleScrollingNormal(
            normalMapIndex,
            uvWorld,
            push.flow.xy * push.flow.w * 2.0,
            push.tune.x * 2.5,
            time
        );
        nMix = normalize(nA + nB + nC * 0.7);
    } else {
        nMix = normalize(nA + nB);
    }

    // Tangent → world space.
    vec3 Nperturbed = normalize(TBN * nMix);

    // Stability clamp — #1025 / F-WAT-04.
    //
    // As the camera grazes the surface, the high-frequency normal-map
    // perturbation can tilt `Nperturbed` past the geometric plane
    // (`dot(Nperturbed, N) <= 0`), producing reflection / refraction
    // rays that hit the water mesh itself from underneath. The
    // pre-#1025 clamp fired only when `dot(Nperturbed, V) < 0.05`
    // and mixed only 60 % toward `N` — leaving 40 % of a still-
    // sub-plane normal in the result, so the failure mode persisted
    // at extreme grazing.
    //
    // Two-part fix, both feeding `Nperturbed` consumed by `reflect`
    // and `refract` below:
    //
    //   1. Project `Nperturbed` into the half-space above the
    //      geometric plane (`dot(Nperturbed, N) >= NORMAL_PLANE_EPS`)
    //      via a single Gram-Schmidt-style step. Smooth — preserves
    //      the tangential perturbation, just removes the sub-plane
    //      component. No visible banding at the threshold.
    //
    //   2. Hard fall-back to the geometric `N` when even after step 1
    //      the perturbed normal points away from the viewer
    //      (`dot(Nperturbed, V) <= 0`). Fresnel computation needs a
    //      positive `N·V` for the Schlick term to be meaningful;
    //      hitting this branch means the view ray and the geometric
    //      plane are essentially parallel (skybox horizon
    //      transition), so the safe choice is a perfectly mirror
    //      surface for that pixel.
    //
    // Sibling: refraction uses the same `Nperturbed` (line ~373),
    // so the clamp covers both `reflect` and `refract` with one pass.
    const float NORMAL_PLANE_EPS = 0.05;
    float NperturbedDotN = dot(Nperturbed, N);
    if (NperturbedDotN < NORMAL_PLANE_EPS) {
        Nperturbed = normalize(Nperturbed + N * (NORMAL_PLANE_EPS - NperturbedDotN));
    }
    if (dot(Nperturbed, V) <= 0.0) {
        Nperturbed = N;
    }

    // ── Fresnel ──
    float NdotV = max(dot(Nperturbed, V), 0.0);
    float F0    = push.misc.x;
    float fresnel = F0 + (1.0 - F0) * pow(1.0 - NdotV, 5.0);

    // ── Reflection ray ──
    vec3 R = reflect(-V, Nperturbed);
    float reflDist; bool reflHit;
    // Reflection-miss: sky tint is the right backdrop (the reflected
    // ray escaped above the water surface).
    vec3 reflColor = traceWaterRay(vWorldPos + N * 0.05, R, REFLECTION_MAX_DIST, skyTint.xyz, reflDist, reflHit);
    if (reflHit) {
        reflColor *= exp(-reflDist * DIST_FALLOFF);
    }
    // Always blend toward sky on miss so the surface doesn't go black
    // when the reflection escapes.
    reflColor = mix(skyTint.xyz, reflColor, reflHit ? 1.0 : 0.0);

    // ── Refraction ray (skipped for waterfalls) ──
    vec3 refrColor;
    float refrDist = push.deep.a; // default: full deep tint on skip
    if (kind != WATER_WATERFALL) {
        vec3 Tdir = refract(-V, Nperturbed, 1.0 / max(ior, 1.0));
        bool refrHit;
        // If TIR (total internal reflection) — refract returns zero —
        // skip the ray and use deep colour.
        if (length(Tdir) > 0.001) {
            // Refraction-miss: deep water tint is the right backdrop
            // (the downward ray escaped the BLAS — cliff edge / sparse
            // exterior — but conceptually it should land in the deep
            // water column, NOT in the sky above). #1015.
            vec3 hitColor = traceWaterRay(vWorldPos + N * 0.05, Tdir, REFRACTION_MAX_DIST, push.deep.rgb, refrDist, refrHit);
            refrColor = absorbWaterColumn(hitColor, refrHit ? refrDist : push.deep.a);
        } else {
            refrColor = push.deep.rgb;
        }
    } else {
        // Waterfalls: just use the deep colour modulated slightly by
        // the perturbed normal facing direction — gives the sheet a
        // pearlescent sheen rather than a flat tint.
        refrColor = push.deep.rgb * (0.7 + 0.3 * NdotV);
    }

    // ── Foam composite ──
    float foamMask = 0.0;
    if (kind != WATER_WATERFALL) {
        foamMask += foamShoreline(vWorldPos, N) * 1.0;
    }
    if (kind == WATER_RAPIDS) {
        foamMask += foamFlowStreaks(vWorldPos, time) * 0.85;
        foamMask += foamCrest(Nperturbed, N) * 0.7;
    } else if (kind == WATER_RIVER) {
        foamMask += foamFlowStreaks(vWorldPos, time) * 0.25;
    } else if (kind == WATER_WATERFALL) {
        // Sheet foam: more at the top and bottom of the falling
        // surface. We don't have a normalised sheet coordinate
        // without extra push-constant plumbing, so approximate with
        // a streak pattern at very high speed for that "fizzing
        // sheet" read.
        foamMask += foamFlowStreaks(vWorldPos, time * 1.6) * 0.95;
        foamMask += foamCrest(Nperturbed, N) * 0.45;
    }
    foamMask = clamp(foamMask * foamStrength, 0.0, 1.0);

    // ── Surface colour ──
    vec3 surfaceColor = mix(refrColor, reflColor * push.tint_reflect.w, fresnel);

    // Foam is bright white-ish with a faint tint from the shallow
    // colour — looks more natural than pure white.
    vec3 foamColor = mix(vec3(0.92, 0.95, 0.98), push.shallow.rgb * 1.1, 0.15);
    surfaceColor = mix(surfaceColor, foamColor, foamMask);

    // ── Alpha ──
    // Waterfalls are heavily opaque; flat water lets some of the
    // refraction colour through but is mostly opaque at the surface
    // since refraction is already baked into surfaceColor. Use a
    // grazing-angle alpha boost so the water plane edges remain
    // visible at low view angles (avoids the classic "water vanishes
    // at the shoreline" artefact).
    float baseAlpha = (kind == WATER_WATERFALL) ? 0.95 : 0.88;
    float grazingBoost = pow(1.0 - NdotV, 2.0) * 0.1;
    float alpha = clamp(baseAlpha + grazingBoost + foamMask * 0.1, 0.0, 1.0);

    outColor = vec4(surfaceColor, alpha);

    // ── #1256 / Phase D of #1210 — water-side caustic splat ─────────
    //
    // Cast a shadow ray toward the sun. On miss (sun visible above
    // this water fragment) refract sunlight through the bumped water
    // normal into the underwater medium, find the floor by tracing
    // the refracted ray against the TLAS, project the world-space
    // hit back to screen-space, and `imageAtomicAdd` a fixed-point
    // luminance contribution to `waterCausticAccum`. Composite
    // (Phase E, #1257) samples + adds it to direct lighting.
    //
    // Constraints per REN-D13-NEW-04 (audit 2026-05-09):
    //   • Single eta — no per-channel chromatic split (no
    //     wavelength dispersion). η = 1.0/1.33 (air → water).
    //   • Single bounce — no reflection-then-refraction chains.
    //
    // Magnitude pinning: the fixed-point scale matches
    // caustic_splat.comp's so the two accumulators sum on a
    // shared luminance basis (composite divides each by the same
    // CAUSTIC_FIXED_SCALE). `clamp_max = 0xFFFFFFFFu / scale`
    // mirrors the #1099 anchor — prevents wraparound when a hot
    // sun + perpendicular surface fragment dumps a large value.
    if (sunDirection.w > 0.0) {
        vec3 sunRay = normalize(sunDirection.xyz);       // light-travel direction
        // 1. Shadow ray toward sun (terminate-on-first-hit).
        rayQueryEXT shadowRq;
        rayQueryInitializeEXT(
            shadowRq, topLevelAS,
            gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT,
            0xFF, vWorldPos + N * 0.05, 0.05, -sunRay, 10000.0
        );
        rayQueryProceedEXT(shadowRq);
        bool sunVisible =
            rayQueryGetIntersectionTypeEXT(shadowRq, true)
            == gl_RayQueryCommittedIntersectionNoneEXT;
        if (sunVisible) {
            // 2. Snell refraction. refract() returns vec3(0) on
            // total-internal-reflection, which can't happen for
            // light entering the denser medium from above — but
            // length-gate anyway in case `sunRay` is grazing.
            vec3 refractDir = refract(sunRay, N, 1.0 / 1.33);
            if (length(refractDir) > 1e-4) {
                // 3. Find floor via TLAS ray (single bounce).
                //
                // Origin bias steps INTO the water along -N: the refracted
                // ray transmits to the -N side, so the floor we want lies
                // below the surface. This matches the transmission
                // convention used by triangle.frag's pane refraction
                // (`fragWorldPos - N * 0.15`) and caustic_splat.comp
                // (`G - ns * 0.1`) — NOT the +N shadow-ray convention. A +N
                // bias would push the origin above the surface so this
                // downward refractDir would re-cross the water plane and
                // self-intersect the surface mesh. tMin 0.05 matches the
                // shadow-ray sibling above, foamShoreline, caustic_splat,
                // and triangle.frag's refraction loop (RT-01 / #1388).
                rayQueryEXT floorRq;
                rayQueryInitializeEXT(
                    floorRq, topLevelAS,
                    gl_RayFlagsOpaqueEXT, 0xFF,
                    vWorldPos - N * 0.05, 0.05, refractDir, 5000.0
                );
                rayQueryProceedEXT(floorRq);
                if (rayQueryGetIntersectionTypeEXT(floorRq, true)
                    != gl_RayQueryCommittedIntersectionNoneEXT) {
                    float floorT = rayQueryGetIntersectionTEXT(floorRq, true);
                    vec3 floorWorld = vWorldPos + refractDir * floorT;
                    // 4. Project floor hit to screen-space.
                    vec4 floorClip = viewProj * vec4(floorWorld, 1.0);
                    if (floorClip.w > 0.0) {
                        vec2 ndc = floorClip.xy / floorClip.w;
                        vec2 uv01 = ndc * 0.5 + 0.5;
                        if (all(greaterThanEqual(uv01, vec2(0.0)))
                            && all(lessThanEqual(uv01, vec2(1.0)))) {
                            ivec2 pixel = ivec2(uv01 * screen.xy);
                            // 5. Directional weighting — caustic
                            // intensity scales with how
                            // perpendicular the water surface is
                            // to the sun (Lambert cosine on the
                            // light side). Grazing sun = dim
                            // caustic; noon sun overhead = full.
                            // Travel falloff matches caustic_splat
                            // (1 / (1 + t²·k)) — caustics fade with
                            // depth as the refracted column spreads.
                            float NdotSun = max(dot(N, -sunRay), 0.0);
                            float travelFall = 1.0 / (1.0 + floorT * floorT * 1e-4);
                            float contrib = sunDirection.w * NdotSun * travelFall;
                            float scale = CAUSTIC_FIXED_SCALE;
                            float clamp_max = float(0xFFFFFFFFu) / scale;
                            uint fixed_val =
                                uint(clamp(contrib * scale, 0.0, clamp_max));
                            if (fixed_val != 0u) {
                                imageAtomicAdd(waterCausticAccum, pixel, fixed_val);
                            }
                        }
                    }
                }
            }
        }
    }
}
