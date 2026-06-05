//! Procedural per-frame light animation — Phase 17.
//!
//! Walks every entity with a `LightFlicker` companion + `LightSource` +
//! `Transform`, modulates the light's intensity + position from the
//! FNAM flicker / pulse parameters parsed off the LIGH record.
//! Skyrim's vanilla flicker bits map to four animation shapes:
//!
//! | Bit                  | Pattern    | Speed   | Notes
//! |----------------------|------------|---------|----------------------------
//! | `FLICKER`     (0x08) | hash noise | normal  | candles, torches
//! | `FLICKER_SLOW`(0x40) | hash noise | half    | dying flames, low oil
//! | `PULSE`       (0x80) | sine       | normal  | crystals, mage-lights
//! | `PULSE_SLOW`  (0x400)| sine       | half    | ambience set-pieces
//!
//! Intensity modulation rides on `LightSource.intensity` (the same
//! field the NIF `NiLightIntensityController` writes to). Both
//! authoring paths converge on one runtime field; the renderer
//! reads `color * dimmer * intensity` and doesn't need to know
//! which authored it. Position jitter rides on
//! `Transform.translation`, restored to `LightFlicker.base_translation`
//! before each frame's noise sample so amplitude doesn't accumulate.

// `Transform` is intentionally not imported: position jitter is disabled
// (see the note in `animate_lights_system`). Re-enabling it means adding
// back the `Transform` write pass and its import.
use byroredux_core::ecs::{
    EntityId, LightFlicker, LightSource, World, LIGHT_FLAG_FLICKER, LIGHT_FLAG_FLICKER_SLOW,
    LIGHT_FLAG_PULSE, LIGHT_FLAG_PULSE_SLOW,
};

/// Damping multiplier applied to the raw `intensity_amplitude`
/// before composing the final modulation. Skyrim authors candles
/// at `intensity_amplitude = 0.25` (±25% around the authored
/// intensity) and the values were tuned against Skyrim's
/// bake-and-vertex-light renderer; mapping them straight into our
/// HDR + tone-mapped pipeline reads as too aggressive — visible as
/// a noticeable brightness pulse rather than a candle's subtle
/// breathing. Phase 19.6 — halve the amplitude. If a future LIGH
/// record warrants its full authored swing, override per-light by
/// scaling `LightFlicker.intensity_amplitude` at spawn time
/// instead of touching this constant.
const FLICKER_INTENSITY_DAMPING: f32 = 0.5;

/// Cheap deterministic hash → `[-1.0, 1.0]`. Wang-style integer hash;
/// flicker is purely cosmetic so a real PRNG would be overkill.
fn hash_to_unit(seed: u32) -> f32 {
    let mut x = seed.wrapping_add(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    let unit = (x & 0x007F_FFFF) as f32 / 0x007F_FFFF as f32;
    unit * 2.0 - 1.0
}

/// Compute the procedural intensity multiplier for one flickering
/// light from its flags, FNAM parameters, and the global clock. Pure
/// and deterministic in `(entity, flags, flicker, total_time)` so it
/// is unit-testable without a `World` (PERF-D4-NEW-04 / #1380).
///
/// Returns the value written to `LightSource.intensity`: `1.0` when no
/// animation bit is set, otherwise `1 + modulation · amplitude · damping`.
fn flicker_intensity(
    entity: EntityId,
    flags: u32,
    flicker: &LightFlicker,
    total_time: f32,
) -> f32 {
    // Slow variants run at half rate by halving the angular velocity.
    // Cheaper than reading period at half the FNAM authored value
    // because some lights set both bits.
    let speed_scale = if flags & (LIGHT_FLAG_FLICKER_SLOW | LIGHT_FLAG_PULSE_SLOW) != 0 {
        0.5
    } else {
        1.0
    };

    // Intensity modulation. Two paths:
    //   * PULSE/PULSE_SLOW → sine wave at the LIGH's period.
    //   * FLICKER/FLICKER_SLOW → smooth-noise: interpolate linearly
    //     between two consecutive hash samples stepped at 12 Hz.
    //     Phase 17 stepped the hash at 24 Hz with no interpolation;
    //     visually that produced a jerky strobe rather than a candle's
    //     gentle dance, surfaced by the user reporting "shadows jump
    //     all over the place" in Phase 19 readings.
    let modulation = if flags & (LIGHT_FLAG_PULSE | LIGHT_FLAG_PULSE_SLOW) != 0 {
        let phase_secs = (total_time + flicker.phase_offset_secs).rem_euclid(flicker.period_secs);
        let phase = phase_secs / flicker.period_secs;
        (phase * speed_scale * std::f32::consts::TAU).sin()
    } else if flags & (LIGHT_FLAG_FLICKER | LIGHT_FLAG_FLICKER_SLOW) != 0 {
        let raw = (total_time + flicker.phase_offset_secs) * 12.0 * speed_scale;
        let bucket = raw.floor() as u32;
        let bucket_t = raw.fract();
        // Smoothstep on the lerp factor — Hermite curve hides the
        // still-visible cusps a pure linear lerp leaves at bucket
        // boundaries.
        let t = bucket_t * bucket_t * (3.0 - 2.0 * bucket_t);
        let entity_seed = entity.wrapping_mul(0x9E37_79B9);
        let n0 = hash_to_unit(entity_seed ^ bucket);
        let n1 = hash_to_unit(entity_seed ^ bucket.wrapping_add(1));
        n0 * (1.0 - t) + n1 * t
    } else {
        0.0
    };

    1.0 + modulation * flicker.intensity_amplitude * FLICKER_INTENSITY_DAMPING
}

/// Per-frame procedural light animation. Exclusive in Stage::Update.
///
/// A single `query_2_mut::<LightFlicker, LightSource>` holds the flicker
/// read + the light write together (distinct storages, TypeId-sorted
/// internally) and writes intensity in place — no intermediate
/// `Vec<LightUpdate>` allocation and no read-then-write lock cycling
/// (PERF-D4-NEW-04 / #1380). Exclusive in Stage::Update, so no other
/// writer competes for `LightSource`.
pub(crate) fn animate_lights_system(world: &World, _dt: f32) {
    let total_time = match world.try_resource::<byroredux_core::ecs::TotalTime>() {
        Some(t) => t.0,
        None => return,
    };

    let Some((flicker_q, mut light_q)) = world.query_2_mut::<LightFlicker, LightSource>() else {
        return;
    };
    for (entity, flicker) in flicker_q.iter() {
        let Some(light) = light_q.get_mut(entity) else {
            continue;
        };
        // Read flags + write intensity through the same `&mut LightSource`.
        // Intensity modulation rides the same field `NiLightIntensityController`
        // writes; the renderer reads `color · dimmer · intensity` and doesn't
        // care which authored it.
        light.intensity = flicker_intensity(entity, light.flags, flicker, total_time);
    }

    // Position jitter is DISABLED (Phase 19.5): pure-random hash noise at
    // any reasonable frequency teleports the light between uncorrelated
    // positions every bucket — the "shadows jumping all over the place"
    // the operator reported on Phase 19. Real candle flames move smoothly
    // by tiny amounts, which needs continuous noise (perlin/simplex), not
    // step-sampled hashes. `movement_amplitude` / `base_translation` stay
    // parsed on `LightFlicker`; re-enabling jitter means adding back a
    // separate `Transform` write pass here (kept separate so the live
    // intensity path above stays a two-storage query).
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flicker(amplitude: f32, period: f32) -> LightFlicker {
        LightFlicker {
            period_secs: period,
            intensity_amplitude: amplitude,
            movement_amplitude: 0.0,
            base_translation: [0.0; 3],
            phase_offset_secs: 0.0,
        }
    }

    #[test]
    fn no_animation_flag_holds_unit_intensity() {
        // No FLICKER/PULSE bit → modulation 0 → exactly 1.0.
        let f = flicker(0.25, 0.5);
        assert_eq!(flicker_intensity(1, 0, &f, 0.0), 1.0);
        assert_eq!(flicker_intensity(1, 0, &f, 3.7), 1.0);
    }

    #[test]
    fn pulse_is_sine_of_phase() {
        let f = flicker(0.4, 1.0); // amplitude 0.4, period 1 s
        // total_time 0 → phase 0 → sin(0) = 0 → unit.
        assert!((flicker_intensity(1, LIGHT_FLAG_PULSE, &f, 0.0) - 1.0).abs() < 1e-6);
        // total_time = period/4 → phase 0.25 → sin(TAU·0.25) = 1
        // → 1 + 1·0.4·0.5 = 1.2.
        let peak = flicker_intensity(1, LIGHT_FLAG_PULSE, &f, 0.25);
        assert!((peak - 1.2).abs() < 1e-6, "expected 1.2, got {peak}");
    }

    #[test]
    fn pulse_slow_runs_at_half_angular_velocity() {
        let f = flicker(0.4, 1.0);
        // At t = period/4: fast PULSE reaches sin(90°)=1 (peak),
        // PULSE_SLOW only sin(45°)=√2/2 — strictly less than the peak.
        let fast = flicker_intensity(1, LIGHT_FLAG_PULSE, &f, 0.25);
        let slow = flicker_intensity(1, LIGHT_FLAG_PULSE_SLOW, &f, 0.25);
        assert!((fast - 1.2).abs() < 1e-6);
        let expected_slow = 1.0 + std::f32::consts::FRAC_1_SQRT_2 * 0.4 * FLICKER_INTENSITY_DAMPING;
        assert!((slow - expected_slow).abs() < 1e-6, "slow={slow}");
        assert!(slow < fast);
    }

    #[test]
    fn flicker_is_deterministic_and_bounded() {
        let f = flicker(0.4, 0.5);
        let a = flicker_intensity(42, LIGHT_FLAG_FLICKER, &f, 1.234);
        let b = flicker_intensity(42, LIGHT_FLAG_FLICKER, &f, 1.234);
        assert_eq!(a, b, "same inputs must be deterministic");
        // modulation ∈ [-1, 1] → intensity ∈ [1 ± amplitude·damping].
        let half = 0.4 * FLICKER_INTENSITY_DAMPING;
        assert!(a >= 1.0 - half - 1e-6 && a <= 1.0 + half + 1e-6, "out of band: {a}");
        // Distinct entity seeds generally diverge → confirm the seed
        // actually feeds the hash (not a constant).
        let other = flicker_intensity(43, LIGHT_FLAG_FLICKER, &f, 1.234);
        assert_ne!(a, other, "entity seed must influence the noise");
    }
}
