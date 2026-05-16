//! Minimal `ActorStats` stand-in for Papyrus's `Actor.GetActorValue` /
//! `Actor.SetActorValue` / `Actor.ModActorValue` surface.
//!
//! Lives under `papyrus_demo` because it's R5-prototype scope, not
//! the production actor-value system. The real implementation lands
//! with M47.1 (Condition Eval) — at that point the static side
//! reads from parsed `AVIF` records (in
//! `byroredux_plugin::esm::records::AvifRecord`) and the runtime
//! side wires through the perk-modifier composition pipeline
//! described in [`actor_value_system.md`](../../../../docs/r5/source/)
//! et al. The shape here is the smallest thing that lets the
//! `DLC2TTR4aPlayerScript` translation read its polled stat.
//!
//! ## What's intentionally minimal
//!
//! - String-keyed lookup. The real ActorValue surface is a
//!   126-entry enum (per `AVIF`) with typed access; the prototype
//!   uses a `HashMap<&'static str, f32>` because the demo only
//!   needs one or two stats and naming them by string matches the
//!   Papyrus source directly (`GetActorValue("Variable05")`).
//! - Base value only. No modifier composition (base + perk
//!   modifier + temporary modifier - damage). That's M47.1 surface.
//! - Per-entity component, not a global registry. Each actor
//!   carries its own `ActorStats`. The player gets one at world
//!   init; NPCs would get one as part of M41-shipped equip + AI
//!   bootstrap.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use std::collections::HashMap;

/// Stand-in for Papyrus's `Actor.GetActorValue`-readable state.
///
/// Each actor entity carries one. The R5-prototype scope is
/// READ-ONLY for the script translation surface — scripts poll
/// stats, the engine (or test harness) writes them. A future
/// scripting feature that needs Papyrus's `ModActorValue` will
/// add a write-path through this same component.
#[derive(Debug, Clone, Default)]
pub struct ActorStats {
    /// Lower-cased stat name → numeric value. Lower-cased because
    /// Papyrus is case-insensitive (`GetActorValue("variable05")`
    /// and `GetActorValue("Variable05")` resolve to the same
    /// slot). Tests insert via [`Self::set`], scripts read via
    /// [`Self::get`].
    values: HashMap<String, f32>,
}

impl ActorStats {
    /// Papyrus `Actor.GetActorValue(name)`. Returns `0.0` for an
    /// unknown stat — matches Papyrus's "unknown actor value
    /// resolves to default 0" contract.
    pub fn get(&self, name: &str) -> f32 {
        self.values
            .get(&name.to_ascii_lowercase())
            .copied()
            .unwrap_or(0.0)
    }

    /// Papyrus `Actor.SetActorValue(name, value)`. Replaces any
    /// existing value (no modifier composition — that's M47.1).
    pub fn set(&mut self, name: &str, value: f32) {
        self.values.insert(name.to_ascii_lowercase(), value);
    }
}

impl Component for ActorStats {
    type Storage = SparseSetStorage<Self>;
}

pub fn register(world: &mut byroredux_core::ecs::world::World) {
    world.register::<ActorStats>();
}
