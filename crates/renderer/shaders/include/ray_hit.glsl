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
