//! Audio subsystem (M44).
//!
//! 3D positional audio backed by [`kira`]. Per the 2026-05-03 priority
//! review, this is the "feels like a game" gap that converts
//! "we render Bethesda content" into "we run Bethesda content."
//! Better-than-Bethesda axis: proper reverb zones, full HRTF where
//! kira allows, no Wwise/FMOD middleware tax.
//!
//! # Which entry point the engine uses
//!
//! Every shipped sound (footsteps, impacts, splashes, dialogue voice,
//! ambient and music) goes through [`AudioWorld`]'s queue API —
//! [`AudioWorld::play_oneshot`] and friends — which needs only `&World`.
//! The entity-shaped flow (`OneShotSound` + [`AudioEmitter`] spawned by
//! [`spawn_oneshot_at`], Phases 1 and 3) has no engine producer; it is kept
//! for tests and tooling that want a one-off emitter entity, and its
//! dispatch path is exercised only by `tests.rs`.
//!
//! # Phase 1
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
//! # Phase 2
//!
//! - [`load_sound_from_bytes`] — decode a fully-buffered audio blob
//!   (typically extracted from a Bethesda BSA) through kira's
//!   symphonia-backed `StaticSoundData::from_cursor` path. The workspace
//!   enables WAV/PCM, OGG/Vorbis, MP3 (Oblivion voice, #5526) and FLAC
//!   explicitly in `Cargo.toml`.
//! - [`SoundCache`] — process-lifetime path-keyed cache of decoded
//!   `Arc<StaticSoundData>`. Repeat plays of the same SFX (footsteps,
//!   weapon fire) skip the decode cost entirely. Open-ended keyspaces —
//!   dialogue voice (#5382) and script-chosen sounds (#6281) — use the
//!   bounded [`VoiceSoundCache`] instead.
//!
//! # Phase 3
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
//!   bundle on a fresh entity. No engine code calls it: gameplay
//!   systems hold `&World` and cannot spawn, so they use
//!   [`AudioWorld::play_oneshot`] (Phase 3.5). It remains for tests and
//!   tooling that want the entity-shaped flow.
//!
//! # Phase 3.5
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
//! # Phase 4
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
//! # Phase 5
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
//! # Phase 6
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
//!   *new* sounds today — the engine does not call kira's
//!   `SpatialTrackHandle::set_send` to retarget existing routes. Kira
//!   can change an existing route's level, but `set_send` cannot add a
//!   route to a track built dry; `apply_reverb_send` omits `with_send`
//!   while the send level is muted. Short SFX naturally pick up a new
//!   level as new sounds replace old ones.
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
//! # Future work
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

mod cache;
mod components;
mod dsp;
mod system;
mod world;

pub use cache::*;
pub use components::*;
pub use system::*;
pub use world::*;

use dsp::*;


// Re-export the kira types downstream crates need so they can hold
// `Arc<StaticSoundData>` (in `Resource`s, components, etc.) without
// pulling kira as a direct dependency. The audio crate is the canon
// owner of the audio-engine surface.
pub use kira::Frame;
pub use kira::sound::static_sound::{
    StaticSoundData as Sound, StaticSoundSettings as SoundSettings,
};

pub mod fuz;

#[cfg(test)]
mod tests;
