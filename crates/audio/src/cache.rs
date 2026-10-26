//! Sound loaders and the `SoundCache` / `VoiceSoundCache` resources.

use super::*;

/// Decode a fully-buffered audio blob into a `StaticSoundData`.
///
/// Takes an owned `Vec<u8>` — typically extracted from a Bethesda BSA
/// via [`byroredux_bsa::BsaArchive::extract`] — though kira 0.12 relaxed
/// `StaticSoundData::from_cursor` to `T: AsRef<[u8]> + Send + Sync` (no
/// longer `'static`), since the data is fully decoded during the call.
///
/// Format detection is automatic via symphonia's probe. The workspace
/// explicitly enables kira's `wav` + `pcm`, `ogg` + `vorbis`, `mp3`, and
/// `flac` features in the root `Cargo.toml` (the container features need
/// their matching codec features). The two formats present in vanilla
/// `Fallout - Sound.bsa`
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

/// Play a streaming sound into one main-track music slot, fading out
/// whatever the slot held over the same duration so the two overlap into a
/// crossfade. Shared by the cell music and the scripted layer (#6226).
pub(crate) fn play_main_track(
    mgr: &mut AudioManager<DefaultBackend>,
    slot: &mut Option<StreamingSoundHandle<FromFileError>>,
    streaming_sound: StreamingSoundData<FromFileError>,
    volume: f32,
    fade_in_secs: f32,
    looping: bool,
) {
    let fade = Tween {
        start_time: kira::StartTime::Immediate,
        duration: Duration::from_secs_f32(fade_in_secs.max(0.0)),
        easing: kira::Easing::Linear,
    };
    if let Some(existing) = slot.as_mut() {
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
        Ok(handle) => *slot = Some(handle),
        Err(e) => {
            log::warn!("M44 Phase 5: play_music failed: {e}");
            *slot = None;
        }
    }
}

/// Fade one main-track music slot out and empty it. Kira keeps the sound
/// alive internally until the fade completes; the slot is cleared at once
/// so a later play never sees a stale handle.
pub(crate) fn stop_main_track(slot: &mut Option<StreamingSoundHandle<FromFileError>>, fade_out_secs: f32) {
    let Some(handle) = slot.as_mut() else {
        return;
    };
    handle.stop(Tween {
        start_time: kira::StartTime::Immediate,
        duration: Duration::from_secs_f32(fade_out_secs.max(0.0)),
        easing: kira::Easing::Linear,
    });
    *slot = None;
}

/// **Phase 5**: streaming variant of [`load_streaming_sound_from_bytes`]
/// that opens the file lazily — kira holds an `std::fs::File` and
/// pulls decoded frames as the playback head advances. Use this for
/// loose `Data/Music/*.mp3` / `*.wav` files that aren't archived.
///
/// Scripted `PlayMusic` uses it for the loose `Data\Music` files Fallout 3
/// / New Vegas ship (#6226); archive-extracted music streams through
/// [`load_streaming_sound_from_bytes`].
pub fn load_streaming_sound_from_file(
    path: impl AsRef<std::path::Path>,
) -> Result<StreamingSoundData<FromFileError>, FromFileError> {
    StreamingSoundData::from_file(path)
}

/// Process-lifetime cache of decoded `StaticSoundData`, keyed by
/// lowercased asset path. Repeat plays of the same SFX (footsteps,
/// weapon fire) skip the decode cost entirely —
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
/// Only a closed keyspace belongs here: today the combat swing / impact /
/// death sounds. Dialogue voice (#5382) and script-chosen sounds
/// (`Sound.Play`, `PlaySound`, graph `SoundPlay.*`, #6281) have open-ended
/// keys and ride the byte-budgeted [`VoiceSoundCache`].
///
/// `None` values represent negative-cache entries and are not counted by
/// `len()` or `bytes_estimate()`: telemetry measures decoded audio retained
/// for playback, while missing/invalid paths stay cached to avoid repeated
/// archive work.
pub struct SoundCache {
    // `None` is a negative-cache entry: a missing archive member or a
    // malformed sound is not probed again on every gameplay event.
    pub(crate) map: HashMap<String, Option<Arc<StaticSoundData>>>,
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
/// so voice rides this separate cache instead of the shared one. Sounds a
/// script or a graph event names (`Sound.Play`, `PlaySound`, `SoundPlay.*`,
/// #6281) are the second open-ended keyspace and share it: it is the engine's
/// bounded cache, whatever the key's origin.
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
    pub(crate) budget_bytes: usize,
    /// LRU order — index 0 is most recently used.
    pub(crate) entries: Vec<(String, Arc<StaticSoundData>, usize)>,
    pub(crate) resident_bytes: usize,
    pub(crate) negatives: std::collections::HashSet<String>,
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

    pub(crate) fn remember_negative(&mut self, key: String) {
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
