//! Improving items — Skyrim tempering at a grindstone or an armor workbench (M43).
//!
//! UESP *Skyrim:Smithing* ("Item Quality", "Improving Items"):
//!
//! * the quality levels are Fine, Superior, Exquisite, Flawless, Epic and Legendary, each unlocked at a Smithing
//!   level that depends on whether the player holds the Smithing perk of the item's material (the "without" and
//!   "with perk" columns of [`QUALITIES`]);
//! * a quality adds a rating bonus — armor rating for armor, damage for weapons — and multiplies the item's
//!   value (`1.167x` … `2.0x`);
//! * an improvement costs the recipe's material and earns `3.8 × Δvalue^0.5 × ΔQ^0.5` Smithing XP;
//! * an improved item can be improved again when the Smithing level allows a higher quality.
//!
//! The *material perk* is not a field of the item: the perks are entry-point perks on `Mod Tempering Health`
//! (`Multiply` by `2.0`) gated by a condition on the item's keyword, so "the player has the item's material
//! perk" is `tempering_multiplier > 1` ([`byroredux_scripting::tempering`]). The enchanted-item rule is the
//! recipes' own condition (`EPTemperingItemIsEnchanted != 1 OR HasPerk(ArcaneBlacksmith)`).
//!
//! Engine policy and not modelled: qualities above Legendary (the page's "continue past Legendary" needs
//! Fortify Smithing's effective skill, which this reads as plain skill), the *armor rating* bonus (the engine
//! has no armor rating to add it to — the quality is stored, and shown, and only the weapon bonus acts), and
//! the improve animation.

/// One row of the UESP *Smithing Qualities* table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Quality {
    pub name: &'static str,
    /// Smithing level required without the item's material perk.
    pub skill_without_perk: f32,
    /// Smithing level required with it.
    pub skill_with_perk: f32,
    /// Rating bonus on chest armor.
    pub chest_bonus: f32,
    /// Rating bonus on helmets, gauntlets, boots, shields and weapons.
    pub other_bonus: f32,
    /// Multiplier on the item's base value.
    pub value_mult: f32,
}

/// Quality `1..=6`, in order (index `quality - 1`).
pub(crate) const QUALITIES: [Quality; 6] = [
    Quality { name: "Fine", skill_without_perk: 14.0, skill_with_perk: 14.0, chest_bonus: 2.0, other_bonus: 1.0, value_mult: 1.167 },
    Quality { name: "Superior", skill_without_perk: 31.0, skill_with_perk: 22.0, chest_bonus: 6.0, other_bonus: 3.0, value_mult: 1.333 },
    Quality { name: "Exquisite", skill_without_perk: 65.0, skill_with_perk: 40.0, chest_bonus: 10.0, other_bonus: 5.0, value_mult: 1.5 },
    Quality { name: "Flawless", skill_without_perk: 100.0, skill_with_perk: 57.0, chest_bonus: 13.0, other_bonus: 7.0, value_mult: 1.667 },
    Quality { name: "Epic", skill_without_perk: 134.0, skill_with_perk: 74.0, chest_bonus: 17.0, other_bonus: 9.0, value_mult: 1.833 },
    Quality { name: "Legendary", skill_without_perk: 168.0, skill_with_perk: 91.0, chest_bonus: 20.0, other_bonus: 10.0, value_mult: 2.0 },
];

/// The best quality `skill` allows, `0` when it allows none.
pub(crate) fn best_quality(skill: f32, with_perk: bool) -> u8 {
    QUALITIES
        .iter()
        .take_while(|quality| skill >= if with_perk { quality.skill_with_perk } else { quality.skill_without_perk })
        .count() as u8
}

/// The display name of quality `quality`; empty for an unimproved item.
pub(crate) fn quality_name(quality: u8) -> &'static str {
    usize::from(quality)
        .checked_sub(1)
        .and_then(|index| QUALITIES.get(index))
        .map_or("", |quality| quality.name)
}

fn row(quality: u8) -> Option<&'static Quality> {
    usize::from(quality).checked_sub(1).and_then(|index| QUALITIES.get(index))
}

/// The weapon-damage bonus of quality `quality` (the "other" rating column).
pub(crate) fn weapon_bonus(quality: u8) -> f32 {
    row(quality).map_or(0.0, |quality| quality.other_bonus)
}

/// The value multiplier of quality `quality` (`1.0` unimproved).
pub(crate) fn value_mult(quality: u8) -> f32 {
    row(quality).map_or(1.0, |quality| quality.value_mult)
}

/// The Smithing XP of improving an item worth `base_value` from quality `from` to `to`: UESP's
/// `3.8 × Δvalue^0.5 × ΔQ^0.5`.
pub(crate) fn improvement_xp(base_value: u32, from: u8, to: u8) -> f32 {
    let delta_value = (base_value as f32 * (value_mult(to) - value_mult(from))).max(0.0);
    let delta_quality = f32::from(to.saturating_sub(from));
    3.8 * delta_value.sqrt() * delta_quality.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skill_unlocks_qualities_per_the_uesp_table() {
        assert_eq!(best_quality(13.0, false), 0);
        assert_eq!(best_quality(14.0, false), 1);
        assert_eq!(best_quality(45.0, false), 2, "UESP's example: 45 without the perk is Superior");
        assert_eq!(best_quality(45.0, true), 3, "…and Exquisite with it");
        assert_eq!(best_quality(100.0, false), 4);
        assert_eq!(best_quality(91.0, true), 6);
        assert_eq!(best_quality(200.0, false), 6, "past Legendary is not modelled");
    }

    #[test]
    fn the_rating_and_value_columns_follow_the_table() {
        assert_eq!((weapon_bonus(0), weapon_bonus(1), weapon_bonus(3), weapon_bonus(6)), (0.0, 1.0, 5.0, 10.0));
        assert_eq!((value_mult(0), value_mult(2), value_mult(6)), (1.0, 1.333, 2.0));
        assert_eq!((quality_name(0), quality_name(2), quality_name(6), quality_name(7)), ("", "Superior", "Legendary", ""));
    }

    /// UESP's worked example: iron armor (value 125) from Fine (145) to Exquisite (187) is a value difference of
    /// 42 and a quality change of 2, so about 34.8 XP.
    #[test]
    fn improvement_xp_reproduces_the_uesp_example() {
        let xp = improvement_xp(125, 1, 3);
        assert!((xp - 34.8).abs() < 0.6, "got {xp}");
        assert_eq!(improvement_xp(125, 3, 3), 0.0, "no change, no XP");
    }
}
