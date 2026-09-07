#ifndef BYRO_GROUNDCOVER_LIGHT_GLSL
#define BYRO_GROUNDCOVER_LIGHT_GLSL

// EXAL ground cover — the light response (§12.1, §12.2, §12.5, §12.6; #4057).
//
// `docs/engine/exal-groundcover.md` §12. Sections 1–11 answer *where* ground
// cover is; these four terms are what separate a correct distribution of
// individually-lit cards from a living surface. Every one is analytic — no
// ray, no probe, no bake step — which is not a compromise: at ankle height
// across a whole worldspace, a closed form that is nearly right beats a traced
// one that cannot be afforded per blade.
//
// Requires:
//   #include "include/shader_constants.glsl"
//
// ## One canopy, four questions
//
// The four terms are two pairs, and each pair shares an input:
//
//   §12.1 occlusion  / §12.5 canopy shadow — how much *sky* / *sun* reaches a
//                                            point inside the sward
//   §12.6 sheen      / §12.2 translucency  — the front-lit / back-lit halves
//                                            of the same blade surface
//
// Building half of either pair has a recognisable failure: grass that is flat
// under overcast or shadowless in direct sun; a meadow that is flat from one
// direction. So they live in one header and are meant to be called together.
//
// ## Why §12.1 has no strength parameter of its own
//
// §11.6 asked "how hard should the base darkening track `d_ground`, and over
// what fraction of blade height". The answer is that it is not a free
// parameter. Ambient occlusion and canopy shadow are *the same extinction
// through the same slab*, differing only in the direction the light arrives
// from: §12.5 integrates along the sun, §12.1 over the hemisphere. The
// hemisphere integral of `exp(−τ/cos θ)` is `2·E₃(τ)`, and the standard
// two-stream approximation to it replaces the angular integral with a single
// diffusivity factor of 5/3 (Elsasser). So §12.1 is §12.5 with `1/cos θ`
// replaced by `GROUNDCOVER_SKY_DIFFUSIVITY`, and a second independent
// "occlusion strength" scalar would only be a way for the two to disagree
// about how thick the same grass is.
//
// ## Everything here reads `d_ground`, never `d_draw`
//
// §3 is explicit. The view-faded density would make a meadow's interior
// brighten, and the shadow under it lighten, as the camera retreats — a slow
// whole-screen brightness change keyed to camera position, very visible and
// very hard to attribute once shipped.

// ── §12.5 / §12.1 — extinction through the canopy slab ──────────────────

/// Optical depth of `depth` world units of canopy at intrinsic density
/// `dGround`, measured **vertically**.
///
/// `K · LAD · d · depth`, where `K` is the Beer–Lambert extinction
/// coefficient for randomly oriented leaves (Monsi & Saeki 1953) and `LAD` is
/// leaf area per unit depth at full density. Both are named constants with a
/// source; see `shader_constants_data.rs`.
float byroGcCanopyOpticalDepth(float dGround, float depth) {
    return GROUNDCOVER_CANOPY_EXTINCTION_K
         * GROUNDCOVER_CANOPY_LEAF_AREA_DENSITY
         * max(dGround, 0.0)
         * max(depth, 0.0);
}

/// §12.5 — Beer–Lambert transmittance of direct light arriving at `cosTheta`
/// from vertical, after `depth` units of canopy at density `dGround`.
///
/// A low sun traverses more canopy, so the shadow deepens and lengthens on
/// its own; that behaviour is what reads as a real shadow, and it comes out
/// of the closed form with no BLAS, no TLAS entry and no ray budget. The
/// `cosTheta` floor is a numerical guard, not a physical claim — see
/// `GROUNDCOVER_CANOPY_MIN_COS`.
float byroGcCanopyTransmittance(float dGround, float depth, float cosTheta) {
    float tau = byroGcCanopyOpticalDepth(dGround, depth);
    return exp(-tau / max(abs(cosTheta), GROUNDCOVER_CANOPY_MIN_COS));
}

/// §12.1 — how much of the *sky* reaches a point `depth` units below the top
/// of the canopy. The same slab as above, integrated over the hemisphere
/// instead of along one direction.
///
/// Full occlusion at the base of a dense sward, none at the tip, none anywhere
/// at zero density — so a blade standing alone is lit along its length and the
/// same blade in a thicket is not, which is the distinction §7's baked
/// `colour_gradient` could not make and no longer tries to.
float byroGcSkyOcclusion(float dGround, float depth) {
    return exp(-byroGcCanopyOpticalDepth(dGround, depth) * GROUNDCOVER_SKY_DIFFUSIVITY);
}

// ── §12.2 — translucency ────────────────────────────────────────────────

/// Fraction of light that survives passing *through* a blade of tapered width
/// `bladeWidth` (world units).
///
/// Beer–Lambert again, through the blade rather than the canopy. Because the
/// caller passes the width already tapered by parametric height, the tip
/// transmits more than the base for free — which is §11.7's answer: one
/// transmission colour per species suffices, and the along-height variation
/// real leaves show falls out of the geometry.
float byroGcBladeTransmittance(float bladeWidth) {
    return exp(-GROUNDCOVER_BLADE_TRANSMISSION_EXTINCTION * max(bladeWidth, 0.0));
}

/// §12.2's directional lobe — Barré-Brisebois & Bouchard's translucency
/// approximation (GDC 2011), which bends the light vector into the surface
/// before evaluating a forward-scattering power lobe.
///
/// **The ordering this exists to keep straight.** The caller must NOT fold
/// this into the diffuse `max(N·L, 0)`: the geometric self-shadow — the near
/// face of a lit blade — is precisely the case that should glow, and clamping
/// it away is the shape a first implementation reaches for and the reason
/// backlit grass so often comes out flat. The traced shadow ray is a different
/// matter and *must* still apply: a blade shadowed by a distant rock receives
/// nothing and must not glow.
float byroGcTransmissionLobe(vec3 N, vec3 V, vec3 L) {
    vec3 bent = normalize(L + N * GROUNDCOVER_TRANSMISSION_DISTORTION);
    return pow(clamp(dot(V, -bent), 0.0, 1.0), GROUNDCOVER_TRANSMISSION_POWER);
}

// ── §12.6 — sheen ───────────────────────────────────────────────────────

/// Charlie sheen distribution (Estevez & Kulla 2017; the fibre model
/// `KHR_materials_sheen` adopted). A blade is a ribbed, waxy fibre, not a
/// microfacet slab, and a GGX lobe over it produces a point glint where the
/// real surface produces a broad silvering.
float byroGcCharlieD(float roughness, float NdotH) {
    float invR = 1.0 / max(roughness, 0.01);
    float cos2h = NdotH * NdotH;
    // Floored so the `pow` cannot see zero at grazing.
    float sin2h = max(1.0 - cos2h, 0.0078125);
    return (2.0 + invR) * pow(sin2h, invR * 0.5) * (1.0 / 6.2831853);
}

/// Ashikhmin's visibility term — the cheap companion to Charlie, and the one
/// the same references pair it with.
float byroGcAshikhminV(float NdotV, float NdotL) {
    return 1.0 / max(4.0 * (NdotL + NdotV - NdotL * NdotV), 1.0e-4);
}

/// Schlick Fresnel at the blade cuticle. `F0` is a property of the wax
/// (n ≈ 1.45), not of the species — see `GROUNDCOVER_SHEEN_F0`.
float byroGcCuticleFresnel(float cosTheta) {
    float m = clamp(1.0 - cosTheta, 0.0, 1.0);
    float m2 = m * m;
    return GROUNDCOVER_SHEEN_F0 + (1.0 - GROUNDCOVER_SHEEN_F0) * (m2 * m2 * m);
}

/// §12.6's front-lit half: a Fresnel-weighted grazing lobe over the diffuse
/// response. `sheen` is the per-species amount.
float byroGcSheenLobe(vec3 N, vec3 V, vec3 L, float sheen) {
    if (sheen <= 0.0) {
        return 0.0;
    }
    vec3 H = normalize(L + V);
    float NdotH = clamp(dot(N, H), 0.0, 1.0);
    float NdotV = clamp(dot(N, V), 1.0e-4, 1.0);
    float NdotL = clamp(dot(N, L), 0.0, 1.0);
    float VdotH = clamp(dot(V, H), 0.0, 1.0);
    return sheen
         * byroGcCuticleFresnel(VdotH)
         * byroGcCharlieD(GROUNDCOVER_SHEEN_ROUGHNESS, NdotH)
         * byroGcAshikhminV(NdotV, NdotL)
         * NdotL;
}

/// §12.6's ambient half: a blade's environment is the sky, and the sky
/// parameters are already a resource. A Fresnel-weighted sky tint at grazing
/// angles is the whole of it — grass is not a mirror, nothing is legible in
/// its reflection, so there is no probe, no cubemap and no reflection ray to
/// justify.
float byroGcSheenAmbient(vec3 N, vec3 V, float sheen) {
    return sheen * byroGcCuticleFresnel(clamp(dot(N, V), 0.0, 1.0));
}

#endif // BYRO_GROUNDCOVER_LIGHT_GLSL
