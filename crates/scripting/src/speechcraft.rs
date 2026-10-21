//! M47.2 — the speech outcomes a script flags on an actor: `Actor.SetBribed` /
//! `IsBribed` ("flags this actor as bribed by the player" / "has been bribed by
//! the player") and `SetIntimidated` / `IsIntimidated` (the same for
//! intimidation).
//!
//! The CK pages state no decay or other clearing rule, so a flag lasts until a
//! script clears it (`SetBribed(false)`). Keyed by the actor's reference FormID
//! like the other per-reference script state; saved.

use std::collections::BTreeSet;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;

/// Which outcome a script is flagging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechcraftOutcome {
    Bribed,
    Intimidated,
}

/// The actors flagged bribed or intimidated by the player.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct SpeechcraftOutcomes {
    bribed: BTreeSet<u32>,
    intimidated: BTreeSet<u32>,
}

impl Resource for SpeechcraftOutcomes {}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub: the scripted SetBribed / SetIntimidated flags (saved).
pub fn install(world: &mut World) {
    world.init_resource::<SpeechcraftOutcomes>();
}

/// #5802 — this subsystem's saved column. Called by the binary's
/// `build_save_registry` at its place in the row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<SpeechcraftOutcomes>("SpeechcraftOutcomes");
}

impl SpeechcraftOutcomes {
    fn set_of(&self, outcome: SpeechcraftOutcome) -> &BTreeSet<u32> {
        match outcome {
            SpeechcraftOutcome::Bribed => &self.bribed,
            SpeechcraftOutcome::Intimidated => &self.intimidated,
        }
    }

    pub fn is(&self, actor: u32, outcome: SpeechcraftOutcome) -> bool {
        self.set_of(outcome).contains(&actor)
    }

    pub fn set(&mut self, actor: u32, outcome: SpeechcraftOutcome, flagged: bool) {
        let set = match outcome {
            SpeechcraftOutcome::Bribed => &mut self.bribed,
            SpeechcraftOutcome::Intimidated => &mut self.intimidated,
        };
        if flagged {
            set.insert(actor);
        } else {
            set.remove(&actor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_outcomes_are_independent_and_clearable() {
        let mut outcomes = SpeechcraftOutcomes::default();
        outcomes.set(7, SpeechcraftOutcome::Bribed, true);
        assert!(outcomes.is(7, SpeechcraftOutcome::Bribed));
        assert!(!outcomes.is(7, SpeechcraftOutcome::Intimidated));
        outcomes.set(7, SpeechcraftOutcome::Intimidated, true);
        outcomes.set(7, SpeechcraftOutcome::Bribed, false);
        assert!(!outcomes.is(7, SpeechcraftOutcome::Bribed));
        assert!(outcomes.is(7, SpeechcraftOutcome::Intimidated));
    }
}
