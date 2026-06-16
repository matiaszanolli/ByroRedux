// Directional-ambient cube sample, octahedral normal codec, noise / disk / hemisphere sampling, ortho basis
//
// NON-STANDALONE shader fragment. Included by triangle.frag in dependency
// order via GL_GOOGLE_include_directive; it references symbols (structs,
// SSBO/UBO bindings, helper functions, constants) defined in shader_constants.glsl
// and in earlier includes. Do not compile on its own.


// Sample the 6-axis directional ambient cube along surface normal `N`.
// Weights are the non-negative components of N on each cardinal axis,
// so a normal pointing straight up reads `dalcPosY` only (sky-fill);
// a normal pointing into a wall reads a mix of the lateral axes (cavity
// fill). Sum of weights = |N.x| + |N.y| + |N.z| ≥ 1 (unit normal),
// matching the typical 6-axis-cube convention from Halo / Frostbite —
// the per-axis values are authored knowing this weighting. Replaces the
// hand-tuned `AMBIENT_AO_FLOOR = 0.3` constant with a directionally-
// correct sample of the WTHR-authored hemisphere. See #993.
vec3 sampleDalcCube(vec3 N) {
    vec3 pw = max(N, vec3(0.0));
    vec3 nw = max(-N, vec3(0.0));
    return dalcPosX.xyz * pw.x + dalcNegX.xyz * nw.x
         + dalcPosY.xyz * pw.y + dalcNegY.xyz * nw.y
         + dalcPosZ.xyz * pw.z + dalcNegZ.xyz * nw.z;
}

// CLUSTER_TILES_X/Y, CLUSTER_SLICES_Z, CLUSTER_NEAR/FAR_FLOOR/FAR_FALLBACK
// from shader_constants.glsl. #628 — cluster-grid far plane sourced from
// CLMT fog_far (`screen.w`) at runtime; clamped to CLUSTER_FAR_FLOOR/FALLBACK.

const float PI = 3.14159265359;

// ── Octahedral normal encoding (Cigolle et al. 2014) ────────────────
// Encodes a unit normal into 2 components for RG16_SNORM storage.
// Saves 50% G-buffer bandwidth vs RGBA16_SNORM. See #275.
vec2 octEncode(vec3 n) {
    n /= (abs(n.x) + abs(n.y) + abs(n.z));
    if (n.z < 0.0) {
        n.xy = (1.0 - abs(n.yx)) * vec2(n.x >= 0.0 ? 1.0 : -1.0,
                                          n.y >= 0.0 ? 1.0 : -1.0);
    }
    return n.xy;
}

vec3 octDecode(vec2 e) {
    vec3 n = vec3(e.xy, 1.0 - abs(e.x) - abs(e.y));
    if (n.z < 0.0) {
        n.xy = (1.0 - abs(n.yx)) * vec2(n.x >= 0.0 ? 1.0 : -1.0,
                                          n.y >= 0.0 ? 1.0 : -1.0);
    }
    return normalize(n);
}

// ── Noise for stochastic shadow rays ────────────────────────────────

// Interleaved gradient noise (Jimenez 2014) — excellent spatial distribution,
// cheap to compute, and when seeded with frame counter gives temporally
// varying patterns that average to smooth penumbra over a few frames.
float interleavedGradientNoise(vec2 fragCoord, float frameCount) {
    vec3 magic = vec3(0.06711056, 0.00583715, 52.9829189);
    float shifted = fract(magic.z * fract(dot(fragCoord + frameCount * vec2(5.588238, 5.588238), magic.xy)));
    return shifted;
}

// Generate a 2D sample on a unit disk using concentric mapping.
// t1, t2 in [0,1] → (x,y) uniformly distributed on unit disk.
vec2 concentricDiskSample(float t1, float t2) {
    float r = sqrt(t1);
    float theta = 2.0 * PI * t2;
    return vec2(r * cos(theta), r * sin(theta));
}

// Build an orthonormal basis from a unit direction vector (for jittering
// the RT ray). Frisvad (2012), "Building an Orthonormal Basis from a 3D
// Unit Vector Without Normalization" — singularity-free everywhere except
// `dir.z = -1` exactly (which is not a plausible terrain normal in our
// Y-up Z-up→Y-up converted scene; the only place that would surface is a
// downward-facing reflection from a perfectly horizontal mirror, where
// the analytic flip-axis branch handles it).
//
// Pre-#574 the implementation was a `cross(up, dir)` with `up` toggling
// to `vec3(1,0,0)` when `abs(dir.y) >= 0.999`. The 0.999 threshold left
// a NaN window: a fragment whose normal is *exactly* `(0,1,0)` (every
// vertex on a flat terrain LAND quad and on horizontal platform meshes)
// fell on the `<` side, so `up = (0,1,0)` was crossed with itself,
// producing `(0,0,0)` and a NaN after `normalize`. The NaN tangent
// propagated into `cosineWeightedHemisphere`'s direction, fed
// `rayQueryInitializeEXT` with NaN, and the entire frame's GI on flat
// exterior cells (Tamriel, Wasteland, etc.) was undefined per the
// Vulkan RT spec.
//
// Frisvad's method has no degenerate near-pole and is branchless except
// for the sign-axis pick. See #574 (RT-2).
void buildOrthoBasis(vec3 dir, out vec3 tangent, out vec3 bitangent) {
    float sign_z = dir.z >= 0.0 ? 1.0 : -1.0;
    float a = -1.0 / (sign_z + dir.z);
    float b = dir.x * dir.y * a;
    tangent = vec3(1.0 + sign_z * dir.x * dir.x * a, sign_z * b, -sign_z * dir.x);
    bitangent = vec3(b, sign_z + dir.y * dir.y * a, -dir.y);
}

// ── Cosine-weighted hemisphere sampling for GI ─────────────────────

// Generate a cosine-weighted random direction in the hemisphere above N.
// u1, u2 in [0,1] — use noise functions seeded by fragCoord + frameCount.
vec3 cosineWeightedHemisphere(vec3 N, float u1, float u2) {
    float r = sqrt(u1);
    float theta = 2.0 * PI * u2;
    vec3 T, B;
    buildOrthoBasis(N, T, B);
    return normalize(T * (r * cos(theta)) + B * (r * sin(theta)) + N * sqrt(max(1.0 - u1, 0.0)));
}

