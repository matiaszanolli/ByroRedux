// Shared 8×8 void-and-cluster blue-noise rank table.
//
// NON-STANDALONE shader fragment. Included by composite.frag and
// volumetrics_inject.comp via GL_GOOGLE_include_directive. Do not compile
// on its own.
//
// #3742 (TD2-2026-08-30-02) — was hand-duplicated byte-identically in both
// consumers (composite.frag's `preResolveDither`, volumetrics_inject.comp's
// `blueNoiseRank`); each consumer's tiling offsets are its own business,
// but the table itself is exactly the kind of value that must never
// diverge: if one copy is regenerated and the other isn't, the composite
// dither and the froxel jitter fall out of phase and produce correlated
// banding that looks like a denoiser bug, not a constants bug. One copy
// here instead.

const uint BLUE_NOISE_RANKS[64] = uint[64](
     0u, 41u, 11u, 59u,  2u, 40u, 10u, 32u,
    51u, 28u, 39u, 24u, 54u, 17u, 42u, 20u,
    12u, 62u,  4u, 53u, 14u, 33u,  6u, 52u,
    48u, 29u, 36u, 22u, 47u, 25u, 35u, 21u,
     3u, 50u, 15u, 38u,  1u, 61u,  8u, 49u,
    45u, 23u, 60u, 31u, 44u, 26u, 55u, 18u,
    13u, 43u,  5u, 58u,  9u, 46u,  7u, 34u,
    63u, 16u, 57u, 27u, 37u, 19u, 56u, 30u
);
