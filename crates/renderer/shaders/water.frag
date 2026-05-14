#version 460
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : require

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
// Push constants (112 bytes, ≤ 128 byte minimum on every Vulkan 1.1
// device — `maxPushConstantsSize ≥ 128`):

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
    // x = uv_scale_a, y = uv_scale_b, z = shoreline_width, w = reflectivity
    vec4 tune;
    // x = fresnel_f0, y = (unused/reserved), z = normal_map_index (uintBitsToFloat — sample with floatBitsToUint), w = (reserved)
    vec4 misc;
} push;

const uint WATER_CALM      = 0u;
const uint WATER_RIVER     = 1u;
const uint WATER_RAPIDS    = 2u;
const uint WATER_WATERFALL = 3u;

layout(location = 0) in vec3 vWorldPos;
layout(location = 1) in vec3 vWorldNormal;
layout(location = 2) in vec3 vWorldTangent;
layout(location = 3) in float vWorldBitangentSign;
layout(location = 4) in vec2 vUV;
layout(location = 5) flat in int vInstanceIndex;

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
};

layout(set = 1, binding = 2) uniform accelerationStructureEXT topLevelAS;

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
    return mix(skyTint.xyz, vec3(0.65, 0.7, 0.75), 0.4);
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
    vec3 reflColor = traceWaterRay(vWorldPos, R, REFLECTION_MAX_DIST, skyTint.xyz, reflDist, reflHit);
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
            vec3 hitColor = traceWaterRay(vWorldPos, Tdir, REFRACTION_MAX_DIST, push.deep.rgb, refrDist, refrHit);
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
    vec3 surfaceColor = mix(refrColor, reflColor * push.tune.w, fresnel);

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
}
