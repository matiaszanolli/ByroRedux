//! `AudioWorld` — the kira manager + listener + active-sound bookkeeping
//! resource, and the sound/pending-one-shot types it owns.

use super::*;

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
///   unless [`AudioWorld::stop_sounds_for`] stops it or the followed
///   entity loses `GlobalTransform` (dialogue voice: the NPC carries no
///   `AudioEmitter`, and its line must survive SFX emitter churn while
///   still tracking a walking speaker).
///
/// Whether the underlying kira sound is looping (set via
/// `loop_region(..)` at dispatch) is decided at the `Pending` /
/// `AudioEmitter` layer; `ActiveSound` itself doesn't need to carry
/// that bit post-#858 since the prune sweep no longer branches on it.
pub(crate) struct ActiveSound {
    pub(crate) source: SoundSource,
    pub(crate) handle: StaticSoundHandle,
    pub(crate) track: SpatialTrackHandle,
    /// The position (audio-space metres) the spatial track was last
    /// placed at — initialised from the dispatch position so a
    /// stationary emitter pushes zero per-tick commands, and compared
    /// by [`sync_emitter_positions`] so a moving source pushes one
    /// `set_position` only on ticks where it actually moved (#3086).
    pub(crate) last_position: Option<Vec3>,
    /// Low-pass control kept with the spatial track so water transitions can
    /// be applied to sounds that are already playing.
    pub(crate) underwater_filter: FilterHandle,
    pub(crate) underwater: bool,
    /// Fade-out duration captured from `AudioEmitter.unload_fade_ms` at
    /// dispatch time. Read by `prune_stopped_sounds` when the source
    /// entity loses its emitter component or an anchored source loses its
    /// transform. Applies to looping AND non-looping post-#858 / SAFE-23 —
    /// one-shots usually terminate naturally before the fade is needed,
    /// but mid-playback unload routes through the same tween. See #845.
    pub(crate) unload_fade_ms: f32,
    /// Set to `true` once `prune_stopped_sounds` has issued the
    /// fade-out `stop` call for this active sound. The handle's
    /// `state()` won't report `Stopped` until the fade completes, so
    /// without this flag the prune walk would re-mark the same entry
    /// every tick during the fade window — kira treats repeated
    /// `stop` as idempotent in effect, but the redundant ringbuf
    /// commands and re-walk cost are unnecessary. Becomes more
    /// visible if the fade duration is tuned up. See #844.
    pub(crate) stop_issued: bool,
}

/// How an [`ActiveSound`] couples to an entity — see the struct doc for
/// the full contract of each mode (#5410 split the old
/// `Option<EntityId>` into three modes because entity-following and
/// emitter-lifecycle are independent: a dialogue voice needs the first
/// and must not have the second).
pub(crate) enum SoundSource {
    /// Queue-driven fire-and-forget: position frozen at queue time.
    Detached,
    /// `OneShotSound + AudioEmitter` dispatch: follows the entity, stops
    /// with the emitter component, removes it on completion.
    Emitter(EntityId),
    /// Entity-anchored queue play: follows the entity, stops via
    /// [`AudioWorld::stop_sounds_for`] or when the entity loses its
    /// `GlobalTransform`; no emitter lifecycle.
    Anchored(EntityId),
}

impl SoundSource {
    /// The followed entity, if this sound has one (both `Emitter` and
    /// `Anchored` track a `GlobalTransform`).
    pub(crate) fn followed_entity(&self) -> Option<EntityId> {
        match self {
            SoundSource::Detached => None,
            SoundSource::Emitter(entity) | SoundSource::Anchored(entity) => Some(*entity),
        }
    }

    /// Whether the prune pass's emitter-presence coupling applies (only
    /// `Emitter` — an `Anchored` voice must survive SFX emitter churn on
    /// the same NPC).
    pub(crate) fn emitter_coupled(&self) -> Option<EntityId> {
        match self {
            SoundSource::Emitter(entity) => Some(*entity),
            SoundSource::Detached | SoundSource::Anchored(_) => None,
        }
    }
}

pub(crate) fn orphaned_anchored_entity(source: &SoundSource, has_global_transform: bool) -> Option<EntityId> {
    match source {
        SoundSource::Anchored(entity) if !has_global_transform => Some(*entity),
        _ => None,
    }
}

/// One-shot queued through [`AudioWorld::play_oneshot`] (detached) or
/// [`AudioWorld::play_oneshot_following`] (entity-anchored). Drained and
/// dispatched by `audio_system` at the start of each frame. Lives in
/// `AudioWorld` rather than as ECS components so callers without
/// `&mut World` (Systems) can still trigger sounds.
pub(crate) struct PendingOneShot {
    pub(crate) sound: Arc<StaticSoundData>,
    pub(crate) position: Vec3,
    pub(crate) attenuation: Attenuation,
    pub(crate) volume: f32,
    /// #5410 — `Detached` for plain `play_oneshot`; `Anchored(entity)`
    /// for a voice line whose speaker may walk while talking.
    pub(crate) source: SoundSource,
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
    pub(crate) active_sounds: Vec<ActiveSound>,
    /// Queued fire-and-forget one-shots from [`Self::play_oneshot`].
    /// Drained at the start of each `audio_system` tick so callers
    /// who can't allocate entities (Systems) can still trigger
    /// sounds. Phase 3.5. Stored as a `VecDeque` so the cap-eviction
    /// path in `play_oneshot` is O(1) `pop_front` rather than O(n)
    /// `Vec::remove(0)` shift-down. See #852.
    pub(crate) pending_oneshots: VecDeque<PendingOneShot>,
    /// Monotonic count of one-shot dispatch requests, including headless
    /// calls discarded before queueing. Used by device-less smoke gates.
    pub(crate) oneshots_requested: u64,
    /// Single-slot music handle (Phase 5). Music is non-spatial —
    /// it routes through the main track, not a spatial sub-track.
    /// Calling `play_music` while a track is already playing fades
    /// the old one out and the new one in (crossfade).
    pub(crate) music: Option<StreamingSoundHandle<FromFileError>>,
    /// #6226 — the scripted music layer (`PlayMusic`): non-spatial on the
    /// main track like [`Self::music`], but a slot of its own, since a
    /// scripted track "will not stop the currently playing music of the
    /// cell" (GECK *PlayMusic*). A second scripted track replaces the first.
    pub(crate) script_music: Option<StreamingSoundHandle<FromFileError>>,
    /// Reverb send track (Phase 6). Created on manager init when
    /// the audio device is available. Each spatial sub-track opts
    /// into routing signal here via `with_send` at construction
    /// time; the per-track send level is `reverb_send_db` at the
    /// moment the track is built. `None` when the manager itself is
    /// inactive or send-track creation failed.
    pub(crate) reverb_send: Option<SendTrackHandle>,
    /// Per-new-spatial-track reverb send level in dB. Default
    /// `f32::NEG_INFINITY` = no reverb. Cell-load logic flips this
    /// to `-12.0` (subtle) for interior cells.
    pub(crate) reverb_send_db: f32,
    /// Lazily-created kira listener — the entity whose
    /// `GlobalTransform` drives spatial attenuation. Created on the
    /// first frame an `AudioListener` is found in the World.
    pub(crate) listener: Option<ListenerHandle>,
    /// kira manager. `None` means no audio device was acquired; every
    /// audio operation no-ops.
    pub(crate) manager: Option<AudioManager<DefaultBackend>>,
    /// One-shot debounce for the multi-`AudioListener` diagnostic
    /// (#843). Set to `true` the first frame `sync_listener_pose`
    /// observes more than one entity carrying the marker; suppresses
    /// per-frame log spam during third-person camera transitions or
    /// fly-cam swaps where two listener entities briefly coexist.
    pub(crate) multi_listener_warned: bool,
    /// Whether the active listener is head-submerged. The engine water
    /// system updates this before `audio_system` dispatches new sounds.
    pub(crate) underwater: bool,
    /// Commands pushed by [`sync_emitter_positions`] over the process
    /// lifetime (#3086). One push per entity-backed emitter per tick it
    /// actually moved — the per-frame cost of the emitter follow pass
    /// is this counter's delta, and a stationary emitter pushes zero.
    /// Surfaced for tests and smoke gates the same way
    /// [`Self::oneshots_requested`] is.
    pub(crate) emitter_position_updates: u64,
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
            script_music: None,
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
    /// #5738 — the shared `play_oneshot*` prologue: the requested-count
    /// bump, the inactive-manager gate, and the 256-entry FIFO cap
    /// (O(1) front-pop — `Vec::remove(0)` was O(n); see #852). Returns
    /// whether the caller should push its entry.
    pub(crate) fn oneshot_slot_ready(&mut self) -> bool {
        self.oneshots_requested = self.oneshots_requested.saturating_add(1);
        if self.manager.is_none() {
            return false;
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
        true
    }

    pub fn play_oneshot(
        &mut self,
        sound: Arc<StaticSoundData>,
        position: Vec3,
        attenuation: Attenuation,
        volume: f32,
    ) {
        self.queue_oneshot(SoundSource::Detached, sound, position, attenuation, volume);
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
        self.queue_oneshot(SoundSource::Anchored(entity), sound, position, attenuation, volume);
    }

    /// The shared tail of [`Self::play_oneshot`] / [`Self::play_oneshot_following`]: claim a
    /// queue slot (dropping the oldest at the cap) and queue the play from `source`.
    pub(crate) fn queue_oneshot(
        &mut self,
        source: SoundSource,
        sound: Arc<StaticSoundData>,
        position: Vec3,
        attenuation: Attenuation,
        volume: f32,
    ) {
        if !self.oneshot_slot_ready() {
            return;
        }
        self.pending_oneshots.push_back(PendingOneShot { sound, position, attenuation, volume, source });
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
        play_main_track(mgr, &mut self.music, streaming_sound, volume, fade_in_secs, looping);
    }

    /// **Phase 5**: stop the currently-playing music with a fade-out.
    /// No-op when nothing is playing or when the manager is inactive.
    pub fn stop_music(&mut self, fade_out_secs: f32) {
        stop_main_track(&mut self.music, fade_out_secs);
    }

    /// #6226 — play a scripted track (`PlayMusic`) on its own main-track
    /// layer, over the cell's music rather than in place of it: streamed,
    /// non-spatial, with no reverb send or underwater filter, exactly as
    /// [`Self::play_music`] plays the cell's. A scripted track already
    /// playing crossfades out. No-op when the manager is inactive.
    pub fn play_script_music(
        &mut self,
        streaming_sound: StreamingSoundData<FromFileError>,
        volume: f32,
        fade_in_secs: f32,
        looping: bool,
    ) {
        let Some(mgr) = self.manager.as_mut() else {
            return;
        };
        play_main_track(mgr, &mut self.script_music, streaming_sound, volume, fade_in_secs, looping);
    }

    /// #6226 — stop the scripted music layer (a cell change or a load ends
    /// it). No-op when nothing is playing.
    pub fn stop_script_music(&mut self, fade_out_secs: f32) {
        stop_main_track(&mut self.script_music, fade_out_secs);
    }

    /// Whether a scripted track is installed and not yet stopped (see
    /// [`Self::is_music_active`] for the fade-tail caveat).
    pub fn is_script_music_active(&self) -> bool {
        self.script_music
            .as_ref()
            .is_some_and(|h| !matches!(h.state(), PlaybackState::Stopped))
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
    /// **Current limitation (#847):** this changes the level used for
    /// newly-built spatial tracks; the engine does not retarget existing
    /// routes. Kira 0.12 provides `SpatialTrackHandle::set_send` to change
    /// an existing route's level, but it cannot add a missing route. This
    /// builder calls `with_send` only when the send is audible, so a track
    /// built while the level is muted has no route to retarget. Long-running
    /// ambients keep their current level until the engine updates their
    /// route or re-dispatches them. A future live reverb fader can make
    /// every track route through the send at `SILENCE_DB` from construction
    /// time, then use `set_send` to ramp that existing route on and off.
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
