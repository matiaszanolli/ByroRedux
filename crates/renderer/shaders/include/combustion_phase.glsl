// Species phase functions for the transported combustion medium (tracking
// #6531, Phase 3 #6535). Shared by the froxel inject pass and composite's
// local fire march so both light the same smoke the same way.
//
// A cell's species comes from its transported spectral albedo: soot absorbs
// (albedo ~0.2), a white-tinted emitter's steam does not (up to 0.9), and the
// albedo advects with the gas, so a cell mixing the two lands in between. The
// weight `combustionSteamWeight` is that position.
//   soot  : the dual-lobe Henyey-Greenstein the local lights already used,
//           weaker in the forward direction than fog.
//   steam : Jendersie & d'Eon 2023, "An Approximate Mie Scattering Function
//           for Fog and Cloud Rendering", eqs. 2-7: (1 - w_D) HG(g_HG) +
//           w_D Draine(g_D, alpha), fitted for droplet diameters 5-50 um.
//
// Multiple scattering keeps the closed-form gain 1 + w1*a + w2*a^2 (a = the
// spectral albedo), but each octave carries its own phase with the
// eccentricity scaled by c^n (Wrenninge et al. 2013, as in Hillaire 2016,
// eqs. 19-20). Every phase integrates to 1, so the gain's total energy is
// unchanged; only its angular spread softens.
//
// Needs, before the include: shader_constants.glsl and medium_transport.glsl.
#ifndef BYRO_COMBUSTION_PHASE_GLSL
#define BYRO_COMBUSTION_PHASE_GLSL

// Draine (2003) phase function: reduces to HG for alpha = 0.
float combustionDrainePhase(float cosTheta, float g, float alpha) {
    float gSafe = clamp(g, -0.999, 0.999);
    float g2 = gSafe * gSafe;
    float c = clamp(cosTheta, -1.0, 1.0);
    float denom = 1.0 + g2 - 2.0 * gSafe * c;
    return (1.0 - g2) / (12.566370614359172 * pow(max(denom, 1.0e-4), 1.5))
        * (1.0 + alpha * c * c) / (1.0 + alpha * (1.0 + 2.0 * g2) / 3.0);
}

// `shrink` scales the anisotropy of every lobe (1 = the single-scatter phase).
float combustionSteamPhase(float cosTheta, float shrink) {
    const float d = COMBUSTION_STEAM_DROPLET_DIAMETER_UM;
    float gHG = exp(-0.0990567 / (d - 1.67154));
    float gD = exp(-2.20679 / (d + 3.91029) - 0.428934);
    float alpha = exp(3.62489 - 8.29288 / (d + 5.52825));
    float wD = exp(-0.599085 / (d - 0.641583) - 0.665888);
    return mix(
        medium_henyey_greenstein(cosTheta, gHG * shrink),
        combustionDrainePhase(cosTheta, gD * shrink, alpha),
        wD
    );
}

float combustionSootPhase(float cosTheta, float shrink) {
    return mix(
        medium_henyey_greenstein(cosTheta, COMBUSTION_LOCAL_LIGHT_PHASE_BACKWARD_G * shrink),
        medium_henyey_greenstein(cosTheta, COMBUSTION_LOCAL_LIGHT_PHASE_FORWARD_G * shrink),
        COMBUSTION_LOCAL_LIGHT_PHASE_MIX
    );
}

float combustionSteamWeight(vec3 albedo) {
    float mean = (albedo.r + albedo.g + albedo.b) * (1.0 / 3.0);
    return smoothstep(COMBUSTION_SPECIES_STEAM_ALBEDO_LOW, COMBUSTION_SPECIES_STEAM_ALBEDO_HIGH, mean);
}

float combustionSpeciesPhase(float cosTheta, float steam, float shrink) {
    if (steam <= 1.0e-3) {
        return combustionSootPhase(cosTheta, shrink);
    }
    if (steam >= 1.0 - 1.0e-3) {
        return combustionSteamPhase(cosTheta, shrink);
    }
    return mix(
        combustionSootPhase(cosTheta, shrink),
        combustionSteamPhase(cosTheta, shrink),
        steam
    );
}

// The phase a unit of the medium's scattering coefficient is multiplied by:
// the single-scatter phase plus the two higher octaves, each weighted by the
// albedo it costs, with their anisotropy shrunk by the octave eccentricity.
// Replaces `multiscatter_gain * phase`.
vec3 combustionMultiScatterPhase(float cosTheta, vec3 albedo) {
    float steam = combustionSteamWeight(albedo);
    float p0 = combustionSpeciesPhase(cosTheta, steam, 1.0);
    float p1 = combustionSpeciesPhase(cosTheta, steam, COMBUSTION_OCTAVE_ECCENTRICITY);
    float p2 = combustionSpeciesPhase(
        cosTheta, steam, COMBUSTION_OCTAVE_ECCENTRICITY * COMBUSTION_OCTAVE_ECCENTRICITY);
    return vec3(p0)
        + COMBUSTION_MULTISCATTER_OCTAVE1_WEIGHT * albedo * p1
        + COMBUSTION_MULTISCATTER_OCTAVE2_WEIGHT * albedo * albedo * p2;
}

#endif
