// SKYAL — cloud-shadow map addressing, shared by the bake
// (`cloud_shadow.comp`) and every surface that samples it (`bindings.glsl`).
//
// The map is a CLOUD_SHADOW_MAP_EXTENT square of CLOUD_SHADOW_MAP_SIZE
// texels, centred on the camera and snapped to whole texels in world space,
// so a moving viewer does not make the shadows swim. A texel holds the
// direct-sun transmittance of the column above the point where a sun ray
// crosses the deck's BASE altitude. A receiver at any altitude below the deck
// looks up the texel its own sun ray crosses, which keeps one 2D map exact for
// receivers on hills and in valleys alike.
//
// All positions are world metres (renderer Y-up). Altitudes are relative to
// the viewer, which the sky model places at altitude 0.
//
// Requires `include/shader_constants.glsl`.
#ifndef CLOUD_SHADOW_GLSL
#define CLOUD_SHADOW_GLSL

// Snapped map centre for a camera at `camera_xz`.
vec2 cloud_shadow_map_centre(vec2 camera_xz) {
    float texel = CLOUD_SHADOW_MAP_EXTENT / CLOUD_SHADOW_MAP_SIZE;
    return floor(camera_xz / texel) * texel;
}

// Where the ray toward the sun from a receiver at `receiver_xz`, `altitude`
// metres above the viewer, crosses the deck base. The 1e-3 floor only keeps a
// horizon sun finite; the result then lands past the map edge, whose texel
// the clamped sampler returns. That texel holds the uniform overcast term
// alone (`cloud_shadow_edge_weight`), so a far crossing reads the broad cover
// without any local shadow smeared over it.
vec2 cloud_shadow_base_crossing(vec2 receiver_xz, float altitude, vec3 to_sun) {
    return receiver_xz + to_sun.xz / max(to_sun.y, 1.0e-3) * (CLOUD_LAYER_BOTTOM - altitude);
}

// Map UV of a deck-base crossing point.
vec2 cloud_shadow_uv(vec2 base_xz, vec2 centre) {
    return (base_xz - centre) / CLOUD_SHADOW_MAP_EXTENT + 0.5;
}

// How much of a texel's LOCAL deck shadow survives at map `uv`: 1.0 inside,
// 0.0 at the edge. The bake blends each texel from the uniform overcast term
// (weight 0) to the full column (weight 1), so the clamped sampler beyond the
// edge returns the uniform term. Fading to 1.0 instead (#6310) dropped that
// term too, and overcast ground got the above-cloud sun once the crossing left
// the map, below about 3 degrees of sun elevation.
float cloud_shadow_edge_weight(vec2 uv) {
    vec2 inside = min(uv, 1.0 - uv);
    return clamp(min(inside.x, inside.y) / CLOUD_SHADOW_EDGE_FADE, 0.0, 1.0);
}

#endif // CLOUD_SHADOW_GLSL
