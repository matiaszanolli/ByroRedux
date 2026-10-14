#ifndef BYRO_GROUNDCOVER_LOD_GLSL
#define BYRO_GROUNDCOVER_LOD_GLSL

// EXAL ground cover — the blade segment LOD, shared by the scatter (which
// publishes which tier a chunk draws) and the blade vertex shader (which
// geomorphs and widens), so the two cannot disagree about a band edge.
//
// Requires, in this order:
//   #include "include/shader_constants.glsl"
//   #include "include/groundcover_scene.glsl"
//   #include "include/terrain_sample.glsl"
//   #include "include/groundcover_density.glsl"
//
// ## The model
//
// The LOD unit is the 512-unit chunk. A chunk is drawn at exactly ONE tier
// chosen from the camera's distance to its centre, so every blade is processed
// by one stream (the previous design drew every blade through every stream and
// discarded the loser per fragment with blue noise):
//
//   tier 0  near   3 segments (7-vertex strip, 15 listed vertices), 4 blades/point
//   tier 1  mid    2 segments (5-vertex strip,  9 listed vertices), 2 blades/point
//   tier 2  far    1 segment  (one triangle,    3 listed vertices), 1 blade/point
//
// ## No pop at a tier edge
//
// Across the `GROUNDCOVER_LOD_MORPH_UNITS` before each edge, every vertex of
// the outgoing tier slides onto the polyline of the next tier's curve
// (`byroGcLodMorph` goes 0 -> 1 and reaches exactly 1 at the edge), and the
// blades the next tier drops fade to zero width while the survivors widen, so
// the chunk is geometrically identical on both sides of the switch. The morph
// is a function of the CHUNK distance, never a blade's own, so all blades of a
// chunk reach the switch on the same frame.
//
// ## Coverage is conserved, not assumed
//
// A tuft's total ribbon width is `GROUNDCOVER_BLADES_PER_POINT` blades wide.
// Dropping blades must not thin the sward, so each survivor is scaled by
// `4 / sum(weights)`: exactly 4 units of blade width per unit of nominal width
// at every distance and mid-morph, rather than a tuned far-field multiplier.

/// Vertices of an `N`-segment blade as a triangle LIST of its strip: the
/// `2N + 1`-vertex strip has `2N - 1` triangles.
#define GROUNDCOVER_LIST_VERTS(N) (3u * (2u * (N) - 1u))

uint byroGcSegmentsForTier(uint tier) {
    return tier == 0u ? GROUNDCOVER_BLADE_SEGMENTS_NEAR
         : (tier == 1u ? GROUNDCOVER_BLADE_SEGMENTS_MID : GROUNDCOVER_BLADE_SEGMENTS_FAR);
}

uint byroGcBladesForTier(uint tier) {
    return tier == 0u ? GROUNDCOVER_BLADES_PER_POINT
         : (tier == 1u ? GROUNDCOVER_BLADES_PER_POINT_MID : GROUNDCOVER_BLADES_PER_POINT_FAR);
}

/// Camera distance to the chunk's centre, with the terrain height at the
/// centre (the camera's own height if the sample falls outside its cell).
float byroGcChunkLodDistance(GroundCoverChunk chunk, GroundCoverCell cell, vec3 cameraPos) {
    vec2 centre = vec2(chunk.baseXZ.x + 0.5 * GROUNDCOVER_CHUNK_UNITS,
                       chunk.baseXZ.y - 0.5 * GROUNDCOVER_CHUNK_UNITS);
    TerrainSample s = byroSampleTerrain(cell.vertexOffset, cell.originXZ, centre);
    return distance(vec3(centre.x, s.valid ? s.height : cameraPos.y, centre.y), cameraPos);
}

uint byroGcLodTier(float chunkDistance) {
    return chunkDistance < GROUNDCOVER_LOD_NEAR_END ? 0u
         : (chunkDistance < GROUNDCOVER_LOD_MID_END ? 1u : 2u);
}

/// 0 -> 1 across the morph band before this tier's far edge; the far tier has
/// nothing left to morph into.
float byroGcLodMorph(uint tier, float chunkDistance) {
    if (tier == 0u) {
        return smoothstep(GROUNDCOVER_LOD_NEAR_END - GROUNDCOVER_LOD_MORPH_UNITS,
                          GROUNDCOVER_LOD_NEAR_END, chunkDistance);
    }
    if (tier == 1u) {
        return smoothstep(GROUNDCOVER_LOD_MID_END - GROUNDCOVER_LOD_MORPH_UNITS,
                          GROUNDCOVER_LOD_MID_END, chunkDistance);
    }
    return 0.0;
}

/// How much of blade `subBlade` of a point survives at `morph`: blades the next
/// tier keeps stay at 1, the ones it drops fade to 0 as the morph completes.
float byroGcBladeWeight(uint tier, uint subBlade, float morph) {
    if (tier >= 2u) {
        return 1.0;
    }
    uint kept = byroGcBladesForTier(tier + 1u);
    return subBlade < kept ? 1.0 : 1.0 - morph;
}

/// `GROUNDCOVER_BLADES_PER_POINT / sum(weights)`: the widening that keeps a
/// point's total blade width constant through every tier and morph.
float byroGcCoverageWiden(uint tier, float morph) {
    uint blades = byroGcBladesForTier(tier);
    float total = 0.0;
    for (uint j = 0u; j < blades; ++j) {
        total += byroGcBladeWeight(tier, j, morph);
    }
    return float(GROUNDCOVER_BLADES_PER_POINT) / max(total, 1.0e-3);
}

#endif // BYRO_GROUNDCOVER_LOD_GLSL
