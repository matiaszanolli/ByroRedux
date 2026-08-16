// RT reflection helpers — hit UV / hit normal lookup + traceReflection
//
// NON-STANDALONE shader fragment. Included by triangle.frag in dependency
// order via GL_GOOGLE_include_directive; it references symbols (structs,
// SSBO/UBO bindings, helper functions, constants) defined in shader_constants.glsl
// and in earlier includes. Do not compile on its own.

// Defined by lighting.glsl later in triangle.frag's include sequence. A
// prototype lets reflection-hit shading use its deliberately bounded
// one-light evaluator. Diffuse GI and refraction termini retain the wider
// locally-selected light set in `giHitIrradiance`.
vec3 reflectionHitIrradiance(vec3 p, vec3 n, uint dbgFlags);

// ── RT Reflection ───────────────────────────────────────────────────

#include "ray_hit.glsl"

// Cast a reflection ray and return the reflected color.
//
// Return contract (#1029 / REN-D9-NEW-06):
//   * `.rgb` is ALWAYS the final reflection colour the caller should
//     use — sky-tinted ambient blend on miss, distance-attenuated
//     surface texel on hit. Pre-#1029 the two callers (metal + glass)
//     interpreted this inconsistently: glass read `.rgb` directly,
//     while metal weighted the mix by `.a` and collapsed to a
//     separate `ambientFallback` on miss — discarding the
//     `skyTint*0.5 + sceneFlags.yzw*0.5` blend this function pays
//     to compute. One function, two semantics, easy to drift.
//   * `.a` is INFORMATIONAL hit confidence: `1.0 = hit`, `0.0 = miss`.
//     Available to callers that genuinely want to gate on
//     "did the ray hit BVH geometry" (e.g. to skip a follow-on cost
//     that only makes sense on hits). The reflection rgb is already
//     correct without it.
//
// Reflection is a shading ray, so it must resolve the CLOSEST opaque hit.
// `TerminateOnFirstHit` returns a traversal-order candidate, which is only
// valid for binary visibility. Sampling that candidate's material made
// mirrors and smooth walls look semi-transparent whenever it was geometry
// behind the actual reflector.
vec4 traceReflection(vec3 origin, vec3 direction, float maxDist, float mipBias,
                     int selfInstance) {
    // Miss / self-hit fallback colour — sky-tinted ambient (exterior) or
    // cell ambient (interior). Hoisted so a self-intersection hit (below)
    // can reuse it.
    bool _isExt = jitter.w > 0.5;
    vec3 missCol = _isExt ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5)
                          : sceneFlags.yzw;
    // Every caller supplies a scale-aware origin from offsetRayOriginForDirection.
    // Continue alpha/self skips with the same representable-float offset and a
    // zero tMin; no world-space epsilon is valid across all seven games.
    const int MAX_TRANSPARENT_SKIPS = 8;
    vec3 rayOrigin = origin;
    float remaining = maxDist;
    float travelled = 0.0;
    int hitInstanceIdx = -1;
    int hitPrimitiveIdx = 0;
    vec2 hitBary = vec2(0.0);
    vec2 hitUV = vec2(0.0);
    vec3 hitPosition = vec3(0.0);

    for (int layer = 0; layer < MAX_TRANSPARENT_SKIPS; ++layer) {
        rayQueryEXT rq;
        rayQueryInitializeEXT(
            rq, topLevelAS, gl_RayFlagsOpaqueEXT, 0xFF,
            rayOrigin, 0.0, direction, remaining);
        while (rayQueryProceedEXT(rq)) {}
        if (rayQueryGetIntersectionTypeEXT(rq, true)
            == gl_RayQueryCommittedIntersectionNoneEXT) break;

        int candidateIdx =
            rayQueryGetIntersectionInstanceCustomIndexEXT(rq, true);
        int candidatePrim = rayQueryGetIntersectionPrimitiveIndexEXT(rq, true);
        vec2 candidateBary = rayQueryGetIntersectionBarycentricsEXT(rq, true);
        float candidateT = rayQueryGetIntersectionTEXT(rq, true);
        GpuInstance candidateInst = instances[candidateIdx];
        GpuMaterial candidateMat = materials[candidateInst.materialId];
        vec2 candidateUV = resolveRayHitUV(
            uint(candidateIdx),
            uint(candidatePrim),
            candidateBary,
            direction,
            candidateMat);
        vec4 candidateBase;
        bool covered = rayHitHasCoverage(
            uint(candidateIdx), uint(candidatePrim), candidateBary,
            candidateInst, candidateMat, candidateUV, candidateBase);

        if (candidateIdx != selfInstance && covered) {
            hitInstanceIdx = candidateIdx;
            hitPrimitiveIdx = candidatePrim;
            hitBary = candidateBary;
            hitUV = candidateUV;
            hitPosition = rayOrigin + direction * candidateT;
            travelled += candidateT;
            break;
        }

        vec3 hitPoint = rayOrigin + direction * candidateT;
        vec3 candidateNormal = getHitTriNormal(
            uint(candidateIdx), uint(candidatePrim));
        vec3 nextOrigin = offsetRayOriginForDirection(
            hitPoint, candidateNormal, direction);
        float advance = length(nextOrigin - rayOrigin);
        travelled += advance;
        remaining -= advance;
        if (remaining <= 0.0) break;
        rayOrigin = nextOrigin;
    }

    if (hitInstanceIdx < 0) {
        // Miss — return sky tint / ambient mix.
        //
        // For exterior cells the ray escaping the BVH IS escaping into
        // real sky, so the half-sky half-ambient blend mirrors what
        // the composite paints behind the world (via #925's skyTint
        // plumbing).
        //
        // For interior cells the half-sky term is wrong: when
        // `SkyParamsRes` is absent (sealed interior, or no exterior
        // load yet this session), `build_sky_params` returns
        // `SkyParams::default()` with `zenith_color = [0.15, 0.3, 0.6]`
        // (clear-noon-blue) — that signal bleeds into glass refractions
        // / reflections as a daylight tint even in fully sealed cells
        // (Megaton, Vault 21, Markarth subterranean rooms). Drop to
        // cell ambient alone (`sceneFlags.yzw`) on the interior path.
        // The pre-#925 comment claiming "skyTint reads as the cell's
        // ceiling colour" was stale wisdom — interior cells get the
        // default zenith, not a per-cell ceiling derivation. See #1125 /
        // REN-D9-NEW-01.
        return vec4(missCol, 0.0);
    }

    // Look up the committed surface. Transparent alpha holes and the source
    // reflector itself were skipped by the bounded traversal above.
    GpuInstance hitInst = instances[hitInstanceIdx];
    GpuMaterial hitMat = materials[hitInst.materialId];

    // Sample the hit surface's texture × its canonical avgAlbedo (material
    // diffuse_color). The texture alone is the neutral white fallback for
    // untextured / vertex-coloured surfaces, so without the avgAlbedo
    // factor a metal/glass reflection of the Cornell red/green walls reads
    // as flat white. avgAlbedo is the white tint for textured content, so
    // detail is preserved there. Mirrors the refraction-colour fix.
    // `mipBias` softens the reflected image for rough surfaces — a
    // DETERMINISTIC pre-filtered-radiance blur in place of a stochastic
    // GGX-cone jitter, so rough-metal reflections carry no per-frame
    // sampling noise (the caller passes roughness-scaled mip and a sharp
    // reflection ray). Smooth surfaces pass mipBias 0 → razor-sharp.
    //
    // #2919 — this is a SECOND fetch, and deliberately so: the traversal
    // loop's `rayHitHasCoverage` already sampled this surface, but at a
    // hardcoded LOD 0 (`sampleRayHitBase(inst, mat, uv, 0.0)` in
    // `ray_hit.glsl`) because coverage is an alpha test that must not be
    // blurred across a mip. Reusing that sample to "optimise away" the
    // fetch below silently drops the roughness-scaled blur and puts the
    // per-frame sampling noise back into rough-metal reflections. The
    // dead `hitBase` carry-out that used to sit alongside `hitUV` and
    // invite exactly that mistake was removed; every other
    // `rayHitHasCoverage` caller (shadow_transport, water.frag,
    // triangle.frag's path loop) genuinely wants the LOD-0 sample and
    // keeps its own.
    vec3 hitBaseRgb = sampleRayHitBase(hitInst, hitMat, hitUV, mipBias).rgb;
    vec3 hitColor = rayHitAlbedo(hitMat, hitBaseRgb);

    float hitDist = travelled;
    vec3 hitPos = hitPosition;
    vec3 hitN = getHitTriNormal(uint(hitInstanceIdx), uint(hitPrimitiveIdx));
    if (dot(hitN, direction) > 0.0) hitN = -hitN;
    vec3 hitIrradiance = reflectionHitIrradiance(
        hitPos, hitN, floatBitsToUint(jitter.z));
    vec3 hitEmissive = rayHitEmission(hitMat, hitUV, hitBaseRgb, mipBias);
    vec3 hitRadiance = hitColor
        * (hitIrradiance * (1.0 / 3.14159265359) + sceneFlags.yzw)
        + hitEmissive;

    // Exponential distance attenuation: distant reflection detail fades into
    // ambient rather than persisting at near-full strength.
    float distFade = exp(-hitDist * 0.0015);

    // Fade distant surface detail into the correct miss radiance. Fading to
    // black made long indoor reflection rays look like dark transparency.
    return vec4(mix(missCol, hitRadiance, distFade), 1.0);
}
