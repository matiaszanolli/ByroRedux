#version 450
#extension GL_EXT_nonuniform_qualifier : require

// UI overlay fragment shader — bindless texture-only, no lighting.

layout(location = 0) in vec2 fragUV;
layout(location = 1) flat in uint fragTexIndex;

layout(location = 0) out vec4 outColor;

// Bindless texture array (same as scene shader).
layout(set = 0, binding = 0) uniform sampler2D textures[];

void main() {
    outColor = texture(textures[nonuniformEXT(fragTexIndex)], fragUV);
}
