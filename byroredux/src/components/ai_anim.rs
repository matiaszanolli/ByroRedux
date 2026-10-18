//! Ambient-AI runtime, walk / combat animation components and clip catalogs. (#5467 split from `components.rs`)

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, Resource, SparseSetStorage};
use byroredux_core::math::Vec3;
// #5173 — the per-lane ground-cover arrays below size from the same splat
// constant the terrain tile + shader loops size from, so a lane bump cannot
// leave this component silently 8-wide while the rest of the chain moves.
use std::collections::HashMap;

pub(crate) use byroredux_core::ecs::components::SeatReservations;

/// M42.9 / #2652 — actor-owned ambient PACK selection state.
///
/// Candidate and winner IDs live in global load-order FormID space, so this
/// component never retains a borrow into the plugin index. The minute marker
/// bounds schedule checks to once per game minute; `None` forces a first-tick
/// confirmation after cell loading has installed scripting resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AmbientPackageRuntime {
    pub(crate) package_candidates: Vec<u32>,
    pub(crate) active_package_form_id: Option<u32>,
    pub(crate) actor_form_id: u32,
    pub(crate) last_evaluated_game_minute: Option<u16>,
}

impl Component for AmbientPackageRuntime {
    type Storage = SparseSetStorage<Self>;
}

/// What an actor was playing before a scripted one-shot IDLE (`PlayIdle`) took its animation player,
/// so the idle it was in resumes when the one-shot ends. `idle_clip` is the one-shot, to tell that the
/// player has since been given something else.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IdleResume {
    pub(crate) idle_clip: u32,
    /// The clip it was playing; `None` when it was playing nothing (the player is then removed).
    pub(crate) clip: Option<u32>,
    pub(crate) speed: f32,
    pub(crate) local_time: f32,
}

impl Component for IdleResume {
    type Storage = SparseSetStorage<Self>;
}

/// M42.1 — the per-cell sit-**enter** clip `(handle, hold_time)`
/// (`meshes\characters\_male\idleanims\chairskirt_leftenter.kf`), resolved once
/// at cell load where the archive provider is available and read by
/// `sandbox_seat_system` (which has no provider). `hold_time` is the clip
/// duration; the seat system parks the player at `local_time = hold_time` with
/// `playing = false` to hold the final seated frame. `None` for Skyrim+/Havok-
/// animation games or when the clip isn't archived — those actors keep their
/// idle and are not seated.
#[derive(Default)]
pub(crate) struct SandboxSitClip(pub(crate) Option<(u32, f32)>);
impl Resource for SandboxSitClip {}

/// Skeleton subtree owned by an actor that can consume Skyrim behavior-IDLE
/// requests. The request serial makes repeated `PlayIdle` calls observable
/// even when they target the same IDLE FormID.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AnimationTarget {
    pub(crate) skeleton_root: EntityId,
    pub(crate) consumed_idle_serial: u64,
}

impl Component for AnimationTarget {
    type Storage = SparseSetStorage<Self>;
}

/// M42.10 — one animation snapshot's worth of `AnimationPlayer` state,
/// captured when the walk-clip system takes playback over from whatever
/// the actor was playing (the spawn idle, a sandbox park, a previous
/// restore) and written back verbatim when the actor stops moving. A
/// `None` capture means "there was no player before the walk" — the
/// Skyrim+ shape, where ambient actors only gain an `AnimationPlayer`
/// when an idle request plays — and stopping removes the player again
/// rather than restoring a state that never existed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WalkAnimSnapshot {
    pub(crate) clip_handle: u32,
    pub(crate) local_time: f32,
    pub(crate) prev_time: f32,
    pub(crate) speed: f32,
    pub(crate) playing: bool,
}

/// M42.10 — per-actor walk-clip playback state, inserted at NPC spawn when
/// an authored walk cycle resolved for the game (KF locomotion clips on
/// Oblivion/FO3/FNV, a decoded HKX walk on Skyrim). Consumed by
/// `npc_walk_animation_system`, which watches the actor's per-tick XZ
/// displacement and swaps `AnimationPlayer` between the captured pose and
/// the walk clip while a locomotion procedure (or combat chase) is moving
/// the actor.
///
/// Deliberately **not** save state: the component re-derives from
/// `(walk_handle, spawn position)` on respawn, and `walking`/`last_pos`/
/// `captured` are per-frame transient — same posture as `NavPath`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WalkAnimation {
    pub(crate) walk_handle: u32,
    /// True while the walk clip is installed in the actor's
    /// `AnimationPlayer` (or while the player we inserted for it exists).
    pub(crate) walking: bool,
    /// XZ position observed last tick — the motion detector's baseline.
    pub(crate) last_pos: Vec3,
    /// The playback state taken over when `walking` became true.
    pub(crate) captured: Option<WalkAnimSnapshot>,
    /// M42.10 hysteresis — how long the actor has been continuously above
    /// the take threshold (`walking == false`) or below the release
    /// threshold (`walking == true`). Collision slides against furniture
    /// produce single near-zero ticks mid-walk; without the dwell windows
    /// the clip take/restores every crossing and visibly stutters (seen
    /// live on Camp McCarran Travel walkers, 2026-09-18).
    pub(crate) transition_secs: f32,
}

impl Component for WalkAnimation {
    type Storage = SparseSetStorage<Self>;
}

/// M42.11 — an actor's authored locomotion speed (world units/second),
/// derived once at spawn from its walk clip's accumulation-root travel
/// (`AnimationClip::authored_horizontal_speed`, sanity-clamped) and read
/// by the six locomotion procedures + combat chase so the step length
/// matches the animated stride instead of a fixed 100 u/s. Actors without
/// a walk clip carry none and keep the engine default.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WalkSpeed(pub(crate) f32);

impl Component for WalkSpeed {
    type Storage = SparseSetStorage<Self>;
}

impl WalkSpeed {
    /// #5587 — the walk-speed-or-engine-default read every locomotion
    /// procedure needs. An actor without a walk clip carries no
    /// `WalkSpeed` component and keeps the engine default; this is the
    /// one spelling of that fallback (it was hand-rolled at nine call
    /// sites, each a chance to pick a different default).
    pub(crate) fn or_default(walk_speed: Option<&Self>) -> f32 {
        walk_speed.map_or(crate::systems::locomotion::LOCOMOTION_WALK_SPEED, |s| s.0)
    }
}

/// #4413 — one shape of an authored ground-cover model's template: the
/// `GRAS` record's model, spawned once per worldspace through the normal
/// mesh path so its material resolves exactly as a placed object's would.
/// Never drawn itself; the static-mesh collector hands its built draw to the
/// ground-cover model tier, which instances it across the terrain.
/// `record` indexes the worldspace's `AuthoredCover::records`.
///
/// `excluded` (#4924) is the spawn-time verdict that the shape cannot be
/// drawn by the tier: it draws with the opaque pipeline, so a shape whose
/// material blends without an alpha test would render as a solid card. An
/// excluded template stays under the template root (so it is reclaimed with
/// it) but the collector hands it to nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthoredCoverTemplate {
    pub(crate) record: u32,
    pub(crate) excluded: bool,
}

impl Component for AuthoredCoverTemplate {
    type Storage = SparseSetStorage<Self>;
}

/// M42.10 — how long an oscillating walker (Wander/Patrol) has been
/// continuously blocked by a collision this leg. Runtime-only scratch:
/// `WanderState`/`PatrolState` are save-shaped and must not grow a field
/// for a value that is meaningless across a save boundary (a reloaded
/// actor re-walks into the same obstacle and re-accumulates from zero —
/// deterministic, since the blocker is architecture, not RNG).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct WalkStuckTimer {
    pub(crate) secs: f32,
}

impl Component for WalkStuckTimer {
    type Storage = SparseSetStorage<Self>;
}

/// M42.10 — the decoded Skyrim humanoid walk cycle
/// (`meshes\actors\character\animations\1hm_walkforward.hkx`), resolved
/// once per cell load beside the cart IDLE installation and read at NPC
/// spawn finalize (which has no archive provider). `None` for non-Skyrim
/// games, when the animations archive is absent, or when decode fails —
/// those actors simply spawn without a walk clip.
#[derive(Default)]
pub(crate) struct SkyrimWalkClip(pub(crate) Option<u32>);
impl Resource for SkyrimWalkClip {}

/// P2 combat tail — the decoded Draugr combat clip family
/// (`docs/engine/p2-combat-anim-sound-fixture.md`): the 2HM attack, the
/// medium stagger (hit reaction) and the backward death take, all
/// one-shot (`CycleType::Clamp`). Resolved once beside the walk-clip
/// installation; `*_secs` are the clips' decoded durations and drive the
/// take countdowns in `systems::combat_anim`. Absent for non-Skyrim
/// games, missing archives, or decode failure — draugr actors then spawn
/// without combat takes (a silent capability downgrade, same contract as
/// [`SkyrimWalkClip`]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct DraugrCombatClips {
    pub(crate) attack: u32,
    pub(crate) hit: u32,
    pub(crate) death: u32,
    pub(crate) attack_secs: f32,
    pub(crate) hit_secs: f32,
}
impl Resource for DraugrCombatClips {}

/// Which combat take `DraugrCombatAnim` is currently playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CombatTake {
    Attack,
    Hit,
}

/// P2 combat tail — one Draugr actor's combat-take playback state.
/// Inserted at NPC spawn finalize for Draugr-race actors; presence IS the
/// "this actor plays the Draugr combat family" marker. Drives the
/// attack/hit/death clip takes in `systems::combat_anim`:
///
/// * `take` + `take_remaining` + `captured` + `inserted_player` mirror
///   `WalkAnimation`'s take/restore protocol (capture the pre-take
///   `AnimationPlayer`, write it back when the one-shot take ends);
/// * death is terminal — `death_played` latches so the take and its
///   voice one-shot fire exactly once, and no snapshot is kept (the
///   `Dead` marker owns the pose from there, and walk_anim's abandon
///   rule already cedes playback to it).
///
/// Never serialized: the marker is re-derived at spawn from the race, and
/// the latch from `Dead` — see [`Self::seen_alive`] (#4708).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DraugrCombatAnim {
    pub(crate) take: Option<CombatTake>,
    pub(crate) take_remaining: f32,
    pub(crate) captured: Option<WalkAnimSnapshot>,
    pub(crate) inserted_player: bool,
    pub(crate) death_played: bool,
    /// #4708 — the feedback system has observed this actor alive. A death
    /// it never saw happen (a corpse respawned on revisit or restored from
    /// a save, `Dead` from its first observed frame) latches
    /// `death_played` silently instead of replaying the take and voice.
    pub(crate) seen_alive: bool,
}

impl Component for DraugrCombatAnim {
    type Storage = SparseSetStorage<Self>;
}

/// Process-lifetime IDLE FormID → decoded animation handle mapping.
///
/// Clips live in `AnimationClipRegistry`; this small companion preserves the
/// plugin-level identity used by Papyrus `PlayIdle` calls.
#[derive(Default)]
pub(crate) struct IdleClipCatalog {
    pub(crate) handles: HashMap<u32, u32>,
    /// `.kf` IDLEs whose file could not be resolved (no such record, no path, not archived): asked
    /// for once, not every frame.
    pub(crate) unresolved: std::collections::HashSet<u32>,
}

impl Resource for IdleClipCatalog {}
