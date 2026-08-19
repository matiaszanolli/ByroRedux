//! CHARAL — the canonical character / progression system.
//!
//! The game-agnostic tier that per-game character rulesets translate into:
//! attributes, skills, perks, level, and derived stats, all resolved so the
//! gameplay runtime reads one representation regardless of source game. See
//! the design doc `docs/engine/charal.md` and the per-game data captures
//! `docs/engine/charal-fo4-ruleset.md` / `charal-fnv-fo3-ruleset.md`.
//!
//! The numeric substrate is [`crate::ecs::components::ActorValues`] (shipped
//! with #1663). This module adds the *rules* and *structure* layered over it:
//!
//! * [`derived`] — [`DerivedStatFormula`], the fixed-layout bilinear formula
//!   every Bethesda derived stat (Health, AP, Carry Weight, …) reduces to.
//! * [`leveling`] — [`LevelingModel`] (enum) + [`LevelReward`]: the FO
//!   XP-curve + reward (FO4 SPECIAL-or-perk vs FO3/FNV skill points) vs the
//!   TES skill-use model (Oblivion: 10 major-skill-ups → level).
//! * [`ruleset`] — [`CharacterRuleset`], the per-game `Resource` bundling the
//!   derived-formula table + leveling model.
//! * [`reputation`] — the reputation family: [`KarmaBand`] (FO3/FNV Karma) +
//!   [`ReputationStanding`] (FNV Fame/Infamy 4×4 grid) classifiers.
//! * [`resistance`] — the affliction family's resistance half:
//!   [`Affliction`] descriptors (radiation / poison resistance derivation) +
//!   the damage-multiplier model.
//! * [`affliction`] — the affliction family's pool/threshold half:
//!   [`AfflictionTable`] (pool → threshold band → SPECIAL penalty) +
//!   [`AfflictionStatus`] (per-actor active-band memory) +
//!   [`affliction_tick_system`] (the diff-and-reapply driver).
//! * [`regen`] — pool regeneration (Fatigue/Magicka), the first CHARAL system
//!   needing a **fixed 60 Hz tick** decoupled from the frame rate:
//!   [`PoolRegenAccumulator`] (the fixed-step clock, mirrors
//!   `crates/physics`'s accumulator) + [`PoolRegenConfig`] (per-game resolved
//!   AVIF ids) + [`pool_regen_tick_system`] (the driver).
//! * [`components`] — [`CharacterLevel`] / [`Perks`] / [`Background`], the
//!   structural per-actor ECS components.
//!
//! Per-game **population** lives at the parser boundary (FO4 `PRPS`/`DNAM`,
//! FNV/FO3 class auto-calc) in `byroredux_plugin`; this crate holds the
//! game-agnostic canonical types those boundaries feed.
//!
//! See also [`crate::combat`] and [`crate::stealth`] — CHARAL-*adjacent*
//! siblings (not submodules of this module) that read `ActorValues` as
//! inputs but evaluate at combat/stealth-resolution time against transient
//! per-hit state that never lives in `ActorValues`. `/audit-character`
//! covers their constants under Dimension 2 (CHAR-D6-05, #2962).

pub mod affliction;
pub mod attribute;
pub mod components;
pub mod derived;
pub mod fallout;
pub mod leveling;
pub mod profile;
pub mod regen;
pub mod reputation;
pub mod resistance;
pub mod ruleset;
pub mod skill;
pub mod skyrim;
pub mod tes;

pub use affliction::{
    affliction_tick_system, reevaluate_affliction, ActiveAffliction, AfflictionBand,
    AfflictionStatus, AfflictionTable, AvPenalty,
};
pub use attribute::{Attribute, AttributeSet};
pub use components::{
    Background, CharacterLevel, FactionReputation, FactionStanding, PerkRank, Perks,
};
pub use derived::{DerivedInput, DerivedOutput, DerivedScope, DerivedStatFormula, RoundMode};
pub use fallout::{fallout3_ruleset, fallout4_ruleset, falloutnv_ruleset};
pub use leveling::{LevelReward, LevelingModel};
pub use profile::{CharacterRulesProfile, NpcHealthCurve, NpcStatModel};
pub use regen::{
    magicka_regen_per_sec, pool_regen_tick_system, PoolRegenAccumulator, PoolRegenConfig,
    FATIGUE_REGEN_PER_SEC, MAGICKA_REGEN_BASE, MAGICKA_REGEN_WILLPOWER_COEFF, POOL_REGEN_DT,
};
pub use reputation::{
    affinity_band, affinity_passive_gain, affinity_reaction_delta, clamp_affinity, clamp_karma,
    karma_band, reputation_bump_points, AffinityBand, AffinityReaction, AffinityReactionSize,
    FactionRepThresholds, KarmaBand, ReputationSentiment, ReputationStanding,
    REPUTATION_BUMP_POINTS,
};
pub use resistance::{damage_multiplier, Affliction};
pub use ruleset::CharacterRuleset;
pub use skill::{ResolvedSkill, SkillDef, SkillSet};
pub use skyrim::{
    skyrim_ruleset, skyrim_skill_xp_between, skyrim_skill_xp_to_next, SKYRIM_POOL_BASE,
    SKYRIM_SKILL_USE_CURVE,
};
pub use tes::{
    oblivion_attribute_bonus, oblivion_fatigue_formulas, oblivion_health_formula,
    oblivion_health_gain_per_level, oblivion_magicka_formula, oblivion_pool_regen_config,
    oblivion_ruleset,
};

#[cfg(test)]
mod tests {
    /// Regression for CHAR-D6-05 / #2962. `combat.rs` and `stealth.rs` held
    /// CHARAL-sourced constants outside every audit skill's declared scope —
    /// this docstring's "see also" pointer is one of the fixes that closed
    /// that blind spot. Source-inspection check (same pattern as
    /// `character::regen`'s doc-drift regressions) so a future edit can't
    /// silently drop the pointer.
    #[test]
    fn mod_docstring_points_at_the_charal_adjacent_siblings() {
        let src = include_str!("mod.rs");
        assert!(
            src.contains("crate::combat") && src.contains("crate::stealth"),
            "character::mod's docstring must keep pointing at crate::combat \
             and crate::stealth as CHARAL-adjacent siblings (#2962) — without \
             it neither module is discoverable from the layer's own entry \
             point"
        );
    }
}
