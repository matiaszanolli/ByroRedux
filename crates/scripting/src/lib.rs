//! Scripting subsystem — ECS-native event system.
//!
//! Replaces the Papyrus VM with native ECS patterns:
//! - Script state → component fields
//! - Script logic → ECS systems
//! - Events → transient marker components
//! - Timers → ScriptTimer component + tick system
//!
//! The core lifecycle: event marker appears → systems process it →
//! cleanup removes it at end of frame.

pub mod cleanup;
pub mod condition;
pub mod events;
pub mod papyrus_demo;
pub mod quest_stages;
pub mod recurring_update;
pub mod registry;
pub mod timer;
pub mod translate;

pub use cleanup::event_cleanup_system;
pub use condition::{
    evaluate as evaluate_condition_list, evaluate_condition, evaluate_function, ConditionContext,
    ConditionFunction,
};
pub use events::{
    ActivateEvent, AnimationTextKeyEvent, AnimationTextKeyEvents, HitEvent, OnCellLoadEvent,
    OnEquipEvent, OnTriggerEnterEvent, TimerExpired,
};
pub use recurring_update::{recurring_update_tick_system, OnUpdateEvent, RecurringUpdate};
pub use registry::{ScriptRegistry, ScriptSpawnFn};
pub use timer::{timer_tick_system, ScriptTimer};
pub use translate::{
    translate_pex, translate_script, CanonicalEvent, RecognizeCtx, Recognized, ScriptSource,
};

use byroredux_core::ecs::world::World;

/// Register all scripting component storages in the world.
///
/// Call during setup so that `query_mut()` works for event markers
/// before any entity has triggered an event.
pub fn register(world: &mut World) {
    world.register::<ActivateEvent>();
    world.register::<HitEvent>();
    world.register::<TimerExpired>();
    world.register::<AnimationTextKeyEvents>();
    world.register::<ScriptTimer>();
    // M47.0 Phase 5 — canonical event markers. OnCellLoadEvent +
    // OnTriggerEnterEvent + OnEquipEvent join the existing
    // ActivateEvent / HitEvent / TimerExpired in the script-event
    // catalog. Emit sites land per-phase:
    //   * OnCellLoadEvent — emitted by the cell loader's
    //     `attach_script_for_refr` (Phase 5, this commit).
    //   * OnTriggerEnterEvent — deferred to Rapier sensor wiring.
    //   * OnEquipEvent — deferred to M41 equip pipeline integration.
    world.register::<OnCellLoadEvent>();
    world.register::<OnTriggerEnterEvent>();
    world.register::<OnEquipEvent>();
    recurring_update::register(world);
    // M47.0 Phase 1 — register the R5 prototype storages so
    // `papyrus_demo` scripts can attach their state components when
    // their owning REFR spawns. Without this call, `query_mut::<…>`
    // returns None for every script-state component and the demo
    // systems no-op. The cell-loader-driven attach lands in Phase 3;
    // this Phase 1 step is the minimum plumbing that makes the demo
    // surface live at runtime. See docs/engine/m47-0-design.md.
    papyrus_demo::register(world);
    log::info!("Scripting subsystem initialized (ECS events + timers + papyrus_demo)");
}
