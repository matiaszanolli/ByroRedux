//! Classic-TES skill-use experience: the cost of a rank, and banking.
//!
//! The arithmetic half of Oblivion's skill-use leveling (PRD
//! `docs/prd/oblivion-skill-use-leveling.md` §5.1, decoded from
//! `Oblivion.exe` in `docs/engine/skill-use.md` §1). Pure data and functions:
//! no ECS, no game branches. The host supplies the skill's value, its banked
//! experience and its [`SkillClass`], and gets back the ranks reached.
//!
//! The cost a skill at value `L` needs to rise by one (`0x548030`):
//!
//! ```text
//! (factor · L) ^ exp · (specialization ? spec_mult : 1) · (major ? major_mult : minor_mult)
//! ```
//!
//! The major multiplier *replaces* the minor one. The settings are the five
//! `fSkillUse*` GMSTs, overlaid by [`super::LevelingModel::with_gmst`].

/// The first Oblivion skill actor-value code (`Armorer`, 12) and the last
/// (`Speechcraft`, 32). The 21 skills run in `CLAS` order, seven per
/// specialization: Combat 12–18, Magic 19–25, Stealth 26–32.
pub const OBLIVION_SKILL_AV_FIRST: u32 = 12;
/// See [`OBLIVION_SKILL_AV_FIRST`].
pub const OBLIVION_SKILL_AV_LAST: u32 = 32;

/// A skill never rises past 100 by use (`Oblivion.exe` `0x668b30`).
pub const SKILL_USE_CAP: u16 = 100;

/// How a skill relates to the player's class: in its specialization, among
/// its seven major skills, both, or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkillClass {
    pub specialization: bool,
    pub major: bool,
}

impl SkillClass {
    /// The class of the skill with actor-value code `skill_av` for a class
    /// with specialization `class_specialization` (`0` Combat, `1` Magic,
    /// `2` Stealth) and `major_skills` (actor-value codes). A code outside the
    /// 21 skills is in no specialization.
    #[must_use]
    pub fn of(skill_av: u32, class_specialization: u32, major_skills: &[u32]) -> Self {
        let in_roster = (OBLIVION_SKILL_AV_FIRST..=OBLIVION_SKILL_AV_LAST).contains(&skill_av);
        Self {
            specialization: in_roster
                && (skill_av - OBLIVION_SKILL_AV_FIRST) / 7 == class_specialization,
            major: major_skills.contains(&skill_av),
        }
    }
}

/// The five `fSkillUse*` settings of the classic-TES cost. `Copy`; held
/// inline by [`super::LevelingModel::SkillUse`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillUseCosts {
    /// `fSkillUseFactor`.
    pub factor: f32,
    /// `fSkillUseExp`.
    pub exp: f32,
    /// `fSkillUseMinorMult`: applies to a skill that is not major.
    pub minor_mult: f32,
    /// `fSkillUseMajorMult`: replaces the minor multiplier for a major skill.
    pub major_mult: f32,
    /// `fSkillUseSpecMult`: applies to a skill of the class's specialization.
    pub spec_mult: f32,
}

impl SkillUseCosts {
    /// Oblivion's resolved settings: `Oblivion.esm` authors Factor 0.35 and
    /// Exp 1.5; the executable's defaults supply Minor 1.25, Major 0.75 and
    /// Spec 0.75 (`docs/engine/skill-use.md` §1, V1).
    pub const OBLIVION: Self = Self {
        factor: 0.35,
        exp: 1.5,
        minor_mult: 1.25,
        major_mult: 0.75,
        spec_mult: 0.75,
    };

    /// The experience a skill at value `level` needs to rise by one. The
    /// original stores 1.0 when the computed cost is 0 (`0x663c50`), so a
    /// skill at 0 still needs something.
    #[must_use]
    pub fn cost(&self, level: u16, class: SkillClass) -> f32 {
        let base = (self.factor * f32::from(level)).powf(self.exp);
        let spec = if class.specialization {
            self.spec_mult
        } else {
            1.0
        };
        let tier = if class.major {
            self.major_mult
        } else {
            self.minor_mult
        };
        let cost = base * spec * tier;
        if cost > 0.0 && cost.is_finite() {
            cost
        } else {
            1.0
        }
    }

    /// Bank `amount` experience against a skill and return what it reached.
    ///
    /// Mirrors the original's entry (`0x668b30`): a skill at 100 or more
    /// banks nothing; otherwise `banked += amount`, and each time the cost is
    /// **strictly below** the banked experience the skill rises by one and the
    /// cost is paid, the surplus staying banked. (The original ranks once per
    /// call; this loops it, as the engine's single entry point returns every
    /// rank reached, OSL-XP-001.) A non-positive or non-finite `amount` banks
    /// nothing.
    #[must_use]
    pub fn bank(&self, value: u16, banked: f32, class: SkillClass, amount: f32) -> SkillBank {
        let mut out = SkillBank {
            value,
            banked: sanitize(banked),
            ranks: Vec::new(),
        };
        if value >= SKILL_USE_CAP || !(amount > 0.0 && amount.is_finite()) {
            return out;
        }
        out.banked += amount;
        while out.value < SKILL_USE_CAP {
            let cost = self.cost(out.value, class);
            if cost >= out.banked {
                break;
            }
            out.banked = (out.banked - cost).max(0.0);
            out.value += 1;
            out.ranks.push(out.value);
        }
        out
    }
}

/// The experience each Oblivion skill's action earns: the two use values of the skill's `SKIL` record
/// (`DATA`: value 1, value 2), which UESP's *Increasing Skills* "Exp./Action" column lists and which
/// `Oblivion.exe` reads through `0x668c30(skill, usage, mult)` (`docs/engine/skill-use.md` §1). Held as
/// data (OSL-USE-001), by actor-value code in `CLAS` order (Armorer 12 first, Speechcraft 32 last; the
/// roster's names live once, in the plugin's `OBLIVION_ACTOR_VALUES`). The real-data test
/// `oblivion_skill_use_values_match_the_masters_skil_records` checks it against `Oblivion.esm`. Each
/// row is value 1 (`usage` 0) and value 2 (`usage` 1).
pub const OBLIVION_SKILL_USE_VALUES: [(f32, f32); 21] = [
    (1.5, 1.0),   // Armorer
    (0.03, 0.04), // Athletics
    (0.5, 1.0),   // Blade
    (1.25, 1.0),  // Block
    (0.5, 1.0),   // Blunt
    (0.6, 1.0),   // Hand to Hand
    (1.25, 1.0),  // Heavy Armor
    (5.0, 0.5),   // Alchemy
    (0.0, 4.0),   // Alteration
    (0.0, 6.0),   // Conjuration
    (0.0, 1.2),   // Destruction
    (0.0, 3.0),   // Illusion
    (0.0, 3.0),   // Mysticism
    (0.0, 0.6),   // Restoration
    (0.3, 3.0),   // Acrobatics
    (1.5, 1.0),   // Light Armor
    (0.8, 1.0),   // Marksman
    (0.4, 1.0),   // Mercantile
    (1.5, 1.0),   // Security
    (0.75, 2.0),  // Sneak
    (2.4, 1.0),   // Speechcraft
];

/// The experience of one use of the skill with actor-value code `skill_av` (12 to 32): `use value[usage]`
/// (`usage` 0 or 1), times `mult` when `mult` is not 0 (`0x668c30`); `None` for a code outside the 21
/// skills or a result not above 0 (the original drops it).
#[must_use]
pub fn skill_use_amount(skill_av: u32, usage: u8, mult: f32) -> Option<f32> {
    let &(first, second) = OBLIVION_SKILL_USE_VALUES.get(skill_av.checked_sub(OBLIVION_SKILL_AV_FIRST)? as usize)?;
    let value = if usage == 0 { first } else { second };
    let amount = if mult != 0.0 { value * mult } else { value };
    (amount > 0.0 && amount.is_finite()).then_some(amount)
}

/// The armor slots a hit can strike, in the executable's roll order (`0x5e5a00`): five weighted slots,
/// then the shield as the remainder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmorHitSlot {
    Helmet,
    Cuirass,
    Greaves,
    Gauntlets,
    Boots,
    Shield,
}

impl ArmorHitSlot {
    /// The five rolled slots with their setting and executable default (`iArmorDamage*Chance`: 10, 25,
    /// 15, 10, 10 of 100; the shield takes the rest, 30).
    pub const ROLLED: [(Self, &'static str, u32); 5] = [
        (Self::Helmet, "iArmorDamageHelmChance", 10),
        (Self::Cuirass, "iArmorDamageCuirassChance", 25),
        (Self::Greaves, "iArmorDamageGreavesChance", 15),
        (Self::Gauntlets, "iArmorDamageGauntletsChance", 10),
        (Self::Boots, "iArmorDamageBootsChance", 10),
    ];

    /// The slot a roll of `0..100` strikes under `chances` (the five rolled slots' weights, in
    /// [`Self::ROLLED`] order): the first slot whose running total exceeds the roll; past the fifth, the shield.
    #[must_use]
    pub fn from_roll(chances: [u32; 5], roll: u32) -> Self {
        let mut total = 0;
        for (slot, chance) in Self::ROLLED.iter().map(|(slot, ..)| *slot).zip(chances) {
            total += chance;
            if roll < total {
                return slot;
            }
        }
        Self::Shield
    }

    /// The Oblivion biped-flag bits the slot's piece occupies (xEdit `wbBipedFlags`: Head 0, Hair 1, Upper
    /// Body 2, Lower Body 3, Hand 4, Foot 5, Shield 13).
    #[must_use]
    pub const fn biped_mask(self) -> u32 {
        match self {
            Self::Helmet => 0b11,
            Self::Cuirass => 1 << 2,
            Self::Greaves => 1 << 3,
            Self::Gauntlets => 1 << 4,
            Self::Boots => 1 << 5,
            Self::Shield => 1 << 13,
        }
    }
}

/// How many times a hit re-rolls when the struck slot holds nothing (`0x5e5a00` loops seven times).
pub const ARMOR_HIT_ATTEMPTS: usize = 7;

fn sanitize(banked: f32) -> f32 {
    if banked.is_finite() && banked > 0.0 {
        banked
    } else {
        0.0
    }
}

/// The result of [`SkillUseCosts::bank`].
#[derive(Debug, Clone, PartialEq)]
pub struct SkillBank {
    /// The skill's value after banking.
    pub value: u16,
    /// The experience left banked toward the next rank.
    pub banked: f32,
    /// Each value the skill reached, in order (empty when it did not rise).
    pub ranks: Vec<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: SkillClass = SkillClass {
        specialization: false,
        major: false,
    };
    const SPEC: SkillClass = SkillClass {
        specialization: true,
        major: false,
    };
    const MAJOR: SkillClass = SkillClass {
        specialization: false,
        major: true,
    };
    const BOTH: SkillClass = SkillClass {
        specialization: true,
        major: true,
    };

    fn close(a: f32, b: f32, tol: f32) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    /// PRD §7.2 scenario 1: UESP *Oblivion: Increasing Skills*, "Experience
    /// Points Needed", at skill 5, 25, 50, 75 and 99. The page prints four
    /// significant digits at the high levels and truncates its low digit
    /// (24.265 prints as 24.26) and its S+M column is a little low (41.16 for 41.18), hence the 0.01, 0.02 and (above 100) 0.1 tolerances; a cell
    /// the page marks impossible (a spec or major skill at 5, S+M at 25) is
    /// not pinned.
    #[test]
    fn the_cost_table_matches_uesp() {
        let c = SkillUseCosts::OBLIVION;
        close(c.cost(5, BASE), 2.89, 0.01);
        close(c.cost(25, BASE), 32.35, 0.01);
        close(c.cost(25, SPEC), 24.26, 0.01);
        close(c.cost(25, MAJOR), 19.41, 0.01);
        close(c.cost(50, BASE), 91.51, 0.01);
        close(c.cost(50, SPEC), 68.63, 0.01);
        close(c.cost(50, MAJOR), 54.90, 0.01);
        close(c.cost(50, BOTH), 41.16, 0.02);
        close(c.cost(75, BASE), 168.1, 0.1);
        close(c.cost(75, SPEC), 126.0, 0.1);
        close(c.cost(75, MAJOR), 100.8, 0.1);
        close(c.cost(75, BOTH), 75.64, 0.02);
        close(c.cost(99, BASE), 254.9, 0.1);
        close(c.cost(99, SPEC), 191.2, 0.1);
        close(c.cost(99, MAJOR), 152.9, 0.1);
        close(c.cost(99, BOTH), 114.7, 0.1);
    }

    /// The page's worked examples (Alchemy for four standard classes). Its
    /// 19.12 is the only value off the formula by more than a rounding step
    /// (the exact value is 19.14; the same page's Nightblade 31.89 is 31.90),
    /// so the tolerance is 0.02: the executable's formula is the source.
    #[test]
    fn the_worked_examples_hold() {
        let c = SkillUseCosts::OBLIVION;
        close(c.cost(30, BOTH), 19.12, 0.02); // Mage, 30 → 31
        close(c.cost(25, MAJOR), 19.41, 0.01); // Assassin, 25 → 26
        close(c.cost(30, MAJOR), 25.51, 0.02); // Assassin, 30 → 31
        close(c.cost(10, SPEC), 6.13, 0.01); // Nightblade, 10 → 11
        close(c.cost(30, SPEC), 31.89, 0.02); // Nightblade, 30 → 31
        close(c.cost(30, BASE), 42.53, 0.01); // Thief, 30 → 31
    }

    /// The page's rollover example: 4 experience at skill 5 (needs 2.89)
    /// leaves 1.11 banked at skill 6, whose next rank costs 4.0.
    #[test]
    fn surplus_rolls_over() {
        let c = SkillUseCosts::OBLIVION;
        let out = c.bank(5, 0.0, BASE, 4.0);
        assert_eq!(out.ranks, vec![6]);
        assert_eq!(out.value, 6);
        close(out.banked, 1.11, 0.01);
        // 2.69 more reaches skill 7 (the page: "only 2.69 more ... to reach 7").
        let again = c.bank(6, out.banked, BASE, 2.70);
        assert_eq!(again.ranks, vec![7]);
    }

    /// One large amount returns every rank it reaches, in order.
    #[test]
    fn a_large_amount_returns_every_rank_in_order() {
        let c = SkillUseCosts::OBLIVION;
        let out = c.bank(5, 0.0, BASE, 100.0);
        assert!(out.ranks.len() > 3);
        assert_eq!(out.ranks[0], 6);
        assert!(out.ranks.windows(2).all(|w| w[1] == w[0] + 1));
        assert_eq!(out.value, *out.ranks.last().unwrap());
    }

    /// `0x668bc8`: the rank-up needs the cost strictly below the experience;
    /// an equal value does not rank.
    #[test]
    fn an_equal_cost_does_not_rank() {
        let c = SkillUseCosts {
            factor: 1.0,
            exp: 1.0,
            minor_mult: 1.0,
            major_mult: 1.0,
            spec_mult: 1.0,
        };
        assert_eq!(c.cost(5, BASE), 5.0);
        assert!(c.bank(5, 0.0, BASE, 5.0).ranks.is_empty());
        assert_eq!(c.bank(5, 0.0, BASE, 5.0).banked, 5.0);
        assert_eq!(c.bank(5, 0.0, BASE, 5.01).ranks, vec![6]);
    }

    /// V2: a skill at 100 banks nothing, and use never takes it past 100.
    #[test]
    fn a_skill_never_exceeds_100_by_use() {
        let c = SkillUseCosts::OBLIVION;
        let at_cap = c.bank(100, 1.5, BASE, 50.0);
        assert_eq!(
            at_cap,
            SkillBank {
                value: 100,
                banked: 1.5,
                ranks: vec![]
            }
        );
        let huge = c.bank(99, 0.0, BOTH, 1.0e9);
        assert_eq!(huge.value, 100);
        assert_eq!(huge.ranks, vec![100]);
    }

    #[test]
    fn nonsense_amounts_bank_nothing() {
        let c = SkillUseCosts::OBLIVION;
        for amount in [0.0, -3.0, f32::NAN, f32::INFINITY] {
            let out = c.bank(10, 2.0, BASE, amount);
            assert_eq!(
                (out.value, out.banked, out.ranks.len()),
                (10, 2.0, 0),
                "{amount}"
            );
        }
        // A corrupt banked value reads as nothing banked.
        assert_eq!(c.bank(10, f32::NAN, BASE, 1.0).banked, 1.0);
    }

    /// `0x663c50` stores 1.0 when the cost is 0 (a skill at 0).
    #[test]
    fn a_zero_cost_is_one() {
        assert_eq!(SkillUseCosts::OBLIVION.cost(0, BASE), 1.0);
    }

    /// The class resolves from the actor-value code: seven skills per
    /// specialization, Combat 12–18, Magic 19–25, Stealth 26–32; majors from
    /// the list. Editing the list changes the next answer (OSL-CLS-002).
    #[test]
    fn the_class_reads_the_record_each_time() {
        // Blade is 14 (Combat), Alchemy 19 (Magic), Light Armor 27 (Stealth).
        let majors = [14, 19];
        assert_eq!(SkillClass::of(14, 0, &majors), BOTH);
        assert_eq!(SkillClass::of(19, 0, &majors), MAJOR);
        assert_eq!(SkillClass::of(27, 2, &[]), SPEC);
        assert_eq!(SkillClass::of(27, 0, &[]), BASE);
        let edited = [27];
        assert_eq!(SkillClass::of(14, 0, &edited), SPEC);
        assert_eq!(SkillClass::of(27, 0, &edited), MAJOR);
        // Outside the roster: no specialization.
        assert_eq!(SkillClass::of(5, 0, &[]), BASE);
        assert_eq!(SkillClass::of(33, 2, &[]), BASE);
    }

    /// OSL-USE-001: there is a row for every skill the ruleset names, and a row holds the amounts UESP lists
    /// (spot checks across the three specializations; codes: Blade 14, Block 15, Alteration 20, Mercantile 29).
    #[test]
    fn every_oblivion_skill_has_a_use_row() {
        assert_eq!(OBLIVION_SKILL_USE_VALUES.len(), crate::character::SkillSet::OBLIVION.len());
        assert_eq!((OBLIVION_SKILL_AV_LAST - OBLIVION_SKILL_AV_FIRST + 1) as usize, OBLIVION_SKILL_USE_VALUES.len());
        for code in OBLIVION_SKILL_AV_FIRST..=OBLIVION_SKILL_AV_LAST {
            assert!(
                skill_use_amount(code, 0, 0.0).is_some() || skill_use_amount(code, 1, 0.0).is_some(),
                "skill {code} earns nothing"
            );
        }
        assert_eq!(skill_use_amount(14, 0, 0.0), Some(0.5)); // Blade: each hit on a target
        assert_eq!(skill_use_amount(15, 0, 0.0), Some(1.25)); // Block: each blocked hit
        assert_eq!(skill_use_amount(20, 1, 0.0), Some(4.0)); // Alteration: each spell cast
        assert_eq!(skill_use_amount(20, 0, 0.0), None, "school skills' value 1 is 0");
        assert_eq!(skill_use_amount(29, 0, 5.0), Some(2.0)); // Mercantile: a multiplier scales the value
        assert_eq!(skill_use_amount(11, 0, 0.0), None, "Encumbrance is not a skill");
        assert_eq!(skill_use_amount(33, 0, 0.0), None);
    }

    /// `0x5e5a00`: a roll under the first weight strikes the helmet; each weight extends the running total;
    /// what is left of the 100 (30 by default) is the shield.
    #[test]
    fn a_roll_strikes_the_slot_whose_running_total_covers_it() {
        let chances = [10, 25, 15, 10, 10];
        let slot = |roll| ArmorHitSlot::from_roll(chances, roll);
        assert_eq!(slot(0), ArmorHitSlot::Helmet);
        assert_eq!(slot(9), ArmorHitSlot::Helmet);
        assert_eq!(slot(10), ArmorHitSlot::Cuirass);
        assert_eq!(slot(34), ArmorHitSlot::Cuirass);
        assert_eq!(slot(35), ArmorHitSlot::Greaves);
        assert_eq!(slot(49), ArmorHitSlot::Greaves);
        assert_eq!(slot(50), ArmorHitSlot::Gauntlets);
        assert_eq!(slot(60), ArmorHitSlot::Boots);
        assert_eq!(slot(69), ArmorHitSlot::Boots);
        assert_eq!(slot(70), ArmorHitSlot::Shield);
        assert_eq!(slot(99), ArmorHitSlot::Shield);
        // The defaults sum to 70, leaving the shield its authored 30.
        assert_eq!(ArmorHitSlot::ROLLED.iter().map(|(.., chance)| chance).sum::<u32>(), 70);
    }
}
