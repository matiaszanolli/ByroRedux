//! Escort AI-behavior markers (M42.6).
//!
//! An actor whose active PACK is an **Escort** package (FO3/FNV procedure
//! type 2 — collect a target actor, then lead it to a location and stop)
//! gets an [`EscortBehavior`] marker at spawn. The `escort_system` (binary
//! crate) walks these through two phases sharing `wander_system`'s
//! straight-line locomotion (`systems::locomotion::step_toward`):
//!
//! 1. **Collect** — close to the PTDT target's live position (mirrors
//!    [`super::follow::FollowBehavior`], re-reading `GlobalTransform` every
//!    tick) until within collect range.
//! 2. **Lead** — resolve the PLDT destination once (mirrors
//!    [`super::travel::TravelBehavior`]'s resolve-or-pick-once discipline)
//!    and walk straight there, tagging [`Escorted`] on arrival (mirrors
//!    [`super::travel::Traveled`]'s one-shot terminal role).
//!
//! Escort is the first M42 procedure to combine two already-decoded
//! sub-records (`PTDT` from Follow, `PLDT` from Travel) instead of needing
//! new parser work.
//!
//! Both `EscortBehavior`/`EscortState` are `SparseSetStorage`: only
//! escorting actors carry them, a small fraction of entities.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};
use crate::math::Vec3;

/// Marks an actor that runs the Escort idle procedure. Attached at NPC
/// spawn when the actor's active package is an Escort-type PACK.
///
/// `target_form_id` carries the package's PTDT target FormID under the
/// same restriction as [`super::follow::FollowBehavior::target_form_id`]
/// (only `SpecificReference`/`ObjectId` PTDT target types resolve). `None`
/// when there's nothing to collect — `escort_system` then skips straight to
/// the lead phase, since Escort's whole point is reaching the destination
/// and (per this crate's own PLDT doc) most FO3/FNV packages carry one.
///
/// `destination_form_id`/`destination_radius` mirror
/// [`super::travel::TravelBehavior`]'s `target_form_id`/`radius` exactly —
/// same `NearReference`-only FormID restriction, same fallback-pick radius
/// convention.
///
/// `form_id` is the raw ESM FormID, captured at spawn — seeds the
/// deterministic fallback-pick hash, same convention as
/// `TravelBehavior::form_id`/`WanderBehavior::form_id`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct EscortBehavior {
    pub target_form_id: Option<u32>,
    pub destination_form_id: Option<u32>,
    pub destination_radius: Option<f32>,
    pub form_id: u32,
}

impl Component for EscortBehavior {
    type Storage = SparseSetStorage<Self>;
}

/// Runtime state for an [`EscortBehavior`] actor.
///
/// `target_entity` is resolved exactly once (lazily, on `escort_system`'s
/// first tick for this actor) and then frozen — mirrors
/// [`super::follow::FollowState`]'s "resolve once" discipline. `None` means
/// there was no `target_form_id`, or resolution failed; either way the
/// actor skips straight into the lead phase.
///
/// `destination` is `None` while still collecting (or immediately, when
/// there's no target to collect) and becomes `Some` once resolved/picked —
/// mirrors [`super::travel::TravelState::destination`], frozen from that
/// point on (Escort does not re-track the destination once leading, same
/// as Travel).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct EscortState {
    pub target_entity: Option<EntityId>,
    pub destination: Option<Vec3>,
}

impl Component for EscortState {
    type Storage = SparseSetStorage<Self>;
}

/// Terminal one-shot marker: this actor has led its target to the Escort
/// destination and `escort_system` should stop processing it. Mirrors
/// [`super::travel::Traveled`].
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Escorted;

impl Component for Escorted {
    type Storage = SparseSetStorage<Self>;
}
