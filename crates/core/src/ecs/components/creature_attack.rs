//! A creature's authored natural attack — the `EquippedWeapon` analogue for
//! actors that fight with claws and teeth instead of items.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// The damage a creature deals per attack, straight from its `CREA`
/// record's `DATA.Damage` (#3762).
///
/// FO3/FNV author a creature's attack on the creature, not on a weapon —
/// there is no `WEAP` to equip and no `AVIF` this maps onto, so it is
/// neither an [`crate::ecs::components::ActorValues`] entry nor an
/// [`crate::ecs::components::EquippedWeapon`]. `CreatureStats::damage`
/// carried it as far as the parser; this component makes it available to
/// combat. `combat::attack_damage` reads it for a creature aggressor, and
/// there are two `HitEvent` producers: the player's melee in `combat.rs`
/// and `npc_combat_ai_system` (`byroredux/src/systems/combat_ai.rs`,
/// `f61ea044`), which strikes for `attack_damage`'s value. A creature
/// therefore hits for its authored damage once combat is armed on it — but
/// combat is armed only by a script's `Actor.StartCombat`; there is no
/// ambient hostility, so most of the 692 FNV / 186 FO3 creatures that author
/// a non-zero damage never attack. (#4105 recorded the earlier state, when
/// the player was the only producer.)
///
/// Deliberately **not** an actor value: inventing an `AVIF` FO3/FNV do not
/// publish would be a guess (the same reasoning that keeps `CreatureStats`'
/// three aggregate skills out of `ActorValues`). This is a dedicated
/// component for exactly the reason `EquippedWeapon` is one — it answers
/// "what does this actor's attack do", which is a combat question, not a
/// character-progression one.
///
/// Not saved: write-once at NPC spawn from static `CREA` data, so a reload
/// re-derives it deterministically (the `FactionRanks` class — see
/// `save_io`'s `REDERIVED_NOT_SAVED`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct CreatureAttack {
    /// Authored per-attack damage. Always finite and `> 0.0` — the spawn
    /// stamp drops non-positive values rather than materialising a
    /// creature that attacks for nothing, so a present component always
    /// means "this actor has an authored attack".
    pub damage: f32,
}

impl Component for CreatureAttack {
    type Storage = SparseSetStorage<Self>;
}
