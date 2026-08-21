//! CHARAL — the canonical character / progression system.
//!
//! The game-agnostic tier that per-game character rulesets translate into:
//! attributes, skills, perks, level, and derived stats, all resolved so the
//! gameplay runtime reads one representation regardless of source game. See
//! the design doc `docs/engine/charal.md` and the six per-game data captures
//! it indexes: `docs/engine/charal-fo4-ruleset.md`,
//! `charal-fnv-fo3-ruleset.md`, `charal-oblivion-ruleset.md`,
//! `charal-skyrim-ruleset.md`, `charal-fo76-ruleset.md`, and
//! `charal-starfield-ruleset.md`.
//!
//! The numeric substrate is [`crate::ecs::components::ActorValues`] (shipped
//! with #1663). This module adds the *rules* and *structure* layered over it:
//!
//! * [`derived`] — [`DerivedStatFormula`], the fixed-layout bilinear formula
//!   every Bethesda derived stat (Health, AP, Carry Weight, …) reduces to.
//! * [`leveling`] — [`LevelingModel`] (enum) + [`LevelReward`]: all three
//!   shipped shapes — `XpCurve` (Fallout: XP-to-next curve + reward, FO4
//!   SPECIAL-or-perk vs FO3/FNV skill points), `SkillUse` (classic TES —
//!   Oblivion: 10 major-skill-ups → level), and `SkillXp` (Skyrim: per-skill
//!   XP, `25·L+75` to next, +10 pool pick and a perk per level).
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
//! * [`attribute`] — [`Attribute`] / [`AttributeSet`]: the per-family
//!   attribute roster (`FALLOUT` SPECIAL, `TES_CLASSIC`, and Skyrim's
//!   deliberately empty set) plus its canonical ordering.
//! * [`skill`] — [`SkillDef`] / [`SkillSet`] / [`ResolvedSkill`]: the
//!   per-game skill roster and its governing-attribute map, resolved to
//!   `AVIF` FormIDs at load (resolve-or-skip).
//! * **Per-game family impls** — where every per-game *number* actually
//!   lives, one module per lineage:
//!   * [`fallout`] — [`fallout3_ruleset`], [`falloutnv_ruleset`],
//!     [`fallout4_ruleset`] + [`MeleeDamageConfig`].
//!   * [`tes`] — [`oblivion_ruleset`] + the classic-TES pool/level helpers.
//!   * [`skyrim`] — [`skyrim_ruleset`] + the per-skill-XP helpers.
//! * [`profile`] — [`CharacterRulesProfile`], the narrow per-game rules key
//!   the parser translates its broad `GameKind` into (FO3 and FNV share one
//!   `GameKind` but not one ruleset), plus [`NpcHealthCurve`] /
//!   [`NpcStatModel`].
//!
//! Per-game **population** lives at the parser boundary (FO4 `PRPS`/`DNAM`,
//! FNV/FO3 class auto-calc) in `byroredux_plugin`; this crate holds the
//! game-agnostic canonical types those boundaries feed. The rulesets
//! themselves have exactly one construction site —
//! `CharacterRulesProfile::build_ruleset`, reached from
//! `build_character_ruleset` (`byroredux/src/npc_spawn.rs`) — so "is game X
//! wired?" is answered there, not by whether a `*_ruleset()` builder exists
//! (Oblivion's and Skyrim's do, and are not yet reachable: #2961's matrix
//! row).
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
pub use fallout::{
    fallout3_ruleset, fallout4_ruleset, falloutnv_ruleset, melee_damage_config, MeleeDamageConfig,
};
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
    /// Regression for CHAR-D6-01 / #2958. The docstring above is the entry
    /// point every CHARAL contributor reads first, and it had drifted to
    /// covering 8 of 14 sub-modules — the five it omitted were the entire
    /// attribute/skill roster half *and* all three per-game family impls, so
    /// a reader working only from it would conclude the crate had no ruleset
    /// builders at all. Counting `pub mod` declarations against the
    /// docstring's own bullets (the `DBG_BITS` catalog-parity pattern, one
    /// crate over) makes that specific drift impossible to reintroduce: a
    /// new sub-module must be indexed, not merely declared.
    #[test]
    fn mod_docstring_indexes_every_sub_module() {
        let src = include_str!("mod.rs");
        let docstring: String = src
            .lines()
            .take_while(|line| line.starts_with("//!"))
            .collect::<Vec<_>>()
            .join("\n");
        let declared: Vec<&str> = src
            .lines()
            .filter_map(|line| line.strip_prefix("pub mod "))
            .filter_map(|rest| rest.strip_suffix(';'))
            .collect();
        assert_eq!(
            declared.len(),
            14,
            "sub-module count changed — {declared:?}"
        );
        for module in declared {
            assert!(
                docstring.contains(&format!("[`{module}`]")),
                "character::mod's docstring never mentions the `{module}` \
                 sub-module. Every `pub mod` needs an index entry: this \
                 docstring is the layer's entry point, and #2958 was five \
                 modules — including every ruleset builder — invisible from \
                 it while re-exported eleven lines below."
            );
        }
    }

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
