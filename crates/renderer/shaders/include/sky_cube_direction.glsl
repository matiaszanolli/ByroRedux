#ifndef BYRO_SKY_CUBE_DIRECTION
#define BYRO_SKY_CUBE_DIRECTION
// Inverse Vulkan cube face selection; uv is face-local in [-1, 1].
vec3 sky_cube_face_direction(uint face, vec2 uv) {
    float u = uv.x;
    float v = uv.y;
    switch (face) {
        case 0u: return vec3( 1.0,   -v,   -u); // +X
        case 1u: return vec3(-1.0,   -v,    u); // -X
        case 2u: return vec3(   u,  1.0,    v); // +Y
        case 3u: return vec3(   u, -1.0,   -v); // -Y
        case 4u: return vec3(   u,   -v,  1.0); // +Z
        default: return vec3(  -u,   -v, -1.0); // -Z
    }
}
#endif
