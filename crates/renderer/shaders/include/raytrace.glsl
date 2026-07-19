// RT reflection helpers — hit UV / hit normal lookup + traceReflection
//
// NON-STANDALONE shader fragment. Included by triangle.frag in dependency
// order via GL_GOOGLE_include_directive; it references symbols (structs,
// SSBO/UBO bindings, helper functions, constants) defined in shader_constants.glsl
// and in earlier includes. Do not compile on its own.

// Defined by lighting.glsl later in triangle.frag's include sequence. A
// prototype lets reflection-hit shading use its deliberately bounded
// one-light evaluator. Diffuse GI and refraction termini retain the wider
// locally-selected light set in `giHitIrradiance`.
vec3 reflectionHitIrradiance(vec3 p, vec3 n, uint dbgFlags);

// ── RT Reflection ───────────────────────────────────────────────────

// Look up UV coordinates at a ray hit point using barycentrics + vertex data.
vec2 getHitUV(uint instanceIdx, uint primitiveIdx, vec2 barycentrics) {
    GpuInstance hitInst = instances[instanceIdx];
    uint vOff = hitInst.vertexOffset;
    uint iOff = hitInst.indexOffset;

    // Triangle vertex indices from the global index buffer.
    uint i0 = indexData[iOff + primitiveIdx * 3 + 0];
    uint i1 = indexData[iOff + primitiveIdx * 3 + 1];
    uint i2 = indexData[iOff + primitiveIdx * 3 + 2];

    // Vertex stride + UV offset come from the file-scope
    // `VERTEX_STRIDE_FLOATS` / `VERTEX_UV_OFFSET_FLOATS` constants —
    // one source of truth across every RT hit-fetch site, see REN-D6-NEW-01.
    vec2 uv0 = vec2(vertexData[(vOff + i0) * VERTEX_STRIDE_FLOATS + VERTEX_UV_OFFSET_FLOATS],
                     vertexData[(vOff + i0) * VERTEX_STRIDE_FLOATS + VERTEX_UV_OFFSET_FLOATS + 1]);
    vec2 uv1 = vec2(vertexData[(vOff + i1) * VERTEX_STRIDE_FLOATS + VERTEX_UV_OFFSET_FLOATS],
                     vertexData[(vOff + i1) * VERTEX_STRIDE_FLOATS + VERTEX_UV_OFFSET_FLOATS + 1]);
    vec2 uv2 = vec2(vertexData[(vOff + i2) * VERTEX_STRIDE_FLOATS + VERTEX_UV_OFFSET_FLOATS],
                     vertexData[(vOff + i2) * VERTEX_STRIDE_FLOATS + VERTEX_UV_OFFSET_FLOATS + 1]);

    // Barycentric interpolation: bary.x = u (vertex 1), bary.y = v (vertex 2), w = 1-u-v (vertex 0).
    float w = 1.0 - barycentrics.x - barycentrics.y;
    return w * uv0 + barycentrics.x * uv1 + barycentrics.y * uv2;
}

// World-space geometric (face) normal of a ray-query hit triangle, from
// its vertex POSITIONS (stride offset 0) transformed by the instance
// model matrix. Same fetch shape as getHitUV (validated in the reflection
// / refraction ray context). Used by two-surface glass refraction to
// refract the ray a second time as it exits the glass back face.
vec3 getHitTriNormal(uint instanceIdx, uint primitiveIdx) {
    GpuInstance hi = instances[instanceIdx];
    uint vOff = hi.vertexOffset;
    uint iOff = hi.indexOffset;
    uint i0 = indexData[iOff + primitiveIdx * 3 + 0];
    uint i1 = indexData[iOff + primitiveIdx * 3 + 1];
    uint i2 = indexData[iOff + primitiveIdx * 3 + 2];
    uint p0 = (vOff + i0) * VERTEX_STRIDE_FLOATS;
    uint p1 = (vOff + i1) * VERTEX_STRIDE_FLOATS;
    uint p2 = (vOff + i2) * VERTEX_STRIDE_FLOATS;
    vec3 v0 = vec3(vertexData[p0], vertexData[p0 + 1], vertexData[p0 + 2]);
    vec3 v1 = vec3(vertexData[p1], vertexData[p1 + 1], vertexData[p1 + 2]);
    vec3 v2 = vec3(vertexData[p2], vertexData[p2 + 1], vertexData[p2 + 2]);
    vec3 w0 = (hi.model * vec4(v0, 1.0)).xyz;
    vec3 w1 = (hi.model * vec4(v1, 1.0)).xyz;
    vec3 w2 = (hi.model * vec4(v2, 1.0)).xyz;
    return normalize(cross(w1 - w0, w2 - w0));
}

// Shared material sampling for every secondary-ray path. Keeping these rules
// here prevents reflection, refraction, GI and shadow traversal from each
// inventing a different meaning for diffuse alpha or glow maps.
vec2 transformRayHitUV(GpuMaterial mat, vec2 uv) {
    return uv * vec2(mat.uvScaleU, mat.uvScaleV)
         + vec2(mat.uvOffsetU, mat.uvOffsetV);
}

vec4 sampleRayHitBase(GpuInstance inst, GpuMaterial mat, vec2 uv, float lod) {
    return textureLod(textures[nonuniformEXT(inst.textureIndex)], uv, lod);
}

bool alphaComparePass(float alpha, float threshold, uint func) {
    if (threshold <= 0.0 || func == 0u) return true;
    if (func == 1u) return alpha < threshold;
    if (func == 2u) return abs(alpha - threshold) < (1.0 / 255.0);
    if (func == 3u) return alpha <= threshold;
    if (func == 4u) return alpha > threshold;
    if (func == 5u) return abs(alpha - threshold) >= (1.0 / 255.0);
    if (func == 6u) return alpha >= threshold;
    return false; // NEVER
}

bool rayHitHasCoverage(
    GpuInstance inst, GpuMaterial mat, vec2 uv, out vec4 baseSample
) {
    baseSample = sampleRayHitBase(inst, mat, uv, 0.0);
    float alpha = baseSample.a;
    // Match the primary BC1 contract: without an authored alpha channel,
    // BC1's index-3 zero is an encoder choice except on explicit alpha-test
    // materials.
    if ((inst.flags & INSTANCE_FLAG_DIFFUSE_ALPHA) == 0u
        && mat.alphaThreshold == 0.0) {
        alpha = 1.0;
    }
    alpha *= mat.materialAlpha;
    if (!alphaComparePass(alpha, mat.alphaThreshold, mat.alphaTestFunc)) {
        return false;
    }
    // Pure blend geometry uses alpha as binary coverage for ray traversal.
    // Physical dielectric transmission is reserved for MATERIAL_KIND_GLASS;
    // furniture/paintings with noisy authored alpha remain solid blockers.
    if ((inst.flags & INSTANCE_FLAG_ALPHA_BLEND) != 0u
        && mat.materialKind != MATERIAL_KIND_GLASS) {
        return alpha >= (1.0 / 255.0);
    }
    return true;
}

vec3 rayHitAlbedo(GpuMaterial mat, vec3 baseRgb) {
    return max(baseRgb * vec3(mat.diffuseR, mat.diffuseG, mat.diffuseB), vec3(0.0));
}

vec3 rayHitEmission(GpuMaterial mat, vec2 uv, vec3 baseRgb, float lod) {
    vec3 mask = baseRgb;
    if (mat.glowMapIndex != 0u) {
        mask = textureLod(
            textures[nonuniformEXT(mat.glowMapIndex)], uv, lod).rgb;
    }
    return max(
        vec3(mat.emissiveR, mat.emissiveG, mat.emissiveB)
        * mat.emissiveMult * mask,
        vec3(0.0));
}

// Cast a reflection ray and return the reflected color.
//
// Return contract (#1029 / REN-D9-NEW-06):
//   * `.rgb` is ALWAYS the final reflection colour the caller should
//     use — sky-tinted ambient blend on miss, distance-attenuated
//     surface texel on hit. Pre-#1029 the two callers (metal + glass)
//     interpreted this inconsistently: glass read `.rgb` directly,
//     while metal weighted the mix by `.a` and collapsed to a
//     separate `ambientFallback` on miss — discarding the
//     `skyTint*0.5 + sceneFlags.yzw*0.5` blend this function pays
//     to compute. One function, two semantics, easy to drift.
//   * `.a` is INFORMATIONAL hit confidence: `1.0 = hit`, `0.0 = miss`.
//     Available to callers that genuinely want to gate on
//     "did the ray hit BVH geometry" (e.g. to skip a follow-on cost
//     that only makes sense on hits). The reflection rgb is already
//     correct without it.
//
// Reflection is a shading ray, so it must resolve the CLOSEST opaque hit.
// `TerminateOnFirstHit` returns a traversal-order candidate, which is only
// valid for binary visibility. Sampling that candidate's material made
// mirrors and smooth walls look semi-transparent whenever it was geometry
// behind the actual reflector.
vec4 traceReflection(vec3 origin, vec3 direction, float maxDist, float mipBias,
                     int selfInstance) {
    // Miss / self-hit fallback colour — sky-tinted ambient (exterior) or
    // cell ambient (interior). Hoisted so a self-intersection hit (below)
    // can reuse it.
    bool _isExt = jitter.w > 0.5;
    vec3 missCol = _isExt ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5)
                          : sceneFlags.yzw;
    // tMin = 0.05 matches the N_bias offset every caller already
    // applies to `origin`. Live callers (grep for `traceReflection(`):
    //   * glass IOR reflection — bias 0.05, maxDist 3000
    //   * metal jittered reflection — bias 0.1, maxDist 5000
    // Same 0.05 tMin convention every other ray-query site in this
    // shader uses (grep `rayQueryInitializeEXT`): window portal,
    // refraction loop, cluster shadow, GI bounce. Pre-#1017 this was
    // 0.01 — five times smaller than the bias — which let perturbed-
    // normal flips at grazing angles fire the ray back through the
    // surface and self-hit, producing black speckle on metals. Same
    // fix shape as the GI-tMin normalisation (grep `giRQ`).
    //
    // Note: previous revisions of this comment cited line numbers
    // (1486/1633/1702/2049/2408/2472/2484) but Session 34's split
    // and subsequent refactors drift them every release; #1158 fix
    // (2026-05-18) replaces them with grep-friendly anchors.
    const int MAX_TRANSPARENT_SKIPS = 8;
    vec3 rayOrigin = origin;
    float remaining = maxDist;
    float travelled = 0.0;
    int hitInstanceIdx = -1;
    int hitPrimitiveIdx = 0;
    vec2 hitBary = vec2(0.0);
    vec2 hitUV = vec2(0.0);
    vec4 hitBase = vec4(0.0);

    for (int layer = 0; layer < MAX_TRANSPARENT_SKIPS; ++layer) {
        rayQueryEXT rq;
        rayQueryInitializeEXT(
            rq, topLevelAS, gl_RayFlagsOpaqueEXT, 0xFF,
            rayOrigin, 0.05, direction, remaining);
        while (rayQueryProceedEXT(rq)) {}
        if (rayQueryGetIntersectionTypeEXT(rq, true)
            == gl_RayQueryCommittedIntersectionNoneEXT) break;

        int candidateIdx =
            rayQueryGetIntersectionInstanceCustomIndexEXT(rq, true);
        int candidatePrim = rayQueryGetIntersectionPrimitiveIndexEXT(rq, true);
        vec2 candidateBary = rayQueryGetIntersectionBarycentricsEXT(rq, true);
        float candidateT = rayQueryGetIntersectionTEXT(rq, true);
        GpuInstance candidateInst = instances[candidateIdx];
        GpuMaterial candidateMat = materials[candidateInst.materialId];
        vec2 candidateUV = transformRayHitUV(
            candidateMat,
            getHitUV(uint(candidateIdx), uint(candidatePrim), candidateBary));
        vec4 candidateBase;
        bool covered = rayHitHasCoverage(
            candidateInst, candidateMat, candidateUV, candidateBase);

        if (candidateIdx != selfInstance && covered) {
            hitInstanceIdx = candidateIdx;
            hitPrimitiveIdx = candidatePrim;
            hitBary = candidateBary;
            hitUV = candidateUV;
            hitBase = candidateBase;
            travelled += candidateT;
            break;
        }

        float advance = candidateT + 0.1;
        travelled += advance;
        remaining -= advance;
        if (remaining <= 0.05) break;
        rayOrigin += direction * advance;
    }

    if (hitInstanceIdx < 0) {
        // Miss — return sky tint / ambient mix.
        //
        // For exterior cells the ray escaping the BVH IS escaping into
        // real sky, so the half-sky half-ambient blend mirrors what
        // the composite paints behind the world (via #925's skyTint
        // plumbing).
        //
        // For interior cells the half-sky term is wrong: when
        // `SkyParamsRes` is absent (sealed interior, or no exterior
        // load yet this session), `build_sky_params` returns
        // `SkyParams::default()` with `zenith_color = [0.15, 0.3, 0.6]`
        // (clear-noon-blue) — that signal bleeds into glass refractions
        // / reflections as a daylight tint even in fully sealed cells
        // (Megaton, Vault 21, Markarth subterranean rooms). Drop to
        // cell ambient alone (`sceneFlags.yzw`) on the interior path.
        // The pre-#925 comment claiming "skyTint reads as the cell's
        // ceiling colour" was stale wisdom — interior cells get the
        // default zenith, not a per-cell ceiling derivation. See #1125 /
        // REN-D9-NEW-01.
        return vec4(missCol, 0.0);
    }

    // Look up the committed surface. Transparent alpha holes and the source
    // reflector itself were skipped by the bounded traversal above.
    GpuInstance hitInst = instances[hitInstanceIdx];
    GpuMaterial hitMat = materials[hitInst.materialId];

    // Sample the hit surface's texture × its canonical avgAlbedo (material
    // diffuse_color). The texture alone is the neutral white fallback for
    // untextured / vertex-coloured surfaces, so without the avgAlbedo
    // factor a metal/glass reflection of the Cornell red/green walls reads
    // as flat white. avgAlbedo is the white tint for textured content, so
    // detail is preserved there. Mirrors the refraction-colour fix.
    // `mipBias` softens the reflected image for rough surfaces — a
    // DETERMINISTIC pre-filtered-radiance blur in place of a stochastic
    // GGX-cone jitter, so rough-metal reflections carry no per-frame
    // sampling noise (the caller passes roughness-scaled mip and a sharp
    // reflection ray). Smooth surfaces pass mipBias 0 → razor-sharp.
    vec3 hitBaseRgb = sampleRayHitBase(hitInst, hitMat, hitUV, mipBias).rgb;
    vec3 hitColor = rayHitAlbedo(hitMat, hitBaseRgb);

    float hitDist = travelled;
    vec3 hitPos = origin + direction * hitDist;
    vec3 hitN = getHitTriNormal(uint(hitInstanceIdx), uint(hitPrimitiveIdx));
    if (dot(hitN, direction) > 0.0) hitN = -hitN;
    vec3 hitIrradiance = reflectionHitIrradiance(
        hitPos, hitN, floatBitsToUint(jitter.z));
    vec3 hitEmissive = rayHitEmission(hitMat, hitUV, hitBaseRgb, mipBias);
    vec3 hitRadiance = hitColor
        * (hitIrradiance * (1.0 / 3.14159265359) + sceneFlags.yzw)
        + hitEmissive;

    // Exponential distance attenuation: distant reflection detail fades into
    // ambient rather than persisting at near-full strength.
    float distFade = exp(-hitDist * 0.0015);

    // Fade distant surface detail into the correct miss radiance. Fading to
    // black made long indoor reflection rays look like dark transparency.
    return vec4(mix(missCol, hitRadiance, distFade), 1.0);
}
