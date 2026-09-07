#ifndef BYRO_GROUNDCOVER_INTERACTION_GLSL
#define BYRO_GROUNDCOVER_INTERACTION_GLSL

// EXAL ground cover — the interaction displacement field (§12.4, #4058).
//
// Grass that does not move when something walks through it is static scenery,
// whatever else is right about it. This is the term with the largest gap
// between "cheap" and "sells the whole feature".
//
// Structurally the same object as §8's wind: a small world-space field centred
// on the camera, sampled by the blade vertex shader at the blade base and
// applied as a bend away from the disturbance. Because neighbouring blades
// sample a *continuous* field at nearby points they part **together** — a
// channel through the grass, not a ring of individually tilted blades.
//
// Requires `#include "include/shader_constants.glsl"` first.
//
// ## Stateful, and that is the interesting part
//
// The naive version re-renders the field from this frame's disturbers, which
// makes a blade snap upright the instant an entity passes — worse than no
// interaction at all, because it draws the eye straight to the boundary. This
// field is **accumulated and decayed**: `groundcover_interaction.comp` reads
// last frame's value, decays it, and takes the per-texel maximum against this
// frame's disturbers. A trail therefore persists behind a runner and fades.
//
// ## Why the origin is texel-snapped
//
// The field is centred on the camera, so it scrolls. If its origin were the
// raw camera position the reprojection would be a fractional texel offset and
// would need a filtered fetch — and a filter applied every frame to its own
// output is a low-pass running at frame rate, which turns a crisp channel into
// a smear within a second or two. Snapping the origin to the texel grid makes
// the frame-to-frame offset an exact integer, so the reprojection is a copy.
//
// ## Storage
//
// One `uint` per texel — the XZ displacement as `packHalf2x16`. The field is a
// plain SSBO rather than a storage image on purpose: the blade vertex shader
// needs a bilinear read, which is six lines of arithmetic here, against a
// sampler, two image layouts and a transition per frame there. Two halves live
// in the one buffer and alternate, so the ping-pong is an index rather than a
// second allocation.

/// Texels in one half of the field buffer.
#define GROUNDCOVER_INTERACTION_TEXEL_COUNT \
    (GROUNDCOVER_INTERACTION_TEXELS * GROUNDCOVER_INTERACTION_TEXELS)

/// World units covered by one texel.
#define GROUNDCOVER_INTERACTION_TEXEL_UNITS \
    (GROUNDCOVER_INTERACTION_UNITS / float(GROUNDCOVER_INTERACTION_TEXELS))

/// Flat index of `texel` within half `writeHalf` of the field buffer.
uint byroGcFieldIndex(ivec2 texel, uint writeHalf) {
    return writeHalf * GROUNDCOVER_INTERACTION_TEXEL_COUNT
         + uint(texel.y) * GROUNDCOVER_INTERACTION_TEXELS
         + uint(texel.x);
}

/// World XZ of a texel's centre, given the field's snapped origin (its
/// minimum corner in both axes).
vec2 byroGcFieldTexelWorld(vec2 origin, ivec2 texel) {
    return origin + (vec2(texel) + 0.5) * GROUNDCOVER_INTERACTION_TEXEL_UNITS;
}

/// Continuous texel coordinate of a world point. Fractional — the blade
/// shader interpolates, the compute pass rounds.
vec2 byroGcFieldCoord(vec2 origin, vec2 worldXZ) {
    return (worldXZ - origin) / GROUNDCOVER_INTERACTION_TEXEL_UNITS - 0.5;
}

bool byroGcFieldInBounds(ivec2 texel) {
    return texel.x >= 0 && texel.y >= 0
        && texel.x < int(GROUNDCOVER_INTERACTION_TEXELS)
        && texel.y < int(GROUNDCOVER_INTERACTION_TEXELS);
}

/// Displacement one entity imposes on a point, in the field's units.
///
/// Pushes **away** from the disturber, full inside its radius and gone by
/// 1.5× it. The taper is the reason the channel has soft walls rather than a
/// visible cylinder cut through the sward.
///
/// `disturber` is `(worldX, worldZ, radius, strength)`.
vec2 byroGcDisturbance(vec4 disturber, vec2 worldXZ) {
    vec2 delta = worldXZ - disturber.xy;
    float dist = length(delta);
    float radius = max(disturber.z, 1.0);
    // Dead centre has no direction to push in, and every blade there is under
    // the entity anyway. Falling back to zero rather than an arbitrary axis
    // keeps the field continuous through the middle.
    if (dist < 1.0e-3) {
        return vec2(0.0);
    }
    float influence = 1.0 - smoothstep(radius * 0.5, radius * 1.5, dist);
    return (delta / dist) * (influence * clamp(disturber.w, 0.0, 1.0));
}

#endif // BYRO_GROUNDCOVER_INTERACTION_GLSL
