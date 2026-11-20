//! The receding-plane lab (RCG-VAL-005): a reproducible, game-data-free exterior whose only
//! content is a flat Lambertian ground plane running from the camera's near clamp to the far plane,
//! under the engine's procedural-fallback sky and one fixed sun. Activated with `--recede-lab`
//! (handled in [`crate::scene::setup_scene`]).
//!
//! It exists to measure two things the radiance cache's level of detail owes the owner:
//!
//! - RCG-LOD-002: a receding flat receiver must show no luminance step across a cache level boundary,
//!   so the capture must hold nothing that changes the plane's brightness but distance;
//! - RCG-PERF-004: the traced-share cost per eligible pixel must be flat across distance, so the
//!   pixels must be one material from the foot of the frame to the horizon.
//!
//! The camera stands at eye height over the plane's centre and looks along `-Z` at the horizon, so
//! the plane fills the lower half of the frame and a pixel's world footprint grows from centimetres at
//! the bottom edge to kilometres at the horizon. The sun stands to the side (`+X`), so every point of
//! the plane is lit at the same angle and nothing casts a shadow onto it: a brightness change along
//! the view is the sky and indirect term and the cache's level scheme, not the lab. No local light is
//! installed and the weather system rewrites the exterior environment every frame, so the lab sets
//! no medium of its own (the same choice as the shaft lab).
use super::*;
use byroredux_core::ecs::components::DEFAULT_RENDER_DISTANCE;

/// Sun elevation of the lab, degrees. High enough that the plane is well lit and the sun's angle on
/// it is the same everywhere; low enough that the sky term is not the whole signal.
pub(crate) const RECEDE_LAB_SUN_ELEVATION_DEG: f32 = 35.0;
/// Camera eye height over the plane, metres.
pub(crate) const RECEDE_LAB_EYE_HEIGHT_M: f32 = 1.7;
/// How far ahead the camera looks, metres. Far enough that the view is level.
const LOOK_AHEAD_M: f32 = 1000.0;
/// Thickness of the plane's slab, metres. The plane's top face sits at `y = 0`.
const PLANE_HALF_THICKNESS_M: f32 = 0.05;
const GROUND_COLOR: [f32; 3] = [0.5, 0.5, 0.5];

/// `--recede-lab`. Exact match only, so `--recede-lab-extra` selects nothing.
pub(crate) fn recede_lab_mode(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--recede-lab")
}

/// The unit direction TO the sun in the engine's Y-up convention: elevation
/// [`RECEDE_LAB_SUN_ELEVATION_DEG`], toward `+X`, so it is perpendicular to the view direction.
pub(crate) fn recede_lab_sun_direction() -> [f32; 3] {
    let elevation = RECEDE_LAB_SUN_ELEVATION_DEG.to_radians();
    [elevation.cos(), elevation.sin(), 0.0]
}

/// Half-extent of the plane in world units, in both horizontal directions: the camera's far plane
/// ([`DEFAULT_RENDER_DISTANCE`]), so the plane reaches the horizon along every view ray.
pub(crate) fn recede_lab_half_extent() -> f32 {
    DEFAULT_RENDER_DISTANCE
}

pub(crate) fn setup_recede_lab_scene(world: &mut World, ctx: &mut VulkanContext) -> (Vec3, Vec3) {
    use crate::fog::WORLD_UNITS_PER_METER as UPM;
    install_cornell_lighting(world, true);
    let sun = recede_lab_sun_direction();
    {
        let mut cell = world.resource_mut::<CellLightingRes>();
        cell.is_interior = false;
        cell.directional_dir = sun;
    }
    world.resource_mut::<crate::components::SkyParamsRes>().sun_direction = sun;

    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);
    let half_extent = recede_lab_half_extent();
    // One upload per entity: unload releases one mesh reference per holder.
    let mesh = builder.box_mesh([half_extent, PLANE_HALF_THICKNESS_M * UPM, half_extent]);
    spawn_object(
        world,
        mesh,
        neutral,
        Vec3::new(0.0, -PLANE_HALF_THICKNESS_M * UPM, 0.0),
        Quat::IDENTITY,
        matte(GROUND_COLOR),
        "recede_plane",
    );
    builder.finish();

    (
        Vec3::new(0.0, RECEDE_LAB_EYE_HEIGHT_M * UPM, 0.0),
        Vec3::new(0.0, RECEDE_LAB_EYE_HEIGHT_M * UPM, -LOOK_AHEAD_M * UPM),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_flag_is_an_exact_match() {
        assert!(recede_lab_mode(&args(&["--recede-lab"])));
        assert!(recede_lab_mode(&args(&["--bench-frames", "60", "--recede-lab"])));
        assert!(!recede_lab_mode(&args(&["--recede-lab-extra"])));
        assert!(!recede_lab_mode(&args(&["--shaft-lab"])));
        assert!(!recede_lab_mode(&args(&[])));
    }

    #[test]
    fn the_sun_is_a_unit_vector_perpendicular_to_the_view() {
        let [x, y, z] = recede_lab_sun_direction();
        assert!((x * x + y * y + z * z - 1.0).abs() < 1e-6);
        // The camera looks along -Z: no component of the sun along the view, so every point of the
        // plane is lit at the same angle.
        assert_eq!(z, 0.0);
        assert!(x > 0.0 && y > 0.0);
        assert!((y.asin().to_degrees() - RECEDE_LAB_SUN_ELEVATION_DEG).abs() < 1e-3);
    }

    #[test]
    fn the_plane_reaches_the_far_plane_in_every_direction() {
        assert_eq!(recede_lab_half_extent(), DEFAULT_RENDER_DISTANCE);
    }
}
