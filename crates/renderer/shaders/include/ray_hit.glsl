// Shared secondary-ray hit reconstruction and material sampling.
//
// NON-STANDALONE shader fragment. Included after `bindings.glsl` by
// triangle.frag and water.frag. It references the shared GpuInstance /
// GpuMaterial tables, global vertex/index buffers, bindless textures, and
// generated material/instance flag constants.

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

// World-space geometric (face) normal of a ray-query hit triangle, from
// its vertex POSITIONS (stride offset 0) transformed by the instance
// model matrix. Used by two-surface glass refraction to refract the ray a
// second time as it exits the glass back face.
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
        vec3 p0 = vec3(vertexData[b0], vertexData[b0 + 1], vertexData[b0 + 2]);
        vec3 p1 = vec3(vertexData[b1], vertexData[b1 + 1], vertexData[b1 + 2]);
        vec3 p2 = vec3(vertexData[b2], vertexData[b2 + 1], vertexData[b2 + 2]);
        vec3 wp0 = (hitInst.model * vec4(p0, 1.0)).xyz;
        vec3 wp1 = (hitInst.model * vec4(p1, 1.0)).xyz;
        vec3 wp2 = (hitInst.model * vec4(p2, 1.0)).xyz;
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
    if (mat.parallaxMapIndex == 0u
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
    float sampledHeight = textureLod(
        textures[nonuniformEXT(mat.parallaxMapIndex)],
        currentUV,
        0.0
    ).r;
    for (int i = 0; i < steps; ++i) {
        if (currentDepth >= sampledHeight) {
            break;
        }
        currentUV -= deltaUV;
        currentDepth += layerDepth;
        sampledHeight = textureLod(
            textures[nonuniformEXT(mat.parallaxMapIndex)],
            currentUV,
            0.0
        ).r;
    }

    vec2 prevUV = currentUV + deltaUV;
    float afterDepth = sampledHeight - currentDepth;
    float beforeDepth = textureLod(
        textures[nonuniformEXT(mat.parallaxMapIndex)],
        prevUV,
        0.0
    ).r - (currentDepth - layerDepth);
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
