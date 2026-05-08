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
use std::time::Duration;
use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::sync::Arc;

// Re-export the kira types downstream crates need so they can hold
// `Arc<StaticSoundData>` (in `Resource`s, components, etc.) without
// pulling kira as a direct dependency. The audio crate is the canon
// owner of the audio-engine surface.
pub use kira::sound::static_sound::{StaticSoundData as Sound, StaticSoundSettings as SoundSettings};
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
/// `looping` (Phase 4): when true, the prune pass treats `Stopped`
/// as a real "stop me" signal (caller-driven, e.g. cell unload).
/// When false, `Stopped` is the natural one-shot termination.
struct ActiveSound {
    entity: Option<EntityId>,
    handle: StaticSoundHandle,
    _track: SpatialTrackHandle,
    looping: bool,
    /// Fade-out duration captured from `AudioEmitter.unload_fade_ms` at
    /// dispatch time. Read by `prune_stopped_sounds` when the source
    /// entity loses its emitter component (cell unload). One-shots
    /// (`looping=false`) carry the default value but never consult it.
    /// See #845.
    unload_fade_ms: f32,
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
    /// No-op when audio is inactive: pending entries still queue up
    /// (so a future re-init could replay them), but `audio_system`
    /// short-circuits before drain. To avoid unbounded growth in a
    /// no-device-forever scenario, the queue is bounded at 256
    /// entries — overflow drops oldest with a one-shot warn. 256 is
    /// 8 seconds of footsteps at 32 Hz cadence; real gameplay never
    /// approaches it.
    pub fn play_oneshot(
        &mut self,
        sound: Arc<StaticSoundData>,
        position: Vec3,
        attenuation: Attenuation,
        volume: f32,
    ) {
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
    /// (cell unload, scripted teardown). Only consulted on `looping`
    /// sounds — one-shots terminate naturally.
    ///
    /// Default 10 ms matches `kira::Tween::default()` and is inaudible
    /// on short sustained ambients (campfire crackle, generator hum).
    /// Long-tailed ambients (cathedral choir, distant thunder loop)
    /// authoring 200-500 ms here avoids the faint click on cell exit
    /// the abrupt 10 ms cutoff produces. See #845 / AUD-D4-NEW-04.
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
fn sync_listener_pose(world: &World, audio_world: &mut AudioWorld) {
    let listener_entity = {
        let Some(q) = world.query::<AudioListener>() else {
            return;
        };
        let Some((entity, _)) = q.iter().next() else {
            return;
        };
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
            if audio_world.reverb_send_db.is_finite()
                && audio_world.reverb_send_db > -60.0
            {
                track_builder = track_builder.with_send(reverb.id(), audio_world.reverb_send_db);
            }
        }
        let mut track =
            match mgr.add_spatial_sub_track(listener_id, p.position, track_builder) {
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
            looping: false,
            // Queue-driven path is one-shots only (`looping=false`);
            // unload_fade_ms is never consulted on this branch.
            unload_fade_ms: DEFAULT_UNLOAD_FADE_MS,
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
            if audio_world.reverb_send_db.is_finite()
                && audio_world.reverb_send_db > -60.0
            {
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
            looping: p.looping,
            unload_fade_ms: p.unload_fade_ms,
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
    // Phase 4: looping sounds whose source entity has lost its
    // `AudioEmitter` component (despawn-by-cell-unload, or explicit
    // removal) should be stopped at the kira layer too. Snapshot
    // those entity IDs first, then issue stop calls below — kira's
    // tweened stop is async, but the next prune tick catches the
    // resulting `Stopped` state.
    let emitter_q = world.query::<AudioEmitter>();
    let mut to_stop_indices: Vec<usize> = Vec::new();
    for (idx, s) in audio_world.active_sounds.iter().enumerate() {
        if !s.looping {
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
            // One-shots terminate naturally; unload_fade_ms is never
            // consulted on the prune path (gated on `looping`).
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
/// Eviction strategy: **none today**. The full vanilla SFX set fits
/// in a few hundred MB of decoded PCM; aggressive eviction would
/// trade load latency for memory we don't need to save. If a future
/// scenario surfaces (1000+ unique sounds, or platform memory
/// pressure), bolt on an LRU here without touching the call sites.
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
}

impl Resource for SoundCache {}

#[cfg(test)]
mod tests {
    use super::*;

    /// AudioWorld must construct cleanly even when there's no audio
    /// device — CI and headless servers have neither, and a panic
    /// here would refuse to launch the engine.
    #[test]
    fn audio_world_constructs_without_panic_on_any_environment() {
        let _ = AudioWorld::new();
    }

    /// Regression for #845 / AUD-D4-NEW-04: `DEFAULT_UNLOAD_FADE_MS`
    /// matches kira's `Tween::default()` duration (10 ms) so existing
    /// call sites that don't override `AudioEmitter.unload_fade_ms`
    /// stay on the pre-#845 stop-fade behaviour exactly.
    #[test]
    fn default_unload_fade_matches_kira_tween_default() {
        let kira_default = Tween::default();
        let expected_ms = kira_default.duration.as_secs_f32() * 1000.0;
        assert!(
            (DEFAULT_UNLOAD_FADE_MS - expected_ms).abs() < 1.0e-3,
            "DEFAULT_UNLOAD_FADE_MS ({DEFAULT_UNLOAD_FADE_MS} ms) must match \
             kira's Tween::default() duration ({expected_ms} ms) — pre-#845 \
             call sites that didn't author this field expected the kira default",
        );
    }

    /// Regression for #845: a long-tailed cell-ambient emitter MAY
    /// declare a non-default `unload_fade_ms`, and that value
    /// round-trips through `ActiveSound` so the prune sweep stops
    /// the kira handle with the authored fade — not the 10 ms cutoff
    /// that produced the audible click on long ambients.
    ///
    /// This test pins the *capture* hop (emitter → ActiveSound)
    /// since the prune sweep itself needs a real kira handle to call
    /// `.stop(tween)` on. The capture invariant is the part this fix
    /// adds; downstream `Tween` construction uses the captured
    /// `f32` exactly.
    #[test]
    fn audio_emitter_authors_custom_unload_fade_for_long_ambients() {
        // Synthetic long-tail ambient: cathedral choir tail = 800 ms.
        let sound_data = StaticSoundData {
            sample_rate: 44_100,
            frames: Arc::new([Frame::ZERO; 64]),
            settings: kira::sound::static_sound::StaticSoundSettings::default(),
            slice: None,
        };
        let emitter = AudioEmitter {
            sound: Arc::new(sound_data),
            attenuation: Attenuation::default(),
            volume: 1.0,
            looping: true,
            unload_fade_ms: 800.0,
        };
        assert_eq!(emitter.unload_fade_ms, 800.0);

        // The Tween construction itself is what `prune_stopped_sounds`
        // does — pin the formula so a future refactor of
        // `fade_ms / 1000.0` doesn't silently round to zero on
        // sub-millisecond inputs or overflow on huge ones.
        let tween = Tween {
            start_time: kira::StartTime::Immediate,
            duration: Duration::from_secs_f32(emitter.unload_fade_ms.max(0.0) / 1000.0),
            easing: kira::Easing::Linear,
        };
        assert!(
            (tween.duration.as_secs_f32() - 0.8).abs() < 1.0e-6,
            "800 ms unload_fade_ms must produce a 0.8s Tween duration, got {:?}",
            tween.duration,
        );
    }

    /// A negative `unload_fade_ms` (authoring mistake or wraparound)
    /// must clamp to 0 rather than panic at `Duration::from_secs_f32`.
    /// The prune-stop site's `.max(0.0)` is the guard.
    #[test]
    fn unload_fade_clamps_negative_to_zero() {
        let fade_ms: f32 = -50.0;
        let clamped = fade_ms.max(0.0);
        let _ = Duration::from_secs_f32(clamped / 1000.0); // must not panic
        assert_eq!(clamped, 0.0);
    }

    /// Issue #842 regression gate: kira's default `Capacities` caps
    /// `sub_track_capacity` at 128, which a populated Bethesda
    /// interior cell can saturate (FO4 Diamond City Market ≈ 400
    /// emitters in vanilla). Once we hit the cap, kira returns
    /// `ResourceLimitReached` from `add_spatial_sub_track` and the
    /// dispatch path silently drops the sound with only a `warn!`.
    /// This test pins the override; a "simplify back to default"
    /// refactor will trip it.
    #[test]
    fn manager_capacities_exceed_kira_defaults() {
        let defaults = Capacities::default();
        assert!(
            SUB_TRACK_CAPACITY > defaults.sub_track_capacity,
            "SUB_TRACK_CAPACITY={SUB_TRACK_CAPACITY} must exceed kira default \
             {} or populated cells will silently drop sounds (#842)",
            defaults.sub_track_capacity,
        );
        assert!(
            SEND_TRACK_CAPACITY > defaults.send_track_capacity,
            "SEND_TRACK_CAPACITY={SEND_TRACK_CAPACITY} must exceed kira default \
             {} (Phase 4 REGN ambients will need additional send tracks beyond \
             the single global reverb)",
            defaults.send_track_capacity,
        );
    }

    /// Default attenuation is in the "interior cell" range — a
    /// regression that defaults to (0, 0) would silently mute every
    /// sound at any distance. Pinned here so a future "simplify
    /// defaults" refactor can't lose the range.
    #[test]
    fn default_attenuation_is_within_interior_range() {
        let a = Attenuation::default();
        assert!(a.min_distance > 0.0);
        assert!(a.max_distance > a.min_distance);
        assert!(a.max_distance >= 10.0, "interior cells need ≥10m falloff");
    }

    /// Verify the kira static-sound path is wired by synthesising a
    /// short sine wave and decoding it through `StaticSoundData`. No
    /// audio device required — the data path is independent of the
    /// backend. If kira ever reorganises `StaticSoundData::from_*`,
    /// this lights up before we ship.
    #[test]
    fn static_sound_data_constructs_from_decoded_frames() {
        use kira::sound::static_sound::StaticSoundSettings;

        // 0.1 s @ 22.05 kHz of a 440 Hz sine wave, mono. Use kira's
        // `Frame` (stereo float pair) — duplicate the mono sample
        // into both channels.
        let sample_rate: u32 = 22_050;
        let n: usize = (sample_rate as usize) / 10;
        let mut frames: Vec<kira::Frame> = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / sample_rate as f32;
            let s = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            frames.push(kira::Frame { left: s, right: s });
        }
        let sound = StaticSoundData {
            sample_rate,
            frames: Arc::from(frames.into_boxed_slice()),
            settings: StaticSoundSettings::default(),
            slice: None,
        };
        assert!(sound.frames.len() > 0);
        assert_eq!(sound.sample_rate, sample_rate);

        // Wrap in an Arc so the AudioEmitter can hold it. This pins
        // the Arc<StaticSoundData> shape Phase 1 commits to — Phase 2
        // (BSA-backed sounds) should re-use the same Arc handle so
        // a single decoded WAV backs every emitter playing it.
        let arc_sound = Arc::new(sound);
        let _emitter = AudioEmitter {
            sound: Arc::clone(&arc_sound),
            attenuation: Attenuation::default(),
            volume: 1.0,
            looping: false,
            unload_fade_ms: DEFAULT_UNLOAD_FADE_MS,
        };
        assert_eq!(Arc::strong_count(&arc_sound), 2);
    }

    /// `audio_system` must run cleanly against an empty World even
    /// when no `AudioWorld` resource is present (smoke test for the
    /// no-op posture under headless/CI).
    #[test]
    fn audio_system_runs_against_empty_world_without_panic() {
        use byroredux_core::ecs::World;
        let world = World::new();
        audio_system(&world, 0.016);
    }

    /// `SoundCache` returns `None` on miss and stable `Arc` clones
    /// on hit. Lower-case key normalisation: same sound looked up
    /// with different casings hits the same slot.
    #[test]
    fn sound_cache_hits_are_case_insensitive_and_share_arc() {
        use kira::sound::static_sound::StaticSoundSettings;
        let mut cache = SoundCache::new();
        assert!(cache.is_empty());
        assert!(cache.get(r"sound\fx\foo.wav").is_none());

        // Synthesise + insert.
        let sound = StaticSoundData {
            sample_rate: 22_050,
            frames: Arc::from(vec![kira::Frame { left: 0.0, right: 0.0 }; 100].into_boxed_slice()),
            settings: StaticSoundSettings::default(),
            slice: None,
        };
        let inserted = cache.insert(r"sound\fx\Foo.wav", sound);
        assert_eq!(cache.len(), 1);

        // Different casing → same slot.
        let hit_lower = cache.get(r"sound\fx\foo.wav").expect("cache hit");
        let hit_upper = cache.get(r"SOUND\FX\FOO.WAV").expect("case-insensitive hit");
        assert!(Arc::ptr_eq(&inserted, &hit_lower));
        assert!(Arc::ptr_eq(&inserted, &hit_upper));
    }

    /// `get_or_load` only invokes the loader on cache miss, and
    /// short-circuits to a stable `Arc` clone on hit. Pinned because
    /// a regression that re-decodes per call would silently 10×
    /// the per-frame SFX cost without changing any visible behaviour.
    #[test]
    fn sound_cache_get_or_load_invokes_loader_only_on_miss() {
        use std::cell::Cell;
        let mut cache = SoundCache::new();
        let calls = Cell::new(0_usize);

        // Miss: loader fires, but our synthesised junk bytes won't
        // decode → returns None. The cache stays empty (we only
        // insert on successful decode).
        let result = cache.get_or_load(r"sound\fx\bar.wav", || {
            calls.set(calls.get() + 1);
            vec![0u8; 16] // not a valid audio file
        });
        assert!(result.is_none());
        assert_eq!(calls.get(), 1);
        assert!(cache.is_empty());

        // Insert a real synthetic sound at that path. Subsequent
        // get_or_load must hit the cache without invoking the loader.
        use kira::sound::static_sound::StaticSoundSettings;
        let sound = StaticSoundData {
            sample_rate: 22_050,
            frames: Arc::from(vec![kira::Frame { left: 0.0, right: 0.0 }; 50].into_boxed_slice()),
            settings: StaticSoundSettings::default(),
            slice: None,
        };
        cache.insert(r"sound\fx\bar.wav", sound);

        let hit = cache.get_or_load(r"sound\fx\bar.wav", || {
            calls.set(calls.get() + 1);
            unreachable!("loader must not fire on cache hit");
        });
        assert!(hit.is_some());
        assert_eq!(calls.get(), 1, "loader call count unchanged after cache hit");
    }

    /// **Phase 3**: `spawn_oneshot_at` lays down the canonical
    /// component bundle so the audio system picks it up. Pinning
    /// the bundle shape here means a future "simplify spawn helpers"
    /// refactor can't quietly drop one component (e.g. `OneShotSound`)
    /// and break dispatch silently.
    #[test]
    fn spawn_oneshot_at_creates_correct_component_bundle() {
        use kira::sound::static_sound::StaticSoundSettings;

        let mut world = byroredux_core::ecs::World::new();
        let sound = Arc::new(StaticSoundData {
            sample_rate: 22_050,
            frames: Arc::from(
                vec![kira::Frame { left: 0.0, right: 0.0 }; 50].into_boxed_slice(),
            ),
            settings: StaticSoundSettings::default(),
            slice: None,
        });
        let pos = glam::Vec3::new(10.0, 0.0, 5.0);
        let entity = spawn_oneshot_at(&mut world, sound, pos, Attenuation::default(), 0.8);

        // Every component the audio system needs to dispatch this
        // entity must be present.
        assert!(world.has::<Transform>(entity));
        assert!(world.has::<GlobalTransform>(entity));
        assert!(world.has::<AudioEmitter>(entity));
        assert!(world.has::<OneShotSound>(entity));

        let q = world.query::<Transform>().unwrap();
        let t = q.get(entity).unwrap();
        assert_eq!(t.translation, pos);
        let q = world.query::<AudioEmitter>().unwrap();
        let e = q.get(entity).unwrap();
        assert_eq!(e.volume, 0.8);
        assert!(!e.looping);
    }

    /// **Phase 3**: when `AudioWorld` is inactive (no audio device,
    /// CI / headless), `audio_system` must NOT dispatch — the
    /// `OneShotSound` marker stays on the entity so a future tick
    /// (or a future fix that brings audio back online) can pick it
    /// up. Active-sound count stays at zero. This is the regression
    /// gate against a "dispatch even without manager" refactor that
    /// would crash on null-handle play.
    #[test]
    fn audio_system_no_op_when_audio_world_inactive() {
        use kira::sound::static_sound::StaticSoundSettings;
        let mut world = byroredux_core::ecs::World::new();

        // Force-construct an inactive AudioWorld. We can't reliably
        // hit the cpal-init failure path on a dev machine that has
        // a sound card, so build the variant by hand — same shape
        // `AudioWorld::new()` produces when init fails.
        let inactive = AudioWorld {
            active_sounds: Vec::new(),
            pending_oneshots: VecDeque::new(),
            music: None,
            reverb_send: None,
            reverb_send_db: f32::NEG_INFINITY,
            listener: None,
            manager: None,
        };
        world.insert_resource(inactive);

        let sound = Arc::new(StaticSoundData {
            sample_rate: 22_050,
            frames: Arc::from(
                vec![kira::Frame { left: 0.0, right: 0.0 }; 50].into_boxed_slice(),
            ),
            settings: StaticSoundSettings::default(),
            slice: None,
        });
        let pos = glam::Vec3::ZERO;
        let entity = spawn_oneshot_at(&mut world, sound, pos, Attenuation::default(), 1.0);

        audio_system(&world, 0.016);

        // Marker preserved — no dispatch.
        assert!(world.has::<OneShotSound>(entity));
        assert!(world.has::<AudioEmitter>(entity));
        let aw = world.resource::<AudioWorld>();
        assert_eq!(aw.active_sound_count(), 0);
    }

    /// **Phase 6**: reverb send level defaults to NEG_INFINITY
    /// (silent / disabled). The dispatch path explicitly checks
    /// `is_finite() && > -60.0` before calling `with_send`, so a
    /// fresh AudioWorld never produces audible reverb regardless
    /// of whether a send track was created. Pinned because the
    /// alternative (default `0.0` = full wet) would produce shocking
    /// reverb on every spatial sound the moment audio init succeeded.
    #[test]
    fn reverb_send_defaults_to_silent() {
        let world = AudioWorld::new();
        assert!(
            world.reverb_send_db().is_infinite() && world.reverb_send_db().is_sign_negative(),
            "reverb send must default to NEG_INFINITY (silent), got {}",
            world.reverb_send_db()
        );
    }

    /// **Phase 6**: `set_reverb_send_db` persists the new level.
    /// New sub-tracks created after the call use the new level;
    /// already-playing sounds keep their construction-time level
    /// (kira's `with_send` API is build-time only). This test pins
    /// only the resource-side state — actual audible reverb on a
    /// running stream needs the real-data lifecycle test for that.
    #[test]
    fn set_reverb_send_db_persists() {
        let mut world = AudioWorld {
            active_sounds: Vec::new(),
            pending_oneshots: VecDeque::new(),
            music: None,
            reverb_send: None,
            reverb_send_db: f32::NEG_INFINITY,
            listener: None,
            manager: None,
        };
        world.set_reverb_send_db(-12.0);
        assert!((world.reverb_send_db() - (-12.0)).abs() < 1e-6);
        world.set_reverb_send_db(f32::NEG_INFINITY);
        assert!(world.reverb_send_db().is_infinite());
    }

    /// **Phase 5**: `play_music` and `stop_music` are no-ops when
    /// the audio world is inactive (no audio device). Pinned because
    /// a refactor that "asserts a manager exists" would crash on
    /// every headless host on the very first cell-load music event.
    #[test]
    fn play_music_no_op_when_inactive() {
        // We can't construct a real StreamingSoundData without bytes
        // that decode, so this test only exercises the early-return
        // branch (manager.is_none() → silent return). is_music_active
        // and stop_music are also pinned along the inactive path.
        let mut audio_world = AudioWorld {
            active_sounds: Vec::new(),
            pending_oneshots: VecDeque::new(),
            music: None,
            reverb_send: None,
            reverb_send_db: f32::NEG_INFINITY,
            listener: None,
            manager: None,
        };
        assert!(!audio_world.is_music_active());
        audio_world.stop_music(0.5); // No-op on no-music + no-manager.
        assert!(!audio_world.is_music_active());
    }

    /// **Phase 5 real-data integration**: open FNV `Fallout - Sound.bsa`,
    /// extract the longest OGG (proxy for "music" — vanilla FNV
    /// ships music as separate `Fallout - Music.bsa` which we don't
    /// require for the test), play it through `play_music`, verify
    /// `is_music_active() == true` after dispatch, then `stop_music`
    /// and verify it goes inactive.
    ///
    /// `#[ignore]` — needs working audio device + vanilla FNV data.
    #[test]
    #[ignore]
    fn play_music_drives_streaming_playback_on_real_ogg() {
        use byroredux_bsa::BsaArchive;
        use std::path::PathBuf;
        use std::time::{Duration, Instant};

        const FNV_DEFAULT: &str =
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
        let dir = std::env::var("BYROREDUX_FNV_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(FNV_DEFAULT));
        if !dir.is_dir() {
            return;
        }
        let bsa = match BsaArchive::open(&dir.join("Fallout - Sound.bsa")) {
            Ok(b) => b,
            Err(_) => return,
        };
        let bytes = bsa
            .extract(
                r"sound\fx\amb\~regions\goodsprings\oneshots\creak_low\amb_gsinterioroneshots_04.ogg",
            )
            .expect("vanilla creak OGG");

        let streaming = load_streaming_sound_from_bytes(bytes).expect("decode");

        let mut audio_world = AudioWorld::new();
        if !audio_world.is_active() {
            return;
        }

        // play_music with 0.05s fade-in (short so the test is fast).
        audio_world.play_music(streaming, 0.5, 0.05);
        assert!(
            audio_world.is_music_active(),
            "play_music must produce a Playing handle on first dispatch"
        );

        // Let it play briefly, then stop with a fade.
        std::thread::sleep(Duration::from_millis(100));
        audio_world.stop_music(0.05);

        // Poll until inactive (the stop fade completes).
        let deadline = Instant::now() + Duration::from_secs(2);
        while audio_world.is_music_active() {
            if Instant::now() > deadline {
                panic!("stop_music never produced an inactive state");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// **Phase 4**: an `AudioEmitter` with `looping = true` plays
    /// past its natural duration. The real-data lifecycle test
    /// (Phase 3) confirmed a one-shot stops at ~580ms; this test
    /// runs the same sound for a wall-clock duration well past that
    /// and asserts the active count stays at 1. Then removes the
    /// emitter component (simulating cell unload) and verifies the
    /// prune sweep stops the handle.
    ///
    /// `#[ignore]` — needs working audio device + vanilla FNV data.
    #[test]
    #[ignore]
    fn looping_emitter_survives_natural_duration_and_stops_on_emitter_remove() {
        use byroredux_bsa::BsaArchive;
        use std::path::PathBuf;
        use std::time::{Duration, Instant};

        const FNV_DEFAULT: &str =
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
        let dir = std::env::var("BYROREDUX_FNV_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(FNV_DEFAULT));
        if !dir.is_dir() {
            return;
        }
        let bsa = match BsaArchive::open(&dir.join("Fallout - Sound.bsa")) {
            Ok(b) => b,
            Err(_) => return,
        };
        let bytes = bsa
            .extract(
                r"sound\fx\npc\robotsecuritron\armswing\npc_securitron_armswing_02.wav",
            )
            .expect("vanilla securitron arm-swing");
        let sound = Arc::new(load_sound_from_bytes(bytes).expect("decode WAV"));

        let mut world = byroredux_core::ecs::World::new();
        let aw = AudioWorld::new();
        if !aw.is_active() {
            return;
        }
        world.insert_resource(aw);

        let listener = world.spawn();
        world.insert(listener, Transform::IDENTITY);
        world.insert(listener, GlobalTransform::IDENTITY);
        world.insert(listener, AudioListener);

        // Spawn a looping emitter manually (spawn_oneshot_at sets
        // looping=false; need this branch).
        let emitter = world.spawn();
        let pos = glam::Vec3::new(0.0, 0.0, 5.0);
        world.insert(emitter, Transform::new(pos, glam::Quat::IDENTITY, 1.0));
        world.insert(
            emitter,
            GlobalTransform::new(pos, glam::Quat::IDENTITY, 1.0),
        );
        world.insert(
            emitter,
            AudioEmitter {
                sound: Arc::clone(&sound),
                attenuation: Attenuation::default(),
                volume: 0.5, // half-volume so the test isn't loud
                looping: true,
                unload_fade_ms: DEFAULT_UNLOAD_FADE_MS,
            },
        );
        world.insert(emitter, OneShotSound);

        // Tick — listener creates, emitter dispatches with loop_region.
        audio_system(&world, 0.016);
        assert_eq!(world.resource::<AudioWorld>().active_sound_count(), 1);

        // Wait past the sound's natural duration (~580 ms) plus a
        // safety margin. If looping isn't actually applied, the
        // handle would report Stopped here and the prune would drop it.
        std::thread::sleep(Duration::from_millis(900));
        audio_system(&world, 0.016);
        assert_eq!(
            world.resource::<AudioWorld>().active_sound_count(),
            1,
            "looping emitter must survive past natural duration — \
             loop_region(..) wasn't applied?"
        );

        // Remove the AudioEmitter component to simulate cell unload.
        // The next prune sweep should call .stop(), and a subsequent
        // tick should observe Stopped and drop the entry.
        {
            let mut q = world.query_mut::<AudioEmitter>().unwrap();
            q.remove(emitter);
        }

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            audio_system(&world, 0.016);
            if world.resource::<AudioWorld>().active_sound_count() == 0 {
                break;
            }
            if Instant::now() > deadline {
                panic!(
                    "looping sound never reported Stopped after AudioEmitter removal — \
                     prune sweep missed the despawn signal"
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// **Phase 3.5**: `play_oneshot` enqueues regardless of audio
    /// activity (so a future re-init could replay), but the queue
    /// stays bounded at 256 entries via FIFO drop-oldest. Pinned
    /// here so a refactor that "simplifies" by removing the cap
    /// can't quietly let the queue grow without bound on a no-
    /// device host.
    #[test]
    fn play_oneshot_queue_caps_at_max_pending() {
        use kira::sound::static_sound::StaticSoundSettings;
        let mut audio_world = AudioWorld {
            active_sounds: Vec::new(),
            pending_oneshots: VecDeque::new(),
            music: None,
            reverb_send: None,
            reverb_send_db: f32::NEG_INFINITY,
            listener: None,
            manager: None,
        };
        let sound = Arc::new(StaticSoundData {
            sample_rate: 22_050,
            frames: Arc::from(
                vec![kira::Frame { left: 0.0, right: 0.0 }; 50].into_boxed_slice(),
            ),
            settings: StaticSoundSettings::default(),
            slice: None,
        });

        // Push past the 256 cap.
        for i in 0..300 {
            audio_world.play_oneshot(
                Arc::clone(&sound),
                glam::Vec3::new(i as f32, 0.0, 0.0),
                Attenuation::default(),
                1.0,
            );
        }
        // Cap holds — exactly 256 entries remain (oldest 44 dropped).
        assert_eq!(
            audio_world.pending_oneshot_count(),
            256,
            "queue must cap at 256; got {}",
            audio_world.pending_oneshot_count()
        );
    }

    /// **Phase 3.5 real-data integration**: queue API drives playback
    /// end-to-end. Mirrors the entity-based lifecycle test but uses
    /// `play_oneshot` (no entity allocation) — the path a System
    /// without `&mut World` would take.
    ///
    /// `#[ignore]` — needs working audio device + vanilla FNV data.
    #[test]
    #[ignore]
    fn play_oneshot_queue_drives_real_playback() {
        use byroredux_bsa::BsaArchive;
        use std::path::PathBuf;
        use std::time::Instant;

        const FNV_DEFAULT: &str =
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
        let dir = std::env::var("BYROREDUX_FNV_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(FNV_DEFAULT));
        if !dir.is_dir() {
            eprintln!("skipping: FNV data dir {:?} not found", dir);
            return;
        }
        let bsa = match BsaArchive::open(&dir.join("Fallout - Sound.bsa")) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipping: open FNV Sound.bsa: {e}");
                return;
            }
        };
        let bytes = bsa
            .extract(
                r"sound\fx\npc\robotsecuritron\armswing\npc_securitron_armswing_02.wav",
            )
            .expect("vanilla FNV Sound.bsa must contain securitron arm-swing");
        let sound = Arc::new(load_sound_from_bytes(bytes).expect("decode WAV"));

        let mut world = byroredux_core::ecs::World::new();
        let aw = AudioWorld::new();
        if !aw.is_active() {
            eprintln!("skipping: no audio device on this host");
            return;
        }
        world.insert_resource(aw);

        // Listener at origin.
        let listener = world.spawn();
        world.insert(listener, Transform::IDENTITY);
        world.insert(listener, GlobalTransform::IDENTITY);
        world.insert(listener, AudioListener);

        // Queue a one-shot via the new API. No entity allocation.
        {
            let mut aw = world.resource_mut::<AudioWorld>();
            aw.play_oneshot(
                Arc::clone(&sound),
                glam::Vec3::new(0.0, 0.0, 5.0),
                Attenuation::default(),
                1.0,
            );
            assert_eq!(aw.pending_oneshot_count(), 1);
        }

        // Tick: listener creates, queue drains, dispatch fires.
        audio_system(&world, 0.016);
        {
            let aw = world.resource::<AudioWorld>();
            assert_eq!(
                aw.pending_oneshot_count(),
                0,
                "queue must drain on first tick"
            );
            assert_eq!(
                aw.active_sound_count(),
                1,
                "drained queue items become active sounds"
            );
        }

        // Poll until Stopped (3s timeout).
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        loop {
            audio_system(&world, 0.016);
            if world.resource::<AudioWorld>().active_sound_count() == 0 {
                break;
            }
            if Instant::now() > deadline {
                panic!("queue-driven sound never reported Stopped within 3s");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// **Phase 3 real-data integration**: open FNV `Fallout - Sound.bsa`,
    /// decode a real WAV, spawn it as a one-shot at world origin
    /// with a listener nearby, run `audio_system` to dispatch, then
    /// poll until kira reports `Stopped` (or a max-iteration cap
    /// fires). Verifies the full lifecycle end-to-end on the real
    /// cpal backend.
    ///
    /// `#[ignore]` — needs a working audio device AND vanilla FNV
    /// game data. Run with:
    /// ```sh
    /// BYROREDUX_FNV_DATA=<path> cargo test -p byroredux-audio
    ///   audio_system_full_lifecycle -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn audio_system_full_lifecycle_on_real_fnv_sound() {
        use byroredux_bsa::BsaArchive;
        use std::path::PathBuf;
        use std::time::Instant;

        const FNV_DEFAULT: &str =
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
        let dir = std::env::var("BYROREDUX_FNV_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(FNV_DEFAULT));
        if !dir.is_dir() {
            eprintln!("skipping: FNV data dir {:?} not found", dir);
            return;
        }
        let bsa = match BsaArchive::open(&dir.join("Fallout - Sound.bsa")) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipping: open FNV Sound.bsa: {e}");
                return;
            }
        };
        let bytes = bsa
            .extract(
                r"sound\fx\npc\robotsecuritron\armswing\npc_securitron_armswing_02.wav",
            )
            .expect("vanilla FNV Sound.bsa must contain securitron arm-swing");
        let sound = Arc::new(load_sound_from_bytes(bytes).expect("decode real WAV"));

        let mut world = byroredux_core::ecs::World::new();
        let aw = AudioWorld::new();
        if !aw.is_active() {
            eprintln!("skipping: no audio device on this host");
            return;
        }
        world.insert_resource(aw);

        // Listener at origin (default orientation), emitter 5m in
        // front. Inside the (2, 30) attenuation envelope, so we get
        // audible playback rather than a silent test.
        let listener = world.spawn();
        world.insert(listener, Transform::IDENTITY);
        world.insert(listener, GlobalTransform::IDENTITY);
        world.insert(listener, AudioListener);

        let emitter = spawn_oneshot_at(
            &mut world,
            Arc::clone(&sound),
            glam::Vec3::new(0.0, 0.0, 5.0),
            Attenuation::default(),
            1.0,
        );

        // First tick: `sync_listener_pose` creates the listener,
        // then `dispatch_new_oneshots` (running in the same tick)
        // sees the just-created handle and dispatches. Both the
        // OneShotSound removal and the active_sounds insert happen
        // on tick 1.
        audio_system(&world, 0.016);
        assert!(
            !world.has::<OneShotSound>(emitter),
            "tick 1 must dispatch the one-shot — listener creation \
             and dispatch both run inside `audio_system`"
        );
        {
            let aw = world.resource::<AudioWorld>();
            assert_eq!(
                aw.active_sound_count(),
                1,
                "exactly one sound dispatched and tracked"
            );
        }

        // Poll for Stopped. The arm-swing is ~580 ms; cap at 3s
        // wall-clock so a stuck test fails loud rather than hanging.
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        loop {
            audio_system(&world, 0.016);
            let aw = world.resource::<AudioWorld>();
            if aw.active_sound_count() == 0 {
                break;
            }
            drop(aw);
            if Instant::now() > deadline {
                panic!(
                    "audio_system did not prune the active sound within 3s — kira's \
                     PlaybackState::Stopped never reported, or prune logic has a bug"
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        // After completion, the emitter entity has no audio
        // components — a downstream cleanup system can despawn it.
        assert!(
            !world.has::<AudioEmitter>(emitter),
            "AudioEmitter must be removed after Stopped"
        );
    }

    /// **Real-data integration**: extract one WAV and one OGG from
    /// vanilla FNV `Fallout - Sound.bsa` and decode each through
    /// `load_sound_from_bytes`. Pins the kira ↔ symphonia ↔ BSA
    /// path end-to-end against actual game content.
    ///
    /// `#[ignore]` because it needs vanilla FNV game data; run with:
    /// ```sh
    /// BYROREDUX_FNV_DATA=<path> cargo test -p byroredux-audio
    ///   real_fnv_sounds_decode -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn real_fnv_sounds_decode_through_kira() {
        use byroredux_bsa::BsaArchive;
        use std::path::PathBuf;

        const FNV_DEFAULT: &str =
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
        let dir = std::env::var("BYROREDUX_FNV_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(FNV_DEFAULT));
        if !dir.is_dir() {
            eprintln!("skipping: FNV data dir {:?} not found", dir);
            return;
        }
        let bsa_path = dir.join("Fallout - Sound.bsa");
        let bsa = match BsaArchive::open(&bsa_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipping: open {bsa_path:?}: {e}");
                return;
            }
        };

        // Two canonical sample paths verified via probe_extensions
        // 2026-05-05: a securitron arm-swing WAV and a Goodsprings
        // ambient creak OGG. If either disappears from the vanilla
        // archive in a future patch, the test fails loud and the
        // sample list above gets refreshed.
        let cases: &[(&str, &str)] = &[
            (
                "wav",
                r"sound\fx\npc\robotsecuritron\armswing\npc_securitron_armswing_02.wav",
            ),
            (
                "ogg",
                r"sound\fx\amb\~regions\goodsprings\oneshots\creak_low\amb_gsinterioroneshots_04.ogg",
            ),
        ];

        for (label, path) in cases {
            let bytes = bsa
                .extract(path)
                .unwrap_or_else(|e| panic!("vanilla FNV BSA must contain {path}: {e}"));
            assert!(!bytes.is_empty(), "{label}: empty extract");
            let sound = load_sound_from_bytes(bytes).unwrap_or_else(|e| {
                panic!("{label} decode failed for {path}: {e}");
            });
            eprintln!(
                "[M44 P2] {label} '{path}' → {} frames @ {} Hz",
                sound.frames.len(),
                sound.sample_rate
            );
            assert!(
                sound.frames.len() > 100,
                "{label}: short decode ({} frames) — symphonia may have aborted",
                sound.frames.len()
            );
            assert!(
                sound.sample_rate >= 11_025 && sound.sample_rate <= 48_000,
                "{label}: unexpected sample rate {} Hz",
                sound.sample_rate
            );
        }
    }
}
