//! Footstep, impact and water-audio components, resources and per-frame scratch. (#5467 split from `components.rs`)

use byroredux_audio::Sound;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, Resource, SparseSetStorage};
use byroredux_core::lighting::BETHESDA_UNITS_PER_METER;
use byroredux_core::math::Vec3;
// #5173 — the per-lane ground-cover arrays below size from the same splat
// constant the terrain tile + shader loops size from, so a lane bump cannot
// leave this component silently 8-wide while the rest of the chain moves.
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Marker + accumulator on the entity whose horizontal movement
/// should produce footstep sounds (today the fly-camera entity; an
/// `M28.5` character controller will own this in future).
///
/// `last_position` and `accumulated_stride` are mutated each frame
/// by `footstep_system`; `stride_threshold` is read-only configuration
/// — a stride distance that triggers one footstep. See
/// [`DEFAULT_STRIDE_THRESHOLD_BU`] for the unit, which is the whole
/// story of #3520.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FootstepEmitter {
    /// World position last frame, in renderer Y-up game units. Set
    /// from the entity's `GlobalTransform.translation` on first tick;
    /// updated each frame.
    pub(crate) last_position: Vec3,
    /// Horizontal distance walked since the last footstep fire, in
    /// **Bethesda units** — it accumulates deltas of
    /// `GlobalTransform.translation`, which is BU. XZ plane only —
    /// vertical motion (jumping, falling) doesn't count toward stride.
    pub(crate) accumulated_stride: f32,
    /// Stride distance that triggers a footstep dispatch, in **Bethesda
    /// units**, because that is what it is compared against.
    pub(crate) stride_threshold: f32,
    /// Whether `last_position` has been initialised. False on first
    /// tick so the system seeds it without computing a bogus delta
    /// against the default zero pose.
    pub(crate) initialised: bool,
}

impl Component for FootstepEmitter {
    type Storage = SparseSetStorage<Self>;
}

/// Default footstep stride, in **Bethesda units**.
///
/// #3520 — this was `1.5` with a docstring reading "~1.5m at FNV scale".
/// It is compared against an accumulation of `GlobalTransform.translation`
/// deltas, which are BU (that is the entire premise of #3178's
/// `bu_to_audio_space` seam, which divides the very same value by
/// [`BETHESDA_UNITS_PER_METER`]). So the effective threshold was
/// 1.5 BU = 2.1 cm of travel, not 1.5 m.
///
/// The engine states the travel it is measured against twice, independently:
/// the fly cam's `move_speed: 200.0` BU/s and the character controller's
/// `220.0`. 200 / 60 = 3.33 BU per frame at 60 FPS — already 2.2x the old
/// threshold before the x3 sprint boost. Since the fire branch is an `if`,
/// not a `while`, the observable behaviour was exactly one footstep per
/// frame while moving: 60 Hz against a human cadence of ~1.8 Hz.
///
/// Authored in BU rather than converted at the compare site so
/// `footstep_system` stays entirely in world space and
/// `bu_to_audio_space` remains the subsystem's only unit seam. 0.75 m is a
/// walking stride; at 70 BU/m that is 52.5 BU, ≈2.6 steps/s at 200 BU/s.
pub(crate) const DEFAULT_STRIDE_THRESHOLD_BU: f32 = 0.75 * BETHESDA_UNITS_PER_METER;

impl FootstepEmitter {
    pub(crate) fn new() -> Self {
        Self {
            last_position: Vec3::ZERO,
            accumulated_stride: 0.0,
            stride_threshold: DEFAULT_STRIDE_THRESHOLD_BU,
            initialised: false,
        }
    }
}

/// Resource — engine-wide footstep configuration. Today carries one
/// hardcoded default sound (loaded at startup from a vanilla BSA when
/// available) plus the per-surface map the Wave-4 material carry-through
/// feeds; the map stays empty until authored impact data (IPDS) is
/// decoded into a material→sound mapping, and an absent entry falls back
/// to the default sound — exactly the pre-Wave-4 behavior.
pub(crate) struct FootstepConfig {
    /// Default footstep sound. `None` when the BSA-load failed
    /// (no archive on disk, decode error, audio inactive).
    pub(crate) default_sound: Option<Arc<Sound>>,
    /// Volume multiplier applied to every footstep play. 0.6 keeps
    /// footsteps mixed below dialogue / weapons / music; tweak per
    /// gameplay feel.
    pub(crate) volume: f32,
    /// Footstep sound per Havok surface material
    /// (`RigidBodyData::surface_material` — the per-game HavokMaterial
    /// enum's u32, read from the collider the walker stands on; Skyrim's
    /// values are CRC32s, #5542).
    pub(crate) surface_sounds: std::collections::BTreeMap<u32, Arc<Sound>>,
}

impl Resource for FootstepConfig {}

impl Default for FootstepConfig {
    fn default() -> Self {
        Self {
            default_sound: None,
            volume: 0.6,
            surface_sounds: std::collections::BTreeMap::new(),
        }
    }
}

/// Resource — one-shot sound used for actor/water-surface interaction.
/// `None` is a valid headless/no-archive state and keeps water gameplay
/// independent from audio-device availability.
pub(crate) struct WaterAudioConfig {
    pub(crate) splash_sound: Option<Arc<Sound>>,
    pub(crate) volume: f32,
}

impl Resource for WaterAudioConfig {}

impl Default for WaterAudioConfig {
    fn default() -> Self {
        Self {
            splash_sound: None,
            volume: 0.7,
        }
    }
}

/// Resource — Wave-4 clutter-impact audio configuration. `impact_audio_
/// system` plays the authored per-material entry for a `started`
/// contact-force crossing (rapier `ContactForceEvent`, drained by
/// `PhysicsWorld::take_contact_force_events`), falling back to the
/// default sound. The map ships empty until IPDS/IPCT decode lands —
/// until then `default_sound` is `None` too and the system is a no-op,
/// the same posture as [`FootstepConfig::surface_sounds`].
pub(crate) struct ImpactConfig {
    /// Impact sound for an unauthored material. `None` keeps the system
    /// silent (no content data yet).
    pub(crate) default_sound: Option<Arc<Sound>>,
    /// Impact sound per Havok surface material
    /// (`RigidBodyData::surface_material` u32, read off either collider
    /// of the pair — the impacted side's authored entry wins; Skyrim's
    /// values are CRC32s, #5542).
    pub(crate) surface_sounds: std::collections::BTreeMap<u32, Arc<Sound>>,
    /// Volume multiplier for every impact play.
    pub(crate) volume: f32,
    /// Minimum `total_force_magnitude` a `started` crossing must carry to
    /// play. Filters the constant low-force chatter of resting contact —
    /// a resting clutter's weight at BU gravity is `mass × 686.7`, so the
    /// default sits well above a small object's static load. Calibrate
    /// when content data exists.
    pub(crate) min_force_magnitude: f32,
}

impl Resource for ImpactConfig {}

impl Default for ImpactConfig {
    fn default() -> Self {
        Self {
            default_sound: None,
            surface_sounds: std::collections::BTreeMap::new(),
            volume: 0.7,
            min_force_magnitude: 5_000.0,
        }
    }
}

/// Reusable cadence state for recurring near-surface ripple sounds. Splash
/// edges are never delayed; this cooldown only prevents a stationary actor
/// from producing a one-shot every frame while `RippleEvent` is present.
#[derive(Default)]
pub(crate) struct WaterAudioState {
    pub(crate) ripple_cooldowns: FxHashMap<EntityId, f32>,
}

impl Resource for WaterAudioState {}

/// Per-frame scratch buffer for `footstep_system`'s two-phase pattern
/// (collect trigger positions while walking emitters, then drain to
/// `AudioWorld::play_oneshot`). Pre-#932 the system allocated a fresh
/// `Vec<Vec3>` every frame; with this resource the same backing buffer
/// is `clear()`-ed and refilled, so capacity persists across frames.
///
/// Sized at 32 — typical loaded cell has 5–10 walking NPCs, peak
/// burst ~50 in dense exteriors. 32 covers the common case without
/// re-growing; the Vec doubles past that anyway if a giant crowd ever
/// triggers all at once.
pub(crate) struct FootstepScratch {
    pub(crate) triggers: Vec<Vec3>,
}

impl Resource for FootstepScratch {}

impl Default for FootstepScratch {
    fn default() -> Self {
        Self {
            triggers: Vec::with_capacity(32),
        }
    }
}

/// #3257 — reusable staging for `submersion_system`'s disturbance markers,
/// the one per-frame `Vec::new()` the #3135 scratch-hoisting sweep left in
/// `systems/water.rs`. Same take/restore shape as
/// `InteractionCandidateScratch` (#3059): the system moves the buffer out for
/// the duration of its own use and hands it back, so the allocated capacity
/// survives to next frame.
///
/// Each entry is `(water entity, entering, intensity, position)` — the
/// carrier that lets the emitter walk finish and drop its `ParticleEmitter`
/// write lock before the `RippleEvent` / `SplashEvent` storages are acquired.
///
/// Sized at 8: the pushes are gated on `DISTURBANCE_RADIUS` around the
/// camera, so this counts water planes the camera is standing in, not water
/// planes in the cell.
pub(crate) struct WaterDisturbanceScratch {
    pub(crate) events: Vec<(EntityId, bool, f32, [f32; 3])>,
}

impl Resource for WaterDisturbanceScratch {}

impl Default for WaterDisturbanceScratch {
    fn default() -> Self {
        Self {
            events: Vec::with_capacity(8),
        }
    }
}
