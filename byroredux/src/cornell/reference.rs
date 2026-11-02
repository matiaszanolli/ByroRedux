//! M-LIGHT reference rungs (`--cornell-reference <rung>`): Cornell scenes
//! built to be measured against an external converged reference
//! (reference-lighting PRD RLT-REF-001, RLT-REF-009, RLT-REF-010,
//! RLT-GI-010; Phase 1a, #6601).
//!
//! Unlike the `--cornell` showcase and the `--cornell-oracle` ladder, every
//! rung here obeys RLT-REF-009: no emissive surface, no glass or eye
//! material, no authored ambient, no DALC cube, no fog in range and no
//! medium. Emitters are inverse-square (`Emitter::inverse_square_point`,
//! RLT-REF-002) with a source radius of at most a tenth of the distance to
//! any measured receiver, and every material carries
//! `MAT_FLAG_LAMBERTIAN_REFERENCE`, so the engine shades it as the plain
//! Lambert lobe Mitsuba's `diffuse` BSDF is (RLT-GI-010).
//!
//! The rungs are room-scale (metres, not the showcase's 8-unit box) so the
//! scale-dependent parts of the pipeline — SSAO radius, ray offsets, the GI
//! distance fade — run in the regime game interiors do. Every rung records
//! its meshes' CPU sources so `--export-mitsuba` can serialise it.

use super::mitsuba_export::CornellMeshSources;
use super::*;
use byroredux_core::lighting::{
    Emitter, Meters, RadiantIntensityRgb, VisibilityMask, BETHESDA_UNITS_PER_METER,
};

/// One metre in world units.
const M: f32 = BETHESDA_UNITS_PER_METER;

/// Emitter source radius: 1.4 world units (2 cm), above the shader's
/// 1.0-unit sampling floor, so the engine's disk and the reference's sphere
/// have the same radius; *r*/*d* ≤ 0.07 at every surface of every rung.
const SOURCE_RADIUS_M: f32 = 0.02;

/// Room-scale box: 4 m wide and deep, 3 m tall, open at +Z.
const BOX_HALF_W: f32 = 2.0 * M;
const BOX_HEIGHT: f32 = 3.0 * M;
const BOX_WALL_T: f32 = 0.05 * M;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceRung {
    /// RLT-REF-010 point calibration: one inverse-square emitter above a
    /// white Lambertian plane, nothing else.
    CalibrationPoint,
    /// The L4 / closure rung: a Lambertian Cornell room (white floor,
    /// ceiling and back wall, red left wall, green right wall) with a tall
    /// block and a short cube, lit by one point emitter under the ceiling.
    LambertBox,
}

impl ReferenceRung {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::CalibrationPoint => "cal-point",
            Self::LambertBox => "lambert-box",
        }
    }
}

const RUNGS: &[ReferenceRung] = &[ReferenceRung::CalibrationPoint, ReferenceRung::LambertBox];

/// Parse `--cornell-reference <rung>`; a typo is an error, never a fallback.
pub(crate) fn cornell_reference_rung(args: &[String]) -> Result<Option<ReferenceRung>, String> {
    let Some(index) = args.iter().position(|a| a == "--cornell-reference") else {
        return Ok(None);
    };
    let names: Vec<&str> = RUNGS.iter().map(|r| r.name()).collect();
    let value = args
        .get(index + 1)
        .ok_or_else(|| format!("--cornell-reference requires one of: {}", names.join(", ")))?;
    RUNGS
        .iter()
        .copied()
        .find(|r| r.name() == value.to_ascii_lowercase())
        .map(Some)
        .ok_or_else(|| {
            format!(
                "unknown reference rung '{value}'; expected one of: {}",
                names.join(", ")
            )
        })
}

/// A Lambertian reference material (RLT-GI-010): metalness 0, no specular,
/// ior 1, and the flag that removes every GGX lobe and (1 − F) weight.
pub(super) fn lambert(color: [f32; 3]) -> Material {
    let mut material = matte(color);
    material.metalness = 0.0;
    material.specular_strength = 0.0;
    material.ior = 1.0;
    material.effect_shader_flags |=
        byroredux_renderer::vulkan::material::material_flag::LAMBERTIAN_REFERENCE;
    material
}

fn spawn_reference_emitter(world: &mut World, pos: Vec3, intensity: f32, name: &str) {
    let light = world.spawn();
    world.insert(light, Transform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(light, GlobalTransform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(
        light,
        LightSource {
            emitter: Emitter::inverse_square_point(
                RadiantIntensityRgb::new([intensity; 3]),
                // Cull window starts at the range: 10 m keeps every surface
                // of every rung inside it, so the law is pure 1/d².
                Meters::new(10.0),
                Meters::new(SOURCE_RADIUS_M),
                VisibilityMask::for_legacy_local_light(),
            ),
            shadow_flags: byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
            ..Default::default()
        },
    );
    name_entity(world, light, name);
}

/// Build the rung. Returns the camera pose (position, target).
pub(crate) fn setup_reference_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    rung: ReferenceRung,
) -> (Vec3, Vec3) {
    // RLT-REF-009: no ambient, no directional, interior (no sky probe, no
    // DALC), fog far beyond the scene and no medium. `bare` fits a medium
    // from even an out-of-range legacy ramp, which the froxel integral then
    // applies as a ~0.4 %/m extinction, so the rung disables it outright.
    world.insert_resource(CellLightingRes {
        directional_fade: Some(1.0),
        fog_medium: crate::fog::FogMedium::DISABLED,
        ..CellLightingRes::bare(
            [0.0; 3],
            [0.0; 3],
            [0.0, 1.0, 0.0],
            true,
            [0.0; 3],
            100_000.0,
            1_000_000.0,
        )
    });
    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);
    let camera = match rung {
        ReferenceRung::CalibrationPoint => {
            let plane = builder.box_mesh([2.0 * M, BOX_WALL_T, 2.0 * M]);
            spawn_object(
                world,
                plane,
                neutral,
                Vec3::new(0.0, -BOX_WALL_T, 0.0),
                Quat::IDENTITY,
                lambert([0.8; 3]),
                "cal_plane",
            );
            spawn_reference_emitter(world, Vec3::new(0.0, 1.5 * M, 0.0), 2.0, "cal_emitter");
            (Vec3::new(0.0, 2.6 * M, 2.6 * M), Vec3::ZERO)
        }
        ReferenceRung::LambertBox => {
            let h = BOX_HALF_W;
            let floor = builder.box_mesh([h, BOX_WALL_T, h]);
            let side = builder.box_mesh([BOX_WALL_T, BOX_HEIGHT * 0.5, h]);
            let back = builder.box_mesh([h, BOX_HEIGHT * 0.5, BOX_WALL_T]);
            let tall = builder.box_mesh([0.4 * M, 0.9 * M, 0.4 * M]);
            let short = builder.box_mesh([0.4 * M, 0.4 * M, 0.4 * M]);
            let white = [0.73; 3];
            let walls = [
                (floor, Vec3::new(0.0, -BOX_WALL_T, 0.0), white, "box_floor"),
                (floor, Vec3::new(0.0, BOX_HEIGHT + BOX_WALL_T, 0.0), white, "box_ceiling"),
                (back, Vec3::new(0.0, BOX_HEIGHT * 0.5, -h - BOX_WALL_T), white, "box_back"),
                (side, Vec3::new(-h - BOX_WALL_T, BOX_HEIGHT * 0.5, 0.0), [0.65, 0.05, 0.05], "box_left_red"),
                (side, Vec3::new(h + BOX_WALL_T, BOX_HEIGHT * 0.5, 0.0), [0.12, 0.45, 0.15], "box_right_green"),
            ];
            for (mesh, pos, color, name) in walls {
                spawn_object(world, mesh, neutral, pos, Quat::IDENTITY, lambert(color), name);
            }
            spawn_object(
                world,
                tall,
                neutral,
                Vec3::new(-0.6 * M, 0.9 * M, -0.5 * M),
                Quat::from_rotation_y(0.3),
                lambert(white),
                "box_tall_block",
            );
            spawn_object(
                world,
                short,
                neutral,
                Vec3::new(0.7 * M, 0.4 * M, 0.4 * M),
                Quat::from_rotation_y(-0.3),
                lambert(white),
                "box_short_cube",
            );
            spawn_reference_emitter(
                world,
                Vec3::new(0.0, BOX_HEIGHT - 0.3 * M, 0.0),
                3.0,
                "box_emitter",
            );
            (
                Vec3::new(0.0, BOX_HEIGHT * 0.5, h + 3.4 * M),
                Vec3::new(0.0, BOX_HEIGHT * 0.5, 0.0),
            )
        }
    };
    let sources = builder.finish_recording_sources();
    world.insert_resource(CornellMeshSources(sources));
    camera
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_every_rung_and_rejects_typos() {
        assert_eq!(cornell_reference_rung(&args(&[])).unwrap(), None);
        for rung in RUNGS {
            assert_eq!(
                cornell_reference_rung(&args(&["--cornell-reference", rung.name()])).unwrap(),
                Some(*rung)
            );
        }
        assert!(cornell_reference_rung(&args(&["--cornell-reference"])).is_err());
        assert!(cornell_reference_rung(&args(&["--cornell-reference", "cal-pt"])).is_err());
    }

    #[test]
    fn lambert_material_carries_the_reference_flag_and_no_specular() {
        let m = lambert([0.5; 3]);
        assert_ne!(
            m.effect_shader_flags & byroredux_renderer::vulkan::material::material_flag::LAMBERTIAN_REFERENCE,
            0
        );
        assert_eq!((m.metalness, m.specular_strength, m.ior), (0.0, 0.0, 1.0));
    }

    #[test]
    fn source_radius_stays_below_a_tenth_of_the_nearest_receiver() {
        // Nearest measured surfaces: the box ceiling 0.3 m above the emitter,
        // the calibration plane 1.5 m below it (RLT-REF-002, r/d ≤ 0.1).
        assert!(SOURCE_RADIUS_M / 0.3 <= 0.1);
        assert!(SOURCE_RADIUS_M * M >= 1.0, "below the shader's 1-unit sampling floor");
    }
}
