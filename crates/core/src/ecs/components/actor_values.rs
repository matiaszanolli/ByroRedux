//! Per-actor actor-value store — the production backing for `GetActorValue`.
//!
//! Attach [`ActorValues`] to any actor entity. It is the ECS surface the
//! M47.1 `GetActorValue` condition function reads from (#1663), and the
//! store perks / magic effects / `ModActorValue` will compose into.
//!
//! Distinct from the string-keyed [`crate`]-external `ActorStats` prototype
//! under `papyrus_demo`: that one keys by Papyrus source name for one demo
//! script and stores a flat value. This is the production store — keyed by
//! the source game's canonical actor-value identifier, layered (base /
//! permanent / temporary / damage), and shared by every gameplay reader.
//!
//! ## Key space
//!
//! Every actor value uses its **AVIF FormID in global load-order space**.
//! Skyrim authors the built-in values as AVIF records too (Health is
//! `0x000003E8` in vanilla `Skyrim.esm`), so #2987 withdrew the retired TES5
//! enum-index workaround and restored one key space for all games.
//! [`ActorVitals`] carries the resolved per-game Health FormID alongside each
//! actor so gameplay consumers do not need load-order-specific lookup rules.
//!
//! ## Composition
//!
//! `current = base + permanent + temporary − damage` (the Bethesda actor-value
//! layering — see the actor-value-system design). `base` is the race/class/
//! level result, `permanent` is perk/enchant offsets, `temporary` is active
//! effects, and `damage` is a separately-restored negative offset (e.g. limb
//! or health damage). An AV the actor doesn't carry composes to `0.0`,
//! matching Bethesda's "absent actor value → default 0" contract.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use std::collections::HashMap;

/// The four composition layers of a single actor value.
///
/// `current()` folds them per the actor-value model. All four default to
/// `0.0`, so a freshly-inserted entry reads `0.0` until a base is set.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct ActorValue {
    /// Race + class + level result (or formula / editor default).
    pub base: f32,
    /// Permanent offsets — perks, enchantments, equipped gear.
    pub permanent_mod: f32,
    /// Temporary offsets — active magic effects (potions, spells).
    pub temporary_mod: f32,
    /// Separately-restored negative offset (limb / health damage). Stored as a
    /// positive magnitude and subtracted by [`ActorValue::current`].
    pub damage: f32,
}

impl ActorValue {
    /// `base + permanent + temporary − damage`.
    pub fn current(&self) -> f32 {
        self.base + self.permanent_mod + self.temporary_mod - self.damage
    }
}

/// An actor's layered actor values, keyed by the game's canonical identifier.
///
/// Sparse storage — most entities are not actors. Map-backed because an actor
/// carries many values (SPECIAL + skills + resistances + resources + derived,
/// up to the full AVIF set) and reads are by id.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct ActorValues {
    values: HashMap<u32, ActorValue>,
}

/// Canonical keys for an actor's vital resource values.
///
/// ActorValues remains a game-agnostic map. This companion supplies the
/// resolved per-game Health AVIF FormID so combat never hard-codes or guesses
/// which entry is health.
///
/// **`health` is a FormID, not an HP value** — despite how it reads, this is
/// not where an actor's current/max health lives. It's stamped once at NPC
/// spawn from static per-game AVIF data and never mutated afterward; the
/// actual mutable health (base/damage/mods) lives in [`ActorValues`], looked
/// up using this FormID as the key. See #3027 (SAVE-D1-2026-08-16-02) — this
/// note exists because the name alone reads as live vital-signs state, which
/// cost an audit pass a false "is this an oversight?" investigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct ActorVitals {
    pub health: u32,
}

impl Component for ActorVitals {
    type Storage = SparseSetStorage<Self>;
}

impl ActorValues {
    /// An actor with no values set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from `(avif_form_id, base)` pairs — the common population shape
    /// (race/class/level produced base values). Later pairs for the same id
    /// overwrite the base.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (u32, f32)>) -> Self {
        let mut out = Self::default();
        for (avif, base) in pairs {
            out.set_base(avif, base);
        }
        out
    }

    /// Composed `current` value for the actor value identified by its
    /// global-space AVIF FormID. Returns `0.0` for an actor value this actor
    /// doesn't carry (Bethesda's absent-AV default).
    pub fn current(&self, avif_form_id: u32) -> f32 {
        self.values
            .get(&avif_form_id)
            .map_or(0.0, ActorValue::current)
    }

    /// The raw layered entry for an actor value, if present.
    pub fn get(&self, avif_form_id: u32) -> Option<&ActorValue> {
        self.values.get(&avif_form_id)
    }

    /// Set the base layer, leaving any permanent/temporary/damage untouched.
    pub fn set_base(&mut self, avif_form_id: u32, base: f32) {
        self.values.entry(avif_form_id).or_default().base = base;
    }

    /// Add to the permanent-modifier layer (perk / enchant / gear). Negative
    /// deltas remove a prior permanent bonus.
    pub fn mod_permanent(&mut self, avif_form_id: u32, delta: f32) {
        self.values.entry(avif_form_id).or_default().permanent_mod += delta;
    }

    /// Add to the temporary-modifier layer (active effects).
    pub fn mod_temporary(&mut self, avif_form_id: u32, delta: f32) {
        self.values.entry(avif_form_id).or_default().temporary_mod += delta;
    }

    /// Apply damage (accumulates; restored separately via [`Self::restore`]).
    pub fn apply_damage(&mut self, avif_form_id: u32, amount: f32) {
        self.values.entry(avif_form_id).or_default().damage += amount;
    }

    /// Restore up to `amount` of accumulated damage (never past the base —
    /// damage floors at `0.0`).
    pub fn restore(&mut self, avif_form_id: u32, amount: f32) {
        let e = self.values.entry(avif_form_id).or_default();
        e.damage = (e.damage - amount).max(0.0);
    }

    /// Number of distinct actor values carried.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// `true` when no actor values are set.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterate canonical AVIF FormIDs and their layered values.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &ActorValue)> {
        self.values.iter().map(|(&form_id, value)| (form_id, value))
    }
}

impl Component for ActorValues {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Stand-in AVIF FormIDs for the test (global space). Real ids come from
    // `EsmIndex::actor_values`; the component is agnostic to their values.
    const AV_HEALTH: u32 = 0x0000_02C9;
    const AV_SNEAK: u32 = 0x0000_02E1;

    #[test]
    fn current_folds_the_four_layers() {
        let mut av = ActorValues::new();
        av.set_base(AV_HEALTH, 100.0);
        av.mod_permanent(AV_HEALTH, 20.0); // +20 perk
        av.mod_temporary(AV_HEALTH, 10.0); // +10 potion
        av.apply_damage(AV_HEALTH, 35.0); // -35 damage
        assert_eq!(av.current(AV_HEALTH), 95.0, "100 + 20 + 10 − 35");
    }

    #[test]
    fn absent_value_is_zero() {
        let av = ActorValues::new();
        assert_eq!(av.current(AV_SNEAK), 0.0, "unset actor value → 0.0 default");
        assert!(av.is_empty());
        assert_eq!(av.get(AV_SNEAK), None);
    }

    #[test]
    fn from_pairs_seeds_bases() {
        let av = ActorValues::from_pairs([(AV_HEALTH, 150.0), (AV_SNEAK, 40.0)]);
        assert_eq!(av.current(AV_HEALTH), 150.0);
        assert_eq!(av.current(AV_SNEAK), 40.0);
        assert_eq!(av.len(), 2);
    }

    #[test]
    fn restore_floors_damage_at_zero() {
        let mut av = ActorValues::from_pairs([(AV_HEALTH, 100.0)]);
        av.apply_damage(AV_HEALTH, 30.0);
        assert_eq!(av.current(AV_HEALTH), 70.0);
        av.restore(AV_HEALTH, 50.0); // over-restore
        assert_eq!(
            av.current(AV_HEALTH),
            100.0,
            "damage floors at 0, not negative"
        );
        assert_eq!(av.get(AV_HEALTH).unwrap().damage, 0.0);
    }

    #[test]
    fn layers_are_independent_per_value() {
        let mut av = ActorValues::new();
        av.set_base(AV_HEALTH, 100.0);
        av.set_base(AV_SNEAK, 25.0);
        av.mod_permanent(AV_SNEAK, 15.0);
        assert_eq!(
            av.current(AV_HEALTH),
            100.0,
            "Sneak mod must not touch Health"
        );
        assert_eq!(av.current(AV_SNEAK), 40.0);
    }
}
