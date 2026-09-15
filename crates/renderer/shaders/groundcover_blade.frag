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
layout(location = 10) flat in uint vBladeSeed;
layout(location = 11) flat in uint vLodTier;
layout(location = 12) flat in uint vFrameSerial;
layout(location = 13) in float vLodMidWeight;
layout(location = 14) in vec2 vCardUv;
layout(location = 15) flat in uint vCard;
layout(location = 16) in float vCardWeight;

layout(location = 0) out vec4 outColor;
layout(location = 2) out vec2 outMotion;
layout(location = 5) out vec4 outAlbedo;
layout(location = 6) out float outFsrReactive;
layout(location = 7) out float outFsrTransparency;

#include "include/bindings.glsl"
#include "include/math_common.glsl"
#include "include/ray_origin.glsl"
#include "include/pbr.glsl"
#include "include/ray_hit.glsl"
#include "include/shadow_common.glsl"
#include "include/shadow_transport.glsl"
#include "include/lighting.glsl"
#include "include/groundcover_light.glsl"
#include "include/blue_noise.glsl"

layout(std430, set = 2, binding = 6) readonly buffer GcSpeciesBuffer {
    GroundCoverSpecies gcSpecies[];
};

void main() {
    // The two geometric streams cover complementary samples of one 8×8 blue
    // noise tile. The tile rotates per blade and per replayable frame serial,
    // preventing a fixed screen-door pattern while preserving exact coverage
    // at either end of the projected-pixel transition.
    // `blueNoiseRankAt` reads only the tile bits, so the per-blade offsets need
    // no pre-masking: the low bits select the X shift, the next ones the Y.
    ivec2 tilePixel = ivec2(
        uvec2(gl_FragCoord.xy)
        + uvec2(vBladeSeed, vBladeSeed >> 3u)
        + vFrameSerial * uvec2(5u, 3u)
    );
    float rank = blueNoiseRankAt(tilePixel);
    float keep = vLodTier == 0u
        ? 1.0 - vLodMidWeight
        : vLodMidWeight * (1.0 - vCardWeight);
    if (vCard == 0u && rank >= keep) {
        discard;
    }
    GroundCoverSpecies sp = gcSpecies[vSpecies];
    vec4 cardSample = vec4(1.0);
    if (vCard != 0u) {
        if (sp.cardAtlas.x == 0u) {
            discard;
        }
        vec2 atlasSize = vec2(textureSize(textures[nonuniformEXT(sp.cardAtlas.x)], 0));
        float atlasY = (float(vSpecies * GROUNDCOVER_DETAIL_ATLAS_EDGE)
            + vCardUv.y * float(GROUNDCOVER_DETAIL_ATLAS_EDGE - 1u) + 0.5) / atlasSize.y;
        cardSample = texture(textures[nonuniformEXT(sp.cardAtlas.x)], vec2(vCardUv.x, atlasY));
        if (cardSample.a <= 0.0) {
            discard;
        }
    }

    // §7's colour gradient: base → tip. Post-#4057 this is the plant's own
    // colour variation and nothing else — the dark base it used to bake as a
    // stand-in for self-shadowing is now §12.1's job, and leaving both in
    // would compound them until the base went black.
    vec3 albedo = vCard != 0u
        ? cardSample.rgb
        : mix(sp.baseColour.rgb, sp.tipColour.rgb, vBladeT);
    // Per-blade colour jitter. A field of identically-coloured blades reads as
    // one object with a texture on it rather than as many plants.
    albedo *= mix(
        GROUNDCOVER_COLOUR_JITTER_MIN,
        GROUNDCOVER_COLOUR_JITTER_MAX,
        vColourJitter);
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

    // Match triangle.frag's current-UV → previous-UV convention. The two
    // positions are unjittered; TAA jitter is only applied to gl_Position.
    vec2 currNDC = vCurrClipPos.xy / vCurrClipPos.w;
    vec2 prevNDC = vPrevClipPos.xy / vPrevClipPos.w;
    outMotion = (currNDC - prevNDC) * 0.5;

    // Outside the transition this remains ordinary opaque, depth-writing
    // geometry. Inside it the stochastic complementary coverage can change
    // composition at a pixel even when blade motion is correct, so expose a
    // bounded reactive term rather than poisoning the whole grass field.
    float midTransition = 4.0 * vLodMidWeight * (1.0 - vLodMidWeight);
    float cardTransition = 4.0 * vCardWeight * (1.0 - vCardWeight);
    // Cards have a stable alpha silhouette, but the Tier-1 coverage that
    // hands off to them is stochastic.  Flag either crossover (not the whole
    // field) so the upscaler avoids treating a changing composition as motion.
    outFsrReactive = 0.9 * max(midTransition, cardTransition);
    outFsrTransparency = 0.0;
}
