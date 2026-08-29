//! Sandbox AI-behavior markers (M42).
//!
//! An actor whose form `ai_packages` include a **Sandbox** package
//! (FO3/FNV procedure type 12 — "idle activities in an area: sit, wander,
//! use furniture") gets a [`SandboxBehavior`] marker at spawn. The
//! `sandbox_seat_system` (binary crate) walks these and seats each actor
//! in a nearby free [`Furniture`](super::furniture::Furniture), tagging it
//! [`Seated`] so the one-shot seat runs once.
//!
//! This is the M42 bootstrap: the full Sandbox algorithm (target scoring,
//! scheduling, meals/sleep, wander, ownership, reservations-with-timeout)
//! is out of scope — v0 is "sit in the nearest free chair, once".
//!
//! Both are `SparseSetStorage`: only sandboxing actors carry them, a
//! small fraction of entities.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};

/// Marks an actor that runs the Sandbox idle procedure. Attached at NPC
/// spawn when the actor's form packages include a Sandbox-type PACK.
///
/// `search_radius` carries the active package's authored PLDT radius
/// (game units) when one was decoded and is `> 0.0`; spawn falls back to
/// `sandbox_seat_system`'s own default otherwise (radius-0 / no-PLDT
/// packages).
///
/// v0 always derives the search *center* from the actor's own
/// `GlobalTransform`, regardless of the authored location type. FormID
/// center resolution (`PackLocationTarget::NearReference` → a live
/// entity's position) was investigated 2026-07-14 (see `npc_spawn.rs`)
/// and found low-value: only ~12% of vanilla FNV NearReference packages
/// resolve to anything spawnable, since most target either an
/// unparsed/unloaded cell or the hardcoded XMarker family that
/// `cell_loader` never spawns as an entity. Not planned as a near-term
/// follow-up unless that changes.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct SandboxBehavior {
    pub search_radius: Option<f32>,
}

impl Component for SandboxBehavior {
    type Storage = SparseSetStorage<Self>;
}

/// Marks a [`SandboxBehavior`] actor that has taken a furniture seat.
/// The wrapped `EntityId` is the furniture entity it occupies — kept so a
/// future stand-up / reservation-release path can free the seat. Its
/// presence is the one-shot guard that stops `sandbox_seat_system` from
/// re-seating an already-seated actor.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Seated {
    /// The furniture entity this actor is seated on.
    pub furniture: EntityId,
    /// The actor's `AnimationPlayer` playback state as it was immediately
    /// *before* `sandbox_seat_system` parked it on the sit-enter clip's final
    /// frame — see [`SeatedAnimationRestore`].
    ///
    /// #3333 — seating overwrites five `AnimationPlayer` fields to hold the
    /// seated end pose, including `playing = false`. Nothing used to undo
    /// that: `clear_ambient_behavior` removed `Seated` and the fourteen AI
    /// components but never touched `AnimationPlayer`, so an actor un-seated
    /// at a package handover walked its next package frozen in a chair pose
    /// and never animated again. Carrying the pre-seat snapshot here makes
    /// the restore a pure component read — no archive access, no idle-clip
    /// re-resolution, and correct even for an actor whose idle differs from
    /// the spawn default.
    pub animation_restore: SeatedAnimationRestore,
}

/// The `AnimationPlayer` playback fields `sandbox_seat_system` overwrites
/// when it parks an actor in a seat, captured before the overwrite so
/// un-seating can put them back verbatim (#3333).
///
/// Exactly the five fields the park writes — no more, so a restore can never
/// clobber state the seat never touched (`reverse_direction`, `root_entity`).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct SeatedAnimationRestore {
    pub clip_handle: u32,
    pub local_time: f32,
    pub prev_time: f32,
    pub playing: bool,
    pub speed: f32,
}

impl Default for SeatedAnimationRestore {
    /// A neutral *playing* idle at normal speed — deliberately not
    /// `#[derive(Default)]`, which would give `playing: false, speed: 0.0`,
    /// i.e. the frozen state this type exists to undo. Used when an actor is
    /// seated without an `AnimationPlayer` at all; restoring it is a no-op
    /// because the teardown only writes to actors that have one.
    fn default() -> Self {
        Self {
            clip_handle: 0,
            local_time: 0.0,
            prev_time: 0.0,
            playing: true,
            speed: 1.0,
        }
    }
}

impl Component for Seated {
    type Storage = SparseSetStorage<Self>;
}
