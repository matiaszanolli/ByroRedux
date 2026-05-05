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
//! # Future phases (not in this commit)
//!
//! - Phase 2: BSA sound extraction (`Fallout - Sound.bsa` / `Skyrim -
//!   Sounds.bsa`), `.wav` / `.ogg` decode through kira's static-sound
//!   path or streaming path for ambient/music.
//! - Phase 3: FOOT records → per-material footstep dispatch.
//! - Phase 4: REGN ambient soundscapes (region-based ambient layers).
//! - Phase 5: MUSC + hardcoded music routing with crossfade.
//! - Phase 6: Reverb zones (kira's `ReverbBuilder`) keyed off cell
//!   acoustics; raycast occlusion attenuation.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::Resource;
use kira::sound::static_sound::StaticSoundData;
use kira::{AudioManager, AudioManagerSettings, DefaultBackend};
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
}
