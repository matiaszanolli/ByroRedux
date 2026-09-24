//! Rule coverage for `env.health` (EX-05 / #2368).
//!
//! Each rule gets a negative case built by perturbing a known-good pair, so a
//! rule that stops firing shows up as a failing test rather than as a smoke
//! run that quietly passes.

use super::*;
use crate::components::DalcCubeYup;
use crate::fog::FogMedium;
use byroredux_core::ecs::components::water::WaterMaterial;

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
        weather: crate::components::WeatherSkyState::default(),
    };
    (lighting, sky)
}

fn fields(findings: &[EnvFinding]) -> Vec<&str> {
    findings.iter().map(|f| f.field.as_str()).collect()
}

/// The pre-#4483 two-resource form, kept so the original rule tests read as
/// before; the water/weather coverage tests below call `check_environment`
/// directly with the full argument list.
fn check_lighting_sky(
    lighting: Option<&CellLightingRes>,
    sky: Option<&SkyParamsRes>,
) -> Vec<EnvFinding> {
    check_environment(lighting, sky, None, &[])
}

#[test]
fn a_healthy_exterior_reports_nothing() {
    let (lighting, sky) = healthy_exterior();
    assert_eq!(check_lighting_sky(Some(&lighting), Some(&sky)), Vec::new());
}

/// Absent resources are the pre-cell-load state, not a defect — the smoke
/// script gates presence by confirming the worldspace instead.
#[test]
fn absent_resources_are_not_findings() {
    assert!(check_lighting_sky(None, None).is_empty());
    let (lighting, _) = healthy_exterior();
    assert!(check_lighting_sky(Some(&lighting), None).is_empty());
}

/// The EX-05 case the pixel counter cannot see on its own: a NaN that never
/// reaches a lit pixel because the sun it belongs to is dark.
#[test]
fn non_finite_sun_color_is_caught_even_at_zero_intensity() {
    let (lighting, mut sky) = healthy_exterior();
    sky.sun_color[1] = f32::NAN;
    sky.sun_intensity = 0.0;
    let findings = check_lighting_sky(Some(&lighting), Some(&sky));
    assert_eq!(fields(&findings), ["sky.sun_color"]);
    assert!(findings[0].detail.contains("index 1"), "{findings:?}");
}

#[test]
fn infinite_fog_far_is_caught() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.fog_far = f32::INFINITY;
    assert_eq!(
        fields(&check_lighting_sky(Some(&lighting), Some(&sky))),
        ["lighting.fog_far"]
    );
}

#[test]
fn negative_radiance_is_caught() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.ambient[2] = -0.01;
    let findings = check_lighting_sky(Some(&lighting), Some(&sky));
    assert_eq!(fields(&findings), ["lighting.ambient"]);
    assert!(findings[0].detail.contains("negative"), "{findings:?}");
}

/// A NaN reports once, as non-finite — not twice, because `NaN < 0.0` is
/// false and the ordering comparison would otherwise be meaningless.
#[test]
fn a_nan_reports_once_not_as_both_rules() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.ambient[0] = f32::NAN;
    let findings = check_lighting_sky(Some(&lighting), Some(&sky));
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].detail.contains("non-finite"), "{findings:?}");
}

#[test]
fn an_unnormalised_sun_direction_is_caught() {
    let (lighting, mut sky) = healthy_exterior();
    sky.sun_direction = [1.0, 2.0, 3.0];
    assert_eq!(
        fields(&check_lighting_sky(Some(&lighting), Some(&sky))),
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
        fields(&check_lighting_sky(Some(&lighting), Some(&sky))),
        ["lighting.directional_dir"]
    );
}

/// `f32` drift from a quaternion rotation must not trip the rule.
#[test]
fn f32_normalisation_drift_is_within_tolerance() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.directional_dir = [0.0, 1.0 + UNIT_LENGTH_EPSILON * 0.5, 0.0];
    assert!(check_lighting_sky(Some(&lighting), Some(&sky)).is_empty());
}

/// "Confirmed exterior lighting": the cell loader and the sky path populate
/// these two flags independently, so a mismatch means one is stale.
#[test]
fn interior_lighting_under_an_exterior_sky_is_caught() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.is_interior = true;
    assert_eq!(
        fields(&check_lighting_sky(Some(&lighting), Some(&sky))),
        ["is_interior/is_exterior"]
    );
}

#[test]
fn a_consistent_interior_pair_is_clean() {
    let (mut lighting, mut sky) = healthy_exterior();
    lighting.is_interior = true;
    sky.is_exterior = false;
    assert!(check_lighting_sky(Some(&lighting), Some(&sky)).is_empty());
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
    assert!(check_lighting_sky(Some(&lighting), Some(&sky)).is_empty());
}

/// `0.0` is the documented "layer disabled" sentinel for a cloud tile scale.
#[test]
fn disabled_cloud_layers_are_not_findings() {
    let (lighting, mut sky) = healthy_exterior();
    sky.cloud_tile_scale = 0.0;
    assert!(check_lighting_sky(Some(&lighting), Some(&sky)).is_empty());

    sky.cloud_tile_scale = -1.0;
    assert_eq!(
        fields(&check_lighting_sky(Some(&lighting), Some(&sky))),
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
        fields(&check_lighting_sky(Some(&lighting), Some(&sky))),
        ["lighting.directional_ambient", "sky.current_dalc_cube"]
    );
}

// ── #4483 — canonical water + weather gate coverage ─────────────────────

/// A weather snapshot with sane fog distances and both media finite.
fn healthy_weather() -> crate::components::WeatherDataRes {
    crate::components::WeatherDataRes {
        sky_colors: [[[0.0; 3]; 6]; 10],
        fog: [100.0, 60000.0, 200.0, 30000.0],
        fog_media: [
            FogMedium::from_legacy_ramp(100.0, 60000.0, None),
            FogMedium::from_legacy_ramp(200.0, 30000.0, None),
        ],
        tod_hours: [6.0, 10.0, 18.0, 22.0],
        skyrim_dalc_per_tod: None,
        wind_speed: 0,
        precipitation: 0.0,
        cloud_layer_velocities: [[0.0; 2]; 4],
        cloud_layer_velocities_authored: [false; 4],
        cloud_layer_colors: [[[1.0; 3]; 4]; 4],
        cloud_layer_alphas: [[1.0; 4]; 4],
        weather: crate::components::WeatherSkyState::default(),
        grass_dimmer: 1.0,
        sunlight_dimmer: 1.0,
        image_space: Default::default(),
    }
}

/// `WaterMaterial::default()` and sane weather fog contribute nothing — the
/// expanded coverage must not turn the healthy exterior into a failure.
#[test]
fn healthy_water_and_weather_report_nothing() {
    let (lighting, sky) = healthy_exterior();
    let wd = healthy_weather();
    assert!(
        check_environment(
            Some(&lighting),
            Some(&sky),
            Some(&wd),
            &[WaterMaterial::default()]
        )
        .is_empty()
    );
}

/// The #4483 completeness fixture: a corrupt WATR carries NaN in the three
/// scalars the translate copies verbatim — `resolve_water_colors`' fog
/// distance, `resolve_water_specular`'s `sun_specular_power`, and
/// `resolve_water_noise_and_rain`'s `wave_amplitude` (clamping is WATAL
/// policy, not this path's) — resolves through the canonical boundary
/// unchanged, and the gate, not the shader, is where the poison is caught.
#[test]
fn a_nan_watr_reports_fail_through_the_resolved_material() {
    use byroredux_plugin::esm::records::misc::{WaterParams, WatrRecord};

    let rec = WatrRecord {
        params: WaterParams {
            fog_near: f32::NAN,
            sun_specular_power: f32::NAN,
            wave_amplitude: f32::NAN,
            ..WaterParams::default()
        },
        ..WatrRecord::default()
    };
    let waters = std::collections::HashMap::from([(rec.form_id, rec)]);
    let (mat, _, _, _, _) = crate::env_translate::resolve_water_material(&waters, Some(0));

    let findings = check_environment(None, None, None, std::slice::from_ref(&mat));
    assert_eq!(
        fields(&findings),
        [
            "water[0].fog_near",
            // The GNAM-less day/night palettes mirror the base fog — the NaN
            // propagates there too, and the gate names every copy.
            "water[0].day_fog_near",
            "water[0].night_fog_near",
            "water[0].wave_amplitude",
            "water[0].sun_specular_power",
        ],
        "the gate must name each verbatim-copy field the NaN reached"
    );
}

/// A negative water colour is radiance — the non-negative rule reaches the
/// water tier, not just the finite rule.
#[test]
fn negative_water_color_is_caught() {
    let mut mat = WaterMaterial::default();
    mat.shallow_color[1] = -0.01;
    let findings = check_environment(None, None, None, &[mat]);
    assert_eq!(fields(&findings), ["water[0].shallow_color"]);
    assert!(findings[0].detail.contains("negative"), "{findings:?}");
}

/// The finite rule walks the whole scalar tail, and the finding names its
/// plane — two planes each break a different scalar.
#[test]
fn non_finite_water_scalars_are_caught_per_plane() {
    let mut a = WaterMaterial::default();
    a.ior = f32::NAN;
    let mut b = WaterMaterial::default();
    b.scroll_b[0] = f32::INFINITY;
    let findings = check_environment(None, None, None, &[a, b]);
    assert_eq!(
        fields(&findings),
        ["water[0].ior", "water[1].scroll_b"],
        "each plane's finding must carry its own index"
    );
}

/// `WeatherDataRes::fog` — the legacy TOD distance quad the system writes
/// into `cell_lit` every frame — is gated (#4483).
#[test]
fn weather_fog_distances_are_gated() {
    let (lighting, sky) = healthy_exterior();
    let mut wd = healthy_weather();
    wd.fog[1] = f32::NAN;
    let findings = check_environment(Some(&lighting), Some(&sky), Some(&wd), &[]);
    assert_eq!(fields(&findings), ["weather.fog"]);
}

/// The three `FogMedium` fields the gate skipped pre-#4483
/// (`single_scatter_albedo`, `coverage`, `scale_height_meters`) are covered,
/// on both the live cell medium and both weather media.
#[test]
fn fog_medium_secondary_fields_are_gated() {
    let (mut lighting, sky) = healthy_exterior();
    lighting.fog_medium.scale_height_meters = f32::NAN;
    let findings = check_lighting_sky(Some(&lighting), Some(&sky));
    assert_eq!(
        fields(&findings),
        ["lighting.fog_medium.scale_height_meters"]
    );

    let mut wd = healthy_weather();
    wd.fog_media[0].single_scatter_albedo = f32::NAN;
    wd.fog_media[1].coverage = f32::NAN;
    let findings = check_environment(None, None, Some(&wd), &[]);
    assert_eq!(
        fields(&findings),
        [
            "weather.fog_media[0].single_scatter_albedo",
            "weather.fog_media[1].coverage"
        ]
    );
}

/// #4731 — tie `check_water_plane`'s hand-kept field walk to
/// `WaterMaterial`'s actual fields. Every float leaf of the default
/// material (via the `inspect` Serialize derive the debug server already
/// enables) must be walked by one of the two rules, and every walked name
/// must still be a real field — so a struct addition reaches the `env:
/// FAIL` gate instead of silently bypassing it, and a rename cannot leave
/// a stale entry guarding nothing. Five WATAL fields landed within two
/// days in August 2026; that burst rate is exactly how the #4483 class
/// re-opens.
#[test]
fn water_plane_walk_covers_every_float_field_of_water_material() {
    let json = serde_json::to_value(WaterMaterial::default())
        .expect("WaterMaterial must serialize to JSON under the inspect feature");
    let serde_json::Value::Object(map) = &json else {
        panic!("WaterMaterial must serialize to an object, got {json:?}");
    };
    let float_fields: Vec<&str> = map
        .iter()
        .filter(|(_, value)| json_value_is_all_f64(value))
        .map(|(key, _)| key.as_str())
        .collect();
    assert!(
        float_fields.len() >= 48,
        "fixture sanity: the default material must expose its float fields \
         (48 walked today); got {float_fields:?}"
    );

    let mat = WaterMaterial::default();
    let walked: Vec<&str> = super::water_radiance_fields(&mat)
        .into_iter()
        .chain(super::water_finite_fields(&mat))
        .map(|(name, _)| name)
        .collect();
    for field in &float_fields {
        assert!(
            walked.contains(field),
            "WaterMaterial float field `{field}` is not walked by \
             check_water_plane — a new WATAL field would reach water.frag's \
             push constants ungated by the finite/radiance rules (#4731, \
             the #4483 class)"
        );
    }
    for name in &walked {
        assert!(
            map.contains_key(*name),
            "check_water_plane walks `{name}`, which is not a WaterMaterial \
             field any more — the walk kept a stale entry after a rename \
             (#4731)"
        );
    }
}

/// A JSON value is a float field (or a non-empty array of them). serde_json
/// keeps the representation: f32/f64 serialize as f64-repr numbers, u32
/// indices as u64 — so `is_f64()` separates the walked float leaves from
/// the structural integer fields the walk deliberately skips.
fn json_value_is_all_f64(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(number) => number.is_f64(),
        serde_json::Value::Array(items) => {
            !items.is_empty() && items.iter().all(json_value_is_all_f64)
        }
        _ => false,
    }
}
