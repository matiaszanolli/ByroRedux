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
//! | `PULSE_SLOW`  (0x100)| sine       | half    | ambience set-pieces
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
    EntityId, LightFlicker, LightKind, LightSource, World, LIGHT_FLAG_FLICKER,
    LIGHT_FLAG_FLICKER_SLOW, LIGHT_FLAG_PULSE, LIGHT_FLAG_PULSE_SLOW, LIGHT_FLAG_SHADOW_MASK,
    LIGHT_FLAG_SPOT,
};
use byroredux_plugin::esm::reader::GameKind;

const SHARED_LIGHT_ANIMATION_MASK: u32 =
    LIGHT_FLAG_FLICKER | LIGHT_FLAG_FLICKER_SLOW | LIGHT_FLAG_PULSE | LIGHT_FLAG_PULSE_SLOW;

/// Decode a game's raw LIGH flags into the shared runtime animation behavior.
///
/// The bits are not portable source data. `0x400` (Shadow Spotlight) is
/// never an animation flag in any game — verified directly against xEdit's
/// `wbDefinitionsTES5.pas` and F4Edit's `wbDefinitionsFO4.pas`, both of
/// which agree `0x400` = Shadow Spotlight, not slow-pulse. Fallout 4's LIGH
/// layout separately leaves the slow-variant bits themselves undefined —
/// `0x40` and `0x100` are reserved/unknown there (F4Edit shows no
/// Flicker-Slow or Pulse-Slow flag for Fallout 4 at all), so it only ever
/// decodes `Flicker`/`Pulse`. Keeping this conversion at the game boundary
/// prevents rendering-only flags from becoming continuous whole-scene
/// light animation.
///
/// #2251 (REN-D22-02) — `GameKind::Fallout76` verified directly against
/// FO76Edit's `wbDefinitionsFO76.pas`: `0x08`/`0x80` (Flicker/Pulse) match
/// Skyrim exactly, and `0x40`/`0x100` are unnamed ("Unknown 6"/"Unknown 8")
/// there too — the identical gap Fallout 4 has, so it shares Fallout 4's
/// arm rather than the Skyrim-shared default.
///
/// `GameKind::Starfield` is its own arm, not folded into the shared
/// default, because there is no positive evidence to fold it into either
/// side: SF1Edit's live LIGH definition (`wbDefinitionsSF1.pas`) replaced
/// the Skyrim/FO4/FO76 `DATA` subrecord with a restructured 76-byte
/// `DAT2` whose only named fields are a handful of floats — the bytes a
/// Flags field would occupy are an undifferentiated `wbUnknown` block, and
/// the `Flicker`/`Pulse`/shadow flag list that would apply to a `DATA`-style
/// layout survives only as commented-out reference dead code, never live.
/// Trusting Skyrim's bit meanings for whatever bytes our own `DAT2` reader
/// (`esm/cell/support.rs`, #1567) currently surfaces as `flags` is a
/// bigger, unverifiable claim than deciding "no animation" — `0` is not a
/// gap gate copied by omission here; it's the deliberate, only-defensible
/// value pending real evidence.
pub(crate) fn canonical_light_animation_flags(game: GameKind, source_flags: u32) -> u32 {
    let source_animation_mask = match game {
        GameKind::Fallout4 | GameKind::Fallout76 => LIGHT_FLAG_FLICKER | LIGHT_FLAG_PULSE,
        GameKind::Starfield => 0,
        _ => SHARED_LIGHT_ANIMATION_MASK,
    };
    source_flags & source_animation_mask
}

/// Decode a game's raw LIGH flags into the shared runtime shadow-projection
/// behavior (#2250 / REN-D22-01), mirroring [`canonical_light_animation_flags`]
/// above.
///
/// `LIGHT_FLAG_SHADOW_MASK` (`0x400`/`0x800`/`0x1000`) is directly verified
/// against TES5's LIGH layout (`light.rs`'s doc comment), and against
/// FO76Edit's `wbDefinitionsFO76.pas` too (#2251) — both name all three
/// bits identically to Skyrim at the same positions, high confidence. No
/// divergence has been identified for Oblivion / FO3 / FNV's own LIGH
/// layouts either, so those stay on the shared mask rather than silently
/// decoding zero, matching `canonical_light_animation_flags`'s own
/// default-to-shared-mask treatment of every game it hasn't specifically
/// gated. A verified per-game divergence gets its own `match` arm here,
/// exactly like `GameKind::Fallout4` does above.
///
/// `GameKind::Starfield` is excluded from that default for the same
/// reason `canonical_light_animation_flags` excludes it (#2251): SF1Edit's
/// live LIGH definition has no named Flags field at all in the
/// restructured `DAT2` subrecord (the byte range a `DATA`-style layout's
/// flags would occupy is an undifferentiated `wbUnknown` block there), so
/// there is no positive evidence to decode any bit meaning from whatever
/// our `DAT2` reader currently surfaces as `flags` — including shadow
/// bits. `0`, not a guess at alternate bit positions.
pub(crate) fn canonical_light_shadow_flags(game: GameKind, source_flags: u32) -> u32 {
    let source_shadow_mask = match game {
        GameKind::Starfield => 0,
        _ => LIGHT_FLAG_SHADOW_MASK,
    };
    source_flags & source_shadow_mask
}

/// The geometry half of a canonical [`LightSource`] derived from an ESM
/// LIGH `DATA`/`DAT2` record: [`LightSource::kind`], [`LightSource::
/// direction`], [`LightSource::outer_angle`]. Sibling boundary to
/// [`canonical_light_shadow_flags`] / [`canonical_light_animation_flags`]
/// above — same "translate once at the game boundary, not per-producer"
/// shape, closing the gap #2205 (NIFAL-D3-01) left for every ESM-sourced
/// light: only the direct-`NiPointLight`/`NiSpotLight` NIF-import path
/// populated these fields; every ESM-LIGH producer hand-copied
/// radius/color/flags/falloff and hard-defaulted the rest, so no
/// ESM-placed spotlight could ever render as anything but a full
/// omnidirectional point light. #2439 / NIFAL-D2-01.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LightGeometry {
    pub kind: LightKind,
    pub direction: [f32; 3],
    pub outer_angle: f32,
}

/// See [`LightGeometry`]. `ref_rot` is the placing REFR's own rotation
/// (already converted to the engine's Y-up quaternion convention via
/// `euler_zup_to_quat_yup_refr`) — these ESM-sourced lights carry no
/// per-light NIF node of their own, so the REFR's placement rotation is
/// the only orientation signal available for the emitted cone.
pub(crate) fn translate_light(
    ld: &byroredux_plugin::esm::cell::LightData,
    game: GameKind,
    ref_rot: byroredux_core::math::Quat,
) -> LightGeometry {
    // `LIGHT_FLAG_SPOT` (bit 9, 0x200, xEdit's `'Spot Light'`) is the
    // light-SHAPE signal — distinct from `LIGHT_FLAG_SHADOW_SPOTLIGHT`
    // (bit 10, 0x400, `canonical_light_shadow_flags` above), which is a
    // shadow-projection-TECHNIQUE choice. See `LIGHT_FLAG_SPOT`'s own doc
    // for why conflating the two (as this boundary's absence previously
    // let every producer implicitly do, by deriving no `kind` signal at
    // all) is wrong.
    //
    // `GameKind::Starfield` excluded for the same reason
    // `canonical_light_shadow_flags` excludes it: its DAT2 flags word is
    // an undifferentiated `wbUnknown` block with no verified bit
    // positions (see that function's doc) — no positive evidence to
    // decode `LIGHT_FLAG_SPOT` from whatever our `DAT2` reader currently
    // surfaces as `flags` either. `Point`, not a guess.
    let is_spot = game != GameKind::Starfield && ld.flags & LIGHT_FLAG_SPOT != 0;
    if !is_spot {
        return LightGeometry {
            kind: LightKind::Point,
            direction: [0.0, 0.0, 0.0],
            outer_angle: 0.0,
        };
    }

    // Gamebryo's `NiSpotLight` model direction is `(1, 0, 0)` — the
    // FIRST column of the world rotation matrix, NOT local -Z (verified
    // directly against `gamebryo-v32/Include/NiSpotLight.h`: "The model
    // location of the light is (0,0,0)... The model direction of the
    // light is (1,0,0). The world direction is the first column of the
    // world rotation matrix."). Identical convention to
    // `NiDirectionalLight`, already validated for XCLL directional
    // lighting (`euler_zup_to_quat_yup_tests.rs`,
    // `matches_refr_placement_rotation_of_model_direction`) — `ref_rot`
    // is the same Z-up-Euler→Y-up-quaternion conversion, confirmed there
    // to reproduce the correct engine-space direction when applied to
    // the `(1, 0, 0)` model direction.
    let direction = (ref_rot * byroredux_core::math::Vec3::new(1.0, 0.0, 0.0)).to_array();

    // xEdit's FOV field (`Core/wbDefinitionsTES5.pas`, dev-4.1.6:
    // `wbFloat('FOV', ..., wbNormalizeToRange(0.001, 160), 90)`) is the
    // FULL cone angle in degrees, default 90°. `LightSource.outer_angle`
    // is documented as the outer cone HALF-angle in radians — halve
    // before converting. `0.0` means the DATA subrecord was too short to
    // carry a FOV at all (see `LightData::fov_degrees`), not an authored
    // zero-degree cone, so it falls back to xEdit's own default rather
    // than producing a degenerate zero-width cone.
    let fov_degrees = if ld.fov_degrees > 0.0 { ld.fov_degrees } else { 90.0 };
    let outer_angle = (fov_degrees * 0.5).to_radians();

    LightGeometry {
        kind: LightKind::Spot,
        direction,
        outer_angle,
    }
}

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
/// light from its shared behavior, FNAM parameters, and the global clock.
/// Pure and deterministic in `(entity, flicker, total_time)` so it
/// is unit-testable without a `World` (PERF-D4-NEW-04 / #1380).
///
/// Returns the value written to `LightSource.intensity`: `1.0` when no
/// animation bit is set, otherwise `1 + modulation · amplitude · damping`.
fn flicker_intensity(entity: EntityId, flicker: &LightFlicker, total_time: f32) -> f32 {
    let flags = flicker.animation_flags;
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
        // Scale the PERIOD, not the phase — `speed_scale` must lengthen
        // the wrap interval so the phase still sweeps the full [0, 1)
        // range once per (slower) cycle. Multiplying the already-wrapped
        // phase by `speed_scale` instead (the pre-fix form) truncates the
        // sine to its positive half and repeats it at the ORIGINAL rate:
        // `sin(TAU * phase * speed_scale)` never exceeds `sin(TAU * 0.5 *
        // speed_scale)` in phase argument before `phase` itself wraps back
        // to 0, so for `speed_scale = 0.5` the argument only ever spans
        // `[0, pi)` — always non-negative, at the same cadence as
        // full-speed PULSE (#2479 / REN-D22-04).
        let effective_period = flicker.period_secs / speed_scale;
        let phase_secs = (total_time + flicker.phase_offset_secs).rem_euclid(effective_period);
        let phase = phase_secs / effective_period;
        (phase * std::f32::consts::TAU).sin()
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
        // Intensity modulation rides the same field `NiLightIntensityController`
        // writes; the renderer reads `color · dimmer · intensity` and doesn't
        // care which authored it. Source-format flags remain untouched on
        // `LightSource`; only the decoded shared behavior drives animation.
        light.intensity = flicker_intensity(entity, flicker, total_time);
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
    use byroredux_core::math::Quat;

    fn light_data(flags: u32, fov_degrees: f32) -> byroredux_plugin::esm::cell::LightData {
        byroredux_plugin::esm::cell::LightData {
            radius: 512.0,
            color: [1.0, 1.0, 1.0],
            flags,
            period_secs: 0.0,
            intensity_amplitude: 0.0,
            movement_amplitude: 0.0,
            falloff_exponent: 0.0,
            fov_degrees,
            xpwr_form_id: None,
        }
    }

    /// #2439 / NIFAL-D2-01 — the dominant case pre-fix: no `LIGHT_FLAG_SPOT`
    /// bit means `Point`, with a genuinely zero (not authored-but-unread)
    /// direction/outer_angle, matching `LightSource::default()`.
    #[test]
    fn translate_light_defaults_to_point_when_spot_flag_clear() {
        let ld = light_data(0, 90.0);
        let geom = translate_light(&ld, GameKind::Skyrim, Quat::IDENTITY);
        assert_eq!(geom.kind, LightKind::Point);
        assert_eq!(geom.direction, [0.0, 0.0, 0.0]);
        assert_eq!(geom.outer_angle, 0.0);
    }

    /// The core regression: `LIGHT_FLAG_SPOT` set must produce
    /// `LightKind::Spot` with the authored FOV halved into radians —
    /// xEdit's FOV is the FULL cone angle in degrees
    /// (`wbNormalizeToRange(0.001, 160)`, default 90), `outer_angle` is
    /// documented as the outer cone HALF-angle in radians.
    #[test]
    fn translate_light_derives_spot_kind_and_half_angle_from_fov() {
        let ld = light_data(LIGHT_FLAG_SPOT, 90.0);
        let geom = translate_light(&ld, GameKind::Skyrim, Quat::IDENTITY);
        assert_eq!(geom.kind, LightKind::Spot);
        let expected_half_angle = std::f32::consts::FRAC_PI_4; // 90° / 2, in radians
        assert!(
            (geom.outer_angle - expected_half_angle).abs() < 1e-6,
            "expected {expected_half_angle}, got {}",
            geom.outer_angle
        );
    }

    /// `fov_degrees == 0.0` means the DATA subrecord was too short to
    /// carry a FOV at all (see `LightData::fov_degrees`'s doc) — must
    /// fall back to xEdit's own 90° default, not produce a degenerate
    /// zero-width cone.
    #[test]
    fn translate_light_falls_back_to_xedit_default_fov_when_unset() {
        let ld = light_data(LIGHT_FLAG_SPOT, 0.0);
        let geom = translate_light(&ld, GameKind::Skyrim, Quat::IDENTITY);
        assert_eq!(geom.kind, LightKind::Spot);
        let expected_half_angle = std::f32::consts::FRAC_PI_4; // 90° default / 2
        assert!(
            (geom.outer_angle - expected_half_angle).abs() < 1e-6,
            "expected xEdit's 90° default halved, got {}",
            geom.outer_angle
        );
    }

    /// Direction must come from `ref_rot` applied to Gamebryo's `(1, 0, 0)`
    /// `NiSpotLight` model direction (verified against
    /// `gamebryo-v32/Include/NiSpotLight.h`), not local -Z. Identity
    /// rotation is the trivial case: direction passes through unchanged.
    #[test]
    fn translate_light_direction_is_model_plus_x_through_ref_rot() {
        let ld = light_data(LIGHT_FLAG_SPOT, 90.0);
        let geom = translate_light(&ld, GameKind::Skyrim, Quat::IDENTITY);
        assert!((geom.direction[0] - 1.0).abs() < 1e-6);
        assert!(geom.direction[1].abs() < 1e-6);
        assert!(geom.direction[2].abs() < 1e-6);
    }

    /// Parity with `euler_zup_to_quat_yup_tests.rs`'s own validated
    /// convention (`xcll_dir_yup`/`matches_refr_placement_rotation_of_
    /// model_direction`): a non-trivial `ref_rot` must rotate the
    /// `(1, 0, 0)` model direction exactly like that test's XCLL path
    /// does, since both derive from the same Gamebryo model-direction
    /// contract.
    #[test]
    fn translate_light_direction_matches_refr_rotation_of_model_direction() {
        let ld = light_data(LIGHT_FLAG_SPOT, 90.0);
        let ref_rot = crate::cell_loader::euler_zup_to_quat_yup(0.25, 0.4, 0.0);
        let geom = translate_light(&ld, GameKind::Skyrim, ref_rot);
        let expected = ref_rot * byroredux_core::math::Vec3::new(1.0, 0.0, 0.0);
        assert!((geom.direction[0] - expected.x).abs() < 1e-6);
        assert!((geom.direction[1] - expected.y).abs() < 1e-6);
        assert!((geom.direction[2] - expected.z).abs() < 1e-6);
    }

    /// Starfield's DAT2 flags word has no verified bit-9 equivalent (same
    /// "undifferentiated wbUnknown block" gap `canonical_light_shadow_flags`
    /// already excludes it for) — must stay `Point` even with the bit set,
    /// matching the "0 is deliberate, not a guess" policy this file uses
    /// consistently for every Starfield LIGH-flag boundary.
    #[test]
    fn translate_light_excludes_starfield_even_with_spot_bit_set() {
        let ld = light_data(LIGHT_FLAG_SPOT, 90.0);
        let geom = translate_light(&ld, GameKind::Starfield, Quat::IDENTITY);
        assert_eq!(geom.kind, LightKind::Point);
        assert_eq!(geom.direction, [0.0, 0.0, 0.0]);
        assert_eq!(geom.outer_angle, 0.0);
    }

    fn flicker(flags: u32, amplitude: f32, period: f32) -> LightFlicker {
        LightFlicker {
            animation_flags: flags,
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
        let f = flicker(0, 0.25, 0.5);
        assert_eq!(flicker_intensity(1, &f, 0.0), 1.0);
        assert_eq!(flicker_intensity(1, &f, 3.7), 1.0);
    }

    #[test]
    fn pulse_is_sine_of_phase() {
        let f = flicker(LIGHT_FLAG_PULSE, 0.4, 1.0); // amplitude 0.4, period 1 s
                                                     // total_time 0 → phase 0 → sin(0) = 0 → unit.
        assert!((flicker_intensity(1, &f, 0.0) - 1.0).abs() < 1e-6);
        // total_time = period/4 → phase 0.25 → sin(TAU·0.25) = 1
        // → 1 + 1·0.4·0.5 = 1.2.
        let peak = flicker_intensity(1, &f, 0.25);
        assert!((peak - 1.2).abs() < 1e-6, "expected 1.2, got {peak}");
    }

    #[test]
    fn pulse_slow_runs_at_half_angular_velocity() {
        let fast_flicker = flicker(LIGHT_FLAG_PULSE, 0.4, 1.0);
        let slow_flicker = flicker(LIGHT_FLAG_PULSE_SLOW, 0.4, 1.0);
        // At t = period/4: fast PULSE reaches sin(90°)=1 (peak),
        // PULSE_SLOW only sin(45°)=√2/2 — strictly less than the peak.
        let fast = flicker_intensity(1, &fast_flicker, 0.25);
        let slow = flicker_intensity(1, &slow_flicker, 0.25);
        assert!((fast - 1.2).abs() < 1e-6);
        let expected_slow = 1.0 + std::f32::consts::FRAC_1_SQRT_2 * 0.4 * FLICKER_INTENSITY_DAMPING;
        assert!((slow - expected_slow).abs() < 1e-6, "slow={slow}");
        assert!(slow < fast);
    }

    #[test]
    fn pulse_slow_diverges_in_sign_from_pulse_past_one_period() {
        // #2479 / REN-D22-04 — a half-wave-rectified-at-original-rate bug
        // is invisible at t = period/4 (both waveforms agree there); it
        // only shows up once the true half-rate wave has entered its
        // second half-cycle. At t = 1.5 * period: PULSE (full rate) has
        // completed 1.5 cycles and sits at its zero-crossing
        // (sin(TAU*0.5) = 0), while true half-rate PULSE_SLOW is only
        // 0.75 of the way through its one (period-doubled) cycle —
        // sin(TAU*0.75) = -1, strictly negative. The pre-fix formula
        // produced +1 here instead (rectified, same rate as PULSE).
        let fast_flicker = flicker(LIGHT_FLAG_PULSE, 0.4, 1.0);
        let slow_flicker = flicker(LIGHT_FLAG_PULSE_SLOW, 0.4, 1.0);
        let fast = flicker_intensity(1, &fast_flicker, 1.5);
        let slow = flicker_intensity(1, &slow_flicker, 1.5);
        assert!((fast - 1.0).abs() < 1e-6, "fast={fast}");
        let expected_slow = 1.0 - 0.4 * FLICKER_INTENSITY_DAMPING;
        assert!((slow - expected_slow).abs() < 1e-6, "slow={slow}");
        assert!(
            slow < 1.0,
            "true half-rate PULSE_SLOW must trough below the authored \
             intensity at t = 1.5 * period, not just brighten it: slow={slow}"
        );
    }

    #[test]
    fn flicker_is_deterministic_and_bounded() {
        let f = flicker(LIGHT_FLAG_FLICKER, 0.4, 0.5);
        let a = flicker_intensity(42, &f, 1.234);
        let b = flicker_intensity(42, &f, 1.234);
        assert_eq!(a, b, "same inputs must be deterministic");
        // modulation ∈ [-1, 1] → intensity ∈ [1 ± amplitude·damping].
        let half = 0.4 * FLICKER_INTENSITY_DAMPING;
        assert!(
            a >= 1.0 - half - 1e-6 && a <= 1.0 + half + 1e-6,
            "out of band: {a}"
        );
        // Distinct entity seeds generally diverge → confirm the seed
        // actually feeds the hash (not a constant).
        let other = flicker_intensity(43, &f, 1.234);
        assert_ne!(a, other, "entity seed must influence the noise");
    }

    #[test]
    fn shadow_spotlight_bit_never_leaks_into_animation_on_any_game() {
        // 0x400 is Shadow Spotlight in both Skyrim's and Fallout 4's LIGH
        // layout (xEdit wbDefinitionsTES5.pas / F4Edit wbDefinitionsFO4.pas
        // agree) — it must never decode as an animation flag anywhere. This
        // was the actual bug: LIGHT_FLAG_PULSE_SLOW used to be defined as
        // 0x400, so every non-Fallout4 game slow-pulsed its shadow spotlights.
        const SHADOW_SPOTLIGHT: u32 = 0x0000_0400;
        assert_eq!(
            canonical_light_animation_flags(GameKind::Fallout4, SHADOW_SPOTLIGHT),
            0,
        );
        assert_eq!(
            canonical_light_animation_flags(GameKind::Skyrim, SHADOW_SPOTLIGHT),
            0,
        );
        assert_eq!(
            canonical_light_animation_flags(GameKind::Fallout3NV, SHADOW_SPOTLIGHT),
            0,
        );
    }

    #[test]
    fn fallout4_slow_variant_bits_are_reserved_not_animated() {
        // Fallout 4's LIGH layout leaves 0x40 and 0x100 reserved/unknown
        // (F4Edit shows no Flicker-Slow or Pulse-Slow flag for FO4 at all) —
        // only Flicker (0x08) and Pulse (0x80) exist there.
        assert_eq!(
            canonical_light_animation_flags(GameKind::Fallout4, LIGHT_FLAG_PULSE_SLOW),
            0,
        );
        assert_eq!(
            canonical_light_animation_flags(GameKind::Fallout4, LIGHT_FLAG_FLICKER_SLOW),
            0,
        );
    }

    #[test]
    fn genuine_pulse_slow_animates_on_non_fallout4_games() {
        // 0x100 is the real Pulse-Slow bit (Skyrim + FO3-lineage, per xEdit).
        assert_eq!(
            canonical_light_animation_flags(GameKind::Skyrim, LIGHT_FLAG_PULSE_SLOW),
            LIGHT_FLAG_PULSE_SLOW,
        );
        assert_eq!(
            canonical_light_animation_flags(GameKind::Fallout3NV, LIGHT_FLAG_PULSE_SLOW),
            LIGHT_FLAG_PULSE_SLOW,
        );
    }

    #[test]
    fn fallout4_real_flicker_and_pulse_map_to_shared_behavior() {
        let source = LIGHT_FLAG_FLICKER | LIGHT_FLAG_PULSE | LIGHT_FLAG_PULSE_SLOW;
        assert_eq!(
            canonical_light_animation_flags(GameKind::Fallout4, source),
            LIGHT_FLAG_FLICKER | LIGHT_FLAG_PULSE,
        );
        assert_eq!(
            canonical_light_animation_flags(GameKind::Skyrim, source),
            source,
        );
    }

    /// Regression for #2251 (REN-D22-02): Fallout 76 verified directly
    /// against FO76Edit's `wbDefinitionsFO76.pas` — `0x40`/`0x100` are
    /// unnamed there too (the identical gap FO4 has), so FO76 shares
    /// FO4's arm rather than the Skyrim-shared default.
    #[test]
    fn fallout76_matches_fallout4_slow_variant_gap() {
        assert_eq!(
            canonical_light_animation_flags(GameKind::Fallout76, LIGHT_FLAG_PULSE_SLOW),
            0,
        );
        assert_eq!(
            canonical_light_animation_flags(GameKind::Fallout76, LIGHT_FLAG_FLICKER_SLOW),
            0,
        );
        let source = LIGHT_FLAG_FLICKER | LIGHT_FLAG_PULSE | LIGHT_FLAG_PULSE_SLOW;
        assert_eq!(
            canonical_light_animation_flags(GameKind::Fallout76, source),
            LIGHT_FLAG_FLICKER | LIGHT_FLAG_PULSE,
        );
    }

    /// Regression for #2250 (REN-D22-01): `canonical_light_shadow_flags`
    /// must mask down to only the three verified shadow-projection bits —
    /// unrelated LIGH flags (Dynamic, Portal-strict, CanCarry, etc.) must
    /// never leak through as a spurious "casts shadows" signal.
    #[test]
    fn unrelated_flags_do_not_leak_into_shadow_decode() {
        const DYNAMIC_AND_PORTAL_STRICT: u32 = 0x0000_2001;
        assert_eq!(
            canonical_light_shadow_flags(GameKind::Skyrim, DYNAMIC_AND_PORTAL_STRICT),
            0,
        );
    }

    /// The three authored shadow-projection bits must all survive the
    /// decode on the verified (Skyrim) game.
    #[test]
    fn authored_shadow_bits_survive_the_decode() {
        use byroredux_core::ecs::{
            LIGHT_FLAG_SHADOW_HEMISPHERE, LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
            LIGHT_FLAG_SHADOW_SPOTLIGHT,
        };
        let source = LIGHT_FLAG_SHADOW_SPOTLIGHT
            | LIGHT_FLAG_SHADOW_HEMISPHERE
            | LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL
            | 0x0000_2001; // plus unrelated bits, must not affect the result
        assert_eq!(
            canonical_light_shadow_flags(GameKind::Skyrim, source),
            LIGHT_FLAG_SHADOW_SPOTLIGHT
                | LIGHT_FLAG_SHADOW_HEMISPHERE
                | LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
        );
    }

    /// No shadow-flag divergence has been identified for any game yet
    /// (unlike animation flags, where Fallout 4 gets its own arm) — every
    /// `GameKind` decodes through the same shared mask today. This pins
    /// that default so a future verified divergence is a deliberate,
    /// visible `match` arm addition, not a silent behavior change.
    #[test]
    fn every_game_shares_the_same_shadow_mask_today() {
        use byroredux_core::ecs::LIGHT_FLAG_SHADOW_MASK;
        for game in [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
        ] {
            assert_eq!(
                canonical_light_shadow_flags(game, LIGHT_FLAG_SHADOW_MASK),
                LIGHT_FLAG_SHADOW_MASK,
                "{game:?} must decode the full shadow mask until a verified divergence is found"
            );
        }
    }

    /// #2251 — Starfield's restructured `DAT2` LIGH subrecord has no
    /// verified Flags field at all (SF1Edit's live definition leaves
    /// those bytes an undifferentiated `wbUnknown` block), so neither
    /// animation nor shadow bits can be trusted to mean what they mean on
    /// Skyrim/FO4/FO76 — both canonicalization functions must decode
    /// exactly zero for Starfield regardless of the raw input.
    #[test]
    fn starfield_has_no_verified_flags_field_for_either_canonicalization() {
        use byroredux_core::ecs::LIGHT_FLAG_SHADOW_MASK;
        let all_bits_set = LIGHT_FLAG_FLICKER
            | LIGHT_FLAG_FLICKER_SLOW
            | LIGHT_FLAG_PULSE
            | LIGHT_FLAG_PULSE_SLOW
            | LIGHT_FLAG_SHADOW_MASK;
        assert_eq!(
            canonical_light_animation_flags(GameKind::Starfield, all_bits_set),
            0,
            "Starfield has no verified animation-flag layout"
        );
        assert_eq!(
            canonical_light_shadow_flags(GameKind::Starfield, all_bits_set),
            0,
            "Starfield has no verified shadow-flag layout"
        );
    }
}
