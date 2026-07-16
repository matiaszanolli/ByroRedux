//! Wander AI-behavior markers (M42.3).
//!
//! An actor whose active PACK is a **Wander** package (FO3/FNV procedure
//! type 5 — walk to random points within a radius, pause, repeat) gets a
//! [`WanderBehavior`] marker at spawn. The `wander_system` (binary crate)
//! walks these and drives straight-line locomotion (no pathing/NAVM)
//! toward a randomly picked point, pausing between walks; [`WanderState`]
//! is the continuously-updated runtime state that drives it.
//!
//! This is the first non-Sandbox procedure runtime and the first NPC
//! locomotion primitive in the engine — Sandbox never needed one because
//! it teleports an actor onto a seat rather than walking there. v0 scope:
//! straight-line walk-to-point only, no target-reference resolution, no
//! scheduling beyond PSDT/CTDA, no animation-clip swap (see `wander_system`
//! module docs for the full v0-scope list).
//!
//! Both are `SparseSetStorage`: only wandering actors carry them, a small
//! fraction of entities.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// Marks an actor that runs the Wander idle procedure. Attached at NPC
/// spawn when the actor's active package is a Wander-type PACK.
///
/// `wander_radius` carries the active package's authored PLDT radius
/// (game units) when one was decoded and is `> 0.0`; `wander_system` falls
/// back to its own default otherwise (radius-0 / no-PLDT packages) — the
/// same `Option<f32>` convention `SandboxBehavior::search_radius` uses.
///
/// v0 always derives the wander *center* (`WanderState::home`) from the
/// actor's own position the first time `wander_system` sees it, regardless
/// of the authored location type — mirroring `SandboxBehavior`'s same v0
/// simplification (FormID-based center resolution was investigated for
/// Sandbox on 2026-07-14 and found low-value; the same reasoning applies
/// here without needing a second investigation).
///
/// `form_id` is the raw ESM FormID (`NpcRecord.form_id`), captured at
/// spawn — the same value `npc_spawn.rs::idle_desync` hashes for
/// per-actor phase/speed desync. `wander_system` folds it into its own
/// deterministic target/pause-duration hash, so it needs no separate
/// `FormIdComponent` query.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct WanderBehavior {
    pub wander_radius: Option<f32>,
    pub form_id: u32,
}

impl Component for WanderBehavior {
    type Storage = SparseSetStorage<Self>;
}

/// Continuously-updated runtime state for a [`WanderBehavior`] actor.
/// Unlike [`super::sandbox::Seated`] (a one-shot terminal guard — Sandbox's
/// seat assignment happens once and never changes), Wander repeats
/// indefinitely, so this state is read *and* written by `wander_system`
/// every tick rather than being a set-once marker.
///
/// Lazily inserted by `wander_system` the first time it sees a
/// `WanderBehavior` actor without one (not attached at spawn, since the
/// spawn site has no per-frame concept of "current position" beyond the
/// initial placement — the system captures `home` on its own first tick).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct WanderState {
    /// Position captured the first tick `wander_system` saw this actor —
    /// the center new targets are picked around, within `wander_radius`.
    pub home: Vec3,
    /// Current walk destination (world space, XZ matters; Y is
    /// ground-snapped independently by `wander_system` each tick).
    pub target: Vec3,
    pub phase: WanderPhase,
    /// Increments on every new target pick. Folded into the deterministic
    /// target/pause-duration hash (alongside `WanderBehavior::form_id`) so
    /// repeated picks for the same actor don't repeat the same point —
    /// mirrors `npc_spawn.rs::idle_desync`'s no-RNG-crate determinism
    /// convention (save/reload-stable, no `rand` dependency).
    pub pick_count: u32,
}

impl Component for WanderState {
    type Storage = SparseSetStorage<Self>;
}

/// A [`WanderState`] actor is either walking toward `target`, or paused
/// with `remaining` seconds left before picking a new target.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum WanderPhase {
    Walking,
    Paused { remaining: f32 },
}
