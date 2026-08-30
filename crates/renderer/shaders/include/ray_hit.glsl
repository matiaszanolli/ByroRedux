// Shared secondary-ray hit reconstruction and material sampling.
//
// NON-STANDALONE shader fragment. Included after `bindings.glsl` by
// triangle.frag and water.frag. It references the shared GpuInstance /
// GpuMaterial tables, global vertex/index buffers, bindless textures, and
// generated material/instance flag constants.

#include "ray_origin.glsl"

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

    // Barycentric interpolation: bary.x = u (vertex 1), bary.y = v
    // (vertex 2), w = 1-u-v (vertex 0).
    float w = 1.0 - barycentrics.x - barycentrics.y;
    return w * uv0 + barycentrics.x * uv1 + barycentrics.y * uv2;
}

// Vertex alpha is coverage whenever NiAlphaProperty is present, regardless
// of the shader's Vertex Colors / Vertex Alpha bits. The primary raster path
// therefore always multiplies `fragColor.a`; secondary rays must reconstruct
// the same barycentric value or alpha-tested leaves/grates cast a different
// silhouette from the visible surface.
float getHitVertexAlpha(
    uint instanceIdx, uint primitiveIdx, vec2 barycentrics
) {
    GpuInstance hitInst = instances[instanceIdx];
    uint vOff = hitInst.vertexOffset;
    uint iOff = hitInst.indexOffset;
    uint i0 = indexData[iOff + primitiveIdx * 3 + 0];
    uint i1 = indexData[iOff + primitiveIdx * 3 + 1];
    uint i2 = indexData[iOff + primitiveIdx * 3 + 2];
    float a0 = vertexData[(vOff + i0) * VERTEX_STRIDE_FLOATS
        + VERTEX_COLOR_OFFSET_FLOATS + 3u];
    float a1 = vertexData[(vOff + i1) * VERTEX_STRIDE_FLOATS
        + VERTEX_COLOR_OFFSET_FLOATS + 3u];
    float a2 = vertexData[(vOff + i2) * VERTEX_STRIDE_FLOATS
        + VERTEX_COLOR_OFFSET_FLOATS + 3u];
    float w = 1.0 - barycentrics.x - barycentrics.y;
    return w * a0 + barycentrics.x * a1 + barycentrics.y * a2;
}

// ABSOLUTE world-space positions of a ray-query hit triangle's three
// vertices. Both branches below return the same frame — see #2747
// (REN-D10-02): pre-fix the rigid branch returned render-origin-RELATIVE
// positions (`hi.model` has been render-origin-relative since the
// markarth cascade's `rebase_model_matrix`) while the skinned branch
// returned ABSOLUTE (`skin_vertices.comp`'s output, same convention
// `tlas_instance_transform` relies on). No wrong pixels resulted — every
// consumer (`getHitTriNormal`, `getRayHitTangentFrame`) differences the
// three positions, and a uniform per-triangle origin offset cancels —
// but the mixed convention was a latent trap for any future absolute
// consumer (re-projection, distance-to-camera, world-space hash,
// second-bounce ray origin), which would get a `renderOrigin`-sized
// displacement on rigid geometry only. Fixed by lifting the rigid branch
// with `+ renderOrigin.xyz`, matching the function's name/header
// contract rather than changing it.
//
// REN-2026-07-28-02 / #2219 — for a skinned instance (boneOffset != 0)
// the BLAS follows the DEFORMED geometry, but before that fix every
// caller reconstructed positions from the BIND-POSE global vertex SSBO
// transformed by the entity's root `model` matrix — correct topology,
// wrong pose. `skin_vertices.comp` writes already-deformed, already-
// absolute-world positions (position-only, SKIN_OUTPUT_STRIDE_FLOATS
// floats/vertex) to a per-entity buffer whose GPU address is threaded
// through `GpuInstance.skinnedVertexAddress`; dereference that instead
// of the bind-pose path when it's set, and skip the model-matrix
// multiply (the deformed positions are already absolute-world — same
// convention `tlas_instance_transform` already uses to emit an identity
// TLAS transform for skinned instances). `i0`/`i1`/`i2` from the shared
// global index buffer are already mesh-local (0-based) — the same
// numbering `skin_vertices.comp`'s output buffer uses — so no offset
// subtraction is needed, only no `+ vOff`.
//
// Residual limitation: this fixes the GEOMETRIC (face) normal/tangent
// reconstruction, which only needs positions. The INTERPOLATED smooth
// normal/tangent in `getRayHitTangentFrame` still reads authored
// per-vertex attributes from the bind-pose buffer — `skin_vertices.comp`
// writes position only, so a deformed smooth normal/tangent isn't
// available without either widening that output buffer or re-deriving
// the skin transform per-vertex from the bone palette in this shader,
// neither of which this fix attempts.
void getHitTriWorldPositions(
    uint instanceIdx,
    uint primitiveIdx,
    out vec3 w0,
    out vec3 w1,
    out vec3 w2
) {
    GpuInstance hi = instances[instanceIdx];
    uint iOff = hi.indexOffset;
    uint i0 = indexData[iOff + primitiveIdx * 3 + 0];
    uint i1 = indexData[iOff + primitiveIdx * 3 + 1];
    uint i2 = indexData[iOff + primitiveIdx * 3 + 2];
    if (hi.boneOffset != 0u && hi.skinnedVertexAddress != 0ul) {
        SkinnedVertexRef ref = SkinnedVertexRef(hi.skinnedVertexAddress);
        uint p0 = i0 * SKIN_OUTPUT_STRIDE_FLOATS;
        uint p1 = i1 * SKIN_OUTPUT_STRIDE_FLOATS;
        uint p2 = i2 * SKIN_OUTPUT_STRIDE_FLOATS;
        w0 = vec3(ref.data[p0], ref.data[p0 + 1], ref.data[p0 + 2]);
        w1 = vec3(ref.data[p1], ref.data[p1 + 1], ref.data[p1 + 2]);
        w2 = vec3(ref.data[p2], ref.data[p2 + 1], ref.data[p2 + 2]);
        return;
    }
    uint vOff = hi.vertexOffset;
    uint p0 = (vOff + i0) * VERTEX_STRIDE_FLOATS;
    uint p1 = (vOff + i1) * VERTEX_STRIDE_FLOATS;
    uint p2 = (vOff + i2) * VERTEX_STRIDE_FLOATS;
    vec3 v0 = vec3(vertexData[p0], vertexData[p0 + 1], vertexData[p0 + 2]);
    vec3 v1 = vec3(vertexData[p1], vertexData[p1 + 1], vertexData[p1 + 2]);
    vec3 v2 = vec3(vertexData[p2], vertexData[p2 + 1], vertexData[p2 + 2]);
    // #2747 — `hi.model` is render-origin-RELATIVE (rebase_model_matrix
    // subtracts renderOrigin on the CPU); lift to ABSOLUTE here so this
    // branch matches the skinned branch above and the function's
    // documented contract, instead of leaving the caller to discover the
    // mismatch only when it stops differencing the result.
    w0 = (hi.model * vec4(v0, 1.0)).xyz + renderOrigin.xyz;
    w1 = (hi.model * vec4(v1, 1.0)).xyz + renderOrigin.xyz;
    w2 = (hi.model * vec4(v2, 1.0)).xyz + renderOrigin.xyz;
}

// World-space geometric (face) normal of a ray-query hit triangle. Used by
// two-surface glass refraction to refract the ray a second time as it
// exits the glass back face, and by `getRayHitTangentFrame`'s flat-shading
// and degenerate-normal fallbacks.
vec3 getHitTriNormal(uint instanceIdx, uint primitiveIdx) {
    vec3 w0, w1, w2;
    getHitTriWorldPositions(instanceIdx, primitiveIdx, w0, w1, w2);
    return normalize(cross(w1 - w0, w2 - w0));
}

// Shared material sampling for every secondary-ray path. Keeping these rules
// here prevents reflection, refraction, GI, water, and shadow traversal from
// each inventing a different meaning for diffuse alpha or glow maps.
vec2 transformRayHitUV(GpuMaterial mat, vec2 uv) {
    return uv * vec2(mat.uvScaleU, mat.uvScaleV)
         + vec2(mat.uvOffsetU, mat.uvOffsetV);
}

// Reconstruct the same view-facing tangent frame used by primary POM, but
// from committed-hit barycentrics instead of fragment derivatives. Authored
// tangents are preferred; the triangle UV gradient is the fallback for
// synthetic meshes that carry a zero tangent.
bool getRayHitTangentFrame(
    uint instanceIdx,
    uint primitiveIdx,
    vec2 barycentrics,
    vec3 viewWorld,
    GpuMaterial mat,
    out vec3 T,
    out vec3 B,
    out vec3 N
) {
    GpuInstance hitInst = instances[instanceIdx];
    uint vOff = hitInst.vertexOffset;
    uint iOff = hitInst.indexOffset;
    uint i0 = indexData[iOff + primitiveIdx * 3 + 0];
    uint i1 = indexData[iOff + primitiveIdx * 3 + 1];
    uint i2 = indexData[iOff + primitiveIdx * 3 + 2];
    uint b0 = (vOff + i0) * VERTEX_STRIDE_FLOATS;
    uint b1 = (vOff + i1) * VERTEX_STRIDE_FLOATS;
    uint b2 = (vOff + i2) * VERTEX_STRIDE_FLOATS;
    float w = 1.0 - barycentrics.x - barycentrics.y;

    vec3 n0 = vec3(
        vertexData[b0 + VERTEX_NORMAL_OFFSET_FLOATS],
        vertexData[b0 + VERTEX_NORMAL_OFFSET_FLOATS + 1],
        vertexData[b0 + VERTEX_NORMAL_OFFSET_FLOATS + 2]);
    vec3 n1 = vec3(
        vertexData[b1 + VERTEX_NORMAL_OFFSET_FLOATS],
        vertexData[b1 + VERTEX_NORMAL_OFFSET_FLOATS + 1],
        vertexData[b1 + VERTEX_NORMAL_OFFSET_FLOATS + 2]);
    vec3 n2 = vec3(
        vertexData[b2 + VERTEX_NORMAL_OFFSET_FLOATS],
        vertexData[b2 + VERTEX_NORMAL_OFFSET_FLOATS + 1],
        vertexData[b2 + VERTEX_NORMAL_OFFSET_FLOATS + 2]);
    vec3 localN = w * n0 + barycentrics.x * n1 + barycentrics.y * n2;

    mat3 model3 = mat3(hitInst.model);
    vec3 worldN;
    if ((hitInst.flags & INSTANCE_FLAG_FLAT_SHADING) != 0u) {
        worldN = getHitTriNormal(instanceIdx, primitiveIdx);
    } else if ((hitInst.flags & INSTANCE_FLAG_NON_UNIFORM_SCALE) != 0u) {
        float det = determinant(model3);
        worldN = abs(det) > 1e-6
            ? transpose(inverse(model3)) * localN
            : model3 * localN;
    } else {
        worldN = model3 * localN;
    }
    if (dot(worldN, worldN) < 1e-8) {
        worldN = getHitTriNormal(instanceIdx, primitiveIdx);
    }
    N = normalize(worldN);
    if (dot(N, viewWorld) < 0.0) {
        N = -N;
    }

    vec4 t0 = vec4(
        vertexData[b0 + VERTEX_TANGENT_OFFSET_FLOATS],
        vertexData[b0 + VERTEX_TANGENT_OFFSET_FLOATS + 1],
        vertexData[b0 + VERTEX_TANGENT_OFFSET_FLOATS + 2],
        vertexData[b0 + VERTEX_TANGENT_OFFSET_FLOATS + 3]);
    vec4 t1 = vec4(
        vertexData[b1 + VERTEX_TANGENT_OFFSET_FLOATS],
        vertexData[b1 + VERTEX_TANGENT_OFFSET_FLOATS + 1],
        vertexData[b1 + VERTEX_TANGENT_OFFSET_FLOATS + 2],
        vertexData[b1 + VERTEX_TANGENT_OFFSET_FLOATS + 3]);
    vec4 t2 = vec4(
        vertexData[b2 + VERTEX_TANGENT_OFFSET_FLOATS],
        vertexData[b2 + VERTEX_TANGENT_OFFSET_FLOATS + 1],
        vertexData[b2 + VERTEX_TANGENT_OFFSET_FLOATS + 2],
        vertexData[b2 + VERTEX_TANGENT_OFFSET_FLOATS + 3]);
    vec4 localTangent = w * t0
        + barycentrics.x * t1
        + barycentrics.y * t2;
    vec3 worldT = model3 * localTangent.xyz;
    float tangentSign = localTangent.w < 0.0 ? -1.0 : 1.0;

    if (dot(worldT, worldT) < 1e-8) {
        // REN-2026-07-28-02 / #2219 — deformed positions for skinned
        // instances, matching getHitTriNormal above (UVs are pose-
        // invariant, so they stay on the bind-pose read below).
        vec3 wp0, wp1, wp2;
        getHitTriWorldPositions(instanceIdx, primitiveIdx, wp0, wp1, wp2);
        vec2 uv0 = transformRayHitUV(
            mat,
            vec2(vertexData[b0 + VERTEX_UV_OFFSET_FLOATS],
                 vertexData[b0 + VERTEX_UV_OFFSET_FLOATS + 1]));
        vec2 uv1 = transformRayHitUV(
            mat,
            vec2(vertexData[b1 + VERTEX_UV_OFFSET_FLOATS],
                 vertexData[b1 + VERTEX_UV_OFFSET_FLOATS + 1]));
        vec2 uv2 = transformRayHitUV(
            mat,
            vec2(vertexData[b2 + VERTEX_UV_OFFSET_FLOATS],
                 vertexData[b2 + VERTEX_UV_OFFSET_FLOATS + 1]));
        vec3 edge1 = wp1 - wp0;
        vec3 edge2 = wp2 - wp0;
        vec2 duv1 = uv1 - uv0;
        vec2 duv2 = uv2 - uv0;
        float uvDet = duv1.x * duv2.y - duv1.y * duv2.x;
        if (abs(uvDet) < 1e-8) {
            return false;
        }
        float invDet = 1.0 / uvDet;
        worldT = (edge1 * duv2.y - edge2 * duv1.y) * invDet;
        vec3 worldB = (edge2 * duv1.x - edge1 * duv2.x) * invDet;
        tangentSign = dot(cross(N, worldT), worldB) < 0.0 ? -1.0 : 1.0;
    }

    worldT -= dot(worldT, N) * N;
    if (dot(worldT, worldT) < 1e-8) {
        return false;
    }
    T = normalize(worldT);
    B = tangentSign * cross(N, T);
    return true;
}

// Resolve the material UV visible at a secondary ray hit. The BVH remains
// the authored triangle surface (POM cannot change silhouettes or the ray
// intersection), but every material lookup at that hit now follows the same
// height field as the primary raster surface.
vec2 resolveRayHitUV(
    uint instanceIdx,
    uint primitiveIdx,
    vec2 barycentrics,
    vec3 rayDirection,
    GpuMaterial mat
) {
    vec2 uv = transformRayHitUV(
        mat, getHitUV(instanceIdx, primitiveIdx, barycentrics));
    uint dbgFlags = floatBitsToUint(jitter.z);
    // #3530 — bit 31 selects the height CHANNEL and is not part of the index.
    // Masking is mandatory here, not cosmetic: sampling
    // `textures[0x8000000N]` would be a wildly out-of-bounds bindless index.
    uint parallaxIdx = mat.parallaxMapIndex & ~PARALLAX_ALPHA_HEIGHT_BIT;
    bool heightInAlpha =
        (mat.parallaxMapIndex & PARALLAX_ALPHA_HEIGHT_BIT) != 0u;
    if (parallaxIdx == 0u
        || mat.parallaxHeightScale <= 0.0
        || (dbgFlags & DBG_BYPASS_POM) != 0u) {
        return uv;
    }

    vec3 T;
    vec3 B;
    vec3 N;
    vec3 viewWorld = normalize(-rayDirection);
    if (!getRayHitTangentFrame(
            instanceIdx, primitiveIdx, barycentrics, viewWorld, mat, T, B, N)) {
        return uv;
    }

    vec3 viewTangent = vec3(
        dot(viewWorld, T),
        dot(viewWorld, B),
        dot(viewWorld, N));
    float vz = max(viewTangent.z, 0.05);
    vec2 planarSlide =
        viewTangent.xy / vz * mat.parallaxHeightScale;

    // Secondary paths can visit several surfaces per primary pixel. Bound
    // their march to twelve layers and spend that budget near grazing
    // angles; the common four-pass legacy material remains exactly four.
    float layerBudget = clamp(mat.parallaxMaxPasses, 4.0, 12.0);
    float layerFloor = min(layerBudget, 4.0);
    int steps = int(mix(
        layerBudget,
        layerFloor,
        clamp(viewTangent.z, 0.0, 1.0)
    ) + 0.5);
    float layerDepth = 1.0 / float(steps);
    vec2 deltaUV = planarSlide / float(steps);
    vec2 currentUV = uv;
    float currentDepth = 0.0;
    vec4 heightTexel =
        textureLod(textures[nonuniformEXT(parallaxIdx)], currentUV, 0.0);
    float sampledHeight = heightInAlpha ? heightTexel.a : heightTexel.r;
    for (int i = 0; i < steps; ++i) {
        if (currentDepth >= sampledHeight) {
            break;
        }
        currentUV -= deltaUV;
        currentDepth += layerDepth;
        heightTexel =
            textureLod(textures[nonuniformEXT(parallaxIdx)], currentUV, 0.0);
        sampledHeight = heightInAlpha ? heightTexel.a : heightTexel.r;
    }

    vec2 prevUV = currentUV + deltaUV;
    float afterDepth = sampledHeight - currentDepth;
    vec4 prevTexel =
        textureLod(textures[nonuniformEXT(parallaxIdx)], prevUV, 0.0);
    float beforeDepth = (heightInAlpha ? prevTexel.a : prevTexel.r)
        - (currentDepth - layerDepth);
    float weight = afterDepth / (afterDepth - beforeDepth + 1e-6);
    return mix(currentUV, prevUV, clamp(weight, 0.0, 1.0));
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
    uint instanceIdx,
    uint primitiveIdx,
    vec2 barycentrics,
    GpuInstance inst,
    GpuMaterial mat,
    vec2 uv,
    out vec4 baseSample
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
    alpha *= mat.materialAlpha
        * getHitVertexAlpha(instanceIdx, primitiveIdx, barycentrics);
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
