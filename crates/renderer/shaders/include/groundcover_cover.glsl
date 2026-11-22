#ifndef BYRO_GROUNDCOVER_COVER_GLSL
#define BYRO_GROUNDCOVER_COVER_GLSL

// Placed geometry covering a ground-cover root — a road, a flagstone path, a
// building's footing — shared by the blade scatter (`groundcover_scatter.comp`)
// and the model tier (`groundcover_models.comp`), which used to carry their own
// copies of this trace.
//
// The includer declares `gcTopLevelAS`, enables GL_EXT_ray_query and includes
// `include/ray_origin.glsl` and `include/shader_constants.glsl` first.

/// Whether something placed covers the root at `root` (on the rendered terrain
/// triangle, `TerrainSample.meshHeight`): trace straight down from `reach`
/// above it to one representable step above it, against architecture and
/// static props only. The terrain itself lies below the span by construction,
/// so no terrain-identity test and no tuned tolerance are needed.
///
/// #7221 — the TSD-4 terrain displacement patches are in the TLAS and sit
/// 1 mm to a few cm above the base terrain, inside the span. They are the
/// terrain's own relief, not something placed on it, so their instances carry
/// `TLAS_SBT_TAG_TERRAIN_PATCH` in the (otherwise unused) SBT record offset and
/// the trace skips them. Every other candidate is confirmed, which is what the
/// opaque trace this replaces did.
bool gcRootCovered(vec3 root, float reach) {
    vec3 floorPoint = offsetRayOrigin(root, vec3(0.0, 1.0, 0.0));
    vec3 top = root + vec3(0.0, reach, 0.0);
    float span = top.y - floorPoint.y;
    if (span <= 0.0) {
        return false;
    }
    rayQueryEXT q;
    rayQueryInitializeEXT(
        q, gcTopLevelAS,
        gl_RayFlagsNoOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT,
        VISIBILITY_LAYER_ARCHITECTURE | VISIBILITY_LAYER_STATIC_PROP,
        top, 0.0, vec3(0.0, -1.0, 0.0), span);
    while (rayQueryProceedEXT(q)) {
        if (rayQueryGetIntersectionTypeEXT(q, false) == gl_RayQueryCandidateIntersectionTriangleEXT
            && rayQueryGetIntersectionInstanceShaderBindingTableRecordOffsetEXT(q, false)
                != TLAS_SBT_TAG_TERRAIN_PATCH) {
            rayQueryConfirmIntersectionEXT(q);
        }
    }
    return rayQueryGetIntersectionTypeEXT(q, true) != gl_RayQueryCommittedIntersectionNoneEXT;
}

#endif
