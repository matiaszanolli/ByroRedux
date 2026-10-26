//! Unit conversion and DSP gates: the Bethesda-unit → kira-space seam, the
//! underwater low-pass filter, and the reverb-send gate.

use super::*;

/// Silence floor in decibels for the linear→dB conversion — clamps
/// non-positive / near-zero amplitudes so `log10` doesn't blow up.
pub(crate) const SILENCE_DB: f32 = -60.0;
/// Low-pass cutoff applied while the listener is submerged.
pub(crate) const UNDERWATER_CUTOFF_HZ: f64 = 900.0;
/// Cutoff parked on the filter while above water.
///
/// This value is **not** what makes the dry state transparent — the `Mix`
/// does (see [`apply_underwater_filter`]). It only shapes the brief
/// surfacing/submerging crossfade, where a partially-wet 20 kHz low-pass is
/// the natural-sounding midpoint.
///
/// Deliberately NOT raised to a "beyond Nyquist" value to force transparency
/// (#3179): kira computes `g = tan(pi * clamp(f_c/f_s, 0.0001, 0.5))`
/// (`kira-0.12.5/src/effect/filter.rs`), so any cutoff at or above half the
/// device rate pins the clamp at `0.5` and gives `tan(pi/2)` ~ 1.6e16, with
/// `a1 = 1/(1 + g*(g+k))` collapsing to ~3.7e-33. That is a numerically
/// degenerate filter, not a transparent one.
pub(crate) const ABOVE_WATER_CUTOFF_HZ: f64 = 20_000.0;

/// The global reverb send's `ReverbBuilder` parameters (#3780).
///
/// **No recorded provenance.** These three values landed with the M44
/// Phase 6 commit (`e191d9f9`) with no rationale in the code or the
/// commit message, and nothing in this tree, in kira's docs, or the
/// Gamebryo 2.3 reference establishes what a Bethesda interior reverb
/// should measure — inventing a plausible-sounding replacement trio
/// would be exactly the failure this project's no-guessing rule exists to
/// prevent. They also aren't kira's own `ReverbBuilder::default()`
/// (`feedback: 0.9, damping: 0.1, stereo_width: 1.0` — kira 0.12.5), so
/// this was a deliberate choice, just an unrecorded one. Named here
/// (unlike the bare literals they replace) purely so the next person
/// tuning interior acoustics (#847) has a greppable, revisitable baseline
/// instead of an invisible one, not because the values themselves are
/// validated against any reference. `f64` to match
/// `ReverbBuilder::feedback`/`damping`/`stereo_width`'s own `Value<f64>`
/// parameter type.
pub(crate) const REVERB_FEEDBACK: f64 = 0.85;
pub(crate) const REVERB_DAMPING: f64 = 0.6;
pub(crate) const REVERB_STEREO_WIDTH: f64 = 1.0;

/// Convert a world position from Bethesda units into the metre-scaled space
/// kira reasons in. **This is the one and only unit seam for audio.**
///
/// The engine's world space is Bethesda units (`BETHESDA_UNITS_PER_METER`,
/// 70 BU/m, declared in `byroredux_core::lighting` and already the authority
/// for physics and the renderer), so every `GlobalTransform.translation` that
/// reaches this crate is in BU. kira is metre-scaled — most visibly its
/// hardcoded `EAR_DISTANCE = 0.1` (`kira-0.12.5/src/track/sub.rs`), which is
/// 10 cm of head width and would otherwise be 10 cm *of Bethesda unit*, i.e.
/// 1.4 mm of world, collapsing the stereo image.
///
/// Converting here rather than scaling [`Attenuation`] keeps kira's ear model
/// in the right space and keeps the metre-authored attenuation constants
/// honest. Every producer that adds a new emitter inherits it for free.
///
/// #3178 — before this existed, BU went in unconverted against metre-authored
/// distances, making the effective audible radius ~1/70th of the intent:
/// `Attenuation::default()` was 2.9 cm..43 cm rather than 2 m..30 m.
pub(crate) fn bu_to_audio_space(position: Vec3) -> Vec3 {
    position / BETHESDA_UNITS_PER_METER
}

/// Add the submersion low-pass to a spatial track and return its handle.
///
/// Shared by both dispatch paths so a future change cannot land on one only —
/// the same extraction [`apply_reverb_send`] exists for (#2405), applied to
/// the duplication that grew back three lines below it (#3179).
///
/// The above-water state is a genuine **bypass**: `Mix::DRY` makes kira's
/// blend `output * sqrt(0) + input * sqrt(1)`, i.e. bit-exact passthrough at
/// every device sample rate. `FilterBuilder::default()` is `Mix::WET`, so the
/// dry state has to be set explicitly — leaving it default meant every sound
/// on dry land carried a fully-wet 20 kHz SVF (Q = 0.5), losing ~1.9 dB at
/// 10 kHz and ~3.9 dB at 15 kHz, by an amount that shifted with the output
/// device's sample rate.
pub(crate) fn apply_underwater_filter(
    track_builder: &mut SpatialTrackBuilder,
    underwater: bool,
) -> FilterHandle {
    track_builder.add_effect(
        FilterBuilder::new()
            .mode(FilterMode::LowPass)
            .cutoff(underwater_cutoff_hz(underwater))
            .mix(underwater_mix(underwater)),
    )
}

/// Cutoff for a given submersion state. Split out so the dispatch-time and
/// per-frame paths cannot disagree.
pub(crate) fn underwater_cutoff_hz(underwater: bool) -> f64 {
    if underwater {
        UNDERWATER_CUTOFF_HZ
    } else {
        ABOVE_WATER_CUTOFF_HZ
    }
}

/// Wet/dry mix for a given submersion state. `Mix::DRY` above water is what
/// makes the filter transparent; see [`apply_underwater_filter`].
pub(crate) fn underwater_mix(underwater: bool) -> Mix {
    if underwater { Mix::WET } else { Mix::DRY }
}

/// Convert a linear gameplay volume (1.0 = "as authored", 0.5 = half-loud)
/// to the decibel gain kira reasons in: `db = 20·log10(amplitude)`, clamped
/// to [`SILENCE_DB`] for non-positive amplitudes. AUD-2026-06-23-01 — was
/// inlined verbatim at three play sites.
pub(crate) fn linear_volume_to_db(volume: f32) -> f32 {
    if volume > 0.0001 {
        20.0 * volume.log10()
    } else {
        SILENCE_DB
    }
}

/// Route a fraction of `builder`'s signal to the global reverb send if
/// one exists and the level isn't muted (below [`SILENCE_DB`]).
/// `with_send` takes a Decibels-convertible f32; the f32 is treated as
/// raw dB, so `f32::NEG_INFINITY` is a clean "no reverb" sentinel.
/// AUD-2026-08-07-D5-01 — was inlined verbatim at two dispatch sites,
/// each re-expressing the `SILENCE_DB` threshold as a bare literal.
pub(crate) fn apply_reverb_send(
    mut builder: SpatialTrackBuilder,
    reverb_send: Option<&SendTrackHandle>,
    reverb_send_db: f32,
) -> SpatialTrackBuilder {
    if let Some(reverb) = reverb_send {
        if reverb_send_gate_open(reverb_send_db) {
            builder = builder.with_send(reverb.id(), reverb_send_db);
        }
    }
    builder
}

/// The audible-threshold half of [`apply_reverb_send`]'s gate, split out
/// so it's unit-testable independent of kira's `SpatialTrackBuilder`
/// (whose internal `sends` map isn't introspectable outside the kira
/// crate). Mirrors [`linear_volume_to_db`]'s [`SILENCE_DB`] floor.
pub(crate) fn reverb_send_gate_open(reverb_send_db: f32) -> bool {
    reverb_send_db.is_finite() && reverb_send_db > SILENCE_DB
}
