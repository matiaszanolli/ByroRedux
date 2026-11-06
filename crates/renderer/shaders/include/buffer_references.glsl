// The `buffer_reference` types shaders use to dereference raw device addresses,
// each declared once (#6681). A `buffer_reference` block is a type, not a
// descriptor binding: it names no set or binding, so a stage on any layout can
// include this header.
//
// The includer supplies `#extension GL_EXT_buffer_reference : require`, because an
// extension directive cannot follow declarations.
//
// Types only: `shared_gpu_structs_are_declared_once` fails on a second declaration
// in any shader and on anything here that binds a resource.

// REN-2026-07-28-02 / #2219 — `GpuInstance.skinnedVertexAddress`.
layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer SkinnedVertexRef {
    float data[];
};

// #3231 — `GpuInstance.morphDeltaAddress` / `.morphWeightAddress`. `MorphDeltaRef` is
// `vec4`-aligned (not `vec3`) for the same std430 reason the shared GPU structs
// avoid `vec3` — `data[target * vertexCount + localVertexIndex].xyz` is the
// per-vertex delta; `.w` is unused padding.
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer MorphDeltaRef {
    vec4 data[];
};
layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer MorphWeightRef {
    float data[];
};
