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
    EntityId, LIGHT_FLAG_FLICKER, LIGHT_FLAG_FLICKER_SLOW, LIGHT_FLAG_PULSE, LIGHT_FLAG_PULSE_SLOW,
    LIGHT_FLAG_SHADOW_MASK, LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL, LIGHT_FLAG_SHADOW_SPOTLIGHT,
    LIGHT_FLAG_SPOT, LightFlicker, LightKind, LightSource, World,
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
/// `GameKind::Starfield` is its own arm, on the same strict-by-default
/// footing as `Fallout4`/`Fallout76` above: its flags word is real, but none
/// of its bit *meanings* are evidenced.
///
/// #3987 corrected the premise this arm used to rest on. It previously read
/// "the bytes a Flags field would occupy are an undifferentiated `wbUnknown`
/// block", i.e. there is no field at all. That is contradicted by our own
/// `DAT2` decoder (`esm/cell/support.rs`, #1567), whose offset table is
/// introduced as verified against `wbDefinitionsSF1.pas` and whose third row
/// is `{12} UInt16 Flags`. It is also contradicted by the data: across all
/// 1,575 LIGH records in `Starfield.esm`, the u16 at `DAT2+12` takes 16
/// distinct sparse values (union of set bits `0x17F1`) while the u16 at
/// `+14` is zero in every single record — a populated 16-bit bitfield
/// followed by two unused bytes, exactly as the decoder's table describes,
/// and not the u32 Skyrim's `DATA` carries.
///
/// What is still absent is the bit *legend*. The observed bit set does not
/// match Skyrim's either: Skyrim's `0x02`/`0x04`/`0x08` (Can-Be-Carried /
/// Negative / Flicker) never appear in any Starfield record, while an
/// unnamed `0x10` does. So Skyrim's `Flicker`/`Pulse` positions cannot be
/// transferred, and an unnamed bit must not decode into *motion* — a light
/// that visibly pulses when the record never asked for it is an obvious,
/// reported artifact. `0` remains the only defensible value here, now for
/// the accurate reason. Contrast the shadow sibling below, whose opposite
/// default applies to Starfield precisely because the field is real.
pub(crate) fn canonical_light_animation_flags(game: GameKind, source_flags: u32) -> u32 {
    let source_animation_mask = match game {
        GameKind::Fallout4 | GameKind::Fallout76 => LIGHT_FLAG_FLICKER | LIGHT_FLAG_PULSE,
        GameKind::Starfield => 0,
        _ => SHARED_LIGHT_ANIMATION_MASK,
    };
    source_flags & source_animation_mask
}

/// Decode legacy projection flags for diagnostics. Runtime visibility is
/// always material-aware and full-scene, independent of this word; the
/// shadow-policy discussion below records the historical decoding rationale.
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
/// ## The asymmetry with the animation sibling is deliberate (#2517)
///
/// These two canonicalizers apply *opposite* defaults to bits that a
/// game's own LIGH layout does not name, and that is a decision rather
/// than drift:
///
/// * **Animation decode is strict-by-default.** `FO4`/`FO76` are narrowed
///   to `FLICKER|PULSE` precisely because `0x40`/`0x100` are unnamed
///   there, and an unnamed bit must not decode into *motion* — a light
///   that visibly pulses when the record never asked for it is an obvious,
///   reported artifact.
/// * **Shadow decode is permissive-by-default.** Dropping a shadow bit
///   that a game *does* name is the strictly worse error: the light
///   silently stops casting RT shadows and the scene just looks flat, with
///   nothing to trace it back to. So an unverified bit stays enabled.
///
/// The concrete open question this leaves: of the three bits, only `0x400`
/// (Spot Shadow) is believed to be named in the Oblivion / FO3 / FNV
/// layouts, so an Oblivion LIGH carrying `0x800`/`0x1000` as reserved junk
/// would be promoted to "casts shadows". Narrowing those games to
/// `LIGHT_FLAG_SHADOW_SPOTLIGHT` alone requires reading xEdit's
/// `wbDefinitionsTES4.pas` / `wbDefinitionsFNV.pas` LIGH flag lists
/// directly — the same authority `equip.rs` cites for biped slots. Those
/// files are not vendored in this tree, and narrowing on the *assumption*
/// that the bits are unnamed would be exactly the guess the shadow-side
/// default exists to avoid. Left permissive pending that evidence; see
/// #2517 for the measurement that would settle it.
///
/// `GameKind::Starfield` used to be excluded from that default, and #3987
/// removed the exception rather than the policy.
///
/// The exclusion rested entirely on one claim (#2251): that SF1Edit's LIGH
/// definition "has no named Flags field at all" in the restructured `DAT2`,
/// so there was nothing to be permissive *about*. That claim is false. Our
/// own `DAT2` decoder reads `{12} UInt16 Flags` from an offset table it
/// introduces as verified against `wbDefinitionsSF1.pas`, and the shipped
/// data agrees: over all 1,575 `Starfield.esm` LIGH records the word at
/// `DAT2+12` is a populated sparse bitfield (16 distinct values, union
/// `0x17F1`) with the following u16 zero in every record.
///
/// That puts Starfield in exactly the position Oblivion / FO3 / FNV are in
/// two paragraphs above — a real, named flags word whose individual bit
/// meanings this tree has not independently verified — and those games get
/// the shared mask, on the stated grounds that narrowing on an *assumption*
/// "would be exactly the guess the shadow-side default exists to avoid".
/// Applying the same default to Starfield is consistency, not a new claim.
///
/// At the time the behaviour this restored was not marginal. With the zero
/// mask, `LightSource::from_legacy_world_units` computed
/// `VisibilityMask::for_legacy_projection(false)` for **every** placed
/// Starfield light — then `ARCHITECTURE` only, broadened to
/// `ARCHITECTURE | DYNAMIC_ACTOR` by `3ce970a5a` — so props, foliage, glass
/// and effects cast no shadow from any of them: the whole-game version of
/// the silent-flatness failure this default was written to prevent.
///
/// #4557 — history, not the current mask: since `b9e961eeb` (lighting
/// unification) `for_legacy_projection` returns `VisibilityMask::FULL`
/// whatever its argument, so shadow flags no longer narrow any light's
/// visibility query. They are still carried through to `LightSource` for
/// diagnostics, which is what the canonicalization below preserves.
///
/// The animation sibling stays at `0`, and the asymmetry is the documented
/// one: an unverified bit must not create motion, but it may cast a shadow.
pub(crate) fn canonical_light_shadow_flags(
    game: GameKind,
    source_flags: u32,
    starfield_light_type: u8,
) -> u32 {
    let authored = source_flags & LIGHT_FLAG_SHADOW_MASK;
    // #4424's evidence pass — Starfield's u16 flags carry NO
    // shadow-projection bits under the now-published xEdit legend; the
    // Shadow-vs-NonShadow choice lives in the DAT2+56 Light Type enum
    // (`1 = Shadow Spotlight`, `2 = NonShadow Spotlight`). Map that enum
    // onto the canonical technique bits: an authored Shadow Spotlight
    // traces spot-cone shadows; a NonShadow Spotlight keeps the cone but
    // carries no spotlight-shadow technique.
    if game == GameKind::Starfield && starfield_light_type == 1 {
        return LIGHT_FLAG_SHADOW_SPOTLIGHT;
    }
    // Fallout 3 / New Vegas' shipped interiors use zero projection bits for
    // ordinary room lights. Prospector's Saloon has 24 placed LIGH records,
    // all with zero projection bits; treating zero as an unshadowed fallback
    // removes static-prop shadows from the entire cell. The RT renderer uses
    // an omnidirectional query instead, matching the visual role of those
    // local sources while retaining authored projection choices when present.
    if game == GameKind::Fallout3NV && authored == 0 {
        LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL
    } else {
        authored
    }
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
    // xEdit dev-4.1.6 Core/wbDefinitions{TES4,FNV,TES5,FO4,FO76,SF1}.pas:
    // "Shadow Spotlight" (0x400) is itself a cone; vanilla Skyrim/FO4 do
    // NOT also set 0x200. FO4/FO76 call 0x200 "Unknown 9" and move the
    // non-shadow cone to 0x4000. Starfield uses DAT2+56's explicit enum;
    // its 0x200 means "Focus Spotlight Beam", not the emitter's shape.
    // Keep these raw-format differences here. "NonShadow" describes the
    // source engine's shadow allocation, not our material-aware visibility.
    let is_spot = match game {
        GameKind::Starfield => matches!(ld.starfield_light_type, 1 | 2),
        GameKind::Fallout4 | GameKind::Fallout76 => {
            const NON_SHADOW_SPOTLIGHT: u32 = 0x4000;
            ld.flags & (LIGHT_FLAG_SHADOW_SPOTLIGHT | NON_SHADOW_SPOTLIGHT) != 0
        }
        GameKind::Oblivion | GameKind::Fallout3NV | GameKind::Skyrim => {
            ld.flags & (LIGHT_FLAG_SPOT | LIGHT_FLAG_SHADOW_SPOTLIGHT) != 0
        }
    };
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
    // FULL cone angle in degrees, default 90°. The legacy constructor
    // accepts the outer cone HALF-angle in radians — halve
    // before converting. `0.0` means the DATA subrecord was too short to
    // carry a FOV at all (see `LightData::fov_degrees`), not an authored
    // zero-degree cone, so it falls back to xEdit's own default rather
    // than producing a degenerate zero-width cone.
    let fov_degrees = if ld.fov_degrees > 0.0 {
        ld.fov_degrees
    } else {
        90.0
    };
    let outer_angle = (fov_degrees * 0.5).to_radians();

    LightGeometry {
        kind: LightKind::Spot,
        direction,
        outer_angle,
    }
}

/// Resolve the LIGH `falloff_exponent` sentinel (`0.0`, the "field absent"
/// encoding documented on [`byroredux_plugin::esm::cell::LightData::
/// falloff_exponent`]) to the per-layout default the doc contract names:
/// `1.0` on the Skyrim+ 48-byte layout, `2.0` on the pre-Skyrim 32-byte one
/// (FO3/FNV's common quadratic `k ≈ 2`; Oblivion's LIGH shares that
/// generation). An authored positive value passes through verbatim. Lives
/// beside its `canonical_light_*` siblings so the per-game knowledge enters
/// once, at the ESM-light boundary, instead of one shared fallback reaching
/// every game (`lighting.rs`'s own `1.0` remains as the inert last-resort
/// net for non-ESM producers).
pub(crate) fn canonical_light_falloff_exponent(game: GameKind, source: f32) -> f32 {
    if source.is_finite() && source > 0.0 {
        return source;
    }
    match game {
        GameKind::Oblivion | GameKind::Fallout3NV => 2.0,
        _ => 1.0,
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

/// Noise samples per authored flicker period (#2516).
///
/// The FLICKER path interpolates between hash buckets; this sets how many
/// buckets one authored `period_secs` spans. `6.0` is picked so Skyrim's
/// vanilla 0.5 s candle steps at `6 / 0.5 = 12` Hz — exactly the rate
/// Phase 19 arrived at by eye (Phase 17's 24 Hz read as a jerky strobe
/// rather than a candle's gentle dance) — while a longer authored period
/// now actually flickers more slowly instead of being ignored.
const FLICKER_BUCKETS_PER_PERIOD: f32 = 6.0;

/// Fallback period for a `LightFlicker` that somehow carries a
/// non-positive `period_secs`. Matches the vanilla Skyrim candle default
/// `attach.rs` already substitutes for pre-Skyrim records (#2478), so the
/// two agree; this copy exists only to keep [`flicker_intensity`] total
/// for direct unit-test callers.
const DEFAULT_FLICKER_PERIOD_SECS: f32 = 0.5;

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
        // Derive the bucket rate from the authored period (#2516). Before
        // this the flicker branch stepped at a hardcoded 12 Hz and never
        // read `period_secs` at all, so every flickering light in a scene
        // ran at an identical rate no matter what its FNAM asked for —
        // `phase_offset_secs` was the only per-light variation left, and a
        // roomful of mixed fixtures flickered homogeneously. The pulse
        // branch immediately above always honoured the period; this is the
        // asymmetry, not a deliberate override.
        //
        // `FLICKER_BUCKETS_PER_PERIOD / period_secs` is chosen so Skyrim's
        // vanilla 0.5 s candle still lands on exactly the 12 Hz that Phase
        // 19 tuned by eye (24 Hz read as a jerky strobe), while a longer
        // authored period now genuinely steps slower. Pre-Skyrim games are
        // safe here: #2478 / REN-D22-03 made `attach.rs` substitute the
        // vanilla 0.5 s candle default when a record authors no period, so
        // `period_secs` is always a real value by the time it reaches this
        // function — which is why this fix had to wait for that one.
        let period_secs = if flicker.period_secs > 0.0 {
            flicker.period_secs
        } else {
            // Defensive only — `attach.rs` guarantees a positive period.
            // Keeps this pure function total for direct unit-test callers.
            DEFAULT_FLICKER_PERIOD_SECS
        };
        let buckets_per_sec = FLICKER_BUCKETS_PER_PERIOD / period_secs;
        let raw = (total_time + flicker.phase_offset_secs) * buckets_per_sec * speed_scale;
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
            starfield_light_type: 0,
        }
    }

    /// The fixture with the Starfield DAT2+56 Light Type enum set
    /// (`wbDefinitionsSF1.pas`: 0 = Omnidirectional, 1 = Shadow Spotlight,
    /// 2 = NonShadow Spotlight).
    fn light_data_typed(
        flags: u32,
        fov_degrees: f32,
        light_type: u8,
    ) -> byroredux_plugin::esm::cell::LightData {
        byroredux_plugin::esm::cell::LightData {
            starfield_light_type: light_type,
            ..light_data(flags, fov_degrees)
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

    #[test]
    fn translate_light_shadow_spot_flag_alone_is_a_cone() {
        // Vanilla Skyrim uses 0x400 without 0x200. The shadow projection
        // flag also describes the emitter's shape, not just its old budget.
        for game in [GameKind::Oblivion, GameKind::Fallout3NV, GameKind::Skyrim] {
            let ld = light_data(0x400, 60.0);
            let geom = translate_light(&ld, game, Quat::IDENTITY);
            assert_eq!(geom.kind, LightKind::Spot, "{game:?}");
            assert_eq!(geom.outer_angle, 30.0f32.to_radians());
        }
    }

    #[test]
    fn translate_light_fallout_spot_flags_do_not_reuse_skyrim_bit_nine() {
        // xEdit FO4/FO76: 0x200 is Unknown 9; 0x400 Shadow Spotlight;
        // 0x4000 NonShadow Spotlight. Both named types emit cones.
        for game in [GameKind::Fallout4, GameKind::Fallout76] {
            for flags in [0x400, 0x4000, 0x4001, 0x4009] {
                let geom = translate_light(&light_data(flags, 70.0), game, Quat::IDENTITY);
                assert_eq!(geom.kind, LightKind::Spot, "{game:?} flags={flags:#x}");
                assert_eq!(geom.outer_angle, 35.0f32.to_radians());
            }
            for flags in [0, 0x200, 0x800, 0x1000, 0x20000] {
                let geom = translate_light(&light_data(flags, 70.0), game, Quat::IDENTITY);
                assert_eq!(geom.kind, LightKind::Point, "{game:?} flags={flags:#x}");
                assert_eq!(geom.direction, [0.0; 3]);
            }
        }
        // Do not transfer FO4's non-shadow cone bit back to Skyrim.
        let geom = translate_light(&light_data(0x4000, 70.0), GameKind::Skyrim, Quat::IDENTITY);
        assert_eq!(geom.kind, LightKind::Point);
    }

    #[test]
    fn translate_light_starfield_unknown_types_are_not_invented_cones() {
        for light_type in [3, 255] {
            let geom = translate_light(
                &light_data_typed(0, 90.0, light_type),
                GameKind::Starfield,
                Quat::IDENTITY,
            );
            assert_eq!(geom.kind, LightKind::Point, "type={light_type}");
        }
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

    /// A non-trivial REFR rotation must rotate the `(1, 0, 0)` model
    /// direction through the dedicated placement-Euler conversion. XCLL uses
    /// authored spherical azimuth/elevation and intentionally does not share
    /// this helper.
    #[test]
    fn translate_light_direction_matches_refr_rotation_of_model_direction() {
        let ld = light_data(LIGHT_FLAG_SPOT, 90.0);
        let ref_rot = crate::cell_loader::euler_zup_to_quat_yup_refr(0.25, 0.4, 0.0);
        let geom = translate_light(&ld, GameKind::Skyrim, ref_rot);
        let expected = ref_rot * byroredux_core::math::Vec3::new(1.0, 0.0, 0.0);
        assert!((geom.direction[0] - expected.x).abs() < 1e-6);
        assert!((geom.direction[1] - expected.y).abs() < 1e-6);
        assert!((geom.direction[2] - expected.z).abs() < 1e-6);
    }

    /// Starfield's `0x200` is NOT the spot-shape bit — the now-published
    /// xEdit legend (`wbDefinitionsSF1.pas`, dev-4.1.6) names it "Focus
    /// Spotlight Beam", and the shape signal is the dedicated DAT2+56
    /// Light Type enum instead. This test keeps pinning the vindicated
    /// refusal: a flags-only spot bit with `light_type == 0`
    /// (Omnidirectional) must stay `Point`.
    #[test]
    fn translate_light_excludes_starfield_even_with_spot_bit_set() {
        let ld = light_data(LIGHT_FLAG_SPOT, 90.0);
        let geom = translate_light(&ld, GameKind::Starfield, Quat::IDENTITY);
        assert_eq!(geom.kind, LightKind::Point);
        assert_eq!(geom.direction, [0.0, 0.0, 0.0]);
        assert_eq!(geom.outer_angle, 0.0);
    }

    /// #4424's evidence pass — Starfield's DAT2+56 Light Type enum IS the
    /// spot-shape authority: `1 = Shadow Spotlight` and
    /// `2 = NonShadow Spotlight` both emit a cone; `0` stays a point. FOV
    /// (DAT2+20, default 90) translates exactly like the other games.
    #[test]
    fn starfield_light_type_enum_drives_the_spot_shape() {
        for light_type in [1u8, 2] {
            let ld = light_data_typed(0, 90.0, light_type);
            let geom = translate_light(&ld, GameKind::Starfield, Quat::IDENTITY);
            assert_eq!(geom.kind, LightKind::Spot, "light_type {light_type}");
            assert_eq!(
                geom.outer_angle,
                (90.0f32 * 0.5).to_radians(),
                "the xEdit 90° default is the full cone angle — halved to the outer half-angle"
            );
        }
        let omni = light_data_typed(0, 90.0, 0);
        assert_eq!(
            translate_light(&omni, GameKind::Starfield, Quat::IDENTITY).kind,
            LightKind::Point
        );
    }

    /// And the enum's shadow split maps onto the canonical technique bits:
    /// `1 = Shadow Spotlight` traces spot-cone shadows; `2 =
    /// NonShadow Spotlight` keeps the cone with no spotlight-shadow
    /// technique; `0` keeps the pre-existing #3987 permissive-mask
    /// behavior for the flags word.
    #[test]
    fn starfield_shadow_technique_follows_the_light_type_enum() {
        use byroredux_core::ecs::LIGHT_FLAG_SHADOW_MASK;
        let shadowed = canonical_light_shadow_flags(GameKind::Starfield, 0, 1);
        assert_eq!(shadowed, LIGHT_FLAG_SHADOW_SPOTLIGHT);
        let non_shadow = canonical_light_shadow_flags(GameKind::Starfield, 0, 2);
        assert_eq!(non_shadow, 0);
        let omni = canonical_light_shadow_flags(GameKind::Starfield, LIGHT_FLAG_SHADOW_MASK, 0);
        assert_eq!(omni, LIGHT_FLAG_SHADOW_MASK);
    }

    /// The falloff sentinel resolves per layout generation: pre-Skyrim
    /// (`Oblivion` / `Fallout3NV`) defaults to the quadratic `k = 2` the
    /// shared doc contract names, Skyrim+ to `k = 1`; an authored value
    /// always wins. A shared 1.0 fallback reaching pre-Skyrim content was
    /// the mismatch (`light_anim.rs`'s own sibling doc called this out).
    #[test]
    fn falloff_exponent_sentinel_resolves_per_layout_generation() {
        use byroredux_plugin::esm::reader::GameKind;
        assert_eq!(
            canonical_light_falloff_exponent(GameKind::Fallout3NV, 0.0),
            2.0
        );
        assert_eq!(
            canonical_light_falloff_exponent(GameKind::Oblivion, 0.0),
            2.0
        );
        assert_eq!(canonical_light_falloff_exponent(GameKind::Skyrim, 0.0), 1.0);
        assert_eq!(
            canonical_light_falloff_exponent(GameKind::Starfield, 0.0),
            1.0
        );
        // Authored values pass through on every game.
        for game in [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Starfield,
        ] {
            assert_eq!(canonical_light_falloff_exponent(game, 2.5), 2.5);
        }
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

    /// #2516 — the FLICKER path stepped its hash buckets at a hardcoded
    /// 12 Hz and never read `period_secs`, so every flickering light ran at
    /// an identical rate whatever its FNAM authored. Count zero-crossings
    /// of the modulation over a fixed window: a short authored period must
    /// produce strictly more of them than a long one.
    #[test]
    fn flicker_rate_follows_the_authored_period() {
        fn crossings(period: f32) -> usize {
            let f = flicker(LIGHT_FLAG_FLICKER, 0.25, period);
            let mut prev = flicker_intensity(7, &f, 0.0) - 1.0;
            let mut n = 0;
            // 4 s window, 1 ms resolution — fine enough to resolve 60 Hz.
            for i in 1..4000 {
                let cur = flicker_intensity(7, &f, i as f32 * 0.001) - 1.0;
                if prev.signum() != cur.signum() {
                    n += 1;
                }
                prev = cur;
            }
            n
        }

        let fast = crossings(0.25);
        let candle = crossings(0.5);
        let slow = crossings(4.0);
        assert!(
            fast > candle && candle > slow,
            "flicker rate must track the authored period: \
             0.25s={fast} 0.5s={candle} 4.0s={slow}"
        );
        // The long period must be *visibly* slower, not marginally so.
        assert!(
            slow * 4 < candle,
            "a 4 s period should flicker far slower than a 0.5 s candle: \
             {slow} vs {candle}"
        );
    }

    /// The 0.5 s vanilla Skyrim candle must still step at the 12 Hz that
    /// Phase 19 tuned by eye — `FLICKER_BUCKETS_PER_PERIOD` is chosen for
    /// exactly this, so a future retune cannot silently drift it.
    #[test]
    fn vanilla_candle_period_still_steps_at_twelve_hz() {
        let rate = FLICKER_BUCKETS_PER_PERIOD / 0.5;
        assert!(
            (rate - 12.0).abs() < 1e-6,
            "expected the tuned 12 Hz for a 0.5 s candle, got {rate}"
        );
    }

    /// A non-positive period must not divide by zero or produce a NaN
    /// intensity — `attach.rs` guarantees a positive value, but this pure
    /// function is called directly by tests and must stay total.
    #[test]
    fn flicker_survives_a_non_positive_authored_period() {
        for bad in [0.0_f32, -1.0] {
            let f = flicker(LIGHT_FLAG_FLICKER, 0.25, bad);
            let v = flicker_intensity(3, &f, 1.234);
            assert!(v.is_finite(), "period {bad} produced {v}");
        }
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
            canonical_light_shadow_flags(GameKind::Skyrim, DYNAMIC_AND_PORTAL_STRICT, 0),
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
            canonical_light_shadow_flags(GameKind::Skyrim, source, 0),
            LIGHT_FLAG_SHADOW_SPOTLIGHT
                | LIGHT_FLAG_SHADOW_HEMISPHERE
                | LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
        );
    }

    #[test]
    fn fallout3nv_zero_projection_lights_get_omnidirectional_rt_visibility() {
        use byroredux_core::ecs::{LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL, LIGHT_FLAG_SHADOW_SPOTLIGHT};
        assert_eq!(
            canonical_light_shadow_flags(GameKind::Fallout3NV, 0, 0),
            LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
            "FO3/FNV room lights commonly author no legacy projection bit; \
             leaving them at zero makes the scene's RT shadows disappear"
        );
        assert_eq!(
            canonical_light_shadow_flags(GameKind::Fallout3NV, LIGHT_FLAG_SHADOW_SPOTLIGHT, 0),
            LIGHT_FLAG_SHADOW_SPOTLIGHT,
            "an authored projection technique must remain authoritative"
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
                canonical_light_shadow_flags(game, LIGHT_FLAG_SHADOW_MASK, 0),
                LIGHT_FLAG_SHADOW_MASK,
                "{game:?} must decode the full shadow mask until a verified divergence is found"
            );
        }
    }

    /// #3987 — Starfield's `DAT2` flags word is real; its bit legend is not.
    /// The two canonicalizers therefore split, along the documented
    /// strict-animation / permissive-shadow asymmetry.
    ///
    /// This test used to be named
    /// `starfield_has_no_verified_flags_field_for_either_canonicalization`
    /// and asserted zero on both sides, encoding the premise #2251 rested on:
    /// that SF1Edit's LIGH definition leaves the flag bytes an
    /// undifferentiated `wbUnknown` block. That is contradicted by our own
    /// `DAT2` decoder's verified offset table (`{12} UInt16 Flags`) and by the
    /// shipped data — 1,575 `Starfield.esm` LIGH records, 16 distinct sparse
    /// values at `DAT2+12` (union `0x17F1`), the following u16 zero in every
    /// one. A test that asserts a false premise is worse than no test, because
    /// it makes the premise look checked.
    #[test]
    fn starfield_flags_are_strict_for_animation_and_permissive_for_shadows() {
        use byroredux_core::ecs::LIGHT_FLAG_SHADOW_MASK;
        let all_bits_set = LIGHT_FLAG_FLICKER
            | LIGHT_FLAG_FLICKER_SLOW
            | LIGHT_FLAG_PULSE
            | LIGHT_FLAG_PULSE_SLOW
            | LIGHT_FLAG_SHADOW_MASK;
        assert_eq!(
            canonical_light_animation_flags(GameKind::Starfield, all_bits_set),
            0,
            "Starfield's flicker/pulse bit positions are still unevidenced, and \
             the observed bit set does not match Skyrim's (0x02/0x04/0x08 never \
             appear, an unnamed 0x10 does) — an unverified bit must not decode \
             into motion"
        );
        assert_eq!(
            canonical_light_shadow_flags(GameKind::Starfield, all_bits_set, 0),
            LIGHT_FLAG_SHADOW_MASK,
            "Starfield must take the same permissive shadow mask as every other \
             game: its flags word is real. Zeroing it (#3987) once narrowed every \
             placed Starfield light to the conservative legacy mask; the mask is \
             FULL for every light since the lighting unification, but the \
             decoded flags must still reach LightSource intact"
        );
    }

    /// The layer the zero mask was felt at. When #3987 landed,
    /// `VisibilityMask::for_legacy_projection(false)` was a conservative
    /// subset (`ARCHITECTURE`, later `| DYNAMIC_ACTOR`), so a Starfield
    /// light decoding no shadow bits cast nothing on the rest. Since the
    /// lighting unification the projection mask is `FULL` either way; the
    /// pin now guards that the decoded shadow bits survive to `LightSource`
    /// (#4557).
    #[test]
    fn a_starfield_shadow_bit_survives_into_the_projection_mask() {
        use byroredux_core::ecs::LIGHT_FLAG_SHADOW_MASK;
        let decoded = canonical_light_shadow_flags(GameKind::Starfield, LIGHT_FLAG_SHADOW_MASK, 0);
        assert_ne!(
            decoded, 0,
            "a Starfield LIGH carrying shadow bits must reach \
             LightSource::from_legacy_world_units with them intact (#3987 / \
             #4557)"
        );
    }
}
