//! Pool regeneration (CHARAL) — Fatigue/Magicka per-second rates, driven by a
//! **fixed 60 Hz tick** decoupled from the variable per-frame rate.
//!
//! ## Why fixed-timestep
//!
//! Every regen rate sourced so far (`docs/engine/charal-oblivion-ruleset.md`)
//! is expressed as points **per real-time second**, not per-frame — applying
//! `rate × frame_dt` directly would make regen frame-rate-dependent (a 30 fps
//! run and a 144 fps run would heal at different *simulated* rates due to
//! floating-point step-size differences accumulating differently). The fix is
//! the same one [`crates/physics`] already uses for its own fixed-step
//! simulation: accumulate the variable frame `dt` into a resource and drain
//! it in fixed `POOL_REGEN_DT` (1/60 s) increments, with a substep cap so a
//! hitch or a load stall can't dump minutes of regen into a single frame.
//! This is the **first CHARAL system** to need this — every other CHARAL
//! consumer (`derived_value`, `affliction_tick_system`) is stateless /
//! evaluated on demand, not time-integrated.
//!
//! ## Sourced formulas (classic Oblivion, 2006 Gamebryo — Remastered's
//! diverging regen math is out of scope, matching
//! [`super::tes::oblivion_health_gain_per_level`])
//!
//! * **Fatigue**: flat `10`/sec — `fFatigueReturnBase=10.0`,
//!   `fFatigureReturnMult=0.0` (the Endurance term ships at coefficient zero
//!   in vanilla).
//! * **Health**: **no passive regen at all** — deliberately not modeled here.
//! * **Magicka**: `(Willpower × 0.02 + 0.75) × (MaxMagicka / 100)`. Stunted
//!   Magicka (Atronach birthsign / *Astral Vapors* disease / one Shivering
//!   Isles item) zeroes this entirely, but no status-effect component exists
//!   yet to carry that flag anywhere in the engine — [`magicka_regen_per_sec`]
//!   accepts a `stunted` parameter so the *formula* is complete, but
//!   [`pool_regen_tick_system`] always passes `false` today. Revisit once a
//!   status-effect component exists (same "mechanism ahead of its data"
//!   deferral already used for `affliction`'s real threshold tables).

use super::ruleset::CharacterRuleset;
use crate::ecs::components::ActorValues;
use crate::ecs::resource::Resource;
use crate::ecs::world::World;

/// Fixed regen tick — 60 Hz. Matches `crates/physics::PHYSICS_DT`, the only
/// other fixed-timestep precedent in the engine.
pub const POOL_REGEN_DT: f32 = 1.0 / 60.0;

/// Spiral-of-death guard: caps how many 60 Hz ticks a single frame can catch
/// up on (a load/hitch shouldn't dump a large batch of regen at once).
/// Mirrors `crates/physics::MAX_SUBSTEPS`.
const MAX_REGEN_SUBSTEPS: u32 = 8;

/// Fatigue's flat per-second regen rate (`fFatigueReturnBase`) — vanilla
/// Oblivion's Endurance coefficient (`fFatigueReturnMult`) is `0.0`, so this
/// is the whole formula, not an approximation.
pub const FATIGUE_REGEN_PER_SEC: f32 = 10.0;

/// Magicka regen's Willpower coefficient (`fMagickaReturnMult`).
pub const MAGICKA_REGEN_WILLPOWER_COEFF: f32 = 0.02;
/// Magicka regen's bias term (`fMagickaReturnBase`).
pub const MAGICKA_REGEN_BASE: f32 = 0.75;

/// `(Willpower × 0.02 + 0.75) × (MaxMagicka / 100)`, or `0.0` while
/// `stunted` (Stunted Magicka — see module docs for why this is always
/// `false` from the tick system today).
#[must_use]
pub fn magicka_regen_per_sec(willpower: f32, max_magicka: f32, stunted: bool) -> f32 {
    if stunted || max_magicka <= 0.0 {
        return 0.0;
    }
    (willpower * MAGICKA_REGEN_WILLPOWER_COEFF + MAGICKA_REGEN_BASE) * (max_magicka / 100.0)
}

/// Cross-frame accumulator driving the fixed 60 Hz regen tick — a `Resource`,
/// one global clock shared by every actor (mirrors `PhysicsWorld`'s single
/// accumulator field, not a per-entity one).
#[derive(Debug, Clone, Copy, Default)]
pub struct PoolRegenAccumulator {
    accumulator: f32,
}

impl Resource for PoolRegenAccumulator {}

impl PoolRegenAccumulator {
    /// Advance by the frame's variable `dt`, returning how many complete
    /// `POOL_REGEN_DT` ticks have now elapsed (0 on a very short frame).
    /// Clamps the internal accumulator to `MAX_REGEN_SUBSTEPS ×
    /// POOL_REGEN_DT` first, so a hitch/load stall can't produce an
    /// unbounded catch-up.
    pub fn advance(&mut self, frame_dt: f32) -> u32 {
        self.accumulator += frame_dt.max(0.0);
        let max_acc = MAX_REGEN_SUBSTEPS as f32 * POOL_REGEN_DT;
        if self.accumulator > max_acc {
            self.accumulator = max_acc;
        }
        let ticks = (self.accumulator / POOL_REGEN_DT).floor() as u32;
        self.accumulator -= ticks as f32 * POOL_REGEN_DT;
        ticks
    }
}

/// Per-game resolved AVIF ids the regen tick needs — a `Resource`, built once
/// per load by a per-game constructor (e.g.
/// [`super::tes::oblivion_pool_regen_config`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolRegenConfig {
    pub fatigue_avif: u32,
    pub magicka_avif: u32,
    pub willpower_avif: u32,
}

impl Resource for PoolRegenConfig {}

/// The fixed-60Hz regen tick. Registered as an ordinary variable-`dt` system
/// (same signature every scheduler system uses); internally drains
/// [`PoolRegenAccumulator`] and applies `rate × elapsed` to every actor that
/// carries the configured pools — actors missing [`PoolRegenConfig`]'s AVIF
/// ids (a different game, or an entity that isn't a full actor) are simply
/// skipped, not force-populated with a zero entry.
///
/// A no-op if [`PoolRegenConfig`] hasn't been inserted (no game loaded yet /
/// not wired for this game) or if less than one 60 Hz tick has elapsed.
pub fn pool_regen_tick_system(world: &World, frame_dt: f32) {
    let Some(config) = world.try_resource::<PoolRegenConfig>() else {
        return;
    };
    let Some(mut accumulator) = world.try_resource_mut::<PoolRegenAccumulator>() else {
        return;
    };
    let ticks = accumulator.advance(frame_dt);
    drop(accumulator);
    if ticks == 0 {
        return;
    }
    let elapsed = ticks as f32 * POOL_REGEN_DT;

    let Some(ruleset) = world.try_resource::<CharacterRuleset>() else {
        return;
    };
    let Some(mut avs_q) = world.query_mut::<ActorValues>() else {
        return;
    };
    for (_entity, avs) in avs_q.iter_mut() {
        if avs.get(config.fatigue_avif).is_some() {
            avs.restore(config.fatigue_avif, FATIGUE_REGEN_PER_SEC * elapsed);
        }
        if avs.get(config.magicka_avif).is_some() {
            let willpower = avs.current(config.willpower_avif);
            // Level doesn't gate Magicka's formula (`2×Intelligence`), so any
            // fixed level value here is correct.
            let max_magicka = ruleset
                .derived_value(config.magicka_avif, avs, 1)
                .unwrap_or(0.0);
            let rate = magicka_regen_per_sec(willpower, max_magicka, false);
            avs.restore(config.magicka_avif, rate * elapsed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magicka_regen_matches_formula_and_gates_on_stunted() {
        // Willpower 30, MaxMagicka 100 → (30×0.02+0.75)×1 = 1.35/sec.
        assert!((magicka_regen_per_sec(30.0, 100.0, false) - 1.35).abs() < 1e-6);
        // Same inputs, Stunted Magicka active → 0.
        assert_eq!(magicka_regen_per_sec(30.0, 100.0, true), 0.0);
        // No Magicka pool at all → 0, not a divide-by-zero artifact.
        assert_eq!(magicka_regen_per_sec(30.0, 0.0, false), 0.0);
    }

    #[test]
    fn accumulator_ticks_at_sixty_hz_and_caps_catchup() {
        let mut acc = PoolRegenAccumulator::default();
        // A single 1/60s frame yields exactly one tick, no leftover.
        assert_eq!(acc.advance(POOL_REGEN_DT), 1);
        assert_eq!(acc.advance(0.0), 0, "no time passed, no extra tick");
        // A big hitch (10 simulated seconds) is capped at MAX_REGEN_SUBSTEPS.
        let mut acc = PoolRegenAccumulator::default();
        assert_eq!(acc.advance(10.0), MAX_REGEN_SUBSTEPS);
        // The capped remainder doesn't roll over into extra ticks next frame.
        assert_eq!(acc.advance(0.0), 0);
    }

    #[test]
    fn accumulator_carries_fractional_time_across_frames() {
        let mut acc = PoolRegenAccumulator::default();
        // Two half-tick frames should yield one tick total, not zero.
        assert_eq!(acc.advance(POOL_REGEN_DT / 2.0), 0);
        assert_eq!(acc.advance(POOL_REGEN_DT / 2.0), 1);
    }

    #[test]
    fn tick_system_applies_fatigue_and_magicka_regen() {
        use crate::character::attribute::AttributeSet;
        use crate::character::skill::SkillSet;
        use crate::character::{DerivedInput, DerivedStatFormula, LevelingModel};
        use crate::ecs::world::World;

        const FATIGUE: u32 = 0x90;
        const MAGICKA: u32 = 0x91;
        const WILLPOWER: u32 = 0x92;
        const INTELLIGENCE: u32 = 0x93;

        let mut world = World::new();
        world.insert_resource(PoolRegenConfig {
            fatigue_avif: FATIGUE,
            magicka_avif: MAGICKA,
            willpower_avif: WILLPOWER,
        });
        world.insert_resource(PoolRegenAccumulator::default());

        let mut rs = CharacterRuleset::new(LevelingModel::OBLIVION)
            .with_attributes(AttributeSet::TES_CLASSIC)
            .with_skills(SkillSet::OBLIVION);
        // Magicka = 2×Intelligence, matching oblivion_magicka_formula.
        rs.push_derived(
            MAGICKA,
            DerivedStatFormula::affine(DerivedInput::actor_value(INTELLIGENCE), 2.0, 0.0),
        );
        world.insert_resource(rs);

        let mut avs = ActorValues::new();
        avs.set_base(FATIGUE, 50.0);
        avs.apply_damage(FATIGUE, 20.0); // current = 30
        avs.set_base(MAGICKA, 100.0);
        avs.apply_damage(MAGICKA, 50.0); // current = 50, MaxMagicka via formula = 2×40 = 80
        avs.set_base(WILLPOWER, 30.0);
        avs.set_base(INTELLIGENCE, 40.0);
        let entity = world.spawn();
        world.insert(entity, avs);

        // One full second (60 ticks) of regen.
        for _ in 0..60 {
            pool_regen_tick_system(&world, POOL_REGEN_DT);
        }

        let q = world.query::<ActorValues>().unwrap();
        let avs = q.get(entity).unwrap();
        // Fatigue: +10/sec for 1s = +10 damage restored → current 40.
        assert!((avs.current(FATIGUE) - 40.0).abs() < 1e-3);
        // Magicka: rate = (30×0.02+0.75)×(80/100) = 1.35×0.8 = 1.08/sec.
        let expected_magicka = 50.0 + 1.08;
        assert!(
            (avs.current(MAGICKA) - expected_magicka).abs() < 1e-2,
            "got {}",
            avs.current(MAGICKA)
        );
    }

    #[test]
    fn tick_system_is_a_noop_without_config() {
        let world = World::new();
        // No PoolRegenConfig inserted — must not panic.
        pool_regen_tick_system(&world, POOL_REGEN_DT);
    }
}
