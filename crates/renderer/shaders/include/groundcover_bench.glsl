#ifndef BYRO_GROUNDCOVER_BENCH_COMMON_GLSL
#define BYRO_GROUNDCOVER_BENCH_COMMON_GLSL

// Shared declarations for the §11.1 terrain-attribute sampling bench (#4052).
//
// One descriptor set, one candidate-point sequence and one sink, used by both
// the compute (scatter consumer) and vertex (raster consumer) halves. Sharing
// them is the point: any difference the bench reports between the two halves
// has to come from the consumer's shader stage and access pattern, not from
// two hand-written copies of the point generator drifting apart.
//
// Requires `#include "include/shader_constants.glsl"` first.

/// One resident exterior terrain cell. `layer` is the array slice of the
/// path-B textures; the index of this record IS that layer.
struct BenchCell {
    /// Y-up world XZ of the cell's (row 0, col 0) vertex.
    vec2 originXZ;
    /// `GpuInstance.vertex_offset` for the terrain mesh covering this cell —
    /// path A's locator. Rebuilt every frame host-side, because the mesh
    /// registry compacts and a cached offset would silently point at another
    /// mesh's vertices (see `MeshRegistry::compact`).
    uint vertexOffset;
    uint pad0;
};

/// One 512-unit ground-cover chunk (§4: 8×8 per exterior cell).
struct BenchChunk {
    /// Y-up world XZ of the chunk's (row 0, col 0) corner — +X and −Z from
    /// here span the chunk, matching the terrain grid's row direction.
    vec2 baseXZ;
    /// Index into `cells[]` — the chunk-to-instance association §11.1 named
    /// as path A's missing link. Both paths pay it, so neither is measured
    /// with a locator the other had to look up.
    uint cellIndex;
    /// Per-chunk scramble seed. §4 requires placement stable frame to frame,
    /// so this is derived from the chunk's grid position, never from time.
    uint seed;
};

layout(std430, set = 0, binding = 0) readonly buffer BenchChunkBuffer {
    BenchChunk chunks[];
};

layout(std430, set = 0, binding = 1) readonly buffer BenchCellBuffer {
    BenchCell cells[];
};

// Path A's source. Same flat `float[]` convention as `bindings.glsl`'s
// `GlobalVertices` — `include/terrain_sample.glsl` reads `vertexData[]` by
// that name, and declaring it here rather than pulling in all of
// `bindings.glsl` keeps the bench off the scene descriptor set.
layout(std430, set = 0, binding = 2) readonly buffer BenchGlobalVertices {
    float vertexData[];
};

// The sink. Every sampled value lands here so no path's loads can be
// dead-code-eliminated; see `benchSink`.
layout(std430, set = 0, binding = 3) writeonly buffer BenchResultBuffer {
    uint results[];
};

layout(set = 0, binding = 4) uniform sampler2DArray terrainAttrTex;
layout(set = 0, binding = 5) uniform sampler2DArray terrainSplat0Tex;
layout(set = 0, binding = 6) uniform sampler2DArray terrainSplat1Tex;

layout(push_constant) uniform BenchPush {
    uint chunkCount;
    /// Compute half: candidate points each thread draws.
    uint samplesPerThread;
    /// Raster half: blades per chunk. Each emits GROUNDCOVER_BENCH_BLADE_VERTS
    /// vertices, and every one of them re-samples — that ratio is the whole
    /// reason §4's store-vs-resample trade is open.
    uint bladesPerChunk;
    /// Uploaded as 0.0, unknown at compile time. Multiplying the accumulated
    /// sample into the output through this is what keeps the sampling loop
    /// alive under optimisation without adding a branch that would itself be
    /// measured. A literal 0.0 would fold; `clamp(x, 0, 0)` would fold too.
    float sinkScale;
    /// Result-buffer index mask (capacity − 1, capacity a power of two).
    uint resultMask;
} pc;

#include "include/terrain_sample.glsl"

/// Specialization constant selecting the sampling path under test.
///
///   0 = path A, the global vertex SSBO.
///   1 = path B, the baked attribute textures.
///   2 = neither — the FLOOR.
///
/// The floor is not a third candidate; it is the control. Both consumers pay
/// costs that have nothing to do with sampling — the scatter pays the
/// candidate-point sequence, the raster pays per-vertex invocation and
/// primitive assembly for every blade it emits — and those costs land inside
/// the same bracket. Without a run that generates the identical points and
/// emits the identical primitives while sampling *nothing*, "path A and path B
/// are indistinguishable here" cannot be told apart from "the sampling is
/// hidden under an overhead that dominates both". §4's store-vs-resample trade
/// turns on that distinction, so the control is measured rather than assumed.
layout(constant_id = 0) const uint BENCH_PATH = 0u;

/// Sample through whichever path this pipeline was specialized for.
TerrainSample benchSample(BenchChunk chunk, vec2 worldXZ) {
    if (BENCH_PATH == 2u) {
        // The control. Returns a valid-shaped sample without touching either
        // source, so everything around the sample still runs and is timed.
        TerrainSample s;
        s.height = 0.0;
        s.normal = vec3(0.0, 1.0, 0.0);
        s.splat0 = vec4(0.0);
        s.splat1 = vec4(0.0);
        s.valid = true;
        return s;
    }
    BenchCell cell = cells[chunk.cellIndex];
    if (BENCH_PATH == 0u) {
        return byroSampleTerrain(cell.vertexOffset, cell.originXZ, worldXZ);
    }
    return byroSampleTerrainBaked(
        terrainAttrTex,
        terrainSplat0Tex,
        terrainSplat1Tex,
        chunk.cellIndex,
        cell.originXZ,
        worldXZ
    );
}

/// Collapse a sample to one scalar the sink can consume. Touches every field
/// both consumers need, so a path cannot win by loading less than the other.
float benchFold(TerrainSample s) {
    return s.height
         + dot(s.normal, vec3(1.0))
         + dot(s.splat0, vec4(1.0))
         + dot(s.splat1, vec4(1.0))
         + (s.valid ? 1.0 : 0.0);
}

/// §4's candidate-point generator: a progressive low-discrepancy sequence
/// (the R2 lattice) scrambled per chunk via Cranley–Patterson rotation.
///
/// Progressive matters for a reason §4 spells out — a truncated prefix has to
/// stay well-distributed — but it matters here for a second one: the points
/// are deliberately *incoherent*, so path A's cost is a scattered gather
/// across the vertex SSBO. A tight synthetic loop over adjacent points would
/// understate exactly the quantity §11.1 is asking about.
vec2 benchCandidate(uint index, uint seed) {
    // R2: the 2-D generalisation of the golden-ratio sequence
    // (Roberts 2018), plastic constant 1.32471795724474602596.
    const float A1 = 0.7548776662466927;
    const float A2 = 0.5698402909980532;
    float u = fract(0.5 + A1 * float(index));
    float v = fract(0.5 + A2 * float(index));
    // Cranley–Patterson rotation by a per-chunk hash keeps every chunk's
    // prefix well-distributed while decorrelating chunks from each other.
    uint h = seed * 0x9E3779B9u;
    h ^= h >> 15;
    h *= 0x85EBCA6Bu;
    h ^= h >> 13;
    float ru = float(h & 0xFFFFu) * (1.0 / 65536.0);
    float rv = float((h >> 16) & 0xFFFFu) * (1.0 / 65536.0);
    return vec2(fract(u + ru), fract(v + rv));
}

/// Candidate `index` of `chunk`, in Y-up world XZ. −Z on the second axis
/// because the terrain grid's row direction is −Z (see `terrain_sample.glsl`).
vec2 benchCandidateWorld(BenchChunk chunk, uint index) {
    vec2 uv = benchCandidate(index, chunk.seed);
    return vec2(
        chunk.baseXZ.x + uv.x * GROUNDCOVER_CHUNK_UNITS,
        chunk.baseXZ.y - uv.y * GROUNDCOVER_CHUNK_UNITS
    );
}

#endif // BYRO_GROUNDCOVER_BENCH_COMMON_GLSL
