#version 460
#extension GL_EXT_nonuniform_qualifier : require

// ── Water surface vertex shader ───────────────────────────────────────
//
// Drives:
//   • Calm lakes / pools             (`WaterKind::Calm`)
//   • Rivers and canals              (`WaterKind::River`)
//   • Rapids / whitewater            (`WaterKind::Rapids`)
//   • Waterfall sheets               (`WaterKind::Waterfall`)
//
// The water mesh is *always* a flat quad in mesh-local space (no
// per-frame BLAS rebuild — see the CP2077 / Cyberpunk 2077 design
// note in `crates/core/src/ecs/components/water.rs`). Wave detail
// is added in the fragment shader as a perturbation of the shading
// normal; the BLAS sees a flat plate.
//
// Inputs reuse the engine `Vertex` layout exactly so the renderer
// can share its global vertex SSBO + index buffer with the rest of
// the world. Bone indices / weights / splat weights are unused by
// water draws (they're authored zero on the water quad meshes); we
// keep the attribute slots wired so `triangle.vert`'s VAO is
// reusable without a second `VertexInputState`.

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;       // unused on water — kept for VAO compat
layout(location = 2) in vec3 inNormal;
layout(location = 3) in vec2 inUV;
layout(location = 4) in uvec4 inBoneIndices; // unused
layout(location = 5) in vec4  inBoneWeights; // unused
layout(location = 6) in vec4 inSplat0;       // unused
layout(location = 7) in vec4 inSplat1;       // unused
layout(location = 8) in vec4 inTangent;      // xyz = tangent, w = bitangent sign

// ── Per-instance SSBO (shared with triangle pipeline) ─────────────────
// We only consume `model`; the rest of the GpuInstance fields are
// not driven by the water material path (which lives in push
// constants — see water.frag). Layout must match the Rust struct
// at `crates/renderer/src/vulkan/instance.rs` byte-for-byte.
struct GpuInstance {
    mat4 model;
    uint textureIndex;
    uint boneOffset;
    uint vertexOffset;
    uint indexOffset;
    uint vertexCount;
    uint flags;
    uint materialId;
    float _padId0;
    float avgAlbedoR;
    float avgAlbedoG;
    float avgAlbedoB;
    float _padAlbedo;
};

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

layout(set = 1, binding = 1) uniform CameraUBO {
    mat4 viewProj;
    mat4 prevViewProj;
    mat4 invViewProj;
    vec4 cameraPos;
    vec4 sceneFlags;
    vec4 screen;
    vec4 fog;
    vec4 jitter;
    vec4 skyTint;
};

layout(location = 0) out vec3 vWorldPos;
layout(location = 1) out vec3 vWorldNormal;
layout(location = 2) out vec3 vWorldTangent;
layout(location = 3) out float vWorldBitangentSign;
layout(location = 4) out vec2 vUV;
layout(location = 5) flat out int vInstanceIndex;

void main() {
    GpuInstance inst = instances[gl_InstanceIndex];

    vec4 worldPos = inst.model * vec4(inPosition, 1.0);
    vWorldPos = worldPos.xyz;

    // For the water quad, `inst.model` is composed of (translation,
    // axis-aligned rotation, uniform scale) — see the cell loader's
    // water-plane spawn. So the 3×3 upper block is a similarity
    // transform and we can transform normal / tangent with it
    // directly (no inverse-transpose needed). The renderer guarantees
    // water meshes never carry non-uniform scale (`INSTANCE_FLAG_NUS`
    // is clear).
    mat3 modelRot = mat3(inst.model);
    vWorldNormal       = normalize(modelRot * inNormal);
    vWorldTangent      = normalize(modelRot * inTangent.xyz);
    vWorldBitangentSign = inTangent.w;

    vUV = inUV;
    vInstanceIndex = gl_InstanceIndex;

    // TAA jitter pulled from the camera UBO — keeps water's projected
    // depth coherent with the opaque pass so the shoreline foam ray
    // and the depth buffer stay in lockstep.
    vec4 clip = viewProj * worldPos;
    clip.xy += jitter.xy * clip.w;
    gl_Position = clip;
}
