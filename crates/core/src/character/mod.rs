//! CHARAL — the canonical character / progression system.
//!
//! The game-agnostic tier that per-game character rulesets translate into:
//! attributes, skills, perks, level, and derived stats, all resolved so the
//! gameplay runtime reads one representation regardless of source game. See
//! the design doc `docs/engine/charal.md` and the per-game data captures
//! `docs/engine/charal-fo4-ruleset.md` / `charal-fnv-fo3-ruleset.md`.
//!
//! The numeric substrate is [`crate::ecs::components::ActorValues`] (shipped
//! with #1663). This module adds the *rules* layered over it:
//!
//! * [`derived`] — [`DerivedStatFormula`], the fixed-layout bilinear formula
//!   every Bethesda derived stat (Health, AP, Carry Weight, …) reduces to.
//!
//! Still to land (per CHARAL §8): `CharacterRuleset` (the per-game formula +
//! leveling tables), `LevelingModel`, and the `CharacterLevel` / `Perks` /
//! `Background` components.

pub mod derived;

pub use derived::{DerivedInput, DerivedOutput, DerivedStatFormula, RoundMode};
