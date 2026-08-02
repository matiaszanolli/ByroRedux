#ifndef BYRO_SHADOW_COMMON_GLSL
#define BYRO_SHADOW_COMMON_GLSL

// Shared shadow-policy decoder and binary TLAS traversal.  All shaders that
// consume GpuLight.params.z use this contract, including passes that cannot
// bind the material tables needed for dielectric RGB transport.
uint decodeShadowPolicy(float encodedPolicy) {
    return uint(clamp(
        floor(encodedPolicy + 0.5),
        float(SHADOW_POLICY_NONE),
        float(SHADOW_POLICY_FULL)
    ));
}

bool shadowPolicyNeedsVisibility(float encodedPolicy) {
    return decodeShadowPolicy(encodedPolicy) != SHADOW_POLICY_NONE;
}

bool shadowPolicyUsesGlass(float encodedPolicy) {
    return decodeShadowPolicy(encodedPolicy) == SHADOW_POLICY_FULL;
}

uint shadowPolicyOpaqueMask(float encodedPolicy) {
    uint policy = decodeShadowPolicy(encodedPolicy);
    if (policy == SHADOW_POLICY_FULL) return SHADOW_MASK_OPAQUE;
    if (policy == SHADOW_POLICY_STRUCTURE) return SHADOW_MASK_STRUCTURE;
    return 0u;
}

bool traceShadowBinary(
    vec3 origin,
    vec3 direction,
    float tMin,
    float tMax,
    uint mask
) {
    if (mask == 0u || tMax <= tMin) return false;
    rayQueryEXT rq;
    rayQueryInitializeEXT(
        rq,
        topLevelAS,
        gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT,
        mask,
        origin,
        tMin,
        direction,
        tMax
    );
    rayQueryProceedEXT(rq);
    return rayQueryGetIntersectionTypeEXT(rq, true)
        != gl_RayQueryCommittedIntersectionNoneEXT;
}

vec3 traceStructureVisibility(vec3 origin, vec3 direction, float maxDist) {
    return traceShadowBinary(
        origin,
        direction,
        0.05,
        maxDist,
        SHADOW_MASK_STRUCTURE
    ) ? vec3(0.0) : vec3(1.0);
}

#endif
