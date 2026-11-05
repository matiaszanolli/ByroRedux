// The one GLSL declaration of the Rust `GpuBenchCell`
// (`vulkan/groundcover_bench.rs`), read by the bench and its bake pass.
// Struct-only, so either can include it.
#ifndef BYRO_GROUNDCOVER_BENCH_CELL_GLSL
#define BYRO_GROUNDCOVER_BENCH_CELL_GLSL

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

#endif // BYRO_GROUNDCOVER_BENCH_CELL_GLSL
