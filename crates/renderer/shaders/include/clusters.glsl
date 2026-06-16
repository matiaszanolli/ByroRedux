// Clustered-lighting cluster index lookup
//
// NON-STANDALONE shader fragment. Included by triangle.frag in dependency
// order via GL_GOOGLE_include_directive; it references symbols (structs,
// SSBO/UBO bindings, helper functions, constants) defined in shader_constants.glsl
// and in earlier includes. Do not compile on its own.

// ── Cluster lookup ──────────────────────────────────────────────────

// Compute which cluster this fragment belongs to from screen position + depth.
//
// #628 — `clusterFar` sources from CLMT fog_far (`screen.w`) at runtime
// rather than the pre-fix hardcoded 10000.0. Mirror of
// `cluster_cull.comp::clusterFar()`. The `LOG_RATIO` (was a precomputed
// const for the 0.1→10000 case) is now computed once per fragment;
// adds one `log()` per fragment but keeps the math byte-identical
// with the cluster builder. Both shaders MUST agree on the value or
// fragments will read out of the wrong cluster slice.
uint getClusterIndex(vec2 fragCoord, float viewDepth, vec2 screenSize) {
    uint tileX = uint(fragCoord.x / screenSize.x * float(CLUSTER_TILES_X));
    uint tileY = uint(fragCoord.y / screenSize.y * float(CLUSTER_TILES_Y));
    tileX = min(tileX, CLUSTER_TILES_X - 1);
    tileY = min(tileY, CLUSTER_TILES_Y - 1);

    // Exponential depth slicing (must match cluster_cull.comp).
    float clusterFar = screen.w > 1.0
        ? max(screen.w, CLUSTER_FAR_FLOOR)
        : CLUSTER_FAR_FALLBACK;
    float logRatio = log(clusterFar / CLUSTER_NEAR);
    uint sliceZ = uint(log(max(viewDepth, CLUSTER_NEAR) / CLUSTER_NEAR) / logRatio * float(CLUSTER_SLICES_Z));
    sliceZ = min(sliceZ, CLUSTER_SLICES_Z - 1);

    return tileX + tileY * CLUSTER_TILES_X + sliceZ * CLUSTER_TILES_X * CLUSTER_TILES_Y;
}

