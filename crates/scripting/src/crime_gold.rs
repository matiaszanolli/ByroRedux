//! M47.2 — the crime gold a faction holds: `Faction.SetCrimeGold(aiGold)` ("the
//! amount of non-violent crime gold on this faction"), `SetCrimeGoldViolent`,
//! `ModCrimeGold(aiAmount, abViolent = False)` ("Modifies the amount of crime gold
//! on this faction"), and the reads `GetCrimeGold` ("the amount of crime gold on
//! this faction that the player needs to pay": both kinds), `GetCrimeGoldViolent`
//! and `GetCrimeGoldNonViolent`.
//!
//! The engine has no crime system to raise or collect it, so scripts' writes are
//! the whole of it. The CK does not say whether a modification may take a count
//! below zero; the count here floors at zero. Keyed by the faction's FormID.
//! Saved.

use std::collections::BTreeMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;

/// A faction's (non-violent, violent) crime gold.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct FactionCrimeGold {
    by_faction: BTreeMap<u32, (i32, i32)>,
}

impl Resource for FactionCrimeGold {}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub.
pub fn install(world: &mut World) {
    world.init_resource::<FactionCrimeGold>();
}

/// #5802 — this subsystem's saved column. Called by the binary's
/// `build_save_registry` in its row order (the schema fingerprint is
/// order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<FactionCrimeGold>("FactionCrimeGold");
}

/// One script edit of a faction's crime gold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrimeGoldChange {
    Set { faction: u32, violent: bool, amount: i32 },
    Mod { faction: u32, violent: bool, amount: i32 },
}

impl FactionCrimeGold {
    pub fn apply(&mut self, change: CrimeGoldChange) {
        let (faction, violent, set, amount) = match change {
            CrimeGoldChange::Set { faction, violent, amount } => (faction, violent, true, amount),
            CrimeGoldChange::Mod { faction, violent, amount } => (faction, violent, false, amount),
        };
        let entry = self.by_faction.entry(faction).or_default();
        let slot = if violent { &mut entry.1 } else { &mut entry.0 };
        *slot = if set { amount } else { slot.saturating_add(amount) }.max(0);
        if *entry == (0, 0) {
            self.by_faction.remove(&faction);
        }
    }

    /// `(non-violent, violent)` crime gold on `faction`.
    pub fn of(&self, faction: u32) -> (i32, i32) {
        self.by_faction.get(&faction).copied().unwrap_or_default()
    }
}

/// `faction`'s crime gold with a run's `pending` edits applied on top.
pub fn crime_gold_with(world: &World, faction: u32, pending: &[CrimeGoldChange]) -> (i32, i32) {
    let mut gold = world.try_resource::<FactionCrimeGold>().map(|gold| gold.clone()).unwrap_or_default();
    for &change in pending {
        gold.apply(change);
    }
    gold.of(faction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_mod_and_floor() {
        let mut gold = FactionCrimeGold::default();
        gold.apply(CrimeGoldChange::Set { faction: 1, violent: false, amount: 40 });
        gold.apply(CrimeGoldChange::Mod { faction: 1, violent: true, amount: 25 });
        assert_eq!(gold.of(1), (40, 25));
        gold.apply(CrimeGoldChange::Mod { faction: 1, violent: false, amount: -100 });
        assert_eq!(gold.of(1), (0, 25), "floors at zero");
        gold.apply(CrimeGoldChange::Set { faction: 1, violent: true, amount: 0 });
        assert_eq!(gold.of(1), (0, 0));
        assert_eq!(gold.of(2), (0, 0));
    }
}
