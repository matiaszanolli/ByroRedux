#version 450
#extension GL_GOOGLE_include_directive : require

// EXAL ground cover §11.1 — the RASTER consumer of the terrain-attribute
// sampling bench (#4052).
//
// §11.1's scope correction: the blade vertex shader samples once per vertex
// per blade, where the scatter samples once per candidate point, so this is
// the larger consumer by a wide margin and the one §4's store-vs-resample
// trade actually turns on. A bench that measured only the scatter would make
// the SSBO path look cheaper than it is.
//
// The blade is not drawn. Every vertex of a blade resolves to the SAME
// clip position, so each triangle has zero area and is discarded before
// rasterisation — the fragment stage does no work and the bracket measures
// the vertex stage. What is faithfully reproduced is the access pattern:
// GROUNDCOVER_BENCH_BLADE_VERTS consecutive invocations sampling the same
// world XZ (§4's blade geometry is generated from a seed around one root
// point), which is exactly the coherence a real blade shader would enjoy.
//
// Compile with:
//   glslangValidator -V -Icrates/renderer/shaders groundcover_bench.vert -o groundcover_bench.vert.spv

#include "include/shader_constants.glsl"
#include "include/groundcover_bench.glsl"

void main() {
    uint vid = uint(gl_VertexIndex);
    uint bladeIdx = vid / GROUNDCOVER_BENCH_BLADE_VERTS;

    uint bladesPerChunk = max(pc.bladesPerChunk, 1u);
    uint chunkIdx = bladeIdx / bladesPerChunk;
    uint bladeInChunk = bladeIdx % bladesPerChunk;

    float acc = 0.0;
    if (chunkIdx < pc.chunkCount) {
        BenchChunk chunk = chunks[chunkIdx];
        // Re-sampled per vertex — the arm §4 is weighing against storing the
        // normal and albedo on the blade record. Every vertex of the blade
        // repeats this lookup at the blade's root.
        acc = benchFold(benchSample(chunk, benchCandidateWorld(chunk, bladeInChunk)));
    }

    // `sinkScale` is 0.0 at runtime, unknown at compile time: the sample
    // cannot be optimised away, and every vertex still collapses onto one
    // point so the triangle is degenerate. Depth 0.5 / w 1.0 keeps it inside
    // the clip volume; a clipped-away primitive could let a driver skip work
    // the real pass would do.
    float sink = acc * pc.sinkScale;
    gl_Position = vec4(sink, sink, 0.5, 1.0);
}
