//! Procedural per-frame light animation — Phase 17.
//!
//! Walks every entity with a `LightFlicker` companion + `LightSource`
//! + `Transform`, modulates the light's intensity + position from the
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

use byroredux_core::ecs::{
    EntityId, LightFlicker, LightSource, Transform, World, LIGHT_FLAG_FLICKER,
    LIGHT_FLAG_FLICKER_SLOW, LIGHT_FLAG_PULSE, LIGHT_FLAG_PULSE_SLOW,
};

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

/// One per-frame animation update for a flickering light.
struct LightUpdate {
    entity: EntityId,
    intensity: f32,
    /// `Some(xyz)` when the FNAM movement_amplitude is non-zero —
    /// the animator should restore Transform.translation to this
    /// jittered position. `None` for pure-intensity flicker / pulse.
    translation: Option<[f32; 3]>,
}

/// Per-frame procedural light animation. Exclusive in Stage::Update.
pub(crate) fn animate_lights_system(world: &World, _dt: f32) {
    let total_time = match world.try_resource::<byroredux_core::ecs::TotalTime>() {
        Some(t) => t.0,
        None => return,
    };

    // Pass 1: compute updates from (LightFlicker read, LightSource
    // read). Two separate read queries are fine — no exclusive lock
    // requirement; they can coexist on the same RwLock-per-storage
    // model. The system runs exclusive in Stage::Update so no other
    // writers are competing for Transform / LightSource here.
    let updates: Vec<LightUpdate> = {
        let Some(flicker_q) = world.query::<LightFlicker>() else {
            return;
        };
        let Some(light_q) = world.query::<LightSource>() else {
            return;
        };
        let mut buf: Vec<LightUpdate> = Vec::new();
        for (entity, flicker) in flicker_q.iter() {
            let Some(light) = light_q.get(entity) else {
                continue;
            };

            // Slow variants run at half rate by halving the
            // angular velocity. Cheaper than reading period at
            // half the FNAM authored value because some lights
            // set both bits.
            let speed_scale =
                if light.flags & (LIGHT_FLAG_FLICKER_SLOW | LIGHT_FLAG_PULSE_SLOW) != 0 {
                    0.5
                } else {
                    1.0
                };

            // Intensity modulation. Two paths:
            //   * PULSE/PULSE_SLOW → sine wave at the LIGH's
            //     period.
            //   * FLICKER/FLICKER_SLOW → smooth-noise: interpolate
            //     linearly between two consecutive hash samples
            //     stepped at 12 Hz. Phase 17 stepped the hash at
            //     24 Hz with no interpolation; visually that
            //     produced a jerky strobe rather than a candle's
            //     gentle dance, surfaced by the user reporting
            //     "shadows jump all over the place" in Phase 19
            //     readings.
            let modulation = if light.flags & (LIGHT_FLAG_PULSE | LIGHT_FLAG_PULSE_SLOW) != 0 {
                let phase_secs = (total_time + flicker.phase_offset_secs)
                    .rem_euclid(flicker.period_secs);
                let phase = phase_secs / flicker.period_secs;
                (phase * speed_scale * std::f32::consts::TAU).sin()
            } else if light.flags & (LIGHT_FLAG_FLICKER | LIGHT_FLAG_FLICKER_SLOW) != 0 {
                let raw = (total_time + flicker.phase_offset_secs) * 12.0 * speed_scale;
                let bucket = raw.floor() as u32;
                let bucket_t = raw.fract();
                // Smoothstep on the lerp factor — Hermite curve
                // hides the still-visible cusps a pure linear
                // lerp leaves at bucket boundaries.
                let t = bucket_t * bucket_t * (3.0 - 2.0 * bucket_t);
                let entity_seed = entity.wrapping_mul(0x9E37_79B9);
                let n0 = hash_to_unit(entity_seed ^ bucket);
                let n1 = hash_to_unit(entity_seed ^ bucket.wrapping_add(1));
                n0 * (1.0 - t) + n1 * t
            } else {
                0.0
            };

            let intensity = 1.0 + modulation * flicker.intensity_amplitude;

            // Position jitter — DISABLED in Phase 19.5.
            //
            // Skyrim authors `movement_amplitude` in BU (typical
            // 1-3 BU on vanilla candles). With our pure-random
            // hash noise at any reasonable frequency the light
            // teleports between uncorrelated positions every
            // bucket — visible as the "shadows jumping all over
            // the place" the operator reported on Phase 19. Real
            // candle flames move smoothly by tiny amounts; faking
            // that needs continuous noise (perlin/simplex), not
            // step-sampled hashes.
            //
            // Field stays on `LightFlicker` so the wiring can be
            // re-enabled when proper smooth noise lands. Phase
            // 17 / 18 / 19 commits documented the original
            // intent — see the field doc on
            // `LightFlicker.movement_amplitude`.
            let translation = None;
            let _ = flicker.movement_amplitude;
            let _ = flicker.base_translation;

            buf.push(LightUpdate {
                entity,
                intensity,
                translation,
            });
        }
        buf
    };

    if updates.is_empty() {
        return;
    }

    // Pass 2: write intensity back.
    if let Some(mut light_q) = world.query_mut::<LightSource>() {
        for u in &updates {
            if let Some(l) = light_q.get_mut(u.entity) {
                l.intensity = u.intensity;
            }
        }
    }

    // Pass 3: write transform translation back (only entities that
    // requested a jitter).
    if let Some(mut tf_q) = world.query_mut::<Transform>() {
        for u in &updates {
            if let Some(t) = u.translation {
                if let Some(tf) = tf_q.get_mut(u.entity) {
                    tf.translation = byroredux_core::math::Vec3::new(t[0], t[1], t[2]);
                }
            }
        }
    }
}
