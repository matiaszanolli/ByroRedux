//! `env.health` — environment-value gate for the exterior smoke matrix
//! (EX-05 / #2368).
//!
//! [`RenderHealthCommand`](super::world_info::RenderHealthCommand) counts
//! non-finite *pixels*; this counts non-finite (and otherwise unusable)
//! *inputs*. The two are complementary and neither substitutes for the other:
//! a NaN sun colour multiplied by a zero-intensity sun leaves the frame clean,
//! and a healthy environment can still be ruined downstream. EX-05 asks for
//! both.
//!
//! The checks below are deliberately limited to properties the producers
//! already guarantee, so a failure means a producer was bypassed rather than
//! that a threshold was set too tight:
//!
//! - **Finite.** `CellLightingRes`, `SkyParamsRes`, `WeatherDataRes` and the
//!   canonical `WaterMaterial` are copied more or less verbatim into the
//!   per-frame UBO / `water.frag` push constants. Nothing between here and
//!   the shaders rejects a NaN, and `fit_legacy_fog_extinction` is the only
//!   consumer that guards its own inputs.
//! - **Non-negative radiance.** Colours and intensities are linear radiance
//!   multipliers. A negative one subtracts light, which no authored record can
//!   express.
//! - **Unit-length directions.** `compute_sun_arc` divides by the vector
//!   length explicitly, and the interior path rotates the Gamebryo model
//!   direction `(1,0,0)` by a quaternion. Both are unit by construction, so a
//!   non-unit vector means neither ran.
//! - **Exterior agreement.** `CellLightingRes::is_interior` and
//!   `SkyParamsRes::is_exterior` are populated independently, by the cell
//!   loader and the weather/sky path respectively. They describe the same
//!   fact, so disagreement means one of the two is stale — the "confirmed
//!   exterior lighting" case the smoke matrix has to gate.
//!
//! #4483 — the gate is deliberately **coverage over the whole canonical env
//! tier**, not just the two legacy resources: a corrupt WATR's NaN reaches
//! `WaterMaterial` (and the push constants) verbatim through
//! `resolve_water_material`'s unclamped scalars — clamping policy belongs to
//! WATAL — so `env.health` is the input gate on that path, exactly as it is
//! for a NaN weather fog distance or a non-finite `FogMedium` coefficient.
//!
//! Fog distances are reported but **not** gated. `fit_legacy_fog_extinction`
//! already treats `far <= near` as "no fog" rather than as an error, so an
//! inverted ramp is a shipped authoring pattern the engine absorbs, not a
//! defect. Gating it would be inventing a rule the engine does not hold.

use super::shared::*;
use byroredux_core::ecs::components::water::{WaterMaterial, WaterPlane};
use crate::components::{CellLightingRes, SkyParamsRes, WeatherDataRes};
use crate::fog::FogMedium;

/// Tolerance on `‖dir‖ == 1`.
///
/// Sized for accumulated `f32` error, not for authoring slop: a quaternion
/// rotation of a unit vector and an explicit `x / len` normalisation each
/// drift by a few ULPs. Anything a producer bypass would yield — a zero
/// vector, an unnormalised Euler triple, a doubled direction — is orders of
/// magnitude outside it.
const UNIT_LENGTH_EPSILON: f32 = 1.0e-3;

/// One violated rule, named by the field that broke it. `field` is a dotted
/// rule name (`lighting.fog_near`, `water[2].ior`, …) — composed for the
/// per-plane/per-medium repeats, hence owned rather than `&'static`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnvFinding {
    pub(crate) field: String,
    pub(crate) detail: String,
}

fn finding(field: impl Into<String>, detail: impl Into<String>) -> EnvFinding {
    EnvFinding {
        field: field.into(),
        detail: detail.into(),
    }
}

fn check_finite(out: &mut Vec<EnvFinding>, field: &str, values: &[f32]) {
    if let Some((i, v)) = values
        .iter()
        .enumerate()
        .find(|(_, v)| !v.is_finite())
        .map(|(i, v)| (i, *v))
    {
        out.push(finding(field, format!("non-finite at index {i}: {v}")));
    }
}

/// Radiance must be finite and non-negative. Checked together so a NaN
/// colour reports once rather than tripping both rules.
fn check_radiance(out: &mut Vec<EnvFinding>, field: &str, values: &[f32]) {
    let before = out.len();
    check_finite(out, field, values);
    if out.len() != before {
        return;
    }
    if let Some((i, v)) = values
        .iter()
        .enumerate()
        .find(|(_, v)| **v < 0.0)
        .map(|(i, v)| (i, *v))
    {
        out.push(finding(
            field,
            format!("negative radiance at index {i}: {v}"),
        ));
    }
}

fn check_unit_direction(out: &mut Vec<EnvFinding>, field: &str, dir: [f32; 3]) {
    let before = out.len();
    check_finite(out, field, &dir);
    if out.len() != before {
        return;
    }
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if (len - 1.0).abs() > UNIT_LENGTH_EPSILON {
        out.push(finding(
            field,
            format!("not unit length: ‖{dir:?}‖ = {len:.6}"),
        ));
    }
}

/// All four coefficients of one canonical fog medium are finite. The fitter
/// makes `from_legacy_ramp` values finite by construction (`fog.rs` returns
/// 0.0 for non-finite inputs) and `lerp` preserves finiteness, so a finding
/// here means a producer bypassed both.
fn check_fog_medium(out: &mut Vec<EnvFinding>, prefix: &str, medium: &FogMedium) {
    check_finite(
        out,
        &format!("{prefix}.extinction_per_meter"),
        &[medium.extinction_per_meter],
    );
    check_finite(
        out,
        &format!("{prefix}.single_scatter_albedo"),
        &[medium.single_scatter_albedo],
    );
    check_finite(out, &format!("{prefix}.coverage"), &[medium.coverage]);
    check_finite(
        out,
        &format!("{prefix}.scale_height_meters"),
        &[medium.scale_height_meters],
    );
}

/// The radiance-rule fields `check_water_plane` walks, as (name, values)
/// pairs. Extracted from the inline walk so the #4731 struct-completeness
/// test can tie the walked names to `WaterMaterial`'s own serialized
/// fields instead of trusting a hand-kept list to stay in sync with the
/// struct.
fn water_radiance_fields(mat: &WaterMaterial) -> Vec<(&'static str, &[f32])> {
    vec![
        ("shallow_color", &mat.shallow_color[..]),
        ("deep_color", &mat.deep_color[..]),
        ("underwater_color", &mat.underwater_color[..]),
        ("reflection_tint", &mat.reflection_tint[..]),
        ("day_shallow_color", &mat.day_shallow_color[..]),
        ("day_deep_color", &mat.day_deep_color[..]),
        ("day_reflection_tint", &mat.day_reflection_tint[..]),
        ("night_shallow_color", &mat.night_shallow_color[..]),
        ("night_deep_color", &mat.night_deep_color[..]),
        ("night_reflection_tint", &mat.night_reflection_tint[..]),
    ]
}

/// The finite-rule fields `check_water_plane` walks — see
/// [`water_radiance_fields`] for why this is a function and not an inline
/// literal (#4731).
fn water_finite_fields(mat: &WaterMaterial) -> Vec<(&'static str, &[f32])> {
    vec![
        ("fog_near", std::slice::from_ref(&mat.fog_near)),
        ("fog_far", std::slice::from_ref(&mat.fog_far)),
        ("depth_amount", std::slice::from_ref(&mat.depth_amount)),
        ("underwater_fog_near", std::slice::from_ref(&mat.underwater_fog_near)),
        ("underwater_fog_far", std::slice::from_ref(&mat.underwater_fog_far)),
        ("underwater_fog_amount", std::slice::from_ref(&mat.underwater_fog_amount)),
        ("opacity", std::slice::from_ref(&mat.opacity)),
        ("alpha_controls", &mat.alpha_controls[..]),
        ("fresnel_f0", std::slice::from_ref(&mat.fresnel_f0)),
        ("reflectivity", std::slice::from_ref(&mat.reflectivity)),
        (
            "reflection_hdr_multiplier",
            std::slice::from_ref(&mat.reflection_hdr_multiplier),
        ),
        ("day_fog_near", std::slice::from_ref(&mat.day_fog_near)),
        ("day_fog_far", std::slice::from_ref(&mat.day_fog_far)),
        ("night_fog_near", std::slice::from_ref(&mat.night_fog_near)),
        ("night_fog_far", std::slice::from_ref(&mat.night_fog_far)),
        ("scroll_a", &mat.scroll_a[..]),
        ("scroll_b", &mat.scroll_b[..]),
        ("scroll_c", &mat.scroll_c[..]),
        ("uv_scale_a", std::slice::from_ref(&mat.uv_scale_a)),
        ("uv_scale_b", std::slice::from_ref(&mat.uv_scale_b)),
        ("uv_scale_c", std::slice::from_ref(&mat.uv_scale_c)),
        ("uv_offset", &mat.uv_offset[..]),
        (
            "noise_amplitude_scales",
            &mat.noise_amplitude_scales[..],
        ),
        ("noise_falloff", std::slice::from_ref(&mat.noise_falloff)),
        ("normal_falloff", &mat.normal_falloff[..]),
        ("displacement", &mat.displacement[..]),
        ("rain_start_size", std::slice::from_ref(&mat.rain_start_size)),
        ("rain_velocity", std::slice::from_ref(&mat.rain_velocity)),
        ("rain_falloff", std::slice::from_ref(&mat.rain_falloff)),
        ("rain_dampener", std::slice::from_ref(&mat.rain_dampener)),
        ("normal_magnitude", std::slice::from_ref(&mat.normal_magnitude)),
        (
            "above_water_fog_amount",
            std::slice::from_ref(&mat.above_water_fog_amount),
        ),
        ("depth_weights", &mat.depth_weights[..]),
        ("effect_controls", &mat.effect_controls[..]),
        ("specular_magnitude", std::slice::from_ref(&mat.specular_magnitude)),
        ("specular_radius", std::slice::from_ref(&mat.specular_radius)),
        ("flowmap_scale", std::slice::from_ref(&mat.flowmap_scale)),
        (
            "absorption_coefficients",
            &mat.absorption_coefficients[..],
        ),
        ("concentration", &mat.concentration[..]),
        ("foam_strength", std::slice::from_ref(&mat.foam_strength)),
        ("shoreline_width", std::slice::from_ref(&mat.shoreline_width)),
        ("ior", std::slice::from_ref(&mat.ior)),
        ("wave_amplitude", std::slice::from_ref(&mat.wave_amplitude)),
        ("wave_frequency", std::slice::from_ref(&mat.wave_frequency)),
        ("angular_velocity", std::slice::from_ref(&mat.angular_velocity)),
        ("rain_response", std::slice::from_ref(&mat.rain_response)),
        ("sun_specular_power", std::slice::from_ref(&mat.sun_specular_power)),
        ("roughness", std::slice::from_ref(&mat.roughness)),
    ]
}

/// Walk one resolved `WaterMaterial` (#4483). Every `f32` here reaches
/// `water.frag`'s push constants verbatim: colours get the radiance rule,
/// every other scalar the finite rule. Structural fields (indices, flags,
/// the normal encoding) have no non-finite value space and are skipped.
fn check_water_plane(out: &mut Vec<EnvFinding>, prefix: &str, mat: &WaterMaterial) {
    for (field, values) in water_radiance_fields(mat) {
        check_radiance(out, &format!("{prefix}.{field}"), values);
    }
    for (field, values) in water_finite_fields(mat) {
        check_finite(out, &format!("{prefix}.{field}"), values);
    }
}

/// Evaluate every environment rule. Pure — no `World`, no renderer — so the
/// rules are unit-testable without a Vulkan device or game data.
///
/// An absent resource is not a finding. `env.health` is run against live cell
/// loads *and* against the pre-load engine state, and "no cell yet" is a
/// legitimate answer; the smoke script gates presence separately, by asserting
/// the worldspace was confirmed.
pub(crate) fn check_environment(
    lighting: Option<&CellLightingRes>,
    sky: Option<&SkyParamsRes>,
    weather: Option<&WeatherDataRes>,
    water_planes: &[WaterMaterial],
) -> Vec<EnvFinding> {
    let mut out = Vec::new();

    if let Some(lit) = lighting {
        check_radiance(&mut out, "lighting.ambient", &lit.ambient);
        check_radiance(
            &mut out,
            "lighting.directional_color",
            &lit.directional_color,
        );
        check_unit_direction(&mut out, "lighting.directional_dir", lit.directional_dir);
        check_radiance(&mut out, "lighting.fog_color", &lit.fog_color);
        check_finite(&mut out, "lighting.fog_near", &[lit.fog_near]);
        check_finite(&mut out, "lighting.fog_far", &[lit.fog_far]);
        check_fog_medium(&mut out, "lighting.fog_medium", &lit.fog_medium);
        if let Some(c) = lit.fog_far_color {
            check_radiance(&mut out, "lighting.fog_far_color", &c);
        }
        if let Some(cube) = lit.directional_ambient {
            for face in cube.iter() {
                check_radiance(&mut out, "lighting.directional_ambient", face);
            }
        }
        if let Some(c) = lit.specular_color {
            check_radiance(&mut out, "lighting.specular_color", &c);
        }
    }

    if let Some(sky) = sky {
        check_unit_direction(&mut out, "sky.sun_direction", sky.sun_direction);
        check_radiance(&mut out, "sky.sun_color", &sky.sun_color);
        check_radiance(&mut out, "sky.sun_intensity", &[sky.sun_intensity]);
        check_radiance(&mut out, "sky.sun_size", &[sky.sun_size]);
        check_radiance(
            &mut out,
            "sky.sun_angular_radius",
            &[sky.sun_angular_radius],
        );
        check_radiance(&mut out, "sky.zenith_color", &sky.zenith_color);
        check_radiance(&mut out, "sky.horizon_color", &sky.horizon_color);
        check_radiance(&mut out, "sky.lower_color", &sky.lower_color);
        // A tile scale multiplies UVs; `0.0` is the documented layer-disabled
        // sentinel, so only negatives and non-finites are wrong.
        check_radiance(
            &mut out,
            "sky.cloud_tile_scale",
            &[
                sky.cloud_tile_scale,
                sky.cloud_tile_scale_1,
                sky.cloud_tile_scale_2,
                sky.cloud_tile_scale_3,
            ],
        );
        if let Some(cube) = sky.current_dalc_cube {
            for face in [
                cube.pos_x,
                cube.neg_x,
                cube.pos_y,
                cube.neg_y,
                cube.pos_z,
                cube.neg_z,
                cube.specular,
            ] {
                check_radiance(&mut out, "sky.current_dalc_cube", &face);
            }
            check_finite(
                &mut out,
                "sky.current_dalc_cube.fresnel_power",
                &[cube.fresnel_power],
            );
        }
    }

    // #4483 — the weather-side canonical inputs: the legacy TOD fog
    // distances (`WeatherDataRes::fog`) feed the `cell_lit` writes every
    // frame and both fitted media feed `fog_medium` verbatim.
    if let Some(wd) = weather {
        check_finite(&mut out, "weather.fog", &wd.fog);
        check_fog_medium(&mut out, "weather.fog_media[0]", &wd.fog_media[0]);
        check_fog_medium(&mut out, "weather.fog_media[1]", &wd.fog_media[1]);
    }

    // #4483 — the canonical water tier. ~40 `f32` fields reach
    // `water.frag`'s push constants verbatim from `resolve_water_material`
    // with no clamp on the corrupt-input path (clamping is WATAL policy),
    // so this gate is the input check for the whole material.
    for (index, mat) in water_planes.iter().enumerate() {
        check_water_plane(&mut out, &format!("water[{index}]"), mat);
    }

    if let (Some(lit), Some(sky)) = (lighting, sky) {
        if lit.is_interior == sky.is_exterior {
            out.push(finding(
                "is_interior/is_exterior",
                format!(
                    "lighting says is_interior={} while sky says is_exterior={} — \
                     one of the two is stale",
                    lit.is_interior, sky.is_exterior
                ),
            ));
        }
    }

    out
}

/// `env.health` — finite/usable check over the live environment resources.
///
/// Emits `env: PASS` or one `env: FAIL - <field>: <detail>` line per violated
/// rule, so the exterior smoke matrix can gate on a `grep` rather than on
/// re-parsing `light.dump`'s formatted floats. Mirrors the shape
/// `world.owners report` already uses for the EX-08 soak.
pub(crate) struct EnvHealthCommand;

impl ConsoleCommand for EnvHealthCommand {
    fn name(&self) -> &str {
        "env.health"
    }

    fn description(&self) -> &str {
        "Gate CellLightingRes + SkyParamsRes + WeatherDataRes + WaterMaterial on finite, usable values (#2368, #4483)"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let lighting = world.try_resource::<CellLightingRes>();
        let sky = world.try_resource::<SkyParamsRes>();
        let weather = world.try_resource::<WeatherDataRes>();
        let water_planes: Vec<WaterMaterial> = world
            .query::<WaterPlane>()
            .map(|query| query.iter().map(|(_, plane)| plane.material).collect())
            .unwrap_or_default();
        let mut lines = vec![format!(
            "env: resources lighting={} sky={} weather={} water_planes={}",
            if lighting.is_some() {
                "present"
            } else {
                "absent"
            },
            if sky.is_some() { "present" } else { "absent" },
            if weather.is_some() {
                "present"
            } else {
                "absent"
            },
            water_planes.len(),
        )];

        // Evidence, not gates — see the module doc on why fog ordering is
        // reported rather than asserted.
        if let Some(lit) = lighting.as_deref() {
            lines.push(format!(
                "env: fog near={:.1} far={:.1} extinction={:.6}/m{}",
                lit.fog_near,
                lit.fog_far,
                lit.fog_medium.extinction_per_meter,
                if lit.fog_far <= lit.fog_near {
                    " (inverted ramp — fog disabled by the fitter)"
                } else {
                    ""
                },
            ));
        }
        if let Some(sky) = sky.as_deref() {
            lines.push(format!(
                "env: sun dir=[{:.3}, {:.3}, {:.3}] intensity={:.3}",
                sky.sun_direction[0], sky.sun_direction[1], sky.sun_direction[2], sky.sun_intensity,
            ));
        }

        let findings = check_environment(
            lighting.as_deref(),
            sky.as_deref(),
            weather.as_deref(),
            &water_planes,
        );
        if findings.is_empty() {
            lines.push("env: PASS".to_string());
        } else {
            for f in &findings {
                lines.push(format!("env: FAIL - {}: {}", f.field, f.detail));
            }
        }
        CommandOutput::lines(lines)
    }
}

#[cfg(test)]
#[path = "env_health_tests.rs"]
mod tests;
