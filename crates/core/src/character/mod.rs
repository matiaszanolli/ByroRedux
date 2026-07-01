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
//! * [`leveling`] — [`LevelingModel`] + [`LevelReward`]: the XP curve and
//!   per-level reward (FO4 SPECIAL-or-perk vs FO3/FNV skill points).
//! * [`ruleset`] — [`CharacterRuleset`], the per-game `Resource` bundling the
//!   derived-formula table + leveling model.
//! * [`reputation`] — the reputation family: [`KarmaBand`] (FO3/FNV Karma) +
//!   [`ReputationStanding`] (FNV Fame/Infamy 4×4 grid) classifiers.
//! * [`resistance`] — the affliction family: [`Affliction`] descriptors
//!   (radiation / poison resistance derivation) + the damage-multiplier model.
//! * [`components`] — [`CharacterLevel`] / [`Perks`] / [`Background`], the
//!   structural per-actor ECS components.
//!
//! Per-game **population** lives at the parser boundary (FO4 `PRPS`/`DNAM`,
//! FNV/FO3 class auto-calc) in `byroredux_plugin`; this crate holds the
//! game-agnostic canonical types those boundaries feed.

pub mod attribute;
pub mod components;
pub mod derived;
pub mod fallout;
pub mod leveling;
pub mod reputation;
pub mod resistance;
pub mod ruleset;
pub mod skill;

pub use attribute::{Attribute, AttributeSet};
pub use components::{
    Background, CharacterLevel, FactionReputation, FactionStanding, PerkRank, Perks,
};
pub use derived::{DerivedInput, DerivedOutput, DerivedScope, DerivedStatFormula, RoundMode};
pub use fallout::{fallout3_ruleset, fallout4_ruleset, falloutnv_ruleset};
pub use leveling::{LevelReward, LevelingModel};
pub use reputation::{
    clamp_karma, karma_band, reputation_bump_points, FactionRepThresholds, KarmaBand,
    ReputationSentiment, ReputationStanding, REPUTATION_BUMP_POINTS,
};
pub use resistance::{damage_multiplier, Affliction};
pub use ruleset::CharacterRuleset;
pub use skill::{ResolvedSkill, SkillDef, SkillSet};
