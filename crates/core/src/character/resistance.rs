//! Resistance / affliction family (CHARAL).
//!
//! Environmental afflictions — **radiation**, **poison** — share one shape: a
//! **resistance actor value** that cuts incoming affliction damage, plus a
//! damage *pool* and a threshold band that debuffs SPECIAL via
//! `temporary_mod`. This module owns the **resistance half**: the
//! per-affliction [`Affliction`] descriptor (its resistance AV + how that AV
//! derives in FO3/FNV) and the percentage damage-reduction interpretation
//! [`damage_multiplier`]. [`super::affliction`] owns the **pool/threshold
//! half** (the mechanism); the per-game **threshold numbers** themselves are
//! still PENDING (no citable source yet — see that module's docs).
//!
//! It mirrors [`super::reputation`] — the other "actor value + classifier"
//! family. There the AV is Karma/Fame and the classifier is a *band*; here the
//! AV is a resistance percentage and the classifier is a *damage multiplier*.
//! Both modules interpret a stored AV into a derived meaning; neither applies
//! the effect (combat / dialogue does that), keeping CHARAL state-producing.
//!
//! ## Per-game shape
//!
//! * **FO3 / FNV** — resistance is a *derived percentage* `(governing − 1)·k`,
//!   optionally capped; incoming affliction damage is cut by that percentage.
//!   [`Affliction`] holds `k` + the cap and builds the
//!   [`DerivedStatFormula`](super::derived::DerivedStatFormula); the per-game
//!   ruleset ([`super::fallout`]) pushes it like any other derived stat, so
//!   the coefficients live **here only** (single source).
//! * **FO4** — resistance is a *flat additive AV* (not Endurance-derived) cut
//!   by the shared FO4 non-linear DR/ER curve, whose closed form isn't sourced
//!   yet (the wiki gives empirical sample tables only). That curve is
//!   deliberately **not** modelled here (no-guessing). FO4 resistance is just a
//!   plain AV until the closed form is sourced.
//! * **Skyrim — a genuinely different shape, NOT this family's mechanism.**
//!   Disease is **not** a pool/threshold system at all: each disease is a
//!   discrete binary status (present or absent) carrying a *fixed* flat
//!   percentage penalty (e.g. "picking locks 25% harder"), not an
//!   accumulating damage pool crossing bands. Survival Mode adds a 3-rung
//!   escalation ladder (Normal → Severe → Crippling after 24h untreated) with
//!   fixed per-rung percentages — a discrete state machine gated by elapsed
//!   time, not [`super::affliction`]'s continuous pool/threshold diff.
//!   Resistance is a flat immunity percentage (Argonian/Bosmer 50 %,
//!   werewolf/vampire 100 %), not an Endurance-derived formula. **Do not
//!   reuse [`super::affliction::AfflictionTable`] for Skyrim disease** without
//!   redesigning it — the mechanisms don't match. Source: UESP
//!   *Skyrim:Disease*, `charal-skyrim-ruleset.md`.

use super::derived::{DerivedInput, DerivedStatFormula};

/// One environmental affliction's **resistance** descriptor: which actor value
/// holds the resistance, and how that value derives from a SPECIAL attribute in
/// FO3/FNV. 24 bytes, `Copy`; the `&'static str` EditorIDs are resolved to
/// AVIF FormIDs at load time (resolve-or-skip), like every other CHARAL AV ref.
/// 40 bytes (two `&'static str` fat pointers + two `f32`), `Copy`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affliction {
    /// Resistance AV EditorID (e.g. `"RadResist"`).
    pub resist_editor_id: &'static str,
    /// Governing attribute EditorID for the FO3/FNV derived % (`"Endurance"`).
    pub governing_editor_id: &'static str,
    /// Coefficient `k` of the FO3/FNV derived percentage `(governing − 1)·k`.
    pub derive_coeff: f32,
    /// Upper percentage clamp (`f32::INFINITY` = uncapped — FO3/FNV Poison
    /// Resistance has no documented cap).
    pub resist_cap: f32,
}

impl Affliction {
    /// Radiation: `RadResist = (END − 1)·2`, capped 85 %. Source: fandom
    /// *Radiation Resistance*.
    pub const RADIATION: Affliction = Affliction {
        resist_editor_id: "RadResist",
        governing_editor_id: "Endurance",
        derive_coeff: 2.0,
        resist_cap: 85.0,
    };

    /// Poison: `PoisonResist = (END − 1)·5`, uncapped (FO3/FNV hidden stat).
    /// Source: fandom *Poison Resistance*.
    pub const POISON: Affliction = Affliction {
        resist_editor_id: "PoisonResist",
        governing_editor_id: "Endurance",
        derive_coeff: 5.0,
        resist_cap: f32::INFINITY,
    };

    /// Every scaffolded affliction (resistance half complete). The per-game
    /// ruleset iterates this to populate FO3/FNV derived resistances.
    pub const ALL: [Affliction; 2] = [Self::RADIATION, Self::POISON];

    /// The FO3/FNV derived-percentage formula `(governing − 1)·k = k·gov − k`,
    /// clamped to the cap. `governing_av` is the resolved governing AVIF
    /// FormID. This is the *only* place the `(gov − 1)·k` shape is encoded.
    pub fn fo3_fnv_resistance_formula(&self, governing_av: u32) -> DerivedStatFormula {
        DerivedStatFormula::affine(
            DerivedInput::actor_value(governing_av),
            self.derive_coeff,
            -self.derive_coeff,
        )
        .capped(self.resist_cap)
    }
}

/// The fraction of incoming affliction damage that survives a percentage
/// resistance — the FO3/FNV model ("damage is reduced by this percentage").
///
/// `resist_pct` is clamped to `[0, cap_pct]`, then the survivor fraction is
/// `(1 − resist/100)`, floored at `0` (resistance ≥ 100 % = immunity, never a
/// heal). A combat consumer multiplies incoming damage by this; CHARAL only
/// interprets the AV — it does not apply the damage.
#[inline]
pub fn damage_multiplier(resist_pct: f32, cap_pct: f32) -> f32 {
    let r = resist_pct.clamp(0.0, cap_pct);
    (1.0 - r / 100.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::ActorValues;

    const END: u32 = 0x07;

    fn avs(end: f32) -> ActorValues {
        ActorValues::from_pairs([(END, end)])
    }

    #[test]
    fn radiation_formula_matches_wiki_and_caps() {
        let f = Affliction::RADIATION.fo3_fnv_resistance_formula(END);
        // (END − 1)·2: END 5 → 8 %.
        assert_eq!(f.eval(&avs(5.0), 1), 8.0);
        // Capped at 85 %: (50 − 1)·2 = 98 → 85.
        assert_eq!(f.eval(&avs(50.0), 1), 85.0);
    }

    #[test]
    fn poison_formula_matches_wiki_uncapped() {
        let f = Affliction::POISON.fo3_fnv_resistance_formula(END);
        // (END − 1)·5: END 5 → 20 %.
        assert_eq!(f.eval(&avs(5.0), 1), 20.0);
        // Uncapped — keeps scaling: (50 − 1)·5 = 245.
        assert_eq!(f.eval(&avs(50.0), 1), 245.0);
    }

    #[test]
    fn damage_multiplier_cuts_and_clamps() {
        // 20 % poison resistance → 80 % of the damage gets through.
        assert!((damage_multiplier(20.0, f32::INFINITY) - 0.8).abs() < 1e-6);
        // No resistance → full damage.
        assert_eq!(damage_multiplier(0.0, 85.0), 1.0);
        // The 85 % rad cap floors the multiplier at 0.15, even past the cap.
        assert!((damage_multiplier(85.0, 85.0) - 0.15).abs() < 1e-6);
        assert!((damage_multiplier(200.0, 85.0) - 0.15).abs() < 1e-6, "over-cap clamps to cap");
        // ≥100 % uncapped resistance → immunity, never a negative (heal).
        assert_eq!(damage_multiplier(120.0, f32::INFINITY), 0.0);
    }

    #[test]
    fn descriptors_are_copy_and_compact() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<Affliction>();
        // Two `&'static str` fat pointers (16 each) + two f32 = 40 bytes.
        assert_eq!(std::mem::size_of::<Affliction>(), 40);
        assert_eq!(Affliction::ALL.len(), 2);
    }
}
