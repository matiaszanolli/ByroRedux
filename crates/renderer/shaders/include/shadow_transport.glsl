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
    const int MAX_OPAQUE_LAYERS = 8;
    vec3 opaqueOrigin = origin;
    float opaqueRemaining = maxDist;
    uint opaqueMask = visibilityMask & VISIBILITY_MASK_ALL_OPAQUE;
    for (int layer = 0; layer < MAX_OPAQUE_LAYERS; ++layer) {
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

#endif
