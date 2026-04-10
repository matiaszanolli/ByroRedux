#version 450

// UI overlay vertex shader — passthrough to clip space, no transforms.
// Vertices are already in NDC ([-1,1] range).
// Uses UiVertex (position + UV only, 20 bytes).
// Reads texture index from instance SSBO for bindless sampling.

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec2 inUV;

struct GpuInstance {
    mat4 model;
    uint textureIndex;
    uint boneOffset;
    uint normalMapIndex;
    float roughness;
    float metalness;
    float emissiveMult;
    float emissiveR, emissiveG, emissiveB;
    float specularStrength;
    float specularR, specularG, specularB;
    uint vertexOffset;
    uint indexOffset;
    uint vertexCount;
};

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

layout(location = 0) out vec2 fragUV;
layout(location = 1) flat out uint fragTexIndex;

void main() {
    gl_Position = vec4(inPosition.xy, 0.0, 1.0);
    fragUV = inUV;
    fragTexIndex = instances[gl_InstanceIndex].textureIndex;
}
