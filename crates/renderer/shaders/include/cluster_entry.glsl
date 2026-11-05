// The one GLSL declaration of the Rust `ClusterEntry` (`vulkan/compute.rs`):
// offset into the flat light index list + count. `cluster_cull.comp` writes the
// grid; a reader includes this and keeps only its own `ClusterGrid` block,
// numbered for its own descriptor set. Struct-only, so any stage can include it.
#ifndef BYRO_CLUSTER_ENTRY_GLSL
#define BYRO_CLUSTER_ENTRY_GLSL

struct ClusterEntry {
    uint offset;
    uint count;
};

#endif // BYRO_CLUSTER_ENTRY_GLSL
