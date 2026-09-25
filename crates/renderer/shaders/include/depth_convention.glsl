// Depth-buffer convention (#3308).
//
// The engine's depth mapping is named once, on the Rust side, by
// `ACTIVE_DEPTH_MAPPING` in `crates/core/src/ecs/components/camera.rs`.
// `BYRO_REVERSED_Z` and `BYRO_DEPTH_CLEAR` are *generated* from it into
// `include/shader_constants.glsl` by the renderer's build script, so there is
// no hand-maintained copy here and nothing to keep in sync — this file only
// turns those two values into the predicates shaders actually call.
//
// SPIR-V is precompiled and checked in, so flipping the engine to reversed-Z
// still means `cargo build -p byroredux-renderer` (regenerates the header)
// followed by recompiling every shader that includes this one.
//
// Conventional: near -> 0, far -> 1, cleared to 1.0.
// Reversed-Z:   near -> 1, far -> 0, cleared to 0.0.
//
// Only the *encoding* changes. Reconstructing a world position from a depth
// sample through `inv_view_proj` needs nothing from this file — the inverse
// projection already carries whichever mapping built it. What does need it is
// every test of the form "is this pixel background, or did something get drawn
// here", because those compare against a literal end of the range.

#ifndef BYRO_DEPTH_CONVENTION_GLSL
#define BYRO_DEPTH_CONVENTION_GLSL

// BYRO_REVERSED_Z (0 = conventional near->0, 1 = reversed-Z near->1) and
// BYRO_DEPTH_CLEAR (the far plane's encoding, which is what the depth
// attachment is cleared to) both come from here.
#include "include/shader_constants.glsl"

// True when nothing was drawn at this sample — the depth still holds the
// clear value.
//
// Exact rather than epsilon-tolerant: the clear value is written verbatim by
// the clear, and a real surface at the far plane is vanishingly rare and
// harmless to treat as background. The epsilon variant below exists for the
// consumers that deliberately want a slack band.
bool depthIsBackground(float z) {
#if BYRO_REVERSED_Z
    return z <= BYRO_DEPTH_CLEAR;
#else
    return z >= BYRO_DEPTH_CLEAR;
#endif
}

// True when a surface was drawn at this sample.
bool depthIsSurface(float z) {
    return !depthIsBackground(z);
}

// `depthIsBackground` with a slack band of `eps` in encoded units, for
// consumers that want near-far samples treated as background too (SSAO's sky
// rejection, historically `depth >= 0.999`).
//
// `eps` is expressed as a positive distance from the clear value under either
// mapping, so a call site does not have to know which direction the buffer
// runs.
bool depthIsBackgroundEps(float z, float eps) {
#if BYRO_REVERSED_Z
    return z <= BYRO_DEPTH_CLEAR + eps;
#else
    return z >= BYRO_DEPTH_CLEAR - eps;
#endif
}

// Recover the view-space depth (distance along the camera's forward axis, in
// world units) that a depth sample encodes at `ndcXY`.
//
// Decoded from the inverse view-projection alone, so it needs no near/far
// planes and no knowledge of the mapping — the inverse matrix already carries
// both. That is the point: a decode that takes `near`/`far` as arguments is
// only correct with the camera projection's own planes, and no shader UBO
// carries them (`CameraUBO.screen.zw` and `CompositeParams.fog_params.xy` are
// the cell's authored FOG near/far — a different pair, 0/0 with no lighting).
// Feeding those to an `n / (1 - z*(1 - n/f))` inverse made the #4588 caustic
// gate 3 %..56 % instead of 3 %, and left composite's froxel bilateral inert
// whenever fog near was ~0 (#4831).
//
// Derivation: with `ndc = (x, y, z, 1)`, `P_h = invViewProj * ndc` satisfies
// `viewProj * P_h = ndc`, so the clip position of the world point
// `P_h.xyz / P_h.w` is `ndc / P_h.w` and its `clip.w` — the perspective
// divisor, i.e. the view-space depth — is `1 / P_h.w`. Exact for any
// invertible view-projection with a standard perspective last row, including
// the TAA-jittered one, under either mapping.
//
// The GLSL counterpart of `Camera::linear_distance_from_depth` /
// `linear_distance_from_depth_reversed` (core), which take the planes because
// the CPU has them.
float depthViewSpace(mat4 invViewProj, vec2 ndcXY, float z) {
    float w = (invViewProj * vec4(ndcXY, z, 1.0)).w;
    // Floor only guards the divide: a surface sample's w is 1/depth, so it
    // stays orders of magnitude above this (400000 BU far -> 2.5e-6).
    return 1.0 / max(abs(w), 1.0e-9);
}

// True when encoded depth `a` is strictly nearer the camera than `b`.
// Consumers that only need "which of two depths is in front" should call
// this instead of spelling out the mapping's direction — the whole point
// of this header (#4545's caustic occlusion gate is the first caller).
bool depthIsInFront(float a, float b) {
#if BYRO_REVERSED_Z
    return a > b;
#else
    return a < b;
#endif
}

#endif // BYRO_DEPTH_CONVENTION_GLSL
