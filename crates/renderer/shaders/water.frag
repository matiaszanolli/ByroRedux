#version 460
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : require
#extension GL_GOOGLE_include_directive : require
// REN-2026-07-28-02 / #2219 — GpuInstance.skinnedVertexAddress +
// SkinnedVertexRef (include/bindings.glsl) need buffer_reference support.
#extension GL_EXT_buffer_reference : require
#extension GL_ARB_gpu_shader_int64 : require

// Water has storage-image side effects. Force depth/stencil testing before
// fragment execution so fully occluded water neither runs its ray-query tree
// nor deposits caustics through the visible foreground surface.
layout(early_fragment_tests) in;

// Shared shader-side constants (CAUSTIC_FIXED_SCALE, MAT_FLAG_* bits,
// INSTANCE_FLAG_* bits, etc.) — generated from
// `crates/renderer/src/shader_constants_data.rs` by build.rs so the
// Rust + GLSL sides stay byte-equal. Required for #1256's
// `imageAtomicAdd` fixed-point scale match against caustic_splat.comp.
#include "include/shader_constants.glsl"
#include "include/caustic_kernel.glsl"

// ── Water surface fragment shader ─────────────────────────────────────
//
// Renders a transparent water surface for one of four
// `WaterKind` modes selected by `push.timing.y`:
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
//     `reflect(-V, N_perturbed)` and resolves the closest covered material
//     texel. Distance attenuation matches the rest of the pipeline.
//   • Refraction — fired into the opposite side of the view-facing surface
//     with the air↔water eta selected from the camera side. The same
//     material-aware closest-hit walk supplies surface texture, alpha
//     coverage, and emission. Hit distance through the water column drives
//     Beer-Lambert absorption.
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
// Per-water material data lives in a compact per-frame UBO. Keeping the
// 256-byte record here, rather than in push constants, leaves room to grow
// the canonical cross-game material without raising device requirements.
struct WaterParams {
    // x = time (engine uptime in seconds — `TotalTime`, accumulated
    //     since engine start and never reset on cell load; f32, so wave
    //     animation quality degrades after many hours of uptime),
    //     y = WaterKind enum cast to float, z = foam_strength (0..1),
    //     w = ior (1.33 ~ 1.5)
    vec4 timing;
    // xyz = flow direction (unit), w = flow speed (world units / s)
    vec4 flow;
    // rgb = shallow_color (linear), a = fog_near
    vec4 shallow;
    // rgb = deep_color (linear), a = fog_far
    vec4 deep;
    // xy = scroll_a (world units/s), zw = scroll_b
    vec4 scroll;
    // xy = scroll_c for the authored third normal layer; zw = underwater
    // fog near/far for the camera-below-surface presentation.
    vec4 scroll_c;
    // x = uv_scale_a, y = uv_scale_b, z = shoreline_width,
    // w = wave_amplitude (WATR DATA wave_amplitude — #2240)
    vec4 tune;
    // x = fresnel_f0, y = wave_frequency (WATR DATA wave_frequency, Hz — #2240),
    // z = normal_map_index (uintBitsToFloat — sample with floatBitsToUint),
    // w = WATR Sun Specular Power (Blinn-Phong exponent)
    vec4 misc;
    // rgb = reflection_tint (WATR DATA reflection_color — tints geometry-hit
    // colour in traceWaterRay; #1069 / F-WAT-09). a = reflectivity (0..1,
    // moved from tune.w).
    vec4 tint_reflect;
    // Bindless indices for authored NAM2/NAM3/NAM4 noise layers.
    uvec4 noise_indices;
    // x = authored NAM4 UV scale; yzw = authored NAM2/3/4 amplitude scales.
    vec4 detail;
    // x/y/z/w = reflection/refraction/normal/specular depth weights.
    vec4 depth;
    // x/y/z/w = refraction magnitude, local specular power, reflection
    // magnitude, and sun-specular magnitude (including the authored
    // Skyrim specular-properties magnitude multiplier).
    vec4 effects;
    // xyz = Starfield color-absorption ranges in world units; zero means
    // pre-Starfield/legacy water and selects the scalar fog response.
    vec4 absorption;
    // xy = transient ripple center in world XZ, z = intensity, w = radius.
    vec4 ripple;
    // rgb = authored underwater tint, a = underwater fog amount. The
    // underwater near/far ramp lives in `scroll_c.zw` to reuse its reserved
    // legacy slots without changing the wave upload shape.
    vec4 underwater;
};

layout(std140, set = 2, binding = 1) uniform WaterParamsBlock {
    WaterParams params[256];
} waterParams;

layout(push_constant) uniform WaterDrawPush {
    uint waterIndex;
    uvec3 _reserved;
} drawPush;

// Preserve the established `push.field` spelling throughout the shader;
// every reference now resolves through the selected UBO record.
#define push waterParams.params[drawPush.waterIndex]

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
// projection; the selected `WaterParams` UBO record carries every
// per-plane parameter the fragment shader needs, so there's no
// `gl_InstanceIndex`-driven instance lookup on this path.

// Single HDR output — water is a transparent draw, blended onto the
// opaque pass's main colour attachment via standard SRC_ALPHA /
// ONE_MINUS_SRC_ALPHA blending. Bypasses the G-buffer split (no
// normal / motion / mesh-ID writes from water — RT denoising stays on
// the opaque pass).
layout(location = 0) out vec4 outColor;
// FSR reconstruction masks (attachments 6 and 7). Water writes both at full
// strength: its surface colour comes from reflection and refraction of
// geometry that its own depth and motion vectors do not describe, which is
// precisely the transparency-and-composition case. The blend state MAX-
// accumulates these, and the intermediate G-buffer attachments stay masked
// off as before.
layout(location = 6) out float outFsrReactive;
layout(location = 7) out float outFsrTransparency;

// Water uses the same scene descriptors and secondary-hit material contract
// as triangle.frag. The scene set already exposes the material table and
// global vertex/index buffers to FRAGMENT shaders, so this adds no host-side
// descriptor or pipeline-layout surface.
#include "include/bindings.glsl"
#include "include/ray_hit.glsl"
#include "include/shadow_common.glsl"
#include "include/shadow_transport.glsl"

// #1256 / Phase D of #1210 — water-side caustic accumulator.
// Per-FIF R32_UINT storage image owned by WaterCausticAccum (#1255),
// cleared pre-render-pass each frame in `context::draw::draw_frame`,
// written here via `imageAtomicAdd` (single eta + single bounce per
// REN-D13-NEW-04), sampled by composite (#1257, Phase E) alongside
// the existing causticTex. Bound at set 2 binding 0 per the
// WaterPipeline pipeline-layout shape declared in #1255. Binding 1 is the
// material UBO above.
layout(set = 2, binding = 0, r32ui) uniform uimage2D waterCausticAccum;

const float REFLECTION_MAX_DIST = 5000.0;
const float REFRACTION_MAX_DIST = 2000.0;
const float DIST_FALLOFF        = 0.0015; // matches triangle.frag
// No SHORELINE_RAY_MAX here (#2804): the shoreline probe's tMax is the
// authored `push.tune.z` (`shoreline_width`), not a shader constant. The
// dead 256.0 that used to sit here read as the cap and contradicted the
// live default (32.0) by 8x.

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

// `ampScale`/`freqScale` are the WATR-authored `wave_amplitude`/
// `wave_frequency` (#2240), normalised against the engine sentinel
// defaults (`WaterMaterial::default()`: 0.05 / 0.6 Hz — see
// `crates/core/src/ecs/components/water.rs`) so a plane with no
// authored WATR (or one that round-trips the sentinel) reproduces
// the pre-#2240 hardcoded chop exactly; other authored values scale
// the perturbation strength / chop density proportionally.
vec3 sampleScrollingNormal(uint normalMapIndex, vec2 uvBase, vec2 originOffset, vec2 scroll, float scale, float time, float ampScale, float freqScale) {
    if (normalMapIndex == 0xFFFFFFFFu) {
        // Procedural fallback — animated six-octave value-noise gradient.
        // The broad first octave carries the swell while the lighter higher
        // octaves restore the small-scale chop that legacy records lack in
        // their texture slots. This keeps Oblivion/FO3/FNV water from reading
        // as a single smooth undulating sheet.
        //
        // PRECISION BOUND (#1502, rebased #1997): `uvBase` is absolute
        // world XZ (`vWorldPos.xz`, the flat-water branch at the call
        // site) — `hash21`'s sin/fract lattice loses precision and
        // visibly bands past ~176k-unit coordinates. This branch is the
        // DEFAULT for any cell whose water has no bound normal map
        // (every FNV/FO3/Oblivion water plane, plus any WATR with an
        // empty `texture_path` on any game — see `resolve_water_material`
        // in `byroredux/src/env_translate.rs`), not an edge case, so
        // `originOffset` (the render origin projected into the same
        // uvBase space by the caller) is subtracted here to keep the
        // hash input render-origin-relative rather than absolute.
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
        vec2 uv = (uvBase - originOffset) * scale + scroll * time;
        float h0 = valueNoise(uv * 4.0 * freqScale);
        float h1 = valueNoise(uv * 9.0 * freqScale + 17.0);
        float h2 = valueNoise(uv * 16.0 * freqScale + 41.0);
        float h3 = valueNoise(uv * 31.0 * freqScale + 73.0);
        float h4 = valueNoise(uv * 52.0 * freqScale + 113.0);
        float h5 = valueNoise(uv * 83.0 * freqScale + 157.0);
        float h  = h0 * 0.38 + h1 * 0.26 + h2 * 0.16 + h3 * 0.10
                 + h4 * 0.06 + h5 * 0.04;
        const float eps = 1.0;
        float hx = valueNoise(uv * 4.0 * freqScale + vec2(eps, 0.0)) * 0.45
                 + valueNoise(uv * 9.0 * freqScale + vec2(eps, 0.0) + 17.0) * 0.30
                 + valueNoise(uv * 16.0 * freqScale + vec2(eps, 0.0) + 41.0) * 0.15
                 + valueNoise(uv * 31.0 * freqScale + vec2(eps, 0.0) + 73.0) * 0.10
                 + valueNoise(uv * 52.0 * freqScale + vec2(eps, 0.0) + 113.0) * 0.06
                 + valueNoise(uv * 83.0 * freqScale + vec2(eps, 0.0) + 157.0) * 0.04;
        float hy = valueNoise(uv * 4.0 * freqScale + vec2(0.0, eps)) * 0.45
                 + valueNoise(uv * 9.0 * freqScale + vec2(0.0, eps) + 17.0) * 0.30
                 + valueNoise(uv * 16.0 * freqScale + vec2(0.0, eps) + 41.0) * 0.15
                 + valueNoise(uv * 31.0 * freqScale + vec2(0.0, eps) + 73.0) * 0.10
                 + valueNoise(uv * 52.0 * freqScale + vec2(0.0, eps) + 113.0) * 0.06
                 + valueNoise(uv * 83.0 * freqScale + vec2(0.0, eps) + 157.0) * 0.04;
        return normalize(vec3((h - hx) * 0.12 * ampScale, (h - hy) * 0.12 * ampScale, 1.0));
    }
    // PRECISION BOUND (#2496, following #2240): this branch stays
    // ABSOLUTE (not origin-relative like the procedural branch above)
    // because the wrapping sampler needs a seamless UV at a render-origin
    // crossing. #2240 folded the unclamped WATR `freqScale` into this
    // product, scaling both the UV magnitude and its quantization step
    // proportionally with no upper bound — at `uvBase` values up to
    // ~176k (MarkarthWorld) and `freqScale` well above the 1.0 default,
    // the f32 ULP can reach a meaningful fraction of a texel. Fix:
    // subtract only the texel-*integral* part of the origin's own UV —
    // `texture()`'s REPEAT wrap is invariant under subtracting an
    // integer, so the sampled result is unchanged, but the magnitude fed
    // into the texture lookup collapses to `uvBase`'s offset from
    // `originOffset` (small, since fragments render near the camera)
    // plus a bounded fractional remainder.
    vec2 o = floor(originOffset * scale * freqScale);
    vec2 uv = uvBase * scale * freqScale - o + scroll * time;
    vec3 n = texture(textures[nonuniformEXT(normalMapIndex)], uv).xyz;
    n = normalize(n * 2.0 - 1.0);
    // Scale the tangent-space tilt by the authored amplitude, keep the
    // sign of the up component, renormalise (mirrors the procedural path).
    return normalize(vec3(n.xy * ampScale, n.z));
}

// ── RT reflection / refraction ────────────────────────────────────────
//
// Water shades the ray terminus (and uses its distance for absorption), so
// these rays require the committed closest intersection. Any-hit traversal
// is reserved for the binary shadow and shoreline-occupancy queries below.

// `missFallback` is the colour returned on a TLAS miss. Reflection
// callers want the sky tint (light from above the water surface bounces
// back toward the camera). Refraction callers want the cell's deep
// water tint — pre-#1015 a single hardcoded `skyTint` return painted a
// faint sky cast through `absorbWaterColumn`'s ~14% surface-radiance
// term on miss (downward refraction rays escaping the BLAS at cliff
// edges or sparse exterior cells).
vec3 traceWaterRay(
    vec3 origin,
    vec3 direction,
    float maxDist,
    vec3 missFallback,
    out float hitDist,
    out bool hit
) {
    // #1561 — match triangle.frag / caustic_splat.comp: skip the ray query
    // when RT is unsupported OR the TLAS was not written this frame.
    // `sceneFlags.x` is 1.0 only when ray_query is supported AND the TLAS is
    // current (draw.rs), so this single gate covers both the non-RT-hardware
    // case (binding 2 may be absent from the bound layout) and the RT first-
    // frame / TLAS-failure stale-TLAS readback. Degrades to the miss
    // fallback (sky reflection / deep-water refraction) instead of tracing.
    if (sceneFlags.x < 0.5) {
        hit = false;
        hitDist = maxDist;
        return missFallback;
    }
    // Alpha-tested leaves, grates, and other cutouts must not become solid
    // slabs in water. Ray queries cannot invoke the raster material's alpha
    // test, so resolve the closest candidate, sample its real UV/material,
    // and continue behind uncovered texels. This is the same bounded
    // contract as traceReflection; eight layers prevents pathological
    // foliage stacks from turning a water pixel into an unbounded walk.
    const int MAX_TRANSPARENT_SKIPS = 8;
    vec3 rayOrigin = origin;
    float travelled = 0.0;
    float remaining = maxDist;

    for (int layer = 0; layer < MAX_TRANSPARENT_SKIPS; ++layer) {
        rayQueryEXT rq;
        rayQueryInitializeEXT(
            rq, topLevelAS,
            gl_RayFlagsOpaqueEXT, 0xFF,
            rayOrigin, 0.0, direction, remaining
        );
        while (rayQueryProceedEXT(rq)) {}

        if (rayQueryGetIntersectionTypeEXT(rq, true)
            == gl_RayQueryCommittedIntersectionNoneEXT) {
            hit = false;
            hitDist = maxDist;
            return missFallback;
        }

        float localT = rayQueryGetIntersectionTEXT(rq, true);
        uint instIdx = uint(
            rayQueryGetIntersectionInstanceCustomIndexEXT(rq, true));
        uint primIdx = uint(
            rayQueryGetIntersectionPrimitiveIndexEXT(rq, true));
        vec2 bary = rayQueryGetIntersectionBarycentricsEXT(rq, true);

        GpuInstance inst = instances[instIdx];
        GpuMaterial mat = materials[inst.materialId];
        vec2 uv = resolveRayHitUV(
            instIdx, primIdx, bary, direction, mat);
        vec4 baseSample;

        if (rayHitHasCoverage(
                instIdx, primIdx, bary, inst, mat, uv, baseSample)) {
            hit = true;
            hitDist = travelled + localT;

            // A secondary water ray needs the visible terminus, not the
            // instance-wide average used by the old shortcut. Texture ×
            // diffuse plus authored emission is a stable, bounded surface-
            // colour proxy that preserves detail without firing another
            // light/shadow tree from every reflection and refraction hit.
            return rayHitAlbedo(mat, baseSample.rgb)
                 + rayHitEmission(mat, uv, baseSample.rgb, 0.0);
        }

        vec3 hitPoint = rayOrigin + direction * localT;
        vec3 nextOrigin = offsetRayOriginForDirection(
            hitPoint, getHitTriNormal(instIdx, primIdx), direction);
        float advance = length(nextOrigin - rayOrigin);
        travelled += advance;
        remaining = maxDist - travelled;
        if (remaining <= 0.0) {
            break;
        }
        rayOrigin = nextOrigin;
    }

    hit = false;
    hitDist = maxDist;
    return missFallback;
}

// ── Beer-Lambert through the water column ─────────────────────────────
//
// `hitDist` is the refraction-ray length under water. Attenuation ramps
// across the authored WATR fog range: clear until `fog_near`, then
// darkening to full `deep_color` by `fog_far`.
//
// #2785 (REN-D15-04) — `fog_near` (`push.shallow.a`) travelled the entire
// EXAL water arm and nothing read it: `t` was `hitDist / fog_far`, so the
// curve was identical for every water body in every game and the authored
// near plane was discarded at the last step.
//
// It is the NEAR PLANE of a linear fog range, not a "50% mix distance" —
// which is what the component docs claimed, and what made this look like a
// one-line `exp2(-d/fog_near)` fix. Measured over vanilla:
//
//   Skyrim.esm  34 WATR — fog_near 0 for most (BlackreachWater 0/290,
//                         MarkarthWater 0/110, HorseTroughWater01 220/4710)
//   FalloutNV   78 WATR — 0 for most (NVCleanWaterGS 7/58)
//   Oblivion    23 WATR — median fog_near/fog_far = 0.001
//
// A half-distance reading would set the 50% point to ZERO for nearly all
// vanilla water and turn every pond opaque. As a ramp start it is
// bit-identical to the old curve wherever `fog_near == 0` (the vanilla
// majority) and only gives the authored clear margin back to the few
// bodies that ask for one. Same pair semantics the cell-lighting fog
// already uses (`components.rs`: "evaluating `fog_near..fog_far`").
//
// The `exp(-2t)` shape itself is unchanged and still empirical.
vec3 absorbWaterColumn(vec3 refractedRadiance, float hitDist, bool cameraUnderwater) {
    bool hasUnderwaterRamp = push.scroll_c.w > push.scroll_c.z + 0.001;
    float fogNear = cameraUnderwater && hasUnderwaterRamp
        ? push.scroll_c.z
        : push.shallow.a;
    float fogFar = cameraUnderwater && hasUnderwaterRamp
        ? push.scroll_c.w
        : push.deep.a;
    // The ESM parser already clamps `fog_far >= fog_near + 1`, so the span
    // is positive; `max` covers a hand-built push block.
    float span = max(fogFar - fogNear, 1.0);
    float t = clamp((hitDist - fogNear) / span, 0.0, 1.0);
    float fogAmount = cameraUnderwater
        ? clamp(push.underwater.a, 0.0, 8.0)
        : 1.0;
    float absorption = exp(-t * 2.0 * max(push.depth.y * fogAmount, 0.0));
    vec3 authoredRanges = max(push.absorption.rgb, vec3(0.0));
    // Starfield authors independent red/green/blue absorption distances.
    // Preserve the legacy scalar path when the triplet is zero, otherwise
    // apply the per-channel Beer–Lambert transmission on top of the shared
    // near/far ramp. This keeps older games byte-compatible while preventing
    // Starfield oceans from collapsing to a generic blue tint.
    vec3 channelTransmission = vec3(1.0);
    if (any(greaterThan(authoredRanges, vec3(0.0)))) {
        channelTransmission = exp(-hitDist / max(authoredRanges, vec3(0.01)));
    }
    vec3 transmission = clamp(channelTransmission * absorption, 0.0, 1.0);
    vec3 shallowTint = cameraUnderwater
        ? mix(push.shallow.rgb, push.underwater.rgb, clamp(push.underwater.a, 0.0, 1.0))
        : push.shallow.rgb;
    vec3 deepTint = cameraUnderwater
        ? mix(push.deep.rgb, push.underwater.rgb, clamp(push.underwater.a, 0.0, 1.0))
        : push.deep.rgb;
    return mix(deepTint, refractedRadiance * shallowTint, transmission);
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
    // #1561 — no RT / unwritten TLAS ⇒ no shoreline foam (same gate as
    // traceWaterRay / caustic_splat.comp).
    if (sceneFlags.x < 0.5) {
        return 0.0;
    }
    rayQueryEXT rq;
    vec3 rayDirection = -surfaceNormal;
    vec3 rayOrigin = offsetRayOriginForDirection(
        worldPos, surfaceNormal, rayDirection);
    rayQueryInitializeEXT(
        rq, topLevelAS,
        gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFF,
        rayOrigin, 0.0, rayDirection, push.tune.z
    );
    rayQueryProceedEXT(rq);
    if (rayQueryGetIntersectionTypeEXT(rq, true) == gl_RayQueryCommittedIntersectionNoneEXT) {
        return 0.0;
    }
    float depthToGround = rayQueryGetIntersectionTEXT(rq, true);
    return 1.0 - smoothstep(0.0, push.tune.z, depthToGround);
}

float foamFlowStreaks(vec3 worldPos, vec3 originOffset, float time) {
    // Project worldPos onto the flow tangent + a perpendicular tangent
    // to get streak coords. Streaks scroll with the flow.
    //
    // PRECISION BOUND (#1502, rebased #1997 for `sampleScrollingNormal`,
    // rebased here #2469): `worldPos` is absolute world-space
    // (`vWorldPos`, passed in by every call site) and feeds `valueNoise`'s
    // `hash21` lattice below, same as `sampleScrollingNormal`. Subtract
    // the render origin before projecting so the hash input stays
    // origin-relative instead of collapsing at Tamriel/Mojave-scale
    // coordinates.
    vec3 flowDir = push.flow.xyz;
    float speed  = push.flow.w;
    // Build a perpendicular in the surface tangent plane.
    vec3 perpRaw = cross(vWorldNormal, flowDir);
    vec3 perp = length(perpRaw) > 1.0e-5
        ? perpRaw / length(perpRaw)
        : (abs(vWorldNormal.y) < 0.9
            ? normalize(cross(vWorldNormal, vec3(0.0, 1.0, 0.0)))
            : vec3(1.0, 0.0, 0.0));
    vec3 relPos = worldPos - originOffset;
    float u = dot(relPos, flowDir) - speed * time;
    float v = dot(relPos, perp);
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

float rainSurfaceNoise(vec2 uv, float time) {
    vec2 p = uv * 0.32 + vec2(time * 0.37, -time * 0.29);
    float c = valueNoise(p);
    float dx = valueNoise(p + vec2(0.035, 0.0)) - c;
    float dy = valueNoise(p + vec2(0.0, 0.035)) - c;
    return clamp(length(vec2(dx, dy)) * 10.0, 0.0, 1.0);
}

void main() {
    outFsrReactive = 1.0;
    outFsrTransparency = 1.0;

    // ── Setup ──
    float time = push.timing.x;
    uint  kind = uint(push.timing.y + 0.5);
    float foamStrength = push.timing.z;
    float ior  = push.timing.w;
    uint  normalMapIndex = floatBitsToUint(push.misc.z);
    uint  noiseMapA = push.noise_indices.x;
    uint  noiseMapB = push.noise_indices.y;
    uint  noiseMapC = push.noise_indices.z;
    // WATR.ANAM is packed into the otherwise-unused fourth noise-index slot
    // so the compact 12-vec4 UBO ABI remains stable.
    float authoredOpacity = clamp(uintBitsToFloat(push.noise_indices.w), 0.0, 1.0);
    // #2240 — WATR-authored wave_amplitude/wave_frequency, normalised
    // against the WaterMaterial sentinel default (see
    // `sampleScrollingNormal`'s doc comment above).
    float ampScale  = push.tune.w / 0.05;
    float freqScale = push.misc.y / 0.6;

    // Imported NIF water meshes are not guaranteed to carry a valid tangent
    // frame (old NiTriShapes commonly have a zero tangent or a tangent
    // parallel to the normal). Never normalize those values directly: a
    // single non-finite basis vector poisons every reflection/refraction
    // result in the fragment. Build a deterministic orthonormal fallback.
    vec3 NsurfaceRaw = vWorldNormal;
    vec3 Nsurface = length(NsurfaceRaw) > 1.0e-5
        ? normalize(NsurfaceRaw)
        : vec3(0.0, 1.0, 0.0);
    vec3 tangentRaw = vWorldTangent;
    vec3 tangentProjected = tangentRaw - Nsurface * dot(tangentRaw, Nsurface);
    vec3 T;
    if (length(tangentProjected) > 1.0e-5) {
        T = normalize(tangentProjected);
    } else {
        vec3 fallbackAxis = abs(Nsurface.y) < 0.9
            ? vec3(0.0, 1.0, 0.0)
            : vec3(1.0, 0.0, 0.0);
        T = normalize(cross(fallbackAxis, Nsurface));
    }
    vec3 bitangentRaw = cross(Nsurface, T);
    vec3 B = length(bitangentRaw) > 1.0e-5
        ? normalize(bitangentRaw) * vWorldBitangentSign
        : vec3(0.0, 0.0, 1.0);
    mat3 TBN = mat3(T, B, Nsurface);

    vec3 V = normalize(cameraPos.xyz - vWorldPos);
    // Determine the camera side from the authored geometric normal rather
    // than assuming +Y. This keeps rotated legacy mesh-water planes correct;
    // explicit waterfall sheets still use their ordinary two-sided tint path.
    bool cameraUnderwater = kind != WATER_WATERFALL
        && dot(cameraPos.xyz - vWorldPos, Nsurface) < 0.0;

    // ── Wave UVs ──
    // For flat surfaces (Calm/River/Rapids), drive the UV from world
    // XZ so the surface texture is continuous across the cell grid
    // (no seams at quad edges). For waterfalls, the UV runs along
    // the surface tangent (mesh-provided flow axis) so the sheet
    // scrolls down naturally.
    vec2 uvWorld;
    vec2 uvOrigin;
    if (kind == WATER_WATERFALL) {
        // Project world position onto the flow tangent (T) for v,
        // and the bitangent for u. The vertex shader sets up T as
        // the mesh tangent — for a waterfall the artist authors that
        // pointing along the fall direction.
        uvWorld = vec2(dot(vWorldPos, B), dot(vWorldPos, T));
        // Render origin projected into the same tangent-space basis, so
        // the procedural branch of `sampleScrollingNormal` can rebase
        // its hash input origin-relative (#1997) without disturbing the
        // textured branch's absolute (wrapping) UV.
        uvOrigin = vec2(dot(renderOrigin.xyz, B), dot(renderOrigin.xyz, T));
    } else {
        // Use world XZ — flat-plane water.
        uvWorld = vWorldPos.xz;
        uvOrigin = renderOrigin.xz;
    }

    // Two scrolling normal layers (the "movement on flat surfaces"
    // case). For River/Rapids/Waterfall, layer A's scroll vector is
    // baked from `flow` on the CPU side, so we don't have to branch
    // here. Push constants carry the final scroll vectors.
    vec3 nA = sampleScrollingNormal(noiseMapA, uvWorld, uvOrigin, push.scroll.xy, push.tune.x, time, ampScale * max(push.detail.y, 0.05) * max(push.depth.z, 0.0), freqScale);
    vec3 nB = sampleScrollingNormal(noiseMapB, uvWorld, uvOrigin, push.scroll.zw, push.tune.y, time, ampScale * max(push.detail.z, 0.05) * max(push.depth.z, 0.0), freqScale);

    // A distinct authored NAM4 layer contributes on every horizontal water
    // kind. Rapids uses the faster flow-biased path for whitewater; calm and
    // river surfaces use the slower primary scroll so legacy sentinel slots
    // (where all three indices are identical) remain exact no-ops.
    vec3 nMix;
    bool hasAuthoredThirdLayer = noiseMapC != noiseMapA && noiseMapC != noiseMapB;
    if (kind == WATER_RAPIDS || hasAuthoredThirdLayer) {
        vec2 thirdScroll = kind == WATER_RAPIDS
            ? vec2(push.flow.x, push.flow.z) * push.flow.w * 2.0
            : push.scroll_c.xy;
        float thirdWeight = kind == WATER_RAPIDS ? 0.7 : 0.35;
        vec3 nC = sampleScrollingNormal(
            noiseMapC,
            uvWorld,
            uvOrigin,
            thirdScroll,
            push.detail.x,
            time,
            ampScale * max(push.detail.w, 0.05) * max(push.depth.z, 0.0),
            freqScale
        );
        nMix = normalize(nA + nB + nC * thirdWeight);
    } else {
        nMix = normalize(nA + nB);
    }

    // Surface-interaction ripple. `RippleEvent` is emitted on the water
    // plane for the same frame as the particle/audio disturbance. Keep the
    // perturbation local and bounded: a narrow annulus produces the radial
    // wavefront while the underlying authored/procedural normals remain in
    // control everywhere else. A zero intensity is the canonical no-event
    // sentinel for all ordinary frames.
    if (push.ripple.z > 0.0) {
        vec2 delta = vWorldPos.xz - push.ripple.xy;
        float distanceToCenter = length(delta);
        float radius = max(push.ripple.w, 0.5);
        float width = mix(1.5, 3.5, clamp(push.ripple.z, 0.0, 1.0));
        float ring = exp(-pow((distanceToCenter - radius) / width, 2.0))
            * clamp(push.ripple.z, 0.0, 1.0);
        vec2 radial = distanceToCenter > 0.001
            ? delta / distanceToCenter
            : vec2(0.0);
        nMix = normalize(nMix + vec3(radial * ring * 0.28, 0.0));
    }
    float rainIntensity = clamp(push.absorption.w, 0.0, 1.0);
    if (rainIntensity > 0.0) {
        float rainNoise = rainSurfaceNoise(uvWorld, time);
        float rainPerturbation = rainNoise * 0.06 * rainIntensity;
        nMix = normalize(nMix + vec3(rainPerturbation, rainPerturbation, 0.0));
    }

    // Tangent → world space.
    vec3 Nperturbed = normalize(TBN * nMix);
    // Caustics are sunlight entering the water from the authored top side;
    // keep that wave normal before the camera-facing orientation below. The
    // view normal is intentionally flipped for underwater shading, but using
    // that flipped value for Snell transport makes the caustic footprint
    // change when the camera crosses the waterline.
    vec3 causticNormal = Nperturbed;

    // Orient the shading surface toward the viewer. Water is transmissive
    // from both sides: reflection stays on the camera side (+N), refraction
    // crosses to the opposite side (-N), and eta reverses below the surface.
    // Keeping `Nsurface` separate preserves the authored above→below
    // convention for shoreline and sunlight-caustic rays.
    bool viewFromPositiveSide = dot(Nsurface, V) >= 0.0;
    vec3 N = viewFromPositiveSide ? Nsurface : -Nsurface;
    if (!viewFromPositiveSide) {
        Nperturbed = -Nperturbed;
    }

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

    // Keep caustic transport in the top-side half-space even when a strongly
    // tilted normal map sample points below the geometric plane. This mirrors
    // the shading stability clamp without coupling it to the camera side.
    float causticNormalDotSurface = dot(causticNormal, Nsurface);
    if (causticNormalDotSurface < NORMAL_PLANE_EPS) {
        causticNormal = normalize(
            causticNormal + Nsurface * (NORMAL_PLANE_EPS - causticNormalDotSurface)
        );
    }

    // ── Fresnel ──
    float NdotV = max(dot(Nperturbed, V), 0.0);
    float F0    = push.misc.x;
    float fresnel = F0 + (1.0 - F0) * pow(1.0 - NdotV, 5.0);

    // ── Reflection ray ──
    vec3 R = reflect(-V, Nperturbed);
    float reflDist; bool reflHit;
    // Reflection miss follows the same environment contract as the main
    // material path: exterior rays see the weather sky; interior rays see
    // the cell ambient. Feeding an interior miss from `skyTint` is what
    // turned cave water into a bright, nearly white slab anywhere the
    // reflected ray escaped sparse TLAS geometry.
    vec3 reflectionMiss = jitter.w > 0.5 ? skyTint.xyz : sceneFlags.yzw;
    vec3 reflColor = traceWaterRay(
        offsetRayOriginForDirection(vWorldPos, N, R),
        R,
        REFLECTION_MAX_DIST,
        reflectionMiss,
        reflDist,
        reflHit
    );
    if (reflHit) {
        reflColor *= exp(-reflDist * DIST_FALLOFF);
    }
    // No miss re-select here (#2804): every `hit = false` path in
    // `traceWaterRay` already returns `missFallback`, which is exactly
    // `reflectionMiss`, so the former
    // `mix(reflectionMiss, reflColor, reflHit ? 1.0 : 0.0)` selected
    // `reflColor` in both branches.
    // WATR DATA reflection_color is a filter on reflected radiance. It must
    // not be mixed into the shared ray terminus, because that contaminates
    // the refraction branch with a reflection-only material parameter.
    reflColor *= push.tint_reflect.rgb;
    reflColor *= max(push.depth.x, 0.0)
        * max(push.effects.z, 0.0);

    // ── Refraction ray (skipped for waterfalls) ──
    vec3 refrColor;
    float refrDist = push.deep.a; // default: full deep tint on skip
    if (kind != WATER_WATERFALL) {
        float eta = viewFromPositiveSide
            ? (1.0 / max(ior, 1.0))
            : max(ior, 1.0);
        // Skyrim's Refraction Magnitude is a bounded normal-distortion
        // weight (the vanilla default is 9). Legacy materials leave it at
        // zero and retain the fully perturbed normal path.
        float refractionNormalWeight = push.effects.x > 0.0
            ? clamp(push.effects.x / 10.0, 0.15, 1.0)
            : 1.0;
        vec3 refractionNormal = normalize(mix(N, Nperturbed, refractionNormalWeight));
        vec3 Tdir = refract(-V, refractionNormal, eta);
        bool refrHit;
        // If TIR (total internal reflection) — possible only while viewing
        // from the water side — refract returns zero and all energy goes to
        // the already-resolved reflection.
        if (length(Tdir) > 0.001) {
            // Refraction-miss: deep water tint is the right backdrop
            // (the downward ray escaped the BLAS — cliff edge / sparse
            // exterior — but conceptually it should land in the deep
            // water column, NOT in the sky above). #1015.
            vec3 hitColor = traceWaterRay(
                offsetRayOriginForDirection(vWorldPos, N, Tdir),
                Tdir,
                REFRACTION_MAX_DIST,
                push.deep.rgb,
                refrDist,
                refrHit
            );
            refrColor = absorbWaterColumn(
                hitColor,
                refrHit ? refrDist : push.deep.a,
                cameraUnderwater
            );
        } else {
            refrColor = reflColor;
            fresnel = 1.0;
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
        foamMask += foamShoreline(vWorldPos, Nsurface) * 1.0;
    }
    if (kind == WATER_RAPIDS) {
        foamMask += foamFlowStreaks(vWorldPos, renderOrigin.xyz, time) * 0.85;
        foamMask += foamCrest(Nperturbed, N) * 0.7;
    } else if (kind == WATER_RIVER) {
        foamMask += foamFlowStreaks(vWorldPos, renderOrigin.xyz, time) * 0.25;
    } else if (kind == WATER_WATERFALL) {
        // Sheet foam: more at the top and bottom of the falling
        // surface. We don't have a normalised sheet coordinate
        // without extra push-constant plumbing, so approximate with
        // a streak pattern at very high speed for that "fizzing
        // sheet" read.
        foamMask += foamFlowStreaks(vWorldPos, renderOrigin.xyz, time * 1.6) * 0.95;
        foamMask += foamCrest(Nperturbed, N) * 0.45;
    }
    foamMask = clamp(foamMask * foamStrength, 0.0, 1.0);
    if (rainIntensity > 0.0) {
        foamMask = clamp(foamMask + rainSurfaceNoise(uvWorld * 1.7, time * 1.3)
            * 0.08 * rainIntensity, 0.0, 1.0);
    }

    // ── Direct-sun glint ──
    // `sunDirection.xyz` points from the surface toward the sun. WATR's
    // Sun Specular Power is an exponent (not a brightness): high values
    // make calm water sparkle in a tight moving lobe while low values
    // spread the highlight over rough/choppy water. `sunDirection.w`
    // carries the active sun intensity and is zero for interiors/night.
    float sunVisibility = 0.0;
    vec3 sunDir = vec3(0.0, 1.0, 0.0);
    if (sunDirection.w > 0.0) {
        sunDir = normalize(sunDirection.xyz);
        sunVisibility = 1.0;
        if (sceneFlags.x >= 0.5) {
            vec3 sunTransmission = traceShadowTransmittance(
                offsetRayOrigin(vWorldPos, Nsurface),
                sunDir,
                DIRECTIONAL_SHADOW_TRACE_DISTANCE,
                0.0,
                VISIBILITY_MASK_FULL
            );
            sunVisibility = dot(
                sunTransmission, vec3(0.2126, 0.7152, 0.0722));
        }
    }
    vec3 sunHalfVector = V + sunDir;
    float sunHalfLength = length(sunHalfVector);
    float NdotSunHalf = sunHalfLength > 1e-5
        ? max(dot(Nperturbed, sunHalfVector / sunHalfLength), 0.0)
        : 0.0;
    float sunSpecular = pow(
        NdotSunHalf,
        clamp(push.effects.y > 0.0 ? push.effects.y : push.misc.w, 1.0, 2048.0)
    ) * sunDirection.w * sunVisibility
        * max(push.depth.w, 0.0)
        * max(push.effects.w, 0.0);

    // Forward sunlight scattering through the water column. This is the
    // low-frequency companion to the authored sun glint above: it makes
    // shallow/clear water glow toward the sun instead of reading as a flat
    // refraction tint. The term is deliberately bounded and reuses the
    // already shadowed sunVisibility, so it costs no additional ray query.
    // `sunDirection` points from the surface toward the sun; the incident
    // light direction is therefore `-sunDir` for the reflected phase.
    float sunHeight = max(sunDir.y, 0.0);
    vec3 scatterBase = mix(push.deep.rgb, push.shallow.rgb, 0.35);
    vec3 scatterColour = mix(
        scatterBase * vec3(1.0, 0.45, 0.20),
        scatterBase,
        clamp(1.0 - exp(-sunHeight * 4.0), 0.0, 1.0)
    );
    float scatterLambert = max(dot(sunDir, Nperturbed) * 0.7 + 0.3, 0.0);
    float scatterReflectAngle = max(
        dot(reflect(-sunDir, Nperturbed), V) * 2.0 - 1.2,
        0.0
    );
    float lightScatter = scatterLambert * scatterReflectAngle
        * 0.30 * sunDirection.w * sunVisibility
        * max(1.0 - exp(-sunHeight), 0.0);
    refrColor = mix(refrColor, scatterColour, clamp(lightScatter, 0.0, 1.0));

    // ── Surface colour ──
    vec3 surfaceColor = mix(refrColor, reflColor * push.tint_reflect.w, fresnel);
    surfaceColor += vec3(sunSpecular);

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
    float baseAlpha = authoredOpacity;
    float grazingBoost = pow(1.0 - NdotV, 2.0) * 0.1;
    float alpha = baseAlpha <= 0.0
        ? 0.0
        : clamp(baseAlpha + grazingBoost + foamMask * 0.1, 0.0, 1.0);

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
    // #1561 — `sceneFlags.x >= 0.5` gates the caustic shadow + floor ray
    // queries below (mirrors the helper gates): no RT / unwritten TLAS ⇒ no
    // caustic projection rather than a trace against an absent / stale TLAS.
    if (sunDirection.w > 0.0 && sceneFlags.x >= 0.5) {
        // INVARIANT (REG-03 / #1635, #1459): `sunDirection.xyz` points TO the
        // sun (light-incoming), matching GpuLight.direction_angle /
        // triangle.frag's directional `L` AND the caustic_splat.comp
        // directional branch. The light-*travel* direction (sun → surface) is
        // therefore `-sunDir`. Flipping this sign suppresses caustics for an
        // overhead sun (the #1459 bug) — keep it consistent with
        // caustic_splat.comp.
        // The surface-glint block above already evaluated the shared,
        // material-aware sun visibility, so caustics reuse it without a
        // second shadow query per water fragment.
        bool sunVisible = sunVisibility > 0.001;
        if (sunVisible) {
            // 2. Snell refraction. refract() takes the incident *propagation*
            // direction (light travel = sun → surface = `-sunDir`), NOT the
            // to-sun direction. refract() returns vec3(0) on total-internal-
            // reflection, which can't happen for light entering the denser
            // medium from above — but length-gate anyway in case grazing.
            //
            // #REN-D15-01 — refract through the wave-perturbed normal, not
            // the flat plane normal. Nsurface is constant (0,1,0) for every
            // fragment of a flat water plane, so refracting through it
            // produces a rigid, structureless translation of the water
            // plane's screen footprint instead of a focused caustic — the
            // same Nperturbed already used by the primary refraction ray
            // above (line ~547) is required to focus light into a caustic
            // pattern.
            vec3 refractDir = refract(-sunDir, causticNormal, 1.0 / 1.33);
            if (length(refractDir) > 1e-4) {
                // 3. Find floor via TLAS ray (single bounce).
                //
                // Select the transmission side from refractDir and advance
                // by representable floats. This starts below the surface
                // without a fixed engine-unit epsilon and shares the same
                // zero-tMin contract as triangle and caustic transport.
                rayQueryEXT floorRq;
                vec3 floorOrigin = offsetRayOriginForDirection(
                    vWorldPos, Nsurface, refractDir);
                rayQueryInitializeEXT(
                    floorRq, topLevelAS,
                    gl_RayFlagsOpaqueEXT, 0xFF,
                    floorOrigin, 0.0, refractDir, 5000.0
                );
                while (rayQueryProceedEXT(floorRq)) {}
                if (rayQueryGetIntersectionTypeEXT(floorRq, true)
                    != gl_RayQueryCommittedIntersectionNoneEXT) {
                    float floorT = rayQueryGetIntersectionTEXT(floorRq, true);
                    vec3 floorWorld = vWorldPos + refractDir * floorT;
                    // 4. Project floor hit to screen-space.
                    // #markarth-precision / #1488 — `floorWorld` is ABSOLUTE
                    // (vWorldPos arrives absolute for the TLAS trace) but
                    // `viewProj` is camera-RELATIVE; rebase before projecting
                    // or the uv01 guard below drops every deposit whenever
                    // render_origin != 0.
                    vec4 floorClip = viewProj * vec4(floorWorld - renderOrigin.xyz, 1.0);
                    if (floorClip.w > 0.0) {
                        vec2 ndc = floorClip.xy / floorClip.w;
                        vec2 uv01 = ndc * 0.5 + 0.5;
                        // #2784 (REN-D15-03) — reject on the INTEGER pixel
                        // against the image size, the way `caustic_splat.comp`
                        // does, instead of on the float uv. The old guard was
                        // `lessThanEqual(uv01, vec2(1.0))`, so `uv01.x == 1.0`
                        // exactly produced `pixel.x == screen.x` — one past the
                        // last texel. Harmless in practice only because Vulkan
                        // defines out-of-range image writes as discarded, and
                        // that robustness rule was also the sole thing keeping
                        // the wholesale conversion in bounds when the pass runs
                        // against the 1x1 `placeholder_caustic_sink` fallback.
                        // Making the bound explicit means the splat no longer
                        // depends on it, and the two caustic writers now state
                        // the same rule.
                        ivec2 pixel = ivec2(uv01 * screen.xy);
                        ivec2 causticSize = ivec2(screen.xy);
                        if (all(greaterThanEqual(pixel, ivec2(0)))
                            && all(lessThan(pixel, causticSize))) {
                            // 5. Directional weighting — caustic
                            // intensity scales with how
                            // perpendicular the water surface is
                            // to the sun (Lambert cosine on the
                            // light side). Grazing sun = dim
                            // caustic; noon sun overhead = full.
                            // Travel falloff matches caustic_splat
                            // (1 / (1 + t²·k)) — caustics fade with
                            // depth as the refracted column spreads.
                            // Matches the refraction normal above (Nperturbed) —
                            // using the flat Nsurface here would weight the
                            // caustic by the plane's macro facing instead of
                            // the same wave-perturbed geometry that focused it.
                            float NdotSun = max(dot(causticNormal, sunDir), 0.0);
                            float travelFall = 1.0 / (1.0 + floorT * floorT * 1e-4);
                            float contrib = sunDirection.w * sunVisibility
                                * NdotSun * travelFall;
                            float scale = CAUSTIC_FIXED_SCALE;
                            float clamp_max = float(0xFFFFFFFFu) / scale;
                            // Match the glass writer's normalised 5x5
                            // footprint. Water intentionally starts from a
                            // clear accumulator every frame: its perturbed
                            // normals animate even while the camera is parked,
                            // so the glass path's static-scene EMA would leave
                            // moving, misregistered trails here.
                            for (int ky = -2; ky <= 2; ++ky) {
                                for (int kx = -2; kx <= 2; ++kx) {
                                    ivec2 q = pixel + ivec2(kx, ky);
                                    if (any(lessThan(q, ivec2(0)))
                                        || any(greaterThanEqual(q, causticSize))) {
                                        continue;
                                    }
                                    float depositF = clamp(
                                        contrib * causticGauss5Weight(kx, ky) * scale,
                                        0.0,
                                        clamp_max
                                    );
                                    uint fixedVal = uint(depositF);
                                    if (fixedVal != 0u) {
                                        imageAtomicAdd(waterCausticAccum, q, fixedVal);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
