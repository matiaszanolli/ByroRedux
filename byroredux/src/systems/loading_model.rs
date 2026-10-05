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
///
/// Driven by the [`crate::components::LoadingCoverClock`], not the
/// scheduler's `dt`: `about_to_wait` pins `dt` to `0.0` for exactly the
/// frames this stage exists (simulation time holds still under the cover),
/// which made the turntable a no-op for its entire life pre-#5306. The
/// clock is the wall-clock advance stamped next to `DeltaTime`; an absent
/// resource or a `0.0` stamp degrades to no rotation.
pub(crate) fn loading_model_turntable_system(world: &World, _dt: f32) {
    let clock = world
        .try_resource::<crate::components::LoadingCoverClock>()
        .map(|clock| clock.0)
        .unwrap_or(0.0);
    if clock <= 0.0 {
        return;
    }
    if let Some((sq, mut tq)) = world.query_2_mut::<LoadingModelStage, Transform>() {
        for (entity, stage) in sq.iter() {
            if stage.no_rotation {
                continue;
            }
            if let Some(transform) = tq.get_mut(entity) {
                let spin = Quat::from_rotation_y(clock * LOADING_TURNTABLE_RAD_PER_S);
                transform.rotation = spin * transform.rotation;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::LoadingCoverClock;
    use byroredux_core::ecs::{Scheduler, Stage};

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
        world.insert_resource(LoadingCoverClock(0.5));

        loading_model_turntable_system(&world, 0.0);
        let tq = world.query::<Transform>().unwrap();
        let spun = tq.get(spinning).unwrap().rotation;
        assert!(
            (spun.angle_between(Quat::IDENTITY) - 0.5 * LOADING_TURNTABLE_RAD_PER_S).abs() < 1e-5
        );
        assert_eq!(tq.get(held).unwrap().rotation, Quat::IDENTITY);
    }

    /// #5306 — the real scheduler path. `about_to_wait` runs the scheduler
    /// with `dt = 0.0` while the cover is active, so a pre-fix scheduler
    /// tick rotated nothing. The clock resource is what carries the
    /// motion: with `dt = 0.0` and a stamped clock, the Update-stage
    /// exclusive must still spin the root.
    #[test]
    fn scheduler_tick_with_dt_zero_still_spins_via_the_cover_clock() {
        let mut world = World::new();
        let root = world.spawn();
        world.insert(
            root,
            LoadingModelStage {
                no_rotation: false,
            },
        );
        world.insert(root, Transform::default());
        // Cover up: the App stamps the wall-clock advance while the
        // scheduler's dt is held at 0.0 — the exact pair about_to_wait
        // produces under an active cover.
        world.insert_resource(LoadingCoverClock(1.0 / 60.0));

        let mut scheduler = Scheduler::new();
        scheduler.add_exclusive(Stage::Update, loading_model_turntable_system);
        scheduler.run(&world, 0.0);

        let tq = world.query::<Transform>().unwrap();
        let spun = tq.get(root).unwrap().rotation;
        // f32 `angle_between` (acos of a dot product near 1) loses precision
        // at this ~0.0067 rad magnitude — allow ~1.5% relative.
        let expected = (1.0 / 60.0) * LOADING_TURNTABLE_RAD_PER_S;
        assert!(
            (spun.angle_between(Quat::IDENTITY) - expected).abs() < 1e-4,
            "a scheduler tick with dt = 0 must still rotate the stage root \
             via the cover clock (got angle {}, expected {expected})",
            spun.angle_between(Quat::IDENTITY)
        );
    }

    /// #5306 — cover down (clock stamped 0.0): the system must be inert
    /// even though the scheduler still runs it every frame.
    #[test]
    fn a_zero_cover_clock_is_inert() {
        let mut world = World::new();
        let root = world.spawn();
        world.insert(
            root,
            LoadingModelStage {
                no_rotation: false,
            },
        );
        world.insert(root, Transform::default());
        world.insert_resource(LoadingCoverClock(0.0));

        loading_model_turntable_system(&world, 1.0 / 60.0);
        let tq = world.query::<Transform>().unwrap();
        assert_eq!(
            tq.get(root).unwrap().rotation,
            Quat::IDENTITY,
            "a 0.0 cover clock must not rotate — the cover is down"
        );
    }
}
