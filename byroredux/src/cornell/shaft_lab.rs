//! The exterior shaft lab (#6726, VLM-SH-001): a reproducible, game-data-free exterior for the
//! exterior sunlight-shaft path, plus its `--shaft-lab` / `--shaft-lab-solid` mode parsing. The
//! outdoor sibling of the interior [`godray_lab`](super::godray_lab).
//!
//! A low sun stands behind a long wall in open sky. In the open variant the wall is a row of slats
//! with gaps between them, so the sun's light passes the gaps and lights slanted slabs of air in
//! front of the wall: the shafts. `--shaft-lab-solid` replaces the slats with one unbroken wall, the
//! matched negative control. Everything else, camera included, is identical, so a pixel difference
//! between the two captures is the shaft and nothing else (`exterior-shafts.sh`, VLM-SH-002).
//!
//! The medium is whatever the engine's procedural-fallback exterior supplies (`0.000777 / m` before the
//! base-fog scale, about 1.8 times FNV's clear weather, read from `env.health`), not a lab-thick fog: the
//! scene exists to show what a clear exterior does, which is what the exterior haze floor (VLM-SH-010)
//! changes. The weather system rewrites the cell's medium every frame, so the lab does not set one of its
//! own. Sizes are metres, converted at the use sites.
use super::*;

/// Sun elevation of the lab, degrees. Low enough that the shafts are long and the floor's horizon
/// gain is in play, high enough that the sun clears the wall's far side.
pub(crate) const SHAFT_LAB_SUN_ELEVATION_DEG: f32 = 10.0;
/// Number of gaps in the open wall's row of slats.
pub(crate) const SHAFT_LAB_SLATS: usize = 12;

const SLAT_HALF_WIDTH_M: f32 = 0.6;
const SLAT_PITCH_M: f32 = 3.0;
const WALL_HEIGHT_M: f32 = 14.0;
const WALL_THICKNESS_M: f32 = 1.0;
const GROUND_COLOR: [f32; 3] = [0.34, 0.32, 0.28];
const WALL_COLOR: [f32; 3] = [0.55, 0.5, 0.45];

/// `--shaft-lab` (`Some(false)`, the open variant) or `--shaft-lab-solid` (`Some(true)`, the control).
/// Exact matches only, so `--shaft-lab-extra` selects nothing.
pub(crate) fn shaft_lab_mode(args: &[String]) -> Option<bool> {
    if args.iter().any(|arg| arg == "--shaft-lab-solid") {
        Some(true)
    } else if args.iter().any(|arg| arg == "--shaft-lab") {
        Some(false)
    } else {
        None
    }
}

/// The unit direction TO the sun, in the engine's Y-up convention: elevation
/// [`SHAFT_LAB_SUN_ELEVATION_DEG`], toward `-Z` so the sun stands behind the wall for a camera on `+Z`.
pub(crate) fn shaft_lab_sun_direction() -> [f32; 3] {
    let elevation = SHAFT_LAB_SUN_ELEVATION_DEG.to_radians();
    [0.0, elevation.sin(), -elevation.cos()]
}

pub(crate) fn setup_shaft_lab_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    solid: bool,
) -> (Vec3, Vec3) {
    use crate::fog::WORLD_UNITS_PER_METER as UPM;
    install_cornell_lighting(world, true);
    let sun = shaft_lab_sun_direction();
    {
        let mut cell = world.resource_mut::<CellLightingRes>();
        cell.is_interior = false;
        cell.directional_dir = sun;
    }
    world.resource_mut::<crate::components::SkyParamsRes>().sun_direction = sun;

    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);
    let mut slabs: Vec<([f32; 3], Vec3, [f32; 3], String)> = vec![(
        // Roughly 200 m a side: past the 128 m froxel range in every direction.
        [100.0 * UPM, 0.05 * UPM, 100.0 * UPM],
        Vec3::new(0.0, -0.05 * UPM, 0.0),
        GROUND_COLOR,
        "shaft_ground".to_string(),
    )];
    let half_height = WALL_HEIGHT_M * 0.5 * UPM;
    let half_thickness = WALL_THICKNESS_M * 0.5 * UPM;
    if solid {
        let half_width = (SHAFT_LAB_SLATS as f32 * SLAT_PITCH_M) * 0.5 * UPM;
        slabs.push((
            [half_width, half_height, half_thickness],
            Vec3::new(0.0, half_height, 0.0),
            WALL_COLOR,
            "shaft_wall_solid".to_string(),
        ));
    } else {
        let first = -(SHAFT_LAB_SLATS as f32 - 1.0) * 0.5 * SLAT_PITCH_M;
        for slat in 0..SHAFT_LAB_SLATS {
            let x = first + slat as f32 * SLAT_PITCH_M;
            slabs.push((
                [SLAT_HALF_WIDTH_M * UPM, half_height, half_thickness],
                Vec3::new(x * UPM, half_height, 0.0),
                WALL_COLOR,
                format!("shaft_slat_{slat}"),
            ));
        }
        // A lintel along the top so the slats read as one structure and the sun cannot pass over it.
        let half_width = (SHAFT_LAB_SLATS as f32 * SLAT_PITCH_M) * 0.5 * UPM;
        slabs.push((
            [half_width, 0.5 * UPM, half_thickness],
            Vec3::new(0.0, WALL_HEIGHT_M * UPM + 0.5 * UPM, 0.0),
            WALL_COLOR,
            "shaft_lintel".to_string(),
        ));
    }
    for (half_extents, position, color, name) in slabs {
        // One upload per entity: unload releases one mesh reference per holder.
        let mesh = builder.box_mesh(half_extents);
        spawn_object(world, mesh, neutral, position, Quat::IDENTITY, matte(color), &name);
    }
    builder.finish();

    // The camera stands 55 m out on the shadow side, ON the sun's axis and looking along it (up the
    // sun's elevation), so every view ray runs nearly parallel to the beams and stays inside one lit
    // or one shadowed slab for its whole length: that is where a shaft is a crisp stripe. Off axis, a
    // ray crosses the 3 m slat pitch every few metres and averages the stripes away. The wall's top
    // (14 m) clears the sun at this range, so the sun disc is hidden and only the beams remain.
    let eye_height = 1.7;
    let distance = 55.0;
    let rise = distance * SHAFT_LAB_SUN_ELEVATION_DEG.to_radians().tan();
    (
        Vec3::new(0.0, eye_height * UPM, distance * UPM),
        Vec3::new(0.0, (eye_height + rise) * UPM, 0.0),
    )
}
