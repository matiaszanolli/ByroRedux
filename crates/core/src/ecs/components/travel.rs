//! Travel AI-behavior markers (M42.4).
//!
//! An actor whose active PACK is a **Travel** package (FO3/FNV procedure
//! type 6 — walk once to the PLDT location and stop) gets a
//! [`TravelBehavior`] marker at spawn. The `travel_system` (binary crate)
//! walks these and drives straight-line locomotion (no pathing/NAVM,
//! shared with `wander_system` via `systems::locomotion::step_toward`)
//! toward a destination, then tags [`Traveled`] once arrived and stops.
//!
//! Unlike [`super::wander::WanderBehavior`], where "actor's own spawn
//! position" is a legitimate v0 approximation for a *search radius*,
//! Travel's whole point is arriving somewhere specific — so
//! `travel_system` first attempts to resolve the package's authored PLDT
//! target (when it's a `NearReference` FormID) to a real live entity's
//! position via `byroredux_scripting::condition::resolve_entity_by_global_form_id`,
//! falling back to the same hash-picked-point-within-radius approximation
//! Wander uses only when that resolution fails (most targets — off-cell,
//! or the hardcoded XMarker family `cell_loader` never spawns, per the
//! 2026-07-14 Sandbox investigation: ~12% of `NearReference` targets
//! resolve to anything spawnable at all).
//!
//! Both are `SparseSetStorage`: only traveling actors carry them, a small
//! fraction of entities.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// Marks an actor that runs the Travel idle procedure. Attached at NPC
/// spawn when the actor's active package is a Travel-type PACK.
///
/// `radius` carries the active package's authored PLDT radius (game
/// units) when one was decoded and is `> 0.0` — used only as the
/// fallback-pick radius when `target_form_id` doesn't resolve to a live
/// entity, the same `Option<f32>` convention `WanderBehavior::wander_radius`
/// uses.
///
/// `target_form_id` carries the package's PLDT target FormID *only* when
/// its location type is `NearReference` — the one PLDT location type
/// `resolve_entity_by_global_form_id` can resolve directly (a specific
/// instance's FormID, not a base-form or cell lookup). `InCell`/`ObjectId`/
/// `Other` location types leave this `None` and fall straight to the
/// fallback pick; see `travel.rs`'s module docs for why (`ObjectId` means
/// "nearest instance of this *base* form," a different lookup entirely).
///
/// `form_id` is the raw ESM FormID (`NpcRecord.form_id`), captured at
/// spawn — used to seed the deterministic fallback-pick hash, the same
/// value `npc_spawn.rs::idle_desync` and `WanderBehavior::form_id` use.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct TravelBehavior {
    pub radius: Option<f32>,
    pub target_form_id: Option<u32>,
    pub form_id: u32,
}

impl Component for TravelBehavior {
    type Storage = SparseSetStorage<Self>;
}

/// Runtime state for a [`TravelBehavior`] actor. `destination` is resolved
/// or picked exactly once (lazily, the first tick `travel_system` sees the
/// actor) and then frozen — unlike [`super::wander::WanderState`], Travel
/// never re-picks, so there's no `pick_count`/phase-oscillation to track.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct TravelState {
    pub destination: Vec3,
}

impl Component for TravelState {
    type Storage = SparseSetStorage<Self>;
}

/// Terminal one-shot marker: this actor has arrived at its Travel
/// destination and `travel_system` should stop processing it. Mirrors
/// [`super::sandbox::Seated`]'s one-shot-guard role (Travel reaches a
/// terminal state, unlike Wander which repeats indefinitely).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Traveled;

impl Component for Traveled {
    type Storage = SparseSetStorage<Self>;
}
