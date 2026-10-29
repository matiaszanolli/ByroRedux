#version 460
#extension GL_GOOGLE_include_directive : require
#extension GL_EXT_nonuniform_qualifier : require
// REN-2026-07-28-02 / #2219 — GpuInstance grew a uint64_t member; this
// mirror never dereferences it, but the type must be recognized to keep
// the struct byte-for-byte identical to the other 4 declaration sites.
#extension GL_EXT_buffer_reference : require
#extension GL_ARB_gpu_shader_int64 : require

#include "include/shader_constants.glsl"

// ── Water surface vertex shader ───────────────────────────────────────
//
// Drives:
//   • Calm lakes / pools             (`WaterKind::Calm`)
//   • Rivers and canals              (`WaterKind::River`)
//   • Rapids / whitewater            (`WaterKind::Rapids`)
//   • Waterfall sheets               (`WaterKind::Waterfall`)
//
// The water mesh is authored as a flat quad in mesh-local space. The
// raster path applies a bounded two-wave vertical displacement below;
// water remains excluded from the TLAS, so no per-frame BLAS rebuild is
// required. The fragment path adds higher-frequency normal detail.
//
// Inputs reuse the engine `Vertex` layout exactly so the renderer
// can share its global vertex SSBO + index buffer with the rest of
// the world. Bone indices / weights / splat weights are unused by
// water draws (they're authored zero on the water quad meshes); we
// keep the attribute slots wired so `triangle.vert`'s VAO is
// reusable without a second `VertexInputState`.

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec4 inColor;       // unused on water — kept for VAO compat
layout(location = 2) in vec3 inNormal;
layout(location = 3) in vec2 inUV;
layout(location = 4) in uvec4 inBoneIndices; // unused
layout(location = 5) in vec4  inBoneWeights; // unused
layout(location = 6) in vec4 inSplat0;       // unused
layout(location = 7) in vec4 inSplat1;       // unused
layout(location = 8) in vec4 inTangent;      // xyz = tangent, w = bitangent sign

// ── Per-instance SSBO (shared with triangle pipeline) ─────────────────
// We only consume `model`; the rest of the GpuInstance fields are
// not driven by the water material path (which lives in push
// constants — see water.frag). Layout must match the Rust struct
// at `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`
// byte-for-byte — the 160-byte invariant is pinned by
// `gpu_instance_is_160_bytes_std430_compatible` at
// `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`.
// (The struct grew to 128 B when #2219 added `skinnedVertexAddress`, then
// to 160 B when #3231 added the morph-target address/count fields; this
// mirror's body tracks it, only the comment kept quoting the old size.)
// Path moved from `vulkan/instance.rs` during the Session-34 split (#1187).
struct GpuInstance {
    mat4 model;
    uint textureIndex;
    uint boneOffset;
    uint vertexOffset;
    uint indexOffset;
    uint vertexCount;
    uint flags;
    uint materialId;
    float ior;             // offset 92 — per-draw optical IOR (read by caustic_splat.comp)
    float avgAlbedoR;
    float avgAlbedoG;
    float avgAlbedoB;
    uint surfaceId;
    uint64_t skinnedVertexAddress; // offset 112 — #2219, unused here
    uvec2 _reserved;               // offset 120 -> total 128
    uint64_t morphDeltaAddress;  // offset 128 — #3231, unused here
    uint64_t morphWeightAddress; // offset 136 — #3231, unused here
    uint morphTargetCount;       // offset 144 — #3231, unused here
    // Deliberately three scalar uints, NOT uvec3 — see gpu_types.rs.
    uint _reserved2a; // offset 148
    uint _reserved2b; // offset 152
    uint _reserved2c; // offset 156 -> total 160
};

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

layout(set = 1, binding = 1) uniform CameraUBO {
    mat4 viewProj;
    mat4 prevViewProj;
    mat4 invViewProj;
    vec4 cameraPos;
    vec4 sceneFlags;
    vec4 screen;
    vec4 fog;
    vec4 jitter;
    vec4 skyTint;
    vec4 sunDirection;
    vec4 dofParams;      // x = aperture half-radius (0.0 = pinhole), y = focus_dist, z = atten knee frac, w = history mode (0 moving, 1 parked, 2 parked + scene-static, 3 parked + rig-geometry-static under flicker; restir_history_mode).
    vec4 renderOrigin;   // #markarth-precision — xyz = camera-relative render origin; add to worldPos_rel for the absolute world position. w = FSR one-frame-reset flag (NOT padding — #2164/L-10).
    uvec4 renderDebug;   // x = structured RENDER_DEBUG_* mode; y = RT LOD scale; z = LOD telemetry; w = packed weather surface (low 16 wetness, high 16 snow).
    // #3323 — EXTERIOR TOD zenith colour, live even on interior cells.
    // Consumed only by triangle.frag's window-portal escape; declared
    // here to keep the five CameraUBO mirrors byte-identical
    // (feedback_shader_struct_sync.md).
    vec4 exteriorSkyTint; // xyz = live exterior zenith colour, w = sky-cubemap
                          // ready flag (1.0 once `sky_cube.comp` has baked into
                          // set 1 / binding 20 this frame; 0.0 if the bake is
                          // absent, e.g. it failed to initialise under VRAM
                          // pressure — consumers must fall back, see raytrace.glsl)
};

// Set 2 binding 1 is shared with water.frag. Vertex visibility is enabled
// so authored amplitude/frequency can affect the silhouette as well as the
// fragment normal, while the indexed record keeps the push block compact.
struct WaterParams {
    vec4 timing;
    vec4 flow;
    vec4 shallow;
    vec4 deep;
    vec4 scroll;
    // xy = authored/flow scroll for the third normal layer; zw = underwater
    // fog near/far.
    vec4 scroll_c;
    vec4 tune;
    vec4 misc;
    vec4 tint_reflect;
    uvec4 noise_indices;
    // x = authored NAM4 UV scale; yzw = authored NAM2/3/4 amplitude scales.
    vec4 detail;
    // x = authored Skyrim noise-falloff distance; y = Blend Normals gate;
    // z = Starfield surface roughness; w = Skyrim Specular Radius.
    vec4 noise_falloff;
    // xyz = shallow/deep/surface-effect normal falloff multipliers.
    vec4 normal_falloff;
    // x = displacement starting size, y = radial falloff, z = dampener.
    vec4 displacement;
    // x/y/z/w = reflection/refraction/normal/specular depth weights.
    vec4 depth;
    // x/y/z/w = refraction/local-specular/reflection/sun-specular controls.
    vec4 effects;
    // x/y/z = Starfield per-channel extinction coefficients; w = precipitation *
    // authored rain-response (0..4). Not a free slot — same trap as
    // VolumetricsParams.render_origin.w / GpuCamera.render_origin.w.
    // Keep these trailing slots in lockstep with water.frag and
    // GpuWaterParams so every array element uses the same 368-byte std430
    // stride when the vertex shader selects a water material by index.
    vec4 absorption;
    // Starfield phytoplankton, sediment, yellow matter, oceanness.
    vec4 concentration;
    // xy = transient ripple center, z = intensity, w = radius.
    vec4 ripple;
    // rgb = authored underwater tint, a = underwater fog amount.
    vec4 underwater;
    // x/y = shallow/deep alpha, z/w = shallow/deep distance thresholds.
    vec4 alpha;
    // xy = authored mesh-water UV offset; z = flow-map index bit-cast;
    // w = authored flow-map scale.
    vec4 uv_offset;
    // x = FO4+/Creation-2 WATR Depth Amount; y = WaterNormalEncoding
    // (0 tangent normal, 1 offset noise); zw reserved.
    vec4 optical;
};
layout(std430, set = 2, binding = 1) readonly buffer WaterParamsBlock {
    WaterParams params[];
} waterParams;

// Bare 4-byte selector — a trailing uvec3 would std430-pad this block to
// 28 B, past the pipeline layout's declared 16 B push range (#4510 /
// VUID-layout-10069). The remaining range width is host-side padding only.
layout(push_constant) uniform WaterDrawPush {
    uint waterIndex;
} drawPush;

layout(location = 0) out vec3 vWorldPos;
layout(location = 1) out vec3 vWorldNormal;
layout(location = 2) out vec3 vWorldTangent;
layout(location = 3) out float vWorldBitangentSign;
// Mesh-bound BGSM flow maps use authored mesh UVs; cell WATR surfaces leave
// the flow-map index at the u32::MAX sentinel and continue using world UVs.
layout(location = 4) out vec2 vUV;
// The bed under this vertex, for water.frag: x = depth below the surface
// (world units; 1e6 when the vertex carries no bed data), yz = the unit
// shoreward direction, w = 1 when bed data is present. vCrest is the swell's
// crest phase (-1 trough .. +1 crest) for breaker foam.
layout(location = 5) out vec4 vShore;
layout(location = 6) out float vCrest;

void main() {
    GpuInstance inst = instances[gl_InstanceIndex];

    vec4 worldPos = inst.model * vec4(inPosition, 1.0);
    WaterParams water = waterParams.params[drawPush.waterIndex];
    uint kind = uint(water.timing.y + 0.5);
    // Flat cell planes use the authored wave pair. Waterfall meshes are
    // already artist-oriented sheets and keep their authored geometry.
    vec3 localNormal = inNormal;
    vShore = vec4(1.0e6, 0.0, 0.0, 0.0);
    vCrest = 0.0;
    if (kind != WATER_WATERFALL) {
        vec3 absolutePos = worldPos.xyz + renderOrigin.xyz;
        // The bed under this vertex (`cell_loader/water.rs::bed_vertex_color`).
        // Only the exact sentinel alpha carries bed data; every other water
        // mesh (interior, LOD, placed NIF) keeps its authored vertex colour.
        bool bedValid = abs(inColor.a - WATER_BED_VALID_ALPHA) < WATER_BED_VALID_ALPHA_TOL;
        float bedDepth = bedValid ? inColor.r * WATER_BED_DEPTH_MAX : 1.0e6;
        vec2 uphill = bedValid ? inColor.gb * 2.0 - 1.0 : vec2(0.0);
        float uphillLen = length(uphill);
        vec2 shoreDir = uphillLen > WATER_SHORE_MIN_UPHILL_LEN ? uphill / uphillLen : vec2(0.0);
        vShore = vec4(bedDepth, shoreDir, bedValid ? 1.0 : 0.0);
        // Waves feel the bed: below WATER_SHORE_REFRACT_DEPTH the crests turn
        // toward the shore (refraction), so a swell arriving at an angle ends
        // up running parallel to the beach.
        float refraction = 0.0;
        if (bedValid && uphillLen > WATER_SHORE_MIN_UPHILL_LEN) {
            refraction = (1.0 - smoothstep(0.25 * WATER_SHORE_REFRACT_DEPTH,
                                           WATER_SHORE_REFRACT_DEPTH, bedDepth))
                * WATER_SHORE_REFRACT_MAX;
        }
        vec2 dirA = water.scroll.xy;
        vec2 dirB = water.scroll.zw;
        if (length(dirA) < 1e-5) dirA = WATER_WAVE_FALLBACK_DIR_A;
        if (length(dirB) < 1e-5) dirB = WATER_WAVE_FALLBACK_DIR_B;
        dirA = normalize(dirA);
        dirB = normalize(dirB);
        if (refraction > 0.0) {
            dirA = normalize(mix(dirA, shoreDir, refraction * WATER_SHORE_RIPPLE_TURN_SHARE));
            dirB = normalize(mix(dirB, shoreDir, refraction * WATER_SHORE_RIPPLE_TURN_SHARE));
        }
        float spatialA = max(abs(water.tune.x), WATER_WAVE_SPATIAL_FLOOR_A);
        float spatialB = max(abs(water.tune.y), WATER_WAVE_SPATIAL_FLOOR_B);
        float frequency = max(abs(water.misc.y), 0.0);
        // The CPU adds the live Weather/WindField velocity to both scroll
        // vectors (the same field that bends SpeedTree objects). Feed that
        // velocity into the low-frequency displacement too, otherwise wind
        // only moves the fragment normal map while the silhouette keeps a
        // fixed-time rhythm. The sentinel defaults reproduce a rate of one;
        // clamp the response so malformed or storm-strength records cannot
        // turn a surface into a numerically unstable choppy sheet. Every
        // WATER_WAVE_* tuning value comes from the generated header, owned by
        // core beside the CPU crest sampler that mirrors this block (#5582).
        float waveRateA = clamp(length(water.scroll.xy) / WATER_WAVE_DEFAULT_SCROLL_A,
                                WATER_WAVE_RATE_MIN, WATER_WAVE_RATE_MAX);
        float waveRateB = clamp(length(water.scroll.zw) / WATER_WAVE_DEFAULT_SCROLL_B,
                                WATER_WAVE_RATE_MIN, WATER_WAVE_RATE_MAX);
        // #4728 — both dominant layers travel ALONG their scroll-derived
        // direction (sin(kx - wt) moves +x), matching the canonical
        // `WaterFlow` contract; wave A used the opposite time sign and ran
        // its crest upwind/upstream against the physics current. The CPU
        // mirror in `crates/physics/src/water.rs` moves in lockstep.
        float phaseA = dot(absolutePos.xz, dirA) * spatialA * 6.2831853
                     - water.timing.x * frequency * waveRateA * 6.2831853;
        float phaseB = dot(absolutePos.xz, dirB) * spatialB * 6.2831853
                     - water.timing.x * frequency * waveRateB
                       * (6.2831853 * WATER_WAVE_PHASE_B_RATE);
        // Author data can contain corrupt/extreme values; keep displacement
        // finite and below a conservative shoreline-safe bound.
        float amplitude = clamp(water.tune.w, 0.0, WATER_WAVE_AMPLITUDE_MAX);
        // A wave cannot stand taller than the water under it: the same
        // breaking limit as the swell, which also takes the displacement to
        // zero at the waterline where the surface meets the shore.
        amplitude = min(amplitude, WATER_SWELL_BREAK_RATIO * bedDepth);
        float slopeA = spatialA * 6.2831853;
        float slopeB = spatialB * 6.2831853;
        float waveA = sin(phaseA) * WATER_WAVE_MIX_A;
        float waveB = sin(phaseB) * WATER_WAVE_MIX_B;
        worldPos.y += amplitude * (waveA + waveB);
        vCrest = waveA + waveB;
        // The wind-driven swell (`WaterSwell` in core, the same wave the CPU
        // crest sampler evaluates): optical.z = open-water crest height,
        // optical.w = heading. It shoals over a rising bed (Green's law),
        // turns toward the shore with the refraction above and is capped by
        // breaking.
        float swellHeight = 0.0;
        float swellDx = 0.0;
        float swellDz = 0.0;
        if (water.optical.z > 0.0) {
            vec2 swellDir = vec2(cos(water.optical.w), sin(water.optical.w));
            // Only a swell travelling toward the shore turns toward it: one
            // running offshore would mix to a near-zero vector at half share,
            // and normalising that flips the heading from vertex to vertex.
            if (refraction > 0.0) {
                float toward = smoothstep(0.0, WATER_SWELL_SHORE_TOWARD_RAMP, dot(swellDir, shoreDir));
                swellDir = normalize(mix(swellDir, shoreDir, refraction * toward));
            }
            float swellK = 6.2831853 / WATER_SWELL_WAVELENGTH_BU;
            float swellW = sqrt(WATER_SWELL_GRAVITY_BU_PER_S2 * swellK);
            float swellPhase = dot(absolutePos.xz, swellDir) * swellK - water.timing.x * swellW;
            float shoal = bedDepth >= WATER_SWELL_SHOAL_REF_DEPTH_BU
                ? 1.0
                : min(pow(WATER_SWELL_SHOAL_REF_DEPTH_BU / max(bedDepth, 1.0), 0.25),
                      WATER_SWELL_SHOAL_GAIN_MAX);
            float swellAmp = min(water.optical.z * shoal, WATER_SWELL_BREAK_RATIO * bedDepth);
            swellHeight = swellAmp * sin(swellPhase);
            swellDx = swellAmp * cos(swellPhase) * swellDir.x * swellK;
            swellDz = swellAmp * cos(swellPhase) * swellDir.y * swellK;
            vCrest = sin(swellPhase);
        }
        worldPos.y += swellHeight;
        // Keep the geometric normal consistent with the displaced surface.
        // The fragment shader adds high-frequency tangent detail later; this
        // low-frequency gradient is what makes silhouettes, Fresnel, and the
        // direct-sun lobe agree with the vertex-wave shape.
        float dHeightDx = swellDx + amplitude * (
            cos(phaseA) * dirA.x * slopeA * WATER_WAVE_MIX_A
            + cos(phaseB) * dirB.x * slopeB * WATER_WAVE_MIX_B
        );
        float dHeightDz = swellDz + amplitude * (
            cos(phaseA) * dirA.y * slopeA * WATER_WAVE_MIX_A
            + cos(phaseB) * dirB.y * slopeB * WATER_WAVE_MIX_B
        );
        localNormal = normalize(vec3(-dHeightDx, 1.0, -dHeightDz));
    }
    // #markarth-precision — `inst.model` is rebased by the render origin (the
    // water plane reuses the same instance buffer as opaques), so `worldPos`
    // is relative; clip is computed from the relative viewProj below. Output
    // the ABSOLUTE world position for water.frag's lighting / RT reflection +
    // refraction (the TLAS is absolute).
    vWorldPos = worldPos.xyz + renderOrigin.xyz;

    // For the water quad, `inst.model` is composed of (translation,
    // axis-aligned rotation, uniform scale) — see the cell loader's
    // water-plane spawn. So the 3×3 upper block is a similarity
    // transform and we can transform normal / tangent with it
    // directly (no inverse-transpose needed). The renderer guarantees
    // water meshes never carry non-uniform scale (`INSTANCE_FLAG_NUS`
    // is clear).
    mat3 modelRot = mat3(inst.model);
    vWorldNormal       = normalize(modelRot * localNormal);
    vec3 tangentWorld = modelRot * inTangent.xyz;
    // Old water meshes often have no tangent frame at all. Avoid normalizing
    // a zero vector here and normalise the handedness sentinel as well: a
    // zero sign would otherwise erase the entire bitangent in water.frag,
    // making authored normal maps read as a flat strip on those meshes.
    vWorldTangent = length(tangentWorld) > 1.0e-5
        ? normalize(tangentWorld)
        : vec3(1.0, 0.0, 0.0);
    vWorldBitangentSign = abs(inTangent.w) > 0.5
        ? (inTangent.w < 0.0 ? -1.0 : 1.0)
        : 1.0;
    vUV = inUV;

    // TAA jitter pulled from the camera UBO — keeps water's projected
    // depth coherent with the opaque pass so the shoreline foam ray
    // and the depth buffer stay in lockstep.
    vec4 clip = viewProj * worldPos;
    clip.xy += jitter.xy * clip.w;
    gl_Position = clip;
}
