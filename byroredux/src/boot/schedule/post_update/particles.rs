//! Particle simulation, after transform propagation.
//!
//! Split out of `post_update.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage, TotalTime};
use crate::systems::particle_system;

/// Particle simulation, after transform propagation.
pub(super) fn register_particles_systems(scheduler: &mut Scheduler) {
    // Particle simulation runs after transform propagation so emitter
    // entities have their final world-space spawn origin (#401).
    //
    // #3653 — declared (rather than a bare `add_exclusive`) because this is
    // the *consuming* half of a deliberate cross-stage lag, and it was the
    // only half `sys.accesses` could not see. `submersion_system`
    // (`Stage::Late`) writes `ParticleEmitter::rate` for every water volume
    // the camera disturbs, and this system — the sole consumer of `rate` —
    // runs in `Stage::PostUpdate`, which executes first. So the rate spawned
    // against in frame N is the one computed in frame N-1.
    //
    // That one-frame lag is ACCEPTED, not overlooked. The alternative in the
    // finding — hoisting the rate write into a PostUpdate step ahead of this
    // one — rests on the premise that the write "needs only `ActiveCamera` +
    // `WaterVolume`, none of the Late-authored camera pose". That premise is
    // false: `disturbance_rate(cam_pos, volume)` is a pure function of the
    // camera's `GlobalTransform`, which `camera_follow_system` authors in
    // `Stage::Late` in player / third-person mode. Moving the write earlier
    // would not remove the frame of latency, it would relocate it from the
    // spawn rate onto the camera position that determines the rate — which
    // is precisely the bug #3180 closed by moving `submersion_system` to
    // Late in the first place. One frame of ripple-density latency at the
    // instant the player enters or leaves water is the cheaper of the two.
    //
    // The analyzer cannot see any of this (`analyze_pair` is intra-stage),
    // so `particle_emitter_rate_lag_is_structural` in
    // `scheduler_access_tests.rs` pins the stage assignment instead.
    scheduler.add_exclusive_with_access(
        Stage::PostUpdate,
        particle_system,
        Access::new()
            .reads_resource::<TotalTime>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .writes::<byroredux_core::ecs::components::ParticleEmitter>(),
    );
}
