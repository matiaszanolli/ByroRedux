#version 450
#extension GL_GOOGLE_include_directive : require

// EXAL ground cover — blade geometry and wind (§4, §8; #4055).
//
// There is no blade mesh and no per-blade vertex data. A blade is a quadratic
// Bezier ribbon — base point, a control point displaced by the bend, a tip —
// generated here from the 24-bit seed the scatter stored. `gl_VertexIndex`
// selects the segment and the side; height, width, yaw, twist and colour
// jitter all derive from the seed. Segment count comes from a push constant,
// so the same shader emits a 3-segment near blade and a 1-segment far blade
// branching on nothing but a per-chunk value (§6).
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
    /// x = gust frequency (bitcast float), yzw = blades-per-chunk, segments,
    /// species count.
    vec4 gustAndCounts;
} pc;

#define GC_CAMERA_POS       pc.cameraPixels.xyz
#define GC_PIXELS_PER_UNIT  pc.cameraPixels.w
#define GC_RENDER_ORIGIN    pc.originTime.xyz
#define GC_TIME             pc.originTime.w
#define GC_GUST_FREQUENCY   pc.gustAndCounts.x
#define GC_BLADES_PER_CHUNK uint(pc.gustAndCounts.y)
#define GC_SEGMENTS         uint(pc.gustAndCounts.z)
#define GC_SPECIES_COUNT    uint(pc.gustAndCounts.w)

#include "include/terrain_sample.glsl"
#include "include/groundcover_density.glsl"

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

void main() {
    uint vertsPerBlade = GC_DEBUG_POINTS == 1u
        ? 1u
        : GC_SEGMENTS * GROUNDCOVER_VERTS_PER_SEGMENT;
    uint bladeIndex = uint(gl_VertexIndex) / vertsPerBlade;
    uint vertInBlade = uint(gl_VertexIndex) % vertsPerBlade;

    GroundCoverBlade blade = gcBlades[bladeIndex];
    uint chunkIndex = bladeIndex / GC_BLADES_PER_CHUNK;
    GroundCoverChunk chunk = gcChunks[chunkIndex];
    GroundCoverCell cell = gcCells[chunk.cellIndex];

    vec3 base = byroGcBladePosition(blade, chunk);
    uint seed = byroGcBladeSeed(blade);
    uint species = byroGcBladeSpecies(blade);
    vSpecies = min(species, max(GC_SPECIES_COUNT, 1u) - 1u);
    vDGround = blade.dGround;
    vColourJitter = gcSeedStream(seed, 5u);

    GroundCoverSpecies sp = gcSpecies[vSpecies];
    float height = mix(sp.sizeRange.x, sp.sizeRange.y, gcSeedStream(seed, 0u));
    float width = mix(sp.sizeRange.z, sp.sizeRange.w, gcSeedStream(seed, 1u));
    float stiffness = clamp(sp.baseColour.a, 0.0, 1.0);

    // Ground normal, re-sampled (see the header). A blade grows out of the
    // ground it sits on, not straight up: on a 20° slope the difference is the
    // whole reason grass looks planted rather than stuck on.
    TerrainSample ground = byroSampleTerrain(cell.vertexOffset, cell.originXZ, base.xz);
    vec3 up = ground.valid ? normalize(mix(vec3(0.0, 1.0, 0.0), ground.normal, 0.75))
                           : vec3(0.0, 1.0, 0.0);

    // Per-blade yaw. The ribbon's width axis is perpendicular to both `up` and
    // the blade's facing, so a blade is a flat ribbon standing on the ground
    // rather than a billboard.
    float yaw = gcSeedStream(seed, 2u) * 6.2831853;
    vec3 facing = normalize(vec3(cos(yaw), 0.0, sin(yaw)));
    vec3 side = normalize(cross(up, facing));

    // ── Wind (§8) ───────────────────────────────────────────────────────
    //
    // The field is sampled in world space and advected downwind, so the
    // pattern is shared by every blade near the sample point — that shared
    // sampling is what produces a gust *front* instead of noise.
    vec2 windDir = pc.wind.xy;
    float windSpeed = pc.wind.z;
    float gustAmp = pc.wind.w;
    float gustFreq = GC_GUST_FREQUENCY;
    float timeS = GC_TIME;
    vec2 advected = base.xz - windDir * (windSpeed * GROUNDCOVER_WIND_ADVECTION_SCALE * timeS);
    float flow = byroGcFbm(advected / GROUNDCOVER_WIND_NOISE_UNITS);
    // Per-blade phase keeps the response out of lockstep without breaking the
    // shared front: it perturbs *when* a blade answers the gust, not where the
    // gust is.
    float phase = gcSeedStream(seed, 3u) * 6.2831853;
    float gust = 1.0 + gustAmp * sin(timeS * gustFreq * 6.2831853 + phase);
    float bendFraction = clamp(windSpeed / GROUNDCOVER_MAX_WIND_SPEED, 0.0, 1.0)
                       * mix(0.35, 1.0, clamp(flow, 0.0, 1.0))
                       * gust
                       * (1.0 - 0.7 * stiffness);
    bendFraction = clamp(bendFraction, 0.0, 1.0) * GROUNDCOVER_WIND_MAX_BEND;

    // Even in dead calm a blade is not a rigid spike; a small seeded lean
    // gives the sward its silhouette. Without it, zero wind renders a bed of
    // nails.
    vec3 leanDir = length(windDir) > 1.0e-4
        ? normalize(vec3(windDir.x, 0.0, -windDir.y))
        : facing;
    float restLean = 0.12 + 0.10 * gcSeedStream(seed, 4u);
    vec3 bend = leanDir * (height * (bendFraction + restLean * (1.0 - bendFraction)));

    // Quadratic Bezier: P0 base, P1 control (half height, displaced by bend),
    // P2 tip. The control at half height is what makes the blade curve rather
    // than hinge.
    vec3 p0 = base;
    vec3 p1 = base + up * (height * 0.5) + bend * 0.5;
    vec3 p2 = base + up * height + bend;

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
        gl_PointSize = clamp(64.0 / max(distance(base, GC_CAMERA_POS) * 0.02, 1.0), 1.0, 6.0);
        return;
    }

    uint segment = vertInBlade / GROUNDCOVER_VERTS_PER_SEGMENT;
    uint corner = vertInBlade % GROUNDCOVER_VERTS_PER_SEGMENT;
    // Two triangles per segment from a quad's four corners, as a list:
    // (0,1,2) and (2,1,3). Index 0/2 are the low/high ring's left edge.
    const uint QUAD[6] = uint[6](0u, 1u, 2u, 2u, 1u, 3u);
    uint quadCorner = QUAD[corner];
    float t = (float(segment) + float(quadCorner >> 1)) / float(GC_SEGMENTS);
    float sideSign = (quadCorner & 1u) == 0u ? -1.0 : 1.0;

    float omt = 1.0 - t;
    vec3 pos = omt * omt * p0 + 2.0 * omt * t * p1 + t * t * p2;
    // Bezier derivative — the blade's own tangent, which the width axis and
    // the shading normal both have to follow around the curve.
    vec3 tangent = normalize(2.0 * omt * (p1 - p0) + 2.0 * t * (p2 - p1));

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
    float minHalfWidth = 0.5 / max(pixelsPerUnit, 1.0e-6);
    halfWidth = max(halfWidth, min(minHalfWidth, width * 4.0));

    // Twist: the ribbon rotates about its own tangent along its length, so a
    // blade catches light on one face near the base and the other near the
    // tip. Without it every blade is a flat card and the field reads as cards.
    float twist = (gcSeedStream(seed, 6u) - 0.5) * 1.4 * t;
    vec3 widthAxis = normalize(cross(tangent, facing) * cos(twist) + facing * sin(twist));

    vWorldPos = pos + widthAxis * (halfWidth * sideSign);
    // Shading normal faces out of the ribbon's flat side.
    vWorldNormal = normalize(cross(widthAxis, tangent));
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
    gl_Position = pc.viewProj * vec4(vWorldPos - GC_RENDER_ORIGIN, 1.0);
}
