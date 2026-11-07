//! Skyrim's armor-rating model (M43, #6567): what a point of rating blocks, the hidden rating a worn
//! piece carries, the cap, and how the wearer's armor skill scales a piece.
//!
//! The numbers are the defaults of seven game settings, so a load order that retunes them (an
//! armor-rebalance mod, a Creation Club armor change) retunes this model: [`ArmorModel::with_gmst`] overlays
//! the load-order-merged values, exactly as [`CrimeModel`](super::CrimeModel) and
//! [`VatsModel`](super::VatsModel) do. The per-game seam is the [`CharacterRulesProfile`](super::CharacterRulesProfile)
//! `armor` row (`Some` on Skyrim only); the consumer in `byroredux/src/armor.rs` reads the installed
//! [`ArmorModel`] resource and never tests the game.
//!
//! Sources, CK *Settings* (`ck-uesp-wiki/Category/Settings.wiki`) and UESP *Skyrim:Armor*:
//!
//! | Setting | Default | Meaning |
//! |---|---|---|
//! | `fArmorScalingFactor` | 0.12 | "Each point of armor rating blocks this % of damage." |
//! | `fArmorBaseFactor` | 0.03 | The hidden armor of a worn piece, a fraction: 0.03 is a 3 % reduction. |
//! | `fMaxArmorRating` | 80 | The cap on the reduction, in percent. |
//! | `fArmorRatingPCBase` / `fArmorRatingPCMax` | 1.0 / 1.4 | The player's piece rating is multiplied by the first at 0 skill and the second at 100. |
//! | `fArmorRatingBase` / `fArmorRatingMax` | 1.0 / 2.5 | The same for an NPC. |
//!
//! Six of the seven are authored `GMST` records (master-authored, byte-scanned in Skyrim.esm, SE and LE, and the
//! DLC and Creation Club masters). `fArmorRatingPCBase` is not: no master authors it, and its 1.0 comes only from
//! the CK *Settings* page. The overlay still reads it so a mod that adds the record retunes the model.
//!
//! The multiplier between 0 and 100 skill is linear: `base + (max - base) × skill / 100`, which with the
//! defaults is UESP's `1 + 0.4 × skill / 100` (player) and `1 + 1.5 × skill / 100` (NPC).

use crate::ecs::resource::Resource;

/// The armor model's seven settings. All reduction values are percentages, all multipliers are plain factors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArmorModel {
    /// `fArmorScalingFactor`: percent of damage blocked per point of displayed rating.
    pub reduction_per_rating: f32,
    /// `fArmorBaseFactor × 100`: the percent a worn (non-clothing) piece blocks by being worn.
    pub hidden_percent_per_piece: f32,
    /// `fMaxArmorRating`: the cap on the total reduction, in percent.
    pub max_reduction: f32,
    /// `fArmorRatingPCBase`: the player's piece-rating multiplier at 0 armor skill.
    pub player_rating_base: f32,
    /// `fArmorRatingPCMax`: the player's multiplier at 100 armor skill.
    pub player_rating_max: f32,
    /// `fArmorRatingBase`: an NPC's multiplier at 0 armor skill.
    pub npc_rating_base: f32,
    /// `fArmorRatingMax`: an NPC's multiplier at 100 armor skill.
    pub npc_rating_max: f32,
}

impl Resource for ArmorModel {}

impl ArmorModel {
    /// Skyrim's defaults: six master-authored (byte-scanned from `Skyrim.esm`) and `fArmorRatingPCBase`, a CK-page
    /// default no master authors. The two skill multipliers are built from CHARAL's
    /// coefficients (`skyrim::LIGHT_ARMOR_RATING_COEFF` / `ARMOR_SKILL_RATING_COEFF_NPC`, #6568) so the Light
    /// Armor `DamageResist` row and this model cannot disagree about the player's.
    pub const SKYRIM: Self = Self {
        reduction_per_rating: 0.12,
        hidden_percent_per_piece: 3.0,
        max_reduction: 80.0,
        player_rating_base: 1.0,
        player_rating_max: 1.0 + super::skyrim::LIGHT_ARMOR_RATING_COEFF * 100.0,
        npc_rating_base: 1.0,
        npc_rating_max: 1.0 + super::skyrim::ARMOR_SKILL_RATING_COEFF_NPC * 100.0,
    };

    /// The model with each setting the load order authors replacing its default. A non-finite or negative
    /// authored value is ignored; the cap is held to 100 % so a retune cannot make armor heal.
    #[must_use]
    pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        let authored = |name: &str, default: f32| {
            gmst(name).filter(|value| value.is_finite() && *value >= 0.0).unwrap_or(default)
        };
        Self {
            reduction_per_rating: authored("fArmorScalingFactor", self.reduction_per_rating),
            hidden_percent_per_piece: authored("fArmorBaseFactor", self.hidden_percent_per_piece / 100.0) * 100.0,
            max_reduction: authored("fMaxArmorRating", self.max_reduction).min(100.0),
            player_rating_base: authored("fArmorRatingPCBase", self.player_rating_base),
            player_rating_max: authored("fArmorRatingPCMax", self.player_rating_max),
            npc_rating_base: authored("fArmorRatingBase", self.npc_rating_base),
            npc_rating_max: authored("fArmorRatingMax", self.npc_rating_max),
        }
    }

    /// The factor a piece's base rating is multiplied by at `skill` (0..=100) armor skill.
    #[must_use]
    pub fn skill_multiplier(self, player: bool, skill: f32) -> f32 {
        let (base, max) = if player {
            (self.player_rating_base, self.player_rating_max)
        } else {
            (self.npc_rating_base, self.npc_rating_max)
        };
        base + (max - base) * skill / 100.0
    }

    /// UESP's per-item formula: `CEILING[(base + quality bonus) × multiplier(skill)]`.
    #[must_use]
    pub fn item_rating(self, base: f32, quality_bonus: f32, skill: f32, player: bool) -> f32 {
        ((base + quality_bonus) * self.skill_multiplier(player, skill)).ceil()
    }

    /// The damage-reduction percentage for a displayed rating over `pieces` armor pieces worn.
    #[must_use]
    pub fn reduction_percent(self, displayed_rating: f32, pieces: usize) -> f32 {
        (displayed_rating * self.reduction_per_rating + self.hidden_percent_per_piece * pieces as f32)
            .clamp(0.0, self.max_reduction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaults are what `Skyrim.esm` authors (CK *Settings*), and the skill multipliers are the CHARAL
    /// coefficients' `1 + coeff × 100` (1.4 / 2.5).
    #[test]
    fn the_skyrim_defaults_are_the_authored_settings() {
        let model = ArmorModel::SKYRIM;
        assert_eq!(model.reduction_per_rating, 0.12);
        assert_eq!(model.hidden_percent_per_piece, 3.0);
        assert_eq!(model.max_reduction, 80.0);
        assert_eq!((model.player_rating_base, model.player_rating_max), (1.0, 1.4));
        assert_eq!((model.npc_rating_base, model.npc_rating_max), (1.0, 2.5));
    }

    /// With the defaults the multiplier is UESP's `1 + 0.4 × skill / 100` and `1 + 1.5 × skill / 100`.
    #[test]
    fn the_skill_multiplier_is_linear_between_the_base_and_the_max() {
        let model = ArmorModel::SKYRIM;
        assert_eq!(model.skill_multiplier(true, 0.0), 1.0);
        assert!((model.skill_multiplier(true, 100.0) - 1.4).abs() < 1e-6);
        assert!((model.skill_multiplier(false, 50.0) - 1.75).abs() < 1e-6);
        assert_eq!(model.item_rating(49.0, 0.0, 100.0, true), 69.0);
        assert_eq!(model.item_rating(49.0, 0.0, 50.0, false), 86.0);
        assert_eq!(model.item_rating(49.0, 20.0, 100.0, true), 97.0, "(49 + 20) × 1.4 = 96.6");
    }

    /// UESP's worked figures: 0.12 % per displayed point plus 3 % per piece, capped at 80 %; the cap is
    /// reached at 567 displayed rating with four pieces (567 × 0.12 + 12 = 80.04).
    #[test]
    fn the_reduction_follows_the_uesp_formula_and_cap() {
        let model = ArmorModel::SKYRIM;
        assert!((model.reduction_percent(100.0, 4) - 24.0).abs() < 1e-4);
        assert_eq!(model.reduction_percent(0.0, 0), 0.0);
        assert_eq!(model.reduction_percent(567.0, 4), 80.0);
        assert_eq!(model.reduction_percent(5000.0, 4), 80.0);
        assert!(model.reduction_percent(566.0, 4) < 80.0);
    }

    /// A load order that retunes the settings retunes the model; unauthored, invalid and negative values keep
    /// the default; a cap past 100 % is held to 100.
    #[test]
    fn with_gmst_overlays_the_load_orders_settings() {
        let retuned = ArmorModel::SKYRIM.with_gmst(|name| match name {
            "fArmorScalingFactor" => Some(0.2),
            "fArmorBaseFactor" => Some(0.05),
            "fMaxArmorRating" => Some(90.0),
            "fArmorRatingPCMax" => Some(1.5),
            "fArmorRatingMax" => Some(f32::NAN),
            "fArmorRatingBase" => Some(-1.0),
            _ => None,
        });
        assert_eq!(retuned.reduction_per_rating, 0.2);
        assert!((retuned.hidden_percent_per_piece - 5.0).abs() < 1e-4, "0.05 is a 5 % reduction");
        assert_eq!(retuned.max_reduction, 90.0);
        assert_eq!(retuned.player_rating_max, 1.5);
        assert_eq!(retuned.npc_rating_max, 2.5, "a non-finite value is ignored");
        assert_eq!(retuned.npc_rating_base, 1.0, "a negative value is ignored");
        assert_eq!(retuned.player_rating_base, 1.0, "unauthored keeps the default");

        let untouched = ArmorModel::SKYRIM.with_gmst(|_| None);
        assert!((untouched.hidden_percent_per_piece - 3.0).abs() < 1e-4);
        assert_eq!(ArmorModel { max_reduction: 80.0, ..untouched }, ArmorModel { max_reduction: 80.0, ..ArmorModel::SKYRIM }.with_gmst(|_| None));
        assert_eq!(ArmorModel::SKYRIM.with_gmst(|name| (name == "fMaxArmorRating").then_some(250.0)).max_reduction, 100.0);
    }
}
