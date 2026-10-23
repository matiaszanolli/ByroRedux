#version 450
#extension GL_GOOGLE_include_directive : require

// EXAL ground cover — blade geometry and wind (§4, §8; #4055).
//
// There is no blade mesh and no per-blade vertex data. A blade is a quadratic
// Bezier ribbon — base point, a control point displaced by the bend, a tip —
// generated here from the 24-bit seed the scatter stored. `gl_VertexIndex`
// selects the segment and the side; height, width, yaw, twist and colour
// jitter all derive from the seed. The segment count comes from the chunk's
// LOD tier (3/2/1 segments, `groundcover_lod.glsl`, §12.15), a generated
// constant per tier rather than per-frame shape state.
//
// That is where "efficient" actually comes from: no vertex fetch, no per-blade
// CPU touch, no instance-buffer growth.
//
// ## The terrain normal is re-sampled here, not stored
//
// §4 left this open and #4052 measured it: re-sampling costs 0.0018 ns per
// vertex sample net, because every vertex of a blade reads the same address
// and the gather is a cache hit after the first, while storing the normal
// would roughly double the blade record — the one structure in this design
// whose size scales with the visible population.
//
// ## Wind
//
// §8: sample a 2-D flow-noise field at the blade base, advected along
// `WindField::direction` at `speed`, and bend the Bezier control point.
// Because neighbouring blades sample a *continuous* field at nearby points
// they bend together — travelling gust waves crossing a meadow rather than
// per-blade jitter, which is most of what sells grass as a living surface. A
// per-blade phase offset from the seed keeps the response out of lockstep.
//
// Compile with:
//   glslangValidator -V -Icrates/renderer/shaders groundcover_blade.vert -o groundcover_blade.vert.spv

#include "include/shader_constants.glsl"
#include "include/groundcover_scene.glsl"

/// 0 = the full Bezier ribbon. 1 = one point per blade, for the §9 Phase 1
/// debug view that judges the *distribution* before any blade exists. Sharing
/// one shader keeps the debug points on exactly the positions the blades will
/// occupy — a separate debug shader could drift and would then be validating
/// itself.
layout(constant_id = 0) const uint GC_DEBUG_POINTS = 0;

layout(std430, set = 2, binding = 0) readonly buffer GcChunkBuffer {
    GroundCoverChunk gcChunks[];
};
layout(std430, set = 2, binding = 1) readonly buffer GcCellBuffer {
    GroundCoverCell gcCells[];
};
layout(std430, set = 2, binding = 2) readonly buffer GcGlobalVertices {
    float vertexData[];
};
layout(std430, set = 2, binding = 3) readonly buffer GcBladeBuffer {
    GroundCoverBlade gcBlades[];
};
layout(std430, set = 2, binding = 6) readonly buffer GcSpeciesBuffer {
    GroundCoverSpecies gcSpecies[];
};
/// §12.4's displacement field and its per-frame header (#4058). Read-only
/// here; `groundcover_interaction.comp` owns the writes.
layout(std430, set = 2, binding = 7) readonly buffer GcFieldBuffer {
    uint gcField[];
};
layout(std430, set = 2, binding = 8) readonly buffer GcFieldStateBuffer {
    vec4 gcFieldCurrent;
    vec4 gcFieldPrevious;
};

/// 64 bytes, inside Vulkan's guaranteed 128-byte `maxPushConstantsSize`
/// floor. Packed rather than laid out one-field-per-vec4 for that reason:
/// this device allows 256, and a layout that only fits there is a portability
/// bug that no test on this machine can see.
layout(push_constant) uniform GcBladePush {
    /// xyz = absolute camera position, w = pixels per world unit at one unit
    /// of depth (§6's projected-pixel widening key).
    vec4 cameraPixels;
    /// xyz = the render origin to subtract, w = seconds.
    vec4 originTime;
    /// xy = unit wind direction, z = speed, w = gust amplitude.
    vec4 wind;
/// x = gust frequency, y = previous shared wind-clock sample, z = species
/// count. `w` packs the tier index (the frame-serial half went with
/// af8323881, #5852); the fixed blade arena and
/// segment counts are generated shader constants, not per-frame tuning state.
///
/// The tier field is **two** bits because af8323881's chunk LOD geomorphs
/// between three segment levels (3/2/1 segments per 512-unit chunk). A
/// one-bit field cannot represent the third level (`2 & 1 == 0`), which is
/// exactly the packing bug #4498 pinned when the third stream shipped.
    vec4 gustAndCounts;
} pc;

#define GC_CAMERA_POS       pc.cameraPixels.xyz
#define GC_PIXELS_PER_UNIT  pc.cameraPixels.w
#define GC_RENDER_ORIGIN    pc.originTime.xyz
#define GC_TIME             pc.originTime.w
#define GC_GUST_FREQUENCY   pc.gustAndCounts.x
#define GC_PREV_TIME        pc.gustAndCounts.y
#define GC_SPECIES_COUNT    uint(pc.gustAndCounts.z)
#define GC_LOD_WORD         uint(pc.gustAndCounts.w)
#define GC_LOD_TIER         (GC_LOD_WORD & 3u)

#include "include/terrain_sample.glsl"
#include "include/groundcover_density.glsl"
#include "include/groundcover_wind.glsl"
#include "include/groundcover_lod.glsl"
#include "include/groundcover_interaction.glsl"

/// §12.4 — bilinear read of the displacement field at a world point.
///
/// Bilinear, not nearest, and that is the whole reason the field works: two
/// blades a few units apart read almost the same value and therefore lean
/// almost the same way, so a walker opens a *channel* through the sward
/// rather than tipping a ring of blades individually.
vec2 byroGcSampleField(vec2 worldXZ, vec4 fieldState) {
    vec2 coord = byroGcFieldCoord(fieldState.xy, worldXZ);
    vec2 base = floor(coord);
    vec2 frac = coord - base;
    ivec2 b = ivec2(base);
    vec2 acc = vec2(0.0);
    // Out-of-bounds taps contribute zero rather than clamping to the edge:
    // past the field there is genuinely no disturbance, and a clamp would
    // stretch the boundary texel across everything beyond it.
    for (int dy = 0; dy < 2; ++dy) {
        for (int dx = 0; dx < 2; ++dx) {
            ivec2 t = b + ivec2(dx, dy);
            if (!byroGcFieldInBounds(t)) {
                continue;
            }
            float w = (dx == 0 ? 1.0 - frac.x : frac.x)
                    * (dy == 0 ? 1.0 - frac.y : frac.y);
            acc += unpackHalf2x16(gcField[byroGcFieldIndex(t, uint(fieldState.w))]) * w;
        }
    }
    return acc;
}

layout(location = 0) out vec3 vWorldPos;
layout(location = 1) out vec3 vWorldNormal;
/// Height along the blade, 0 at the base and 1 at the tip. Drives the colour
/// gradient and, later, §12.1's base occlusion.
layout(location = 2) out float vBladeT;
/// `d_ground` — the intrinsic density, NOT `d_draw`. §3 is explicit that
/// everything downstream of the accept test reads this one.
layout(location = 3) out float vDGround;
layout(location = 4) flat out uint vSpecies;
layout(location = 5) out float vColourJitter;
/// The blade's own height in world units (§12.5) and its half-width at this
/// vertex (§12.2). Both are seed-derived here and would otherwise have to be
/// re-derived from the seed in the fragment shader, which would duplicate the
/// taper formula on the one side that cannot see `gl_VertexIndex`.
///
/// `flat` on the height because it is a per-blade constant; interpolating it
/// would make the canopy slab thickness vary across a single blade's own
/// triangles, which is not a thing a slab does.
layout(location = 6) flat out float vBladeHeight;
layout(location = 7) out float vBladeWidth;
// Both positions remain unjittered. The fragment shader performs the
// perspective divide, just as triangle.vert/triangle.frag do for opaque
// meshes, so interpolation stays correct at blade edges.
layout(location = 8) out vec4 vCurrClipPos;
layout(location = 9) out vec4 vPrevClipPos;
// ── §12.3 ground-colour coupling inputs (#4056) ─────────────────────────
// Sampled once at the blade base, where the blade meets the ground it takes
// its colour from. The splat weights ride along with the terrain normal the
// vertex shader already re-samples; the diffuse fetch itself happens in the
// fragment stage, where the bindless `textures[]` array is readable without
// widening its descriptor stages (and their per-stage bindless budget).
/// Terrain diffuse UV rebuilt from world position — the same
/// `col/32 * tiles`, `(1 - row/32) * tiles` mapping
/// `cell_loader/terrain.rs` authored the terrain vertices with.
layout(location = 17) out vec2 vTerrainUv;
/// Splat weights for LAND layers 0-3 / 4-7 at the blade base.
layout(location = 18) out vec4 vTerrainSplat0;
layout(location = 19) out vec4 vTerrainSplat1;
/// Terrain-tile SSBO slot for the layer diffuse indices, or
/// `GROUNDCOVER_NO_TERRAIN_TILE`. `flat`: it is a per-cell constant and a
/// non-uniform index into `terrainTiles[]` must not be interpolated.
layout(location = 20) flat out uint vTerrainTileSlot;
/// The cell's BTXT base diffuse handle, 0 when unresolved — the coupling's
/// substrate on cells with no terrain tile (a base and no ATXT paint, #5174).
layout(location = 21) flat out uint vTerrainBaseIndex;
/// [0,1) tone of the clump this blade belongs to (warm <-> cool), constant over
/// the clump — the colour half of the per-clump variation.
layout(location = 22) out float vClumpTone;

// The scene set already binds CameraUBO at set 1 / binding 1.  Only the
// leading fields through `jitter` are declared here; the descriptor's full
// range is larger, which Vulkan permits, and this avoids another clock or
// camera-history copy.
//
// Blades project with this UBO's `viewProj`, not a push-constant copy (#4296):
// it is the matrix the rest of the main pass projects with — DOF-effective,
// **render-origin-RELATIVE** (#1496), un-jittered — and the one
// `gcPrevViewProj` is the previous frame of, so motion vectors compare like
// with like. Projecting an absolute position with it puts the geometry
// kilometres off screen, silently.
layout(set = 1, binding = 1) uniform GcCameraUBO {
    mat4 gcViewProj;
    mat4 gcPrevViewProj;
    mat4 gcInvViewProj;
    vec4 gcCameraPos;
    vec4 gcSceneFlags;
    vec4 gcScreen;
    vec4 gcFog;
    /// xy = the frame's sub-pixel TAA/FSR jitter in NDC.
    vec4 gcJitter;
};

/// `gl_Position` from an un-jittered clip position. The projection jitter
/// TAA supersamples with and FSR is told to remove has to land on the blades
/// as on every other main-pass surface (`triangle.vert`, `water.vert`), or the
/// largest thin-geometry population in an exterior frame reaches the
/// reconstruction at a fixed sub-pixel offset while the terrain it stands on
/// moves (#4296). `vCurrClipPos` stays un-jittered: motion is scene motion.
vec4 gcJittered(vec4 clip) {
    clip.xy += gcJitter.xy * clip.w;
    return clip;
}

/// Split the 24-bit seed into independent [0,1) streams. Multiplying by
/// distinct large odd constants and taking the high bits decorrelates them;
/// slicing the seed into fixed bit-fields instead would tie blade height to
/// blade yaw, which reads as a field of blades all leaning the same way at
/// each height.
float gcSeedStream(uint seed, uint stream) {
    uint h = (seed * 0x9E3779B9u) ^ (stream * 0x85EBCA6Bu);
    h ^= h >> 15;
    h *= 0x2C1B3C6Du;
    h ^= h >> 12;
    h *= 0x297A2D39u;
    h ^= h >> 15;
    return float(h >> 8) * (1.0 / 16777216.0);
}

vec3 byroGcWindBend(
    vec3 base,
    vec3 facing,
    float height,
    float maxSpeciesHeight,
    float stiffness,
    uint seed,
    float timeS
) {
    vec2 windDir = pc.wind.xy;
    float phase = gcSeedStream(seed, 3u) * GROUNDCOVER_TWO_PI;
    float secondaryWave;
    float bendFraction = byroGcWindBendFraction(
        pc.wind, GC_GUST_FREQUENCY, base.xz, phase, stiffness, timeS, secondaryWave);
    // #4729 — the lean follows WindField's declared "blows toward" sense:
    // blades tip along +windDir in engine XZ, the same direction their §8
    // gust waves roll (the advection above already moves +windDir). The
    // old code mirrored the lean's Z component against every other
    // consumer of the field.
    vec3 leanDir = length(windDir) > GROUNDCOVER_BLADE_VECTOR_EPSILON
        ? normalize(vec3(windDir.x, 0.0, windDir.y))
        : facing;
    vec3 lateralDir = vec3(-leanDir.z, 0.0, leanDir.x);
    float restLean = GROUNDCOVER_REST_LEAN_BASE
        + GROUNDCOVER_REST_LEAN_VARIATION * gcSeedStream(seed, 4u);
    float downwindBend = bendFraction + restLean * (1.0 - bendFraction);
    // The phase is seeded once at the blade base and used for the whole
    // ribbon, so this adds lateral motion without the heat-haze shimmer from
    // per-vertex noise. Its one-third scale is the Step 6 checklist ratio.
    float lateralBend = bendFraction
        * GROUNDCOVER_WIND_LATERAL_FRACTION * secondaryWave;
    // A cantilever's visible deflection should fall off faster than length:
    // scale with height² while normalising by the authored species maximum so
    // the tallest blade retains the established full-strength response.
    float heightSquaredScale = height * height / max(maxSpeciesHeight, GROUNDCOVER_BLADE_VECTOR_EPSILON);
    return (leanDir * downwindBend + lateralDir * lateralBend) * heightSquaredScale;
}

void byroGcControlPoints(
    vec3 base, vec3 up, float height, vec3 windBend, vec2 disturbance,
    float bendCarry, out vec3 p1, out vec3 p2
) {
    float disturbAmount = min(length(disturbance), 1.0);
    vec3 bend = windBend;
    if (disturbAmount > GROUNDCOVER_BLADE_VECTOR_EPSILON) {
        vec3 pushDir = normalize(vec3(disturbance.x, 0.0, disturbance.y));
        float k = GROUNDCOVER_INTERACTION_MAX_BEND * disturbAmount;
        bend += pushDir * (height * k);
    }
    // A horizontal Bezier-tip offset without a matching vertical reduction
    // makes the plant grow whenever either a gust or a disturbance bends it.
    // Keep the root-to-tip chord exactly `height`: this is the right bounded
    // approximation for the ribbon here, and crucially it is evaluated from
    // the *combined* bend so wind and interaction cannot each preserve a
    // different fictional length.
    float bendFraction = min(length(bend) / max(height, GROUNDCOVER_BLADE_VECTOR_EPSILON), 1.0);
    float uprightScale = sqrt(max(1.0 - bendFraction * bendFraction, 0.0));
    // The control point stays over the root (`up` axis) and carries only
    // `bendCarry` of the lean, so the blade rises vertically and bows toward
    // its tip instead of being a straight stick tilted about the root. At
    // `bendCarry = 0.5` this is the old collinear chord. The same carry is
    // used at both clock samples so motion vectors stay consistent.
    p1 = base + up * (height * GROUNDCOVER_BEZIER_CONTROL_FRACTION * uprightScale)
        + bend * bendCarry;
    p2 = base + up * (height * uprightScale) + bend;
}

/// Quadratic Bezier through the three control points.
vec3 byroGcBezier(vec3 p0, vec3 p1, vec3 p2, float t) {
    float omt = 1.0 - t;
    return omt * omt * p0 + 2.0 * omt * t * p1 + t * t * p2;
}

vec3 byroGcBezierTangent(vec3 p0, vec3 p1, vec3 p2, float t) {
    return normalize(2.0 * (1.0 - t) * (p1 - p0) + 2.0 * t * (p2 - p1));
}

/// Half-width of the blade at parameter `t`: full for most of its length,
/// narrowing sharply toward a point — the shape real grass has. The strip's
/// last vertex is the tip, where this is exactly zero.
float byroGcHalfWidthAt(float width, float t) {
    return 0.5 * width * (1.0 - t * t);
}

void main() {
    // The tier is the chunk's segment LOD (`groundcover_lod.glsl`); the scatter
    // published a draw for this stream only if the chunk's distance falls in
    // its band, so every blade arrives here at exactly one level.
    uint tier = GC_LOD_TIER;
    uint segments = byroGcSegmentsForTier(tier);
    uint vertsPerBlade = GC_DEBUG_POINTS == 1u
        ? 1u
        : GROUNDCOVER_LIST_VERTS(segments);
    // Debug remains one point per accepted root because it judges placement,
    // not LOD density.
    uint bladesPerPoint = GC_DEBUG_POINTS == 1u ? 1u : byroGcBladesForTier(tier);
    uint vertsPerPoint = vertsPerBlade * bladesPerPoint;
    uint pointIndex = uint(gl_VertexIndex) / vertsPerPoint;
    uint vertInPoint = uint(gl_VertexIndex) % vertsPerPoint;
    uint subBlade = vertInPoint / vertsPerBlade;
    uint vertInBlade = vertInPoint % vertsPerBlade;

    GroundCoverBlade blade = gcBlades[pointIndex];
    uint chunkIndex = pointIndex / GROUNDCOVER_MAX_BLADES_PER_CHUNK;
    GroundCoverChunk chunk = gcChunks[chunkIndex];
    GroundCoverCell cell = gcCells[chunk.cellIndex];

    vec3 base = byroGcBladePosition(blade, chunk);
    // One 24-bit seed per blade in the tuft. Blade 0 keeps the scatter's own
    // seed; the others draw a fresh one from stream 7 onward, which no
    // per-blade attribute below consumes, so height, yaw, twist, lean and
    // colour jitter all decorrelate between blades sharing a root. The seeds
    // do not depend on the tier, so a blade a far tier keeps is the same blade.
    uint seed = byroGcBladeSeed(blade);
    if (subBlade > 0u) {
        seed = uint(gcSeedStream(seed, 6u + subBlade) * 16777216.0);
    }
    uint species = byroGcBladeSpecies(blade);
    vSpecies = min(species, max(GC_SPECIES_COUNT, 1u) - 1u);
    vDGround = blade.dGround;
    vColourJitter = gcSeedStream(seed, 5u);

    GroundCoverSpecies sp = gcSpecies[vSpecies];
    // Per-clump variation: every blade of a clump shares the clump's height
    // scale and tone, so a clump reads as one patch (taller here, paler there)
    // instead of every blade differing independently — the sprinkled-evenly look.
    uint clumpSeed = uint(byroGcClumpId(base.xz) * 16777216.0);
    float clumpHeight = 1.0 + GROUNDCOVER_CLUMP_HEIGHT_VARIATION
        * (2.0 * gcSeedStream(clumpSeed, 11u) - 1.0);
    vClumpTone = gcSeedStream(clumpSeed, 12u);
    float height = mix(sp.sizeRange.x, sp.sizeRange.y, gcSeedStream(seed, 0u))
        * clumpHeight * chunk.entryProgress;
    float width = mix(sp.sizeRange.z, sp.sizeRange.w, gcSeedStream(seed, 1u));
    float stiffness = clamp(sp.baseColour.a, 0.0, 1.0);

    // ── Segment LOD: morph, survivors, coverage ─────────────────────────
    //
    // `morph` is a function of the CHUNK's distance, so every blade of a chunk
    // reaches the tier switch on the same frame (see the include's header).
    float lodDistance = byroGcChunkLodDistance(chunk, cell, GC_CAMERA_POS);
    float morph = GC_DEBUG_POINTS == 1u ? 0.0 : byroGcLodMorph(tier, lodDistance);
    // Blades the next tier drops fade to nothing across the morph; the rest
    // widen so the tuft's total width is constant (`byroGcCoverageWiden`).
    float bladeWeight = byroGcBladeWeight(tier, subBlade, morph);
    float coverageWiden = byroGcCoverageWiden(tier, morph);
    // Distance thinning (the scatter's accept test) is paid back the same way,
    // by the survivors' inverse fade. Both are bounded: coverage falls off
    // gracefully past the cap rather than the blades turning into wide cones.
    float pointFade = byroGcBladeDistanceFade(distance(base, GC_CAMERA_POS));
    float fadeWiden = min(1.0 / max(pointFade, 1.0e-3), GROUNDCOVER_FADE_COVERAGE_MAX_GAIN);
    float widthScale = min(coverageWiden * fadeWiden, GROUNDCOVER_COVERAGE_MAX_WIDEN) * bladeWeight;

    // Ground normal, re-sampled (see the header). A blade grows out of the
    // ground it sits on, not straight up: on a 20° slope the difference is the
    // whole reason grass looks planted rather than stuck on.
    TerrainSample ground = byroSampleTerrain(cell.vertexOffset, cell.originXZ, base.xz);
    vec3 up = ground.valid ? normalize(mix(
        vec3(0.0, 1.0, 0.0), ground.normal, GROUNDCOVER_TERRAIN_NORMAL_WEIGHT))
                           : vec3(0.0, 1.0, 0.0);

    // ── §12.3 ground-colour coupling: the blade-base terrain sample ─────
    //
    // The species gradient is blended toward the terrain's own albedo at the
    // base, by the per-species coupling weight riding `tipColour.a`. The
    // fetch runs in the fragment stage; this block only captures what it
    // needs from the sample the vertex shader already takes: the splat
    // weights, the rebuilt diffuse UV and the tile slot naming the layer
    // textures. An invalid sample zeroes the splats — no substrate, nothing
    // to couple to — and the sentinel slot makes the fragment skip outright.
    vTerrainSplat0 = ground.splat0;
    vTerrainSplat1 = ground.splat1;
    vTerrainTileSlot = ground.valid
        ? cell.terrainTileSlot
        : GROUNDCOVER_NO_TERRAIN_TILE;
    vTerrainBaseIndex = ground.valid ? cell.baseDiffuseIndex : 0u;
    // Inverse of `cell_loader/terrain.rs`'s UV authoring: `u = col/32 *
    // TILES`, `v = (1 - row/32) * TILES`, with `col = (base.x - ox)/128`
    // and `row = (oz - base.z)/128` — the same grid mapping
    // `byroSampleTerrain` inverts (`terrain_sample.glsl`), where the row
    // axis runs *opposite* world Z. Pinned against the Rust authoring
    // formula by `groundcover_terrain_uv_matches_terrain_rs` (#4056).
    vTerrainUv = vec2(
        (base.x - cell.originXZ.x) * (LAND_TEXTURE_TILES_PER_CELL / EXTERIOR_CELL_UNITS),
        (base.z - cell.originXZ.y) * (LAND_TEXTURE_TILES_PER_CELL / EXTERIOR_CELL_UNITS)
            + LAND_TEXTURE_TILES_PER_CELL);

    // Per-blade yaw. The ribbon's width axis is perpendicular to both `up` and
    // the blade's facing, so a blade is a flat ribbon standing on the ground
    // rather than a billboard.
    float yaw = gcSeedStream(seed, 2u) * GROUNDCOVER_TWO_PI;
    vec3 facing = normalize(vec3(cos(yaw), 0.0, sin(yaw)));

    // ── Wind (§8) ───────────────────────────────────────────────────────
    //
    // The field is sampled in world space and advected downwind, so the
    // pattern is shared by every blade near the sample point — that shared
    // sampling is what produces a gust *front* instead of noise.
    float timeS = GC_TIME;
    vec3 bend = byroGcWindBend(
        base, facing, height, sp.sizeRange.y, stiffness, seed, timeS);
    vec3 prevBend = byroGcWindBend(
        base, facing, height, sp.sizeRange.y, stiffness, seed, GC_PREV_TIME);

    // ── Interaction (§12.4, #4058) ──────────────────────────────────────
    //
    // Sampled at the blade *base*, not at each vertex: the disturbance is a
    // property of the ground the plant is rooted in, and per-vertex sampling
    // would shear a single blade against itself as the field varies along its
    // own length. Added to the wind bend rather than blended with it (a
    // trodden blade is doing both), with the Bezier control point keeping the
    // tip on the near side of the ground.
    vec2 disturbance = byroGcSampleField(base.xz, gcFieldCurrent);
    vec2 previousDisturbance = byroGcSampleField(base.xz, gcFieldPrevious);
    // Quadratic Bezier: P0 base, P1 control, P2 tip. Evaluate both shared-clock
    // samples so TAA/FSR reprojects the blade itself, rather than whichever
    // terrain was behind it last frame.
    vec3 p0 = base;
    vec3 p1;
    vec3 p2;
    vec3 prevP1;
    vec3 prevP2;
    // Stream 10: no other per-blade attribute reads it (0-6 are taken, and the
    // sub-blade seeds above draw 7-9 from the PARENT seed, not this one).
    float bendCarry = mix(
        GROUNDCOVER_BEZIER_BEND_CARRY_MIN,
        GROUNDCOVER_BEZIER_BEND_CARRY_MAX,
        gcSeedStream(seed, 10u));
    byroGcControlPoints(base, up, height, bend, disturbance, bendCarry, p1, p2);
    byroGcControlPoints(
        base, up, height, prevBend, previousDisturbance, bendCarry, prevP1, prevP2);

    if (GC_DEBUG_POINTS == 1u) {
        // The distribution view. One point at the accepted position, sized so
        // near points stay legible without the far field turning into a
        // solid sheet.
        vWorldPos = base;
        vWorldNormal = up;
        vBladeT = 0.0;
        vBladeHeight = height;
        vBladeWidth = width;
        vCurrClipPos = gcViewProj * vec4(base - GC_RENDER_ORIGIN, 1.0);
        gl_Position = gcJittered(vCurrClipPos);
        vPrevClipPos = gcPrevViewProj * vec4(base - GC_RENDER_ORIGIN, 1.0);
        gl_PointSize = clamp(64.0 / max(distance(base, GC_CAMERA_POS) * 0.02, 1.0), 1.0, 6.0);
        return;
    }

    // The blade is one triangle strip of 2N+1 vertices (N segments): two per
    // level for levels 0..N-1 and a single tip vertex at level N, listed as
    // 2N-1 triangles. Triangle i of the list reads strip vertices i..i+2.
    uint triangle = vertInBlade / 3u;
    uint stripVertex = triangle + (vertInBlade % 3u);
    uint level = min(stripVertex >> 1u, segments);
    // The tip has zero width, so its "side" is immaterial.
    float sideSign = (stripVertex & 1u) == 0u ? -1.0 : 1.0;
    float t = float(level) / float(segments);

    vec3 pos = byroGcBezier(p0, p1, p2, t);
    vec3 prevPos = byroGcBezier(p0, prevP1, prevP2, t);
    vec3 tangent = byroGcBezierTangent(p0, p1, p2, t);
    vec3 prevTangent = byroGcBezierTangent(p0, prevP1, prevP2, t);
    float halfWidth = byroGcHalfWidthAt(width, t);

    // Geomorph: slide this vertex onto the polyline of the NEXT tier's curve
    // (N-1 segments through the same Bezier) as `morph` goes 0 -> 1. The end
    // levels are on both polylines already, so only the intermediate vertices
    // move; at morph == 1 the strip is geometrically the next tier's strip and
    // the stream switch changes nothing.
    if (morph > 0.0 && segments > 1u) {
        float lowerSegments = float(segments - 1u);
        float u = t * lowerSegments;
        float seg = min(floor(u), lowerSegments - 1.0);
        float f = u - seg;
        float tA = seg / lowerSegments;
        float tB = (seg + 1.0) / lowerSegments;
        vec3 a = byroGcBezier(p0, p1, p2, tA);
        vec3 b = byroGcBezier(p0, p1, p2, tB);
        vec3 prevA = byroGcBezier(p0, prevP1, prevP2, tA);
        vec3 prevB = byroGcBezier(p0, prevP1, prevP2, tB);
        pos = mix(pos, mix(a, b, f), morph);
        prevPos = mix(prevPos, mix(prevA, prevB, f), morph);
        halfWidth = mix(halfWidth,
            mix(byroGcHalfWidthAt(width, tA), byroGcHalfWidthAt(width, tB), f), morph);
        tangent = normalize(mix(tangent, normalize(b - a), morph));
        prevTangent = normalize(mix(prevTangent, normalize(prevB - prevA), morph));
    }
    // Survivors widen and dropped blades vanish (`widthScale` = 0).
    halfWidth *= widthScale;
    // §6's widening is keyed to *projected pixel size*, not distance: a blade
    // narrower than a pixel does not antialias, it flickers, and thin
    // high-contrast geometry is the case TAA handles worst. Keyed to distance
    // instead the floor is only correct at the resolution it was tuned at —
    // the same scene shimmers at 4K and is stable at 1080p. A blade faded out
    // by the LOD (weight 0) must stay invisible, so the floor is skipped.
    if (widthScale > 0.0 && level < segments) {
        float viewDist = max(distance(pos, GC_CAMERA_POS), GROUNDCOVER_PROJECTED_DEPTH_EPSILON);
        float pixelsPerUnit = GC_PIXELS_PER_UNIT / viewDist;
        float minHalfWidth = GROUNDCOVER_MIN_VISIBLE_HALF_WIDTH_PIXELS
            / max(pixelsPerUnit, 1.0e-6);
        halfWidth = max(halfWidth, min(minHalfWidth, width * GROUNDCOVER_MAX_WIDTH_MULTIPLIER));
    }

    // Twist: the ribbon rotates about its own tangent along its length, so a
    // blade catches light on one face near the base and the other near the
    // tip. Without it every blade is a flat card and the field reads as cards.
    float twist = (gcSeedStream(seed, 6u) - 0.5) * GROUNDCOVER_TWIST_RADIANS * t;
    vec3 widthAxis = normalize(cross(tangent, facing) * cos(twist) + facing * sin(twist));
    vec3 prevWidthAxis = normalize(cross(prevTangent, facing) * cos(twist) + facing * sin(twist));

    vWorldPos = pos + widthAxis * (halfWidth * sideSign);
    // Shading normal faces out of the ribbon's flat side, tilted outward
    // toward this vertex's own edge so the flat ribbon shades as a folded leaf.
    //
    // The fold is Jahrmann & Wimmer 2017's "3D displacement" (§6.3, Eq. 23):
    // the blade's middle axis moves along the normal by
    // `w · (0.5 − |u − 0.5|) · (1 − v)`, giving "approximately a right angle"
    // cross-section that flattens toward the tip. Their demo shader
    // (klejah/ResponsiveGrassDemo, GrassDrawShader.tes lines 54, 113–123)
    // confirms `w` is the FULL blade width, so each half rises
    // `0.5·w·(1 − v)` over a half-span of `0.5·w`: a face slope of exactly
    // `(1 − v)`, i.e. a tilt of `atan(1 − v)` — 45° at the base, none at the
    // tip. (The draft's Eq. 21 writes the edges as `c ± w·t1`, which would
    // halve that slope; the demo and the paper's own "right angle, unfolded
    // width ×√2" both say otherwise.)
    //
    // Adding `widthAxis · sideSign · tan θ` to the unit normal and
    // renormalising rotates it by θ. The ribbon has one vertex per edge, so
    // interpolation across it rounds the fold — Ghost of Tsushima's "tilt the
    // normals of the grass blades outward … a more natural rounded look"
    // (GDC 2021, 13:45), with its angle now from a source that states one.
    // The fragment shader's two-sided flip negates the whole vector, which
    // turns the tilt inward on the concave back face, as it should.
    vec3 flatNormal = normalize(cross(widthAxis, tangent));
    vWorldNormal = normalize(flatNormal + widthAxis * (sideSign * (1.0 - t)));
    vBladeT = t;
    vBladeHeight = height;
    // §12.2's thickness proxy is the blade's *tapered* width, not the species
    // width: a tip that has narrowed to nothing transmits freely, and that
    // along-height variation is what §11.7 asked whether the transmission
    // colour had to carry. It does not — the taper carries it. The pixel-size
    // floor above is deliberately excluded: it widens the ribbon so it can be
    // antialiased, and letting a far blade's antialiasing floor make it
    // optically thicker would darken the distant field for a reason that has
    // nothing to do with the plant.
    vBladeWidth = width * (1.0 - t * t);
    // Absolute out to the fragment shader (lighting and the RT shadow ray
    // both want world space), render-origin-relative into the projection.
    vCurrClipPos = gcViewProj * vec4(vWorldPos - GC_RENDER_ORIGIN, 1.0);
    vPrevClipPos = gcPrevViewProj * vec4(
        prevPos + prevWidthAxis * (halfWidth * sideSign) - GC_RENDER_ORIGIN, 1.0);
    gl_Position = gcJittered(vCurrClipPos);
}
