//! The Creation-era loading-screen turntable (Skyrim/FO4 LSCR model
//! stages). The original menus slowly rotate the loading model about the
//! vertical axis while the cell streams in; this system drives the same
//! slow turn on the stage root. FO4+ records can opt out via the LSCR
//! header's "No Rotation" flag (0x8000), carried on the marker component.

use byroredux_core::ecs::{Transform, World};
use byroredux_core::math::Quat;

use crate::components::LoadingModelStage;

/// Auto-rotation rate, radians per second (~23°/s — the classic Bethesda
/// load-screen turn). A presentation constant of this backend, not an
/// authored value: the authored rotation input (ONAM bounds + user drag)
/// is a separate, future surface.
pub(crate) const LOADING_TURNTABLE_RAD_PER_S: f32 = 0.4;

/// Rotate every live loading-model root around the world vertical axis,
/// unless the record opted out. Roots, not meshes: the stage pose (TRNS /
/// SNAM-RNAM-XNAM) composes on top of the root transform, so rotating the
/// root turns the whole posed model in place.
pub(crate) fn loading_model_turntable_system(world: &World, dt: f32) {
    if let Some((sq, mut tq)) = world.query_2_mut::<LoadingModelStage, Transform>() {
        for (entity, stage) in sq.iter() {
            if stage.no_rotation {
                continue;
            }
            if let Some(transform) = tq.get_mut(entity) {
                let spin = Quat::from_rotation_y(dt * LOADING_TURNTABLE_RAD_PER_S);
                transform.rotation = spin * transform.rotation;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_turntable_rotates_the_root_and_the_no_rotation_flag_holds_it() {
        let mut world = World::new();
        let spinning = world.spawn();
        world.insert(
            spinning,
            LoadingModelStage {
                no_rotation: false,
            },
        );
        world.insert(spinning, Transform::default());
        let held = world.spawn();
        world.insert(held, LoadingModelStage { no_rotation: true });
        world.insert(held, Transform::default());

        loading_model_turntable_system(&world, 0.5);
        let tq = world.query::<Transform>().unwrap();
        let spun = tq.get(spinning).unwrap().rotation;
        assert!(
            (spun.angle_between(Quat::IDENTITY) - 0.5 * LOADING_TURNTABLE_RAD_PER_S).abs() < 1e-5
        );
        assert_eq!(tq.get(held).unwrap().rotation, Quat::IDENTITY);
    }
}
