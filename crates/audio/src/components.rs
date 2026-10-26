//! ECS-facing audio components (`AudioListener`, `Attenuation`,
//! `AudioEmitter`, `OneShotSound`) and `spawn_oneshot_at`.

use super::*;

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
    /// Min/max as a kira `RangeInclusive`, normalized so `min < max`.
    /// kira computes attenuation as `distance.clamp(min, max)`, and
    /// `f32::clamp` panics when `min > max` — a reversed range (e.g. from a
    /// future data-driven producer, or hand-edited content) would otherwise
    /// abort the whole audio render thread at playback, invisible to the
    /// dispatch call site. Clamp-normalize at the boundary instead. #1612.
    ///
    /// An equal range is the other degenerate case: kira's linear falloff is
    /// `(distance - min) / (max - min)`, which is `0 / 0 = NaN` when the two
    /// meet, and the NaN gain poisons the whole mix (and, through the reverb
    /// send, every later interior sound). `max` is pushed a few ULPs above
    /// `min`, so a listener at or inside `min` still hears full volume. #6207.
    pub fn distance_range(&self) -> std::ops::RangeInclusive<f32> {
        let lo = self.min_distance.min(self.max_distance);
        let hi = self
            .min_distance
            .max(self.max_distance)
            .max(lo + lo.abs().max(1.0) * f32::EPSILON * 4.0);
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
///
/// The only non-test insert is [`spawn_oneshot_at`], which the engine
/// does not call (it queues through [`AudioWorld::play_oneshot`]); this
/// path is test/tooling-only.
pub struct OneShotSound;

impl Component for OneShotSound {
    type Storage = SparseSetStorage<Self>;
}

/// Spawn a one-shot sound entity at `position` with default
/// orientation. The audio system picks it up next tick (post
/// transform propagation). Returns the entity so callers can attach
/// gameplay components (e.g. parenting under an actor for
/// short-lived position tracking) before the system fires.
///
/// This is the public ECS-shape contract Phase 3 committed to. No engine
/// system uses it — gameplay code queues through
/// [`AudioWorld::play_oneshot`] because a system cannot spawn — so it is
/// exercised by `tests.rs` and available to tooling.
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
