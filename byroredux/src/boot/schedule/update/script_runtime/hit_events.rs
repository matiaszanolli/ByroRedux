//! Reference-script, Papyrus and ObScript hit and combat events.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Reference-script, Papyrus and ObScript hit and combat events.
pub(super) fn register_hit_events_systems(scheduler: &mut Scheduler) {
    // #5991 — the reference / alias scripts' `OnHit` (and its remote relays) read the same
    // markers, so they are delivered here, after the projectile and trap hits have landed;
    // `reference_script_event_system` above delivers every other marker.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::reference_script_hit_system);
    // M47.2 — the hits scripts registered for (Fallout 4 / Starfield
    // `RegisterForHitEvent`), delivered once each while the `HitEvent` markers
    // stand.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::hit_event_system);
    // M47.3 — the same markers for the ObScript `OnHit` / `OnHitWith` blocks of a struck
    // actor's script, and the combat transitions for `OnStartCombat` / `OnCombatEnd`.
    // Here rather than beside `obscript_object_system`: the projectile and trap hits
    // above are produced after that one runs.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_hit_system);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_combat_system);
}
