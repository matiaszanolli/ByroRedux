//! The light level at the player (Q3, SNK-DET-009; design §6): computed on the
//! CPU from the lights the renderer is given, so it is deterministic, testable
//! and has no frame of latency. It is blind to ray-traced shadowing (PRD §6
//! row 1); `stealth.status` prints it.

use byroredux_core::ecs::components::{GlobalTransform, LightSource};
use byroredux_core::ecs::World;
use byroredux_core::lighting::Meters;
use byroredux_core::math::Vec3;

/// Rec. 709 luminance weights.
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

fn luminance(rgb: [f32; 3]) -> f32 {
    rgb[0] * LUMA[0] + rgb[1] * LUMA[1] + rgb[2] * LUMA[2]
}

/// The cell's ambient share of [`light_level_at`], 0 to 1 (Fallout 4's interior light-level mod takes at most
/// this much off, #6996).
pub(crate) fn ambient_light(world: &World) -> f32 {
    world.try_resource::<crate::components::CellLightingRes>().map_or(0.0, |cell| luminance(cell.ambient).clamp(0.0, 1.0))
}

/// The light level at `position`, 0 to 1: the cell's ambient, the directional
/// light in an exterior (an interior's XCLL directional is a tint, not a sun),
/// and each placed light's colour through the renderer's own distance law
/// (`Emitter::distance_attenuation`), as luminance, clamped.
pub(crate) fn light_level_at(world: &World, position: Vec3) -> f32 {
    let mut level = ambient_light(world);
    if let Some(cell) = world.try_resource::<crate::components::CellLightingRes>() {
        if !cell.is_interior {
            level += luminance(cell.directional_color);
        }
    }
    let lights: Vec<(LightSource, Vec3)> = match (world.query::<LightSource>(), world.query::<GlobalTransform>()) {
        (Some(lights), Some(transforms)) => lights
            .iter()
            .filter_map(|(entity, light)| transforms.get(entity).map(|t| (*light, t.translation)))
            .collect(),
        _ => Vec::new(),
    };
    for (light, at) in lights {
        let distance = Meters::from_bethesda_units(at.distance(position));
        let colour = light.emitter.radiant_intensity.get();
        level += luminance(colour) * light.dimmer * light.intensity * light.emitter.distance_attenuation(distance);
    }
    level.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::lighting::{Emitter, EmitterKind, VisibilityMask};

    fn light(range: f32, colour: [f32; 3]) -> LightSource {
        LightSource {
            emitter: Emitter::from_legacy_world_units(
                range,
                colour,
                EmitterKind::Point,
                [0.0; 3],
                0.0,
                1.0,
                VisibilityMask::for_legacy_local_light(),
            ),
            ..LightSource::default()
        }
    }

    #[test]
    fn a_nearby_light_brightens_and_distance_dims() {
        let mut world = World::new();
        world.register::<LightSource>();
        world.register::<GlobalTransform>();
        assert_eq!(light_level_at(&world, Vec3::ZERO), 0.0, "no lights, no cell");
        let lamp = world.spawn();
        world.insert(lamp, light(256.0, [1.0, 0.9, 0.7]));
        world.insert(lamp, GlobalTransform::new(Vec3::ZERO, byroredux_core::math::Quat::IDENTITY, 1.0));
        let near = light_level_at(&world, Vec3::new(32.0, 0.0, 0.0));
        let far = light_level_at(&world, Vec3::new(400.0, 0.0, 0.0));
        assert!(near > far, "{near} > {far}");
        assert!(near <= 1.0 && far >= 0.0);
    }
}
