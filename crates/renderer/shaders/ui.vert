#version 450

// UI overlay vertex shader — passthrough to clip space, no transforms.
// Vertices are already in NDC ([-1,1] range).

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;    // unused, but must match Vertex layout
layout(location = 2) in vec3 inNormal;   // unused
layout(location = 3) in vec2 inUV;

layout(location = 0) out vec2 fragUV;

void main() {
    gl_Position = vec4(inPosition.xy, 0.0, 1.0);
    fragUV = inUV;
}
