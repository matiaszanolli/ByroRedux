//! Magic effects, magic hits and the magic-shader clock and tint.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Magic effects, magic hits and the magic-shader clock and tint.
pub(super) fn register_magic_systems(scheduler: &mut Scheduler) {
    // M47.3 — the magic effects running on actors: `ScriptEffectStart` / `Update` / `Finish`, value
    // changes over time, and the `OnMagicEffectHit` they raised when they landed. After the ObScript
    // systems whose `Cast` / `CastImmediateOnSelf` start them.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::magic_effects::magic_effect_system);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::magic_effects::magic_hit_system);
    // M47.3 — magic shaders: advance their clocks, then glow (or restore) the surfaces they are on.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::magic_shaders::magic_shader_clock_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::magic_shader::magic_shader_tint_system);
}
