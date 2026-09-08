#ifndef BYRO_OCT_CODEC_GLSL
#define BYRO_OCT_CODEC_GLSL

// ── Octahedral normal codec (Cigolle et al. 2014) ───────────────────
//
// STANDALONE. Pure math over `vec2`/`vec3` — no bindings, no structs, no
// generated constants — so any stage can `#include "include/oct_codec.glsl"`
// at any point, unlike `include/math_common.glsl`, which needs the DALC
// uniforms and the cluster constants to already be in scope.
//
// This header exists because the decoder used to be copy-pasted into five
// files (#4008 / REN-2026-09-06-D13-02). #3607 fixed the *discoverability*
// half of that — every copy was renamed to `octDecode` and given a
// maintenance comment enumerating its siblings — but not the *drift* half:
// every guard was a name/count pin, so a one-line edit to some of the
// copies would still have passed the whole suite while leaving TAA
// rejecting history on a different predicate than SVGF, and both of them
// disagreeing with the `octEncode` producer that wrote the G-buffer.
// Encoder and decoder must stay exact inverses, so they live together.
//
// Encodes a unit normal into 2 components for RG16_SNORM storage; saves
// 50% G-buffer bandwidth vs RGBA16_SNORM. See #275.
//
// Producer: `triangle.frag`'s `outNormal = octEncode(...)` writes, plus the
// ReSTIR reservoir's `packSnorm2x16(octEncode(...))` geometric-normal tag.
// Consumers: `taa.comp`, `svgf_temporal.comp`, `svgf_atrous.comp`,
// `caustic_splat.comp`, `triangle.frag`.
vec2 octEncode(vec3 n) {
    n /= (abs(n.x) + abs(n.y) + abs(n.z));
    if (n.z < 0.0) {
        n.xy = (1.0 - abs(n.yx)) * vec2(n.x >= 0.0 ? 1.0 : -1.0,
                                        n.y >= 0.0 ? 1.0 : -1.0);
    }
    return n.xy;
}

vec3 octDecode(vec2 e) {
    vec3 n = vec3(e.xy, 1.0 - abs(e.x) - abs(e.y));
    if (n.z < 0.0) {
        n.xy = (1.0 - abs(n.yx)) * vec2(n.x >= 0.0 ? 1.0 : -1.0,
                                        n.y >= 0.0 ? 1.0 : -1.0);
    }
    return normalize(n);
}

#endif
