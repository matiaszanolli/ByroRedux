#version 450

// Fullscreen triangle via gl_VertexIndex — no vertex buffer binding needed.
// Standard three-vertex trick that covers the whole screen with a single
// oversized triangle. The hardware rasterizer clips the out-of-screen parts.
//
//   vertex 0: (-1, -1)  — top-left of screen
//   vertex 1: (-1,  3)  — below bottom-left
//   vertex 2: ( 3, -1)  — right of top-right
//
// Vulkan NDC has Y pointing DOWN, so (-1, -1) is top-left. UV maps so that
// (0, 0) = top-left and (1, 1) = bottom-right, matching how texture()
// samples a 2D image.

layout(location = 0) out vec2 fragUV;

void main() {
    vec2 pos = vec2(
        float((gl_VertexIndex << 1) & 2) * 2.0 - 1.0,
        float(gl_VertexIndex & 2) * 2.0 - 1.0
    );
    gl_Position = vec4(pos, 0.0, 1.0);
    fragUV = (pos + 1.0) * 0.5;
}
