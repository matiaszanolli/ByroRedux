// PBR — GGX / anisotropic GGX, Smith geometry, Fresnel, Disney diffuse split, specular AA
//
// NON-STANDALONE shader fragment. Included by triangle.frag in dependency
// order via GL_GOOGLE_include_directive; it references symbols (structs,
// SSBO/UBO bindings, helper functions, constants) defined in shader_constants.glsl
// and in earlier includes. Do not compile on its own.

// ── PBR: GGX / Cook-Torrance BRDF ──────────────────────────────────
//
// #2811 (REN-D17-09) — the `knightcrawler25/GLSL-PathTracer` (MIT)
// file:line citations throughout this file are NOT verifiable offline:
// that repo is not present under `/mnt/data/src/reference/` (which
// holds Champollion, Gibbed.Starfield, Material-Editor, gamebryo-v26/
// v32, havok-20070919/2013, nifly, nifxml, openmw). The line ranges
// were accurate when transcribed and are kept as provenance, but they
// cannot be re-checked until the repo is re-cloned — so treat the
// Rust-side regression tests as the authority on these formulas, not
// the citations. Same caveat on `vulkan/material.rs`'s `presets`.

// Normal Distribution Function (GGX/Trowbridge-Reitz).
float distributionGGX(float NdotH, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float denom = NdotH * NdotH * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom);
}

// Anisotropic GGX (Disney convention) — Trowbridge-Reitz with
// independent roughness along the tangent (ax) and bitangent (ay)
// axes. Drives directional specular streak on hair, brushed metal,
// vinyl, satin. Reduces exactly to `distributionGGX` when ax == ay
// (the legacy isotropic case the default ax = ay = roughness path
// hits). Reference: knightcrawler25/GLSL-PathTracer (MIT)
// `src/shaders/common/sampling.glsl:90-95` — `GTR2Aniso`.
//
// `HdotX` and `HdotY` are the half-vector projections onto the
// surface tangent and bitangent; combined with `NdotH` they form
// the H-vector's full tangent-space coordinates. Caller computes
// these from the world-space H + the per-fragment tangent /
// bitangent. See #1250.
float distributionGGXAniso(float NdotH, float HdotX, float HdotY, float ax, float ay) {
    float ax2 = ax * ax;
    float ay2 = ay * ay;
    float denom = HdotX * HdotX / ax2 + HdotY * HdotY / ay2 + NdotH * NdotH;
    return 1.0 / (PI * ax * ay * denom * denom);
}

// Derive Disney ax / ay from per-material perceptual roughness +
// anisotropic strength. `aspect = sqrt(1 - anisotropic * 0.9)` caps
// the lobe stretch at sqrt(0.1) so anisotropic = 1 doesn't produce
// a fully degenerate needle (Disney convention).
//
// Convention sync: `distributionGGX` (above) computes
// `a = roughness²` internally and feeds `a² = α²` into the NDF
// denominator — i.e. α (linear GGX roughness) = roughness² in this
// shader's convention. To stay byte-identical with the isotropic
// path when `anisotropic = 0`, we apply the same `roughness²`
// remap here and let `distributionGGXAniso` consume the resulting
// `ax` / `ay` directly (its formula squares them as `ax*ax` /
// `ay*ay` so the final α² magnitude matches the isotropic NDF).
//
// 0.025² floor on `ax`/`ay` (α-units) mirrors `specularAaRoughness`'s
// effective `roughness ≥ 0.025` floor (its clamp lives in α² units,
// `0.025⁴`, since #2471 — same roughness floor, different stage of
// the round trip) — preserves the BSLightingShader gloss-cap behaviour
// documented at that helper. The audit's "drop to 0.001" suggestion
// is deferred pending a RenderDoc bench on extreme-gloss materials
// (see #1250 closeout).
//
// See #1250 / GLSL-PathTracer `pathtrace.glsl:100-102` (MIT).
void deriveAxAy(float roughness, float anisotropic, out float ax, out float ay) {
    float alpha = roughness * roughness; // shader convention: α = roughness²
    // #1254 — defense-in-depth: clamp anisotropic to [0, 1] before the
    // sqrt. A future BGSM v9+ / Starfield .mat importer that ships an
    // unclamped authored value > 1.0 would otherwise give
    // `sqrt(1 - 0.9·a) < 0` → NaN, propagating through ax/ay into
    // distributionGGXAniso → black/undefined fragment. < 0 inputs
    // shrink ax below the intended floor — same single-line guard.
    float aniso = clamp(anisotropic, 0.0, 1.0);
    float aspect = sqrt(1.0 - aniso * 0.9);
    ax = max(0.025 * 0.025, alpha / aspect);
    ay = max(0.025 * 0.025, alpha * aspect);
}

// Geometry function (Smith's Schlick-GGX).
float geometrySmith(float NdotV, float NdotL, float roughness) {
    float r = roughness + 1.0;
    float k = (r * r) / 8.0;
    float g1v = NdotV / (NdotV * (1.0 - k) + k);
    float g1l = NdotL / (NdotL * (1.0 - k) + k);
    return g1v * g1l;
}

// Fifth-power weight shared by every Schlick-shaped lobe. Express the fixed
// exponent as three multiplies instead of GLSL `pow`: the latter survives as
// a GLSL.std.450 Pow instruction in the shipped SPIR-V and can consume an SFU
// slot even though x^5 has this exact multiply-chain form.
float schlickWeight(float cosTheta) {
    float x = clamp(1.0 - cosTheta, 0.0, 1.0);
    float x2 = x * x;
    return x2 * x2 * x;
}

// Scalar form for dielectric glass. Avoids broadcasting F0 to vec3 only to
// discard two channels at the call site.
float fresnelSchlickScalar(float cosTheta, float F0) {
    float weight = schlickWeight(cosTheta);
    return F0 + (1.0 - F0) * weight;
}

// Fresnel (Schlick approximation).
vec3 fresnelSchlick(float cosTheta, vec3 F0) {
    float weight = schlickWeight(cosTheta);
    return F0 + (1.0 - F0) * weight;
}

// Derive Schlick F0 from a per-material refractive index. Pre-#1248
// every dielectric site hardcoded `vec3(0.04)` — the value F0 takes
// when η = 1.5 (soda-lime / generic dielectric default). Honouring
// per-material IOR makes water (η ≈ 1.33 → F0 ≈ 0.02), ice
// (η ≈ 1.31 → 0.018), polished stone (η ≈ 1.54 → 0.045), and
// gemstone-class surfaces (diamond η ≈ 2.42 → F0 ≈ 0.172) all
// representable. Reference: knightcrawler25/GLSL-PathTracer
// (MIT) `src/shaders/common/disney.glsl:56-57`. See #1248.
float dielectricF0FromIor(float eta) {
    // #1253 — defense-in-depth: clamp η > 0 so an importer-side bug
    // shipping uninitialized `mat.ior = 0` doesn't yield `F0 = 1.0`
    // (mirror-class) on what should be a dielectric. The 1e-3 floor
    // is below any physically-meaningful refractive index but above
    // the divide-by-zero / sign-flip regimes.
    float e = max(eta, 1e-3);
    float r = (1.0 - e) / (1.0 + e);
    return r * r;
}

// Disney diffuse lobe — Burley retro-reflection + Hanrahan-Krueger
// fake-SSS subsurface + sheen. Replaces plain Lambert for materials
// that author Disney-style fields (gated on `MAT_FLAG_PBR_BSDF` at
// the call site). Pre-#1249 every direct-light fragment used pure
// Lambert `albedo / PI` regardless of authored PBR data — cloth
// looked flat, sand had no edge brighten, skin / wax / marble
// missed the SSS approximation.
//
// **Split return** (#1252 / REN-D6-2026-05-24-01): the diffuse and
// sheen lobes have DIFFERENT scaling conventions — diffuse is /PI
// (Lambertian), sheen is NOT /PI (Disney 2012 spec: layered
// Fresnel-shaped highlight). The two call sites (fallback-directional
// and per-light loop) need to compose them with different PI scales
// because the per-light loop carries a `kD * albedo` (no /PI)
// legacy convention. The pre-#1252 form returned both in a single
// `vec3` so the per-light's compensating `* PI` over-amplified the
// sheen lobe by ~3.14×. Returning a struct makes the compositional
// shape explicit at every call site.
//
// Reference: knightcrawler25/GLSL-PathTracer (MIT)
// `src/shaders/common/disney.glsl:67-87` — `EvalDisneyDiffuse`.
//
//   albedo:       base colour (linear)
//   roughness:    perceptual roughness [0, 1]
//   subsurface:   [0, 1] mix factor between Burley diffuse and
//                 Hanrahan-Krueger fake-SSS (0 = pure Burley,
//                 1 = pure fake-SSS).
//   sheen:        [0, 1] strength of the Fresnel-weighted edge
//                 highlight (cloth / silk / velvet).
//   sheenTint:    [0, 1] interpolation between white sheen and
//                 base-colour-tinted sheen.
//   NdotL, NdotV, HdotL: precomputed cosines (already clamped).
//
// Output fields:
//   .diffuse: the Burley + HK diffuse value, already /PI'd.
//   .sheen:   the Fresnel-weighted sheen value, NOT /PI'd.
struct DisneyDiffuseSplit {
    vec3 diffuse;
    vec3 sheen;
};

DisneyDiffuseSplit disneyDiffuseSplit(
    vec3 albedo,
    float roughness,
    float subsurface,
    float sheen,
    float sheenTint,
    float NdotL,
    float NdotV,
    float HdotL
) {
    // SchlickWeight = (1 - c)^5 — Fresnel-shaped grazing falloff.
    float FL = schlickWeight(NdotL);
    float FV = schlickWeight(NdotV);
    float FH = schlickWeight(HdotL);

    // Burley retro-reflection (rough-surface backscatter on
    // grazing-light angles — edge brightening on cloth / sand /
    // matte wood).
    float Rr = 2.0 * roughness * HdotL * HdotL;
    float Fretro = Rr * (FL + FV + FL * FV * (Rr - 1.0));

    // Pure-Burley diffuse falloff at grazing — energy-conserving.
    float Fd = (1.0 - 0.5 * FL) * (1.0 - 0.5 * FV);

    // Hanrahan-Krueger fake subsurface. Cheap SSS approximation
    // without a BSSRDF; visible on wax / marble / skin / leaves.
    // Guard against divide-by-zero on grazing-on-grazing pairs.
    float Fss90 = 0.5 * Rr;
    float Fss = mix(1.0, Fss90, FL) * mix(1.0, Fss90, FV);
    float ss = 1.25 * (Fss * (1.0 / max(NdotL + NdotV, 1e-4) - 0.5) + 0.5);

    // Sheen — Fresnel-weighted edge highlight, tinted between white and a
    // luminance-normalised base-colour tint. Layered on top of the diffuse
    // lobe. #2819 (REN-D17-05) — both cited references (Disney 2012
    // `disney.brdf`'s `Ctint = baseColor / Cdlum`, and
    // knightcrawler25/GLSL-PathTracer's `GetSpecColor`, which this
    // function's doc block names as its reference) normalise by luminance
    // BEFORE mixing, so `sheenTint` transfers hue only, not intensity.
    // Mixing in raw `albedo` instead coupled the two: a dark base colour
    // (e.g. black velvet — the canonical sheen material) scaled the whole
    // sheen lobe down at `sheenTint = 1.0` instead of just tinting it.
    // Weights are the Disney-paper luminance coefficients (0.3, 0.6, 0.1)
    // — deliberately NOT `pathLuminance`'s Rec.709 weights above, which
    // are for the path-tracer's variance estimator, not this BRDF term.
    float sheenLuminance = dot(albedo, vec3(0.3, 0.6, 0.1));
    vec3 sheenTintColor = sheenLuminance > 0.0 ? albedo / sheenLuminance : vec3(1.0);
    vec3 sheenColor = mix(vec3(1.0), sheenTintColor, sheenTint);

    DisneyDiffuseSplit o;
    o.diffuse = albedo * mix(Fd + Fretro, ss, subsurface) * (1.0 / PI);
    o.sheen = FH * sheen * sheenColor;
    return o;
}

// Specular antialiasing — Kaplanyan & Hoffman 2016
// ("Stable Geometric Specular Antialiasing With Projected-Space NDF
// Filtering", Siggraph Talks). At distance, a single fragment can
// cover many normal-map periods (corrugated metal, brick mortar,
// fence cutouts, etc.). The plain GGX lobe stays narrow and adjacent
// pixels swing between bright specular hit and dark miss — the
// "soft lighting + distance" striping that read as a recurring bug
// class on Quonset / industrial interiors (Nellis Museum was the
// canonical regression).
//
// Estimate the per-fragment normal-vector variance from screen-space
// derivatives, then widen `α²` (NOT `α`, NOT `roughness²` — see #2471)
// by `2 × kernel_variance`, per the published filter: `α²_filtered =
// α² + 2σ²`. The lobe smears the bright/dark across pixels at exactly
// the rate the underlying normal aliases — converging back to the
// authored roughness on smooth surfaces (small variance) so
// close-range specular highlights stay sharp.
//
// #2471 — this shader's convention is `α = roughness²` (see
// `deriveAxAy` above / `distributionGGX`'s `a = roughness²`, `a2 =
// a²`). The pre-fix code added the kernel variance to `roughness²`
// (i.e. to `α`, not `α²`) and `sqrt`'d once; since callers square the
// return value again to derive `α`, the effective result was
// `α_filtered = α + 2σ²` — under-filtering low-roughness surfaces by
// roughly 4× at `roughness = 0.1` relative to the Kaplanyan & Hoffman
// 2016 / Filament `normalFiltering()` reference. Fixed by squaring
// `α` once more before adding the variance and taking the matching
// 4th root on the way out.
//
// Returns the filtered roughness (already round-tripped back to
// `roughness` units so the caller can pass it straight to
// [`distributionGGX`] / [`geometrySmith`] / [`deriveAxAy`], each of
// which squares it to `α` again). The floor is `0.025⁴` — the α²
// value that round-trips (via `sqrt(sqrt(..))`) back to the same
// `roughness ≥ 0.025` floor the pre-fix `0.025²` clamp on `α`
// enforced (mirrors what the BSLightingShader gloss path reaches at
// maximum gloss); the `min(.., 1.0)` upper bound is the GGX validity
// ceiling.
float specularAaRoughness(vec3 N, float roughness) {
    vec3 dNdx = dFdx(N);
    vec3 dNdy = dFdy(N);
    float kernelVariance = 0.25 * (dot(dNdx, dNdx) + dot(dNdy, dNdy));
    float alpha = roughness * roughness; // α = roughness²
    float alpha2 = alpha * alpha;        // α²
    float filteredAlpha2 = clamp(alpha2 + 2.0 * kernelVariance, 0.025 * 0.025 * 0.025 * 0.025, 1.0);
    return sqrt(sqrt(filteredAlpha2));
}

// Karis analytic split-sum environment BRDF (the Lazarov / Karis
// approximation). Returns the (scale, bias) pair such that the
// single-scattering environment response for specular reflectance F0 is
// `F0 · scale + bias`. This is the LUT-FREE form — no precomputed DFG
// texture, no tunable parameter.
//
// Reference: Karis, "Physically Based Shading on Mobile" (2014) —
// `EnvBRDFApprox`. Same form used by Frostbite / UE4 mobile.
vec2 envBRDFApprox(float NdotV, float roughness) {
    const vec4 c0 = vec4(-1.0, -0.0275, -0.572, 0.022);
    const vec4 c1 = vec4(1.0, 0.0425, 1.04, -0.04);
    vec4 r = roughness * c0 + c1;
    float a004 = min(r.x * r.x, exp2(-9.28 * NdotV)) * r.x + r.y;
    return vec2(-1.04, 1.04) * a004 + r.zw;
}

// Multi-scatter energy compensation (Fdez-Agüera 2019 / Filament §4.7.2).
//
// The single-scattering Cook-Torrance lobe `D·G·F / (4·NdotV·NdotL)` only
// accounts for light that leaves the microsurface after ONE bounce. Energy
// that bounces multiple times between microfacets before exiting is dropped,
// so rough conductors (brushed steel, satin, frosted metal, the kitchen
// cookware class) progressively DARKEN as roughness rises — visibly
// under-energized vs. ground truth, and the darker the rougher.
//
// We add that lost energy back with the standard analytic correction: a
// multiplicative boost applied to the single-scatter specular,
//
//     compensation = 1 + F0 · (1 / Ess − 1)
//
// where `Ess = scale + bias` is the directional albedo of the white-furnace
// microfacet specular (F0 = 1) — the fraction of energy surviving
// single-scatter masking, read from `envBRDFApprox` above.
//
// Well-behaved at both extremes by construction:
//   - roughness → 0  ⇒  Ess → 1  ⇒  compensation → 1 (smooth surfaces
//     are untouched; this CANNOT widen the roughness reflection gate or
//     re-trigger the reverted "chrome thugs" regression — it only scales
//     the existing rough lobe's magnitude).
//   - roughness → 1  ⇒  Ess < 1  ⇒  compensation > 1 (energy restored;
//     fully-rough metal recovers ~the half it loses to masking).
//   - F0 → 0         ⇒  compensation → 1 (never brightens a surface that
//     reflects nothing).
//
// Parameter-free and game-agnostic (both source methods are published) —
// no per-game branch, no authored data, no `feedback_no_guessing` exposure.
//
// References:
//   - Fdez-Agüera, "A Multiple-Scattering Microfacet Model for Real-Time
//     Image-Based Lighting" (JCGT 2019) §4.
//   - Filament documentation §4.7.2 "Energy compensation".
vec3 multiScatterEnergyCompensation(vec3 F0, float NdotV, float roughness) {
    vec2 ab = envBRDFApprox(NdotV, roughness);
    float Ess = ab.x + ab.y;
    return 1.0 + F0 * (1.0 / max(Ess, 1e-3) - 1.0);
}

// ── Bounded path-transport BSDF ─────────────────────────────────────
//
// Primary raster shading has derivatives, authored tangents, and all of the
// material-specific Disney extensions above. A ray-query hit has only its
// geometric normal, so secondary transport deliberately uses the isotropic
// core shared by every material: energy-conserving Lambert + GGX reflection.
// That is still materially different from the former path, which treated
// every non-glass hit — including polished conductors — as Lambertian.
//
// The GGX sampler follows Eric Heitz, "Sampling the GGX Distribution of
// Visible Normals" (JCGT 7(4), 2018). Sampling the visible-normal
// distribution avoids the below-surface rejection and high grazing-angle
// variance of raw NDF sampling. `roughness` remains perceptual roughness;
// the shader-wide convention is alpha = roughness².

float pathLuminance(vec3 value) {
    return dot(max(value, vec3(0.0)), vec3(0.2126, 0.7152, 0.0722));
}

float ggxSmithG1(float NdotW, float roughness) {
    float n = clamp(NdotW, 0.0, 1.0);
    float alpha = max(roughness * roughness, 0.025 * 0.025);
    float alpha2 = alpha * alpha;
    return (2.0 * n)
        / max(n + sqrt(alpha2 + (1.0 - alpha2) * n * n), 1e-6);
}

vec3 sampleVisibleGgxNormal(
    vec3 N, vec3 V, float roughness, vec2 randomSample
) {
    vec3 T;
    vec3 B;
    buildOrthoBasis(N, T, B);

    float alpha = max(roughness * roughness, 0.025 * 0.025);
    vec3 localV = vec3(dot(V, T), dot(V, B), max(dot(V, N), 1e-5));
    vec3 stretchedV = normalize(
        vec3(alpha * localV.x, alpha * localV.y, localV.z));

    float lensq = dot(stretchedV.xy, stretchedV.xy);
    vec3 basis1 = lensq > 1e-7
        ? vec3(-stretchedV.y, stretchedV.x, 0.0) * inversesqrt(lensq)
        : vec3(1.0, 0.0, 0.0);
    vec3 basis2 = cross(stretchedV, basis1);

    float radius = sqrt(randomSample.x);
    float phi = 2.0 * PI * randomSample.y;
    float diskX = radius * cos(phi);
    float diskY = radius * sin(phi);
    float blend = 0.5 * (1.0 + stretchedV.z);
    diskY = mix(sqrt(max(0.0, 1.0 - diskX * diskX)), diskY, blend);

    vec3 projectedNormal = diskX * basis1 + diskY * basis2
        + sqrt(max(0.0, 1.0 - diskX * diskX - diskY * diskY)) * stretchedV;
    vec3 localH = normalize(vec3(
        alpha * projectedNormal.x,
        alpha * projectedNormal.y,
        max(projectedNormal.z, 0.0)));
    return normalize(T * localH.x + B * localH.y + N * localH.z);
}

vec3 evaluatePathBsdf(
    vec3 N, vec3 V, vec3 L,
    vec3 baseColor, float metalness, float roughness, float ior,
    out float diffusePdf, out float specularPdf
) {
    float NdotV = max(dot(N, V), 0.0);
    float NdotL = max(dot(N, L), 0.0);
    diffusePdf = NdotL * (1.0 / PI);
    specularPdf = 0.0;
    if (NdotV <= 1e-5 || NdotL <= 1e-5) return vec3(0.0);

    vec3 halfSum = V + L;
    if (dot(halfSum, halfSum) <= 1e-8) return vec3(0.0);
    vec3 H = normalize(halfSum);
    float NdotH = max(dot(N, H), 0.0);
    float VdotH = max(dot(V, H), 0.0);
    if (NdotH <= 1e-5 || VdotH <= 1e-5) return vec3(0.0);

    float safeRoughness = clamp(roughness, 0.025, 1.0);
    float D = distributionGGX(NdotH, safeRoughness);
    float G1V = ggxSmithG1(NdotV, safeRoughness);
    float G = G1V * ggxSmithG1(NdotL, safeRoughness);
    vec3 F0 = mix(
        vec3(dielectricF0FromIor(max(ior, 1e-3))),
        clamp(baseColor, vec3(0.0), vec3(1.0)),
        clamp(metalness, 0.0, 1.0));
    vec3 F = fresnelSchlick(VdotH, F0);

    vec3 diffuse = (1.0 - F)
        * (1.0 - clamp(metalness, 0.0, 1.0))
        * max(baseColor, vec3(0.0))
        * (1.0 / PI);
    vec3 specular = D * G * F / max(4.0 * NdotV * NdotL, 1e-6);

    // VNDF half-vector density transformed through reflection:
    // p(wi) = D(h) G1(wo) / (4 |n·wo|).
    specularPdf = D * G1V / max(4.0 * NdotV, 1e-6);
    return diffuse + specular;
}

float pathSpecularProbability(
    vec3 N, vec3 V,
    vec3 baseColor, float metalness, float ior
) {
    float NdotV = max(dot(N, V), 0.0);
    vec3 F0 = mix(
        vec3(dielectricF0FromIor(max(ior, 1e-3))),
        clamp(baseColor, vec3(0.0), vec3(1.0)),
        clamp(metalness, 0.0, 1.0));
    vec3 F = fresnelSchlick(NdotV, F0);
    float specularEnergy = pathLuminance(F);
    float diffuseEnergy = pathLuminance(
        (1.0 - F) * (1.0 - clamp(metalness, 0.0, 1.0)) * baseColor);
    if (diffuseEnergy <= 1e-5) return 1.0;
    if (specularEnergy <= 1e-5) return 0.0;
    return clamp(
        specularEnergy / (specularEnergy + diffuseEnergy),
        0.05, 0.95);
}

bool samplePathBsdf(
    vec3 N, vec3 V,
    vec3 baseColor, float metalness, float roughness, float ior,
    float lobeSample, vec2 directionSample,
    out vec3 sampledDirection, out vec3 sampleWeight,
    out bool sampledSpecular
) {
    float specularProbability = pathSpecularProbability(
        N, V, baseColor, metalness, ior);
    sampledSpecular = lobeSample < specularProbability;
    if (sampledSpecular) {
        vec3 H = sampleVisibleGgxNormal(N, V, roughness, directionSample);
        sampledDirection = normalize(reflect(-V, H));
    } else {
        sampledDirection = cosineWeightedHemisphere(
            N, directionSample.x, directionSample.y);
    }

    float NdotL = max(dot(N, sampledDirection), 0.0);
    if (NdotL <= 1e-5) {
        sampleWeight = vec3(0.0);
        return false;
    }

    float diffusePdf;
    float specularPdf;
    vec3 bsdf = evaluatePathBsdf(
        N, V, sampledDirection,
        baseColor, metalness, roughness, ior,
        diffusePdf, specularPdf);
    float mixturePdf = mix(
        diffusePdf, specularPdf, specularProbability);
    if (mixturePdf <= 1e-7) {
        sampleWeight = vec3(0.0);
        return false;
    }

    sampleWeight = bsdf * (NdotL / mixturePdf);
    return !any(isnan(sampleWeight))
        && !any(isinf(sampleWeight))
        && max(max(sampleWeight.r, sampleWeight.g), sampleWeight.b) > 0.0;
}
