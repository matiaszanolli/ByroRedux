//! Derived actor-value formulas (CHARAL).
//!
//! Bethesda derived stats — Health, Action Points, Carry Weight, Melee
//! Damage, Critical Chance, Unarmed Damage, XP multiplier — are all
//! computed from a small bilinear expression over **at most two** inputs
//! (a SPECIAL attribute or skill actor value, or the character level):
//!
//! ```text
//! output = round( bias + cₐ·A + c_b·B + cross·A·B )   then clamped to a cap
//! ```
//!
//! Every formula captured across FO3 / FNV / FO4 (see
//! `docs/engine/charal-fo4-ruleset.md` + `charal-fnv-fo3-ruleset.md`) fits
//! this one shape — the most complex is FO4 Health
//! (`floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)`), which needs the cross
//! term; the rest are affine. So a single fixed-layout [`DerivedStatFormula`]
//! covers the whole derived-stat surface with **no expression tree, no heap,
//! no per-game branching** — the per-game seam is data (the coefficients),
//! not code.
//!
//! ## Efficiency
//!
//! [`DerivedStatFormula`] is `Copy` and 32 bytes (half a cache line); a
//! per-game [`super::CharacterRuleset`] holds a flat `Vec` of them. [`eval`]
//! is ~5 FMAs + one branch + one `min` — no allocation, no virtual dispatch.
//!
//! ## Chaining
//!
//! An input is identified by **AVIF FormID**, so a derived stat can read
//! *another* actor value — a skill, not just an attribute (FNV Unarmed
//! Damage ← Unarmed skill ← SPECIAL). The deriver must therefore populate
//! base attributes + skills into [`ActorValues`] **before** evaluating
//! derived formulas that depend on them; chaining is resolved by that
//! ordering, not by the formula type.
//!
//! [`eval`]: DerivedStatFormula::eval

use crate::ecs::components::ActorValues;

/// One input to a [`DerivedStatFormula`], packed into 4 bytes.
///
/// A bare `u32` with two reserved sentinels, so a two-input formula stays
/// `Copy` and cache-tight (no enum tag/padding):
/// * `0` — **unused** (the FormID `0` is the null form, never a real AV).
/// * `u32::MAX` — the character **level** (never a plausible FormID).
/// * anything else — an actor value by its **global-space AVIF FormID**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedInput(u32);

impl DerivedInput {
    /// Contributes nothing (coefficient is multiplied by `0.0`).
    pub const UNUSED: Self = Self(0);
    /// Reads the character level rather than an actor value.
    pub const LEVEL: Self = Self(u32::MAX);

    /// An actor value by global-space AVIF FormID. (Caller guarantees the
    /// id is neither `0` nor `u32::MAX` — real Bethesda FormIDs never are.)
    pub const fn actor_value(avif_form_id: u32) -> Self {
        Self(avif_form_id)
    }

    /// Resolve to a numeric value against the actor's state.
    #[inline]
    fn read(self, avs: &ActorValues, level: u16) -> f32 {
        match self.0 {
            0 => 0.0,
            u32::MAX => f32::from(level),
            avif => avs.current(avif),
        }
    }
}

/// Rounding applied to a formula's raw value before the cap clamp. Bethesda
/// floors Health (`TotalHitPoints = floor(...)`) and ceils Unarmed Damage
/// (`ceil((10 + Unarmed)/20)`); most stats are exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RoundMode {
    /// No rounding (the value is used as-is).
    #[default]
    None,
    /// `floor` — e.g. Health.
    Floor,
    /// `ceil` — e.g. Unarmed Damage.
    Ceil,
}

/// How the consumer applies a formula's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum DerivedOutput {
    /// The result **is** the actor value (Health, AP, Carry Weight …).
    #[default]
    Absolute,
    /// The result is a **multiplier** applied at use against a base — e.g.
    /// FO4 Melee Damage `×(1 + STR/10)`, the XP multiplier `×(1 + 0.03·INT)`.
    /// `eval` returns the multiplier; the combat / XP system multiplies.
    Multiplier,
}

/// Which actors a formula applies to.
///
/// `fAVD`-prefixed stats (Carry Weight, Melee Damage) derive identically for
/// every actor; Health and Action Points are flagged "player only" by the
/// wiki — NPCs ship *baked* values (FO4 `DNAM`) or derive them on a different
/// path, so the player formula must **not** be applied to them. A consumer
/// that computes a derived stat for an arbitrary entity checks this before
/// trusting the result. (Fits in `DerivedStatFormula`'s existing padding —
/// the struct stays 32 bytes.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum DerivedScope {
    /// Applies to any actor.
    #[default]
    ActorGeneral,
    /// Applies only to the player character.
    PlayerOnly,
}

/// A per-game derived-stat formula: `round(bias + cₐ·A + c_b·B + cross·A·B)`
/// clamped to `cap`. Fixed 32-byte `Copy` layout — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DerivedStatFormula {
    /// Constant term.
    pub bias: f32,
    /// First input and its coefficient.
    pub a: DerivedInput,
    pub coeff_a: f32,
    /// Second input and its coefficient (`UNUSED` for single-input stats).
    pub b: DerivedInput,
    pub coeff_b: f32,
    /// Coefficient of the `A·B` cross term (`0.0` when absent). Only
    /// FO4 Health uses it (`0.5·L·END`).
    pub cross: f32,
    /// Upper clamp (`f32::INFINITY` = uncapped). FO3 AP 85, FNV AP 95,
    /// Critical Chance 0.10, FO4 VATS 0.95.
    pub cap: f32,
    /// Rounding before the cap.
    pub round: RoundMode,
    /// Absolute value vs multiplier.
    pub kind: DerivedOutput,
    /// Player-only vs actor-general (see [`DerivedScope`]). Free — fits in
    /// the struct's alignment padding.
    pub scope: DerivedScope,
}

impl DerivedStatFormula {
    /// `bias + coeff·input` — the common single-input affine stat
    /// (AP, Carry Weight, Critical Chance, Melee Damage …).
    pub const fn affine(input: DerivedInput, coeff: f32, bias: f32) -> Self {
        Self {
            bias,
            a: input,
            coeff_a: coeff,
            b: DerivedInput::UNUSED,
            coeff_b: 0.0,
            cross: 0.0,
            cap: f32::INFINITY,
            round: RoundMode::None,
            kind: DerivedOutput::Absolute,
            scope: DerivedScope::ActorGeneral,
        }
    }

    /// `bias + coeff_a·a + coeff_b·b + cross·a·b` — the two-input form
    /// (FO3/FNV/FO4 Health off Endurance + level).
    pub const fn bilinear(
        a: DerivedInput,
        coeff_a: f32,
        b: DerivedInput,
        coeff_b: f32,
        cross: f32,
        bias: f32,
    ) -> Self {
        Self {
            bias,
            a,
            coeff_a,
            b,
            coeff_b,
            cross,
            cap: f32::INFINITY,
            round: RoundMode::None,
            kind: DerivedOutput::Absolute,
            scope: DerivedScope::ActorGeneral,
        }
    }

    /// Set the upper clamp (chainable).
    pub const fn capped(mut self, cap: f32) -> Self {
        self.cap = cap;
        self
    }

    /// Floor the result before clamping (chainable) — Health.
    pub const fn floored(mut self) -> Self {
        self.round = RoundMode::Floor;
        self
    }

    /// Ceil the result before clamping (chainable) — Unarmed Damage.
    pub const fn ceiled(mut self) -> Self {
        self.round = RoundMode::Ceil;
        self
    }

    /// Mark the output a multiplier (chainable) — Melee Damage, XP mult.
    pub const fn as_multiplier(mut self) -> Self {
        self.kind = DerivedOutput::Multiplier;
        self
    }

    /// Mark the formula player-only (chainable) — Health, Action Points
    /// (NPCs ship baked values / derive differently).
    pub const fn player_only(mut self) -> Self {
        self.scope = DerivedScope::PlayerOnly;
        self
    }

    /// Evaluate against an actor's [`ActorValues`] + level. Reads each input
    /// (AV by FormID, or the level), folds the bilinear expression, rounds,
    /// then clamps to `cap`. Inputs absent from `avs` read `0.0` (the
    /// Bethesda absent-AV default), so a partially-populated actor degrades
    /// gracefully rather than panicking.
    #[inline]
    pub fn eval(&self, avs: &ActorValues, level: u16) -> f32 {
        let a = self.a.read(avs, level);
        let b = self.b.read(avs, level);
        let raw = self.bias + self.coeff_a * a + self.coeff_b * b + self.cross * a * b;
        let rounded = match self.round {
            RoundMode::None => raw,
            RoundMode::Floor => raw.floor(),
            RoundMode::Ceil => raw.ceil(),
        };
        rounded.min(self.cap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Stand-in global-space AVIF FormIDs for the SPECIAL + a skill. The
    // formula type is agnostic to their values — real ids come from the
    // parsed AVIF set.
    const STR: u32 = 0x05;
    const END: u32 = 0x07;
    const AGI: u32 = 0x0A;
    const LUCK: u32 = 0x0B;
    const UNARMED: u32 = 0x2C;

    fn av(input: u32) -> DerivedInput {
        DerivedInput::actor_value(input)
    }

    /// Build an `ActorValues` from `(avif, base)` pairs.
    fn avs(pairs: &[(u32, f32)]) -> ActorValues {
        ActorValues::from_pairs(pairs.iter().copied())
    }

    #[test]
    fn formula_is_thirty_two_bytes_and_copy() {
        // Efficiency guard: fixed layout, half a cache line, no heap — the
        // round/kind/scope u8 flags ride the alignment padding, so adding
        // `scope` kept it at 32 bytes.
        assert_eq!(std::mem::size_of::<DerivedStatFormula>(), 32);
        fn assert_copy<T: Copy>() {}
        assert_copy::<DerivedStatFormula>();
        // Scope defaults to actor-general; `player_only` flips it.
        let f = DerivedStatFormula::affine(DerivedInput::LEVEL, 1.0, 0.0);
        assert_eq!(f.scope, DerivedScope::ActorGeneral);
        assert_eq!(f.player_only().scope, DerivedScope::PlayerOnly);
    }

    #[test]
    fn fo4_health_bilinear_with_floor_matches_wiki() {
        // floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END); END 2, L 2 → 93.5 → 93.
        let f = DerivedStatFormula::bilinear(av(END), 4.5, DerivedInput::LEVEL, 2.5, 0.5, 77.5)
            .floored();
        assert_eq!(f.eval(&avs(&[(END, 2.0)]), 2), 93.0);
        // END 5, L 1 → floor(77.5+22.5+2.5+2.5)=floor(105)=105.
        assert_eq!(f.eval(&avs(&[(END, 5.0)]), 1), 105.0);
    }

    #[test]
    fn fnv_and_fo3_health_match_wiki() {
        // FNV: 100 + 20·END + 5·(L−1) = 95 + 20·END + 5·L. END 5, L 1 → 200.
        let fnv = DerivedStatFormula::bilinear(av(END), 20.0, DerivedInput::LEVEL, 5.0, 0.0, 95.0);
        assert_eq!(fnv.eval(&avs(&[(END, 5.0)]), 1), 200.0);
        assert_eq!(fnv.eval(&avs(&[(END, 10.0)]), 30), 445.0); // 95+200+150
        // FO3: 90 + 20·END + 10·L. END 5, L 1 → 200.
        let fo3 = DerivedStatFormula::bilinear(av(END), 20.0, DerivedInput::LEVEL, 10.0, 0.0, 90.0);
        assert_eq!(fo3.eval(&avs(&[(END, 5.0)]), 1), 200.0);
    }

    #[test]
    fn action_points_affine_with_cap() {
        // FO4: 60 + 10·AGI (uncapped). AGI 5 → 110.
        let fo4 = DerivedStatFormula::affine(av(AGI), 10.0, 60.0);
        assert_eq!(fo4.eval(&avs(&[(AGI, 5.0)]), 1), 110.0);
        // FO3: 65 + 2·AGI, cap 85. AGI 5 → 75; AGI 20 → 105 → clamped 85.
        let fo3 = DerivedStatFormula::affine(av(AGI), 2.0, 65.0).capped(85.0);
        assert_eq!(fo3.eval(&avs(&[(AGI, 5.0)]), 1), 75.0);
        assert_eq!(fo3.eval(&avs(&[(AGI, 20.0)]), 1), 85.0, "cap clamps");
        // FNV: 65 + 3·AGI, cap 95. AGI 5 → 80.
        let fnv = DerivedStatFormula::affine(av(AGI), 3.0, 65.0).capped(95.0);
        assert_eq!(fnv.eval(&avs(&[(AGI, 5.0)]), 1), 80.0);
    }

    #[test]
    fn carry_weight_affine() {
        // FO3/FNV: 150 + 10·STR; FO4: 200 + 10·STR.
        let cw_fo3 = DerivedStatFormula::affine(av(STR), 10.0, 150.0);
        let cw_fo4 = DerivedStatFormula::affine(av(STR), 10.0, 200.0);
        assert_eq!(cw_fo3.eval(&avs(&[(STR, 6.0)]), 1), 210.0);
        assert_eq!(cw_fo4.eval(&avs(&[(STR, 6.0)]), 1), 260.0);
    }

    #[test]
    fn melee_damage_additive_vs_multiplier() {
        // FO3/FNV Melee Damage: 0.5·STR (additive). STR 5 → 2.5.
        let fo3 = DerivedStatFormula::affine(av(STR), 0.5, 0.0);
        assert_eq!(fo3.eval(&avs(&[(STR, 5.0)]), 1), 2.5);
        assert_eq!(fo3.kind, DerivedOutput::Absolute);
        // FO4 Melee Damage: ×(1 + 0.1·STR) (multiplier). STR 10 → 2.0×.
        let fo4 = DerivedStatFormula::affine(av(STR), 0.1, 1.0).as_multiplier();
        assert_eq!(fo4.eval(&avs(&[(STR, 10.0)]), 1), 2.0);
        assert_eq!(fo4.kind, DerivedOutput::Multiplier);
    }

    #[test]
    fn critical_chance_capped_and_xp_multiplier() {
        // FO3/FNV Critical Chance: 0.01·Luck, cap 0.10. Luck 5 → 0.05; 15 → 0.10.
        let crit = DerivedStatFormula::affine(av(LUCK), 0.01, 0.0).capped(0.10);
        assert!((crit.eval(&avs(&[(LUCK, 5.0)]), 1) - 0.05).abs() < 1e-6);
        assert!((crit.eval(&avs(&[(LUCK, 15.0)]), 1) - 0.10).abs() < 1e-6, "cap");
        // FO4 XP multiplier: ×(1 + 0.03·INT). INT 10 → 1.30×.
        let xp = DerivedStatFormula::affine(av(0x09), 0.03, 1.0).as_multiplier();
        assert!((xp.eval(&avs(&[(0x09, 10.0)]), 1) - 1.30).abs() < 1e-6);
    }

    #[test]
    fn unarmed_damage_ceils_off_a_skill_av() {
        // FO3/FNV Unarmed Damage: ceil((10 + Unarmed)/20) = ceil(0.5 + 0.05·U).
        // Governed by the Unarmed *skill* AV — proves chaining off a non-attribute.
        let f = DerivedStatFormula::affine(av(UNARMED), 0.05, 0.5).ceiled();
        assert_eq!(f.eval(&avs(&[(UNARMED, 90.0)]), 1), 5.0);
        assert_eq!(f.eval(&avs(&[(UNARMED, 100.0)]), 1), 6.0);
        assert_eq!(f.eval(&avs(&[(UNARMED, 0.0)]), 1), 1.0, "ceil(0.5) = 1");
    }

    #[test]
    fn absent_input_reads_zero() {
        // An actor missing the input AV degrades to the bias, not a panic.
        let f = DerivedStatFormula::affine(av(STR), 10.0, 200.0);
        assert_eq!(f.eval(&ActorValues::new(), 1), 200.0);
    }
}
