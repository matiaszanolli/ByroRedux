//! `Stage::PostUpdate` registrations (#3855, split from `boot.rs`).

use byroredux_core::ecs::{Access, Scheduler, Stage, TotalTime, Transform};

use crate::systems::{
    make_transform_propagation_system, make_world_bound_propagation_system, particle_system,
};

/// `Stage::PostUpdate` registrations (#3739 split of `build_scheduler`).
pub(super) fn register_post_update_systems(scheduler: &mut Scheduler) {
    scheduler.add_to_with_access(
        Stage::PostUpdate,
        make_transform_propagation_system(),
        Access::new()
            .reads::<byroredux_core::ecs::Parent>()
            .reads::<byroredux_core::ecs::Children>()
            // WRITE (was read): the system drains the per-entity
            // Transform change-tracking dirty set, which needs &mut on
            // the storage. Local transforms are still only read.
            .writes::<Transform>()
            .writes::<byroredux_core::ecs::GlobalTransform>(),
    );
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
    // M42 — sandbox seat procedure. GATED OFF by default (opt in with
    // `BYRO_SANDBOX_SIT=1`). The seat placement + clip-swap pipeline is fully
    // verified (live bone inspection: actors land on the correct furniture
    // marker and the sit clip *is* applied — L-thigh matches the authored
    // folded pose). M42.1 fixed the earlier float bug (the generic
    // `dynamicidle_*` sit loops carry no pelvis/root channel) by holding the
    // FNV/FO3 sit-**enter** transition clip's final frame instead, which does
    // lower `Bip01`/`NonAccum` onto the seat; see `systems::sandbox` module
    // docs for the full mechanism. The rest of the M42 foundation (Sandbox
    // package tagging, `Furniture` markers, `Seated`, resources) stays live
    // regardless. Runs after transform propagation, same exclusive lane as
    // the systems above.
    if std::env::var_os("BYRO_SANDBOX_SIT").is_some() {
        log::info!(
            "BYRO_SANDBOX_SIT set — enabling sandbox seat-snap \
             (grounded sit-enter pose on FNV/FO3; see systems::sandbox docs for other games)"
        );
        scheduler.add_exclusive(
            Stage::PostUpdate,
            crate::systems::make_sandbox_seat_system(),
        );
    }
    // M42.3 — Wander locomotion. GATED OFF by default (opt in with
    // `BYRO_WANDER=1`), mirroring `BYRO_SANDBOX_SIT` above. Straight-line
    // walk-to-point, no pathing/NAVM, no animation-clip swap — see
    // `systems::wander` module docs for the full v0-scope list. Same
    // exclusive PostUpdate lane, after transform propagation, as
    // `sandbox_seat_system`; the two never touch the same actor (an NPC's
    // active package is a single winning `PackRecord`, so `SandboxBehavior`
    // and `WanderBehavior` are mutually exclusive), so their relative
    // order doesn't matter.
    if std::env::var_os("BYRO_WANDER").is_some() {
        log::info!("BYRO_WANDER set — enabling NPC wander locomotion (M42.3 v0)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_wander_system());
    }
    // M42.4 — Travel locomotion. GATED OFF by default (opt in with
    // `BYRO_TRAVEL=1`), mirroring `BYRO_WANDER`/`BYRO_SANDBOX_SIT` above.
    // Shares `wander_system`'s straight-line walk primitive via
    // `systems::locomotion::step_toward`, but walks once to a destination
    // and stops (terminal `Traveled` marker) instead of repeating — see
    // `systems::travel` module docs for the resolution/fallback mechanism
    // and the full v0-scope list. Same exclusive PostUpdate lane, after
    // transform propagation; Sandbox/Wander/Travel never touch the same
    // actor (a single winning `PackRecord` per NPC), so relative order
    // among the three doesn't matter.
    if std::env::var_os("BYRO_TRAVEL").is_some() {
        log::info!("BYRO_TRAVEL set — enabling NPC travel locomotion (M42.4 v0)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_travel_system());
    }
    // M42.5 — Follow locomotion. GATED OFF by default (opt in with
    // `BYRO_FOLLOW=1`), mirroring `BYRO_TRAVEL`/`BYRO_WANDER` above.
    // Shares the same `step_toward` locomotion primitive, but tracks a
    // *live* target's position every tick instead of a frozen destination
    // (Travel) or a hash-picked point (Wander) — see `systems::follow`
    // module docs for the PTDT target-resolution mechanism and the full
    // v0-scope list. Same exclusive PostUpdate lane, after transform
    // propagation; Sandbox/Wander/Travel/Follow never touch the same
    // actor (a single winning `PackRecord` per NPC), so relative order
    // among the four doesn't matter.
    if std::env::var_os("BYRO_FOLLOW").is_some() {
        log::info!("BYRO_FOLLOW set — enabling NPC follow locomotion (M42.5 v0)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_follow_system());
    }
    // M42.6 — Escort locomotion. GATED OFF by default (opt in with
    // `BYRO_ESCORT=1`), mirroring `BYRO_FOLLOW`/`BYRO_TRAVEL`/`BYRO_WANDER`
    // above. Shares the same `step_toward` locomotion primitive across two
    // phases — collect a live PTDT target (like Follow), then lead it to a
    // frozen PLDT destination and stop (like Travel, terminal `Escorted`
    // marker) — see `systems::escort` module docs for the full mechanism
    // and v0-scope list. Same exclusive PostUpdate lane, after transform
    // propagation; Sandbox/Wander/Travel/Follow/Escort never touch the
    // same actor (a single winning `PackRecord` per NPC), so relative
    // order among the five doesn't matter.
    if std::env::var_os("BYRO_ESCORT").is_some() {
        log::info!("BYRO_ESCORT set — enabling NPC escort locomotion (M42.6 v0)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_escort_system());
    }
    // M42.7 — Guard locomotion. GATED OFF by default (opt in with
    // `BYRO_GUARD=1`), mirroring `BYRO_ESCORT`/`BYRO_FOLLOW`/`BYRO_TRAVEL`/
    // `BYRO_WANDER` above. Shares `travel_system`'s `NearReference`-resolve
    // primitive (#2561) but diverges on fallback (home, not a random pick)
    // and never reaches a terminal state — holds the anchor indefinitely,
    // returning if displaced beyond its radius — see `systems::guard`
    // module docs. Same exclusive PostUpdate lane, after transform
    // propagation; Sandbox/Wander/Travel/Follow/Escort/Guard never touch
    // the same actor (a single winning `PackRecord` per NPC), so relative
    // order among the six doesn't matter.
    if std::env::var_os("BYRO_GUARD").is_some() {
        log::info!("BYRO_GUARD set — enabling NPC guard locomotion (M42.7 v0)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_guard_system());
    }
    // M42.8 — Patrol locomotion. GATED OFF by default (opt in with
    // `BYRO_PATROL=1`), mirroring the gates above. v0 Patrol is Wander's
    // exact random-point-in-radius algorithm under a different procedure
    // tag — no patrol-route data is decoded anywhere in this codebase, so
    // there is nothing to differentiate it on yet; see `systems::patrol`
    // module docs. Same exclusive PostUpdate lane.
    if std::env::var_os("BYRO_PATROL").is_some() {
        log::info!("BYRO_PATROL set — enabling NPC patrol locomotion (M42.8 v0, aliases Wander's algorithm)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_patrol_system());
    }
    // PostUpdate ordering contract (#1375 invariant pin, revised by #3652):
    //   1. transform_propagation — BFS GlobalTransform composition
    //   2. make_world_bound_propagation_system — drains GT dirty set,
    //      folds WorldBounds from the per-frame transforms available so far
    //
    // #3652 — `make_billboard_system` used to run here as step 2 (between
    // the two above), but its entire input is the active camera's
    // GlobalTransform, which `camera_follow_system` authors in `Stage::Late`
    // in player/third-person mode. `Stage::Late` runs strictly after
    // `Stage::PostUpdate`, so billboard was reading last frame's camera pose
    // every frame — visible as shearing/sliver billboard quads on a fast
    // camera turn, resolving the instant the camera stopped (see
    // `register_late_systems` for why `camera_follow_system` itself cannot
    // move the other direction, to PostUpdate: it needs `Stage::Physics`'s
    // post-step body position). Moved to `Stage::Late`, immediately after
    // `camera_follow_system`, below.
    //
    // INVARIANT: no Stage::Late system may write GlobalTransform on
    // a LocalBound-bearing entity. If it does, that entity's
    // WorldBound will silently lag one frame because bound
    // propagation's GT drain fires before the Late write. `camera_follow_
    // system` + audio emitters write GT in Late but carry no LocalBound —
    // the lag is benign for them. `make_billboard_system` is the first
    // exception: billboard meshes DO carry a LocalBound. The resulting
    // one-frame-stale WorldBound is accepted (not proven zero-cost) — a
    // billboard quad's local bounding-sphere center sits at its own pivot
    // for essentially every vanilla asset, and rotation about a sphere's own
    // center doesn't move it, so the practical drift is at most the
    // radius-scale term (rotation-invariant) recomputed from a pose one
    // frame stale, versus the shearing defect being visibly wrong on every
    // fast turn. Any future Late system that writes GT on a bounded entity
    // must make the same call, or be promoted to PostUpdate (before bounds).
    //
    // #2391 — the two systems of this PostUpdate chain declare their access
    // (`add_exclusive_with_access`) even though the analyzer doesn't pair
    // exclusives: the ordering contract above is entirely about who touches
    // `GlobalTransform` when, and a blank `sys.accesses` row is exactly the
    // wrong place for that to be invisible.
    // Bound propagation runs last in PostUpdate so it sees final
    // world transforms for everything PostUpdate itself writes. See #217.
    // #3652 — billboard rotations moved to Stage::Late (below), so this no
    // longer sees the CURRENT frame's billboard pose; see this function's
    // INVARIANT comment above for why that's accepted.
    //
    // The `GlobalTransform` entry is a **write**: the system takes a
    // write guard on that storage to drain its change-tracking dirty set
    // (`bounds.rs`, `drain_dirty_into`) before reading transforms
    // through it. It mutates no transform values, but the lock it takes
    // is exclusive, and the declaration describes lock acquisition.
    scheduler.add_exclusive_with_access(
        Stage::PostUpdate,
        make_world_bound_propagation_system(),
        Access::new()
            .reads::<byroredux_core::ecs::LocalBound>()
            .reads::<byroredux_core::ecs::Parent>()
            .reads::<byroredux_core::ecs::Children>()
            .reads::<byroredux_core::ecs::SkinnedMesh>()
            .writes::<byroredux_core::ecs::GlobalTransform>()
            .writes::<byroredux_core::ecs::WorldBound>(),
    );
}
