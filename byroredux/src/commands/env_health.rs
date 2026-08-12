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
//! - **Finite.** `CellLightingRes` and `SkyParamsRes` are copied more or less
//!   verbatim into the per-frame UBO. Nothing between here and the shader
//!   rejects a NaN, and `fit_legacy_fog_extinction` is the only consumer that
//!   guards its own inputs.
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
//! Fog distances are reported but **not** gated. `fit_legacy_fog_extinction`
//! already treats `far <= near` as "no fog" rather than as an error, so an
//! inverted ramp is a shipped authoring pattern the engine absorbs, not a
//! defect. Gating it would be inventing a rule the engine does not hold.

use super::shared::*;
use crate::components::{CellLightingRes, SkyParamsRes};

/// Tolerance on `‖dir‖ == 1`.
///
/// Sized for accumulated `f32` error, not for authoring slop: a quaternion
/// rotation of a unit vector and an explicit `x / len` normalisation each
/// drift by a few ULPs. Anything a producer bypass would yield — a zero
/// vector, an unnormalised Euler triple, a doubled direction — is orders of
/// magnitude outside it.
const UNIT_LENGTH_EPSILON: f32 = 1.0e-3;

/// One violated rule, named by the field that broke it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnvFinding {
    pub(crate) field: &'static str,
    pub(crate) detail: String,
}

fn finding(field: &'static str, detail: impl Into<String>) -> EnvFinding {
    EnvFinding {
        field,
        detail: detail.into(),
    }
}

fn check_finite(out: &mut Vec<EnvFinding>, field: &'static str, values: &[f32]) {
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
fn check_radiance(out: &mut Vec<EnvFinding>, field: &'static str, values: &[f32]) {
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

fn check_unit_direction(out: &mut Vec<EnvFinding>, field: &'static str, dir: [f32; 3]) {
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
        check_finite(
            &mut out,
            "lighting.fog_medium.extinction_per_meter",
            &[lit.fog_medium.extinction_per_meter],
        );
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
        "Gate CellLightingRes + SkyParamsRes on finite, usable values (#2368)"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let lighting = world.try_resource::<CellLightingRes>();
        let sky = world.try_resource::<SkyParamsRes>();
        let mut lines = vec![format!(
            "env: resources lighting={} sky={}",
            if lighting.is_some() {
                "present"
            } else {
                "absent"
            },
            if sky.is_some() { "present" } else { "absent" },
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

        let findings = check_environment(lighting.as_deref(), sky.as_deref());
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
