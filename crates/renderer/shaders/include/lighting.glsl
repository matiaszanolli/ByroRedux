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
        atten *= clamp((spotFactor - spotAngle) / (1.0 - spotAngle), 0.0, 1.0);
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
        diffuseBrdf = (dd.diffuse * PI + dd.sheen) * (1.0 - metalness);
    } else {
        diffuseBrdf = kD * albedo;
    }
    vec3 brdfResult = (diffuseBrdf + specular * specStrength * specColor) * NdotL;
    return brdfResult * unshadowedRadiance;
}

// ── Ray-traced visibility with dielectric transmission ─────────────

// Return RGB visibility along a light segment. Opaque geometry is a binary
// blocker; glass is accumulated interface by interface so clear/tinted panes
// transmit light instead of casting wall-like black shadows. TLAS instance
// masks keep both traversals coherent even though every BLAS is flagged
// opaque at the geometry level.
vec3 traceShadowTransmittance(
    vec3 origin, vec3 direction, float maxDist, float emitterRadius
) {
    // Alpha-aware opaque traversal. Ray-query geometry is intentionally built
    // opaque, so alpha-test holes and effect cards must be continued through
    // explicitly. Treating their triangle bounds as solid is what let flame
    // cards and lamp globes shadow the point light they visually emit.
    const int MAX_OPAQUE_LAYERS = 8;
    vec3 opaqueOrigin = origin;
    float opaqueRemaining = maxDist;
    for (int layer = 0; layer < MAX_OPAQUE_LAYERS; ++layer) {
        rayQueryEXT opaqueRQ;
        rayQueryInitializeEXT(
            opaqueRQ, topLevelAS, gl_RayFlagsOpaqueEXT,
            SHADOW_MASK_OPAQUE,
            opaqueOrigin, 0.05, direction, opaqueRemaining);
        while (rayQueryProceedEXT(opaqueRQ)) {}
        if (rayQueryGetIntersectionTypeEXT(opaqueRQ, true)
            == gl_RayQueryCommittedIntersectionNoneEXT) break;

        int hitIdx = rayQueryGetIntersectionInstanceCustomIndexEXT(opaqueRQ, true);
        int hitPrim = rayQueryGetIntersectionPrimitiveIndexEXT(opaqueRQ, true);
        vec2 hitBary = rayQueryGetIntersectionBarycentricsEXT(opaqueRQ, true);
        float hitT = rayQueryGetIntersectionTEXT(opaqueRQ, true);
        GpuInstance hitInst = instances[hitIdx];
        GpuMaterial hitMat = materials[hitInst.materialId];
        bool effectCard = hitMat.materialKind == MATERIAL_KIND_EFFECT_SHADER;
        if (effectCard) {
            float advance = hitT + 0.1;
            opaqueRemaining -= advance;
            if (opaqueRemaining <= 0.05) break;
            opaqueOrigin += direction * advance;
            continue;
        }

        bool alphaSensitive = hitMat.alphaThreshold > 0.0
            || (hitInst.flags & INSTANCE_FLAG_ALPHA_BLEND) != 0u;
        bool nearEmitter = emitterRadius > 0.0
            && opaqueRemaining - hitT <= emitterRadius
            && max(max(hitMat.emissiveR, hitMat.emissiveG), hitMat.emissiveB)
                * max(hitMat.emissiveMult, 1.0) > 0.01;
        // The overwhelmingly common case is a fully opaque, non-emissive
        // blocker. Keep that path binary and texture-free; only authored
        // coverage and terminal emitter shells need material sampling.
        if (!alphaSensitive && !nearEmitter) return vec3(0.0);

        vec2 hitUV = transformRayHitUV(
            hitMat, getHitUV(uint(hitIdx), uint(hitPrim), hitBary));
        vec4 hitBase;
        bool covered = rayHitHasCoverage(hitInst, hitMat, hitUV, hitBase);
        vec3 hitEmission = rayHitEmission(hitMat, hitUV, hitBase.rgb, 0.0);
        bool sourceShell = nearEmitter
            && max(max(hitEmission.r, hitEmission.g), hitEmission.b) > 0.01;
        if (covered && !sourceShell) return vec3(0.0);

        float advance = hitT + 0.1;
        opaqueRemaining -= advance;
        if (opaqueRemaining <= 0.05) break;
        opaqueOrigin += direction * advance;
    }

    const int MAX_GLASS_INTERFACES = 4;
    vec3 transmission = vec3(1.0);
    vec3 rayOrigin = origin;
    float remaining = maxDist;

    for (int layer = 0; layer < MAX_GLASS_INTERFACES; ++layer) {
        rayQueryEXT glassRQ;
        rayQueryInitializeEXT(
            glassRQ, topLevelAS, gl_RayFlagsOpaqueEXT,
            SHADOW_MASK_GLASS,
            rayOrigin, 0.05, direction, remaining);
        while (rayQueryProceedEXT(glassRQ)) {}
        if (rayQueryGetIntersectionTypeEXT(glassRQ, true)
            == gl_RayQueryCommittedIntersectionNoneEXT) {
            break;
        }

        int hitIdx = rayQueryGetIntersectionInstanceCustomIndexEXT(glassRQ, true);
        int hitPrim = rayQueryGetIntersectionPrimitiveIndexEXT(glassRQ, true);
        vec2 hitBary = rayQueryGetIntersectionBarycentricsEXT(glassRQ, true);
        float hitT = rayQueryGetIntersectionTEXT(glassRQ, true);
        GpuInstance hitInst = instances[hitIdx];
        GpuMaterial hitMat = materials[hitInst.materialId];
        vec2 hitUV = transformRayHitUV(
            hitMat, getHitUV(uint(hitIdx), uint(hitPrim), hitBary));
        vec4 glassTex;
        bool covered = rayHitHasCoverage(hitInst, hitMat, hitUV, glassTex);
        if (!covered) {
            float advance = hitT + 0.1;
            remaining -= advance;
            if (remaining <= 0.05) break;
            rayOrigin += direction * advance;
            continue;
        }
        vec3 tint = clamp(
            glassTex.rgb * vec3(hitMat.diffuseR, hitMat.diffuseG, hitMat.diffuseB),
            vec3(0.02), vec3(1.0));
        float authoredOpacity = clamp(glassTex.a * hitMat.materialAlpha, 0.0, 1.0);
        vec3 hitN = getHitTriNormal(uint(hitIdx), uint(hitPrim));
        if (dot(hitN, direction) > 0.0) hitN = -hitN;
        float cosTheta = clamp(dot(-direction, hitN), 0.0, 1.0);
        float f0 = dielectricF0FromIor(max(hitMat.ior, 1.0));
        float fresnel = fresnelSchlick(cosTheta, vec3(f0)).r;

        // Opacity in Bethesda glass mostly encodes surface tint/decal
        // strength, not solid occlusion. Apply modest Beer-style absorption
        // per interface and the dielectric Fresnel transmission loss.
        float absorption = mix(0.08, 0.45, authoredOpacity);
        transmission *= mix(vec3(1.0), tint, absorption) * (1.0 - fresnel);
        if (max(max(transmission.r, transmission.g), transmission.b) < 0.01) {
            return vec3(0.0);
        }

        float advance = hitT + 0.1;
        remaining -= advance;
        if (remaining <= 0.05) break;
        rayOrigin += direction * advance;
    }
    return transmission;
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
        dist = 100000.0;
        atten = 1.0;
    }
    contrib = atten * max(dot(n, L), 0.0);
    return contrib >= 1.0e-4;
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
        vec3 visibility = traceShadowTransmittance(
            p + n * 0.1, L, max(dist - 0.2, 0.05), lights[i].params.y);
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
        vec3 visibility = traceShadowTransmittance(
            p + n * 0.1, L, max(dist - 0.2, 0.05), lights[i].params.y);
        if (max(max(visibility.r, visibility.g), visibility.b) <= 0.001) continue;
        e += lights[i].color_type.rgb * contrib * visibility;
        visibleCount++;
        if (visibleCount >= GI_VISIBLE_LIGHT_CAP) break;
    }
    return e;
}
