//! Scripted quest-alias overrides (M47.2): `ReferenceAlias.ForceRefTo`,
//! `LocationAlias.ForceLocationTo` and `Alias.Clear`.
//!
//! The CK's *ReferenceAlias Script*: `ForceRefTo` "forces this alias to use
//! the specified reference", `Clear` "clears this alias of its reference" —
//! both override whatever the alias's fill type would pick, until the quest
//! stops. The alias refresh recomputes bindings from the authored fills, so
//! an override is recorded here (by reference FormID, which outlives the
//! entity) and applied by the refresh in the alias's place in the fill order.
//! Saved; a stopped quest's overrides are dropped.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;

use crate::quest_stages::QuestFormId;

/// One alias override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum AliasOverride {
    /// `ForceRefTo` — the alias holds this reference (`0x14` for the player).
    Forced(u32),
    /// `Clear` — the alias holds nothing.
    Cleared,
    /// `LocationAlias.ForceLocationTo` — the location alias holds this
    /// location.
    ForcedLocation(u32),
}

/// Every scripted alias override of a running quest. Saved.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct AliasOverrides {
    by_quest: HashMap<QuestFormId, HashMap<i32, AliasOverride>>,
}

impl Resource for AliasOverrides {}

impl AliasOverrides {
    pub fn get(&self, quest: QuestFormId, alias_id: i32) -> Option<AliasOverride> {
        self.by_quest.get(&quest)?.get(&alias_id).copied()
    }

    pub fn set(&mut self, quest: QuestFormId, alias_id: i32, value: AliasOverride) {
        self.by_quest.entry(quest).or_default().insert(alias_id, value);
    }

    /// Drop the overrides of quests `running` rejects.
    pub fn retain_quests(&mut self, mut running: impl FnMut(QuestFormId) -> bool) {
        self.by_quest.retain(|quest, _| running(*quest));
    }
}

/// #6705 — this subsystem's saved column(s). Called by the binary's `build_save_registry` in its
/// row order (the schema fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    // M47.2 — scripted ForceRefTo / Clear alias overrides, by reference
    // FormID; dropped when their quest stops.
    registry.register_resource::<AliasOverrides>("AliasOverrides");
}

/// #6706 — this subsystem's world setup, owned here instead of the crate's `register` hub.
pub fn install(world: &mut byroredux_core::ecs::World) {
    // M47.2 — scripted ForceRefTo / Clear alias overrides (saved).
    world.init_resource::<AliasOverrides>();
}
