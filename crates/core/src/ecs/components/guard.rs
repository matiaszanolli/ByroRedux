//! Guard AI-behavior markers (M42.7).
//!
//! An actor whose active PACK is a **Guard** package (FO3/FNV procedure
//! type 14 — hold the PLDT location, returning if displaced) gets a
//! [`GuardBehavior`] marker at spawn. The `guard_system` (binary crate)
//! walks these and drives straight-line locomotion (shared with
//! `wander_system`/`travel_system`/`follow_system`/`escort_system` via
//! `systems::locomotion::step_toward`) toward an anchor point, resolved or
//! picked once (mirrors [`super::travel::TravelBehavior`]'s
//! resolve-or-pick-once discipline exactly — same `NearReference`-or-
//! hash-pick fallback).
//!
//! Unlike [`super::travel::TravelBehavior`], Guard never reaches a
//! terminal state: once the anchor is resolved, `guard_system` checks
//! every tick whether the actor has drifted more than `radius` from it,
//! walking back if so — the same indefinite, non-terminal shape
//! [`super::wander::WanderBehavior`] has, just triggered by displacement
//! rather than a pause timer.
//!
//! Both `GuardBehavior`/`GuardState` are `SparseSetStorage`: only
//! guarding actors carry them, a small fraction of entities.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// Marks an actor that runs the Guard idle procedure. Attached at NPC
/// spawn when the actor's active package is a Guard-type PACK.
///
/// `anchor_form_id`/`radius` mirror
/// [`super::travel::TravelBehavior`]'s `target_form_id`/`radius` exactly —
/// same `NearReference`-only FormID restriction, same fallback-pick-radius
/// convention. For Guard, `radius` doubles as the leash tolerance:
/// `guard_system` walks back toward the anchor once the actor is more than
/// `radius` away from it.
///
/// `form_id` is the raw ESM FormID, captured at spawn — seeds the
/// deterministic fallback-pick hash, same convention as
/// `TravelBehavior::form_id`/`WanderBehavior::form_id`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct GuardBehavior {
    pub anchor_form_id: Option<u32>,
    pub radius: Option<f32>,
    pub form_id: u32,
}

impl Component for GuardBehavior {
    type Storage = SparseSetStorage<Self>;
}

/// Runtime state for a [`GuardBehavior`] actor. `anchor` is resolved or
/// picked exactly once (lazily, the first tick `guard_system` sees the
/// actor) and then frozen — mirrors
/// [`super::travel::TravelState::destination`]. Unlike Travel, there is no
/// terminal marker: guarding continues indefinitely, so this state is read
/// *and* written every tick, the same shape
/// [`super::wander::WanderState`] has.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct GuardState {
    pub anchor: Vec3,
}

impl Component for GuardState {
    type Storage = SparseSetStorage<Self>;
}
