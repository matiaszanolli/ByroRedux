#ifndef BYRO_TERRAIN_SAMPLE_GLSL
#define BYRO_TERRAIN_SAMPLE_GLSL

// Exterior LAND terrain attribute sampling at an arbitrary world point.
//
// EXAL ground cover (#4052 / #4054) — "path A" of the design's §11.1 sampling
// question: read the renderer's existing global vertex SSBO directly rather
// than baking a per-cell attribute texture. Nothing in this renderer has ever
// sampled terrain attributes at an arbitrary point before; splat weights reach
// `triangle.frag` as interpolated vertex attributes, not as a lookup. So this
// is the first consumer, and its cost is what §11.1 exists to measure.
//
// Requires, in this order:
//   #include "include/shader_constants.glsl"
//   #include "include/bindings.glsl"        // provides `vertexData[]`
//
// ## Locating a cell's vertices
//
// `exal-groundcover.md` §11.1 originally said the base-vertex offset lives on
// the terrain-tile record. It does not — `GpuTerrainTile` is 24 texture
// indices and nothing else. The offset lives on `GpuInstance.vertex_offset`,
// so the caller passes it in and owns the chunk → covering-terrain-instance
// association.
//
// ## The grid mapping this inverts
//
// `cell_loader/terrain.rs` builds 33×33 vertices row-major, `idx = row*33 + col`:
//
//     bx = origin_x + col * SPACING          -> world X
//     by = origin_y + row * SPACING          -> world -Z  (Z-up -> Y-up flip)
//     height                                 -> world Y
//
// so with `cellOriginXZ` = the Y-up XZ of vertex (row 0, col 0):
//
//     col = (worldXZ.x - cellOriginXZ.x) / SPACING
//     row = (cellOriginXZ.y - worldXZ.y) / SPACING     <- note the sign flip
//
// Getting that sign wrong samples a mirrored row and produces terrain that
// looks plausible but is wrong, so it is pinned by a host-side test that walks
// the same arithmetic (`terrain_sample_grid_mapping_matches_terrain_rs`).

struct TerrainSample {
    /// World-space Y (up).
    float height;
    /// World-space normal, Y-up, renormalised after interpolation.
    vec3 normal;
    /// Splat weights for LAND layers 0-3 and 4-7.
    vec4 splat0;
    vec4 splat1;
    /// False when the requested point fell outside the cell. The caller must
    /// branch: a zeroed sample is not a neutral value, it is a cliff (normal
    /// 0) with no substrate (splat 0), which the density field would read as
    /// a legitimate hole rather than as missing data.
    bool valid;
};

/// First float of vertex (row, col) within the cell starting at `vertexOffset`.
uint byroTerrainVertexBase(uint vertexOffset, uint row, uint col) {
    return (vertexOffset + row * LAND_GRID_VERTS + col) * VERTEX_STRIDE_FLOATS;
}

/// Bilinearly sample height, normal and both splat sets at `worldXZ`.
///
/// `vertexOffset` is `GpuInstance.vertex_offset` for the terrain cell covering
/// the point; `cellOriginXZ` is the Y-up world XZ of that cell's (row 0, col 0)
/// vertex.
TerrainSample byroSampleTerrain(uint vertexOffset, vec2 cellOriginXZ, vec2 worldXZ) {
    TerrainSample s;
    s.height = 0.0;
    s.normal = vec3(0.0, 1.0, 0.0);
    s.splat0 = vec4(0.0);
    s.splat1 = vec4(0.0);
    s.valid = false;

    float fc = (worldXZ.x - cellOriginXZ.x) / LAND_VERTEX_SPACING;
    float fr = (cellOriginXZ.y - worldXZ.y) / LAND_VERTEX_SPACING;

    // Reject before clamping. Clamping alone would silently snap an
    // out-of-cell query onto the boundary row and return a confident wrong
    // answer — the caller has a neighbouring cell for that case.
    float maxIdx = float(LAND_GRID_VERTS - 1u);
    if (!(fc >= 0.0 && fc <= maxIdx && fr >= 0.0 && fr <= maxIdx)) {
        return s;
    }

    uint c0 = uint(floor(fc));
    uint r0 = uint(floor(fr));
    uint c1 = min(c0 + 1u, LAND_GRID_VERTS - 1u);
    uint r1 = min(r0 + 1u, LAND_GRID_VERTS - 1u);
    float tc = fc - float(c0);
    float tr = fr - float(r0);

    uint b00 = byroTerrainVertexBase(vertexOffset, r0, c0);
    uint b01 = byroTerrainVertexBase(vertexOffset, r0, c1);
    uint b10 = byroTerrainVertexBase(vertexOffset, r1, c0);
    uint b11 = byroTerrainVertexBase(vertexOffset, r1, c1);

    // Height is position.y — float lane 1, a safe direct read.
    float h00 = vertexData[b00 + 1u];
    float h01 = vertexData[b01 + 1u];
    float h10 = vertexData[b10 + 1u];
    float h11 = vertexData[b11 + 1u];
    s.height = mix(mix(h00, h01, tc), mix(h10, h11, tc), tr);

    // Normal lanes 7..9 — also safe direct reads.
    vec3 n00 = vec3(vertexData[b00 + VERTEX_NORMAL_OFFSET_FLOATS],
                    vertexData[b00 + VERTEX_NORMAL_OFFSET_FLOATS + 1u],
                    vertexData[b00 + VERTEX_NORMAL_OFFSET_FLOATS + 2u]);
    vec3 n01 = vec3(vertexData[b01 + VERTEX_NORMAL_OFFSET_FLOATS],
                    vertexData[b01 + VERTEX_NORMAL_OFFSET_FLOATS + 1u],
                    vertexData[b01 + VERTEX_NORMAL_OFFSET_FLOATS + 2u]);
    vec3 n10 = vec3(vertexData[b10 + VERTEX_NORMAL_OFFSET_FLOATS],
                    vertexData[b10 + VERTEX_NORMAL_OFFSET_FLOATS + 1u],
                    vertexData[b10 + VERTEX_NORMAL_OFFSET_FLOATS + 2u]);
    vec3 n11 = vec3(vertexData[b11 + VERTEX_NORMAL_OFFSET_FLOATS],
                    vertexData[b11 + VERTEX_NORMAL_OFFSET_FLOATS + 1u],
                    vertexData[b11 + VERTEX_NORMAL_OFFSET_FLOATS + 2u]);
    // Interpolating unit vectors does not preserve length, and the slope gate
    // reads normal.y directly.
    vec3 n = mix(mix(n00, n01, tc), mix(n10, n11, tc), tr);
    float nlen = length(n);
    s.normal = nlen > 1.0e-6 ? n / nlen : vec3(0.0, 1.0, 0.0);

    // Splat lanes 20/21 are 4x u8 unorm, NOT floats. Reading them as floats
    // yields NaN/denormal garbage — see the WARNING block above
    // `GlobalVertices` in bindings.glsl.
    vec4 s0_00 = unpackUnorm4x8(floatBitsToUint(vertexData[b00 + VERTEX_SPLAT0_OFFSET_FLOATS]));
    vec4 s0_01 = unpackUnorm4x8(floatBitsToUint(vertexData[b01 + VERTEX_SPLAT0_OFFSET_FLOATS]));
    vec4 s0_10 = unpackUnorm4x8(floatBitsToUint(vertexData[b10 + VERTEX_SPLAT0_OFFSET_FLOATS]));
    vec4 s0_11 = unpackUnorm4x8(floatBitsToUint(vertexData[b11 + VERTEX_SPLAT0_OFFSET_FLOATS]));
    s.splat0 = mix(mix(s0_00, s0_01, tc), mix(s0_10, s0_11, tc), tr);

    vec4 s1_00 = unpackUnorm4x8(floatBitsToUint(vertexData[b00 + VERTEX_SPLAT1_OFFSET_FLOATS]));
    vec4 s1_01 = unpackUnorm4x8(floatBitsToUint(vertexData[b01 + VERTEX_SPLAT1_OFFSET_FLOATS]));
    vec4 s1_10 = unpackUnorm4x8(floatBitsToUint(vertexData[b10 + VERTEX_SPLAT1_OFFSET_FLOATS]));
    vec4 s1_11 = unpackUnorm4x8(floatBitsToUint(vertexData[b11 + VERTEX_SPLAT1_OFFSET_FLOATS]));
    s.splat1 = mix(mix(s1_00, s1_01, tc), mix(s1_10, s1_11, tc), tr);

    s.valid = true;
    return s;
}

// ---------------------------------------------------------------------------
// Path B — the baked per-cell attribute texture.
// ---------------------------------------------------------------------------
//
// §11.1's fallback. Three array textures, one array layer per resident terrain
// cell, each `LAND_GRID_VERTS` square:
//
//   attrTex   RGBA32F  (height, normal.x, normal.y, normal.z)
//   splat0Tex RGBA8    LAND layers 0-3
//   splat1Tex RGBA8    LAND layers 4-7
//
// Height is 32-bit on purpose. Exterior world Y runs to five figures in
// Bethesda units and an f16 quantises to 4-unit steps up there — a quarter of
// the 128-unit vertex spacing, which would put a visible terrace under the
// grass and make path B lose the comparison for a reason that is an encoding
// choice rather than a property of the path.
//
// The bake resolution deliberately equals the vertex grid, and a LINEAR
// sampler over texel centres reproduces path A's bilinear blend exactly. That
// is what makes the two paths comparable: the same interpolation of the same
// numbers, differing only in how they are fetched. It is also why the
// remaining difference between them is entirely the memory path, which is what
// the bench exists to price.
//
// `layer` is the array slice for the cell covering the point, and
// `cellOriginXZ` is that cell's (row 0, col 0) vertex — both come from the
// same chunk record path A reads its `vertexOffset` from, so neither path gets
// a free ride on the chunk-to-cell association.
TerrainSample byroSampleTerrainBaked(
    sampler2DArray attrTex,
    sampler2DArray splat0Tex,
    sampler2DArray splat1Tex,
    uint layer,
    vec2 cellOriginXZ,
    vec2 worldXZ
) {
    TerrainSample s;
    s.height = 0.0;
    s.normal = vec3(0.0, 1.0, 0.0);
    s.splat0 = vec4(0.0);
    s.splat1 = vec4(0.0);
    s.valid = false;

    // Same inverse mapping as path A, and same rejection rather than clamp —
    // see the block comment on `byroSampleTerrain`. A clamping path B would
    // also read as cheaper here, since the reject branch is the one place the
    // two paths can diverge in control flow.
    float fc = (worldXZ.x - cellOriginXZ.x) / LAND_VERTEX_SPACING;
    float fr = (cellOriginXZ.y - worldXZ.y) / LAND_VERTEX_SPACING;

    float maxIdx = float(LAND_GRID_VERTS - 1u);
    if (!(fc >= 0.0 && fc <= maxIdx && fr >= 0.0 && fr <= maxIdx)) {
        return s;
    }

    // Vertex (row, col) is baked into texel (col, row); its centre is at
    // (idx + 0.5) / LAND_GRID_VERTS in normalised coordinates. Dropping the
    // half-texel would bias every sample by half a vertex spacing (64 units) —
    // wrong, and wrong in a way that still looks like terrain.
    vec2 uv = (vec2(fc, fr) + 0.5) / float(LAND_GRID_VERTS);
    float l = float(layer);

    vec4 attr = texture(attrTex, vec3(uv, l));
    s.height = attr.x;
    vec3 n = attr.yzw;
    float nlen = length(n);
    s.normal = nlen > 1.0e-6 ? n / nlen : vec3(0.0, 1.0, 0.0);

    s.splat0 = texture(splat0Tex, vec3(uv, l));
    s.splat1 = texture(splat1Tex, vec3(uv, l));

    s.valid = true;
    return s;
}

#endif // BYRO_TERRAIN_SAMPLE_GLSL
