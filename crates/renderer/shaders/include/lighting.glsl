// Point/spot attenuation, shadowable light radiance, one-bounce GI irradiance
//
// NON-STANDALONE shader fragment. Included by triangle.frag in dependency
// order via GL_GOOGLE_include_directive; it references symbols (structs,
// SSBO/UBO bindings, helper functions, constants) defined in shader_constants.glsl
// and in earlier includes. Do not compile on its own.

// ── Point / spot distance attenuation (REND-#1451) ──────────────────
//
// Two-term model mirroring the OpenMW / Bethesda light lineage
// (`1/(c + l·d + q·d²) × (1 − quickstep(d/r − 1))`, see
// reference/openmw/files/shaders/lib/light/lighting_util.glsl): a
// PHYSICAL near-zone falloff keyed to the AUTHORED radius, MULTIPLIED
// by a soft CULL window that fades the residual tail full→zero from
// the authored radius out to the cull radius `R`.
//
// `R` is `lights[i].position_radius.w` — the cull radius the CPU
// uploads as `authored × LIGHT_RANGE_EXTENSION` (cluster culling in
// cluster_cull.comp depends on `.w` carrying that, so it stays the
// cull radius and the authored radius is recovered here as
// `knee = kneeFrac × R`).
//
//   kneeFrac = dofParams.z, runtime-tunable for the REND-#1451
//   controlled bench (0 → default 0.5 ⇒ authored radius at half the
//   cull radius, the LIGHT_RANGE_EXTENSION = 2.0 geometry). Lower
//   kneeFrac ⇒ light is dimmer at the authored radius.
//
// The pre-fix formula used ONLY the cull window as the entire
// attenuation, stretched to `R`, so it read 75% at the authored
// radius (`d = R/2`) — the bright near-zone ring (Lonesome Road /
// Ulysses Temple). DBG_LEGACY_LIGHT_ATTEN restores it so both models
// can be A/B'd live in one session.
//
// MUST be called from BOTH the pass-1 reservoir loop in main() AND
// shadowableLightRadiance() so the WRS unshadowed accumulation cancels
// bit-for-bit against the shadowed subtraction (#1369). Spot lights
// call this for the distance term, then multiply the cone factor.
float pointSpotAtten(float dist, float R, float shape, uint dbgFlags) {
    if ((dbgFlags & DBG_LEGACY_LIGHT_ATTEN) != 0u) {
        float ratio = dist / max(R, 1.0);
        float window = clamp(1.0 - ratio * ratio, 0.0, 1.0);
        return pow(window, shape);
    }
    float kneeFrac = (dofParams.z > 0.0001) ? dofParams.z : 0.5;
    float knee = max(R * kneeFrac, 1.0);
    float dn = dist / knee;
    // Smooth, bounded near-zone falloff: 1.0 at the source, decaying
    // with the per-light `shape` (falloff_exponent). shape 1 → 50% at
    // the authored radius, ~2.3 → ~30%. Never reaches 0 by itself —
    // the cull window below does the zeroing.
    float phys = 1.0 / (1.0 + shape * dn * dn);
    // Anti-pop-in cull: full at the authored radius, zero at R, C¹ at
    // both ends (smoothstep) so there is no visible circular shoulder
    // on floors (the pre-window-era regression, commit 78632a6).
    float wcull = 1.0 - smoothstep(knee, max(R, knee + 1.0), dist);
    return phys * wcull;
}

// Direct Cook-Torrance contribution of cluster light `i` at this
// fragment — exactly the `brdfResult * unshadowedRadiance` the WRS
// streaming pass accumulates and the shadow pass subtracts on a hit.
// Factored out of the pass-1 loop (#1369) so pass 2 recomputes this
// from the light index instead of caching a vec3 per reservoir. That
// retires the `resRadiance[NUM_RESERVOIRS]` array (192 B/thread of
// local storage at 16 reservoirs), the dominant per-thread footprint
// suppressing WRS occupancy. The body is copied verbatim from the
// pass-1 attenuation + BRDF so both call sites evaluate the identical
// expression and the unshadowed accumulation cancels bit-for-bit
// against the shadowed subtraction. Assumes a non-interior-fill
// point/spot/directional light that already cleared the contribution
// gate — callers own the fill early-out and the gate.
vec3 shadowableLightRadiance(
    uint i, vec3 N, vec3 V, float NdotV, vec3 F0,
    vec3 albedo, float roughness, float metalness,
    float specStrength, vec3 specColor,
    GpuMaterial mat, vec4 fragTangent, vec3 fragWorldPos, uint dbgFlags)
{
    vec3 lightPos = lights[i].position_radius.xyz;
    float radius = lights[i].position_radius.w;
    vec3 lightColor = lights[i].color_type.rgb;
    float lightType = lights[i].color_type.w;
    float falloffShape = lights[i].params.x;

    vec3 L;
    float dist;
    float atten;
    if (lightType < 0.5) {
        // Point light.
        vec3 toLight = lightPos - fragWorldPos;
        dist = length(toLight);
        L = toLight / max(dist, 0.001);
        atten = pointSpotAtten(dist, radius, falloffShape, dbgFlags);
    } else if (lightType < 1.5) {
        // Spot light.
        vec3 toLight = lightPos - fragWorldPos;
        dist = length(toLight);
        L = toLight / max(dist, 0.001);
        vec3 spotDir = normalize(lights[i].direction_angle.xyz);
        float spotAngle = lights[i].direction_angle.w;
        atten = pointSpotAtten(dist, radius, falloffShape, dbgFlags);
        float spotFactor = dot(-L, spotDir);
        atten *= clamp((spotFactor - spotAngle) / (1.0 - spotAngle), 0.0, 1.0);
    } else {
        // Directional light.
        L = normalize(lights[i].direction_angle.xyz);
        dist = 10000.0;
        atten = 1.0;
    }

    float NdotL = max(dot(N, L), 0.0);

    vec3 H = normalize(V + L);
    float NdotH = max(dot(N, H), 0.0);
    float HdotV = max(dot(H, V), 0.0);

    float aaRoughness = ((dbgFlags & DBG_DISABLE_SPECULAR_AA) != 0u)
        ? roughness
        : specularAaRoughness(N, roughness);
    float D;
    if (mat.anisotropic > 0.0
        && dot(fragTangent.xyz, fragTangent.xyz) > 1e-4)
    {
        vec3 T = normalize(fragTangent.xyz);
        T = normalize(T - dot(T, N) * N);
        vec3 B = normalize(cross(N, T)) * fragTangent.w;
        float HdotX = dot(H, T);
        float HdotY = dot(H, B);
        float ax;
        float ay;
        deriveAxAy(aaRoughness, mat.anisotropic, ax, ay);
        D = distributionGGXAniso(NdotH, HdotX, HdotY, ax, ay);
    } else {
        D = distributionGGX(NdotH, aaRoughness);
    }
    float G = geometrySmith(NdotV, NdotL, aaRoughness);
    vec3 F = fresnelSchlick(HdotV, F0);

    vec3 kD = (1.0 - F) * (1.0 - metalness);
    vec3 specular = (D * G * F) / max(4.0 * NdotV * NdotL, 0.01);
    // Multi-scatter energy compensation (Fdez-Agüera / Filament). Restores
    // the energy the single-scatter D·G·F lobe loses to microfacet masking
    // as roughness rises — rough conductors stop darkening. Computed inside
    // this shared function so BOTH WRS passes (pass-1 accumulate, pass-2
    // shadowed subtract) evaluate the identical expression and the
    // unshadowed accumulation still cancels bit-for-bit (#1369 invariant).
    // No-op at low roughness, so it cannot affect the reflection gate.
    if ((dbgFlags & DBG_DISABLE_MULTISCATTER) == 0u) {
        specular *= multiScatterEnergyCompensation(F0, NdotV, aaRoughness);
    }
    vec3 unshadowedRadiance = lightColor * atten;
    vec3 diffuseBrdf;
    if ((mat.materialFlags & MAT_FLAG_PBR_BSDF) != 0u) {
        float HdotL = max(dot(H, L), 0.0);
        DisneyDiffuseSplit dd = disneyDiffuseSplit(
            albedo, roughness, mat.subsurface, mat.sheen, mat.sheenTint,
            NdotL, NdotV, HdotL
        );
        diffuseBrdf = (dd.diffuse * PI + dd.sheen) * (1.0 - metalness);
    } else {
        diffuseBrdf = kD * albedo;
    }
    vec3 brdfResult = (diffuseBrdf + specular * specStrength * specColor) * NdotL;
    return brdfResult * unshadowedRadiance;
}

// ── One-bounce GI helper (REND — true light-transport bounce) ───────

// Direct irradiance arriving at a one-bounce GI hit point `p` (surface
// normal `n`), summed over the scene lights with a shadow ray per
// contributing light. This is the term that turns the GI bounce into
// real light transport (and colour bleeding) instead of the legacy
// `cell_ambient × albedo` approximation. Lights are range-culled and the
// count is capped (`GI_HIT_LIGHT_CAP`) so dense cells can't explode the
// per-hit cost. Returns radiance in the same units the primary direct
// path uses, so the bounce energy matches.
//
// PERF-D5-NEW-02 / #1800 — this walks a FIXED prefix of the `lights[]`
// array (upload order), not a per-hit-point selection like the primary
// path's clustered culling. `collect_lights` (byroredux/src/render/
// lights.rs) sorts the point-light suffix by descending
// `gi_priority_score` before upload specifically so this fixed prefix
// approximates "the most influential lights scene-wide" instead of
// "whatever ECS iteration order produced." Still an approximation (no
// idea where `p` actually is) — the per-light `dist > radius` skip below
// is what keeps genuinely out-of-range lights from contributing, same
// as before.
vec3 giHitIrradiance(vec3 p, vec3 n, uint dbgFlags) {
    vec3 e = vec3(0.0);
    uint count = min(lightCount, GI_HIT_LIGHT_CAP);
    for (uint i = 0u; i < count; ++i) {
        float lightType = lights[i].color_type.w;
        vec3 lightColor = lights[i].color_type.rgb;
        float radius = lights[i].position_radius.w;
        float falloffShape = lights[i].params.x;

        vec3 L;
        float dist;
        float atten;
        if (lightType < 1.5) {
            // Point (type 0) or spot (type 1).
            vec3 toLight = lights[i].position_radius.xyz - p;
            dist = length(toLight);
            if (dist > radius) continue; // out of influence range
            L = toLight / max(dist, 1e-3);
            atten = pointSpotAtten(dist, radius, falloffShape, dbgFlags);
            if (lightType >= 0.5) {
                vec3 spotDir = normalize(lights[i].direction_angle.xyz);
                float spotAngle = lights[i].direction_angle.w;
                float spotFactor = dot(-L, spotDir);
                atten *= clamp((spotFactor - spotAngle) / (1.0 - spotAngle), 0.0, 1.0);
            }
        } else {
            // Directional (type 2): exterior sun / interior fill.
            L = normalize(lights[i].direction_angle.xyz);
            dist = 1.0e4;
            atten = 1.0;
        }

        float NdotL = max(dot(n, L), 0.0);
        float contrib = atten * NdotL;
        if (contrib < 1.0e-4) continue;

        // Shadow ray toward the light — skip the contribution if occluded.
        rayQueryEXT sRQ;
        rayQueryInitializeEXT(
            sRQ, topLevelAS,
            gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT, 0xFF,
            p + n * 0.1, 0.05, L, max(dist - 0.2, 0.05));
        rayQueryProceedEXT(sRQ);
        if (rayQueryGetIntersectionTypeEXT(sRQ, true)
            != gl_RayQueryCommittedIntersectionNoneEXT) {
            continue;
        }
        e += lightColor * contrib;
    }
    return e;
}

