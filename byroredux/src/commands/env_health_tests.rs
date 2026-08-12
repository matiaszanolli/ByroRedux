//! Rule coverage for `env.health` (EX-05 / #2368).
//!
//! Each rule gets a negative case built by perturbing a known-good pair, so a
//! rule that stops firing shows up as a failing test rather than as a smoke
//! run that quietly passes.

use super::*;
use crate::components::DalcCubeYup;
use crate::fog::FogMedium;

/// The shape a real exterior load produces: normalised sun, non-negative
/// radiance everywhere, and the two exterior flags agreeing.
fn healthy_exterior() -> (CellLightingRes, SkyParamsRes) {
    let sun = [0.267_261_2, 0.534_522_5, 0.801_783_7]; // (1,2,3) normalised
    let lighting = CellLightingRes {
        ambient: [0.10, 0.11, 0.14],
        directional_color: [1.0, 0.95, 0.80],
        directional_dir: sun,
        is_interior: false,
        fog_color: [0.50, 0.45, 0.30],
        fog_near: 100.0,
        fog_far: 8000.0,
        fog_medium: FogMedium::from_legacy_ramp(100.0, 8000.0, None),
        directional_fade: None,
        fog_clip: None,
        fog_power: None,
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
        inheritance_flags: None,
    };
    let sky = SkyParamsRes {
        zenith_color: [0.20, 0.35, 0.70],
        horizon_color: [0.60, 0.62, 0.58],
        lower_color: [0.18, 0.16, 0.14],
        sun_direction: sun,
        sun_color: [1.0, 0.96, 0.86],
        sun_size: 0.02,
        sun_intensity: 4.0,
        sun_angular_radius: 0.020,
        is_exterior: true,
        cloud_tile_scale: 4.0,
        cloud_texture_index: 7,
        sun_texture_index: 0,
        cloud_tile_scale_1: 0.0,
        cloud_texture_index_1: 0,
        cloud_tile_scale_2: 0.0,
        cloud_texture_index_2: 0,
        cloud_tile_scale_3: 0.0,
        cloud_texture_index_3: 0,
        current_dalc_cube: None,
    };
    (lighting, sky)
}

fn fields(findings: &[EnvFinding]) -> Vec<&'static str> {
    findings.iter().map(|f| f.field).collect()
}

#[test]
fn a_healthy_exterior_reports_nothing() {
    let (lighting, sky) = healthy_exterior();
    assert_eq!(check_environment(Some(&lighting), Some(&sky)), Vec::new());
}

/// Absent resources are the pre-cell-load state, not a defect — the smoke
/// script gates presence by confirming the worldspace instead.
#[test]
fn absent_resources_are_not_findings() {
    assert!(check_environment(None, None).is_empty());
    let (lighting, _) = healthy_exterior();
    assert!(check_environment(Some(&lighting), None).is_empty());
}

/// The EX-05 case the pixel counter cannot see on its own: a NaN that never
/// reaches a lit pixel because the sun it belongs to is dark.
#[test]
fn non_finite_sun_color_is_caught_even_at_zero_intensity() {
    let (lighting, mut sky) = healthy_exterior();
    sky.sun_color[1] = f32::NAN;
    sky.sun_intensity = 0.0;
    let findings = check_environment(Some(&lighting), Some(&sky));
    assert_eq!(fields(&findings), ["sky.sun_color"]);
    assert!(findings[0].detail.contains("index 1"), "{findings:?}");
}

#[test]
fn infinite_fog_far_is_caught() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.fog_far = f32::INFINITY;
    assert_eq!(
        fields(&check_environment(Some(&lighting), Some(&sky))),
        ["lighting.fog_far"]
    );
}

#[test]
fn negative_radiance_is_caught() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.ambient[2] = -0.01;
    let findings = check_environment(Some(&lighting), Some(&sky));
    assert_eq!(fields(&findings), ["lighting.ambient"]);
    assert!(findings[0].detail.contains("negative"), "{findings:?}");
}

/// A NaN reports once, as non-finite — not twice, because `NaN < 0.0` is
/// false and the ordering comparison would otherwise be meaningless.
#[test]
fn a_nan_reports_once_not_as_both_rules() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.ambient[0] = f32::NAN;
    let findings = check_environment(Some(&lighting), Some(&sky));
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].detail.contains("non-finite"), "{findings:?}");
}

#[test]
fn an_unnormalised_sun_direction_is_caught() {
    let (lighting, mut sky) = healthy_exterior();
    sky.sun_direction = [1.0, 2.0, 3.0];
    assert_eq!(
        fields(&check_environment(Some(&lighting), Some(&sky))),
        ["sky.sun_direction"]
    );
}

/// The producer-bypass case that motivates the rule: nobody normalised, so
/// the vector is zero and the directional light has no orientation at all.
#[test]
fn a_zero_direction_is_caught() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.directional_dir = [0.0, 0.0, 0.0];
    assert_eq!(
        fields(&check_environment(Some(&lighting), Some(&sky))),
        ["lighting.directional_dir"]
    );
}

/// `f32` drift from a quaternion rotation must not trip the rule.
#[test]
fn f32_normalisation_drift_is_within_tolerance() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.directional_dir = [0.0, 1.0 + UNIT_LENGTH_EPSILON * 0.5, 0.0];
    assert!(check_environment(Some(&lighting), Some(&sky)).is_empty());
}

/// "Confirmed exterior lighting": the cell loader and the sky path populate
/// these two flags independently, so a mismatch means one is stale.
#[test]
fn interior_lighting_under_an_exterior_sky_is_caught() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.is_interior = true;
    assert_eq!(
        fields(&check_environment(Some(&lighting), Some(&sky))),
        ["is_interior/is_exterior"]
    );
}

#[test]
fn a_consistent_interior_pair_is_clean() {
    let (mut lighting, mut sky) = healthy_exterior();
    lighting.is_interior = true;
    sky.is_exterior = false;
    assert!(check_environment(Some(&lighting), Some(&sky)).is_empty());
}

/// An inverted ramp is a shipped authoring pattern the fog fitter absorbs
/// (`far <= near` → extinction 0), so it is reported by the command but must
/// never fail the gate.
#[test]
fn an_inverted_fog_ramp_is_not_a_finding() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.fog_near = 8000.0;
    lighting.fog_far = 100.0;
    lighting.fog_medium = FogMedium::from_legacy_ramp(8000.0, 100.0, None);
    assert_eq!(lighting.fog_medium.extinction_per_meter, 0.0);
    assert!(check_environment(Some(&lighting), Some(&sky)).is_empty());
}

/// `0.0` is the documented "layer disabled" sentinel for a cloud tile scale.
#[test]
fn disabled_cloud_layers_are_not_findings() {
    let (lighting, mut sky) = healthy_exterior();
    sky.cloud_tile_scale = 0.0;
    assert!(check_environment(Some(&lighting), Some(&sky)).is_empty());

    sky.cloud_tile_scale = -1.0;
    assert_eq!(
        fields(&check_environment(Some(&lighting), Some(&sky))),
        ["sky.cloud_tile_scale"]
    );
}

/// The Skyrim-only optional payloads are reached, not skipped.
#[test]
fn optional_skyrim_payloads_are_checked() {
    let (mut lighting, mut sky) = healthy_exterior();
    let mut cube = [[0.1, 0.1, 0.1]; 6];
    cube[3][2] = f32::NAN;
    lighting.directional_ambient = Some(cube);
    sky.current_dalc_cube = Some(DalcCubeYup {
        pos_x: [0.1; 3],
        neg_x: [0.1; 3],
        pos_y: [0.1; 3],
        neg_y: [0.1; 3],
        pos_z: [0.1; 3],
        neg_z: [-0.5, 0.1, 0.1],
        specular: [0.1; 3],
        fresnel_power: 1.0,
    });
    assert_eq!(
        fields(&check_environment(Some(&lighting), Some(&sky))),
        ["lighting.directional_ambient", "sky.current_dalc_cube"]
    );
}
