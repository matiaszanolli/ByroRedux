//! The ambient AI motion systems behind the single BYRO_NO_AI_LOCOMOTION kill switch.
//!
//! Split out of `post_update.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// The ambient AI motion systems behind the single BYRO_NO_AI_LOCOMOTION kill switch.
pub(super) fn register_ambient_ai_systems(scheduler: &mut Scheduler) {
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
    let locomotion_enabled = super::ai_locomotion_enabled();
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
        // M42 — FleeNotCombat (procedure 10): keep away from the PTDT target
        // or hold at the PLDT refuge. See `systems::flee_package` docs.
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_flee_package_system());
        // M42 — Find (procedure 0): go to a target and take / attack / greet /
        // activate it. See `systems::find_package` docs.
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_find_package_system());
        // M42.8 — Patrol: shares Wander's oscillating algorithm (no
        // patrol-route data is decoded anywhere yet — see
        // `systems::patrol` module docs).
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_patrol_system());
        // M42 — Eat/Sleep (procedures 3/4): walk once to the PLDT
        // location, then seat at the nearest matching furniture marker
        // through the sandbox path. See `systems::eat_sleep` docs.
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_eat_sleep_system());
        // M42.10 — walk-cycle playback. See `systems::walk_anim` module
        // docs for the take/restore/yield/abandon protocol.
        scheduler.add_exclusive(
            Stage::PostUpdate,
            crate::systems::make_npc_walk_animation_system(),
        );
        // M47.2 — the same motion, read for the behavior graphs of the actors the
        // walk-clip system does not drive (Fallout 4): `Speed` / `Direction` and
        // `moveStart` / `moveStop`. See `systems::graph_locomotion` module docs.
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::console_walk_system);
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::graph_locomotion_system);
    }
}
