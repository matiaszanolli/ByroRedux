#ifndef BYRO_MESH_ID_GLSL
#define BYRO_MESH_ID_GLSL

// MESH_ID_NO_HISTORY_BIT / MESH_ID_STABLE_MASK come from the generated
// header (#3881) — the bit is a CPU/GPU attachment contract, so it is defined
// once in shader_constants_data.rs rather than here.
//
// Temporal history is valid only for opaque IDs. Bit 31 does not decorate a
// stable ID: it switches the low bits into the alpha draw-index namespace.
// Masking it before comparison can therefore alias unrelated surfaces.
#include "shader_constants.glsl"

bool meshIdHasStableHistory(uint id) {
    return (id & MESH_ID_NO_HISTORY_BIT) == 0u;
}

bool stableMeshIdsMatch(uint lhs, uint rhs) {
    return meshIdHasStableHistory(lhs)
        && meshIdHasStableHistory(rhs)
        && lhs == rhs;
}

#endif
