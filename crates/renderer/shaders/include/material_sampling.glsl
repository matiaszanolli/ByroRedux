// Parallax occlusion mapping + normal-map perturbation
//
// NON-STANDALONE shader fragment. Included by triangle.frag in dependency
// order via GL_GOOGLE_include_directive; it references symbols (structs,
// SSBO/UBO bindings, helper functions, constants) defined in shader_constants.glsl
// and in earlier includes. Do not compile on its own.

// ── Parallax occlusion mapping ──────────────────────────────────────
//
// Standard step + linear-interpolate POM using screen-space derivatives
// (no vertex tangents needed). Height values live in `parallaxMapIdx`'s
// `.r` channel in [0,1]; the surface is displaced INWARD (along -N) as
// the view grazes, so the sampled UV slides along -viewTS.xy.
//
// Returns the displaced UV. When `parallaxMapIdx == 0` the caller
// short-circuits and this function is never entered.
//
// `heightScale` is typically 0.02–0.08 (Bethesda brickwork range);
// `maxPasses` is clamped to [4, 32] — below 4 the stair-step artifacts
// are visible, above 32 the per-pixel cost spikes without a quality
// benefit at typical FOV. Caller feeds `BSShaderPPLightingProperty.
// parallax_max_passes` (default 4) and `parallax_scale` (default 0.04),
// matching the Gamebryo runtime defaults. See #453.
vec2 parallaxDisplaceUV(
    vec2 uv,
    vec3 viewWorld,
    vec3 N,
    vec3 worldPos,
    uint parallaxMapIdx,
    float heightScale,
    float maxPasses
) {
    // Build TBN from screen-space derivatives (same recipe as
    // perturbNormal — keep the derivation identical so the tangent
    // basis is consistent across the two passes). The `screenSign`
    // multiplier on B encodes UV-mirror handedness so V_ts.y has
    // the same sign convention on mirrored shells as on non-mirrored
    // ones — otherwise parallax slide along V would be inverted on
    // every symmetrical mesh half. Companion fix to #1104; the
    // perturbNormal Path-2 site carries the same correction.
    vec3 dPdx = dFdx(worldPos);
    vec3 dPdy = dFdy(worldPos);
    vec2 dUVdx = dFdx(uv);
    vec2 dUVdy = dFdy(uv);
    vec3 T = normalize(dPdx * dUVdy.y - dPdy * dUVdx.y);
    vec3 B = normalize(dPdy * dUVdx.x - dPdx * dUVdy.x);
    float screenSign = sign(dUVdx.x * dUVdy.y - dUVdx.y * dUVdy.x);
    T = normalize(T - dot(T, N) * N);
    B = screenSign * cross(N, T);

    // View direction in tangent space. xy is the planar slide the
    // ray makes per unit of depth; z > 0 means we're looking into
    // the surface (which is always the case for back-face-culled
    // draws we parallax-map anyway).
    vec3 V_ts = vec3(dot(viewWorld, T), dot(viewWorld, B), dot(viewWorld, N));
    float vz = max(V_ts.z, 0.05);
    vec2 planarSlide = V_ts.xy / vz * heightScale;

    int steps = int(clamp(maxPasses, 4.0, 32.0));
    float layerDepth = 1.0 / float(steps);
    vec2 deltaUV = planarSlide / float(steps);

    vec2 currentUV = uv;
    float currentDepth = 0.0;
    float sampledHeight =
        texture(textures[nonuniformEXT(parallaxMapIdx)], currentUV).r;
    for (int i = 0; i < steps; ++i) {
        if (currentDepth >= sampledHeight) {
            break;
        }
        currentUV -= deltaUV;
        currentDepth += layerDepth;
        sampledHeight = texture(textures[nonuniformEXT(parallaxMapIdx)], currentUV).r;
    }

    // Linear interpolate against the previous layer for smoother
    // transitions — avoids visible stair-stepping when the step count
    // is low.
    vec2 prevUV = currentUV + deltaUV;
    float afterDepth = sampledHeight - currentDepth;
    float beforeDepth =
        texture(textures[nonuniformEXT(parallaxMapIdx)], prevUV).r
        - (currentDepth - layerDepth);
    float weight = afterDepth / (afterDepth - beforeDepth + 1e-6);
    return mix(currentUV, prevUV, clamp(weight, 0.0, 1.0));
}

// ── Normal mapping ──────────────────────────────────────────────────
//
// Samples the per-fragment normal map and rotates the tangent-space
// perturbation into world space. Two TBN-source paths:
//
//   1. **Authored vertex tangent** (#783 / M-NORMALS) — preferred.
//      `vertexTangent.xyz` carries the world-space tangent direction
//      from the NIF's authored data; `.w` carries the bitangent sign.
//      Reconstructed B as `sign × cross(N, T)` is smooth across mesh
//      boundaries because the authored tangent itself is per-vertex
//      smooth — no derivative discontinuity. This is the path
//      Bethesda content uses on every BSShaderPPLighting / BSLighting
//      mesh, and it eliminates the chrome-walls regression that
//      surfaced the prior screen-space-derivative reconstruction.
//
//   2. **Screen-space derivative fallback** — used when
//      `vertexTangent.xyz` has zero magnitude (no authored data —
//      synthetic content like the spinning cube, particle billboards,
//      or non-Bethesda assets). Reconstructs T/B from `dFdx/dFdy`
//      of `worldPos` + `uv`. Suffers from boundary discontinuities
//      that produced the chrome-walls regression on Bethesda content,
//      but acceptable on synthetic / particle content where mesh
//      boundaries are simpler. See revert chain at 8305456.

vec3 perturbNormal(vec3 N, vec3 worldPos, vec2 uv, uint normalMapIdx, vec4 vertexTangent) {
    // Sample normal map (tangent-space, [0,1] → [-1,1]).
    vec3 tangentNormal = texture(textures[nonuniformEXT(normalMapIdx)], uv).rgb;
    tangentNormal = tangentNormal * 2.0 - 1.0;
    // Reconstruct Z from XY. Bethesda normal maps (Skyrim+/FO4 standard)
    // ship as BC5_UNORM_BLOCK (DDS FourCC `ATI2` / `BC5U` / DX10
    // `DXGI_FORMAT_BC5_UNORM`) which encodes only X and Y; per Vulkan
    // spec the sampler returns `(Nx, Ny, 0, 1)` — Z is hardware-zeroed.
    // Pre-fix the `* 2.0 - 1.0` remap turned the zero into `-1`, so
    // every per-pixel normal pointed INTO the surface and every
    // lighting equation ran on an inverted basis. Effect was loudest
    // on high-frequency carvings (e.g. Dragonsreach panels) where the
    // X/Y magnitude is largest, but it shifted the lit colour of
    // every BC5-normal-mapped surface by a fixed amount.
    //
    // For genuine RGB-encoded normals (rare in Bethesda content but
    // permitted by the format) the stored Z is already ≈ +1 and this
    // reconstruction reproduces the same value within float precision.
    // The `max(0, …)` clamps over-saturated artistic normals
    // (Nx²+Ny² > 1) to a Z=0 fallback so the result stays in the
    // tangent plane rather than producing a NaN.
    tangentNormal.z = sqrt(max(0.0, 1.0 - dot(tangentNormal.xy, tangentNormal.xy)));

    // Path 1 — authored vertex tangent (#783 / M-NORMALS).
    if (dot(vertexTangent.xyz, vertexTangent.xyz) > 1e-4) {
        vec3 T = normalize(vertexTangent.xyz);
        // Re-orthogonalize T against N (Gram-Schmidt) so the per-
        // fragment N (vertex-interpolated, possibly different from
        // the authored per-vertex N at the same vertex due to
        // smoothing groups) doesn't break the right-angle invariant
        // the bitangent sign was authored against.
        T = normalize(T - dot(T, N) * N);
        vec3 B = vertexTangent.w * cross(N, T);
        mat3 TBN = mat3(T, B, N);
        return normalize(TBN * tangentNormal);
    }

    // Path 2 — screen-space derivative fallback (no authored tangent).
    vec3 dPdx = dFdx(worldPos);
    vec3 dPdy = dFdy(worldPos);
    vec2 dUVdx = dFdx(uv);
    vec2 dUVdy = dFdy(uv);

    // Solve the linear system for T and B.
    vec3 T = normalize(dPdx * dUVdy.y - dPdy * dUVdx.y);
    vec3 B = normalize(dPdy * dUVdx.x - dPdx * dUVdy.x);

    // Ensure TBN is right-handed relative to N. The `cross(N, T)`
    // reconstruction loses the sign of the UV-Jacobian determinant
    // — i.e. on UV-mirrored shells (any symmetrical mesh half) the
    // bitangent would always come out right-handed, flipping the
    // tangent-space normal across the mirror seam. Multiply by the
    // determinant sign so B follows the authored +V direction in
    // both orientations. Path-1 (authored tangent) carries this sign
    // explicitly via `vertexTangent.w`; this is the Path-2 analog.
    // See #1104 (REN-D16-002). Critical for every Starfield mesh
    // since SF BSGeometry tangents land empty until #1086 lands a
    // tangent extractor, so every SF mesh reaches this fallback.
    float screenSign = sign(dUVdx.x * dUVdy.y - dUVdx.y * dUVdy.x);
    T = normalize(T - dot(T, N) * N);
    B = screenSign * cross(N, T);

    mat3 TBN = mat3(T, B, N);
    return normalize(TBN * tangentNormal);
}

