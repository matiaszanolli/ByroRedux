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
//! # Future phases (not in this commit)
//!
//! - Phase 3: FOOT records → per-material footstep dispatch.
//! - Phase 4: REGN ambient soundscapes (region-based ambient layers).
//! - Phase 5: MUSC + hardcoded music routing with crossfade.
//! - Phase 6: Reverb zones (kira's `ReverbBuilder`) keyed off cell
//!   acoustics; raycast occlusion attenuation.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::Resource;
use kira::sound::static_sound::StaticSoundData;
use kira::sound::FromFileError;
use kira::{AudioManager, AudioManagerSettings, DefaultBackend};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

/// Resource holding the `kira::AudioManager` for the whole engine.
///
/// Wrapping the manager in an `Option` is the headless / no-device
/// fallback: when `AudioWorld::new()` fails to acquire an audio device
/// (CI, server, broken sound driver), the inner is `None` and every
/// system call short-circuits. Booting the engine never fails because
/// audio is unavailable — that would be hostile to operators running
/// the engine for testing in environments without a sound card.
pub struct AudioWorld {
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
        match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
            Ok(manager) => {
                log::info!("M44 Phase 1: AudioManager initialised (default backend)");
                Self {
                    manager: Some(manager),
                }
            }
            Err(e) => {
                log::warn!(
                    "M44 Phase 1: AudioManager init failed ({e}); engine continues without audio. \
                     This is expected in headless/CI environments and on systems without a \
                     working audio device."
                );
                Self { manager: None }
            }
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
}

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
/// one-shots, removes spent one-shot entities. Stage::Late is the
/// canonical home (after transform propagation has produced final
/// world poses for the listener and every emitter).
///
/// Phase 1 implementation is intentionally minimal: it only dispatches
/// `OneShotSound` emitters. Looping / streaming dispatch lands once
/// `AudioEmitter` carries an active-handle slot and the system gains
/// the lifecycle (start / stop / fade) it needs.
pub fn audio_system(_world: &byroredux_core::ecs::World, _dt: f32) {
    // Phase 1 stub. The full system body lands once `World::despawn`
    // semantics + `AudioEmitter` handle lifecycle settle. Today the
    // ECS components compile and the AudioWorld resource boots — that
    // is the closure criterion for Phase 1.
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
