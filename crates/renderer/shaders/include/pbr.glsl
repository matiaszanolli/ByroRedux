// PBR — GGX / anisotropic GGX, Smith geometry, Fresnel, Disney diffuse split, specular AA
//
// NON-STANDALONE shader fragment. Included by triangle.frag in dependency
// order via GL_GOOGLE_include_directive; it references symbols (structs,
// SSBO/UBO bindings, helper functions, constants) defined in shader_constants.glsl
// and in earlier includes. Do not compile on its own.

// ── PBR: GGX / Cook-Torrance BRDF ──────────────────────────────────

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
// 0.025 floor mirrors `specularAaRoughness`'s `filteredR² ≥ 0.025²`
// clamp — preserves the BSLightingShader gloss-cap behaviour
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

// Fresnel (Schlick approximation).
vec3 fresnelSchlick(float cosTheta, vec3 F0) {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
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
    float FL = pow(clamp(1.0 - NdotL, 0.0, 1.0), 5.0);
    float FV = pow(clamp(1.0 - NdotV, 0.0, 1.0), 5.0);
    float FH = pow(clamp(1.0 - HdotL, 0.0, 1.0), 5.0);

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

    // Sheen — Fresnel-weighted edge highlight, tinted between white
    // and base colour. Layered on top of the diffuse lobe.
    vec3 sheenColor = mix(vec3(1.0), albedo, sheenTint);

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
// derivatives, then widen `roughness²` by `2 × kernel_variance`. The
// lobe smears the bright/dark across pixels at exactly the rate
// the underlying normal aliases — converging back to the authored
// roughness on smooth surfaces (small variance) so close-range
// specular highlights stay sharp.
//
// Returns the filtered roughness (already `sqrt`'d so the caller
// can pass it straight to [`distributionGGX`] / [`geometrySmith`]).
// `roughness` clamp at `0.025` mirrors what the BSLightingShader
// gloss path reaches at maximum gloss; the `min(.., 1.0)` upper
// bound is the GGX validity ceiling.
float specularAaRoughness(vec3 N, float roughness) {
    vec3 dNdx = dFdx(N);
    vec3 dNdy = dFdy(N);
    float kernelVariance = 0.25 * (dot(dNdx, dNdx) + dot(dNdy, dNdy));
    float roughness2 = roughness * roughness;
    float filteredR2 = clamp(roughness2 + 2.0 * kernelVariance, 0.025 * 0.025, 1.0);
    return sqrt(filteredR2);
}

