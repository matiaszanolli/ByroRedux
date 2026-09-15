#version 450
#extension GL_GOOGLE_include_directive : require

// EXAL ground cover — blade geometry and wind (§4, §8; #4055).
//
// There is no blade mesh and no per-blade vertex data. A blade is a quadratic
// Bezier ribbon — base point, a control point displaced by the bend, a tip —
// generated here from the 24-bit seed the scatter stored. `gl_VertexIndex`
// selects the segment and the side; height, width, yaw, twist and colour
// jitter all derive from the seed. The shipping tier-0 path uses the generated
// 3-segment constant; the planned tier chain will add the 1-segment and card
// representations without making this per-frame shape state (§6).
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

/// Exactly 128 bytes — Vulkan's guaranteed `maxPushConstantsSize` floor.
/// Packed rather than laid out one-field-per-vec4 for that reason: this
/// device allows 256, and a layout that only fits there is a portability bug
/// that no test on this machine can see.
layout(push_constant) uniform GcBladePush {
    /// **Render-origin-RELATIVE**, exactly as `triangle.vert` uses it. The
    /// renderer subtracts a cell-grid-snapped origin from world positions
    /// before projecting (#1496 / #markarth-precision), so projecting an
    /// absolute position with this matrix puts the geometry kilometres off
    /// screen — silently, since nothing renders rather than rendering wrong.
    mat4 viewProj;
    /// xyz = absolute camera position, w = pixels per world unit at one unit
    /// of depth (§6's projected-pixel widening key).
    vec4 cameraPixels;
    /// xyz = the render origin to subtract, w = seconds.
    vec4 originTime;
    /// xy = unit wind direction, z = speed, w = gust amplitude.
    vec4 wind;
/// x = gust frequency, y = previous shared wind-clock sample, z = species
/// count. `w` packs `(frame serial << 1) | lod tier` for the blue-noise LOD
/// cross-fade; the fixed blade arena and segment counts are generated shader
/// constants, not per-frame tuning state.
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
#define GC_LOD_TIER         (GC_LOD_WORD & 1u)
#define GC_FRAME_SERIAL     (GC_LOD_WORD >> 1u)

#include "include/terrain_sample.glsl"
#include "include/groundcover_density.glsl"
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
/// Cross-fade payload. The fragment uses a screen-space blue-noise rank; the
/// seed rotates its tile per blade and the packed clock rotates it per frame.
layout(location = 10) flat out uint vBladeSeed;
layout(location = 11) flat out uint vLodTier;
layout(location = 12) flat out uint vFrameSerial;
layout(location = 13) out float vLodMidWeight;
layout(location = 14) out vec2 vCardUv;
layout(location = 15) flat out uint vCard;
layout(location = 16) out float vCardWeight;

// The scene set already binds CameraUBO at set 1 / binding 1.  Only the two
// leading matrices are declared here; the descriptor's full range is larger,
// which Vulkan permits, and this avoids another clock or camera-history copy.
layout(set = 1, binding = 1) uniform GcCameraMotionUBO {
    mat4 gcViewProj;
    mat4 gcPrevViewProj;
};

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
    float windSpeed = pc.wind.z;
    vec2 advected = base.xz - windDir
        * (windSpeed * GROUNDCOVER_WIND_ADVECTION_SCALE * timeS);
    float flow = byroGcFbm(advected / GROUNDCOVER_WIND_NOISE_UNITS);
    float phase = gcSeedStream(seed, 3u) * GROUNDCOVER_TWO_PI;
    float gustClock = timeS * GC_GUST_FREQUENCY * GROUNDCOVER_TWO_PI;
    float primaryWave = sin(gustClock + phase);
    float secondaryWave = sin(
        gustClock * GROUNDCOVER_WIND_HARMONIC_FREQUENCY_MULTIPLIER
        + phase * GROUNDCOVER_WIND_HARMONIC_FREQUENCY_MULTIPLIER);
    float gust = 1.0 + pc.wind.w * (
        primaryWave + GROUNDCOVER_WIND_SECONDARY_AMPLITUDE * secondaryWave);
    float bendFraction = clamp(windSpeed / GROUNDCOVER_MAX_WIND_SPEED, 0.0, 1.0)
        * mix(GROUNDCOVER_WIND_FLOW_FLOOR, 1.0, clamp(flow, 0.0, 1.0))
        * gust * (1.0 - GROUNDCOVER_WIND_STIFFNESS_ATTENUATION * stiffness);
    bendFraction = clamp(bendFraction, 0.0, 1.0) * GROUNDCOVER_WIND_MAX_BEND;
    vec3 leanDir = length(windDir) > 1.0e-4
        ? normalize(vec3(windDir.x, 0.0, -windDir.y))
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
    float heightSquaredScale = height * height / max(maxSpeciesHeight, 1.0e-4);
    return (leanDir * downwindBend + lateralDir * lateralBend) * heightSquaredScale;
}

void byroGcControlPoints(
    vec3 base, vec3 up, float height, vec3 windBend, vec2 disturbance,
    out vec3 p1, out vec3 p2
) {
    float disturbAmount = min(length(disturbance), 1.0);
    vec3 bend = windBend;
    if (disturbAmount > 1.0e-4) {
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
    float bendFraction = min(length(bend) / max(height, 1.0e-4), 1.0);
    float uprightScale = sqrt(max(1.0 - bendFraction * bendFraction, 0.0));
    p1 = base + up * (height * 0.5 * uprightScale) + bend * 0.5;
    p2 = base + up * (height * uprightScale) + bend;
}

void main() {
    bool cardTier = GC_LOD_TIER == 2u;
    uint cardCluster = GROUNDCOVER_BLADES_PER_POINT * GROUNDCOVER_BLADES_PER_POINT;
    uint segments = GC_LOD_TIER == 0u
        ? GROUNDCOVER_BLADE_SEGMENTS_NEAR
        : GROUNDCOVER_BLADE_SEGMENTS_MID;
    uint vertsPerBlade = GC_DEBUG_POINTS == 1u
        ? 1u
        : segments * GROUNDCOVER_VERTS_PER_SEGMENT;
    // Tier 0 grows the cited four-blade tuft. Tier 1 keeps only its stable
    // representative and widens it below, reducing the geometric population
    // without changing the root sequence or residency slab. Debug remains one
    // point per accepted root because it judges placement, not LOD density.
    uint bladesPerPoint = (GC_DEBUG_POINTS == 1u || GC_LOD_TIER == 1u || cardTier)
        ? 1u
        : GROUNDCOVER_BLADES_PER_POINT;
    uint vertsPerPoint = vertsPerBlade * bladesPerPoint;
    uint pointIndex = cardTier
        ? (uint(gl_VertexIndex) / GROUNDCOVER_VERTS_PER_SEGMENT) * cardCluster
        : uint(gl_VertexIndex) / vertsPerPoint;
    uint vertInPoint = cardTier
        ? uint(gl_VertexIndex) % GROUNDCOVER_VERTS_PER_SEGMENT
        : uint(gl_VertexIndex) % vertsPerPoint;
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
    // colour jitter all decorrelate between blades sharing a root.
    uint seed = byroGcBladeSeed(blade);
    if (subBlade > 0u) {
        seed = uint(gcSeedStream(seed, 6u + subBlade) * 16777216.0);
    }
    uint species = byroGcBladeSpecies(blade);
    vSpecies = min(species, max(GC_SPECIES_COUNT, 1u) - 1u);
    vDGround = blade.dGround;
    vColourJitter = gcSeedStream(seed, 5u);
    vBladeSeed = seed;
    vLodTier = GC_LOD_TIER;
    vFrameSerial = GC_FRAME_SERIAL;
    vCard = cardTier ? 1u : 0u;

    GroundCoverSpecies sp = gcSpecies[vSpecies];
    float height = mix(sp.sizeRange.x, sp.sizeRange.y, gcSeedStream(seed, 0u))
        * chunk.entryProgress;
    float width = mix(sp.sizeRange.z, sp.sizeRange.w, gcSeedStream(seed, 1u));
    float stiffness = clamp(sp.baseColour.a, 0.0, 1.0);
    float projectedHeight = height * GC_PIXELS_PER_UNIT
        / max(distance(base, GC_CAMERA_POS), 1.0e-3);
    // 0 = all tier 0; 1 = all tier 1. Both streams remain present during the
    // band and the fragment admits complementary blue-noise samples.
    vLodMidWeight = 1.0 - smoothstep(
        GROUNDCOVER_MIN_VISIBLE_HALF_WIDTH_PIXELS,
        GROUNDCOVER_MIN_VISIBLE_HALF_WIDTH_PIXELS * GROUNDCOVER_MAX_WIDTH_MULTIPLIER,
        projectedHeight);
    // The card band follows the mid ribbon band using only the same existing
    // pixel floor/cap: Tier 1 is complete at the floor, then yields to cards
    // over the next cap-derived interval without a distance threshold.
    vCardWeight = 1.0 - smoothstep(
        GROUNDCOVER_MIN_VISIBLE_HALF_WIDTH_PIXELS / GROUNDCOVER_MAX_WIDTH_MULTIPLIER,
        GROUNDCOVER_MIN_VISIBLE_HALF_WIDTH_PIXELS,
        projectedHeight);

    // Ground normal, re-sampled (see the header). A blade grows out of the
    // ground it sits on, not straight up: on a 20° slope the difference is the
    // whole reason grass looks planted rather than stuck on.
    TerrainSample ground = byroSampleTerrain(cell.vertexOffset, cell.originXZ, base.xz);
    vec3 up = ground.valid ? normalize(mix(
        vec3(0.0, 1.0, 0.0), ground.normal, GROUNDCOVER_TERRAIN_NORMAL_WEIGHT))
                           : vec3(0.0, 1.0, 0.0);

    // Per-blade yaw. The ribbon's width axis is perpendicular to both `up` and
    // the blade's facing, so a blade is a flat ribbon standing on the ground
    // rather than a billboard.
    float yaw = gcSeedStream(seed, 2u) * GROUNDCOVER_TWO_PI;
    vec3 facing = normalize(vec3(cos(yaw), 0.0, sin(yaw)));
    vec3 side = normalize(cross(up, facing));

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
    // own length.
    //
    // Added to the wind bend rather than blended with it. A blade in a gust
    // that someone also walks through is doing both, and a `mix` would make
    // the trodden blade stand *back up* into the wind — the one direction it
    // certainly is not going. The sum is clamped through `bend`'s use below,
    // where the Bezier control point keeps the tip on the near side of the
    // ground.
    vec2 disturbance = byroGcSampleField(base.xz, gcFieldCurrent);
    vec2 previousDisturbance = byroGcSampleField(base.xz, gcFieldPrevious);
    // A trodden blade gets shorter as it lies over, because a blade is not a
    // rubber band. Without this the tip stays at full height and only slides
    // sideways — a lean, not a flattening — and the channel reads as grass
    // combed rather than walked through. `sqrt(1 - k²)` is the vertical leg of
    // a blade of fixed length whose tip has moved `k` of that length
    // horizontally, so the plant keeps its length as it bends.
    // Quadratic Bezier: P0 base, P1 control (half height, displaced by bend),
    // P2 tip. Evaluate both shared-clock samples so TAA/FSR reprojects the
    // blade itself, rather than whichever terrain was behind it last frame.
    vec3 p0 = base;
    vec3 p1;
    vec3 p2;
    vec3 prevP1;
    vec3 prevP2;
    byroGcControlPoints(base, up, height, bend, disturbance, p1, p2);
    byroGcControlPoints(base, up, height, prevBend, previousDisturbance, prevP1, prevP2);

    if (cardTier) {
        const uint QUAD[6] = uint[6](0u, 1u, 2u, 2u, 1u, 3u);
        uint corner = QUAD[vertInBlade];
        float t = float(corner >> 1);
        float sideSign = (corner & 1u) == 0u ? -1.0 : 1.0;
        // The card preserves the summed ribbon coverage of the 4×4 root
        // cluster. Its atlas alpha supplies the individual blade silhouette.
        float areaScale = sqrt(vCardWeight);
        float halfWidth = 0.5 * width * float(cardCluster) * areaScale;
        vec3 tangent = normalize(p2 - p0);
        vec3 prevTangent = normalize(prevP2 - p0);
        vec3 widthAxis = normalize(cross(tangent, facing));
        vec3 prevWidthAxis = normalize(cross(prevTangent, facing));
        vWorldPos = mix(p0, mix(p0, p2, areaScale), t) + widthAxis * (halfWidth * sideSign);
        vWorldNormal = normalize(cross(widthAxis, tangent));
        vBladeT = t;
        vBladeHeight = height;
        vBladeWidth = width;
        vCardUv = vec2(float(corner & 1u), t);
        vCurrClipPos = pc.viewProj * vec4(vWorldPos - GC_RENDER_ORIGIN, 1.0);
        vPrevClipPos = gcPrevViewProj * vec4(
            mix(p0, mix(p0, prevP2, areaScale), t) + prevWidthAxis * (halfWidth * sideSign) - GC_RENDER_ORIGIN,
            1.0);
        gl_Position = vCurrClipPos;
        return;
    }

    if (GC_DEBUG_POINTS == 1u) {
        // The distribution view. One point at the accepted position, sized so
        // near points stay legible without the far field turning into a
        // solid sheet.
        vWorldPos = base;
        vWorldNormal = up;
        vBladeT = 0.0;
        vBladeHeight = height;
        vBladeWidth = width;
        gl_Position = pc.viewProj * vec4(base - GC_RENDER_ORIGIN, 1.0);
        vCurrClipPos = gl_Position;
        vPrevClipPos = gcPrevViewProj * vec4(base - GC_RENDER_ORIGIN, 1.0);
        gl_PointSize = clamp(64.0 / max(distance(base, GC_CAMERA_POS) * 0.02, 1.0), 1.0, 6.0);
        return;
    }

    uint segment = vertInBlade / GROUNDCOVER_VERTS_PER_SEGMENT;
    uint corner = vertInBlade % GROUNDCOVER_VERTS_PER_SEGMENT;
    // Two triangles per segment from a quad's four corners, as a list:
    // (0,1,2) and (2,1,3). Index 0/2 are the low/high ring's left edge.
    const uint QUAD[6] = uint[6](0u, 1u, 2u, 2u, 1u, 3u);
    uint quadCorner = QUAD[corner];
    float t = (float(segment) + float(quadCorner >> 1)) / float(segments);
    float sideSign = (quadCorner & 1u) == 0u ? -1.0 : 1.0;

    float omt = 1.0 - t;
    vec3 pos = omt * omt * p0 + 2.0 * omt * t * p1 + t * t * p2;
    vec3 prevPos = omt * omt * p0 + 2.0 * omt * t * prevP1 + t * t * prevP2;
    // Bezier derivative — the blade's own tangent, which the width axis and
    // the shading normal both have to follow around the curve.
    vec3 tangent = normalize(2.0 * omt * (p1 - p0) + 2.0 * t * (p2 - p1));
    vec3 prevTangent = normalize(2.0 * omt * (prevP1 - p0) + 2.0 * t * (prevP2 - prevP1));

    // Width tapers to zero at the tip, so the last segment degenerates into a
    // triangle with no special case. `1 - t*t` keeps the blade full-width for
    // most of its length and narrows sharply near the tip, which is the shape
    // real grass has.
    float halfWidth = 0.5 * width * (1.0 - t * t);
    // §6's widening is keyed to *projected pixel size*, not distance: a blade
    // narrower than a pixel does not antialias, it flickers, and thin
    // high-contrast geometry is the case TAA handles worst. Keyed to distance
    // instead the floor is only correct at the resolution it was tuned at —
    // the same scene shimmers at 4K and is stable at 1080p.
    float viewDist = max(distance(pos, GC_CAMERA_POS), 1.0e-3);
    float pixelsPerUnit = GC_PIXELS_PER_UNIT / viewDist;
    float minHalfWidth = GROUNDCOVER_MIN_VISIBLE_HALF_WIDTH_PIXELS
        / max(pixelsPerUnit, 1.0e-6);
    halfWidth = max(halfWidth, min(minHalfWidth, width * GROUNDCOVER_MAX_WIDTH_MULTIPLIER));
    // One tier-1 ribbon stands for the four near-tuft ribbons emitted from a
    // scatter root. The same existing width cap is the coverage bound, so this
    // is density compensation rather than a new far-field tuning parameter.
    if (GC_LOD_TIER == 1u) {
        halfWidth = min(
            halfWidth * float(GROUNDCOVER_BLADES_PER_POINT),
            width * GROUNDCOVER_MAX_WIDTH_MULTIPLIER);
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
    vCurrClipPos = pc.viewProj * vec4(vWorldPos - GC_RENDER_ORIGIN, 1.0);
    vPrevClipPos = gcPrevViewProj * vec4(
        prevPos + prevWidthAxis * (halfWidth * sideSign) - GC_RENDER_ORIGIN, 1.0);
    gl_Position = vCurrClipPos;
}
