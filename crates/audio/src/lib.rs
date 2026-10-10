//! Audio subsystem (M44).
//!
//! 3D positional audio backed by [`kira`]. Per the 2026-05-03 priority
//! review, this is the "feels like a game" gap that converts
//! "we render Bethesda content" into "we run Bethesda content."
//! Better-than-Bethesda axis: proper reverb zones, full HRTF where
//! kira allows, no Wwise/FMOD middleware tax.
//!
//! # Phase 1 (this commit)
//!
//! - [`AudioWorld`] resource — wraps `kira::AudioManager` with a
//!   graceful-degradation fallback. Init failure (no audio device,
//!   CI, headless) leaves the inner `Option<AudioManager>` as
//!   `None`; every downstream operation no-ops cleanly so the engine
//!   doesn't refuse to boot on a server.
//! - [`AudioListener`] component — marker on the camera entity. Its
//!   `GlobalTransform` drives the per-frame listener pose update.
//! - [`AudioEmitter`] component — point source with embedded sound
//!   data + attenuation curve. Position comes from the entity's
//!   `GlobalTransform`.
//! - [`OneShotSound`] component — transient marker for "play this
//!   once and remove." Cleaned up by [`audio_system`] after dispatch.
//! - [`audio_system`] — ECS system that updates listener position and
//!   follows moving emitters,
//!   plays new emitters, and prunes finished one-shots.
//!
//! # Phase 2 (this commit)
//!
//! - [`load_sound_from_bytes`] — decode a fully-buffered audio blob
//!   (typically extracted from a Bethesda BSA) through kira's
//!   symphonia-backed `StaticSoundData::from_cursor` path. WAV + OGG
//!   covered by kira's default features.
//! - [`SoundCache`] — process-lifetime path-keyed cache of decoded
//!   `Arc<StaticSoundData>`. Repeat plays of the same SFX (footsteps,
//!   weapon fire, dialogue line) skip the decode cost entirely.
//!
//! # Phase 3 (this commit)
//!
//! - [`audio_system`] is no longer a stub — it now lazily creates a
//!   `kira::ListenerHandle` from the `AudioListener` entity's
//!   `GlobalTransform`, dispatches `OneShotSound` emitters through
//!   per-emitter `SpatialTrackHandle`s (kira's spatial sub-track
//!   model), and prunes `Stopped` sounds each tick — including
//!   removing the entity's audio-emitter components so a future
//!   pruning system can despawn the entity if it carries no other
//!   gameplay components.
//! - [`spawn_oneshot_at`] — public helper that composes the
//!   `OneShotSound + AudioEmitter + Transform + GlobalTransform`
//!   bundle on a fresh entity. The intended consumer is gameplay
//!   code (footstep timer, weapon-fire trigger, dialogue dispatcher)
//!   that owns the policy of *when* to play; this helper owns the
//!   ECS-shape of *how* to play.
//!
//! # Phase 3.5 (this commit)
//!
//! - [`AudioWorld::play_oneshot`] — fire-and-forget queue API.
//!   Gameplay code with `&World` access (a System, which can't spawn
//!   entities) writes a pending one-shot via `world.resource_mut::<
//!   AudioWorld>().play_oneshot(...)`. `audio_system` drains the
//!   queue at the start of each frame and dispatches each entry
//!   through the same spatial-sub-track path as the entity-based
//!   `OneShotSound + AudioEmitter` flow. No entity allocation
//!   required — sidesteps the "Systems can't `&mut World::spawn`"
//!   constraint that motivates this API.
//!
//! # Phase 4 (this commit)
//!
//! - `AudioEmitter.looping = true` is no longer just metadata. The
//!   dispatch path applies kira's `StaticSoundData::loop_region(..)`
//!   when the flag is set — the sound loops the full playback
//!   region indefinitely. The prune sweep notices when a looping
//!   sound's source entity has lost its `AudioEmitter` component
//!   (despawn-by-cell-unload, or explicit removal) and issues a
//!   tweened `stop()` on the kira handle; the next prune tick
//!   observes `Stopped` and drops the entry.
//!
//! # Phase 5 (this commit)
//!
//! - [`load_streaming_sound_from_bytes`] / [`load_streaming_sound_from_file`]
//!   — kira's `StreamingSoundData` lets multi-minute music play
//!   without buffering the whole decompressed PCM in memory. The
//!   bytes-overload is for BSA-extracted music; the file-overload
//!   is for loose `Data/Music/*.mp3` / `*.wav`.
//! - [`AudioWorld::play_music`] — single-slot music dispatch through
//!   the main (non-spatial) track. Overwrites any currently-playing
//!   track with a tweened fade. Music is non-positional by design:
//!   the listener doesn't move relative to the music source.
//! - [`AudioWorld::stop_music`] — explicit stop (cell exit, menu
//!   open, etc.) with a configurable fade duration.
//!
//! # Phase 6 (this commit)
//!
//! - [`AudioWorld::set_reverb_send_db`] — global reverb send level.
//!   On manager init, the audio crate creates one kira send track
//!   with a `ReverbBuilder` effect at full-wet output. Every spatial
//!   sub-track for an `AudioEmitter` or queue-driven one-shot opts
//!   into routing some signal to that send via `with_send` at
//!   construction time. The default send level is `f32::NEG_INFINITY`
//!   (silent, "reverb off") so the engine boots with no audible
//!   reverb. Cell-load logic (an interior detector that runs after
//!   `cell_loader` finishes) toggles to `-12 dB` for interiors,
//!   back to silent for exteriors. Send level changes apply to
//!   *new* sounds — already-playing sounds keep their construction-
//!   time level, which is fine for short SFX (footsteps, gunshots
//!   loop the per-frame send level naturally as new sounds replace
//!   old ones).
//!
//! # Water audio (WATAL consumer, shipped)
//!
//! - [`AudioWorld::set_underwater`] / [`AudioWorld::underwater`] — the
//!   listener's submersion state, written each frame by
//!   `byroredux::systems::water_audio_system` from the active camera's
//!   `SubmersionState`.
//! - Every spatial sub-track carries a low-pass built by
//!   `apply_underwater_filter` at construction and driven per frame by
//!   `update_underwater_filters`: `UNDERWATER_CUTOFF_HZ` (900 Hz) fully wet
//!   while submerged, and a genuine `Mix::DRY` bypass above water.
//! - Water-surface splash / ripple one-shots reach the queue path through
//!   `water_audio_system` + `WaterAudioConfig`, sourced from the
//!   `SplashEvent` / `RippleEvent` markers WATAL emits.
//!
//! # Units
//!
//! World positions are **Bethesda units** (70 BU/m); kira is metre-scaled.
//! [`bu_to_audio_space`] is the single conversion seam, applied to the
//! listener pose and to both spatial-sub-track dispatch paths.
//! [`Attenuation`] distances are authored in **metres** and must not be
//! pre-scaled by callers. See #3178.
//!
//! # Future work (not in this commit)
//!
//! Listed by name rather than phase number (Phases 4–6 above have
//! shipped; see `docs/feature-matrix.md` for authoritative status):
//!
//! - FOOT records parser (3.5b) → per-material sound lookup.
//! - REGN `incidental`/`sounds` ambient-loop selection (blocked on the
//!   `chance_raw` fixed-point scale). REGN ambient background **music** has
//!   shipped — see `byroredux/src/asset_provider/audio.rs::dispatch_region_ambient_music`.
//! - MUSC + hardcoded music routing with crossfade.
//! - Per-cell acoustic reverb zones (kira's `ReverbBuilder`) keyed off
//!   cell acoustics; raycast occlusion attenuation.

use byroredux_core::ecs::Resource;
use byroredux_core::ecs::components::{GlobalTransform, Transform};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_core::lighting::BETHESDA_UNITS_PER_METER;
use glam::Vec3;
use kira::effect::filter::{FilterBuilder, FilterHandle, FilterMode};
use kira::effect::reverb::ReverbBuilder;
use kira::listener::ListenerHandle;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
use kira::sound::{FromFileError, PlaybackState};
use kira::track::{SendTrackBuilder, SendTrackHandle, SpatialTrackBuilder, SpatialTrackHandle};
use kira::{AudioManager, AudioManagerSettings, Capacities, DefaultBackend, Mix, Tween};
use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

/// Silence floor in decibels for the linear→dB conversion — clamps
/// non-positive / near-zero amplitudes so `log10` doesn't blow up.
const SILENCE_DB: f32 = -60.0;
/// Low-pass cutoff applied while the listener is submerged.
const UNDERWATER_CUTOFF_HZ: f64 = 900.0;
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
const ABOVE_WATER_CUTOFF_HZ: f64 = 20_000.0;

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
const REVERB_FEEDBACK: f64 = 0.85;
const REVERB_DAMPING: f64 = 0.6;
const REVERB_STEREO_WIDTH: f64 = 1.0;

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
fn bu_to_audio_space(position: Vec3) -> Vec3 {
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
fn apply_underwater_filter(
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
fn underwater_cutoff_hz(underwater: bool) -> f64 {
    if underwater {
        UNDERWATER_CUTOFF_HZ
    } else {
        ABOVE_WATER_CUTOFF_HZ
    }
}

/// Wet/dry mix for a given submersion state. `Mix::DRY` above water is what
/// makes the filter transparent; see [`apply_underwater_filter`].
fn underwater_mix(underwater: bool) -> Mix {
    if underwater { Mix::WET } else { Mix::DRY }
}

/// Convert a linear gameplay volume (1.0 = "as authored", 0.5 = half-loud)
/// to the decibel gain kira reasons in: `db = 20·log10(amplitude)`, clamped
/// to [`SILENCE_DB`] for non-positive amplitudes. AUD-2026-06-23-01 — was
/// inlined verbatim at three play sites.
fn linear_volume_to_db(volume: f32) -> f32 {
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
fn apply_reverb_send(
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
fn reverb_send_gate_open(reverb_send_db: f32) -> bool {
    reverb_send_db.is_finite() && reverb_send_db > SILENCE_DB
}

// Re-export the kira types downstream crates need so they can hold
// `Arc<StaticSoundData>` (in `Resource`s, components, etc.) without
// pulling kira as a direct dependency. The audio crate is the canon
// owner of the audio-engine surface.
pub use kira::Frame;
pub use kira::sound::static_sound::{
    StaticSoundData as Sound, StaticSoundSettings as SoundSettings,
};

// Headroom over kira's defaults. Each active spatial sound (entity-
// path one-shot, queue-path one-shot, looping emitter) holds one
// spatial sub-track for the duration of playback; populated Bethesda
// interiors (FO4 Diamond City Market sits ~400 emitters in vanilla)
// blow past kira's default 128 cap once Phase 3.5b FOOT records and
// Phase 4 REGN ambients land. 512 + 32 give comfortable headroom and
// still fit on a couple kilobytes of manager state. Pinned here so
// the cap is one-line-greppable; see issue #842 for the failure
// mode the bump prevents (silent-drop on `ResourceLimitReached`).
pub(crate) const SUB_TRACK_CAPACITY: usize = 512;
pub(crate) const SEND_TRACK_CAPACITY: usize = 32;

/// One currently-playing sound. The `track` field keeps the spatial
/// sub-track alive — dropping it would tear down playback even if the
/// `handle` is still ticking — and carries the position updates for
/// entity-backed emitters (#3086) and entity-anchored queue plays
/// (#5410). The three coupling modes:
///
/// - [`SoundSource::Detached`] — fire-and-forget queue plays
///   ([`AudioWorld::play_oneshot`], Phase 3.5): the position is by
///   contract the one captured at queue time, nothing follows it, and
///   the sound runs to natural termination (no stop path, by design).
/// - [`SoundSource::Emitter`] — the entity-based `OneShotSound +
///   AudioEmitter` flow (Phase 3): the follow pass tracks the entity's
///   `GlobalTransform` (#3086), the prune pass stops the sound when the
///   emitter component goes away, and completion removes the
///   `AudioEmitter` so a downstream cleanup system can despawn the
///   entity.
/// - [`SoundSource::Anchored`] (#5410) — an entity-anchored queue play
///   ([`AudioWorld::play_oneshot_following`]): the follow pass tracks
///   the entity exactly like `Emitter`, but there is no emitter
///   component lifecycle — the sound plays to natural termination
///   unless [`AudioWorld::stop_sounds_for`] stops it (dialogue voice:
///   the NPC carries no `AudioEmitter`, and its line must survive SFX
///   emitter churn while still tracking a walking speaker).
///
/// Whether the underlying kira sound is looping (set via
/// `loop_region(..)` at dispatch) is decided at the `Pending` /
/// `AudioEmitter` layer; `ActiveSound` itself doesn't need to carry
/// that bit post-#858 since the prune sweep no longer branches on it.
struct ActiveSound {
    source: SoundSource,
    handle: StaticSoundHandle,
    track: SpatialTrackHandle,
    /// The position (audio-space metres) the spatial track was last
    /// placed at — initialised from the dispatch position so a
    /// stationary emitter pushes zero per-tick commands, and compared
    /// by [`sync_emitter_positions`] so a moving source pushes one
    /// `set_position` only on ticks where it actually moved (#3086).
    last_position: Option<Vec3>,
    /// Low-pass control kept with the spatial track so water transitions can
    /// be applied to sounds that are already playing.
    underwater_filter: FilterHandle,
    underwater: bool,
    /// Fade-out duration captured from `AudioEmitter.unload_fade_ms` at
    /// dispatch time. Read by `prune_stopped_sounds` when the source
    /// entity loses its emitter component (cell unload). Applies to
    /// looping AND non-looping post-#858 / SAFE-23 — one-shots
    /// usually terminate naturally before the fade is needed, but
    /// despawn-mid-playback routes through the same tween. See #845.
    unload_fade_ms: f32,
    /// Set to `true` once `prune_stopped_sounds` has issued the
    /// fade-out `stop` call for this active sound. The handle's
    /// `state()` won't report `Stopped` until the fade completes, so
    /// without this flag the prune walk would re-mark the same entry
    /// every tick during the fade window — kira treats repeated
    /// `stop` as idempotent in effect, but the redundant ringbuf
    /// commands and re-walk cost are unnecessary. Becomes more
    /// visible if the fade duration is tuned up. See #844.
    stop_issued: bool,
}

/// How an [`ActiveSound`] couples to an entity — see the struct doc for
/// the full contract of each mode (#5410 split the old
/// `Option<EntityId>` into three modes because entity-following and
/// emitter-lifecycle are independent: a dialogue voice needs the first
/// and must not have the second).
enum SoundSource {
    /// Queue-driven fire-and-forget: position frozen at queue time.
    Detached,
    /// `OneShotSound + AudioEmitter` dispatch: follows the entity, stops
    /// with the emitter component, removes it on completion.
    Emitter(EntityId),
    /// Entity-anchored queue play: follows the entity, stops only via
    /// [`AudioWorld::stop_sounds_for`], no emitter lifecycle.
    Anchored(EntityId),
}

impl SoundSource {
    /// The followed entity, if this sound has one (both `Emitter` and
    /// `Anchored` track a `GlobalTransform`).
    fn followed_entity(&self) -> Option<EntityId> {
        match self {
            SoundSource::Detached => None,
            SoundSource::Emitter(entity) | SoundSource::Anchored(entity) => Some(*entity),
        }
    }

    /// Whether the prune pass's emitter-presence coupling applies (only
    /// `Emitter` — an `Anchored` voice must survive SFX emitter churn on
    /// the same NPC).
    fn emitter_coupled(&self) -> Option<EntityId> {
        match self {
            SoundSource::Emitter(entity) => Some(*entity),
            SoundSource::Detached | SoundSource::Anchored(_) => None,
        }
    }
}

/// One-shot queued through [`AudioWorld::play_oneshot`] (detached) or
/// [`AudioWorld::play_oneshot_following`] (entity-anchored). Drained and
/// dispatched by `audio_system` at the start of each frame. Lives in
/// `AudioWorld` rather than as ECS components so callers without
/// `&mut World` (Systems) can still trigger sounds.
struct PendingOneShot {
    sound: Arc<StaticSoundData>,
    position: Vec3,
    attenuation: Attenuation,
    volume: f32,
    /// #5410 — `Detached` for plain `play_oneshot`; `Anchored(entity)`
    /// for a voice line whose speaker may walk while talking.
    source: SoundSource,
}

/// Resource holding the `kira::AudioManager` + listener + active-sound
/// tracking for the whole engine.
///
/// Wrapping the manager in an `Option` is the headless / no-device
/// fallback: when `AudioWorld::new()` fails to acquire an audio device
/// (CI, server, broken sound driver), the inner is `None` and every
/// system call short-circuits. Booting the engine never fails because
/// audio is unavailable — that would be hostile to operators running
/// the engine for testing in environments without a sound card.
///
/// Field-drop order matters: `active_sounds` (which owns
/// `SpatialTrackHandle`s) drops before `listener` drops before
/// `manager` drops. Rust struct-field drop order is declaration order
/// — the field declarations below match that, top-to-bottom.
pub struct AudioWorld {
    /// Currently-playing one-shot sounds. Cleaned up per-frame as
    /// kira reports `PlaybackState::Stopped`.
    active_sounds: Vec<ActiveSound>,
    /// Queued fire-and-forget one-shots from [`Self::play_oneshot`].
    /// Drained at the start of each `audio_system` tick so callers
    /// who can't allocate entities (Systems) can still trigger
    /// sounds. Phase 3.5. Stored as a `VecDeque` so the cap-eviction
    /// path in `play_oneshot` is O(1) `pop_front` rather than O(n)
    /// `Vec::remove(0)` shift-down. See #852.
    pending_oneshots: VecDeque<PendingOneShot>,
    /// Monotonic count of one-shot dispatch requests, including headless
    /// calls discarded before queueing. Used by device-less smoke gates.
    oneshots_requested: u64,
    /// Single-slot music handle (Phase 5). Music is non-spatial —
    /// it routes through the main track, not a spatial sub-track.
    /// Calling `play_music` while a track is already playing fades
    /// the old one out and the new one in (crossfade).
    music: Option<StreamingSoundHandle<FromFileError>>,
    /// Reverb send track (Phase 6). Created on manager init when
    /// the audio device is available. Each spatial sub-track opts
    /// into routing signal here via `with_send` at construction
    /// time; the per-track send level is `reverb_send_db` at the
    /// moment the track is built. `None` when the manager itself is
    /// inactive or send-track creation failed.
    reverb_send: Option<SendTrackHandle>,
    /// Per-new-spatial-track reverb send level in dB. Default
    /// `f32::NEG_INFINITY` = no reverb. Cell-load logic flips this
    /// to `-12.0` (subtle) for interior cells.
    reverb_send_db: f32,
    /// Lazily-created kira listener — the entity whose
    /// `GlobalTransform` drives spatial attenuation. Created on the
    /// first frame an `AudioListener` is found in the World.
    listener: Option<ListenerHandle>,
    /// kira manager. `None` means no audio device was acquired; every
    /// audio operation no-ops.
    manager: Option<AudioManager<DefaultBackend>>,
    /// One-shot debounce for the multi-`AudioListener` diagnostic
    /// (#843). Set to `true` the first frame `sync_listener_pose`
    /// observes more than one entity carrying the marker; suppresses
    /// per-frame log spam during third-person camera transitions or
    /// fly-cam swaps where two listener entities briefly coexist.
    multi_listener_warned: bool,
    /// Whether the active listener is head-submerged. The engine water
    /// system updates this before `audio_system` dispatches new sounds.
    underwater: bool,
    /// Commands pushed by [`sync_emitter_positions`] over the process
    /// lifetime (#3086). One push per entity-backed emitter per tick it
    /// actually moved — the per-frame cost of the emitter follow pass
    /// is this counter's delta, and a stationary emitter pushes zero.
    /// Surfaced for tests and smoke gates the same way
    /// [`Self::oneshots_requested`] is.
    emitter_position_updates: u64,
}

impl Default for AudioWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioWorld {
    /// Construct an `AudioWorld` from a fresh `AudioManager`. On
    /// failure (no audio device, denied permissions, dev-environment
    /// without `cpal`-supported backend), logs at WARN and returns an
    /// audioless world that no-ops cleanly.
    pub fn new() -> Self {
        let settings = AudioManagerSettings::<DefaultBackend> {
            capacities: Capacities {
                sub_track_capacity: SUB_TRACK_CAPACITY,
                send_track_capacity: SEND_TRACK_CAPACITY,
                ..Capacities::default()
            },
            ..Default::default()
        };
        let mut manager = match AudioManager::<DefaultBackend>::new(settings) {
            Ok(manager) => {
                log::info!(
                    "M44 Phase 1: AudioManager initialised (default backend, \
                     sub_track_capacity={SUB_TRACK_CAPACITY}, \
                     send_track_capacity={SEND_TRACK_CAPACITY})"
                );
                Some(manager)
            }
            Err(e) => {
                log::warn!(
                    "M44 Phase 1: AudioManager init failed ({e}); engine continues \
                     without audio. This is expected in headless/CI environments and \
                     on systems without a working audio device."
                );
                None
            }
        };
        // Phase 6: create a send track with a reverb effect at full
        // wet output. Per-spatial-track send levels (in dB) control
        // how much of each sound goes through. Default-disabled at
        // f32::NEG_INFINITY so engine boots silent-of-reverb until
        // a cell-load flips the toggle for interiors.
        let reverb_send = manager.as_mut().and_then(|mgr| {
            let builder = SendTrackBuilder::new().with_effect(
                ReverbBuilder::new()
                    .feedback(REVERB_FEEDBACK)
                    .damping(REVERB_DAMPING)
                    .stereo_width(REVERB_STEREO_WIDTH)
                    .mix(Mix::WET),
            );
            match mgr.add_send_track(builder) {
                Ok(handle) => {
                    log::info!("M44 Phase 6: reverb send track created (initially silent)");
                    Some(handle)
                }
                Err(e) => {
                    log::warn!("M44 Phase 6: add_send_track for reverb failed: {e}");
                    None
                }
            }
        });
        Self {
            reverb_send,
            manager,
            ..Self::headless()
        }
    }

    /// Construct without contacting an audio device or starting backend threads.
    /// Useful for dedicated servers and hardware-independent gameplay tests.
    /// Playback is discarded, just as when device initialization fails; listener
    /// and reverb configuration can still be updated by gameplay systems.
    pub fn headless() -> Self {
        Self {
            active_sounds: Vec::new(),
            pending_oneshots: VecDeque::new(),
            oneshots_requested: 0,
            music: None,
            reverb_send: None,
            reverb_send_db: f32::NEG_INFINITY,
            listener: None,
            manager: None,
            multi_listener_warned: false,
            underwater: false,
            emitter_position_updates: 0,
        }
    }

    /// True when an `AudioManager` was successfully acquired. Systems
    /// can early-exit on `false` without touching the inner.
    pub fn is_active(&self) -> bool {
        self.manager.is_some()
    }

    /// Borrow the inner `AudioManager` mutably. Returns `None` if
    /// audio init failed; callers must handle that case.
    pub fn manager_mut(&mut self) -> Option<&mut AudioManager<DefaultBackend>> {
        self.manager.as_mut()
    }

    /// Number of one-shot sounds currently tracked as active. Useful
    /// for telemetry — a runaway count signals a pruning regression.
    pub fn active_sound_count(&self) -> usize {
        self.active_sounds.len()
    }

    /// `set_position` commands pushed to entity-backed emitters'
    /// spatial tracks over the process lifetime (#3086). A stationary
    /// emitter pushes zero per tick; one push per moving emitter per
    /// tick it moved. Zero forever on a session with moving emitters
    /// means the follow pass is not running.
    pub fn emitter_position_updates(&self) -> u64 {
        self.emitter_position_updates
    }

    /// Number of one-shots queued but not yet dispatched. Drained on
    /// each `audio_system` tick. A runaway count would signal that
    /// `audio_system` isn't running, or that the manager is inactive
    /// and queue items pile up indefinitely.
    pub fn pending_oneshot_count(&self) -> usize {
        self.pending_oneshots.len()
    }

    /// Total calls to [`Self::play_oneshot`], including calls discarded
    /// because this world has no active audio manager.
    pub fn oneshots_requested(&self) -> u64 {
        self.oneshots_requested
    }

    /// Fire-and-forget one-shot dispatch from a context that cannot
    /// allocate ECS entities (i.e., a System with `&World`). The
    /// next `audio_system` tick drains the queue and plays each
    /// pending entry through a fresh spatial sub-track at the
    /// authored position.
    ///
    /// **Drops on inactive audio (#853 / C4-NEW-01).** When the
    /// manager is `None` (headless CI, no device, init failure),
    /// `audio_system` early-returns before drain. Pre-#853 the
    /// queue still filled to its 256-entry cap, pinning ~12 KB +
    /// one `Arc<StaticSoundData>` strong-count per cached sound
    /// for the lifetime of the engine. Now we drop the call up
    /// front and the queue stays empty.
    ///
    /// When audio IS active and the system is running, the queue
    /// is bounded at 256 entries via FIFO drop-oldest as a safety
    /// net against a runaway producer (256 = ~8 s of footsteps
    /// at 32 Hz; real gameplay never approaches it).
    pub fn play_oneshot(
        &mut self,
        sound: Arc<StaticSoundData>,
        position: Vec3,
        attenuation: Attenuation,
        volume: f32,
    ) {
        self.oneshots_requested = self.oneshots_requested.saturating_add(1);
        if self.manager.is_none() {
            return;
        }
        const MAX_PENDING: usize = 256;
        if self.pending_oneshots.len() >= MAX_PENDING {
            log::warn!(
                "M44: pending one-shot queue at cap ({MAX_PENDING}); dropping oldest. \
                 audio_system may not be running, or the queue is being filled \
                 faster than it's drained."
            );
            // O(1) front-pop — `Vec::remove(0)` was O(n) shift-down
            // for the 256-element queue. See #852.
            self.pending_oneshots.pop_front();
        }
        self.pending_oneshots.push_back(PendingOneShot {
            sound,
            position,
            attenuation,
            volume,
            source: SoundSource::Detached,
        });
    }

    /// #5410 — as [`Self::play_oneshot`], but the dispatched sound is
    /// ANCHORED to `entity`: the per-tick follow pass repositions its
    /// spatial track at the entity's `GlobalTransform`, so a speaker
    /// who walks while talking (force-greet approach, followers, a
    /// package resuming after Goodbye) keeps voicing from where they
    /// are, not where the line began. Unlike the `AudioEmitter` flow
    /// there is no emitter component lifecycle — the sound plays to
    /// natural termination unless [`Self::stop_sounds_for`] stops it,
    /// so ambient SFX emitter churn on the same entity cannot truncate
    /// a line. `position` is still required (the dispatch-time anchor
    /// and the fallback if the entity's transform is absent on a tick).
    pub fn play_oneshot_following(
        &mut self,
        entity: EntityId,
        sound: Arc<StaticSoundData>,
        position: Vec3,
        attenuation: Attenuation,
        volume: f32,
    ) {
        self.oneshots_requested = self.oneshots_requested.saturating_add(1);
        if self.manager.is_none() {
            return;
        }
        const MAX_PENDING: usize = 256;
        if self.pending_oneshots.len() >= MAX_PENDING {
            log::warn!(
                "M44: pending one-shot queue at cap ({MAX_PENDING}); dropping oldest. \
                 audio_system may not be running, or the queue is being filled \
                 faster than it's drained."
            );
            self.pending_oneshots.pop_front();
        }
        self.pending_oneshots.push_back(PendingOneShot {
            sound,
            position,
            attenuation,
            volume,
            source: SoundSource::Anchored(entity),
        });
    }

    /// #5410 — fade out every active sound coupled to `entity` (both the
    /// emitter-dispatched SFX and the anchored queue plays, i.e. a
    /// speaker's voice line) and drop any of its still-queued segments,
    /// returning how many were stopped or dequeued. This is the stop hook
    /// the queue path never had: conversation close, topic change, and
    /// actor death route their audio teardown through here instead of
    /// letting a multi-segment line keep playing across a door or cell
    /// transition at stale world coordinates.
    pub fn stop_sounds_for(&mut self, entity: EntityId, fade_ms: f32) -> usize {
        // Segments still waiting for dispatch are removed outright — a
        // fade applies to playing handles, not queue entries.
        let before = self.pending_oneshots.len();
        self.pending_oneshots.retain(|pending| {
            !matches!(pending.source, SoundSource::Anchored(e) | SoundSource::Emitter(e) if e == entity)
        });
        let mut stopped = before - self.pending_oneshots.len();
        let tween = Tween {
            start_time: kira::StartTime::Immediate,
            duration: Duration::from_secs_f32(fade_ms.max(0.0) / 1000.0),
            easing: kira::Easing::Linear,
        };
        for sound in &mut self.active_sounds {
            if sound.stop_issued {
                continue;
            }
            if sound.source.followed_entity() == Some(entity) {
                sound.handle.stop(tween);
                sound.stop_issued = true;
                stopped += 1;
            }
        }
        stopped
    }

    /// **Phase 5**: play a streaming sound through the main track.
    /// Music is non-spatial by design — it shouldn't attenuate with
    /// player position the way a campfire's crackle does. Volume
    /// is linear amplitude (1.0 = nominal); `fade_in_secs` controls
    /// the kira tween used to fade in (and to fade out any existing
    /// track being replaced). `looping` sets kira's `loop_region` to
    /// the whole track (`0.0..`) so playback continues indefinitely
    /// instead of stopping after one pass through — kira's own
    /// default is `loop_region: None` (#3775 / AUD-2026-08-30-D4-01):
    /// with no continuation mechanism at all, a non-looping track
    /// plays exactly once and then goes silent until something
    /// re-dispatches it.
    ///
    /// No-op when the manager is inactive (returns silently). When
    /// active and a track is already playing, the existing handle
    /// is told to fade out over `fade_in_secs` and replaced — the
    /// fade-in of the new track and fade-out of the old overlap as
    /// a natural crossfade.
    pub fn play_music(
        &mut self,
        streaming_sound: StreamingSoundData<FromFileError>,
        volume: f32,
        fade_in_secs: f32,
        looping: bool,
    ) {
        let Some(mgr) = self.manager.as_mut() else {
            return;
        };
        let fade = Tween {
            start_time: kira::StartTime::Immediate,
            duration: Duration::from_secs_f32(fade_in_secs.max(0.0)),
            easing: kira::Easing::Linear,
        };
        // Fade out any current track over the same duration so the
        // two overlap into a crossfade.
        if let Some(existing) = self.music.as_mut() {
            existing.stop(fade);
        }
        let db = linear_volume_to_db(volume);
        let configured = streaming_sound.volume(db).fade_in_tween(Some(fade));
        let configured = if looping {
            configured.loop_region(0.0..)
        } else {
            configured
        };
        match mgr.play(configured) {
            Ok(handle) => {
                self.music = Some(handle);
            }
            Err(e) => {
                log::warn!("M44 Phase 5: play_music failed: {e}");
                self.music = None;
            }
        }
    }

    /// **Phase 5**: stop the currently-playing music with a fade-out.
    /// No-op when nothing is playing or when the manager is inactive.
    pub fn stop_music(&mut self, fade_out_secs: f32) {
        let Some(handle) = self.music.as_mut() else {
            return;
        };
        let fade = Tween {
            start_time: kira::StartTime::Immediate,
            duration: Duration::from_secs_f32(fade_out_secs.max(0.0)),
            easing: kira::Easing::Linear,
        };
        handle.stop(fade);
        // Drop the handle so a future play_music call doesn't see
        // a stale reference. Kira keeps the sound alive internally
        // until the fade completes.
        self.music = None;
    }

    /// True while a music handle is installed and its state has not
    /// yet reached `Stopped`. **Not** true for the whole fade-out
    /// window: `stop_music` clears the slot immediately after issuing
    /// `handle.stop(fade)`, so this reports `false` from the moment
    /// `stop_music` returns even though kira keeps rendering the fade
    /// tail (`fade_out_secs`, e.g. `REGN_AMBIENT_CROSSFADE_SECS` =
    /// 3.0 s) internally. A caller that needs to know "is the slot
    /// still audibly occupied" cannot use this alone.
    pub fn is_music_active(&self) -> bool {
        self.music
            .as_ref()
            .map(|h| !matches!(h.state(), PlaybackState::Stopped))
            .unwrap_or(false)
    }

    /// **Phase 6**: set the per-new-spatial-track reverb send level
    /// in decibels. Already-playing sounds keep their construction-
    /// time send level; the change applies to *new* sounds dispatched
    /// after the call. Use `f32::NEG_INFINITY` (or any value below
    /// ~-60 dB) to silence reverb. `-12.0` is a subtle interior
    /// reverb; `-6.0` is more pronounced; `0.0` is full wet (rare —
    /// the dry-too-wet ratio normally wants the wet attenuated).
    ///
    /// **Limitation (#847):** kira 0.12's `with_send` is build-time
    /// only on `SpatialTrackBuilder` — there is no
    /// `SpatialTrackHandle::set_send_volume`, so a level change
    /// cannot retro-apply to already-playing tracks. Long-running
    /// looping ambients (cathedral chant, generator hum, REGN
    /// wind layer) spawned *before* this call keep their construction-
    /// time send level until they're stopped and re-dispatched. For
    /// short SFX (footsteps, gunshots, dialogue lines) the level
    /// naturally refreshes as new sounds replace old ones; for long
    /// ambients, a future cell-load reverb-flip handler must re-issue
    /// each looping emitter through the dispatch path with the new
    /// send level for the change to take effect. Until that handler
    /// lands, callers should treat `set_reverb_send_db` as a "next-
    /// dispatch" knob, not a live mixer fader.
    pub fn set_reverb_send_db(&mut self, db: f32) {
        self.reverb_send_db = db;
    }

    /// Current reverb send level (Phase 6). For telemetry / tests.
    pub fn reverb_send_db(&self) -> f32 {
        self.reverb_send_db
    }

    /// Update the listener's water state. The next audio tick applies a
    /// short low-pass transition to every active spatial sound and uses the
    /// same cutoff for newly-dispatched sounds.
    pub fn set_underwater(&mut self, underwater: bool) {
        self.underwater = underwater;
    }

    /// Current listener water state, exposed for diagnostics and tests.
    pub fn underwater(&self) -> bool {
        self.underwater
    }
}

impl Resource for AudioWorld {}

/// Marker component placed on the entity representing the "ears" of
/// the world — typically the active camera. The audio system reads
/// this entity's `GlobalTransform` once per frame and updates kira's
/// listener pose so spatial-attenuated sounds reflect the player's
/// current position.
///
/// At most one entity should carry this. If multiple do, the audio
/// system uses whichever one comes first in the query iteration.
pub struct AudioListener;

impl Component for AudioListener {
    type Storage = SparseSetStorage<Self>;
}

/// Per-emitter attenuation curve bounds. Sounds within `min_distance`
/// play at full volume; sounds at or beyond `max_distance` are
/// inaudible. Linear falloff between the two — kira's spatial scene
/// supports more nuanced curves (logarithmic, custom), and we'll plumb
/// those in once perf lets us afford a custom-curve descriptor per
/// emitter.
///
/// # Units: these distances are **metres**
///
/// World positions are Bethesda units (70 BU/m); the audio boundary converts
/// them on the way in via [`bu_to_audio_space`], so producers author
/// attenuation in metres and never scale anything themselves. Do not
/// pre-multiply these by `BETHESDA_UNITS_PER_METER` — that would double the
/// conversion and leave kira's ear model in the wrong space (#3178).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attenuation {
    pub min_distance: f32,
    pub max_distance: f32,
}

impl Attenuation {
    /// Min/max as a kira `RangeInclusive`, normalized so `min <= max`.
    /// kira computes attenuation as `distance.clamp(min, max)`, and
    /// `f32::clamp` panics when `min > max` — a reversed range (e.g. from a
    /// future data-driven producer, or hand-edited content) would otherwise
    /// abort the whole audio render thread at playback, invisible to the
    /// dispatch call site. Clamp-normalize at the boundary instead. #1612.
    pub fn distance_range(&self) -> std::ops::RangeInclusive<f32> {
        let lo = self.min_distance.min(self.max_distance);
        let hi = self.min_distance.max(self.max_distance);
        lo..=hi
    }
}

impl Default for Attenuation {
    fn default() -> Self {
        // Metres (see the type docs). Chosen for Bethesda interior
        // cells: inside a 2-3m sphere it's full volume; out at 30m
        // it's gone. Footsteps and small impacts will want tighter
        // ranges; ambient loops and music want larger.
        Self {
            min_distance: 2.0,
            max_distance: 30.0,
        }
    }
}

/// Static-payload audio emitter. Holds the decoded sound data and
/// attenuation. The audio system reads the entity's `GlobalTransform`
/// every frame and keeps the spatial sub-track anchored to it — a sound
/// from a moving actor follows the actor (#3086; change-gated, so a
/// stationary emitter costs no per-tick commands).
///
/// Phase 1 ships static (fully-decoded) sounds only. Streaming
/// (for ambient music / long loops) lands in Phase 5.
pub struct AudioEmitter {
    /// Decoded sound payload. `Arc` so the same SFX can back many
    /// emitters without re-decoding.
    pub sound: Arc<StaticSoundData>,
    /// Per-emitter attenuation envelope.
    pub attenuation: Attenuation,
    /// Volume multiplier (linear amplitude, not dB) applied on top
    /// of the spatial attenuation. 1.0 = nominal authored level.
    pub volume: f32,
    /// Looping playback. Footsteps / one-shot impacts are `false`;
    /// torch crackle / distant generator hum / cell ambient is `true`.
    pub looping: bool,
    /// Fade-out duration when this emitter's source entity is despawned
    /// (cell unload, scripted teardown). Applies to looping AND non-
    /// looping sounds — pre-#858 only the looping path consulted it,
    /// leaving non-looping SFX to play out at the stale despawn pose
    /// until natural termination (50 ms – 3 s typical, audible as
    /// faint cross-cell bleed on fast interior↔interior travel).
    ///
    /// Default 10 ms matches `kira::Tween::default()` and is inaudible
    /// on short sustained ambients (campfire crackle, generator hum).
    /// Long-tailed ambients (cathedral choir, distant thunder loop)
    /// authoring 200-500 ms here avoids the faint click on cell exit
    /// the abrupt 10 ms cutoff produces. See #845 / AUD-D4-NEW-04
    /// (looping) and #858 / SAFE-23 (non-looping extension).
    ///
    /// Captured into `ActiveSound.unload_fade_ms` at dispatch time
    /// because the `AudioEmitter` component is removed from the entity
    /// as part of the despawn that triggers the prune-stop, so the
    /// prune sweep can't read the live component.
    pub unload_fade_ms: f32,
}

/// Default fade-out duration for looping emitters whose source entity
/// gets despawned. Matches `kira::Tween::default()` (10 ms linear) so
/// existing call sites that don't author `unload_fade_ms` keep their
/// pre-#845 behaviour exactly.
pub const DEFAULT_UNLOAD_FADE_MS: f32 = 10.0;

impl Component for AudioEmitter {
    type Storage = SparseSetStorage<Self>;
}

/// Transient marker — Phase 1 dispatch contract is "spawn an entity
/// with `AudioEmitter` + `OneShotSound`, the system plays it once and
/// removes the entity." This avoids needing a per-emitter playback
/// handle held inside the component (which would force `'static` on
/// the kira sound handle and complicate Drop).
pub struct OneShotSound;

impl Component for OneShotSound {
    type Storage = SparseSetStorage<Self>;
}

/// Per-frame audio update — synchronises listener pose, follows moving
/// emitters, plays new one-shots through per-emitter spatial sub-tracks,
/// prunes finished sounds. `Stage::Late` is the canonical home (after
/// transform propagation has produced final world poses for the listener
/// and every emitter).
///
/// The six passes, in body order:
///
/// 1. **Listener sync** ([`sync_listener_pose`]): locate the (single)
///    `AudioListener` entity. On first frame, lazily call
///    `manager.add_listener` with its `GlobalTransform`. On subsequent
///    frames, push pose updates through `ListenerHandle::set_position` /
///    `set_orientation`. Positions cross the BU→metre seam here
///    ([`bu_to_audio_space`]).
/// 2. **Emitter follow** ([`sync_emitter_positions`], #3086): for every
///    entity-backed active sound, push the source entity's current
///    `GlobalTransform` into its spatial sub-track so a sound emitted
///    from a moving actor stays anchored to it. Change-gated — a
///    stationary emitter pushes zero commands per tick (the counter is
///    [`AudioWorld::emitter_position_updates`]).
/// 3. **Underwater filters** ([`update_underwater_filters`]): drive every
///    live sub-track's low-pass cutoff and wet/dry mix to match
///    [`AudioWorld::underwater`], which `water_audio_system` sets from the
///    camera's `SubmersionState`.
/// 4. **Drain the pending queue** ([`AudioWorld::play_oneshot`]'s Phase 3.5
///    path): dispatch each queued entry through the spatial-sub-track path.
///    No entity allocation — this is the API for Systems, which cannot spawn.
/// 5. **Dispatch new one-shots**: for each entity carrying both
///    `OneShotSound` + `AudioEmitter`, create a spatial sub-track
///    anchored at the entity's `GlobalTransform`, play the sound on
///    that track, and remove `OneShotSound` so the dispatcher won't
///    re-trigger next frame. The `AudioEmitter` stays so callers
///    can query "is this entity still playing?" via the active list.
/// 6. **Prune stopped**: walk `active_sounds`, drop any whose handle
///    reports `PlaybackState::Stopped`. Removing the entity's
///    `AudioEmitter` lets a downstream cleanup system (or the cell
///    unloader) despawn it without coupling to audio state.
pub fn audio_system(world: &World, _dt: f32) {
    let Some(mut audio_world) = world.try_resource_mut::<AudioWorld>() else {
        return;
    };
    if !audio_world.is_active() {
        return;
    }

    sync_listener_pose(world, &mut audio_world);
    sync_emitter_positions(world, &mut audio_world);
    update_underwater_filters(&mut audio_world);
    drain_pending_oneshots(&mut audio_world);
    dispatch_new_oneshots(world, &mut audio_world);
    prune_stopped_sounds(world, &mut audio_world);
}

/// Push every entity-backed active sound's spatial sub-track to its
/// source entity's current `GlobalTransform` (#3086).
///
/// Pre-fix, an emitter's position was captured once at dispatch and
/// never refreshed — the `AudioEmitter` docstring promised a per-frame
/// update the system never performed, so footsteps, weapon fire and
/// looping ambients on moving actors (or vehicles) detached from their
/// source. The listener half was always updated per frame, which made
/// the failure directional and easy to misread as a listener-pose bug.
///
/// Queue-driven detached sounds are fire-and-forget by contract — there
/// is no entity to follow — and music is non-spatial, so neither is
/// touched here. `Anchored` queue plays (#5410, dialogue voice) follow
/// exactly like emitters. An entity that has been despawned or lost its
/// `GlobalTransform` mid-playback is skipped; the prune sweep owns the
/// `Emitter` half's termination, `stop_sounds_for` the `Anchored` half's.
///
/// Per-tick cost: one `GlobalTransform` lookup per entity-backed
/// active sound plus one kira `set_position` command **only on ticks
/// where the source actually moved** — the dispatch position seeds
/// `ActiveSound::last_position`, so a stationary emitter (the vast
/// majority: torches, ambient loops, machinery) costs a comparison and
/// nothing else. The listener update above remains unconditional, as it
/// always was.
fn sync_emitter_positions(world: &World, audio_world: &mut AudioWorld) {
    let Some(gt_q) = world.query::<GlobalTransform>() else {
        return;
    };
    for active in &mut audio_world.active_sounds {
        let Some(entity) = active.source.followed_entity() else {
            continue;
        };
        let Some(gt) = gt_q.get(entity) else {
            continue;
        };
        let position = bu_to_audio_space(gt.translation);
        if active.last_position == Some(position) {
            continue;
        }
        active.track.set_position(position, Tween::default());
        active.last_position = Some(position);
        audio_world.emitter_position_updates += 1;
    }
}

/// Drive every live sub-track's low-pass to match the listener's submersion
/// state. Both the cutoff **and** the wet/dry mix are tweened: the mix is what
/// actually engages and bypasses the filter (#3179), the cutoff shapes the
/// crossfade in between.
fn update_underwater_filters(audio_world: &mut AudioWorld) {
    let underwater = audio_world.underwater;
    let cutoff = underwater_cutoff_hz(underwater);
    let mix = underwater_mix(underwater);
    for active in &mut audio_world.active_sounds {
        if active.underwater == underwater {
            continue;
        }
        active
            .underwater_filter
            .set_cutoff(cutoff, Tween::default());
        active.underwater_filter.set_mix(mix, Tween::default());
        active.underwater = underwater;
    }
}

/// Find the (first) `AudioListener` entity in the world, read its
/// `GlobalTransform`, and either lazy-create the kira listener or
/// push a pose update through the existing handle.
///
/// **Listener handle reuse contract (#849):** the kira listener
/// handle (`audio_world.listener`) is created lazily on the first
/// frame an `AudioListener` is observed and **never cleared**.
/// When the entity carrying the marker is despawned this function
/// early-returns at the first `iter.next()`; on the next respawn
/// (third-person camera transition, fly-cam swap, save-load cycle)
/// the existing handle's pose is updated rather than a fresh
/// `add_listener` call. This is intentional: kira's
/// `listener_capacity` is 8 (`kira-0.12.5`'s `ManagerSettings`
/// default), so a "clear on missing entity → re-add on respawn" simplification
/// would burn through that capacity on a bursty
/// debug-fly-cam-destroy-create loop and lock out future spawns.
/// Future maintainers must keep the `listener` field sticky across
/// entity churn.
fn sync_listener_pose(world: &World, audio_world: &mut AudioWorld) {
    let listener_entity = {
        let Some(q) = world.query::<AudioListener>() else {
            return;
        };
        // Diagnose multi-listener scenarios on the *first* frame the
        // count exceeds 1, then debounce so third-person camera
        // transitions / fly-cam swaps don't spam the log per-frame
        // for the brief window where two listener entities coexist.
        // The crate docstring on `AudioListener` documents the
        // "first wins" iteration policy; this surfaces it. See #843.
        let mut iter = q.iter();
        let Some((entity, _)) = iter.next() else {
            return;
        };
        if iter.next().is_some() && !audio_world.multi_listener_warned {
            // We've already pulled two; count remaining for an
            // accurate total in the warn message.
            let extra = iter.count();
            let total = 2 + extra;
            log::warn!(
                "M44: multiple AudioListener entities found ({total}); \
                 using whichever the query iteration produced first \
                 ({entity:?}). Cell-load / fly-cam swap usually leaves \
                 only the active camera tagged — check for a stale \
                 marker on a despawning entity."
            );
            audio_world.multi_listener_warned = true;
        }
        entity
    };
    let pose = {
        let Some(q) = world.query::<GlobalTransform>() else {
            return;
        };
        let Some(gt) = q.get(listener_entity) else {
            return;
        };
        (gt.translation, gt.rotation)
    };
    if audio_world.listener.is_none() {
        let Some(mgr) = audio_world.manager.as_mut() else {
            return;
        };
        match mgr.add_listener(bu_to_audio_space(pose.0), pose.1) {
            Ok(handle) => {
                log::info!(
                    "M44 Phase 3: kira listener created at ({:.1},{:.1},{:.1})",
                    pose.0.x,
                    pose.0.y,
                    pose.0.z,
                );
                audio_world.listener = Some(handle);
            }
            Err(e) => {
                log::warn!("M44 Phase 3: add_listener failed: {e}");
            }
        }
    } else if let Some(handle) = audio_world.listener.as_mut() {
        handle.set_position(bu_to_audio_space(pose.0), Tween::default());
        handle.set_orientation(pose.1, Tween::default());
    }
}

/// Drain the `play_oneshot` queue and dispatch each entry through a
/// fresh spatial sub-track. Entity-less; queued items have no
/// associated `EntityId`. Logs at WARN if a single tick drains more
/// than 32 items — that's footstep-tempo gone wrong, audible signal
/// that something upstream is firing per-frame instead of per-stride.
fn drain_pending_oneshots(audio_world: &mut AudioWorld) {
    let Some(listener_id) = audio_world.listener.as_ref().map(|l| l.id()) else {
        return;
    };
    if audio_world.pending_oneshots.is_empty() {
        return;
    }
    // Manager-active gate moves *before* the drain below (#851).
    // Pre-fix, an unconditional drain ran first, so on a hypothetical
    // `manager = None` re-entry the drained items would be silently
    // dropped — the `// Inactive — queue cleared` branch below was
    // reachable in theory and would have lost the queued one-shots
    // forever. In practice the parent `audio_system` early-returns at
    // `is_active()` before calling this helper, so the manager is
    // always `Some` here, but the defensive ordering keeps the
    // contract local: we only consume the queue once we know we can
    // dispatch its contents.
    let Some(mgr) = audio_world.manager.as_mut() else {
        return;
    };
    if audio_world.pending_oneshots.len() > 32 {
        log::warn!(
            "M44 Phase 3.5: drained {} pending one-shots in one tick — \
             upstream system is firing too fast (footstep stride, weapon \
             rate-of-fire, dialogue queue?)",
            audio_world.pending_oneshots.len()
        );
    }
    // #3521 (AUD-2026-08-27-D1-01) — `VecDeque::drain(..)` in place,
    // not the old swap-in-a-fresh-default-then-consume-by-value shape.
    // That prior approach replaced the live queue with a fresh,
    // zero-capacity default and dropped the old (capacity-holding) one
    // at the end of this function, so every tick that drained anything
    // paid an allocate+free pair on the next `play_oneshot` push.
    // `drain(..)` empties `pending_oneshots` in place and keeps its
    // allocated capacity for the next fill — same class of fix as
    // `#932` / `#3059` / `#3257`. `mgr` (borrowed from
    // `audio_world.manager`) and the field accesses below
    // (`reverb_send`, `active_sounds`, `underwater`) are all disjoint
    // fields from `pending_oneshots`, so the borrow checker accepts
    // draining it in place across the loop.
    //
    // NOTE for future editors: this comment deliberately never spells
    // out the old approach's own method-path text — the sibling
    // regression test in `tests.rs` scans this function's source for
    // exactly that substring, and writing it here would make the test
    // match its own describing comment instead of real code.
    for p in audio_world.pending_oneshots.drain(..) {
        let track_builder = SpatialTrackBuilder::new().distances(p.attenuation.distance_range());
        // Phase 6: route a fraction of this track's signal to the
        // global reverb send, if one exists and the level isn't muted.
        let track_builder = apply_reverb_send(
            track_builder,
            audio_world.reverb_send.as_ref(),
            audio_world.reverb_send_db,
        );
        let mut track_builder = track_builder;
        let underwater = audio_world.underwater;
        let underwater_filter = apply_underwater_filter(&mut track_builder, underwater);
        let mut track = match mgr.add_spatial_sub_track(
            listener_id,
            bu_to_audio_space(p.position),
            track_builder,
        ) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("M44 Phase 3.5: add_spatial_sub_track failed: {e}");
                continue;
            }
        };
        let db = linear_volume_to_db(p.volume);
        let sound = (*p.sound).clone().volume(db);
        let handle = match track.play(sound) {
            Ok(h) => h,
            Err(e) => {
                log::warn!("M44 Phase 3.5: track.play (queue) failed: {e}");
                continue;
            }
        };
        audio_world.active_sounds.push(ActiveSound {
            source: p.source,
            handle,
            track,
            // `Detached`: the track sits at the queued position for its
            // whole life (nothing to follow), so the seeded value is
            // final (#3086). `Anchored` (#5410): the dispatch position
            // anchors the track and the follow pass corrects it on the
            // first tick the speaker has moved.
            last_position: Some(bu_to_audio_space(p.position)),
            underwater_filter,
            underwater,
            // Queue-driven sounds carry no emitter lifecycle — `Detached`
            // runs to natural termination as `play_oneshot`'s documented
            // contract requires, and `Anchored` stops only through
            // `stop_sounds_for` (#5410). The prune sweep's
            // emitter-presence check skips both, and
            // `unload_fade_ms` is never consulted on this branch.
            unload_fade_ms: DEFAULT_UNLOAD_FADE_MS,
            // Queue-driven sounds never re-enter the prune sweep's
            // stop branch, so this flag stays `false` until an explicit
            // `stop_sounds_for` flips it. See #844 / #858 / #5410.
            stop_issued: false,
        });
    }
}

/// Iterate `OneShotSound + AudioEmitter` entities; for each, create
/// a spatial sub-track anchored at the entity's world position, play
/// the sound on that track, and remove `OneShotSound` so the entity
/// isn't re-dispatched next frame. The track + handle land in
/// `active_sounds` so they outlive the helper-function scope.
fn dispatch_new_oneshots(world: &World, audio_world: &mut AudioWorld) {
    let Some(listener_id) = audio_world.listener.as_ref().map(|l| l.id()) else {
        // No listener yet — defer dispatch. The next frame's
        // `sync_listener_pose` will create it; one-shots queued this
        // frame will dispatch then.
        return;
    };

    // Snapshot the (entity, sound, attenuation, volume, position) tuple
    // for every new one-shot before mutating storages. Locks held
    // across `manager_mut().add_spatial_sub_track` would otherwise
    // collide with the per-emitter component reads.
    struct Pending {
        entity: EntityId,
        sound: Arc<StaticSoundData>,
        attenuation: Attenuation,
        volume: f32,
        position: Vec3,
        looping: bool,
        unload_fade_ms: f32,
    }
    let mut pending: Vec<Pending> = Vec::new();
    {
        let Some(oneshot_q) = world.query::<OneShotSound>() else {
            return;
        };
        let Some(emitter_q) = world.query::<AudioEmitter>() else {
            return;
        };
        let Some(gt_q) = world.query::<GlobalTransform>() else {
            return;
        };
        for (entity, _) in oneshot_q.iter() {
            let Some(emitter) = emitter_q.get(entity) else {
                continue;
            };
            let Some(gt) = gt_q.get(entity) else {
                continue;
            };
            pending.push(Pending {
                entity,
                sound: Arc::clone(&emitter.sound),
                attenuation: emitter.attenuation,
                volume: emitter.volume,
                position: gt.translation,
                looping: emitter.looping,
                unload_fade_ms: emitter.unload_fade_ms,
            });
        }
    }

    if pending.is_empty() {
        return;
    }

    let Some(mgr) = audio_world.manager.as_mut() else {
        return;
    };
    // Entities whose `OneShotSound` marker is spent this frame —
    // successes *and* failures alike (#2394 / ECS-D7-2026-08-07-01). A
    // failed one-shot is still a consumed one-shot: leaving the marker
    // on a dispatch failure re-collects the entity into `pending` every
    // subsequent frame, which at 60 Hz is one `warn!` per entity per
    // frame plus (when only `track.play` fails) a spatial sub-track
    // allocated and dropped per frame, forever. `prune_stopped_sounds`
    // can't clean up after it either: it walks `active_sounds`, and a
    // failed dispatch never pushed an `ActiveSound`.
    let mut consumed: Vec<EntityId> = Vec::with_capacity(pending.len());
    for p in pending {
        // kira's `SpatialTrackBuilder::distances` accepts a
        // `RangeInclusive<f32>` (or `(f32, f32)` / `[f32; 2]`); the
        // exclusive `..` range we use elsewhere doesn't impl
        // `Into<SpatialTrackDistances>`. The values are min..=max
        // **metres** (#3178 — the position handed to kira alongside
        // them is converted from BU by `bu_to_audio_space`), falloff
        // between is linear (kira default).
        let track_builder = SpatialTrackBuilder::new().distances(p.attenuation.distance_range());
        // Phase 6: route a fraction of this track's signal to the
        // global reverb send, if one exists and the level isn't muted.
        let track_builder = apply_reverb_send(
            track_builder,
            audio_world.reverb_send.as_ref(),
            audio_world.reverb_send_db,
        );
        let mut track_builder = track_builder;
        let underwater = audio_world.underwater;
        let underwater_filter = apply_underwater_filter(&mut track_builder, underwater);
        let mut track = match mgr.add_spatial_sub_track(
            listener_id,
            bu_to_audio_space(p.position),
            track_builder,
        ) {
            Ok(t) => t,
            Err(e) => {
                log::warn!(
                    "M44 Phase 3: add_spatial_sub_track failed for entity {:?}: {e}",
                    p.entity
                );
                // Consume the marker anyway — see `consumed`'s note.
                consumed.push(p.entity);
                continue;
            }
        };
        // kira reasons about gain in decibels; gameplay reasons in linear
        // amplitude. `linear_volume_to_db` does the conversion + silence
        // clamp. The underlying `Arc<[Frame]>` is reused — `volume()` returns
        // a fresh `StaticSoundData` value with new settings, not new audio.
        let db = linear_volume_to_db(p.volume);
        let mut sound = (*p.sound).clone().volume(db);
        if p.looping {
            // Phase 4: kira's `loop_region(..)` enables full-region
            // looping. When the source entity is despawned externally
            // (cell unload), the cleanup-looping sweep notices the
            // missing entity and stops the handle.
            sound = sound.loop_region(..);
        }
        let handle = match track.play(sound) {
            Ok(h) => h,
            Err(e) => {
                log::warn!(
                    "M44 Phase 3: track.play failed for entity {:?}: {e}",
                    p.entity
                );
                // Consume the marker anyway — see `consumed`'s note.
                consumed.push(p.entity);
                continue;
            }
        };
        audio_world.active_sounds.push(ActiveSound {
            source: SoundSource::Emitter(p.entity),
            handle,
            track,
            // The sub-track was created AT the dispatch position, so that
            // is the seeded `last_position` — a stationary emitter's
            // follow pass is a comparison per tick, not a command (#3086).
            last_position: Some(bu_to_audio_space(p.position)),
            underwater_filter,
            underwater,
            unload_fade_ms: p.unload_fade_ms,
            stop_issued: false,
        });
        consumed.push(p.entity);
    }

    // Clear the OneShotSound marker on every entity whose dispatch was
    // attempted — started or failed — so we don't re-dispatch next
    // frame. AudioEmitter stays — callers can observe "is this entity
    // still playing?" through the active list.
    if !consumed.is_empty() {
        if let Some(mut oneshot_q) = world.query_mut::<OneShotSound>() {
            for entity in consumed {
                oneshot_q.remove(entity);
            }
        }
    }
}

/// Walk `active_sounds`, drop any whose `StaticSoundHandle::state()`
/// reports `Stopped`, and remove the `AudioEmitter` component from
/// the source entity so a downstream cleanup system can despawn it
/// without coupling to audio state.
fn prune_stopped_sounds(world: &World, audio_world: &mut AudioWorld) {
    // Phase 4 / #858 / SAFE-23: any active sound whose source entity
    // has lost its `AudioEmitter` (despawn-by-cell-unload, explicit
    // remove) should be stopped at the kira layer. Pre-#858 only
    // looping sounds were truncated here — non-looping SFX kept
    // playing past the despawn at the stale entity transform until
    // natural termination (50 ms – 3 s typical), surfacing as faint
    // cross-cell SFX bleed on fast interior↔interior fast-travel.
    // #5410 — only `SoundSource::Emitter` entries carry that despawn
    // coupling. Detached queue plays have no entity by contract, and
    // `Anchored` plays (dialogue voice) deliberately keep talking
    // through SFX emitter churn on the same speaker: both run to
    // natural termination (or an explicit `stop_sounds_for`).
    let emitter_q = world.query::<AudioEmitter>();
    let mut to_stop_indices: Vec<usize> = Vec::new();
    for (idx, s) in audio_world.active_sounds.iter().enumerate() {
        // Don't re-mark entries whose stop has already been issued —
        // the handle is fading out asynchronously and won't report
        // `Stopped` until the tween completes. Pre-fix every prune
        // tick during the fade window re-walked + re-pushed the
        // ringbuf `stop` command (idempotent in effect, wasted CPU
        // on the active-list walk). See #844.
        if s.stop_issued {
            continue;
        }
        let Some(entity) = s.source.emitter_coupled() else {
            continue;
        };
        let still_has_emitter = emitter_q
            .as_ref()
            .map(|q| q.get(entity).is_some())
            .unwrap_or(false);
        if !still_has_emitter {
            to_stop_indices.push(idx);
        }
    }
    drop(emitter_q);
    for idx in &to_stop_indices {
        // Per-emitter fade-out (#845). Captured at dispatch time from
        // `AudioEmitter.unload_fade_ms` because the source emitter
        // component is already gone by the time we're stopping. The
        // 10 ms default matches `Tween::default()` exactly so authors
        // who don't override stay on the pre-#845 behaviour.
        let fade_ms = audio_world.active_sounds[*idx].unload_fade_ms.max(0.0);
        let tween = Tween {
            start_time: kira::StartTime::Immediate,
            duration: Duration::from_secs_f32(fade_ms / 1000.0),
            easing: kira::Easing::Linear,
        };
        audio_world.active_sounds[*idx].handle.stop(tween);
        // Mark so subsequent prune ticks skip the re-stop until the
        // handle actually transitions to `Stopped` and `retain`
        // drops the entry. See #844.
        audio_world.active_sounds[*idx].stop_issued = true;
    }

    let mut finished: Vec<EntityId> = Vec::new();
    audio_world.active_sounds.retain(|s| {
        if matches!(s.handle.state(), PlaybackState::Stopped) {
            // Queue-driven plays (Detached, and Anchored since #5410)
            // have no emitter component to clean up. Emitter-dispatched
            // plays surface their `EntityId` so the prune pass can
            // remove the `AudioEmitter` component.
            if let Some(e) = s.source.emitter_coupled() {
                finished.push(e);
            }
            false
        } else {
            true
        }
    });
    if !finished.is_empty() {
        if let Some(mut emitter_q) = world.query_mut::<AudioEmitter>() {
            for entity in finished {
                emitter_q.remove(entity);
            }
        }
    }
}

/// Spawn a one-shot sound entity at `position` with default
/// orientation. The audio system picks it up next tick (post
/// transform propagation). Returns the entity so callers can attach
/// gameplay components (e.g. parenting under an actor for
/// short-lived position tracking) before the system fires.
///
/// This is the public ECS-shape contract Phase 3 commits to.
/// Gameplay code (footstep timer, weapon-fire trigger, dialogue
/// dispatcher) owns the *when*; this helper owns the *how*.
pub fn spawn_oneshot_at(
    world: &mut World,
    sound: Arc<StaticSoundData>,
    position: Vec3,
    attenuation: Attenuation,
    volume: f32,
) -> EntityId {
    let entity = world.spawn();
    world.insert(entity, Transform::new(position, glam::Quat::IDENTITY, 1.0));
    world.insert(
        entity,
        GlobalTransform::new(position, glam::Quat::IDENTITY, 1.0),
    );
    world.insert(
        entity,
        AudioEmitter {
            sound,
            attenuation,
            volume,
            looping: false,
            // One-shots usually terminate naturally before any
            // `unload_fade_ms` is consulted; the field still applies
            // if the entity is despawned mid-playback (post-#858 the
            // prune sweep truncates non-looping despawned emitters
            // through the same fade-out path as looping ones).
            unload_fade_ms: DEFAULT_UNLOAD_FADE_MS,
        },
    );
    world.insert(entity, OneShotSound);
    entity
}

/// Decode a fully-buffered audio blob into a `StaticSoundData`.
///
/// Takes an owned `Vec<u8>` — typically extracted from a Bethesda BSA
/// via [`byroredux_bsa::BsaArchive::extract`] — though kira 0.12 relaxed
/// `StaticSoundData::from_cursor` to `T: AsRef<[u8]> + Send + Sync` (no
/// longer `'static`), since the data is fully decoded during the call.
///
/// Format detection is automatic via symphonia's probe (kira pulls
/// in symphonia with the `wav`, `ogg`, `mp3`, and `flac` features by
/// default). The two formats present in vanilla `Fallout - Sound.bsa`
/// — WAV (4233 / 6465 files) and OGG Vorbis (2232 / 6465 files) —
/// both decode through this path.
///
/// **Not** for ambient music or other long-running streams: those
/// should land on `kira::sound::streaming` once Phase 5 wires it.
/// Static decoding loads the entire decompressed audio into memory
/// up-front, which is what we want for short SFX (footsteps, impacts,
/// gunshots) but wasteful for multi-minute ambient loops.
pub fn load_sound_from_bytes(bytes: Vec<u8>) -> Result<StaticSoundData, FromFileError> {
    let cursor = Cursor::new(bytes);
    StaticSoundData::from_cursor(cursor)
}

/// #5367 Phase V — a settings-only copy of `sound` that starts `secs`
/// seconds from now. Lets a caller schedule sequential one-shots (a
/// dialogue line's response segments) through the same
/// [`AudioWorld::play_oneshot`] queue without holding kira types: the
/// underlying `Arc<[Frame]>` samples are shared, only the start-time
/// setting differs.
pub fn with_start_delay(sound: &std::sync::Arc<StaticSoundData>, secs: f64) -> std::sync::Arc<StaticSoundData> {
    std::sync::Arc::new(
        (**sound)
            .clone()
            .start_time(kira::StartTime::Delayed(std::time::Duration::from_secs_f64(secs.max(0.0)))),
    )
}

/// **Phase 5**: decode a fully-buffered audio blob as a streaming
/// sound. Unlike [`load_sound_from_bytes`], the result decodes
/// audio frames incrementally during playback — appropriate for
/// multi-minute music that would otherwise burn ~30 MB of RAM per
/// track decompressed.
pub fn load_streaming_sound_from_bytes(
    bytes: Vec<u8>,
) -> Result<StreamingSoundData<FromFileError>, FromFileError> {
    let cursor = Cursor::new(bytes);
    StreamingSoundData::from_cursor(cursor)
}

/// **Phase 5**: streaming variant of [`load_streaming_sound_from_bytes`]
/// that opens the file lazily — kira holds an `std::fs::File` and
/// pulls decoded frames as the playback head advances. Use this for
/// loose `Data/Music/*.mp3` / `*.wav` files that aren't archived.
pub fn load_streaming_sound_from_file(
    path: impl AsRef<std::path::Path>,
) -> Result<StreamingSoundData<FromFileError>, FromFileError> {
    StreamingSoundData::from_file(path)
}

/// Process-lifetime cache of decoded `StaticSoundData`, keyed by
/// lowercased asset path. Repeat plays of the same SFX (footsteps,
/// weapon fire, dialogue lines) skip the decode cost entirely —
/// kira clones the `Arc<StaticSoundData>` cheaply when handing it
/// to the playback handle.
///
/// Lookup is case-insensitive to match the BSA / NIF / texture
/// asset-path convention shared across the engine. Storing lowercased
/// keys means `get` / `insert` callers don't have to re-lowercase
/// per-call; intern the lowered form once at insert time.
///
/// Eviction strategy: **manual, via [`Self::clear`]**. No automatic
/// LRU today. The full vanilla SFX set fits in a few hundred MB of
/// decoded PCM; the cell-unload path can call `clear()` when a region
/// exits scope to bound memory across long sessions with mod-loaded
/// SFX (Project Nevada / TTW / FCO stacks push past 1 GB without it).
/// [`Self::bytes_estimate`] is intended to surface the cache footprint
/// to ownership telemetry so an unbounded-growth regression shows up
/// before OOM. If a real LRU is ever needed (1000+ unique
/// sounds with frequent rotation), bolt it on without touching the
/// call sites. See #850 / AUD-D6-NEW-09.
///
/// `None` values represent negative-cache entries and are not counted by
/// `len()` or `bytes_estimate()`: telemetry measures decoded audio retained
/// for playback, while missing/invalid paths stay cached to avoid repeated
/// archive work.
pub struct SoundCache {
    // `None` is a negative-cache entry: a missing archive member or a
    // malformed sound is not probed again on every gameplay event.
    map: HashMap<String, Option<Arc<StaticSoundData>>>,
}

impl Default for SoundCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundCache {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Look up a decoded sound by path. `None` means either no entry or a
    /// negative-cache entry; use [`Self::get_or_load`] when loading so cached
    /// misses do not trigger another archive probe.
    pub fn get(&self, path: &str) -> Option<Arc<StaticSoundData>> {
        self.map
            .get(&path.to_ascii_lowercase())
            .and_then(|sound| sound.clone())
    }

    /// Insert a decoded sound at `path`. Returns the `Arc` so callers
    /// can chain into an [`AudioEmitter::sound`] without a second
    /// lookup. Repeated inserts at the same path overwrite — useful
    /// when a mod replaces a vanilla SFX.
    pub fn insert(&mut self, path: &str, sound: StaticSoundData) -> Arc<StaticSoundData> {
        let key = path.to_ascii_lowercase();
        let arc = Arc::new(sound);
        self.map.insert(key, Some(Arc::clone(&arc)));
        arc
    }

    /// Convenience: cache hit → reuse, cache miss → extract and decode
    /// through `loader`. The loader is only invoked on a miss, so callers
    /// can pay the BSA-extract cost lazily. A miss or decode failure is
    /// cached too, preventing repeated archive probes for unavailable SFX.
    ///
    /// Returns `None` when the loader has no bytes or the bytes fail to
    /// decode. Callers can log + skip; subsequent calls for this path do
    /// not invoke the loader again. Use [`Self::clear`] to retry after
    /// changing the archive set.
    pub fn get_or_load<F>(&mut self, path: &str, loader: F) -> Option<Arc<StaticSoundData>>
    where
        F: FnOnce() -> Option<Vec<u8>>,
    {
        let key = path.to_ascii_lowercase();
        if let Some(existing) = self.map.get(&key) {
            return existing.as_ref().map(Arc::clone);
        }
        let Some(bytes) = loader() else {
            self.map.insert(key, None);
            return None;
        };
        match load_sound_from_bytes(bytes) {
            Ok(sound) => {
                let arc = Arc::new(sound);
                self.map.insert(key, Some(Arc::clone(&arc)));
                Some(arc)
            }
            Err(e) => {
                log::warn!("M44: decode failed for sound '{path}': {e}");
                self.map.insert(key, None);
                None
            }
        }
    }

    /// Number of cached sounds. Useful for telemetry — a sudden
    /// growth burst during a cell load is the canonical signal that
    /// SFX dispatch is firing per-NPC instead of per-archive-load.
    pub fn len(&self) -> usize {
        self.map.values().filter(|sound| sound.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.map.values().all(Option::is_none)
    }

    /// Drop every cached sound. Existing `StaticSoundHandle`s playing
    /// the dropped `Arc<StaticSoundData>` keep their own clone alive
    /// for the lifetime of the handle — kira never reads through the
    /// cache after the initial play call. Intended for the cell-unload
    /// path to bound memory across long sessions; vanilla gameplay
    /// can leave the cache populated process-lifetime. See #850 /
    /// AUD-D6-NEW-09.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Best-effort estimate of cached decoded PCM size (bytes). Sums
    /// `frames.len() * size_of::<kira::Frame>()` for each entry —
    /// frame storage is `Arc<[Frame]>` where `Frame = { f32 left, f32
    /// right }` (8 B/frame for stereo). Does NOT count the
    /// `Arc<StaticSoundData>` header, `StaticSoundSettings`, or the
    /// `HashMap` overhead — those are O(entries) and small next to
    /// the PCM blob. Sampled into ownership telemetry so an unbounded-growth
    /// regression surfaces before OOM. See #850 / AUD-D6-NEW-09.
    pub fn bytes_estimate(&self) -> usize {
        let frame_size = std::mem::size_of::<kira::Frame>();
        self.map
            .values()
            .filter_map(Option::as_ref)
            .map(|sound| sound.frames.len() * frame_size)
            .sum()
    }
}

impl Resource for SoundCache {}

/// #5382 — byte-budgeted LRU cache for dialogue voice lines.
///
/// [`SoundCache`] is process-lifetime by design: its consumers (combat
/// SFX, UI) draw from a closed keyspace of archive staples. Dialogue
/// voice is the first open-ended keyspace — every distinct spoken line
/// retains ~0.19 MB of decoded PCM per voiced second (measured on Doc
/// Mitchell's 5 s mono 24 kHz greeting → 960,400 B of stereo `f32`
/// frames), and FNV's Voices1 archive alone holds 105,517 candidates —
/// so voice rides this separate cache instead of the shared one.
///
/// Same `get_or_load` contract as [`SoundCache`] (the loader runs under
/// the cache write guard, preserving the #5383 cache→provider lock
/// order), with two bounds the shared cache lacks:
///
/// - decoded PCM is evicted least-recently-used once `bytes_estimate`
///   exceeds the budget (default 64 MiB ≈ one recent conversation's
///   worth of lines); playing handles keep their own `Arc` clone alive,
///   exactly like [`SoundCache::clear`].
/// - negative entries are capped and *counted* (`negative_len`) instead
///   of accumulating silently — a miss storm on a modded voices archive
///   clears the negative set once past the cap and re-probes, rather
///   than growing for the process lifetime.
pub struct VoiceSoundCache {
    budget_bytes: usize,
    /// LRU order — index 0 is most recently used.
    entries: Vec<(String, Arc<StaticSoundData>, usize)>,
    resident_bytes: usize,
    negatives: std::collections::HashSet<String>,
}

/// Negative-entry cap: past this many distinct misses the set clears and
/// re-probes. Sized so the common per-session distinct-miss population
/// (wrong voice type, absent DLC archives) stays cached, while a broken
/// path storm cannot grow the set without bound.
pub const VOICE_NEGATIVE_CAP: usize = 4096;

impl Default for VoiceSoundCache {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceSoundCache {
    pub const DEFAULT_BUDGET_BYTES: usize = 64 * 1024 * 1024;

    pub fn new() -> Self {
        Self::with_budget(Self::DEFAULT_BUDGET_BYTES)
    }

    pub fn with_budget(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            entries: Vec::new(),
            resident_bytes: 0,
            negatives: std::collections::HashSet::new(),
        }
    }

    /// Cache hit → reuse (and mark most-recent), cache miss → extract and
    /// decode through `loader`, same shape as [`SoundCache::get_or_load`].
    /// A newly decoded sound evicts least-recently-used entries until the
    /// byte budget holds; the just-inserted entry is never evicted by its
    /// own insertion. Returns `None` on a miss or decode failure, both
    /// remembered as (capped) negatives.
    pub fn get_or_load<F>(&mut self, path: &str, loader: F) -> Option<Arc<StaticSoundData>>
    where
        F: FnOnce() -> Option<Vec<u8>>,
    {
        let key = path.to_ascii_lowercase();
        if let Some(i) = self.entries.iter().position(|(k, ..)| *k == key) {
            let (k, sound, bytes) = self.entries.remove(i);
            self.entries.insert(0, (k, sound.clone(), bytes));
            return Some(sound);
        }
        if self.negatives.contains(&key) {
            return None;
        }
        let Some(bytes) = loader() else {
            self.remember_negative(key);
            return None;
        };
        match load_sound_from_bytes(bytes) {
            Ok(sound) => {
                let pcm_bytes = sound.frames.len() * std::mem::size_of::<kira::Frame>();
                self.resident_bytes += pcm_bytes;
                let arc = Arc::new(sound);
                self.entries.insert(0, (key, Arc::clone(&arc), pcm_bytes));
                while self.resident_bytes > self.budget_bytes && self.entries.len() > 1 {
                    let (_, _, evicted) = self.entries.pop().expect("len > 1 checked above");
                    self.resident_bytes -= evicted;
                }
                Some(arc)
            }
            Err(e) => {
                log::warn!("M44: decode failed for voice '{path}': {e}");
                self.remember_negative(key);
                None
            }
        }
    }

    fn remember_negative(&mut self, key: String) {
        if self.negatives.len() >= VOICE_NEGATIVE_CAP {
            self.negatives.clear();
        }
        self.negatives.insert(key);
    }

    /// Number of cached voice decodes (telemetry — see
    /// [`SoundCache::len`] for the shared-cache counterpart).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Distinct cached misses — surfaced because the shared cache's
    /// uncounted `None` entries were invisible to telemetry (#5382).
    pub fn negative_len(&self) -> usize {
        self.negatives.len()
    }

    /// Resident decoded PCM, same accounting as
    /// [`SoundCache::bytes_estimate`]. Bounded by the budget by
    /// construction; a value pinned at the budget with `len()` still
    /// growing is the signal the LRU is churning.
    pub fn bytes_estimate(&self) -> usize {
        self.resident_bytes
    }

    /// The LRU's byte budget.
    pub fn budget_bytes(&self) -> usize {
        self.budget_bytes
    }
}

impl Resource for VoiceSoundCache {}

#[cfg(test)]
mod tests;
