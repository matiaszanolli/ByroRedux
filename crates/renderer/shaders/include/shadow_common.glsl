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

// PAF-001 (docs/engine/penumbra-aware-shadow-filter.md §3.1): world-space
// penumbra width behind a blocker, from Fernando's parallel-planes estimate
// (PCSS, NVIDIA 2005): w = (dReceiver - dBlocker) * wLight / dBlocker.
//   blockerT      mean hit distance of this frame's occluded samples, receiver
//                 to blocker; negative when no sample was occluded (-> 0).
//   local         point / spot light: wLight = 2 r at receiver-to-light
//                 distance `lightDistance`, so dBlocker = lightDistance - t.
//   !local        directional light: w = t * 2 tan(theta); `sunAngularRadius`
//                 is theta, the cone half-angle.
// `shadow_filter.comp` reads the result from the high half of
// `directUnshadowed[].y` (an f16, world units).
float penumbraWidthFromBlocker(
    float blockerT,
    bool local,
    float lightDistance,
    float sourceRadius,
    float sunAngularRadius
) {
    if (!(blockerT > 0.0)) return 0.0;
    if (local) {
        return blockerT * 2.0 * sourceRadius / max(lightDistance - blockerT, 1e-3);
    }
    return blockerT * 2.0 * tan(sunAngularRadius);
}

#endif
