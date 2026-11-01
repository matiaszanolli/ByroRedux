// World-space combustion local grid — per-frame parameters and box lookup
// (tracking #6531, Phase 1 #6533).
//
// The same UBO is bound by the froxel inject pass (hand-off of coarse state,
// irradiance field) and by composite (the close-range ray-march), so both agree
// on which box owns which point. Positions passed to the helpers below are
// ABSOLUTE world positions; composite adds `render_origin.xyz` to its
// render-origin-relative ones first.
//
// Needs, before the include: shader_constants.glsl (LOCAL_GRID_DIM,
// LOCAL_GRID_MAX_BOXES) and `#define LOCAL_GRID_PARAMS_BINDING <n>`.
#ifndef BYRO_LOCAL_GRID_PARAMS_GLSL
#define BYRO_LOCAL_GRID_PARAMS_GLSL

layout(std140, set = 0, binding = LOCAL_GRID_PARAMS_BINDING) uniform LocalGridParams {
    // xyz = absolute world position of the box's minimum corner, w = cell edge
    // in world units. Slot b owns atlas Z slab b.
    vec4 boxes[LOCAL_GRID_MAX_BOXES];
    // x = atlas slabs in use, y = bit b set when slot b has history,
    // z = bit b set when slot b holds a live box, w = unused.
    uvec4 info;
    // xyz = direction TO the sun (world space, unit), w unused.
    vec4 sun_dir;
    // rgb = sun radiance with intensity applied (zero when the sun is off),
    // w unused.
    vec4 sun_radiance;
    // Sun phase: x = forward-lobe g, y = backward-lobe g, z = forward mix.
    vec4 phase;
    // xyz = the render origin the camera-relative frame is offset by.
    vec4 render_origin;
} localGrid;

bool localBoxLive(uint box) {
    return (localGrid.info.z & (1u << box)) != 0u;
}

float localBoxEdge(uint box) {
    return localGrid.boxes[box].w * float(LOCAL_GRID_DIM);
}

// The box that owns `p`: the finest live box containing it (ties go to the
// lower slot). Finer boxes win so a campfire box nested in a larger blast box
// is not drawn twice.
bool localBoxOwner(vec3 p, out uint owner) {
    owner = 0u;
    bool found = false;
    float best = 0.0;
    for (uint b = 0u; b < uint(LOCAL_GRID_MAX_BOXES); ++b) {
        if (!localBoxLive(b)) {
            continue;
        }
        vec3 local = p - localGrid.boxes[b].xyz;
        float edge = localBoxEdge(b);
        if (any(lessThan(local, vec3(0.0))) || any(greaterThanEqual(local, vec3(edge)))) {
            continue;
        }
        float cell = localGrid.boxes[b].w;
        if (!found || cell < best) {
            found = true;
            best = cell;
            owner = b;
        }
    }
    return found;
}

// True when `p` lies within `margin` world units of any live box (inside
// counts). Lets the passes that feed the close-range march prepare a one
// froxel skirt around each box, so filtering at the box edge has data.
bool localBoxNear(vec3 p, float margin) {
    for (uint b = 0u; b < uint(LOCAL_GRID_MAX_BOXES); ++b) {
        if (!localBoxLive(b)) {
            continue;
        }
        vec3 local = p - localGrid.boxes[b].xyz;
        float edge = localBoxEdge(b);
        if (all(greaterThanEqual(local, vec3(-margin)))
            && all(lessThan(local, vec3(edge + margin)))) {
            return true;
        }
    }
    return false;
}

// Atlas texture coordinate of absolute point `p` inside box `box`, clamped to
// the slab's texel centres so filtering never reaches a neighbouring box.
vec3 localAtlasUvw(uint box, vec3 p) {
    float dim = float(LOCAL_GRID_DIM);
    float slabs = float(LOCAL_GRID_MAX_BOXES);
    vec3 cells = (p - localGrid.boxes[box].xyz) / localGrid.boxes[box].w;
    vec3 uvw = cells / dim;
    float half_texel = 0.5 / dim;
    float zMin = (float(box) * dim + 0.5) / (dim * slabs);
    float zMax = (float(box + 1u) * dim - 0.5) / (dim * slabs);
    return vec3(
        clamp(uvw.xy, vec2(half_texel), vec2(1.0 - half_texel)),
        clamp((float(box) + uvw.z) / slabs, zMin, zMax)
    );
}

#endif
