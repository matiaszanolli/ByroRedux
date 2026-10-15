#ifndef BYRO_SHADOW_TRANSPORT_GLSL
#define BYRO_SHADOW_TRANSPORT_GLSL

// Shadow transport is included by both triangle.frag (which also includes the
// PBR helpers) and water.frag (which deliberately does not). Keep its scalar
// dielectric Fresnel self-contained so either shader can rebuild independently.
float shadowFresnelSchlickScalar(float cosTheta, float f0) {
    float x = clamp(1.0 - cosTheta, 0.0, 1.0);
    float x2 = x * x;
    float weight = x2 * x2 * x;
    return f0 + (1.0 - f0) * weight;
}

// Material-aware RGB visibility along a light segment. Opaque geometry is an
// alpha-aware binary blocker; glass accumulates tint, absorption and Fresnel
// loss interface by interface.  Callers must include bindings.glsl and
// ray_hit.glsl and shadow_common.glsl before this file.
bool advanceShadowRayPastHit(
    inout vec3 origin,
    inout float remaining,
    vec3 direction,
    float hitT
) {
    vec3 hitPoint = origin + direction * hitT;
    vec3 nextOrigin = offsetRayOrigin(hitPoint, direction);
    remaining -= length(nextOrigin - origin);
    origin = nextOrigin;
    return remaining > 0.0;
}

vec3 traceShadowTransmittanceDetailed(
    vec3 origin, vec3 direction, float maxDist, float emitterRadius,
    uint visibilityMask,
    out uint committedInstance,
    out float committedDistance
) {
    committedInstance = 0xFFFFFFFFu;
    committedDistance = uintBitsToFloat(0x7F800000u);
    // MAX_ALPHA_SKIP_LAYERS is the shared 8-layer budget (#2265 / TD7-001;
    // shader_constants.glsl).
    vec3 opaqueOrigin = origin;
    float opaqueRemaining = maxDist;
    uint opaqueMask = visibilityMask & VISIBILITY_MASK_ALL_OPAQUE;
    for (int layer = 0; layer < int(MAX_ALPHA_SKIP_LAYERS); ++layer) {
        if (opaqueMask == 0u) break;
        rayQueryEXT opaqueRQ;
        rayQueryInitializeEXT(
            opaqueRQ, topLevelAS, gl_RayFlagsOpaqueEXT,
            opaqueMask,
            opaqueOrigin, 0.0, direction, opaqueRemaining);
        while (rayQueryProceedEXT(opaqueRQ)) {}
        if (rayQueryGetIntersectionTypeEXT(opaqueRQ, true)
            == gl_RayQueryCommittedIntersectionNoneEXT) break;

        int hitIdx = rayQueryGetIntersectionInstanceCustomIndexEXT(opaqueRQ, true);
        int hitPrim = rayQueryGetIntersectionPrimitiveIndexEXT(opaqueRQ, true);
        vec2 hitBary = rayQueryGetIntersectionBarycentricsEXT(opaqueRQ, true);
        float hitT = rayQueryGetIntersectionTEXT(opaqueRQ, true);
        if (committedInstance == 0xFFFFFFFFu) {
            committedInstance = uint(hitIdx);
            committedDistance = (maxDist - opaqueRemaining) + hitT;
        }
        GpuInstance hitInst = instances[hitIdx];
        GpuMaterial hitMat = materials[hitInst.materialId];
        // Effects have their own non-opaque visibility layer and therefore
        // cannot arrive through `opaqueMask`; keep this defensive skip for
        // stale TLAS content during a hot reload.
        bool effectCard = hitMat.materialKind == MATERIAL_KIND_EFFECT_SHADER
            || hitMat.materialKind == MATERIAL_KIND_FIRE_REFRACTION;
        if (effectCard) {
            if (!advanceShadowRayPastHit(
                    opaqueOrigin, opaqueRemaining, direction, hitT)) break;
            continue;
        }

        bool alphaSensitive = hitMat.alphaThreshold > 0.0
            || (hitInst.flags & INSTANCE_FLAG_ALPHA_BLEND) != 0u;
        bool nearEmitter = emitterRadius > 0.0
            && opaqueRemaining - hitT <= emitterRadius
            && max(max(hitMat.emissiveR, hitMat.emissiveG), hitMat.emissiveB)
                * max(hitMat.emissiveMult, 1.0) > 0.01;
        if (!alphaSensitive && !nearEmitter) return vec3(0.0);

        vec2 hitUV = resolveRayHitUV(
            uint(hitIdx), uint(hitPrim), hitBary, direction, hitMat);
        vec4 hitBase;
        bool covered = rayHitHasCoverage(
            uint(hitIdx), uint(hitPrim), hitBary,
            hitInst, hitMat, hitUV, hitBase);
        vec3 hitEmission = rayHitEmission(hitMat, hitUV, hitBase.rgb, 0.0);
        bool sourceShell = nearEmitter
            && max(max(hitEmission.r, hitEmission.g), hitEmission.b) > 0.01;
        if (covered && !sourceShell) return vec3(0.0);

        if (!advanceShadowRayPastHit(
                opaqueOrigin, opaqueRemaining, direction, hitT)) break;
    }

    // #5848 — shader-local by design (no CPU-side value clamps against
    // it): the glass-interface allowance of one shadow-transport march.
    // The GLASS_RAY_BUDGET table entry scales how often the march runs,
    // not how deep one march goes; telemetry that speaks of an interface
    // allowance "growing per tier" means the march frequency, not this
    // fixed depth.
    const int MAX_GLASS_INTERFACES = 4;
    vec3 transmission = vec3(1.0);
    if ((visibilityMask & VISIBILITY_LAYER_GLASS) == 0u) return transmission;
    vec3 rayOrigin = origin;
    float remaining = maxDist;

    for (int layer = 0; layer < MAX_GLASS_INTERFACES; ++layer) {
        rayQueryEXT glassRQ;
        rayQueryInitializeEXT(
            glassRQ, topLevelAS, gl_RayFlagsOpaqueEXT,
            VISIBILITY_LAYER_GLASS,
            rayOrigin, 0.0, direction, remaining);
        while (rayQueryProceedEXT(glassRQ)) {}
        if (rayQueryGetIntersectionTypeEXT(glassRQ, true)
            == gl_RayQueryCommittedIntersectionNoneEXT) {
            break;
        }

        int hitIdx = rayQueryGetIntersectionInstanceCustomIndexEXT(glassRQ, true);
        int hitPrim = rayQueryGetIntersectionPrimitiveIndexEXT(glassRQ, true);
        vec2 hitBary = rayQueryGetIntersectionBarycentricsEXT(glassRQ, true);
        float hitT = rayQueryGetIntersectionTEXT(glassRQ, true);
        if (committedInstance == 0xFFFFFFFFu) {
            committedInstance = uint(hitIdx);
            committedDistance = (maxDist - remaining) + hitT;
        }
        GpuInstance hitInst = instances[hitIdx];
        GpuMaterial hitMat = materials[hitInst.materialId];
        vec2 hitUV = resolveRayHitUV(
            uint(hitIdx), uint(hitPrim), hitBary, direction, hitMat);
        vec4 glassTex;
        bool covered = rayHitHasCoverage(
            uint(hitIdx), uint(hitPrim), hitBary,
            hitInst, hitMat, hitUV, glassTex);
        if (!covered) {
            if (!advanceShadowRayPastHit(
                    rayOrigin, remaining, direction, hitT)) break;
            continue;
        }
        vec3 tint = clamp(
            glassTex.rgb * vec3(hitMat.diffuseR, hitMat.diffuseG, hitMat.diffuseB),
            vec3(0.02), vec3(1.0));
        float authoredOpacity = clamp(glassTex.a * hitMat.materialAlpha, 0.0, 1.0);
        vec3 hitN = getHitTriNormal(uint(hitIdx), uint(hitPrim));
        if (dot(hitN, direction) > 0.0) hitN = -hitN;
        float cosTheta = clamp(dot(-direction, hitN), 0.0, 1.0);
        float ior = max(hitMat.ior, 1.0);
        float f0 = (ior - 1.0) / (ior + 1.0);
        f0 *= f0;
        float fresnel = shadowFresnelSchlickScalar(cosTheta, f0);

        float absorption = mix(0.08, 0.45, authoredOpacity);
        transmission *= mix(vec3(1.0), tint, absorption) * (1.0 - fresnel);
        if (max(max(transmission.r, transmission.g), transmission.b) < 0.01) {
            return vec3(0.0);
        }

        if (!advanceShadowRayPastHit(
                rayOrigin, remaining, direction, hitT)) break;
    }
    return transmission;
}

vec3 traceShadowTransmittance(
    vec3 origin, vec3 direction, float maxDist, float emitterRadius,
    uint visibilityMask
) {
    uint committedInstance;
    float committedDistance;
    return traceShadowTransmittanceDetailed(
        origin,
        direction,
        maxDist,
        emitterRadius,
        visibilityMask,
        committedInstance,
        committedDistance
    );
}

// #5565 — the glass leg of whatever is left of a segment after
// `traceShadowTransmittanceSkippingInstance`'s self-skip walk. Defined ABOVE
// that function (and its docblock) so the block stays attached to the walk it
// describes (#5702). Returns lit when the light carries no glass layer or
// nothing remains.
vec3 traceShadowGlassLegRemaining(
    vec3 at, vec3 direction, float remaining, float emitterRadius,
    uint visibilityMask
) {
    uint glassOnly = visibilityMask & VISIBILITY_LAYER_GLASS;
    if (glassOnly == 0u || remaining <= 0.0) return vec3(1.0);
    // With ONLY the glass bit the shared transport's opaque loop sees an
    // empty mask and exits at once, and its glass loop runs — the same
    // interface-by-interface tint/absorption/Fresnel the reflection half and
    // water.frag already apply.
    return traceShadowTransmittance(
        at, direction, remaining, emitterRadius, glassOnly);
}

// #5249 — visibility of a light segment that ignores the receiver's OWN
// instance: the transmission lobes' shadow ray. #5018's light-side origin
// starts that ray just inside a closed body, so the body's own surfaces
// must not block the very lobe modelling light transmitting THROUGH them
// (#5192's self-occlusion zero) — while every other blocker (a building
// between the sun and back-lit foliage, the wall behind a shadowed
// interior NPC — the #4946 wall-bleed #5192 re-opened) still must. Own
// hits are skipped by advancing past them, the same transport the
// alpha-skip walk above uses; the first FOREIGN hit defers to the shared
// alpha/glass-aware trace for the remaining leg, so translucent blockers
// keep their tint semantics. Exhausting MAX_TRANSMISSION_SELF_SKIPS
// reports lit — the #5192 conservative default — rather than zeroing the
// lobe on an unbounded self-overlap. The self-skip WALK is opaque layers
// only (the glass walk is about what the ray passes THROUGH, which the
// own body is not), but the foreign-hit handoff gets the FULL light mask
// (#5455): `traceShadowTransmittanceDetailed` runs its glass loop only
// when the mask carries VISIBILITY_LAYER_GLASS, so handing it the
// opaque-only mask made a window pane between a back-lit surface and its
// light silently pass the transmission lobe untinted.
//
// #5565 — that handoff only runs when the walk finds a foreign OPAQUE hit,
// and the walk cannot see a pane (glass is outside VISIBILITY_MASK_ALL_OPAQUE).
// A segment holding the receiver's own body and a window pane but no other
// opaque blocker therefore ended the walk on "no more hits" and reported the
// segment lit, which is #5455's own repro. Every lit-returning exit now
// finishes with the glass leg of the remaining segment instead of a bare
// `vec3(1.0)`.
vec3 traceShadowTransmittanceSkippingInstance(
    vec3 origin, vec3 direction, float maxDist,
    float emitterRadius, uint visibilityMask, int skipInstance
) {
    uint mask = visibilityMask & VISIBILITY_MASK_ALL_OPAQUE; // skip walk only
    vec3 at = origin;
    float remaining = maxDist;
    for (int hop = 0; hop < int(MAX_TRANSMISSION_SELF_SKIPS); ++hop) {
        rayQueryEXT skipRQ;
        rayQueryInitializeEXT(
            skipRQ, topLevelAS, gl_RayFlagsOpaqueEXT,
            mask,
            at, 0.0, direction, remaining);
        while (rayQueryProceedEXT(skipRQ)) {}
        if (rayQueryGetIntersectionTypeEXT(skipRQ, true)
            == gl_RayQueryCommittedIntersectionNoneEXT) {
            return traceShadowGlassLegRemaining(
                at, direction, remaining, emitterRadius, visibilityMask);
        }
        int hitIdx = rayQueryGetIntersectionInstanceCustomIndexEXT(skipRQ, true);
        if (hitIdx != skipInstance) {
            // A foreign blocker: hand the remaining leg to the shared
            // transport so alpha-aware and glass blockers keep their
            // visibility semantics along this ray too.
            return traceShadowTransmittance(
                at, direction, remaining, emitterRadius, visibilityMask);
        }
        float hitT = rayQueryGetIntersectionTEXT(skipRQ, true);
        if (!advanceShadowRayPastHit(at, remaining, direction, hitT)) {
            return traceShadowGlassLegRemaining(
                at, direction, remaining, emitterRadius, visibilityMask);
        }
    }
    return traceShadowGlassLegRemaining(
        at, direction, remaining, emitterRadius, visibilityMask);
}

#endif
