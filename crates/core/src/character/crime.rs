//! The crime family (CHARAL): what a crime costs the criminal.
//!
//! Oblivion's and Skyrim's fines are sourced ([`CrimeModel::OBLIVION`] /
//! [`CrimeModel::SKYRIM`]), and theft, assault and murder are wired. Pickpocket
//! (25) and trespass (5) ride as associated consts for the faction-fine path;
//! the rest of the CS wiki's *Crime* table (jailbreak 50, horse theft 250) has
//! no consumer yet, so it is not carried here. FO3 / FNV crime is
//! faction-based (per-faction crime values) and a different model; a game
//! with no model here has no crime consequence, which is a profile row
//! saying `None`, never a branch in the consumer.
//!
//! The model is engine-supplied defaults that the load order's GMSTs
//! override by name, like [`super::BarterModel`].

use crate::ecs::resource::Resource;

/// The kinds of crime every Bethesda game numbers the same way (xEdit's
/// `wbCrimeTypeEnum`, shared by Oblivion, FO3, FNV, Skyrim and FO4 for these
/// five; the sixth value differs per game and has no consumer). A condition's
/// `GetCrime` / `GetCrimeKnown` crime-type argument is this number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum CrimeKind {
    Steal,
    Pickpocket,
    Trespass,
    Attack,
    Murder,
}

impl CrimeKind {
    /// The authored crime-type number, `None` for one this enum does not carry.
    pub fn from_wire(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Steal,
            1 => Self::Pickpocket,
            2 => Self::Trespass,
            3 => Self::Attack,
            4 => Self::Murder,
            _ => return None,
        })
    }
}

/// Where a game keeps the bounty a crime earns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BountyScope {
    /// One bounty on the player (Oblivion's `GetCrimeGold`), guards, jails and all.
    Player,
    /// A bounty per crime faction (Skyrim's holds): a witness reports to its own crime faction,
    /// and the faction's authored Crime tab decides what is tracked, ignored and fined.
    CrimeFaction,
}

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
    /// `iCrimeGoldTresspass` (sic, the setting's own spelling): the bounty for a trespass (CS
    /// wiki, default 5; Oblivion.esm does not author it).
    pub trespass_gold: i32,
    /// `iCrimeGoldPickpocket`: the bounty for a pickpocket (CS wiki, default 25; Oblivion.esm does
    /// not author it).
    pub pickpocket_gold: i32,
    /// UESP *Oblivion:Infamy*: one infamy point for "doing anything that
    /// increases your bounty by 1000. Can be done repeatedly." Counted per
    /// incident, so a murder is one point and a 700-gold theft none. A wiki
    /// observation, not a setting the masters author, so it is data on the
    /// model (a mod's rules profile can change it), not a constant.
    pub infamy_bounty_step: i32,
    /// `iCrimeDaysInPrisonMod`: "to get number of days, player bounty is
    /// divided by the value of this setting" (CS wiki, default 100; not
    /// authored by Oblivion.esm).
    pub prison_days_divisor: i32,
    /// `iCrimeAlarmRecDistance`: "the distance from which a crime victim's cries
    /// for help can be heard" (CS wiki, default 4000; Oblivion.esm authors 4000).
    pub alarm_distance: f32,
    /// `iNumberGuardsCrimeResponse`: "the number of guards that appear when the
    /// player commits a crime and it has been reported. If set to 0, no guard
    /// appears" (CS wiki, default 4; not authored).
    pub guards_responding: u32,
    /// `iCrimeGoldAttackMin`: "the minimum bounty level required for guards to rush a player on
    /// sight. This is not the level that causes guards to attack" (CS wiki). The wiki lists no
    /// default; the default here is Oblivion.esm's authored 500, which the Gray Cowl of Nocturnal's
    /// script uses as the bounty that makes every guard come ("wearing the cowl will prompt guards
    /// to immediately arrest you"). The higher figures the wiki quotes (arrest at about 1000,
    /// attack at about 4000) are not settings: they are conditions in the guards' authored dialogue.
    pub rush_bounty: i32,
    /// Where the bounty is kept (see [`BountyScope`]).
    pub scope: BountyScope,
    /// Whether the fine for theft rounds up (Oblivion: "bounties always round up when the value is
    /// a decimal") or down (Skyrim: "half of the stolen item's value, rounded down").
    pub theft_rounds_up: bool,
    /// Whether animals witness a crime (Skyrim: "most living creatures can witness crimes"; Oblivion's
    /// witnesses are NPCs).
    pub creatures_witness: bool,
}

impl Resource for CrimeModel {}

impl CrimeModel {
    /// Oblivion's defaults (CS wiki *fCrimeGoldSteal*, UESP *Crime*).
    pub const OBLIVION: Self = Self {
        steal_value_mult: 0.5,
        attack_gold: 40,
        murder_gold: 1000,
        trespass_gold: 5,
        pickpocket_gold: 25,
        infamy_bounty_step: 1000,
        prison_days_divisor: 100,
        alarm_distance: 4000.0,
        guards_responding: 4,
        rush_bounty: 500,
        scope: BountyScope::Player,
        theft_rounds_up: true,
        creatures_witness: false,
    };

    /// Skyrim's defaults: what a crime faction that ticks "Crime Gold - Use Defaults" fines (UESP
    /// *Skyrim:Crime*: theft half the item's value rounded down, trespass 5, pickpocket 25, assault
    /// 40, murder 1000). The bounty is per crime faction, and animals witness. The guards and jail
    /// settings below are Oblivion's and unused here: Skyrim's guards are an AI package.
    pub const SKYRIM: Self = Self {
        scope: BountyScope::CrimeFaction,
        theft_rounds_up: false,
        creatures_witness: true,
        ..Self::OBLIVION
    };

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
            trespass_gold: Self::authored_gold(&gmst, "iCrimeGoldTresspass", self.trespass_gold),
            pickpocket_gold: Self::authored_gold(&gmst, "iCrimeGoldPickpocket", self.pickpocket_gold),
            infamy_bounty_step: self.infamy_bounty_step,
            prison_days_divisor: Self::authored_gold(&gmst, "iCrimeDaysInPrisonMod", self.prison_days_divisor).max(1),
            alarm_distance: gmst("iCrimeAlarmRecDistance")
                .filter(|distance| distance.is_finite() && *distance >= 0.0)
                .unwrap_or(self.alarm_distance),
            // Zero is a real setting here (no guard appears), unlike the divisor.
            guards_responding: Self::authored_gold(&gmst, "iNumberGuardsCrimeResponse", self.guards_responding as i32) as u32,
            rush_bounty: Self::authored_gold(&gmst, "iCrimeGoldAttackMin", self.rush_bounty),
            scope: self.scope,
            theft_rounds_up: self.theft_rounds_up,
            creatures_witness: self.creatures_witness,
        }
    }

    fn authored_gold(gmst: &impl Fn(&str) -> Option<f32>, name: &str, default: i32) -> i32 {
        gmst(name).filter(|gold| gold.is_finite() && *gold >= 0.0).map_or(default, |gold| gold as i32)
    }

    /// The days a jail sentence for `bounty` lasts: `bounty / iCrimeDaysInPrisonMod`
    /// (CS wiki; UESP: "one day for every 100 gold").
    #[must_use]
    pub fn sentence_days(self, bounty: i32) -> f32 {
        bounty.max(0) as f32 / self.prison_days_divisor.max(1) as f32
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
        let raw = value as f64 * f64::from(self.steal_value_mult);
        let fine = if self.theft_rounds_up { raw.ceil() } else { raw.floor() };
        // `as` saturates, so a mod-supplied absurd value cannot wrap. Oblivion fines at least a
        // septim ("if the value is zero, it will always be at least one"); Skyrim's rounded-down
        // half of a 1-gold item is nothing.
        let minimum = i32::from(self.theft_rounds_up);
        (fine as i32).max(minimum)
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
    fn skyrim_rounds_a_theft_down_and_keeps_the_bounty_per_crime_faction() {
        let model = CrimeModel::SKYRIM;
        assert_eq!(model.theft_bounty(5), 2, "half of 5, rounded down");
        assert_eq!(model.theft_bounty(1), 0);
        assert_eq!(model.theft_bounty(100), 50);
        assert_eq!(model.scope, BountyScope::CrimeFaction);
        assert!(model.creatures_witness && !CrimeModel::OBLIVION.creatures_witness);
        assert_eq!((model.attack_gold, model.murder_gold), (40, 1000));
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
    fn a_sentence_is_a_day_per_hundred_gold_unless_the_setting_says_otherwise() {
        assert_eq!(CrimeModel::OBLIVION.sentence_days(250), 2.5);
        let slow = CrimeModel::OBLIVION.with_gmst(|name| (name == "iCrimeDaysInPrisonMod").then_some(50.0));
        assert_eq!(slow.sentence_days(250), 5.0);
        let zero = CrimeModel::OBLIVION.with_gmst(|name| (name == "iCrimeDaysInPrisonMod").then_some(0.0));
        assert_eq!(zero.sentence_days(100), 100.0, "a zero divisor reads as 1, never a division by zero");
    }

    #[test]
    fn crime_types_carry_the_numbers_conditions_author() {
        let kinds = [CrimeKind::Steal, CrimeKind::Pickpocket, CrimeKind::Trespass, CrimeKind::Attack, CrimeKind::Murder];
        for (wire, kind) in kinds.into_iter().enumerate() {
            assert_eq!(CrimeKind::from_wire(wire as u32), Some(kind));
        }
        assert_eq!(CrimeKind::from_wire(5), None, "the sixth value differs per game and is not carried");
    }

    #[test]
    fn the_rush_threshold_is_the_authored_setting() {
        assert_eq!(CrimeModel::OBLIVION.rush_bounty, 500);
        let model = CrimeModel::OBLIVION.with_gmst(|name| (name == "iCrimeGoldAttackMin").then_some(750.0));
        assert_eq!(model.rush_bounty, 750);
    }

    #[test]
    fn the_alarm_settings_are_data() {
        let model = CrimeModel::OBLIVION.with_gmst(|name| match name {
            "iCrimeAlarmRecDistance" => Some(1500.0),
            "iNumberGuardsCrimeResponse" => Some(0.0),
            _ => None,
        });
        assert_eq!((model.alarm_distance, model.guards_responding), (1500.0, 0), "0 guards is a real setting");
        assert_eq!(CrimeModel::OBLIVION.alarm_distance, 4000.0);
        assert_eq!(CrimeModel::OBLIVION.guards_responding, 4);
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
    fn the_trespass_and_pickpocket_fines_are_authored_settings() {
        assert_eq!((CrimeModel::OBLIVION.trespass_gold, CrimeModel::OBLIVION.pickpocket_gold), (5, 25));
        assert_eq!((CrimeModel::SKYRIM.trespass_gold, CrimeModel::SKYRIM.pickpocket_gold), (5, 25));
        let model = CrimeModel::OBLIVION.with_gmst(|name| match name {
            "iCrimeGoldTresspass" => Some(10.0),
            "iCrimeGoldPickpocket" => Some(f32::NAN),
            _ => None,
        });
        assert_eq!((model.trespass_gold, model.pickpocket_gold), (10, 25), "junk keeps the default");
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
