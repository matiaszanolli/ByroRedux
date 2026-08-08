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
// against the shadowed subtraction. Assumes a point/spot/directional
// light that already cleared the contribution gate.
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
        // #2205 — guard against a degenerate near-zero authored outer
        // angle (cos ≈ 1.0), matching giLightSample's guard below now
        // that spot lights are actually reachable instead of silently
        // downgraded to point lights.
        atten *= clamp((spotFactor - spotAngle) / max(1.0 - spotAngle, 1e-4), 0.0, 1.0);
    } else {
        // Directional light.
        L = normalize(lights[i].direction_angle.xyz);
        dist = DIRECTIONAL_SHADOW_TRACE_DISTANCE;
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
        // This path keeps the legacy non-/PI Lambert convention below.
        // Rescale the complete Disney lobe together so sheen retains the
        // same weight relative to diffuse as the /PI direct-sun path.
        diffuseBrdf = (dd.diffuse + dd.sheen) * PI * (1.0 - metalness);
    } else {
        diffuseBrdf = kD * albedo;
    }
    vec3 brdfResult = (diffuseBrdf + specular * specStrength * specColor) * NdotL;
    return brdfResult * unshadowedRadiance;
}

// Per-light visibility policy shared by direct, reflected and GI paths.
vec3 traceLightTransmittance(
    uint lightIndex,
    vec3 origin,
    vec3 direction,
    float maxDist
) {
    float policy = lights[lightIndex].params.z;
    if (decodeShadowPolicy(policy) == SHADOW_POLICY_FULL) {
        return traceShadowTransmittance(
            origin,
            direction,
            maxDist,
            lights[lightIndex].params.y
        );
    }
    if (decodeShadowPolicy(policy) == SHADOW_POLICY_STRUCTURE) {
        return traceStructureVisibility(origin, direction, maxDist);
    }
    return vec3(1.0);
}

// ── Indirect-hit lighting ───────────────────────────────────────────

bool giLightSample(
    uint i, vec3 p, vec3 n, uint dbgFlags,
    out vec3 L, out float dist, out float contrib
) {
    float lightType = lights[i].color_type.w;
    float radius = lights[i].position_radius.w;
    float falloffShape = lights[i].params.x;
    float atten;
    if (lightType < 1.5) {
        vec3 toLight = lights[i].position_radius.xyz - p;
        dist = length(toLight);
        if (dist > radius) return false;
        L = toLight / max(dist, 1e-3);
        atten = pointSpotAtten(dist, radius, falloffShape, dbgFlags);
        if (lightType >= 0.5) {
            vec3 spotDir = normalize(lights[i].direction_angle.xyz);
            float spotAngle = lights[i].direction_angle.w;
            float spotFactor = dot(-L, spotDir);
            atten *= clamp(
                (spotFactor - spotAngle) / max(1.0 - spotAngle, 1e-4),
                0.0, 1.0);
        }
    } else {
        L = normalize(lights[i].direction_angle.xyz);
        dist = DIRECTIONAL_SHADOW_TRACE_DISTANCE;
        atten = 1.0;
    }
    contrib = atten * max(dot(n, L), 0.0);
    return contrib >= 1.0e-4;
}

// Radiance seen by a bounded path that escapes the TLAS. Interiors retain
// their authored cell ambient; Skyrim DALC cells use the authored directional
// cube; exteriors blend horizon ambient into the sky zenith along the actual
// ray direction. The old path returned `sceneFlags.yzw * 0.5` everywhere,
// which made an upward sky ray and a downward horizon ray indistinguishable.
//
// #2472 — `sceneFlags.yzw` (XCLL cell ambient) is `sampleDalcCube`'s
// sibling everywhere else this shader treats them interchangeably (see
// the `ambientFallback` ternary in `triangle.frag`): both are authored
// irradiance. #2244 converted the DALC arm below to radiance (`* (1.0 /
// PI)`) but left the sky-mix and interior-fallback arms in irradiance
// units, so a Skyrim DALC cell's bounded-path escape came out ~π× dimmer
// than an otherwise-identical FO3/FNV/Oblivion XCLL cell. `skyTint` is
// already rendered sky radiance (see `triangle.frag`'s `skyColor =
// skyTint.rgb` background write) and is untouched by the conversion.
vec3 pathEnvironmentRadiance(vec3 direction) {
    vec3 rayDir = normalize(direction);
    if (jitter.w > 0.5) {
        float skyWeight = smoothstep(-0.2, 0.8, rayDir.y);
        return mix(sceneFlags.yzw * (1.0 / PI), skyTint.xyz, skyWeight);
    }
    if (dalcFlags.x > 0.5) {
        // DALC stores directional irradiance; an escaping path needs
        // environment radiance before multiplying by its throughput.
        return sampleDalcCube(rayDir) * (1.0 / PI);
    }
    return sceneFlags.yzw * (1.0 / PI) * 0.5;
}

// Direct outgoing radiance at a bounded-path hit. Candidate selection remains
// bounded by GI_HIT_LIGHT_CAP and the caller-selected visible-light limit, but
// scores and accumulates the actual material BRDF rather than treating every
// hit as Lambertian `baseColor / PI`.
vec3 pathHitRadiance(
    vec3 p, vec3 n, vec3 viewDir,
    GpuMaterial mat, vec3 baseColor,
    uint dbgFlags, uint visibleLightLimit
) {
    uint selected[GI_HIT_LIGHT_CAP];
    float selectedScore[GI_HIT_LIGHT_CAP];
    for (uint k = 0u; k < GI_HIT_LIGHT_CAP; ++k) {
        selected[k] = 0xFFFFFFFFu;
        selectedScore[k] = -1.0;
    }

    for (uint i = 0u; i < lightCount; ++i) {
        vec3 candidateL;
        float candidateDist;
        float candidateContrib;
        if (!giLightSample(
                i, p, n, dbgFlags,
                candidateL, candidateDist, candidateContrib)) continue;
        float diffusePdf;
        float specularPdf;
        vec3 candidateBsdf = evaluatePathBsdf(
            n, viewDir, candidateL,
            baseColor, mat.metalness, mat.roughness, mat.ior,
            diffusePdf, specularPdf);
        float score = candidateContrib
            * pathLuminance(lights[i].color_type.rgb * candidateBsdf);
        int insertAt = -1;
        for (uint k = 0u; k < GI_HIT_LIGHT_CAP; ++k) {
            if (score > selectedScore[k]) {
                insertAt = int(k);
                break;
            }
        }
        if (insertAt < 0) continue;
        for (int k = int(GI_HIT_LIGHT_CAP) - 1; k > insertAt; --k) {
            selected[k] = selected[k - 1];
            selectedScore[k] = selectedScore[k - 1];
        }
        selected[insertAt] = i;
        selectedScore[insertAt] = score;
    }

    vec3 radiance = vec3(0.0);
    uint visibleCount = 0u;
    for (uint k = 0u; k < GI_HIT_LIGHT_CAP; ++k) {
        uint i = selected[k];
        if (i == 0xFFFFFFFFu) break;
        vec3 L;
        float dist;
        float contribution;
        if (!giLightSample(
                i, p, n, dbgFlags, L, dist, contribution)) continue;
        float diffusePdf;
        float specularPdf;
        vec3 bsdf = evaluatePathBsdf(
            n, viewDir, L,
            baseColor, mat.metalness, mat.roughness, mat.ior,
            diffusePdf, specularPdf);
        if (pathLuminance(bsdf) <= 1e-6) continue;
        vec3 visibility = traceLightTransmittance(
            i, p + n * 0.1, L, max(dist - 0.2, 0.05));
        if (max(max(visibility.r, visibility.g), visibility.b) <= 0.001) continue;
        radiance += lights[i].color_type.rgb
            * contribution * bsdf * visibility;
        visibleCount++;
        if (visibleCount >= visibleLightLimit) break;
    }
    return radiance;
}

// Reflection/refraction rays run per glossy primary fragment, so evaluating
// the full GI light cap here multiplies one primary ray into dozens of nested
// queries in glass-heavy views. Pick the single strongest local light and use
// one opaque visibility query. The wider glass-aware evaluator remains on the
// low-rate diffuse GI path.
vec3 reflectionHitIrradiance(vec3 p, vec3 n, uint dbgFlags) {
    const int REFLECTION_LIGHT_CANDIDATES = 4;
    uint selected[REFLECTION_LIGHT_CANDIDATES];
    float selectedScore[REFLECTION_LIGHT_CANDIDATES];
    for (int k = 0; k < REFLECTION_LIGHT_CANDIDATES; ++k) {
        selected[k] = 0xFFFFFFFFu;
        selectedScore[k] = -1.0;
    }

    for (uint i = 0u; i < lightCount; ++i) {
        vec3 L;
        float dist;
        float contrib;
        if (!giLightSample(i, p, n, dbgFlags, L, dist, contrib)) continue;
        float score = contrib
            * dot(max(lights[i].color_type.rgb, vec3(0.0)),
                  vec3(0.2126, 0.7152, 0.0722));
        int insertAt = -1;
        for (int k = 0; k < REFLECTION_LIGHT_CANDIDATES; ++k) {
            if (score > selectedScore[k]) {
                insertAt = k;
                break;
            }
        }
        if (insertAt >= 0) {
            for (int k = REFLECTION_LIGHT_CANDIDATES - 1; k > insertAt; --k) {
                selected[k] = selected[k - 1];
                selectedScore[k] = selectedScore[k - 1];
            }
            selected[insertAt] = i;
            selectedScore[insertAt] = score;
        }
    }

    // Try candidates in local-importance order. The former single-choice
    // estimator returned black whenever its strongest light was occluded even
    // if the next lamp was plainly visible in the reflection/refraction.
    for (int k = 0; k < REFLECTION_LIGHT_CANDIDATES; ++k) {
        uint i = selected[k];
        if (i == 0xFFFFFFFFu) break;
        vec3 L;
        float dist;
        float contrib;
        if (!giLightSample(i, p, n, dbgFlags, L, dist, contrib)) continue;
        vec3 visibility = traceLightTransmittance(
            i, p + n * 0.1, L, max(dist - 0.2, 0.05));
        if (max(max(visibility.r, visibility.g), visibility.b) <= 0.001) continue;
        return lights[i].color_type.rgb * contrib * visibility;
    }
    return vec3(0.0);
}

// Direct irradiance arriving at a ray hit. Select the locally strongest
// lights before casting visibility rays; the previous fixed upload prefix
// frequently contained no light whose radius reached the hit point, making
// indirect illumination black even in visibly lit rooms.
vec3 giHitIrradiance(vec3 p, vec3 n, uint dbgFlags) {
    uint selected[GI_HIT_LIGHT_CAP];
    float selectedScore[GI_HIT_LIGHT_CAP];
    for (uint k = 0u; k < GI_HIT_LIGHT_CAP; ++k) {
        selected[k] = 0xFFFFFFFFu;
        selectedScore[k] = -1.0;
    }

    for (uint i = 0u; i < lightCount; ++i) {
        vec3 candidateL;
        float candidateDist;
        float candidateContrib;
        if (!giLightSample(
                i, p, n, dbgFlags,
                candidateL, candidateDist, candidateContrib)) continue;
        float score = candidateContrib
            * dot(max(lights[i].color_type.rgb, vec3(0.0)),
                  vec3(0.2126, 0.7152, 0.0722));
        int insertAt = -1;
        for (uint k = 0u; k < GI_HIT_LIGHT_CAP; ++k) {
            if (score > selectedScore[k]) {
                insertAt = int(k);
                break;
            }
        }
        if (insertAt < 0) continue;
        for (int k = int(GI_HIT_LIGHT_CAP) - 1; k > insertAt; --k) {
            selected[k] = selected[k - 1];
            selectedScore[k] = selectedScore[k - 1];
        }
        selected[insertAt] = i;
        selectedScore[insertAt] = score;
    }

    vec3 e = vec3(0.0);
    const uint GI_VISIBLE_LIGHT_CAP = 2u;
    uint visibleCount = 0u;
    for (uint k = 0u; k < GI_HIT_LIGHT_CAP; ++k) {
        uint i = selected[k];
        if (i == 0xFFFFFFFFu) break;
        vec3 L;
        float dist;
        float contrib;
        if (!giLightSample(i, p, n, dbgFlags, L, dist, contrib)) continue;
        vec3 visibility = traceLightTransmittance(
            i, p + n * 0.1, L, max(dist - 0.2, 0.05));
        if (max(max(visibility.r, visibility.g), visibility.b) <= 0.001) continue;
        e += lights[i].color_type.rgb * contrib * visibility;
        visibleCount++;
        if (visibleCount >= GI_VISIBLE_LIGHT_CAP) break;
    }
    return e;
}
