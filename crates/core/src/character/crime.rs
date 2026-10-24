//! The crime family (CHARAL): what a crime costs the criminal.
//!
//! Only Oblivion's fines are sourced so far, and theft, assault and murder
//! are wired: the other fines in the CS wiki's *Crime* table (pickpocket 25,
//! trespass 5, jailbreak 50, horse theft 250) have no consumer yet, so they
//! are not carried here. FO3 / FNV crime is
//! faction-based (per-faction crime values) and a different model; a game
//! with no model here has no crime consequence, which is a profile row
//! saying `None`, never a branch in the consumer.
//!
//! The model is engine-supplied defaults that the load order's GMSTs
//! override by name, like [`super::BarterModel`].

use crate::ecs::resource::Resource;

/// A game's crime fines (the defaults of the settings the masters may
/// author).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrimeModel {
    /// `fCrimeGoldSteal`: "acts as a multiplier to the value of the stolen
    /// item in calculating the fine for theft. The (extremely simple)
    /// formula is Crime Gold = itemValue * fCrimeGoldSteal" (CS wiki,
    /// default 0.5; Oblivion.esm does not author it).
    pub steal_value_mult: f32,
    /// `iCrimeGoldAttack`: the bounty for an assault (CS wiki, default 40;
    /// Oblivion.esm does not author it, only `iCrimeGoldAttackMin`).
    pub attack_gold: i32,
    /// `iCrimeGoldMurder`: the bounty for a murder (CS wiki, default 1000;
    /// Oblivion.esm does not author it).
    pub murder_gold: i32,
    /// UESP *Oblivion:Infamy*: one infamy point for "doing anything that
    /// increases your bounty by 1000. Can be done repeatedly." Counted per
    /// incident, so a murder is one point and a 700-gold theft none. A wiki
    /// observation, not a setting the masters author, so it is data on the
    /// model (a mod's rules profile can change it), not a constant.
    pub infamy_bounty_step: i32,
}

impl Resource for CrimeModel {}

impl CrimeModel {
    /// Oblivion's defaults (CS wiki *fCrimeGoldSteal*, UESP *Crime*).
    pub const OBLIVION: Self = Self { steal_value_mult: 0.5, attack_gold: 40, murder_gold: 1000, infamy_bounty_step: 1000 };

    /// The model with each setting the load order authors replacing its
    /// default. A non-finite or negative authored value is ignored.
    #[must_use]
    pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        Self {
            steal_value_mult: gmst("fCrimeGoldSteal")
                .filter(|mult| mult.is_finite() && *mult >= 0.0)
                .unwrap_or(self.steal_value_mult),
            attack_gold: Self::authored_gold(&gmst, "iCrimeGoldAttack", self.attack_gold),
            murder_gold: Self::authored_gold(&gmst, "iCrimeGoldMurder", self.murder_gold),
            infamy_bounty_step: self.infamy_bounty_step,
        }
    }

    fn authored_gold(gmst: &impl Fn(&str) -> Option<f32>, name: &str, default: i32) -> i32 {
        gmst(name).filter(|gold| gold.is_finite() && *gold >= 0.0).map_or(default, |gold| gold as i32)
    }

    /// The infamy a crime that adds `bounty` earns (see [`Self::infamy_bounty_step`]).
    #[must_use]
    pub fn infamy_for(self, bounty: i32) -> f32 {
        (bounty.max(0) / self.infamy_bounty_step.max(1)) as f32
    }

    /// The bounty a witnessed theft of goods worth `value` adds. UESP
    /// *Oblivion:Crime*: "bounties always round up when the value is a
    /// decimal, and if the value is zero, it will always be at least one
    /// septim".
    #[must_use]
    pub fn theft_bounty(self, value: u64) -> i32 {
        let fine = (value as f64 * f64::from(self.steal_value_mult)).ceil();
        // `as` saturates, so a mod-supplied absurd value cannot wrap.
        (fine as i32).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theft_fines_half_the_value_rounded_up_and_never_less_than_one() {
        let model = CrimeModel::OBLIVION;
        assert_eq!(model.theft_bounty(100), 50);
        assert_eq!(model.theft_bounty(5), 3, "2.5 rounds up");
        assert_eq!(model.theft_bounty(1), 1, "0.5 rounds up");
        assert_eq!(model.theft_bounty(0), 1, "a worthless item still costs a septim");
    }

    #[test]
    fn an_absurd_value_saturates_instead_of_wrapping() {
        assert_eq!(CrimeModel::OBLIVION.theft_bounty(u64::MAX), i32::MAX);
    }

    #[test]
    fn infamy_is_one_point_per_thousand_gold_of_a_single_crime() {
        let model = CrimeModel::OBLIVION;
        assert_eq!(model.infamy_for(model.murder_gold), 1.0);
        assert_eq!(model.infamy_for(model.attack_gold), 0.0);
        assert_eq!(model.infamy_for(2500), 2.0);
        assert_eq!(model.infamy_for(-5), 0.0);
    }

    #[test]
    fn authored_fines_replace_the_defaults() {
        let model = CrimeModel::OBLIVION.with_gmst(|name| match name {
            "iCrimeGoldAttack" => Some(60.0),
            "iCrimeGoldMurder" => Some(f32::NAN),
            _ => None,
        });
        assert_eq!((model.attack_gold, model.murder_gold), (60, 1000), "junk keeps the default");
    }

    #[test]
    fn an_authored_multiplier_replaces_the_default_and_junk_does_not() {
        let authored = CrimeModel::OBLIVION.with_gmst(|name| (name == "fCrimeGoldSteal").then_some(1.0));
        assert_eq!(authored.theft_bounty(10), 10);
        for junk in [f32::NAN, f32::INFINITY, -1.0] {
            let model = CrimeModel::OBLIVION.with_gmst(|_| Some(junk));
            assert_eq!(model, CrimeModel::OBLIVION, "{junk} keeps the default");
        }
        assert_eq!(CrimeModel::OBLIVION.with_gmst(|_| None), CrimeModel::OBLIVION);
    }
}
