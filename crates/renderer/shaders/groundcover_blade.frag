#version 460
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : require
#extension GL_GOOGLE_include_directive : require
// `GpuInstance.skinnedVertexAddress` / `SkinnedVertexRef` in bindings.glsl
// need buffer_reference support, same as water.frag.
#extension GL_EXT_buffer_reference : require
#extension GL_ARB_gpu_shader_int64 : require

// EXAL ground cover — blade shading (§5 Stage 1, §12.1/12.2/12.5/12.6;
// #4055 / #4057).
//
// §5 decided the ray-tracing boundary: grass is **receive-only**. It
// rasterizes in the main geometry pass and traces the existing shadow ray like
// any other fragment, so it is correctly lit and shadowed by the world. It
// contributes nothing back — no BLAS, no TLAS entry, no ray budget. Blades as
// TLAS instances are not viable and the design records why: at
// MAX_INSTANCES = 262144 a single mid-density chunk would exhaust the scene
// budget on its own.
//
// ## What this writes, and what it deliberately does not
//
// The blade vertex shader evaluates the same wind and displacement model at
// the current and preceding shared clock sample, so this opaque, depth-writing
// geometry writes a real velocity just like the ordinary opaque path. It is
// therefore not a blanket FSR reactive/transparency exception.
//
// Compile with:
//   glslangValidator -V -Icrates/renderer/shaders groundcover_blade.frag -o groundcover_blade.frag.spv

#include "include/shader_constants.glsl"
#include "include/groundcover_scene.glsl"

layout(location = 0) in vec3 vWorldPos;
layout(location = 1) in vec3 vWorldNormal;
layout(location = 2) in float vBladeT;
layout(location = 3) in float vDGround;
layout(location = 4) flat in uint vSpecies;
layout(location = 5) in float vColourJitter;
layout(location = 6) flat in float vBladeHeight;
layout(location = 7) in float vBladeWidth;
layout(location = 8) in vec4 vCurrClipPos;
layout(location = 9) in vec4 vPrevClipPos;
// §12.3 ground-colour coupling (#4056): the blade-base terrain sample the
// vertex shader captured — splat weights, the rebuilt diffuse UV and the
// terrain-tile slot naming the layer diffuse textures. See the blend below.
layout(location = 17) in vec2 vTerrainUv;
layout(location = 18) in vec4 vTerrainSplat0;
layout(location = 19) in vec4 vTerrainSplat1;
layout(location = 20) flat in uint vTerrainTileSlot;
layout(location = 21) flat in uint vTerrainBaseIndex;
layout(location = 22) in float vClumpTone;

layout(location = 0) out vec4 outColor;
layout(location = 2) out vec2 outMotion;
layout(location = 5) out vec4 outAlbedo;
layout(location = 6) out float outFsrReactive;
layout(location = 7) out float outFsrTransparency;

#include "include/bindings.glsl"
#include "include/render_debug_view.glsl"
#include "include/math_common.glsl"
#include "include/ray_origin.glsl"
#include "include/pbr.glsl"
#include "include/ray_hit.glsl"
#include "include/shadow_common.glsl"
#include "include/shadow_transport.glsl"
#include "include/lighting.glsl"
#include "include/groundcover_light.glsl"

layout(std430, set = 2, binding = 6) readonly buffer GcSpeciesBuffer {
    GroundCoverSpecies gcSpecies[];
};

void main() {
    // No LOD discard: the segment tier is chosen per chunk and geomorphed in the
    // vertex shader, so every fragment that arrives here belongs to the one
    // representation of its blade (`groundcover_lod.glsl`).
    GroundCoverSpecies sp = gcSpecies[vSpecies];

    // §7's colour gradient: base → tip. Post-#4057 this is the plant's own
    // colour variation and nothing else — the dark base it used to bake as a
    // stand-in for self-shadowing is now §12.1's job, and leaving both in
    // would compound them until the base went black.
    vec3 albedo = mix(sp.baseColour.rgb, sp.tipColour.rgb, vBladeT);

    // ── §12.3 ground-colour coupling (#4056) ────────────────────────────
    //
    // Blend the species gradient toward the terrain's own albedo at the
    // blade base, by the per-species coupling weight riding `tipColour.a`:
    // a meadow on red clay is not the green of one on peat, and when grass
    // and ground disagree the grass reads as a layer resting *on* the
    // terrain rather than as part of it. The weight fades to zero at the
    // tip — the coupling roots the plant, it does not dye the whole blade.
    //
    // The layer loop mirrors `triangle.frag`'s splat blend, with two
    // differences that are both load-bearing. The `continue` branches below
    // branch on per-fragment varyings, and a 2×2 pixel quad routinely
    // straddles two blades of thin geometry — so nothing here is
    // quad-uniform and `textureGrad` with explicitly captured varying
    // derivatives is the only sound fetch (#4016's class of hazard; the
    // derivatives are captured unconditionally, before any branch, for the
    // same reason `triangle.frag` captures its splat gradients outside
    // `terrainSplatActive`). And the blend is the terrain's own composition
    // — the BTXT base under the painted layers (#4907) — falling back to the
    // painted-layer average only where the base is unresolved. A cell with a
    // base and no ATXT paint has no terrain tile; its blades couple to the
    // base alone, so no root-colour step appears at its border (#5174).
    vec2 terrainUvDx = dFdx(vTerrainUv);
    vec2 terrainUvDy = dFdy(vTerrainUv);
    // The terrain's own colour at the blade root, when it could be resolved;
    // the luminance anchor below reads it once the branches have run.
    vec3 rootGroundAlbedo = vec3(0.0);
    bool haveRootGround = false;
    if (vTerrainTileSlot != GROUNDCOVER_NO_TERRAIN_TILE) {
        GpuTerrainTile groundTile = terrainTiles[nonuniformEXT(vTerrainTileSlot)];
        // #4907 — the ground colour is the TERRAIN's composition, not a
        // renormalised average of the overlays: the BTXT base under an
        // ordered `mix(base, layer, w)` per lane, via the same
        // `byroTerrainSplatAlbedo` helper `triangle.frag` shades with. The
        // old `sum(w*layer)/sum(w)` read a 0.05 overlay as 100% of the
        // ground colour wherever the base carried the visible 95%, with a
        // discontinuity where the paint thinned to nothing.
        if (groundTile.baseDiffuseIndex != 0u) {
            vec3 baseRGB = textureGrad(
                textures[nonuniformEXT(groundTile.baseDiffuseIndex)],
                vTerrainUv, terrainUvDx, terrainUvDy).rgb;
            vec3 groundAlbedo = byroTerrainSplatAlbedo(
                baseRGB, vTerrainUv, terrainUvDx, terrainUvDy,
                vTerrainSplat0, vTerrainSplat1, groundTile);
            float coupling = clamp(sp.tipColour.a, 0.0, 1.0) * (1.0 - vBladeT);
            albedo = mix(albedo, groundAlbedo, coupling);
            rootGroundAlbedo = groundAlbedo;
            haveRootGround = true;
        } else {
            // Base unresolved (BTXT missing and no default land): the
            // painted-layer average is the best remaining estimate.
            vec3 groundAlbedo = vec3(0.0);
            float weightSum = 0.0;
            for (uint i = 0u; i < TERRAIN_SPLAT_LAYERS; ++i) {
                float w = i < TERRAIN_SPLAT_LANES_PER_WORD
                    ? vTerrainSplat0[i]
                    : vTerrainSplat1[i - TERRAIN_SPLAT_LANES_PER_WORD];
                if (w <= 0.0) continue;
                uint layerIdx = groundTile.layerDiffuseIndex[i];
                if (layerIdx == 0u) continue; // layer slot unused
                groundAlbedo += w * textureGrad(
                    textures[nonuniformEXT(layerIdx)],
                    vTerrainUv, terrainUvDx, terrainUvDy).rgb;
                weightSum += w;
            }
            if (weightSum > 0.0) {
                float coupling = clamp(sp.tipColour.a, 0.0, 1.0) * (1.0 - vBladeT);
                albedo = mix(albedo, groundAlbedo / weightSum, coupling);
                rootGroundAlbedo = groundAlbedo / weightSum;
                haveRootGround = true;
            }
        }
    } else if (vTerrainBaseIndex != 0u) {
        // #5174 — no tile: the terrain draws its base alone (no ATXT paint,
        // or the tile table was full), so the base is the ground colour.
        vec3 groundAlbedo = textureGrad(
            textures[nonuniformEXT(vTerrainBaseIndex)],
            vTerrainUv, terrainUvDx, terrainUvDy).rgb;
        float coupling = clamp(sp.tipColour.a, 0.0, 1.0) * (1.0 - vBladeT);
        albedo = mix(albedo, groundAlbedo, coupling);
        rootGroundAlbedo = groundAlbedo;
        haveRootGround = true;
    }

    // ── Luminance anchor to the terrain ─────────────────────────────────
    //
    // The hue is the species'; the brightness is the ground's. Without this the
    // palette's linear albedo (0.3-0.5) is lit by the SAME terrain GI as dirt
    // that is 0.05-0.15, so every blade returns several times the terrain's
    // radiance — in sun and in a rock's shadow alike, which is the "self-lit"
    // look (the direct terms are not the cause: removing them leaves it).
    // See `GROUNDCOVER_BASE_LUMA_RATIO`.
    if (haveRootGround) {
        // What the terrain shows at the root is its splat texture mixed toward
        // the species grass-detail colour by the SAME `d_ground` the blade
        // carries (`triangle.frag`'s detail layer), not the texture alone: in a
        // dense sward the ground is palette-coloured, and anchoring to the bare
        // soil texture there turned the blades black.
        vec3 paletteMid = 0.5 * (sp.baseColour.rgb + sp.tipColour.rgb);
        vec3 terrainAlbedo = mix(rootGroundAlbedo, paletteMid, clamp(vDGround, 0.0, 1.0));
        float groundLuma = dot(terrainAlbedo, LUMA_REC709);
        float bladeLuma = max(dot(albedo, LUMA_REC709), 1.0e-4);
        float wanted = groundLuma * mix(
            GROUNDCOVER_BASE_LUMA_RATIO, GROUNDCOVER_TIP_LUMA_RATIO, vBladeT);
        albedo *= min(wanted / bladeLuma, GROUNDCOVER_LUMA_ANCHOR_MAX_GAIN);
    }

    // Per-blade colour jitter. A field of identically-coloured blades reads as
    // one object with a texture on it rather than as many plants.
    albedo *= mix(
        GROUNDCOVER_COLOUR_JITTER_MIN,
        GROUNDCOVER_COLOUR_JITTER_MAX,
        vColourJitter);
    // Per-clump tone: a warm <-> cool shift shared by every blade of the clump.
    float clumpShift = GROUNDCOVER_CLUMP_TINT_VARIATION * (2.0 * vClumpTone - 1.0);
    albedo *= vec3(1.0 + clumpShift, 1.0, 1.0 - clumpShift);
    vec3 transmissionColour = sp.transmissionSheen.rgb;
    float sheen = max(sp.transmissionSheen.a, 0.0);

    // Two-sided: a blade is a ribbon with no inside, so the shading normal has
    // to follow whichever face the camera is looking at.
    vec3 V = normalize(cameraPos.xyz - vWorldPos);
    vec3 N = normalize(vWorldNormal);
    if (dot(N, V) < 0.0) {
        N = -N;
    }

    // ── The canopy this fragment sits inside (§12.5) ────────────────────
    //
    // §12.5 treats ground cover as a participating slab of thickness equal to
    // the local blade height. A fragment at parametric height `t` therefore
    // has `height * (1 - t)` units of canopy above it: a blade deep in the
    // sward is shadowed by what stands over it while one at the edge — or the
    // tip of any blade — is not, and *that* is what gives a patch interior
    // depth instead of uniform brightness.
    //
    // `vDGround`, never `d_draw` (§3): the view-faded density would make the
    // interior of a meadow brighten as the camera retreats from it.
    float canopyAbove = max(vBladeHeight * (1.0 - vBladeT), 0.0);
    // §12.2's optical thickness — the blade's own tapered width. Kept
    // separate from the canopy depth above: one is how much grass is between
    // this fragment and the sun, the other is how much *leaf* is.
    float bladeTransmittance = byroGcBladeTransmittance(vBladeWidth);

    vec3 lit = vec3(0.0);
    vec3 transmitted = vec3(0.0);
    float sheenTotal = 0.0;
    // Directional lights only. Grass is ankle height across a whole
    // worldspace: the sun is what lights it, and iterating the clustered
    // point/spot set per blade fragment would spend the entire ground-cover
    // budget on lights whose radius rarely reaches the ground plane. Local
    // lights reaching grass is a refinement, not the base case.
    uint count = min(lightCount, MAX_LIGHTS);
    for (uint i = 0u; i < count; ++i) {
        if (lights[i].color_type.w < 1.5) {
            continue;
        }
        vec3 L = normalize(lights[i].direction_angle.xyz);
        float NdotL = dot(N, L);

        // §12.6 front-lit / §12.2 back-lit. These are the two halves of one
        // surface and either alone leaves the meadow flat from one direction,
        // which is why they are computed together rather than in two passes.
        float diffuse = max(NdotL, 0.0);
        float sheenLobe = byroGcSheenLobe(N, V, L, sheen);
        float lobe = byroGcTransmissionLobe(N, V, L);

        // **The ordering §12.2 exists to keep straight.** Transmission is NOT
        // gated on `max(N·L, 0)`: the geometric self-shadow — the near face of
        // a lit blade — is exactly the case that should glow, and clamping it
        // away is the shape a first implementation reaches for and the reason
        // backlit grass so often comes out flat. The traced shadow below is a
        // different matter and does apply to both: a blade shadowed by a
        // distant rock receives nothing and must not glow.
        if (diffuse <= 0.0 && lobe <= 0.0 && sheenLobe <= 0.0) {
            continue;
        }

        vec3 shadow = vec3(1.0);
        if (sceneFlags.x > 0.5) {
            shadow = traceLightTransmittance(
                i,
                offsetRayOrigin(vWorldPos, N),
                L,
                DIRECTIONAL_SHADOW_TRACE_DISTANCE);
        }
        // Ground cover has no TLAS presence (§5), so that ray cannot hit
        // another blade; it reports the *world's* occluders only, and the
        // canopy's own contribution has to come from the closed form.
        //
        // `L` points from the surface toward the light, so `L.y` is the
        // cosine of the light's angle from vertical.
        float canopy = byroGcCanopyTransmittance(vDGround, canopyAbove, L.y);
        // A directional light's radiance is its colour alone. `params.x` is
        // the point/spot falloff exponent, which `collect_lights` writes as
        // 0.0 for every directional source and `shadowableLightRadiance`
        // never reads on its directional arm — scaling by it here zeroed the
        // sun on every blade (#4291).
        vec3 incoming = lights[i].color_type.rgb * shadow * canopy;

        lit += incoming * (diffuse + sheenLobe);
        transmitted += incoming * (lobe * bladeTransmittance);
    }

    // §12.1 — contact occlusion. The same slab as §12.5, integrated over the
    // sky instead of along the sun, so ambient dies toward the base of a dense
    // sward and does not in a sparse one. The pre-#4057 stand-in here was a
    // fixed `mix(0.45, 1.0, t)`, which shaded the sparse edge of a patch as
    // dark at the base as the middle of a meadow — when the whole point is
    // that the middle is dark *because* it is the middle.
    float skyVisibility = byroGcSkyOcclusion(vDGround, canopyAbove);
    // §12.6's ambient half: the blade's environment is the sky, and a
    // Fresnel-weighted sky tint at grazing angles is the whole of it.
    vec3 sheenAmbient = exteriorSkyDiffuseOr(N, sceneFlags.yzw)
        * (byroGcSheenAmbient(N, V, sheen) * skyVisibility);

    // Diffuse ambient is NOT added here. This pass leaves the raw-indirect
    // attachment holding the ground's demodulated GI underneath the blade, and
    // composite reassembles `direct + indirect * albedo` per pixel. Writing the
    // blade's own albedo makes that the blade's ambient — lit by the same GI
    // the ground receives, which at ankle height is very nearly its own. A
    // flat `sceneFlags` ambient on top of it counted the sky twice and paired
    // the blade with the *terrain's* albedo, which is what made it glow.
    // §12.1's occlusion rides the albedo, since composite multiplies it in.
    vec3 colour = albedo * (lit + sheenAmbient) + transmissionColour * transmitted;
    outColor = vec4(colour, 1.0);
    outAlbedo = vec4(albedo * skyVisibility, 1.0);
    // #4979 — procedural blades implement no structured view; under any raw
    // one they recede like every other non-participant. Motion and the FSR
    // masks below stay live so temporal state is unaffected.
    if (renderDebugSurfaceRecedes(renderDebug.x)) {
        outColor = vec4(RENDER_DEBUG_NON_PARTICIPANT_GREY, 1.0);
        outAlbedo = vec4(1.0);
    }

    // Match triangle.frag's current-UV → previous-UV convention. The two
    // positions are unjittered; TAA jitter is only applied to gl_Position.
    vec2 currNDC = vCurrClipPos.xy / vCurrClipPos.w;
    vec2 prevNDC = vPrevClipPos.xy / vPrevClipPos.w;
    outMotion = (currNDC - prevNDC) * 0.5;

    // Ordinary opaque, depth-writing geometry at every distance: the segment
    // tiers geomorph into one another instead of cross-fading, so there is no
    // changing composition for the upscaler to mistake for motion.
    outFsrReactive = 0.0;
    outFsrTransparency = 0.0;
}
