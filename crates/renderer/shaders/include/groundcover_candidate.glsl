// EXAL ground cover — the candidate-point sequence (§4).
//
// NON-STANDALONE shader fragment. Included by groundcover_scatter.comp, which
// places blades from it, and by include/groundcover_bench.glsl, the §11.1
// sampling bench that exists to time that same distribution.
//
// #4348 — the two used to carry byte-identical copies (`gcCandidate` and
// `benchCandidate`). A bench that re-types the sequence it measures stops
// measuring production the moment one copy is edited, which is the same reason
// #4057 moved the Laplacian into a shared include.
#ifndef BYRO_GROUNDCOVER_CANDIDATE_GLSL
#define BYRO_GROUNDCOVER_CANDIDATE_GLSL

// The R2 lattice (Roberts 2018) — a progressive low-discrepancy sequence,
// scrambled per chunk by Cranley–Patterson rotation.
//
// Progressive is the requirement §4 spells out: every prefix is
// well-distributed, so a chunk that saturates its slice degrades to a
// uniformly sparser chunk rather than to a directionally clumped one.
// Deterministic in the chunk's seed, so a blade does not move when the camera
// does; the per-chunk rotation decorrelates neighbouring chunks.
vec2 byroGcCandidate(uint index, uint seed) {
    // R2: the 2-D generalisation of the golden-ratio sequence, plastic
    // constant 1.32471795724474602596.
    const float A1 = 0.7548776662466927;
    const float A2 = 0.5698402909980532;
    float u = fract(0.5 + A1 * float(index));
    float v = fract(0.5 + A2 * float(index));
    uint h = seed * 0x9E3779B9u;
    h ^= h >> 15;
    h *= 0x85EBCA6Bu;
    h ^= h >> 13;
    return vec2(
        fract(u + float(h & 0xFFFFu) * (1.0 / 65536.0)),
        fract(v + float((h >> 16) & 0xFFFFu) * (1.0 / 65536.0))
    );
}

#endif // BYRO_GROUNDCOVER_CANDIDATE_GLSL
