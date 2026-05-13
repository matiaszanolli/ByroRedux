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
//! - [`audio_system`] — ECS system that updates listener position,
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
//! # Future phases (not in this commit)
//!
//! - Phase 3.5b: FOOT records parser → per-material sound lookup.
//! - Phase 4: REGN ambient soundscapes (region-based ambient layers).
//! - Phase 5: MUSC + hardcoded music routing with crossfade.
//! - Phase 6: Reverb zones (kira's `ReverbBuilder`) keyed off cell
//!   acoustics; raycast occlusion attenuation.

use byroredux_core::ecs::components::{GlobalTransform, Transform};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_core::ecs::Resource;
use glam::Vec3;
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

// Re-export the kira types downstream crates need so they can hold
// `Arc<StaticSoundData>` (in `Resource`s, components, etc.) without
// pulling kira as a direct dependency. The audio crate is the canon
// owner of the audio-engine surface.
pub use kira::sound::static_sound::{
    StaticSoundData as Sound, StaticSoundSettings as SoundSettings,
};
pub use kira::Frame;

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

/// One currently-playing sound. The `_track` field keeps the spatial
/// sub-track alive — dropping it would tear down playback even if the
/// `handle` is still ticking. `entity` is `Some(EntityId)` for the
/// entity-based `OneShotSound + AudioEmitter` flow (Phase 3) and
/// `None` for queue-driven fire-and-forget plays (Phase 3.5
/// `play_oneshot`). When `Some`, the prune pass removes the
/// `AudioEmitter` component on completion so a downstream cleanup
/// system can despawn the entity. Underscore-prefix on `_track`
/// because we hold it for `Drop` side effect only.
///
/// Whether the underlying kira sound is looping (set via
/// `loop_region(..)` at dispatch) is decided at the `Pending` /
/// `AudioEmitter` layer; `ActiveSound` itself doesn't need to carry
/// that bit post-#858 since the prune sweep no longer branches on it.
struct ActiveSound {
    entity: Option<EntityId>,
    handle: StaticSoundHandle,
    _track: SpatialTrackHandle,
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

/// Fire-and-forget one-shot queued via [`AudioWorld::play_oneshot`].
/// Drained and dispatched by `audio_system` at the start of each
/// frame. Lives in `AudioWorld` rather than as ECS components so
/// callers without `&mut World` (Systems) can still trigger sounds.
struct PendingOneShot {
    sound: Arc<StaticSoundData>,
    position: Vec3,
    attenuation: Attenuation,
    volume: f32,
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
                    .feedback(0.85)
                    .damping(0.6)
                    .stereo_width(1.0)
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
            active_sounds: Vec::new(),
            pending_oneshots: VecDeque::new(),
            music: None,
            reverb_send,
            reverb_send_db: f32::NEG_INFINITY,
            listener: None,
            manager,
            multi_listener_warned: false,
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

    /// Number of one-shots queued but not yet dispatched. Drained on
    /// each `audio_system` tick. A runaway count would signal that
    /// `audio_system` isn't running, or that the manager is inactive
    /// and queue items pile up indefinitely.
    pub fn pending_oneshot_count(&self) -> usize {
        self.pending_oneshots.len()
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
        });
    }

    /// **Phase 5**: play a streaming sound through the main track.
    /// Music is non-spatial by design — it shouldn't attenuate with
    /// player position the way a campfire's crackle does. Volume
    /// is linear amplitude (1.0 = nominal); `fade_in_secs` controls
    /// the kira tween used to fade in (and to fade out any existing
    /// track being replaced).
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
        let db = if volume > 0.0001 {
            20.0 * volume.log10()
        } else {
            -60.0
        };
        let configured = streaming_sound.volume(db).fade_in_tween(Some(fade));
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

    /// True when music is currently playing or fading out. Useful
    /// for menu-toggle / cell-load gameplay logic that wants to
    /// avoid stacking music calls.
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
    /// **Limitation (#847):** kira 0.10's `with_send` is build-time
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attenuation {
    pub min_distance: f32,
    pub max_distance: f32,
}

impl Default for Attenuation {
    fn default() -> Self {
        // Defaults chosen for Bethesda interior cells: inside a 2-3m
        // sphere it's full volume; out at 30m it's gone. Footsteps
        // and small impacts will want tighter ranges; ambient loops
        // and music want larger.
        Self {
            min_distance: 2.0,
            max_distance: 30.0,
        }
    }
}

/// Static-payload audio emitter. Holds the decoded sound data and
/// attenuation. The audio system reads the entity's `GlobalTransform`
/// every frame to update the spatial position.
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

/// Per-frame audio update — synchronises listener pose, plays new
/// one-shots through per-emitter spatial sub-tracks, prunes finished
/// sounds. `Stage::Late` is the canonical home (after transform
/// propagation has produced final world poses for the listener and
/// every emitter).
///
/// Phase 3 implementation:
///
/// 1. **Listener sync**: locate the (single) `AudioListener` entity.
///    On first frame, lazily call `manager.add_listener` with its
///    `GlobalTransform`. On subsequent frames, push pose updates
///    through `ListenerHandle::set_position` / `set_orientation`.
/// 2. **Dispatch new one-shots**: for each entity carrying both
///    `OneShotSound` + `AudioEmitter`, create a spatial sub-track
///    anchored at the entity's `GlobalTransform`, play the sound on
///    that track, and remove `OneShotSound` so the dispatcher won't
///    re-trigger next frame. The `AudioEmitter` stays so callers
///    can query "is this entity still playing?" via the active list.
/// 3. **Prune stopped**: walk `active_sounds`, drop any whose handle
///    reports `PlaybackState::Stopped`. Removing the entity's
///    `AudioEmitter` lets a downstream cleanup system (or the cell
///    unloader) despawn it without coupling to audio state.
///
/// Looping playback / fade-in / fade-out / streaming lifecycle land
/// in subsequent phases on top of the same shape.
pub fn audio_system(world: &World, _dt: f32) {
    let Some(mut audio_world) = world.try_resource_mut::<AudioWorld>() else {
        return;
    };
    if !audio_world.is_active() {
        return;
    }

    sync_listener_pose(world, &mut audio_world);
    drain_pending_oneshots(&mut audio_world);
    dispatch_new_oneshots(world, &mut audio_world);
    prune_stopped_sounds(world, &mut audio_world);
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
/// `listener_capacity` is 8 (kira-0.10's manager settings cap),
/// so a "clear on missing entity → re-add on respawn" simplification
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
        match mgr.add_listener(pose.0, pose.1) {
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
        handle.set_position(pose.0, Tween::default());
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
    // Manager-active gate moves *before* the `mem::take` (#851).
    // Pre-fix the take ran first, so on a hypothetical `manager =
    // None` re-entry the drained Vec would be silently dropped — the
    // `// Inactive — queue cleared` branch below was reachable in
    // theory and would have lost the queued one-shots forever. In
    // practice the parent `audio_system` early-returns at
    // `is_active()` before calling this helper, so the manager is
    // always `Some` here, but the defensive ordering keeps the
    // contract local: we only consume the queue once we know we can
    // dispatch its contents.
    let Some(mgr) = audio_world.manager.as_mut() else {
        return;
    };
    let pending = std::mem::take(&mut audio_world.pending_oneshots);
    if pending.len() > 32 {
        log::warn!(
            "M44 Phase 3.5: drained {} pending one-shots in one tick — \
             upstream system is firing too fast (footstep stride, weapon \
             rate-of-fire, dialogue queue?)",
            pending.len()
        );
    }
    for p in pending {
        let mut track_builder = SpatialTrackBuilder::new()
            .distances(p.attenuation.min_distance..=p.attenuation.max_distance);
        // Phase 6: route a fraction of this track's signal to the
        // global reverb send if one exists and the level isn't
        // muted. with_send takes a Decibels-convertible f32; the
        // f32 is treated as raw dB, so f32::NEG_INFINITY is a clean
        // "no reverb" sentinel.
        if let Some(reverb) = audio_world.reverb_send.as_ref() {
            if audio_world.reverb_send_db.is_finite() && audio_world.reverb_send_db > -60.0 {
                track_builder = track_builder.with_send(reverb.id(), audio_world.reverb_send_db);
            }
        }
        let mut track = match mgr.add_spatial_sub_track(listener_id, p.position, track_builder) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("M44 Phase 3.5: add_spatial_sub_track failed: {e}");
                continue;
            }
        };
        let db = if p.volume > 0.0001 {
            20.0 * p.volume.log10()
        } else {
            -60.0
        };
        let sound = (*p.sound).clone().volume(db);
        let handle = match track.play(sound) {
            Ok(h) => h,
            Err(e) => {
                log::warn!("M44 Phase 3.5: track.play (queue) failed: {e}");
                continue;
            }
        };
        audio_world.active_sounds.push(ActiveSound {
            entity: None,
            handle,
            _track: track,
            // Queue-driven sounds have `entity == None` — they're
            // intentionally decoupled from despawn coupling (no
            // entity, no cell unload to truncate against), so the
            // prune sweep's emitter-presence check skips them and
            // they run to natural termination as `play_oneshot`'s
            // documented contract requires. `unload_fade_ms` is
            // never consulted on this branch.
            unload_fade_ms: DEFAULT_UNLOAD_FADE_MS,
            // Queue-driven sounds never re-enter the prune sweep's
            // stop branch (no entity → no despawn signal), so this
            // flag stays `false` for life. See #844 / #858.
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
    let mut started: Vec<EntityId> = Vec::with_capacity(pending.len());
    for p in pending {
        // kira's `SpatialTrackBuilder::distances` accepts a
        // `RangeInclusive<f32>` (or `(f32, f32)` / `[f32; 2]`); the
        // exclusive `..` range we use elsewhere doesn't impl
        // `Into<SpatialTrackDistances>`. The values are min..=max
        // game-units, falloff between is linear (kira default).
        let mut track_builder = SpatialTrackBuilder::new()
            .distances(p.attenuation.min_distance..=p.attenuation.max_distance);
        // Phase 6: route a fraction of this track's signal to the
        // global reverb send if one exists and the level isn't
        // muted. with_send takes a Decibels-convertible f32; the
        // f32 is treated as raw dB, so f32::NEG_INFINITY is a clean
        // "no reverb" sentinel.
        if let Some(reverb) = audio_world.reverb_send.as_ref() {
            if audio_world.reverb_send_db.is_finite() && audio_world.reverb_send_db > -60.0 {
                track_builder = track_builder.with_send(reverb.id(), audio_world.reverb_send_db);
            }
        }
        let mut track = match mgr.add_spatial_sub_track(listener_id, p.position, track_builder) {
            Ok(t) => t,
            Err(e) => {
                log::warn!(
                    "M44 Phase 3: add_spatial_sub_track failed for entity {:?}: {e}",
                    p.entity
                );
                continue;
            }
        };
        // kira reasons about gain in decibels; gameplay reasons in
        // linear amplitude (1.0 = "as authored", 0.5 = half-loud).
        // Convert: db = 20 * log10(amplitude). Clamp to SILENCE
        // (-60 dB) for non-positive volumes so log10 doesn't blow
        // up. The underlying `Arc<[Frame]>` is reused — `volume()`
        // returns a fresh `StaticSoundData` value with new settings,
        // not new audio.
        let db = if p.volume > 0.0001 {
            20.0 * p.volume.log10()
        } else {
            -60.0
        };
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
                continue;
            }
        };
        audio_world.active_sounds.push(ActiveSound {
            entity: Some(p.entity),
            handle,
            _track: track,
            unload_fade_ms: p.unload_fade_ms,
            stop_issued: false,
        });
        started.push(p.entity);
    }

    // Clear the OneShotSound marker on every entity that started so we
    // don't re-dispatch next frame. AudioEmitter stays — callers can
    // observe "is this entity still playing?" through the active list.
    if !started.is_empty() {
        if let Some(mut oneshot_q) = world.query_mut::<OneShotSound>() {
            for entity in started {
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
    // Queue-driven plays (`entity == None`, see `play_oneshot`) are
    // unaffected — no entity, no despawn coupling, they run to
    // natural termination as `play_oneshot`'s documented contract
    // requires.
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
        let Some(entity) = s.entity else {
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
            // Queue-driven plays have `entity == None` — nothing to
            // clean up on the ECS side. Entity-driven plays surface
            // their `EntityId` so the prune pass can remove the
            // `AudioEmitter` component.
            if let Some(e) = s.entity {
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
/// `bytes` must own its data so kira's `Cursor<T: AsRef<[u8]> + Send +
/// Sync + 'static>` requirement is satisfied — typically a `Vec<u8>`
/// extracted from a Bethesda BSA via [`byroredux_bsa::BsaArchive::extract`].
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
/// [`Self::bytes_estimate`] surfaces the cache footprint to telemetry
/// so a future unbounded-growth regression shows up in `stats` output
/// rather than at OOM. If a real LRU is ever needed (1000+ unique
/// sounds with frequent rotation), bolt it on without touching the
/// call sites. See #850 / AUD-D6-NEW-09.
///
/// **Dormant API (#859):** the engine binary currently has zero
/// call sites for `SoundCache`. The footstep dispatch path at
/// `byroredux/src/asset_provider.rs::resolve_footstep_sound` writes
/// directly into `FootstepConfig.default_sound: Option<Arc<Sound>>`,
/// bypassing the cache; the decoded `Arc` is held by exactly one
/// `Resource` (`FootstepConfig`) for the engine lifetime. The "no
/// eviction → unbounded growth" concern surfaces only when a future
/// commit wires a real consumer (FOOT records, REGN ambient,
/// multi-sound SFX dispatch). Until then `len() == 0` is the steady
/// state. The decoupled API + tests stay so a producer can land
/// without a structural rewrite — but anyone wiring the first real
/// consumer should also wire eviction at the same time.
pub struct SoundCache {
    map: HashMap<String, Arc<StaticSoundData>>,
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

    /// Look up a cached sound by path. Returns `None` on a miss —
    /// callers should follow up with [`Self::insert`] after extracting
    /// + decoding the bytes.
    pub fn get(&self, path: &str) -> Option<Arc<StaticSoundData>> {
        self.map.get(&path.to_ascii_lowercase()).cloned()
    }

    /// Insert a decoded sound at `path`. Returns the `Arc` so callers
    /// can chain into an [`AudioEmitter::sound`] without a second
    /// lookup. Repeated inserts at the same path overwrite — useful
    /// when a mod replaces a vanilla SFX.
    pub fn insert(&mut self, path: &str, sound: StaticSoundData) -> Arc<StaticSoundData> {
        let key = path.to_ascii_lowercase();
        let arc = Arc::new(sound);
        self.map.insert(key, Arc::clone(&arc));
        arc
    }

    /// Convenience: cache hit → reuse, cache miss → decode the bytes
    /// returned by `loader` and insert. The loader is only invoked
    /// on a miss, so callers can pay the BSA-extract cost lazily.
    ///
    /// Returns `None` if the cache missed AND the decode failed —
    /// the loader's bytes were unusable. Callers can log + skip.
    pub fn get_or_load<F>(&mut self, path: &str, loader: F) -> Option<Arc<StaticSoundData>>
    where
        F: FnOnce() -> Vec<u8>,
    {
        let key = path.to_ascii_lowercase();
        if let Some(existing) = self.map.get(&key) {
            return Some(Arc::clone(existing));
        }
        match load_sound_from_bytes(loader()) {
            Ok(sound) => {
                let arc = Arc::new(sound);
                self.map.insert(key, Arc::clone(&arc));
                Some(arc)
            }
            Err(e) => {
                log::warn!("M44: decode failed for sound '{path}': {e}");
                None
            }
        }
    }

    /// Number of cached sounds. Useful for telemetry — a sudden
    /// growth burst during a cell load is the canonical signal that
    /// SFX dispatch is firing per-NPC instead of per-archive-load.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
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
    /// the PCM blob. Useful for `stats` console output so a future
    /// unbounded-growth regression surfaces in telemetry rather than
    /// at OOM. See #850 / AUD-D6-NEW-09.
    pub fn bytes_estimate(&self) -> usize {
        let frame_size = std::mem::size_of::<kira::Frame>();
        self.map
            .values()
            .map(|sound| sound.frames.len() * frame_size)
            .sum()
    }
}

impl Resource for SoundCache {}


#[cfg(test)]
mod tests;
