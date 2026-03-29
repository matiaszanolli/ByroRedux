#version 450

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;
layout(location = 2) in vec2 inUV;

layout(push_constant) uniform PushConstants {
    mat4 viewProj;
    mat4 model;
} pc;

layout(location = 0) out vec3 fragColor;
layout(location = 1) out vec2 fragUV;

void main() {
    gl_Position = pc.viewProj * pc.model * vec4(inPosition, 1.0);
    fragColor = inColor;
    fragUV = inUV;
}
