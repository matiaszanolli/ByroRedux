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
    // Transform normal by model's upper 3x3. For uniform scale (our
    // Transform uses f32 scale), this is equivalent to inverse-transpose.
    // Guard against zero-scale meshes (NIF placeholders, animated transitions)
    // where mat3(model) is degenerate and normalize() would produce NaN.
    vec3 n = mat3(pc.model) * inNormal;
    fragNormal = (dot(n, n) > 0.0) ? normalize(n) : vec3(0.0, 1.0, 0.0);
    fragWorldPos = worldPos.xyz;
}
