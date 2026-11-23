#ifndef BYRO_TERRAIN_TILE_GLSL
#define BYRO_TERRAIN_TILE_GLSL

// `GpuTerrainTile` and the terrain layer-height blend (#6196 / #6197), split
// out of `bindings.glsl` for TSD-4.1 (#7152) so `terrain_patch.comp` displaces
// its geometry with the very function the fragment shader blends and
// parallax-marches with: one source of terrain height, not two copies.
//
// Requires, before inclusion: `include/shader_constants.glsl`,
// `GL_EXT_nonuniform_qualifier`, and the bindless `sampler2D textures[]`
// array (any set/binding; the functions below read it by name).

// Per-terrain-tile bindless texture indices for LAND splat layers
// (#470). Fragment shader reads `terrainTiles[tileIdx]` when the
// `INSTANCE_FLAG_TERRAIN_SPLAT` bit (flags bit 3) is set. The tile
// index is packed into the top 16 bits of `flags` and must be
// unpacked with the generated `INSTANCE_TERRAIN_TILE_SHIFT`/`_MASK`
// macros, never a literal — see #4027.
struct GpuTerrainTile {
    uint layerDiffuseIndex[TERRAIN_SPLAT_LAYERS];
    uint layerNormalIndex[TERRAIN_SPLAT_LAYERS];
    uint layerSpecularIndex[TERRAIN_SPLAT_LAYERS];
    // ── EXAL ground cover, §12.5's terrain receiver (#4057) ──────────────
    //
    // Grass shadowing the ground is the half of §12.5 anyone actually
    // notices, and it needs `d_ground` at the terrain fragment. Splat
    // weights, the terrain normal and the global vertex SSBO are already
    // here; these four per-cell values were the only missing inputs, and
    // they ride the record the tile slot already indexes rather than a
    // parallel array plus a new descriptor binding to keep in step with it.
    //
    // std430: both vec4 rows are on their 16-byte boundaries (96, 112), the
    // Tier-3 atlas uvec4 lands on its own at 144, the #4903 base affinity
    // follows at 160, #6195's height handles at 168..204, #6196's band at
    // 204, #6197's parallax settings at 208..220, `lodSoftenEnd` at 220 and TSD-4.3's
    // patch-geometry lanes at 224..240. `gpu_terrain_tile_field_offsets_match_shader_contract` and
    // `gpu_terrain_tile_is_240_bytes` pin the Rust half against exactly
    // those numbers.
    /// `cover_affinity` for LAND splat layers 0-3 and 4-7.
    vec4 coverAffinity0;
    vec4 coverAffinity1;
    /// Y-up world XZ of the tile's (row 0, col 0) vertex.
    vec2 cellOriginXZ;
    /// Y-up water-plane height, or `GROUNDCOVER_NO_WATER`.
    float waterY;
    /// Canopy slab thickness, world units. **Zero disables the term** —
    /// a LOD tile, an interior, or a worldspace with no ground-cover
    /// palette, which is why there is no separate enable flag.
    float canopyHeight;
    // x = bindless per-species detail-atlas handle; y = active species
    // count. Zero x keeps Tier 3 disabled until the host has published the
    // palette-generated texture. z/w are std430 padding.
    uvec4 groundcoverDetailAtlas;
    /// The BTXT base LTEX's own `cover_affinity` (#4903) — the terrain-side
    /// density evaluations start their ordered mix from it. The named tail
    /// pads below round the record out to the 240-byte std430 array stride
    /// (the struct's 16-alignment from its vec4 rows).
    float baseCoverAffinity;
    /// Bindless diffuse handle of the BTXT base texture (#4907). Zero when
    /// the base failed to resolve.
    uint baseDiffuseIndex;
    /// #6195 — per-layer `[0, 1]` height maps (BC4), base then splat lanes.
    /// Zero = no height (yet). Appended at the tail: stride 176 → 208 at #6195/#6196, 224 at #6197, 240 since TSD-4.3.
    uint baseHeightIndex;
    uint layerHeightIndex[TERRAIN_SPLAT_LAYERS];
    /// #6196 — height-blend transition band; 0 = height blending off.
    float heightBlendBand;
    /// #6197 — near-camera parallax occlusion. Global settings, broadcast to
    /// every live tile like the band above. `parallaxDepth` is the relief
    /// amplitude in world units (0 = parallax off); the effect fades to plain
    /// shading between `parallaxFadeStart` and `parallaxFadeEnd` world units
    /// from the camera. The fourth lane is `lodSoftenEnd`: the camera distance
    /// at which the full-detail terrain is fully softened toward the distant
    /// LOD look (starting at half of it); 0 = off. It took the last named pad of
    /// the 224-byte stride; TSD-4.3's patch lanes below grew the record to 240.
    float parallaxDepth;
    float parallaxFadeStart;
    float parallaxFadeEnd;
    float lodSoftenEnd;
    /// TSD-4.3 (design §3.6) — the displacement patches' geometry, broadcast like the parallax
    /// settings above by `VulkanContext::set_terrain_patch_frame`. A patch draw carries
    /// `patchAmplitude · fade` of the relief in its vertices, where `fade` falls from 1 to 0 between
    /// `patchFadeStart` and `patchFadeEnd` world units from the camera, so its parallax march covers
    /// only the rest. `patchParallax` > 0.5 keeps that march on patch draws; 0 is the
    /// `terrain.patches parallax off` A/B. Appended at the tail: stride 224 → 240.
    float patchAmplitude;
    float patchFadeStart;
    float patchFadeEnd;
    float patchParallax;
};

/// #6196 — height-based blending between terrain layers.
///
/// The splat lanes compose in order, `mix(prev, layer, w)` from the BTXT
/// base up, so layer `i`'s share of the surface is `w_i · Π_{j>i}(1 − w_j)`
/// and the base keeps the rest; the shares sum to 1. Among the layers that
/// have a height map (base included), their combined share `S` is
/// re-divided by height: each scores `h_i + share_i / S`, and every layer
/// within `band` of the top score keeps a part proportional to how far into
/// the band it reaches (Mishkinis, "Advanced Terrain Texture Splatting",
/// Game Developer, 2013 — the same `height + weight` rule, generalised from
/// two layers to N). Where two layers overlap, the higher one wins inside a
/// transition band; a narrower `band` is a sharper transition.
///
/// The result is written back as ordered-`mix` weights, so every consumer
/// of the splat lanes — diffuse, specular, normal, the blade ground-colour
/// coupling — composes the same blended surface with its existing loop.
/// Layers without a height map keep their exact share, and a fragment with
/// fewer than two height-mapped layers in play is returned untouched, so
/// terrain without height maps shades exactly as before. The only texture
/// reads are the height samples themselves, one per weighted height-mapped
/// layer, with the caller's uniform-flow gradients (#4016).
///
/// #6202 — a lane's blended share must vanish *with* its painted share.
/// Raw `h_i + share_i / S` tends to `h_i`, not to nothing, as `share_i → 0`,
/// so a high texel kept up to half the surface right up to the lane's
/// zero-weight edge and then dropped out — a hard seam along the triangle
/// edge at every painted rim (and at a fully opaque layer's core, where the
/// base's share reaches 0). Each lane's height and in-band part are scaled
/// by `fade_i = min(share_i / (S · k), 1)`, `k = 1 / (TERRAIN_SPLAT_LAYERS
/// + 1)`: the largest `k` that keeps the biggest-share lane (≥ `S / lanes`)
/// unfaded, so the band always holds a full-strength lane and a vanishing
/// lane can never be the only survivor. Lanes at or above `S · k` blend
/// exactly as before.
///
/// #6197 — the share/height arithmetic lives in `byroTerrainBlendCore`, shared
/// by `byroTerrainHeightBlend` (rewrites the splat weights) and
/// `byroTerrainBlendedHeight` (the surface height the terrain parallax marches
/// against), so the two can never describe different surfaces. The core
/// returns the share-weighted height of the height-mapped lanes
/// (`Σ share·h / Σ share`), or -1 when no lane has a height map. `blended`
/// reports whether the band rule re-divided the shares: it is false with
/// fewer than two height-mapped lanes or with `band <= 0`, and `share` then
/// holds the painted shares, so a height query still works with blending off.
float byroTerrainBlendCore(
    vec4 splat0,
    vec4 splat1,
    GpuTerrainTile tile,
    vec2 uv,
    vec2 dx,
    vec2 dy,
    float band,
    out float share[TERRAIN_SPLAT_LAYERS + 1u],
    out bool blended
) {
    float w[TERRAIN_SPLAT_LAYERS];
    for (uint i = 0u; i < TERRAIN_SPLAT_LAYERS; ++i) {
        w[i] = i < TERRAIN_SPLAT_LANES_PER_WORD
            ? splat0[i]
            : splat1[i - TERRAIN_SPLAT_LANES_PER_WORD];
    }
    // Shares, top lane down; `share[TERRAIN_SPLAT_LAYERS]` is the base.
    float rest = 1.0;
    for (int i = int(TERRAIN_SPLAT_LAYERS) - 1; i >= 0; --i) {
        float wi = clamp(w[i], 0.0, 1.0);
        share[i] = wi * rest;
        rest *= 1.0 - wi;
    }
    share[TERRAIN_SPLAT_LAYERS] = rest;
    blended = false;

    float hRaw[TERRAIN_SPLAT_LAYERS + 1u];
    bool hasHeight[TERRAIN_SPLAT_LAYERS + 1u];
    uint count = 0u;
    float heightShare = 0.0;
    for (uint i = 0u; i <= TERRAIN_SPLAT_LAYERS; ++i) {
        uint idx = i < TERRAIN_SPLAT_LAYERS ? tile.layerHeightIndex[i] : tile.baseHeightIndex;
        hasHeight[i] = idx != 0u && share[i] > 0.0;
        hRaw[i] = 0.0;
        if (hasHeight[i]) {
            hRaw[i] = textureGrad(textures[nonuniformEXT(idx)], uv, dx, dy).r;
            heightShare += share[i];
            ++count;
        }
    }
    if (count == 0u || heightShare <= 1.0e-6) {
        return -1.0;
    }
    if (count < 2u || band <= 0.0) {
        float painted = 0.0;
        for (uint i = 0u; i <= TERRAIN_SPLAT_LAYERS; ++i) {
            if (hasHeight[i]) {
                painted += share[i] * hRaw[i];
            }
        }
        return painted / heightShare;
    }

    float h[TERRAIN_SPLAT_LAYERS + 1u];
    const float fadeShare = 1.0 / float(TERRAIN_SPLAT_LAYERS + 1u);
    float fade[TERRAIN_SPLAT_LAYERS + 1u];
    float top = -1.0e30;
    for (uint i = 0u; i <= TERRAIN_SPLAT_LAYERS; ++i) {
        fade[i] = 0.0;
        h[i] = 0.0;
        if (hasHeight[i]) {
            float rel = share[i] / heightShare;
            fade[i] = min(rel / fadeShare, 1.0);
            h[i] = fade[i] * hRaw[i] + rel;
            top = max(top, h[i]);
        }
    }
    float floorScore = top - max(band, 1.0e-4);
    float total = 0.0;
    for (uint i = 0u; i <= TERRAIN_SPLAT_LAYERS; ++i) {
        if (hasHeight[i]) {
            h[i] = fade[i] * max(h[i] - floorScore, 0.0);
            total += h[i];
        }
    }
    // `total > 0`: the top scorer sits `band` above the floor, and it is
    // never faded below 1/10 (a faded lane outscores the unfaded
    // biggest-share lane only with `fade ≥ 0.1`).
    float blendedHeight = 0.0;
    for (uint i = 0u; i <= TERRAIN_SPLAT_LAYERS; ++i) {
        if (hasHeight[i]) {
            share[i] = heightShare * h[i] / total;
            blendedHeight += share[i] * hRaw[i];
        }
    }
    blended = true;
    return blendedHeight / heightShare;
}

void byroTerrainHeightBlend(
    inout vec4 splat0,
    inout vec4 splat1,
    GpuTerrainTile tile,
    vec2 uv,
    vec2 dx,
    vec2 dy,
    float band
) {
    float share[TERRAIN_SPLAT_LAYERS + 1u];
    bool blended;
    byroTerrainBlendCore(splat0, splat1, tile, uv, dx, dy, band, share, blended);
    if (!blended) {
        return;
    }

    // Back to ordered-`mix` weights: lane i keeps share_i of what has been
    // composed up to and including it, so the existing loops reproduce the
    // blended shares exactly.
    float composed = share[TERRAIN_SPLAT_LAYERS];
    for (uint i = 0u; i < TERRAIN_SPLAT_LAYERS; ++i) {
        composed += share[i];
        float wi = composed > 1.0e-6 ? share[i] / composed : 0.0;
        if (i < TERRAIN_SPLAT_LANES_PER_WORD) {
            splat0[i] = wi;
        } else {
            splat1[i - TERRAIN_SPLAT_LANES_PER_WORD] = wi;
        }
    }
}

/// #6197 — the blended layer height at `uv` for the fragment's painted
/// weights, in `[0, 1]`, or -1 where no layer in play has a height map.
/// `band <= 0` queries the painted (un-blended) surface.
float byroTerrainBlendedHeight(
    vec4 splat0,
    vec4 splat1,
    GpuTerrainTile tile,
    vec2 uv,
    vec2 dx,
    vec2 dy,
    float band
) {
    float share[TERRAIN_SPLAT_LAYERS + 1u];
    bool blended;
    return byroTerrainBlendCore(splat0, splat1, tile, uv, dx, dy, band, share, blended);
}

/// The terrain's splat colour composition (#4907): start from the BTXT base
/// RGB and mix each splat layer over it by its own weight, in layer order,
/// skipping zero-weight and unused lanes — the exact loop `triangle.frag`'s
/// terrain branch has always run. Shared by that branch and
/// `groundcover_blade.frag`'s ground-colour coupling so the colour under a
/// blade is the colour the terrain shows, by construction rather than by
/// two copies staying in step.
vec3 byroTerrainSplatAlbedo(
    vec3 baseRGB,
    vec2 uv,
    vec2 dx,
    vec2 dy,
    vec4 splat0,
    vec4 splat1,
    GpuTerrainTile tile
) {
    vec3 albedo = baseRGB;
    for (uint i = 0u; i < TERRAIN_SPLAT_LAYERS; ++i) {
        float w = i < TERRAIN_SPLAT_LANES_PER_WORD
            ? splat0[i]
            : splat1[i - TERRAIN_SPLAT_LANES_PER_WORD];
        if (w <= 0.0) continue;
        uint layerIdx = tile.layerDiffuseIndex[i];
        if (layerIdx == 0u) continue; // layer slot unused
        vec3 layer = textureGrad(
            textures[nonuniformEXT(layerIdx)], uv, dx, dy).rgb;
        albedo = mix(albedo, layer, w);
    }
    return albedo;
}

#endif // BYRO_TERRAIN_TILE_GLSL
