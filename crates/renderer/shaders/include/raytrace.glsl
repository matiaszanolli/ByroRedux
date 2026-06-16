// RT reflection helpers — hit UV / hit normal lookup + traceReflection
//
// NON-STANDALONE shader fragment. Included by triangle.frag in dependency
// order via GL_GOOGLE_include_directive; it references symbols (structs,
// SSBO/UBO bindings, helper functions, constants) defined in shader_constants.glsl
// and in earlier includes. Do not compile on its own.

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
// Uses gl_RayFlagsTerminateOnFirstHitEXT — we only need ANY opaque hit
// (the first one becomes the reflection surface). Without the flag the
// driver pays "find closest hit" cost across the full maxDist=5000 unit
// reach. Fix #420.
vec4 traceReflection(vec3 origin, vec3 direction, float maxDist, float mipBias,
                     int selfInstance) {
    rayQueryEXT rq;
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
    rayQueryInitializeEXT(
        rq, topLevelAS,
        gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFF,
        origin, 0.05, direction, maxDist
    );
    rayQueryProceedEXT(rq);

    if (rayQueryGetIntersectionTypeEXT(rq, true) == gl_RayQueryCommittedIntersectionNoneEXT) {
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

    // Hit — get SSBO instance index via custom index (encodes the draw
    // command position, which matches the SSBO layout). InstanceId would
    // give the TLAS-internal index, which diverges when some meshes lack BLAS.
    int hitInstanceIdx = rayQueryGetIntersectionInstanceCustomIndexEXT(rq, true);
    // Self-intersection rejection (Issue C). At grazing incidence the
    // reflected ray goes nearly tangent to a curved glass surface and
    // re-hits the originating instance's own mesh — a convex surface can't
    // legitimately reflect itself, so this hit is the self-intersection
    // that paints the tessellation moire on the glass rim. Treat it as a
    // miss (ambient) instead of sampling the glass's own surface. Scale-
    // independent (no epsilon to tune): keyed on instance identity.
    if (selfInstance >= 0 && hitInstanceIdx == selfInstance) {
        return vec4(missCol, 0.0);
    }
    int hitPrimitiveIdx = rayQueryGetIntersectionPrimitiveIndexEXT(rq, true);
    vec2 hitBary = rayQueryGetIntersectionBarycentricsEXT(rq, true);

    // Look up the hit surface's texture and UV.
    GpuInstance hitInst = instances[hitInstanceIdx];
    GpuMaterial hitMat = materials[hitInst.materialId];
    uint hitTexIdx = hitMat.textureIndex;
    vec2 hitUV = getHitUV(uint(hitInstanceIdx), uint(hitPrimitiveIdx), hitBary);
    // #494 — apply the hit instance's own BGSM UV transform before
    // sampling. Each hit carries its own per-material offset/scale;
    // the primary path's `baseUV` transform doesn't propagate.
    // R1 Phase 6 — UV transform now lives on the material table.
    hitUV = hitUV * vec2(hitMat.uvScaleU, hitMat.uvScaleV)
          + vec2(hitMat.uvOffsetU, hitMat.uvOffsetV);

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
    vec3 hitColor = textureLod(textures[nonuniformEXT(hitTexIdx)], hitUV, mipBias).rgb
        * vec3(hitInst.avgAlbedoR, hitInst.avgAlbedoG, hitInst.avgAlbedoB);

    // Exponential distance attenuation: distant reflections gracefully fade
    // into ambient rather than persisting at near-full strength. The old
    // 1/(1+d*0.005) barely attenuated over the 5000-unit ray length. #320.
    float hitDist = rayQueryGetIntersectionTEXT(rq, true);
    float distFade = exp(-hitDist * 0.0015);

    return vec4(hitColor * distFade, 1.0);
}

