#ifndef BYRO_GROUNDCOVER_SCENE_GLSL
#define BYRO_GROUNDCOVER_SCENE_GLSL

// EXAL ground cover — the per-frame GPU records shared by the scatter pass and
// everything that draws its output (#4054).
//
// Kept in one header because the scatter *writes* the blade buffer and the
// blade/debug vertex shaders *read* it, and a record whose two ends drifted
// would not fail any test — it would render a field of blades at plausible
// wrong positions.
//
// Requires `#include "include/shader_constants.glsl"` first.

/// One resident exterior terrain cell.
///
/// Extends the #4052 bench's `BenchCell` with the two per-cell inputs the
/// density field needs and the terrain vertices cannot supply: the water plane
/// (§3 `moisture`) and the per-layer `cover_affinity` table (§3 `affinity`),
/// both resolved host-side at terrain spawn from `LTEX` names and `XCLW`.
struct GroundCoverCell {
    /// Y-up world XZ of the cell's (row 0, col 0) terrain vertex.
    vec2 originXZ;
    /// `GpuInstance.vertex_offset` — path A's locator, re-resolved every frame
    /// because `MeshRegistry` compacts (#4052).
    uint vertexOffset;
    /// Padding to keep `affinity0` on its std430 vec4 alignment.
    uint pad0;
    /// `cover_affinity` for LAND splat layers 0-3 and 4-7, in the same layer
    /// order the vertex splat lanes use.
    vec4 affinity0;
    vec4 affinity1;
    /// Y-up water-plane height, or `GROUNDCOVER_NO_WATER`. A cell with no
    /// water must reach `byroGcMoisture` as the sentinel, not as 0.0 — see
    /// that function.
    float waterY;
    float pad1;
    float pad2;
    float pad3;
};

/// One 512-unit ground-cover chunk — §4's unit of dispatch, culling and LOD.
struct GroundCoverChunk {
    /// Y-up world XZ of the chunk's (row 0, col 0) corner. +X and −Z from here
    /// span the chunk, matching the terrain grid's row direction.
    vec2 baseXZ;
    /// Index into `gcCells[]`.
    uint cellIndex;
    /// Scramble seed for this chunk's candidate sequence. Derived from the
    /// chunk's world position and nothing else, because §4 requires placement
    /// stable frame to frame and across sessions — a blade must not move when
    /// the camera does.
    uint seed;
};

/// One accepted blade. §4's ~16-byte record, and deliberately not a
/// `GpuInstance`: a separate, much smaller SSBO that no other pass reads.
///
/// **What is absent is a decision, not an omission.** There is no terrain
/// normal and no terrain albedo here; both are re-sampled in the blade vertex
/// shader instead. #4052 measured that trade — re-sampling costs 0.0018 ns per
/// vertex sample net, because every vertex of a blade reads the same address
/// and the gather is a cache hit after the first, while storing the two terms
/// would roughly double this record, on the one structure in the design whose
/// size scales with the visible population.
struct GroundCoverBlade {
    /// Chunk-relative XZ as 2x16-bit fixed point over [0, GROUNDCOVER_CHUNK_UNITS).
    /// 1/128-unit precision, ~40x finer than the 6-unit blade it positions.
    uint packedXZ;
    /// World-space Y of the blade base. Stored rather than re-derived: it is
    /// the height the scatter actually accepted, and a vertex shader that
    /// re-sampled it could disagree by a bilinear epsilon and float the blade.
    float baseY;
    /// `seed << 8 | speciesIndex`. The seed drives every per-blade property
    /// §4 lists — height, width, bend stiffness, twist, colour jitter — so no
    /// per-blade vertex data is needed anywhere.
    uint seedSpecies;
    /// `d_ground`, NOT `d_draw` (§3). Consumed downstream by §12.1 occlusion,
    /// §12.3 ground coupling and §12.5 canopy shadow. Storing the faded value
    /// here makes the shadow under a meadow lighten as the camera retreats.
    float dGround;
};

/// Unpack a blade's world-space base position.
vec3 byroGcBladePosition(GroundCoverBlade blade, GroundCoverChunk chunk) {
    vec2 local = vec2(float(blade.packedXZ & 0xFFFFu), float(blade.packedXZ >> 16))
               * (GROUNDCOVER_CHUNK_UNITS / 65536.0);
    // −Z on the second axis: chunk-local coordinates run the same direction as
    // the terrain grid's rows.
    return vec3(chunk.baseXZ.x + local.x, blade.baseY, chunk.baseXZ.y - local.y);
}

uint byroGcBladeSeed(GroundCoverBlade blade) {
    return blade.seedSpecies >> 8;
}

uint byroGcBladeSpecies(GroundCoverBlade blade) {
    return blade.seedSpecies & 0xFFu;
}

/// One palette entry. Mirrors `byroredux_core::ecs::components::groundcover::
/// GroundCoverSpecies`'s *rendering* fields; the selection fields (climate
/// weights) are resolved host-side and never reach the GPU.
struct GroundCoverSpecies {
    /// min/max blade height and width, Gamebryo units.
    vec4 sizeRange;
    /// Base colour (rgb) + bend stiffness (a).
    vec4 baseColour;
    /// Tip colour (rgb) + ground-coupling weight (a, §12.3).
    vec4 tipColour;
    /// Transmission colour (rgb, §12.2) + sheen amount (a, §12.6).
    ///
    /// Both landed with their consumer in #4057 rather than ahead of it, per
    /// §12's own rule: their defaults had to be calibrated against a render,
    /// and an invented scalar in a canonical type is how a placeholder becomes
    /// the value nobody revisits.
    vec4 transmissionSheen;
};

#endif // BYRO_GROUNDCOVER_SCENE_GLSL
