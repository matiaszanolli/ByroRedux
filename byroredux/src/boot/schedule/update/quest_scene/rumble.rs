//! Papyrus-demo controller rumble on activation, and its tick.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage, World};

/// Papyrus-demo controller rumble on activation, and its tick.
pub(super) fn register_rumble_systems(scheduler: &mut Scheduler) {
    // M47.0 Phase 1 — R5 papyrus_demo dispatchers. These are
    // event-driven (early-return when no ActivateEvent /
    // OnUpdateEvent / RecurringUpdate is present), so they
    // contribute zero work to frames without scripted activity.
    // Registered as exclusive in Update so they run serially
    // after the parallel batch — keeps M27's zero-conflict
    // report intact without forcing each demo to declare its
    // (component-heavy) access surface. Declared access lands
    // when the demos move to Stage::Script in a follow-up; for
    // now they ride the same "trivial gameplay system =
    // exclusive" lane as spin_system / footstep_system /
    // particle_system. See docs/engine/m47-0-design.md.
    //
    // Signature note: the 6 World-only demos predate engine
    // wiring (R5 was a prototype against unit tests that called
    // them directly). The blanket `impl<F: FnMut(&World, f32)>
    // System for F` requires the `dt` arg, so we adapt via inline
    // closures that drop the unused parameter. Renaming the demo
    // signatures themselves would break ~30 unit-test call sites
    // in papyrus_demo/tests.rs that intentionally call without
    // dt; closures are cheaper.
    // F7 (2026-05-27) — wrap each `&World`-only papyrus-demo
    // system as a local `fn` item so it gets a unique
    // `std::any::type_name`. The previous closures all shared the
    // auto-generated name `byroredux::App::new::{{closure}}`,
    // tripping the scheduler's `has_system(name)` duplicate-name
    // probe and emitting 5+ "duplicate exclusive system" warnings
    // per launch (cosmetic — the systems still all got added).
    // Local fn items each have their own unique type and unique
    // type-name path, so the duplicate check passes naturally.
    fn rumble_on_activate_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::rumble_on_activate_system(world)
    }
    scheduler.add_exclusive(Stage::Update, rumble_on_activate_dispatch);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::papyrus_demo::rumble_tick_system,
    );
}
