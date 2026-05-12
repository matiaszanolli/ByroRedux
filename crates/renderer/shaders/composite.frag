#version 450
#extension GL_EXT_nonuniform_qualifier : require

// Composite pass: combines the main render pass outputs into the final
// tone-mapped swapchain image, with sky rendering for background pixels.
//
//   For geometry pixels (depth < 1.0):
//     final = direct + indirect
//     output = aces(final * exposure)
//
//   For sky pixels (depth == 1.0, exterior only):
//     Reconstruct world-space view direction from screen UV + inv_view_proj.
//     Compute sky gradient (horizon → zenith) + cloud layer + sun disc.
//     output = aces(sky * exposure)
//
// Direct and indirect are separated so SVGF can filter only the noisy
// indirect signal without blurring crisp direct-light shadows. The
// albedo attachment lets us re-multiply after demodulation.

layout(set = 0, binding = 0) uniform sampler2D hdrTex;       // direct light
layout(set = 0, binding = 1) uniform sampler2D indirectTex;  // demodulated indirect
layout(set = 0, binding = 2) uniform sampler2D albedoTex;    // surface albedo (multiplies demodulated indirect)
layout(set = 0, binding = 3) uniform CompositeParams {
    vec4 fog_color;      // xyz = RGB, w = enabled (1.0 = yes)
    // x = near, y = far, z = XCLL cubic-fog clip distance (0 = no
    // curve), w = XCLL cubic-fog falloff exponent (0 = no curve).
    // When z > 0 && w > 0, use pow(dist / z, w) instead of the linear
    // (dist - near) / (far - near) blend. See #865 / FNV-D3-NEW-06.
    vec4 fog_params;
    vec4 depth_params;   // x = is_exterior (1.0 = sky enabled), y = exposure, z/w = unused
    vec4 sky_zenith;     // xyz = zenith color (linear RGB), w = sun_size (cos threshold)
    vec4 sky_horizon;    // xyz = horizon color (linear RGB), w = unused
    vec4 sky_lower;      // xyz = below-horizon ground tint (WTHR SKY_LOWER), w = unused (#541)
    vec4 sun_dir;        // xyz = sun direction (world-space, normalized), w = sun_intensity
    vec4 sun_color;      // xyz = sun disc color (linear RGB), w = CLMT FNAM sun sprite idx (floatBitsToUint; 0 = procedural disc)
    vec4 cloud_params;   // x=scroll_u, y=scroll_v, z=tile_scale (0=disabled), w=texture_idx(uintBits)
    vec4 cloud_params_1; // cloud layer 1 (WTHR CNAM) — same packing as cloud_params
    vec4 cloud_params_2; // cloud layer 2 (WTHR ANAM) — same packing (M33.1)
    vec4 cloud_params_3; // cloud layer 3 (WTHR BNAM) — same packing (M33.1)
    vec4 camera_pos;     // xyz = world position, w = unused. Fog distance origin (#428).
    mat4 inv_view_proj;  // inverse view-projection for ray reconstruction
    // Underwater post-FX: xyz = water deep-color tint (linear RGB),
    // w = camera depth below water surface (world units, >0 = under).
    // The shader's final branch mixes `combined` toward `underwater.xyz`
    // by a depth-driven extinction when `underwater.w > 0`. When `w == 0`
    // the branch is a no-op.
    vec4 underwater;
} params;
layout(set = 0, binding = 4) uniform sampler2D depthTex;     // depth buffer
layout(set = 0, binding = 5) uniform usampler2D causticTex;  // R32_UINT caustic accumulator (#321)
// M55 Phase 4: volumetric froxel volume (RGBA16F).
//   rgb = inscatter radiance (HDR linear, pre-tone-map)
//   a   = extinction coefficient (1 / m)
// Sampled per-fragment with a 32-step ray-march for the in-scatter +
// transmittance modulation applied to `combined` before ACES.
layout(set = 0, binding = 6) uniform sampler3D volumetricFroxel;
// M58: bloom output mip 0 (B10G11R11_UFLOAT, half-screen). Sampled
// at full screen resolution with bilinear hardware filtering;
// added to `combined` before ACES per Frostbite §8.
layout(set = 0, binding = 7) uniform sampler2D bloomTex;

// Set 1: bindless texture array from TextureRegistry — shared with the
// main geometry pipeline. Used here to sample WTHR cloud textures by index.
layout(set = 1, binding = 0) uniform sampler2D textures[];
// CAUSTIC_FIXED_SCALE — divide the uint accumulator by this to
// recover luminance. The compute side (`caustic_splat.comp`) reads
// the same value via the `causticTune.x` UBO uploaded by
// `caustic.rs` every frame, so the splat→accumulator path is
// auto-synced. Composite reads from the storage image directly
// without going through that UBO, so the literal is duplicated
// here. Pinned in lockstep with `caustic.rs::CAUSTIC_FIXED_SCALE`
// by the unit test
// `caustic_fixed_scale_sync_tests::composite_frag_caustic_fixed_scale_matches_rust_const`
// — bumping the Rust const fails the test until this literal is
// updated. See #667 / SH-12.
const float CAUSTIC_FIXED_SCALE = 65536.0;

// M55 — volumetric far plane. Must match `volumetrics::DEFAULT_VOLUME_FAR`
// (Rust side) and the `params.volume_extent.x` value passed to the
// injection compute pass; otherwise the slice→view-distance mapping
// disagrees and fog appears compressed or stretched. With Phase 3
// pre-integration the per-fragment cost is now ONE sampler3D tap, so
// no step-count dial is needed here — quality scales with the froxel
// resolution and dt set on the host.
const float VOLUME_FAR = 200.0;

// M58 — bloom contribution coefficient. 0.15 (≈4× the Frostbite
// SIGGRAPH 2015 default of 0.04) compensates for Bethesda content
// being LDR-authored: emissive surfaces sit in the 0–1 monitor-
// space range rather than HDR cd/m², so a Frostbite-default
// intensity reads as essentially-invisible. Hand-tuned downward
// from 0.20 on Prospector saloon (sun-lit windows + chandelier
// globes were producing halos that bled too far across walls);
// 0.15 keeps emissives obviously bloomed without flooding dim
// surfaces. Pinned in lockstep with `bloom::DEFAULT_BLOOM_INTENSITY`
// (Rust side); update both at once. See feedback memo "Color Space
// — Not sRGB" for why we don't HDR-boost emissives globally instead.
const float BLOOM_INTENSITY = 0.15;

layout(location = 0) in vec2 fragUV;
layout(location = 0) out vec4 outColor;

vec3 aces(vec3 x) {
    const float a = 2.51;
    const float b = 0.03;
    const float c = 2.43;
    const float d = 0.59;
    const float e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}

// Reconstruct world-space view direction from screen UV and inverse VP.
vec3 screen_to_world_dir(vec2 uv) {
    // NDC: x [-1,1], y [-1,1], z = 1.0 (far plane in Vulkan reversed-Z? no,
    // we use standard depth where 1.0 = far).
    vec2 ndc = uv * 2.0 - 1.0;
    vec4 clip = vec4(ndc, 1.0, 1.0);
    vec4 world = params.inv_view_proj * clip;
    // #926 / REN-D10-NEW-06 — defensive guard against a singular
    // projection matrix producing `world.w == 0`. Real perspective
    // cameras keep w strictly positive at the far plane, so the
    // clamp is a no-op on the hot path; the floor only fires for
    // degenerate matrices (zero-FOV, behind-camera ray, etc.) and
    // keeps the result finite instead of producing NaN/inf that
    // would propagate into the sky / aerial-perspective branches.
    float w = max(abs(world.w), 1e-6);
    return normalize(world.xyz / w);
}

// Compute sky color from view direction.
vec3 compute_sky(vec3 dir) {
    vec3 zenith = params.sky_zenith.xyz;
    vec3 horizon = params.sky_horizon.xyz;
    float sun_size = params.sky_zenith.w;
    float sun_intensity = params.sun_dir.w;
    // Host promises `params.sun_dir.xyz` is already normalised
    // (per `SkyParams::sun_direction` doc — "normalized, world-space
    // Y-up"). The pre-fix `normalize(...)` per fragment was wasted
    // compute on the composite fullscreen draw. See REN-D10-NEW-07
    // (audit 2026-05-09). If the host contract ever weakens, the
    // dot-product / sun-disc maths below silently degrade — re-add
    // the normalize at that point and fix the SkyParams comment.
    vec3 sun_direction = params.sun_dir.xyz;
    vec3 sun_col = params.sun_color.xyz;

    // Elevation: 0 at horizon, 1 at zenith. Clamp negative (below horizon)
    // to a slightly darkened horizon for a ground-plane approximation.
    float elevation = dir.y;

    // Sky gradient: smooth blend from horizon to zenith.
    // Use a non-linear curve so the horizon band is wider (more natural).
    float t = clamp(elevation, 0.0, 1.0);
    t = sqrt(t); // widen horizon band
    vec3 sky = mix(horizon, zenith, t);

    // Below-horizon darkening: ground approximation (not a ground
    // plane, just a colour fade toward the WTHR-authored ground
    // tint). #541 — pre-fix this branch faked the ground colour as
    // `horizon * 0.3` and dropped the authored `SKY_LOWER` group
    // entirely. The real Sky-Lower colour ships per-TOD on every
    // exterior WTHR, so the night ground is appropriately dark and
    // the sunrise / sunset fringe inherits the warm authored tint
    // without compositor-side tweaking.
    if (elevation < 0.0) {
        float below = clamp(-elevation * 3.0, 0.0, 1.0);
        sky = mix(horizon, params.sky_lower.xyz, below);
    }

    // Cloud layer 0 (from WTHR cloud_textures[0]).
    //
    // Project the upper hemisphere onto an infinite horizontal plane
    // overhead: uv = (dir.xz / dir.y) × tile_scale. This gives perspective-
    // correct foreshortening at low elevations (clouds at the horizon look
    // stretched and tile densely, directly overhead they look large and
    // slow-moving) without needing a real dome mesh.
    //
    // cloud_params.z == 0 disables the sample so the checkerboard fallback
    // handle is never read on cells without WTHR cloud data.
    // Analytic mip LOD: as elevation → 0 the UV magnitude grows as 1/elevation,
    // so the mip should rise by log2(1/elevation). Factor 0.5 keeps the transition
    // gentler than the raw log2 slope. textureLod bypasses the driver's dFdx/dFdy
    // estimate, which would see a 100x–500x UV discontinuity across horizon-fade
    // quads and snap to mip-0 (per-texel aliasing visible in #730). SH-13.
    float cloud_lod = log2(1.0 / max(elevation, 0.05)) * 0.5;

    float tile_scale = params.cloud_params.z;
    if (tile_scale > 0.0 && elevation > 0.0) {
        uint cloud_idx = floatBitsToUint(params.cloud_params.w);
        // max() floor guards against the overhead singularity (dir.y → 0)
        // producing NaN UVs. 0.05 matches ~3° of remaining foreshortening.
        vec2 uv = dir.xz / max(elevation, 0.05) * tile_scale
                + params.cloud_params.xy;
        vec4 cloud = textureLod(textures[nonuniformEXT(cloud_idx)], uv, cloud_lod);
        // Fade clouds out at the horizon so the projection singularity
        // doesn't produce an ugly stretched band right at elevation=0.
        float horizon_fade = smoothstep(0.0, 0.12, elevation);
        sky = mix(sky, cloud.rgb, cloud.a * horizon_fade);
    }

    // Cloud layer 1 (WTHR CNAM — higher-altitude deck, opposite drift direction).
    // tile_scale_1 == 0.0 when no CNAM texture was loaded; the branch is
    // skipped entirely so the bindless array is never sampled with index 0.
    float tile_scale_1 = params.cloud_params_1.z;
    if (tile_scale_1 > 0.0 && elevation > 0.0) {
        uint cloud_idx_1 = floatBitsToUint(params.cloud_params_1.w);
        vec2 uv_1 = dir.xz / max(elevation, 0.05) * tile_scale_1
                  + params.cloud_params_1.xy;
        vec4 cloud_1 = textureLod(textures[nonuniformEXT(cloud_idx_1)], uv_1, cloud_lod);
        float horizon_fade_1 = smoothstep(0.0, 0.12, elevation);
        sky = mix(sky, cloud_1.rgb, cloud_1.a * horizon_fade_1);
    }

    // Cloud layer 2 (WTHR ANAM, M33.1) — same projection / fade as layer 1.
    float tile_scale_2 = params.cloud_params_2.z;
    if (tile_scale_2 > 0.0 && elevation > 0.0) {
        uint cloud_idx_2 = floatBitsToUint(params.cloud_params_2.w);
        vec2 uv_2 = dir.xz / max(elevation, 0.05) * tile_scale_2
                  + params.cloud_params_2.xy;
        vec4 cloud_2 = textureLod(textures[nonuniformEXT(cloud_idx_2)], uv_2, cloud_lod);
        float horizon_fade_2 = smoothstep(0.0, 0.12, elevation);
        sky = mix(sky, cloud_2.rgb, cloud_2.a * horizon_fade_2);
    }

    // Cloud layer 3 (WTHR BNAM, M33.1) — same projection / fade as layer 1.
    float tile_scale_3 = params.cloud_params_3.z;
    if (tile_scale_3 > 0.0 && elevation > 0.0) {
        uint cloud_idx_3 = floatBitsToUint(params.cloud_params_3.w);
        vec2 uv_3 = dir.xz / max(elevation, 0.05) * tile_scale_3
                  + params.cloud_params_3.xy;
        vec4 cloud_3 = textureLod(textures[nonuniformEXT(cloud_idx_3)], uv_3, cloud_lod);
        float horizon_fade_3 = smoothstep(0.0, 0.12, elevation);
        sky = mix(sky, cloud_3.rgb, cloud_3.a * horizon_fade_3);
    }

    // Sun disc: bright circular spot with a soft edge.
    // sun_size is cos(half-angle) of the disc — lower = wider.
    // Use a smooth transition band outside the core to avoid hard edges
    // from screen-space direction reconstruction precision.
    //
    // `elevation > 0.0` matches the cloud-layer gate convention above
    // and stops the disc painting over the below-horizon ground tint
    // at sunset/sunrise (the sky-lower mix at L107 produces a "ground"
    // colour that the disc would otherwise overwrite). #800.
    float cos_angle = dot(dir, sun_direction);
    float sun_edge_start = sun_size - 0.002; // soft outer fringe
    if (cos_angle > sun_edge_start && elevation > 0.0) {
        float t = (cos_angle - sun_edge_start) / (1.0 - sun_edge_start);
        t = smoothstep(0.0, 1.0, t);
        // Core is bright, edge fades smoothly.
        float core = smoothstep(sun_size, 1.0, cos_angle);
        float disc = mix(t * 0.5, 1.0, core);

        // #478 — when CLMT FNAM ships a sun sprite (non-zero index),
        // sample it within the disc and multiply by sun_col; otherwise
        // fall back to the flat sun_col (pre-fix behaviour). The UV
        // comes from projecting `dir` onto a tangent plane
        // perpendicular to `sun_direction` and scaling so the texture
        // fills the disc radius.
        uint sun_tex_idx = floatBitsToUint(params.sun_color.w);
        vec3 disc_color = sun_col;
        if (sun_tex_idx != 0u) {
            // Local 2D basis on the plane perpendicular to sun_direction.
            vec3 up_world = abs(sun_direction.y) < 0.99
                ? vec3(0.0, 1.0, 0.0)
                : vec3(0.0, 0.0, 1.0);
            vec3 tangent = normalize(cross(up_world, sun_direction));
            vec3 bitangent = cross(sun_direction, tangent);

            // Disc radius in tangent-plane units: `sqrt(1 - sun_size^2)`
            // matches the angular half-width on the unit sphere. We
            // normalise by this so the sprite fills the disc exactly.
            float disc_r = sqrt(max(0.0, 1.0 - sun_size * sun_size));
            vec2 uv = vec2(dot(dir, tangent), dot(dir, bitangent)) / max(disc_r, 1e-4);
            uv = uv * 0.5 + 0.5;
            vec4 sprite = texture(textures[nonuniformEXT(sun_tex_idx)], uv);
            disc_color = sun_col * sprite.rgb;
        }

        sky += disc_color * sun_intensity * disc;
    }

    // Sun glow: soft radial halo around the sun.
    //
    // #799 — multiply by `sun_intensity` so the halo fades with the
    // disc through the day/night ramp. Pre-fix the disc faded
    // correctly (line 222) but the halo stayed at constant 0.15 *
    // sun_col, so a WTHR with non-zero `SKY_SUN[NIGHT]` (e.g.
    // Skyrim's MoonShadow) painted a faint warm halo at midnight.
    //
    // Falloff tightened to `pow(., 8)` × 0.10 (was `pow(., 4)` × 0.15)
    // on the Markarth 2026-05-10 probe. The wider 4-power halo at
    // `sun_intensity = 4` was adding +0.28 RGB to the sky ~33° off the
    // sun direction; ACES tonemap then pushed the entire visible
    // upper hemisphere to pale-white in any view including the sun's
    // half of the sky. The `pow(., 8)` curve concentrates the halo
    // to ~15° around the disc — preserves the bright sun region
    // without washing the rest of the sky.
    float glow = max(cos_angle, 0.0);
    glow = pow(glow, 8.0);
    sky += sun_col * glow * 0.10 * sun_intensity;

    return sky;
}

void main() {
    vec4 direct4 = texture(hdrTex, fragUV);
    vec3 direct = direct4.rgb;

    float depth = texture(depthTex, fragUV).r;
    bool is_sky = (depth >= 0.9999) && (params.depth_params.x > 0.5);

    if (is_sky) {
        // Sky pixel: reconstruct view direction and compute sky color.
        vec3 dir = screen_to_world_dir(fragUV);
        vec3 sky = compute_sky(dir);

        float exposure = params.depth_params.y;  // host-set; default 0.85 (DEN-10)
        // Pass `direct4.a` through (mirroring the geometry branch at
        // line 279) so the alpha-blend marker bit `DEN-6 / #676`
        // preserves through TAA stays consistent across both sky and
        // geometry pixels. Sky pixels by construction don't have a
        // glass surface in front of them today, so today's
        // `direct4.a` on a sky-branch fragment is zero — but a future
        // decal pass / transparent UI / lens-flare feature that asks
        // "is this swapchain pixel sky?" via swapchain alpha would
        // see an asymmetric "1.0 = sky, anything else = geometry"
        // contract that's harder to reason about than the symmetric
        // "alpha is the marker bit, branch on it the same way."
        // DEN-11.
        vec3 sky_tonemapped = aces(sky * exposure);
        // Underwater post-FX on the sky branch — the camera looking
        // up through the water surface should also see the deep tint
        // when submerged. Same model as the geometry branch (see the
        // matching block at the end of the geometry path).
        if (params.underwater.w > 0.0) {
            float extinction = clamp(1.0 - exp(-params.underwater.w / 120.0), 0.0, 0.85);
            vec3 underwater_tonemap = aces(params.underwater.xyz * exposure);
            sky_tonemapped = mix(sky_tonemapped, underwater_tonemap, extinction);
        }
        outColor = vec4(sky_tonemapped, direct4.a);
    } else {
        // Geometry pixel: combine direct + (indirect × albedo) and tone map.
        // The shader wrote lighting-only indirect (no local albedo) so
        // SVGF operates on a texture-free signal; multiply here to
        // re-apply surface color. See #268.
        vec3 indirect = texture(indirectTex, fragUV).rgb;
        vec3 albedo = texture(albedoTex, fragUV).rgb;

        // Caustic (#321): refracted-light scatter from the caustic_splat
        // pass. Stored as fixed-point luminance in a R32_UINT accumulator;
        // decode here and add a warm-white contribution scaled by the
        // receiver's own albedo so colored surfaces pick up the caustic
        // with their own tint.
        uint causticRaw = texelFetch(causticTex, ivec2(gl_FragCoord.xy), 0).r;
        float causticLum = float(causticRaw) / CAUSTIC_FIXED_SCALE;
        vec3 caustic = albedo * causticLum;

        vec3 combined = direct + indirect * albedo + caustic;

        // M55 Phase 3 — volumetric modulation via single sampler3D tap.
        // The volumetric pipeline pre-integrates `(∫inscatter, T_cum)`
        // along the view ray per froxel column in a compute pass; here
        // we just look up the value at the fragment's depth slice and
        // modulate `combined`. Done in HDR-linear (pre-ACES) per
        // Frostbite §5.3 so the tone-mapper sees the inscattered
        // radiance and the scene together.
        //
        // The volumetric volume is screen-space in xy and depth-slice
        // in z under linear distribution, so the sample coordinate is
        // (fragUV, worldDist / VOLUME_FAR). No NDC projection needed,
        // no per-step loop.
        if (depth < 0.9999) {
            vec2 ndc_xy = fragUV * 2.0 - 1.0;
            vec4 clip = vec4(ndc_xy, depth, 1.0);
            vec4 world = params.inv_view_proj * clip;
            vec3 worldPos = world.xyz / world.w;
            float worldDist = length(worldPos - params.camera_pos.xyz);
            // clamp(0, 1 - eps): max-slice texel still samples within
            // the volume rather than the CLAMP_TO_EDGE neighbour, which
            // would over-extrapolate transmittance for fragments past
            // the volume far plane.
            float slice = clamp(worldDist / VOLUME_FAR, 0.0, 0.9999);
            vec4 vol = texture(volumetricFroxel, vec3(fragUV, slice));
            // vol.rgb = ∫inscatter accumulated 0..slice (HDR-linear)
            // vol.a   = cumulative transmittance through 0..slice
            // M55 Phase 2c volumetric contribution gated OFF on
            // 2026-05-09. Diagnostic confirmed the per-froxel single-
            // shadow-ray approach produces ~8-pixel-wide vertical
            // bands on bright surfaces (1-bit visibility per froxel
            // column → bilinear sampling can't recover sub-froxel
            // detail). Stripes were clearly visible on lantern
            // bodies in Prospector; disabling the read restored
            // smooth shading. Re-enable when M-LIGHT (multi-tap
            // shadow rays + temporal stability) lands — see Tier 8
            // row in ROADMAP.md.
            //
            // ── Lockstep host-side gate (#928) ──────────────────
            // The Rust-side `volumetrics::VOLUMETRIC_OUTPUT_CONSUMED`
            // const gates `vol.dispatch()` in `draw.rs::draw_frame`.
            // While both this `* 0.0` and that const are paired,
            // the volumetric pipeline does NO GPU work per frame —
            // recovers ~10–20 ms/frame estimated. When M-LIGHT v2
            // ships, flip BOTH together: remove the `* 0.0` here and
            // set `VOLUMETRIC_OUTPUT_CONSUMED = true`.
            //
            // The `vol.rgb * 0.0` keeps the texture sample alive so
            // SPIR-V reflection (validate_set_layout) still sees
            // binding 6 referenced from this shader; removing the
            // sample entirely would require also dropping the host-
            // side binding declaration and is more churn than the
            // gate is worth.
            combined += vol.rgb * 0.0;
        }

        // M58 — bloom add. Sampled with bilinear from mip 0 of the
        // bloom up-pyramid (half-screen resolution; hardware filter
        // upscales to full screen for free). Added in HDR-linear
        // (pre-ACES) so the tone-mapper compresses scene + bloom
        // together — bright surfaces' glow doesn't clip independently
        // of the surface. `fragUV` in [0,1]² works directly against
        // the half-res bloom view.
        vec3 bloom = texture(bloomTex, fragUV).rgb;
        combined += bloom * BLOOM_INTENSITY;

        float exposure = params.depth_params.y;  // host-set; default 0.85 (DEN-10)

        // Tone-map the unfogged combined HDR to display space FIRST,
        // then apply fog as a display-space mix. Pre-#784 the fog
        // was mixed into HDR-linear `combined` and then both went
        // through ACES together — XCLL-authored `fog_color` values
        // (raw monitor-space floats per `feedback_color_space.md`,
        // typically 0.05-0.4 on interior cells) blended in linear
        // HDR appear perceptually amplified once the result is
        // tone-mapped, producing a visible yellow / sepia distance
        // wash on distant interior surfaces. Display-space mix
        // matches the perceptual intent of the authored fog values
        // and preserves #428's SVGF-coherence goal (fog applied at
        // composite, not in the geometry pass) since SVGF reads
        // un-fogged HDR upstream regardless of where the mix lands
        // on the post-tone-map side.
        vec3 tonemapped = aces(combined * exposure);

        // Aerial-perspective fog — exterior cells only (Markarth probe
        // 2026-05-10). Pre-fix the geometry branch shipped with NO
        // distance fade because M55 Phase 3 (2026-05-09) removed the
        // legacy display-space fog mix on the assumption that the
        // volumetric pipeline at line 368 above would take over. That
        // pipeline's per-froxel single-shadow-ray approach produced
        // ~8-pixel vertical bands on bright surfaces and was gated
        // OFF (`vol.rgb * 0.0`) pending M-LIGHT v2. Net effect from
        // 2026-05-09 to 2026-05-10: no atmospheric perspective on any
        // exterior — distant cliffs read as harsh black silhouettes
        // against the bright sky (Markarth screenshot, looking up
        // between the canyon walls). Restoring the legacy mix here
        // covers the gap until M-LIGHT v2 lands; when it does, drop
        // this branch in lockstep with flipping the `* 0.0` on
        // `vol.rgb` and flipping `VOLUMETRIC_OUTPUT_CONSUMED = true`
        // in `draw.rs::draw_frame`.
        //
        // The mix targets the SKY COLOUR along the view direction (not
        // the flat `params.fog_color`), so the haze pulls each pixel
        // toward whatever the sky behind it would have painted — real
        // aerial perspective. `fog_color` stays in the UBO for the
        // future REGN-driven volumetric density tint (M55 Phase 6).
        //
        // Display-space mix (post-tonemap) per #784: HDR-linear fog
        // values authored in raw monitor space (`feedback_color_space.md`)
        // get perceptually amplified through ACES if mixed pre-tonemap,
        // producing a yellow / sepia distance wash on warm-fog cells.
        // Display-space mix lands closer to the perceptual intent of
        // the authored values.
        if (params.depth_params.x > 0.5 && depth < 0.9999) {
            float fog_near = params.fog_params.x;
            float fog_far  = params.fog_params.y;
            float fog_clip  = params.fog_params.z;
            float fog_power = params.fog_params.w;
            if (fog_far > fog_near) {
                // worldDist was computed above in the volumetric
                // branch but only inside that `if (depth < 0.9999)`
                // scope — recompute here so the geometry path
                // doesn't depend on volumetric branch ordering.
                vec2 ndc_xy_fog = fragUV * 2.0 - 1.0;
                vec4 clip_fog = vec4(ndc_xy_fog, depth, 1.0);
                vec4 world_fog = params.inv_view_proj * clip_fog;
                vec3 worldPos_fog = world_fog.xyz / world_fog.w;
                float worldDist = length(worldPos_fog - params.camera_pos.xyz);
                // #865 / FNV-D3-NEW-06 — when XCLL authors a cubic-fog
                // curve (FNV+ 40-byte tail), use `pow(dist / clip, power)`
                // instead of the linear ramp. Vanilla FNV interiors
                // (Doc Mitchell's House, Goodsprings Source Pump)
                // author both fields to shape close-camera fog more
                // gently than the linear blend allows. Falls through
                // to the linear ramp when either field is 0 (un-authored).
                float fog_t;
                if (fog_clip > 0.0 && fog_power > 0.0) {
                    fog_t = clamp(pow(worldDist / fog_clip, fog_power), 0.0, 1.0);
                } else {
                    fog_t = clamp(
                        (worldDist - fog_near) / (fog_far - fog_near),
                        0.0, 1.0
                    );
                }
                // Sky colour along the view direction — same shader
                // function the sky branch uses, so the haze matches
                // what the geometry occludes.
                vec3 viewDir = screen_to_world_dir(fragUV);
                vec3 skyHaze = compute_sky(viewDir);
                vec3 tonemappedHaze = aces(skyHaze * exposure);
                tonemapped = mix(tonemapped, tonemappedHaze, fog_t);
            }
        }

        // ── Underwater post-FX ────────────────────────────────────────
        //
        // Drives a depth-based tint toward the active water material's
        // `deep_color` when the camera is submerged. The CPU sets
        // `params.underwater.xyz` to the deep-color tint and
        // `params.underwater.w` to the camera's depth below the
        // surface; `w == 0` keeps the branch a no-op.
        //
        // Extinction model: Beer-Lambert with a 1/120-wu falloff,
        // chosen so a 60-wu submersion (head just under) sits at
        // ~40% tint and a 240-wu submersion saturates at ~85% tint.
        // The cap at 0.85 prevents full black when the player dives
        // arbitrarily deep — the scene stays legible.
        if (params.underwater.w > 0.0) {
            float extinction = 1.0 - exp(-params.underwater.w / 120.0);
            extinction = clamp(extinction, 0.0, 0.85);
            // Tone-map the underwater colour with the same exposure
            // so the mix doesn't drag the scene into HDR space where
            // ACES would re-bend it. The underwater colour is
            // already authored in display-linear (per the
            // `feedback_color_space.md` policy on Gamebryo colour
            // bytes), so an ACES pass on a tone-mapped target is
            // visually correct here.
            vec3 underwater_tonemap = aces(params.underwater.xyz * exposure);
            tonemapped = mix(tonemapped, underwater_tonemap, extinction);
        }

        outColor = vec4(tonemapped, direct4.a);
    }
}
