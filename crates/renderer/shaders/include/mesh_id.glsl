#ifndef BYRO_MESH_ID_GLSL
#define BYRO_MESH_ID_GLSL

// Temporal history is valid only for opaque IDs. Bit 31 does not decorate a
// stable ID: it switches the low bits into the alpha draw-index namespace.
// Masking it before comparison can therefore alias unrelated surfaces.
const uint MESH_ID_NO_HISTORY_BIT = 0x80000000u;

bool meshIdHasStableHistory(uint id) {
    return (id & MESH_ID_NO_HISTORY_BIT) == 0u;
}

bool stableMeshIdsMatch(uint lhs, uint rhs) {
    return meshIdHasStableHistory(lhs)
        && meshIdHasStableHistory(rhs)
        && lhs == rhs;
}

#endif
