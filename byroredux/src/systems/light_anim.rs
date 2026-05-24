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
            // Phase angle in [0, 1) — wraps per period_secs.
            let phase_secs = (total_time + flicker.phase_offset_secs).rem_euclid(flicker.period_secs);
            let phase = phase_secs / flicker.period_secs;

            // Slow variants run at half rate by halving the angular
            // velocity. Cheaper than reading period at half the
            // FNAM authored value because some lights set both
            // bits.
            let speed_scale =
                if light.flags & (LIGHT_FLAG_FLICKER_SLOW | LIGHT_FLAG_PULSE_SLOW) != 0 {
                    0.5
                } else {
                    1.0
                };
            let scaled_phase = phase * speed_scale;

            // Intensity modulation: pulse is sinusoidal, flicker
            // is hashed noise stepped at ~24 Hz so it reads as a
            // flame's chaotic dance rather than per-frame
            // whitenoise.
            let modulation = if light.flags & (LIGHT_FLAG_PULSE | LIGHT_FLAG_PULSE_SLOW) != 0 {
                (scaled_phase * std::f32::consts::TAU).sin()
            } else if light.flags & (LIGHT_FLAG_FLICKER | LIGHT_FLAG_FLICKER_SLOW) != 0 {
                let bucket = (total_time * 24.0 * speed_scale) as u32;
                hash_to_unit(entity.wrapping_mul(0x9E37_79B9) ^ bucket)
            } else {
                0.0
            };

            let intensity = 1.0 + modulation * flicker.intensity_amplitude;

            // Position jitter — only when movement_amplitude is
            // non-zero so pure-pulse lights don't move. Use the
            // same 24 Hz bucket so brightness + motion stay
            // synchronised (more visually plausible than
            // independent noises).
            let translation = if flicker.movement_amplitude > 0.0 {
                let bucket = (total_time * 24.0 * speed_scale) as u32;
                let seed = entity.wrapping_mul(0x9E37_79B9) ^ bucket;
                let jx = hash_to_unit(seed) * flicker.movement_amplitude;
                let jy =
                    hash_to_unit(seed.wrapping_add(0x1234_5678)) * flicker.movement_amplitude;
                let jz =
                    hash_to_unit(seed.wrapping_add(0x8765_4321)) * flicker.movement_amplitude;
                Some([
                    flicker.base_translation[0] + jx,
                    flicker.base_translation[1] + jy,
                    flicker.base_translation[2] + jz,
                ])
            } else {
                None
            };

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
