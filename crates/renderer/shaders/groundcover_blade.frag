#version 460
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : require
#extension GL_GOOGLE_include_directive : require
// `GpuInstance.skinnedVertexAddress` / `SkinnedVertexRef` in bindings.glsl
// need buffer_reference support, same as water.frag.
#extension GL_EXT_buffer_reference : require
#extension GL_ARB_gpu_shader_int64 : require

// EXAL ground cover — blade shading (§5 Stage 1, §12.3; #4055 / #4056).
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
// The G-buffer's normal / motion / mesh-ID attachments stay unwritten, exactly
// as `water.frag` leaves them and for the same reason: this geometry is
// generated in the vertex shader from a seed and a *time-varying* wind field,
// so it has no motion vector that a reprojection could believe. Instead the
// FSR reactive and transparency masks are written at full strength, which is
// the documented remedy for precisely this case — surface colour that the
// pass's own depth and motion vectors do not describe.
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

layout(location = 0) out vec4 outColor;
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

layout(std430, set = 2, binding = 6) readonly buffer GcSpeciesBuffer {
    GroundCoverSpecies gcSpecies[];
};

void main() {
    GroundCoverSpecies sp = gcSpecies[vSpecies];

    // §7's colour gradient: base → tip. Blades are darker at the base in
    // nearly all real vegetation, because the base is self-shadowed by the
    // canopy above it — so this gradient is already doing a weak, free version
    // of what §12.1's contact occlusion will do properly.
    vec3 albedo = mix(sp.baseColour.rgb, sp.tipColour.rgb, vBladeT);
    // Per-blade colour jitter. A field of identically-coloured blades reads as
    // one object with a texture on it rather than as many plants.
    albedo *= mix(0.82, 1.18, vColourJitter);

    // Two-sided: a blade is a ribbon with no inside, so the shading normal has
    // to follow whichever face the camera is looking at.
    vec3 V = normalize(cameraPos.xyz - vWorldPos);
    vec3 N = normalize(vWorldNormal);
    if (dot(N, V) < 0.0) {
        N = -N;
    }

    vec3 lit = vec3(0.0);
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
        // Wrapped diffuse. A blade is thin and translucent, so light arriving
        // slightly behind it still reaches the eye; a hard `max(NdotL, 0)`
        // turns every back-lit blade black and is the single most obvious way
        // grass reads as cardboard. §12.2 replaces this with a real
        // transmission term; the wrap is the honest cheap stand-in until then.
        float wrapped = clamp((NdotL + 0.4) / 1.4, 0.0, 1.0);
        if (wrapped <= 0.0) {
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
        lit += lights[i].color_type.rgb * (wrapped * lights[i].params.x) * shadow;
    }

    // Ambient. The base of a sward sees much less sky than the tip; scaling
    // ambient by blade height is the cheapest version of that and stops the
    // stratum reading as uniformly flat-lit under overcast.
    vec3 ambient = sceneFlags.yzw * mix(0.45, 1.0, vBladeT);

    outColor = vec4(albedo * (lit + ambient), 1.0);

    // See the header. Procedural, wind-animated geometry has no motion vector
    // this pass could write, so the reconstruction is told to trust the
    // current frame here rather than reproject history onto it.
    outFsrReactive = 1.0;
    outFsrTransparency = 1.0;
}
