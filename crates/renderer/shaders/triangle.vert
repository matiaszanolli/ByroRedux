#version 450

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;
layout(location = 2) in vec3 inNormal;
layout(location = 3) in vec2 inUV;

layout(push_constant) uniform PushConstants {
    mat4 viewProj;
    mat4 model;
} pc;

layout(location = 0) out vec3 fragColor;
layout(location = 1) out vec2 fragUV;
layout(location = 2) out vec3 fragNormal;
layout(location = 3) out vec3 fragWorldPos;

void main() {
    vec4 worldPos = pc.model * vec4(inPosition, 1.0);
    gl_Position = pc.viewProj * worldPos;
    fragColor = inColor;
    fragUV = inUV;
    // Transform normal by inverse-transpose of the model matrix upper 3x3.
    // Correct for non-uniform scale (mat3(model) alone distorts normals).
    fragNormal = transpose(inverse(mat3(pc.model))) * inNormal;
    fragWorldPos = worldPos.xyz;
}
