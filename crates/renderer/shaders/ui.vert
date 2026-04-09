#version 450

// UI overlay vertex shader — passthrough to clip space, no transforms.
// Vertices are already in NDC ([-1,1] range).
// Uses UiVertex (position + UV only, 20 bytes) — no bone/normal/color fields.

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec2 inUV;

layout(location = 0) out vec2 fragUV;

void main() {
    gl_Position = vec4(inPosition.xy, 0.0, 1.0);
    fragUV = inUV;
}
