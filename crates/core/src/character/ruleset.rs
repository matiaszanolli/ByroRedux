//! Per-game character ruleset (CHARAL) — the engine-facing Resource.
//!
//! Assembled once per loaded game from AUTHORED data (parsed GMST / AVIF
//! constants) + engine-supplied tables, and held as an ECS [`Resource`]. The
//! runtime reads it game-agnostically: the per-game seam is the *data* in the
//! tables, never a branch in the consumer (the CHARAL doctrine,
//! `docs/engine/charal.md`).

use super::attribute::AttributeSet;
use super::derived::{DerivedOutput, DerivedScope, DerivedStatFormula};
use super::leveling::LevelingModel;
use super::skill::SkillSet;
use crate::ecs::components::ActorValues;
use crate::ecs::resource::Resource;

/// The per-game character ruleset: the derived-stat formula table + the
/// leveling model.
///
/// The derived table is a **flat `Vec`** keyed by output AVIF FormID and
/// scanned linearly. A game computes only ~6–10 derived stats, so a
/// contiguous array beats a `HashMap` on both lookup latency (no hash, no
/// pointer-chase) and footprint at that N — and it stays cache-resident.
#[derive(Debug, Clone)]
pub struct CharacterRuleset {
    /// The game's primary-attribute roster — 7 SPECIAL (Fallout), 8 (TES
    /// classic) or none (Skyrim / Starfield). ENGINE-SUPPLIED membership; the
    /// AVIF FormIDs each resolves to stay AUTHORED.
    pub attributes: AttributeSet,
    /// The game's skill roster + governing-attribute map — 21 (Oblivion),
    /// 18 ungoverned (Skyrim), or none (FO4 / FO76). ENGINE-SUPPLIED.
    pub skills: SkillSet,
    /// `(output AVIF FormID, formula)` — the stat each formula produces and
    /// how to compute it from base AVs + level.
    derived: Vec<(u32, DerivedStatFormula)>,
    /// XP curve + per-level reward.
    pub leveling: LevelingModel,
}

impl CharacterRuleset {
    /// An empty ruleset with the given leveling model and no attribute roster;
    /// attach one with [`Self::with_attributes`] and populate the derived
    /// table with [`Self::with_derived`].
    pub fn new(leveling: LevelingModel) -> Self {
        Self {
            attributes: AttributeSet::default(),
            skills: SkillSet::default(),
            derived: Vec::new(),
            leveling,
        }
    }

    /// Set the primary-attribute roster (builder style) — the per-game seam
    /// picks the canonical [`AttributeSet`] for its family.
    #[must_use]
    pub fn with_attributes(mut self, attributes: AttributeSet) -> Self {
        self.attributes = attributes;
        self
    }

    /// Set the skill roster + governing-attribute map (builder style) — the
    /// per-game seam picks the canonical [`SkillSet`] for its family.
    #[must_use]
    pub fn with_skills(mut self, skills: SkillSet) -> Self {
        self.skills = skills;
        self
    }

    /// Register a derived-stat formula producing `output_avif` (builder
    /// style). The caller resolves `output_avif` from the parsed AVIF set —
    /// the formula *shape* is engine-supplied (the locked coefficients), the
    /// FormID is AUTHORED.
    #[must_use]
    pub fn with_derived(mut self, output_avif: u32, formula: DerivedStatFormula) -> Self {
        self.push_derived(output_avif, formula);
        self
    }

    /// Register a derived-stat formula in place — the conditional /
    /// resolve-or-skip form used by the per-game builders ([`super::fallout`]).
    ///
    /// A stat may be registered as **several rows** with the same
    /// `output_avif`; [`Self::derived_value`] sums them. This is how
    /// multi-attribute stats that exceed the two-input [`DerivedStatFormula`]
    /// layout are expressed (e.g. Oblivion Fatigue = STR + WIL + AGI + END, as
    /// four affine rows). Multi-row stats must use **uncapped, unrounded,
    /// absolute** rows — per-row caps/rounding would apply before the sum, not
    /// to the total.
    pub fn push_derived(&mut self, output_avif: u32, formula: DerivedStatFormula) {
        self.derived.push((output_avif, formula));
    }

    /// The **first** formula row producing `output_avif`, if this game derives
    /// that stat. For a multi-row stat this is only one contribution — read
    /// metadata (scope) here, but take the value from [`Self::derived_value`],
    /// which sums every row.
    #[inline]
    pub fn derived_formula(&self, output_avif: u32) -> Option<&DerivedStatFormula> {
        self.derived
            .iter()
            .find(|(id, _)| *id == output_avif)
            .map(|(_, f)| f)
    }

    /// #4674 (CHAR-2026-09-21-D4-01) — the distinct output AVIFs of every
    /// `PlayerOnly`-scoped derived row (Health, Action Points, Oblivion's
    /// pools, …). The player stamper drops the NPC-baked carried pairs for
    /// these keys — the NPC path answers them by construction the player
    /// must not inherit — and re-evaluates them through
    /// [`Self::derived_value`] so the player's own formulas finally apply.
    pub fn player_only_output_avifs(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self
            .derived
            .iter()
            .filter(|(_, f)| f.scope == DerivedScope::PlayerOnly)
            .map(|(id, _)| *id)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Compute derived stat `output_avif` for an actor — the **sum** of every
    /// registered row producing it, each evaluated against the actor's base
    /// AVs + level. Single-row stats (the common case) sum to that one row.
    /// `None` when no row produces the stat (it's an authored / equipment AV,
    /// e.g. Damage Resistance — read it from [`ActorValues`] directly instead).
    #[inline]
    pub fn derived_value(&self, output_avif: u32, avs: &ActorValues, level: u16) -> Option<f32> {
        let mut sum = 0.0;
        let mut found = false;
        for (id, f) in &self.derived {
            if *id == output_avif {
                sum += f.eval(avs, level);
                found = true;
            }
        }
        found.then_some(sum)
    }

    /// #5042 — the one composed actor-value reading for an arbitrary actor:
    /// what `GetActorValue` and the melee-damage bonus both report.
    ///
    /// * A carried value whose base was authored (`ActorValues::set_base`:
    ///   the NPC derivation, the player stamper, a save, `SetBase`) wins, as
    ///   `current()`.
    /// * Otherwise, when this game derives the stat for any actor
    ///   (`ActorGeneral` + `Absolute`, the #2933 contract), the formula output
    ///   is the base: `derived_value + permanent + temporary − damage` of
    ///   whatever modifier-only entry a constant spell, `modav` or the SDK
    ///   left. Such an entry's `0.0` base is a placeholder, so treating it as
    ///   carried returned the bare modifier (FNV Finesse CritChance `5`, not
    ///   `Luck + 5`).
    /// * Anything else — an authored AV this game does not derive, a
    ///   `PlayerOnly` row on a non-player, a `Multiplier` row — composes the
    ///   entry as carried: `current()`, `0.0` when absent.
    pub fn actor_value(&self, avif: u32, avs: &ActorValues, level: u16) -> f32 {
        let carried = avs.get(avif);
        if carried.is_some_and(|value| value.base_authored) {
            return avs.current(avif);
        }
        let derives_for_any_actor = self.derived_formula(avif).is_some_and(|formula| {
            formula.scope == DerivedScope::ActorGeneral && formula.kind == DerivedOutput::Absolute
        });
        if !derives_for_any_actor {
            return avs.current(avif);
        }
        let base = self.derived_value(avif, avs, level).unwrap_or(0.0);
        let layers = carried.map_or(0.0, |value| {
            value.permanent_mod + value.temporary_mod - value.damage
        });
        base + layers
    }

    /// #5039 — re-evaluate every `PlayerOnly` + `Absolute` stat (FO3/FNV/FO4
    /// Health + AP, …) against the player's *current* inputs and write the
    /// results as the authored base. Returns whether any base changed.
    ///
    /// The player's derived pools are stamped into the base layer, because
    /// combat, drowning, restoration, the HUD and death all read
    /// `ActorValues::current` directly with no ruleset in hand. The stamp is
    /// only correct while it is re-run whenever an input changes. Formula
    /// inputs read `current()` (except rows marked `*_from_base`), so an
    /// Endurance ability, `modav Endurance`, the SDK or a level change
    /// reaches Health on the next refresh. Modifier and damage layers on the
    /// output are left alone, per the actor-value composition model.
    ///
    /// Allocation-free — it runs every frame for the player. Stats are
    /// refreshed in table order, so a row that read another `PlayerOnly`
    /// output would see that output already refreshed; no shipped row does.
    pub fn refresh_player_only_bases(&self, avs: &mut ActorValues, level: u16) -> bool {
        let mut changed = false;
        for (index, (key, formula)) in self.derived.iter().enumerate() {
            // Scope and kind come from a stat's first row, as in
            // `derived_formula`; its value is the sum of all its rows.
            if self.derived[..index].iter().any(|(earlier, _)| earlier == key) {
                continue;
            }
            if formula.scope != DerivedScope::PlayerOnly || formula.kind != DerivedOutput::Absolute
            {
                continue;
            }
            let Some(value) = self.derived_value(*key, avs, level) else {
                continue;
            };
            if avs
                .get(*key)
                .is_some_and(|carried| carried.base_authored && carried.base == value)
            {
                continue;
            }
            avs.set_base(*key, value);
            changed = true;
        }
        changed
    }

    /// #2934 — DOCTRINE GAP (recorded, not fixed here). CHARAL's spec gives
    /// this struct a `skill_calc: SkillDerivation { base, attr_mult, luck_mult }`
    /// field so the FNV/FO3 auto-calc *rule* (`skill = 2 + 2·governing +
    /// ceil(Luck/2)`) lives with the other per-game rules. That field does not
    /// exist workspace-wide; the coefficients currently sit in
    /// `crates/plugin/src/esm/records/actor_value_derive.rs`, i.e. in a
    /// consumer rather than in the ruleset. The attribute-roster half of the
    /// duplication is closed (that module now reads `AttributeSet::FALLOUT`).
    /// The rule half is only partly GMST-sourceable (#3173): `base` is
    /// per-skill and authored (`fAVDSkill<DisplayName>Base`, 13 FNV GMSTs,
    /// keyed by display name — the inverse of the `AVIF` record-identity
    /// convention); `attr_mult` and `luck_mult` are geckwiki-documented but
    /// authored by neither `Fallout3.esm` nor `FalloutNV.esm` — they stay
    /// engine constants when this field lands, not a GMST read.
    ///
    /// Number of derived-stat formula **rows** — not the number of distinct
    /// stats.
    ///
    /// #2935 — a stat may be registered across several rows whose values sum
    /// (`derived_value` adds every row matching an output id; TES Fatigue is
    /// the motivating case, and Oblivion registers 8 rows for 5 stats). The
    /// old name and docstring said "derived stats", which reads as a stat
    /// count and made the flat-`Vec` sizing rationale look tighter than it is.
    pub fn derived_row_len(&self) -> usize {
        self.derived.len()
    }
}

impl Resource for CharacterRuleset {}

#[cfg(test)]
mod tests {
    use super::super::derived::DerivedInput;
    use super::*;

    // Stand-in resolved AVIF FormIDs (what the loader would pull from the
    // parsed AVIF set).
    const STR: u32 = 0x05;
    const END: u32 = 0x07;
    const AGI: u32 = 0x0A;
    const AV_HEALTH: u32 = 0x2C9;
    const AV_AP: u32 = 0x2D0;
    const AV_CARRY: u32 = 0x2D1;

    fn av(id: u32) -> DerivedInput {
        DerivedInput::actor_value(id)
    }

    /// Build an FO4-shaped ruleset against resolved FormIDs and evaluate it
    /// end-to-end — the integration the loader performs per game.
    fn fo4_ruleset() -> CharacterRuleset {
        CharacterRuleset::new(LevelingModel::FO4)
            .with_derived(
                AV_HEALTH,
                DerivedStatFormula::bilinear(av(END), 4.5, DerivedInput::LEVEL, 2.5, 0.5, 77.5)
                    .floored(),
            )
            .with_derived(AV_AP, DerivedStatFormula::affine(av(AGI), 10.0, 60.0))
            .with_derived(AV_CARRY, DerivedStatFormula::affine(av(STR), 10.0, 200.0))
    }

    #[test]
    fn derived_value_evaluates_registered_formula() {
        let rs = fo4_ruleset();
        let avs = ActorValues::from_pairs([(STR, 7.0), (END, 5.0), (AGI, 6.0)]);
        // Health: floor(77.5 + 22.5 + 2.5·L + 0.5·L·5); L 1 → floor(105) = 105.
        assert_eq!(rs.derived_value(AV_HEALTH, &avs, 1), Some(105.0));
        // AP: 60 + 10·6 = 120.
        assert_eq!(rs.derived_value(AV_AP, &avs, 1), Some(120.0));
        // Carry Weight: 200 + 10·7 = 270.
        assert_eq!(rs.derived_value(AV_CARRY, &avs, 1), Some(270.0));
        assert_eq!(rs.derived_row_len(), 3);
    }

    #[test]
    fn unregistered_stat_is_none() {
        // Damage Resistance et al. aren't derived — no formula, read the AV.
        let rs = fo4_ruleset();
        let avs = ActorValues::new();
        assert_eq!(rs.derived_value(0xDEAD, &avs, 1), None);
        assert!(rs.derived_formula(0xDEAD).is_none());
    }

    /// #5042 — FO4 `AbStrongStats` shape: a constant +140 Carry Weight on an
    /// actor whose derivation carries no Carry Weight key. The modifier-only
    /// entry must compose onto the formula, not replace it.
    #[test]
    fn modifier_only_entry_composes_onto_the_formula() {
        let rs = fo4_ruleset();
        let mut avs = ActorValues::from_pairs([(STR, 7.0)]);
        avs.mod_permanent(STR, 10.0); // the same ability's STR +10
        avs.mod_permanent(AV_CARRY, 140.0);
        avs.mod_temporary(AV_CARRY, 10.0);
        // 200 + 10·(7 + 10) = 370, + 140 permanent + 10 temporary.
        assert_eq!(rs.actor_value(AV_CARRY, &avs, 1), 520.0);
        // Damage is subtracted from the composed value, not from the formula
        // alone.
        avs.apply_damage(AV_CARRY, 20.0);
        assert_eq!(rs.actor_value(AV_CARRY, &avs, 1), 500.0);
    }

    /// #5042 — an authored base still wins over the formula (FO4 companions
    /// that carry Carry Weight via PRPS), with its modifiers.
    #[test]
    fn authored_base_wins_over_the_formula() {
        let rs = fo4_ruleset();
        let mut avs = ActorValues::from_pairs([(STR, 7.0), (AV_CARRY, 300.0)]);
        avs.mod_permanent(AV_CARRY, 10.0);
        assert_eq!(rs.actor_value(AV_CARRY, &avs, 1), 310.0);
    }

    /// #5042 — rows outside the actor-general Absolute contract compose the
    /// entry as carried: an underived AV, a `PlayerOnly` row, a `Multiplier`.
    #[test]
    fn non_actor_general_rows_compose_as_carried() {
        let rs = CharacterRuleset::new(LevelingModel::FO4)
            .with_derived(AV_HEALTH, DerivedStatFormula::affine(av(END), 10.0, 0.0).player_only())
            .with_derived(AV_AP, DerivedStatFormula::affine(av(AGI), 0.1, 1.0).as_multiplier());
        let mut avs = ActorValues::from_pairs([(END, 5.0), (AGI, 5.0)]);
        avs.mod_permanent(AV_HEALTH, 4.0);
        avs.mod_permanent(AV_AP, 2.0);
        avs.mod_permanent(0xDEAD, 3.0);
        assert_eq!(rs.actor_value(AV_HEALTH, &avs, 1), 4.0);
        assert_eq!(rs.actor_value(AV_AP, &avs, 1), 2.0);
        assert_eq!(rs.actor_value(0xDEAD, &avs, 1), 3.0);
        assert_eq!(rs.actor_value(0xBEEF, &avs, 1), 0.0, "absent stays 0");
    }

    /// #5039 — the player's `PlayerOnly` bases follow their inputs: an
    /// Endurance modifier or a level change moves Health on the next refresh,
    /// and damage on the output survives it.
    #[test]
    fn player_only_refresh_follows_endurance_and_level() {
        let rs = CharacterRuleset::new(LevelingModel::FO4).with_derived(
            AV_HEALTH,
            DerivedStatFormula::bilinear(av(END), 4.5, DerivedInput::LEVEL, 2.5, 0.5, 77.5)
                .floored()
                .player_only(),
        );
        let mut avs = ActorValues::from_pairs([(END, 5.0)]);
        assert!(rs.refresh_player_only_bases(&mut avs, 1));
        assert_eq!(avs.current(AV_HEALTH), 105.0);
        assert!(!rs.refresh_player_only_bases(&mut avs, 1), "idempotent");

        avs.apply_damage(AV_HEALTH, 30.0);
        avs.mod_permanent(END, 2.0); // modav Endurance 2
        assert!(rs.refresh_player_only_bases(&mut avs, 1));
        // floor(77.5 + 4.5·7 + 2.5 + 0.5·7) = 115, − 30 damage.
        assert_eq!(avs.current(AV_HEALTH), 85.0);

        assert!(rs.refresh_player_only_bases(&mut avs, 2));
        // floor(77.5 + 31.5 + 5 + 7) = 121, − 30.
        assert_eq!(avs.current(AV_HEALTH), 91.0);
    }

    /// #5039 — actor-general rows are never stamped by the player refresh.
    #[test]
    fn player_only_refresh_leaves_actor_general_rows_derived() {
        let rs = fo4_ruleset();
        let mut avs = ActorValues::from_pairs([(STR, 7.0), (END, 5.0), (AGI, 6.0)]);
        assert!(!rs.refresh_player_only_bases(&mut avs, 1));
        assert!(avs.get(AV_CARRY).is_none());
    }

    #[test]
    fn leveling_travels_with_the_ruleset() {
        let rs = fo4_ruleset();
        assert_eq!(rs.leveling.xp_to_next(10), 875.0);
    }
}
