// Per-instance data from the instance SSBO: the one GLSL declaration of the
// Rust `GpuInstance` (`vulkan/scene_buffer/gpu_types.rs`). A shader that reads
// or writes instances includes this and keeps only its own `InstanceBuffer`
// block, numbered for its own descriptor set. Struct-only, so any stage can
// include it. The including shader must enable `GL_ARB_gpu_shader_int64` for
// the `uint64_t` lanes.
//
// R1 Phase 6 collapsed the per-material fields onto the `MaterialBuffer` SSBO
// indexed by `materialId`; what's left is strictly per-DRAW data. Each draw's
// gl_InstanceIndex maps to one entry containing the model matrix, mesh refs,
// flags, materialId, and avgAlbedo (kept for caustic).
//
// CRITICAL: all scalars, NO vec3 (vec3 has 16-byte alignment in std430, which
// would mismatch the tightly-packed Rust #[repr(C)] struct).
#ifndef BYRO_GPU_INSTANCE_GLSL
#define BYRO_GPU_INSTANCE_GLSL

struct GpuInstance {
    mat4 model;            // offset 0,  64 bytes
    uint textureIndex;     // offset 64 — diffuse / albedo
    uint boneOffset;       // offset 68
    uint vertexOffset;     // offset 72
    uint indexOffset;      // offset 76
    uint vertexCount;      // offset 80
    // offset 84: per-instance bit flags + packed fields.
    //   bit 0      — non-uniform scale (#273)
    //   bit 1      — NiAlphaProperty blend bit (#263)
    //   bit 2      — caustic source (#321)
    //   bit 3      — terrain splat (#470); enables the ATXT blend loop
    //                against `terrainTiles[]`
    //   bits 16-31 — terrain tile index (only meaningful with bit 3),
    //                unpacked via INSTANCE_TERRAIN_TILE_SHIFT/_MASK
    uint flags;
    uint materialId;       // offset 88 — index into MaterialBuffer SSBO (R1)
    float ior;             // offset 92 — per-draw optical IOR (read by caustic_splat.comp)
    float avgAlbedoR;      // offset 96 — kept for caustic_splat.comp (set 0 reads, not migrated)
    float avgAlbedoG;      // offset 100
    float avgAlbedoB;      // offset 104
    uint surfaceId;        // offset 108 — stable temporal-shadow identity
    // REN-2026-07-28-02 / #2219 — GPU address of this entity's skinned-
    // vertex output buffer (position-only, SKIN_OUTPUT_STRIDE_FLOATS
    // floats/vertex, already absolute world space). `0` for rigid
    // instances (boneOffset == 0). Dereferenced via `SkinnedVertexRef`
    // (include/bindings.glsl) by ray_hit.glsl's hit-normal helpers
    // instead of the bind-pose vertexData SSBO, which otherwise produces
    // a bind-pose-shaped normal at the entity's root transform for every
    // animated actor's secondary rays (reflection/shadow/refraction).
    uint64_t skinnedVertexAddress; // offset 112, 8 bytes
    uvec2 _reserved;               // offset 120, 8 bytes -> total 128
    // #3231 — GPU morph-target blending. `morphDeltaAddress` points at
    // `targetCount` groups of `vertexCount` `vec4` position deltas (xyz,
    // w unused); `morphWeightAddress` points at `targetCount` scalar
    // floats, re-uploaded every frame from `AnimatedMorphWeights`. Both
    // `0` when this entity has no morph data (rigid instances, or
    // skinned instances whose mesh carries no `NiGeomMorpherController`
    // — the overwhelming majority). Dereferenced via the MorphDeltaRef /
    // MorphWeightRef buffer_reference types (include/bindings.glsl,
    // triangle.vert, skin_vertices.comp).
    uint64_t morphDeltaAddress;  // offset 128, 8 bytes
    uint64_t morphWeightAddress; // offset 136, 8 bytes
    uint morphTargetCount;       // offset 144, 4 bytes
    // Deliberately three scalar uints, NOT uvec3 — see the matching
    // Rust field doc (gpu_types.rs) for why: uvec3 is 16-byte-aligned
    // under std430 (same footgun as vec3), which desyncs the array
    // stride the shader computes from the one the CPU uploads with.
    uint _reserved2a; // offset 148, 4 bytes
    uint _reserved2b; // offset 152, 4 bytes
    uint _reserved2c; // offset 156, 4 bytes -> total 160
};

#endif // BYRO_GPU_INSTANCE_GLSL
