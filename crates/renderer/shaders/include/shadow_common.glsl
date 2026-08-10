#ifndef BYRO_SHADOW_COMMON_GLSL
#define BYRO_SHADOW_COMMON_GLSL

// Shared explicit visibility-mask decoder and binary TLAS traversal. The mask
// is encoded in GpuLight.params.z as an exact small integer.
uint decodeVisibilityMask(float encodedMask) {
    return uint(max(floor(encodedMask + 0.5), 0.0)) & VISIBILITY_MASK_FULL;
}

bool visibilityMaskNeedsTrace(float encodedMask) {
    uint mask = decodeVisibilityMask(encodedMask);
    return (mask & (VISIBILITY_MASK_ALL_OPAQUE | VISIBILITY_LAYER_GLASS)) != 0u;
}

bool visibilityMaskUsesGlass(float encodedMask) {
    return (decodeVisibilityMask(encodedMask) & VISIBILITY_LAYER_GLASS) != 0u;
}

uint visibilityOpaqueMask(float encodedMask) {
    return decodeVisibilityMask(encodedMask) & VISIBILITY_MASK_ALL_OPAQUE;
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

#endif
