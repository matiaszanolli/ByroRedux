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
//! Qualities continue past Legendary up to the table's 44th (a Smithing level above 100, which only Fortify
//! Smithing gives; the level read is the actor value's current value, modifiers included). The weapon bonus acts
//! on the equipped damage; the *armor rating* bonus is stored, shown in the item's name and details, and
//! added to the displayed armor rating, but the engine has no armor mitigation for it to act on. Not modelled:
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

/// Qualities `1..=44`, in order (index `quality - 1`): UESP's full *Smithing Qualities* table. The first six carry
/// their own names; the rest are all displayed "Legendary" ("quality levels continue past Legendary, although the
/// displayed quality will not reflect that"), and are reached only with a Smithing level above 100 — Fortify
/// Smithing's effects.
pub(crate) const QUALITIES: [Quality; 44] = [
    Quality { name: "Fine", skill_without_perk: 14.0, skill_with_perk: 14.0, chest_bonus: 2.0, other_bonus: 1.0, value_mult: 1.167 },
    Quality { name: "Superior", skill_without_perk: 31.0, skill_with_perk: 22.0, chest_bonus: 6.0, other_bonus: 3.0, value_mult: 1.333 },
    Quality { name: "Exquisite", skill_without_perk: 65.0, skill_with_perk: 40.0, chest_bonus: 10.0, other_bonus: 5.0, value_mult: 1.5 },
    Quality { name: "Flawless", skill_without_perk: 100.0, skill_with_perk: 57.0, chest_bonus: 13.0, other_bonus: 7.0, value_mult: 1.667 },
    Quality { name: "Epic", skill_without_perk: 134.0, skill_with_perk: 74.0, chest_bonus: 17.0, other_bonus: 9.0, value_mult: 1.833 },
    Quality { name: "Legendary", skill_without_perk: 168.0, skill_with_perk: 91.0, chest_bonus: 20.0, other_bonus: 10.0, value_mult: 2.0 },
    Quality { name: "Legendary", skill_without_perk: 202.0, skill_with_perk: 108.0, chest_bonus: 24.0, other_bonus: 12.0, value_mult: 2.167 },
    Quality { name: "Legendary", skill_without_perk: 236.0, skill_with_perk: 125.0, chest_bonus: 28.0, other_bonus: 14.0, value_mult: 2.333 },
    Quality { name: "Legendary", skill_without_perk: 272.0, skill_with_perk: 143.0, chest_bonus: 31.0, other_bonus: 16.0, value_mult: 2.5 },
    Quality { name: "Legendary", skill_without_perk: 306.0, skill_with_perk: 160.0, chest_bonus: 35.0, other_bonus: 18.0, value_mult: 2.667 },
    Quality { name: "Legendary", skill_without_perk: 340.0, skill_with_perk: 177.0, chest_bonus: 38.0, other_bonus: 19.0, value_mult: 2.833 },
    Quality { name: "Legendary", skill_without_perk: 374.0, skill_with_perk: 194.0, chest_bonus: 42.0, other_bonus: 21.0, value_mult: 3.0 },
    Quality { name: "Legendary", skill_without_perk: 408.0, skill_with_perk: 211.0, chest_bonus: 46.0, other_bonus: 23.0, value_mult: 3.167 },
    Quality { name: "Legendary", skill_without_perk: 442.0, skill_with_perk: 228.0, chest_bonus: 49.0, other_bonus: 25.0, value_mult: 3.333 },
    Quality { name: "Legendary", skill_without_perk: 478.0, skill_with_perk: 246.0, chest_bonus: 53.0, other_bonus: 27.0, value_mult: 3.5 },
    Quality { name: "Legendary", skill_without_perk: 512.0, skill_with_perk: 263.0, chest_bonus: 56.0, other_bonus: 28.0, value_mult: 3.667 },
    Quality { name: "Legendary", skill_without_perk: 546.0, skill_with_perk: 280.0, chest_bonus: 60.0, other_bonus: 30.0, value_mult: 3.833 },
    Quality { name: "Legendary", skill_without_perk: 580.0, skill_with_perk: 297.0, chest_bonus: 64.0, other_bonus: 32.0, value_mult: 4.0 },
    Quality { name: "Legendary", skill_without_perk: 614.0, skill_with_perk: 314.0, chest_bonus: 67.0, other_bonus: 34.0, value_mult: 4.167 },
    Quality { name: "Legendary", skill_without_perk: 648.0, skill_with_perk: 331.0, chest_bonus: 71.0, other_bonus: 36.0, value_mult: 4.333 },
    Quality { name: "Legendary", skill_without_perk: 684.0, skill_with_perk: 349.0, chest_bonus: 75.0, other_bonus: 38.0, value_mult: 4.5 },
    Quality { name: "Legendary", skill_without_perk: 718.0, skill_with_perk: 366.0, chest_bonus: 78.0, other_bonus: 39.0, value_mult: 4.667 },
    Quality { name: "Legendary", skill_without_perk: 752.0, skill_with_perk: 383.0, chest_bonus: 82.0, other_bonus: 41.0, value_mult: 4.833 },
    Quality { name: "Legendary", skill_without_perk: 786.0, skill_with_perk: 400.0, chest_bonus: 85.0, other_bonus: 43.0, value_mult: 5.0 },
    Quality { name: "Legendary", skill_without_perk: 820.0, skill_with_perk: 417.0, chest_bonus: 89.0, other_bonus: 45.0, value_mult: 5.167 },
    Quality { name: "Legendary", skill_without_perk: 854.0, skill_with_perk: 434.0, chest_bonus: 93.0, other_bonus: 47.0, value_mult: 5.333 },
    Quality { name: "Legendary", skill_without_perk: 890.0, skill_with_perk: 452.0, chest_bonus: 96.0, other_bonus: 48.0, value_mult: 5.5 },
    Quality { name: "Legendary", skill_without_perk: 924.0, skill_with_perk: 469.0, chest_bonus: 100.0, other_bonus: 50.0, value_mult: 5.667 },
    Quality { name: "Legendary", skill_without_perk: 958.0, skill_with_perk: 486.0, chest_bonus: 103.0, other_bonus: 52.0, value_mult: 5.833 },
    Quality { name: "Legendary", skill_without_perk: 992.0, skill_with_perk: 503.0, chest_bonus: 107.0, other_bonus: 54.0, value_mult: 6.0 },
    Quality { name: "Legendary", skill_without_perk: 1026.0, skill_with_perk: 520.0, chest_bonus: 111.0, other_bonus: 56.0, value_mult: 6.167 },
    Quality { name: "Legendary", skill_without_perk: 1060.0, skill_with_perk: 537.0, chest_bonus: 114.0, other_bonus: 57.0, value_mult: 6.333 },
    Quality { name: "Legendary", skill_without_perk: 1096.0, skill_with_perk: 555.0, chest_bonus: 118.0, other_bonus: 59.0, value_mult: 6.5 },
    Quality { name: "Legendary", skill_without_perk: 1130.0, skill_with_perk: 572.0, chest_bonus: 121.0, other_bonus: 61.0, value_mult: 6.667 },
    Quality { name: "Legendary", skill_without_perk: 1164.0, skill_with_perk: 589.0, chest_bonus: 125.0, other_bonus: 63.0, value_mult: 6.833 },
    Quality { name: "Legendary", skill_without_perk: 1198.0, skill_with_perk: 606.0, chest_bonus: 128.0, other_bonus: 64.0, value_mult: 7.0 },
    Quality { name: "Legendary", skill_without_perk: 1232.0, skill_with_perk: 623.0, chest_bonus: 132.0, other_bonus: 66.0, value_mult: 7.167 },
    Quality { name: "Legendary", skill_without_perk: 1266.0, skill_with_perk: 640.0, chest_bonus: 136.0, other_bonus: 68.0, value_mult: 7.333 },
    Quality { name: "Legendary", skill_without_perk: 1302.0, skill_with_perk: 658.0, chest_bonus: 139.0, other_bonus: 70.0, value_mult: 7.5 },
    Quality { name: "Legendary", skill_without_perk: 1336.0, skill_with_perk: 675.0, chest_bonus: 143.0, other_bonus: 72.0, value_mult: 7.667 },
    Quality { name: "Legendary", skill_without_perk: 1370.0, skill_with_perk: 692.0, chest_bonus: 146.0, other_bonus: 73.0, value_mult: 7.833 },
    Quality { name: "Legendary", skill_without_perk: 1404.0, skill_with_perk: 709.0, chest_bonus: 150.0, other_bonus: 75.0, value_mult: 8.0 },
    Quality { name: "Legendary", skill_without_perk: 1439.0, skill_with_perk: 726.0, chest_bonus: 154.0, other_bonus: 77.0, value_mult: 8.167 },
    Quality { name: "Legendary", skill_without_perk: 1473.0, skill_with_perk: 743.0, chest_bonus: 157.0, other_bonus: 79.0, value_mult: 8.333 },
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

/// The armor-rating bonus of quality `quality`: the chest column for a cuirass, the other column for the rest.
pub(crate) fn armor_bonus(quality: u8, chest: bool) -> f32 {
    row(quality).map_or(0.0, |quality| if chest { quality.chest_bonus } else { quality.other_bonus })
}

/// The inventory line for an item at `quality`: its quality name and the rating bonus it adds — damage for a
/// weapon, armor rating for armor (the cuirass column for the biped body slot, bit 2 of the Skyrim `BOD2`
/// flags per UESP's slot list, the other column for the rest).
pub(crate) fn describe(quality: u8, definition: Option<&crate::inventory::InventoryItemDefinition>) -> String {
    use crate::inventory::EquipTarget;
    let name = quality_name(quality);
    match definition {
        Some(definition) if definition.weapon_damage.is_some() => format!("Improved: {name} (+{} damage)", weapon_bonus(quality)),
        Some(definition) => match definition.equip_target {
            Some(EquipTarget::BipedSlots(mask)) => {
                format!("Improved: {name} (+{} armor rating)", armor_bonus(quality, mask & 0x4 != 0))
            }
            _ => format!("Improved: {name}"),
        },
        None => format!("Improved: {name}"),
    }
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
        assert_eq!(best_quality(200.0, false), 6, "202 is the next level");
        assert_eq!(best_quality(202.0, false), 7, "past Legendary: UESP's level 7");
        assert_eq!(best_quality(1473.0, false), 44, "the table's last level");
        assert_eq!(best_quality(99999.0, true), 44);
    }

    #[test]
    fn the_rating_and_value_columns_follow_the_table() {
        assert_eq!((weapon_bonus(0), weapon_bonus(1), weapon_bonus(3), weapon_bonus(6)), (0.0, 1.0, 5.0, 10.0));
        assert_eq!((weapon_bonus(7), weapon_bonus(44), armor_bonus(7, true), armor_bonus(3, false)), (12.0, 79.0, 24.0, 5.0));
        assert!(QUALITIES.windows(2).all(|pair| pair[1].skill_without_perk > pair[0].skill_without_perk && pair[1].skill_with_perk > pair[0].skill_with_perk), "monotone thresholds");
        assert_eq!((value_mult(0), value_mult(2), value_mult(6)), (1.0, 1.333, 2.0));
        assert_eq!((quality_name(0), quality_name(2), quality_name(6), quality_name(44), quality_name(45)), ("", "Superior", "Legendary", "Legendary", ""));
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
