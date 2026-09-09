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

// Recover the view-space eye distance a depth sample encodes.
//
// The GLSL twin of `Camera::linear_distance_from_depth` /
// `linear_distance_from_depth_reversed` (core), and the one place a *decode*
// has to know the mapping — reconstructing a world position through
// `inv_view_proj` does not, because the inverse matrix already carries it.
//
// Conventional: z = f/(f-n) * (1 - n/d)  =>  d = n / (1 - z*(1 - n/f))
// Reversed:     z = (n/d - n/f)/(1 - n/f) =>  d = n / (z*(1 - n/f) + n/f)
//
// The two differ only by the affine flip `z -> 1 - z`, which is why the
// denominators are mirror images.
float depthLinearize(float z, float nearPlane, float farPlane) {
    float nOverF = nearPlane / max(farPlane, 1.0e-4);
#if BYRO_REVERSED_Z
    float denom = z * (1.0 - nOverF) + nOverF;
#else
    float denom = 1.0 - z * (1.0 - nOverF);
#endif
    return nearPlane / max(denom, 1.0e-6);
}

#endif // BYRO_DEPTH_CONVENTION_GLSL
