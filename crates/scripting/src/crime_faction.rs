//! M47.2 — the faction an actor reports its crimes to: `Actor.GetCrimeFaction()`
//! ("the faction this actor reports it's crimes to") and `SetCrimeFaction(akFaction)`
//! ("Sets the faction this actor reports crimes to. You may pass None to clear out
//! the crime faction on the actor. If the actor does not currently belong to the
//! crime faction, it will be added to that faction").
//!
//! An actor's authored crime faction is its base `NPC_`'s `CRIF`
//! ([`CrimeFactions`], derived from the load order); a script's `SetCrimeFaction`
//! overrides it per reference ([`CrimeFactionOverrides`], saved, a cleared faction
//! kept as `0`). No `CRIF` and no override is no crime faction: how the engine
//! would pick one from the actor's other factions is not documented.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_core::character::{CrimeKind, CrimeModel};
use byroredux_plugin::esm::records::{EsmIndex, FactionCrime};

/// Every `NPC_`'s authored crime faction, by base FormID. Derived, never saved.
#[derive(Debug, Default, Clone)]
pub struct CrimeFactions(Arc<HashMap<u32, u32>>);

impl Resource for CrimeFactions {}

/// Install the load order's authored crime factions; returns how many.
pub fn install_crime_factions(world: &mut World, index: &EsmIndex) -> usize {
    let factions: HashMap<u32, u32> = index
        .npcs
        .iter()
        .filter(|(_, npc)| npc.crime_faction_form_id != 0)
        .map(|(&form_id, npc)| (form_id, npc.crime_faction_form_id))
        .collect();
    let count = factions.len();
    world.insert_resource(CrimeFactions(Arc::new(factions)));
    count
}

/// The crime factions scripts set, by actor reference FormID; `0` is a cleared one.
/// Saved.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct CrimeFactionOverrides {
    set: BTreeMap<u32, u32>,
}

impl Resource for CrimeFactionOverrides {}

impl CrimeFactionOverrides {
    pub fn set(&mut self, actor: u32, faction: u32) {
        self.set.insert(actor, faction);
    }

    pub fn get(&self, actor: u32) -> Option<u32> {
        self.set.get(&actor).copied()
    }
}

/// Every faction's authored Crime tab (Skyrim, FO4), by faction FormID. Derived, never saved.
#[derive(Debug, Default, Clone)]
pub struct CrimeFactionData(Arc<HashMap<u32, FactionCrime>>);

impl Resource for CrimeFactionData {}

impl CrimeFactionData {
    /// A table of `data` (faction FormID to its Crime tab), for a caller that stands in for a load order.
    pub fn from_map(data: HashMap<u32, FactionCrime>) -> Self {
        Self(Arc::new(data))
    }

    pub fn get(&self, faction: u32) -> Option<&FactionCrime> {
        self.0.get(&faction)
    }
}

/// Install the load order's faction Crime tabs; returns how many factions have one.
pub fn install_crime_faction_data(world: &mut World, index: &EsmIndex) -> usize {
    let data: HashMap<u32, FactionCrime> = index
        .factions
        .iter()
        .filter_map(|(&form_id, faction)| faction.crime.clone().map(|crime| (form_id, crime)))
        .collect();
    let count = data.len();
    world.insert_resource(CrimeFactionData(Arc::new(data)));
    count
}

/// What a crime fines on the crime faction `crime` (Skyrim's Crime tab), and whether it is a
/// violent one (the bounty is kept in two parts, `GetCrimeGoldViolent` / `GetCrimeGoldNonviolent`):
/// `None` when the faction does not track crime or ignores this kind.
///
/// CK *Faction*: "Track Crime: if a faction is flagged to track crime, its members will remember
/// crimes against it", the `Ignore Crimes: ...` flags drop a kind, and "Crime Gold: allows you to
/// specify bounty amounts allocated for specific crimes; Use Defaults: use the default values" —
/// [`CrimeModel`]'s. A theft is fined by the item value times the faction's steal multiplier (the
/// default being the model's). `theft_value` is the gold value of what was taken.
pub fn faction_fine(crime: &FactionCrime, model: &CrimeModel, kind: CrimeKind, theft_value: u64) -> Option<(bool, i32)> {
    use byroredux_plugin::esm::records::crime_flags::*;
    if crime.flags & TRACK_CRIME == 0 {
        return None;
    }
    let ignored = match kind {
        CrimeKind::Steal => IGNORE_STEALING,
        CrimeKind::Pickpocket => IGNORE_PICKPOCKET,
        CrimeKind::Trespass => IGNORE_TRESPASS,
        CrimeKind::Attack => IGNORE_ASSAULT,
        CrimeKind::Murder => IGNORE_MURDER,
    };
    if crime.flags & ignored != 0 {
        return None;
    }
    let authored = if crime.flags & USE_DEFAULT_CRIME_GOLD != 0 { None } else { crime.values.as_ref() };
    let amount = match (kind, authored) {
        (CrimeKind::Steal, None) => model.theft_bounty(theft_value),
        (CrimeKind::Steal, Some(values)) => CrimeModel { steal_value_mult: values.steal_multiplier, ..*model }.theft_bounty(theft_value),
        (CrimeKind::Pickpocket, None) => CrimeModel::PICKPOCKET_GOLD,
        (CrimeKind::Pickpocket, Some(values)) => i32::from(values.pickpocket),
        (CrimeKind::Trespass, None) => CrimeModel::TRESPASS_GOLD,
        (CrimeKind::Trespass, Some(values)) => i32::from(values.trespass),
        (CrimeKind::Attack, None) => model.attack_gold,
        (CrimeKind::Attack, Some(values)) => i32::from(values.assault),
        (CrimeKind::Murder, None) => model.murder_gold,
        (CrimeKind::Murder, Some(values)) => i32::from(values.murder),
    };
    Some((matches!(kind, CrimeKind::Attack | CrimeKind::Murder), amount))
}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub. The authored [`CrimeFactions`] arrive with the load order
/// ([`install_crime_factions`]).
pub fn install(world: &mut World) {
    world.init_resource::<CrimeFactionOverrides>();
}

/// #5802 — this subsystem's saved column, `Actor.SetCrimeFaction`. Called by
/// the binary's `build_save_registry` in its row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<CrimeFactionOverrides>("CrimeFactionOverrides");
}

/// The faction `actor` (reference FormID `reference`) reports crimes to.
pub fn crime_faction(world: &World, entity: EntityId, reference: u32) -> Option<u32> {
    if let Some(faction) = world
        .try_resource::<CrimeFactionOverrides>()
        .and_then(|overrides| overrides.get(reference))
    {
        return (faction != 0).then_some(faction);
    }
    let base = crate::relationships::actor_base(world, entity)?;
    world.try_resource::<CrimeFactions>()?.0.get(&base).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::NpcRecord;

    #[test]
    fn only_npcs_with_a_crif_have_an_authored_crime_faction() {
        let mut index = EsmIndex::default();
        index.npcs.insert(0x10, NpcRecord { form_id: 0x10, crime_faction_form_id: 0x99, ..Default::default() });
        index.npcs.insert(0x11, NpcRecord { form_id: 0x11, ..Default::default() });
        let mut world = World::new();
        assert_eq!(install_crime_factions(&mut world, &index), 1);
        let factions = world.resource::<CrimeFactions>();
        assert_eq!(factions.0.get(&0x10), Some(&0x99));
        assert_eq!(factions.0.get(&0x11), None);
    }

    fn tab(flags: u32, values: Option<byroredux_plugin::esm::records::CrimeValues>) -> FactionCrime {
        FactionCrime { flags, values, ..Default::default() }
    }

    #[test]
    fn a_faction_that_tracks_no_crime_or_ignores_a_kind_fines_nothing() {
        use byroredux_plugin::esm::records::crime_flags::*;
        let model = CrimeModel::SKYRIM;
        assert_eq!(faction_fine(&tab(0, None), &model, CrimeKind::Murder, 0), None, "not tracking");
        let ignoring = tab(TRACK_CRIME | USE_DEFAULT_CRIME_GOLD | IGNORE_STEALING, None);
        assert_eq!(faction_fine(&ignoring, &model, CrimeKind::Steal, 100), None);
        assert_eq!(faction_fine(&ignoring, &model, CrimeKind::Murder, 0), Some((true, 1000)), "other kinds still count");
    }

    #[test]
    fn the_defaults_are_the_models_and_violent_means_assault_or_murder() {
        use byroredux_plugin::esm::records::crime_flags::*;
        let model = CrimeModel::SKYRIM;
        let defaults = tab(TRACK_CRIME | USE_DEFAULT_CRIME_GOLD, None);
        let fine = |kind, value| faction_fine(&defaults, &model, kind, value);
        assert_eq!(fine(CrimeKind::Murder, 0), Some((true, 1000)));
        assert_eq!(fine(CrimeKind::Attack, 0), Some((true, 40)));
        assert_eq!(fine(CrimeKind::Trespass, 0), Some((false, 5)));
        assert_eq!(fine(CrimeKind::Pickpocket, 0), Some((false, 25)));
        assert_eq!(fine(CrimeKind::Steal, 101), Some((false, 50)), "half the value, rounded down");
    }

    #[test]
    fn authored_crime_values_replace_the_defaults() {
        use byroredux_plugin::esm::records::{crime_flags::*, CrimeValues};
        let values = CrimeValues { murder: 500, assault: 7, trespass: 1, pickpocket: 2, steal_multiplier: 1.0, ..Default::default() };
        let custom = tab(TRACK_CRIME, Some(values));
        let model = CrimeModel::SKYRIM;
        assert_eq!(faction_fine(&custom, &model, CrimeKind::Murder, 0), Some((true, 500)));
        assert_eq!(faction_fine(&custom, &model, CrimeKind::Attack, 0), Some((true, 7)));
        assert_eq!(faction_fine(&custom, &model, CrimeKind::Steal, 80), Some((false, 80)), "the faction's own multiplier");
        // Companions-style: a zero for a kind means the faction does not fine it.
        let free = tab(TRACK_CRIME, Some(CrimeValues::default()));
        assert_eq!(faction_fine(&free, &model, CrimeKind::Murder, 0), Some((true, 0)));
    }

    #[test]
    fn a_script_override_wins_and_zero_clears() {
        let mut overrides = CrimeFactionOverrides::default();
        assert_eq!(overrides.get(7), None);
        overrides.set(7, 0x55);
        assert_eq!(overrides.get(7), Some(0x55));
        overrides.set(7, 0);
        assert_eq!(overrides.get(7), Some(0));
    }
}
