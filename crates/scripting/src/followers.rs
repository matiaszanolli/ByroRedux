//! M47.2 — the player-follower flags scripts keep on actors:
//! `Actor.SetPlayerTeammate(abTeammate, abCanDoFavor)` / `IsPlayerTeammate()`
//! and `Actor.SetDoingFavor(abDoingFavor)` / `IsDoingFavor()`.
//!
//! The CK *SetPlayerTeammate - Actor* page lists what a teammate does
//! (sneaks with the player, draws with the player, wears its best armor,
//! its crimes count as the player's, can be commanded); those behaviors
//! belong to the systems that own sneaking, equipment and crime. This is the
//! state they read: keyed by the actor's FormID so it outlives the cell that
//! despawns the entity. Saved.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;

/// One actor's follower flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct FollowerFlags {
    /// `SetPlayerTeammate(abTeammate)`.
    pub teammate: bool,
    /// `SetPlayerTeammate(.., abCanDoFavor)`: "the actor's ability to do
    /// favors when they are the player's teammate".
    pub can_do_favor: bool,
    /// `SetDoingFavor`.
    pub doing_favor: bool,
}

/// Which follower flag a guard reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum FollowerFlag {
    Teammate,
    DoingFavor,
}

/// Every actor a script has flagged, by FormID.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct FollowerState {
    actors: HashMap<u32, FollowerFlags>,
}

impl Resource for FollowerState {}

impl FollowerState {
    pub fn flags(&self, actor: u32) -> FollowerFlags {
        self.actors.get(&actor).copied().unwrap_or_default()
    }

    pub fn read(&self, actor: u32, flag: FollowerFlag) -> bool {
        let flags = self.flags(actor);
        match flag {
            FollowerFlag::Teammate => flags.teammate,
            FollowerFlag::DoingFavor => flags.doing_favor,
        }
    }

    pub fn set_teammate(&mut self, actor: u32, teammate: bool, can_do_favor: bool) {
        let flags = self.actors.entry(actor).or_default();
        flags.teammate = teammate;
        flags.can_do_favor = can_do_favor;
    }

    pub fn set_doing_favor(&mut self, actor: u32, doing: bool) {
        self.actors.entry(actor).or_default().doing_favor = doing;
    }
}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub: `SetPlayerTeammate` / `SetDoingFavor` flags (saved).
pub fn install(world: &mut World) {
    world.init_resource::<FollowerState>();
}

/// #5802 — this subsystem's saved column. Called by the binary's
/// `build_save_registry` at its place in the row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<FollowerState>("FollowerState");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_are_independent_per_actor() {
        let mut state = FollowerState::default();
        state.set_teammate(1, true, false);
        state.set_doing_favor(1, true);
        assert!(state.read(1, FollowerFlag::Teammate));
        assert!(state.read(1, FollowerFlag::DoingFavor));
        assert!(!state.flags(1).can_do_favor);
        assert!(!state.read(2, FollowerFlag::Teammate));
        state.set_teammate(1, false, true);
        assert!(!state.read(1, FollowerFlag::Teammate));
        assert!(state.read(1, FollowerFlag::DoingFavor));
    }
}
