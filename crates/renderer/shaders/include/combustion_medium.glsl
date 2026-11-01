// Combustion medium: the optical coefficients a transported combustion state
// implies (extinction, spectral scattering, thermal emission), plus the state
// sanitizers and the blackbody approximation the emission uses. A pure function
// of one (chemistry, optical) texel pair — no bindings, no storage — so the
// froxel inject pass, the local-grid sim pass and composite's local fire march
// all derive the SAME medium from the SAME state (tracking #6531, Phase 1
// #6533). Moved verbatim from the sources/transport includes.
//
// chemistry = (fuel mass fraction, temperature K, total extinction sigma_t per
//              world unit, visible-radiance calibration at 1850 K)
// optical   = (spectral scattering sigma_s.rgb per world unit, steam extinction)
//
// Needs, before the include: shader_constants.glsl (LUMA_REC709,
// COMBUSTION_MAX_TEMPERATURE_K).
#ifndef BYRO_COMBUSTION_MEDIUM_GLSL
#define BYRO_COMBUSTION_MEDIUM_GLSL

const float AMBIENT_TEMPERATURE_K = 293.15;

// Accumulated coefficients of the radiative transfer equation contributed by
// authored local media at one froxel. Emission is carried separately from
// in-scattering because the two behave differently under temporal
// accumulation: in-scattering is a stochastic visibility estimate that WANTS
// history, while emission is deterministic and fast-changing and does not.
struct LocalMedium {
    vec3 scattering;   // sigma_s, per world unit
    float extinction;  // sigma_t, per world unit
    vec3 emission;     // sigma_a * L_e, radiance per world unit
};

// Canonical renderer-side combustion-medium constants. Keeping the fuel
// vapour coefficients here gives both camera-space integration and
// light-space transmittance the same material instead of restating it in two
// solver functions.
const float COMBUSTION_FUEL_VAPOUR_SIGMA_T = 0.0035;
const float COMBUSTION_FUEL_VAPOUR_SINGLE_SCATTER_ALBEDO = 0.10;

LocalMedium emptyLocalMedium() {
    return LocalMedium(vec3(0.0), 0.0, vec3(0.0));
}

// Approximate blackbody chromaticity from Planck's law at representative
// linear-sRGB wavelengths. Energy is normalized to unit Rec. 709 luminance;
// the physically dominant T^4 radiance scaling is applied separately below.
vec3 blackbodyChromaticity(float kelvin) {
    float temperature = clamp(kelvin, 700.0, 12000.0);
    vec3 wavelengthNm = vec3(610.0, 550.0, 460.0);
    const float referenceNm = 610.0;
    const float c2NmK = 1.438776877e7;
    float referenceDenominator = exp(clamp(c2NmK / (referenceNm * temperature), 0.0, 80.0)) - 1.0;
    vec3 denominator = exp(clamp(
        vec3(c2NmK) / (wavelengthNm * temperature),
        vec3(0.0),
        vec3(80.0)
    )) - vec3(1.0);
    vec3 spectral = pow(vec3(referenceNm) / wavelengthNm, vec3(5.0))
        * referenceDenominator / max(denominator, vec3(1.0e-6));
    return spectral / max(dot(spectral, LUMA_REC709), 1.0e-4);
}

// Visible-band blackbody-radiance ratio, evaluated at the photopic response
// peak (555 nm). The CPU canonical source uses a full CIE integration; this
// one-wavelength ratio is the inexpensive per-froxel approximation to the
// same Planck-law magnitude trend. Calibration is transported separately, so
// the ratio is exactly 1 at the shared 1850 K flame reference and preserves
// each source's authored/canonical HDR exposure.
float blackbodyVisibleRadianceRatio(float kelvin, float referenceKelvin) {
    const float wavelengthNm = 555.0;
    const float c2NmK = 1.438776877e7;
    float temperature = clamp(kelvin, 700.0, 12000.0);
    float referenceTemperature = clamp(referenceKelvin, 700.0, 12000.0);
    float numerator = exp(clamp(
        c2NmK / (wavelengthNm * referenceTemperature),
        0.0,
        80.0
    )) - 1.0;
    float denominator = exp(clamp(
        c2NmK / (wavelengthNm * temperature),
        0.0,
        80.0
    )) - 1.0;
    return clamp(numerator / max(denominator, 1.0e-6), 0.0, 4096.0);
}

vec4 sanitizedChemistry(vec4 chemistry) {
    if (any(isnan(chemistry)) || any(isinf(chemistry))) {
        return vec4(0.0, AMBIENT_TEMPERATURE_K, 0.0, 0.0);
    }
    chemistry.x = clamp(chemistry.x, 0.0, 1.0);
    chemistry.y = chemistry.y > 1.0
        ? clamp(chemistry.y, AMBIENT_TEMPERATURE_K, COMBUSTION_MAX_TEMPERATURE_K)
        : AMBIENT_TEMPERATURE_K;
    chemistry.z = clamp(chemistry.z, 0.0, 0.20);
    chemistry.w = clamp(chemistry.w, 0.0, 64.0);
    return chemistry;
}

vec4 sanitizedOptical(vec4 optical) {
    if (any(isnan(optical)) || any(isinf(optical))) {
        return vec4(0.0);
    }
    optical.rgb = clamp(optical.rgb, vec3(0.0), vec3(0.20));
    // The w lane is the steam extinction (same units and range as chemistry.z).
    optical.w = clamp(optical.w, 0.0, 0.20);
    return optical;
}

// Steam share of a cell's extinction, in [0, 1]: 0 is pure soot/smoke, 1 is
// pure steam. Steam sources write their extinction into optical.w; everything
// else leaves it at zero, so mixed cells land in between as the gas mixes.
float combustionSteamFraction(vec4 chemistry, vec4 optical) {
    chemistry = sanitizedChemistry(chemistry);
    optical = sanitizedOptical(optical);
    float sigmaT = chemistry.z + chemistry.x * COMBUSTION_FUEL_VAPOUR_SIGMA_T;
    return clamp(optical.w / max(sigmaT, 1.0e-6), 0.0, 1.0);
}

LocalMedium combustionMedium(vec4 chemistry, vec4 optical) {
    LocalMedium medium = emptyLocalMedium();
    chemistry = sanitizedChemistry(chemistry);
    optical = sanitizedOptical(optical);
    float fuelVapourSigmaT = chemistry.x * COMBUSTION_FUEL_VAPOUR_SIGMA_T;
    float sigmaT = chemistry.z + fuelVapourSigmaT;
    if (sigmaT <= 1.0e-7) {
        return medium;
    }

    vec3 sigmaS = optical.rgb
        + fuelVapourSigmaT * vec3(COMBUSTION_FUEL_VAPOUR_SINGLE_SCATTER_ALBEDO);
    sigmaS = clamp(sigmaS, vec3(0.0), vec3(sigmaT));
    vec3 sigmaA = vec3(sigmaT) - sigmaS;
    medium.extinction = sigmaT;
    medium.scattering = sigmaS;

    float thermalLuma = chemistry.w
        * blackbodyVisibleRadianceRatio(chemistry.y, 1850.0);
    vec3 thermalRadiance = blackbodyChromaticity(chemistry.y) * thermalLuma;
    medium.emission = sigmaA * thermalRadiance;
    return medium;
}

#endif
