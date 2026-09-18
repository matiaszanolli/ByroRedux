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
    // M42.10 — ambient AI locomotion is **live by default**. The seven
    // systems below sat behind per-procedure opt-in env gates from M42.1
    // through M42.9 while each procedure's pipeline was being verified in
    // isolation (the ambient_locomotion_default_on_tests pin guards
    // against their reintroduction);
    // with the walk cycle animated (`npc_walk_animation_system`) and the
    // per-tick move physics-backed (KCC collide-and-slide in
    // `locomotion::step_toward_detailed`), the gates' reason — "don't ship
    // an unverified behavior to every cell" — no longer applies, and
    // NPCs loaded from real cells now walk their authored packages
    // without operator flags.
    //
    // `BYRO_NO_AI_LOCOMOTION=1` is the single kill-switch (a debugging
    // aid for isolating renderer/perf work from AI motion, and an escape
    // hatch if a cell turns up a content regression), replacing the seven
    // opt-in variables — which are no longer read.
    //
    // All eight run in the same exclusive PostUpdate lane, after transform
    // propagation (NPC placement roots are propagation roots — no
    // `Parent` — so `Transform` == world position for them). The two
    // invariants that make the order below safe:
    //
    // 1. The six procedure systems never touch the same actor as
    //    `sandbox_seat_system` (a single winning `PackRecord` per NPC
    //    installs exactly one behavior component), so their relative
    //    order among themselves is free.
    // 2. `npc_walk_animation_system` must be registered **last**: it
    //    classifies an actor as moving/stationary from this frame's final
    //    position, so it has to observe the post-locomotion transform,
    //    and its take/restore protocol assumes every mover has already
    //    written for the tick (a swap decided on a pre-move position
    //    would flicker at leg boundaries).
    let locomotion_enabled = std::env::var_os("BYRO_NO_AI_LOCOMOTION").is_none();
    if locomotion_enabled {
        // M42.1 — sandbox seat-snap. The seat placement + clip-swap
        // pipeline is fully verified (live bone inspection: actors land on
        // the correct furniture marker and the sit clip *is* applied —
        // L-thigh matches the authored folded pose). M42.1 fixed the
        // earlier float bug (the generic `dynamicidle_*` sit loops carry
        // no pelvis/root channel) by holding the FNV/FO3 sit-**enter**
        // transition clip's final frame instead, which does lower
        // `Bip01`/`NonAccum` onto the seat; see `systems::sandbox` module
        // docs for the full mechanism. The rest of the M42 foundation
        // (Sandbox package tagging, `Furniture` markers, `Seated`,
        // resources) runs regardless.
        scheduler.add_exclusive(
            Stage::PostUpdate,
            crate::systems::make_sandbox_seat_system(),
        );
        // M42.3 — Wander: straight-line walk-to-point, walk-to-pause
        // oscillation. See `systems::wander` module docs.
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_wander_system());
        // M42.4 — Travel: walk once to a destination, terminal `Traveled`.
        // See `systems::travel` module docs.
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_travel_system());
        // M42.5 — Follow: live-target stand-off tracking, terminal-less.
        // See `systems::follow` module docs.
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_follow_system());
        // M42.6 — Escort: collect a live target, then lead it to a frozen
        // destination, terminal `Escorted`. See `systems::escort` docs.
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_escort_system());
        // M42.7 — Guard: hold an anchor + leash, never terminal. See
        // `systems::guard` module docs.
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_guard_system());
        // M42.8 — Patrol: shares Wander's oscillating algorithm (no
        // patrol-route data is decoded anywhere yet — see
        // `systems::patrol` module docs).
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_patrol_system());
        // M42.10 — walk-cycle playback. See `systems::walk_anim` module
        // docs for the take/restore/yield/abandon protocol.
        scheduler.add_exclusive(
            Stage::PostUpdate,
            crate::systems::make_npc_walk_animation_system(),
        );
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
