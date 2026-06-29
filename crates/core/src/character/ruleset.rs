//! Per-game character ruleset (CHARAL) — the engine-facing Resource.
//!
//! Assembled once per loaded game from AUTHORED data (parsed GMST / AVIF
//! constants) + engine-supplied tables, and held as an ECS [`Resource`]. The
//! runtime reads it game-agnostically: the per-game seam is the *data* in the
//! tables, never a branch in the consumer (the CHARAL doctrine,
//! `docs/engine/charal.md`).

use super::derived::DerivedStatFormula;
use super::leveling::LevelingModel;
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
    /// `(output AVIF FormID, formula)` — the stat each formula produces and
    /// how to compute it from base AVs + level.
    derived: Vec<(u32, DerivedStatFormula)>,
    /// XP curve + per-level reward.
    pub leveling: LevelingModel,
}

impl CharacterRuleset {
    /// An empty ruleset with the given leveling model; populate the derived
    /// table with [`Self::with_derived`].
    pub fn new(leveling: LevelingModel) -> Self {
        Self {
            derived: Vec::new(),
            leveling,
        }
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
    pub fn push_derived(&mut self, output_avif: u32, formula: DerivedStatFormula) {
        self.derived.push((output_avif, formula));
    }

    /// The formula producing `output_avif`, if this game derives that stat.
    #[inline]
    pub fn derived_formula(&self, output_avif: u32) -> Option<&DerivedStatFormula> {
        self.derived
            .iter()
            .find(|(id, _)| *id == output_avif)
            .map(|(_, f)| f)
    }

    /// Compute derived stat `output_avif` for an actor — evaluate its formula
    /// against the actor's base AVs + level. `None` when no formula produces
    /// that stat (it's an authored / equipment AV, e.g. Damage Resistance —
    /// read it from [`ActorValues`] directly instead).
    #[inline]
    pub fn derived_value(&self, output_avif: u32, avs: &ActorValues, level: u16) -> Option<f32> {
        self.derived_formula(output_avif)
            .map(|f| f.eval(avs, level))
    }

    /// Number of derived stats this game computes.
    pub fn derived_len(&self) -> usize {
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
        assert_eq!(rs.derived_len(), 3);
    }

    #[test]
    fn unregistered_stat_is_none() {
        // Damage Resistance et al. aren't derived — no formula, read the AV.
        let rs = fo4_ruleset();
        let avs = ActorValues::new();
        assert_eq!(rs.derived_value(0xDEAD, &avs, 1), None);
        assert!(rs.derived_formula(0xDEAD).is_none());
    }

    #[test]
    fn leveling_travels_with_the_ruleset() {
        let rs = fo4_ruleset();
        assert_eq!(rs.leveling.xp_to_next(10), 875.0);
    }
}
