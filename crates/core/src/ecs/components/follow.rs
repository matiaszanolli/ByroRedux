//! Follow AI-behavior markers (M42.5).
//!
//! An actor whose active PACK is a **Follow** package (FO3/FNV procedure
//! type 1 — continuously follow a target actor, closing to and holding a
//! stand-off distance) gets a [`FollowBehavior`] marker at spawn. The
//! `follow_system` (binary crate) walks these and drives straight-line
//! locomotion (no pathing/NAVM, shared with `wander_system`/`travel_system`
//! via `systems::locomotion::step_toward`) toward the target's **live**
//! position, re-read every tick.
//!
//! Unlike [`super::travel::TravelBehavior`], whose destination is resolved
//! or picked once and then frozen, Follow's whole point is tracking a
//! moving target — so [`FollowState`] caches only the *resolved entity*
//! (via `byroredux_scripting::condition::resolve_entity_by_global_form_id`,
//! attempted once, lazily, on `follow_system`'s first tick per actor), and
//! `follow_system` re-reads that entity's `GlobalTransform` fresh every
//! tick rather than caching a position.
//!
//! Both are `SparseSetStorage`: only following actors carry them, a small
//! fraction of entities.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};

/// Marks an actor that runs the Follow idle procedure. Attached at NPC
/// spawn when the actor's active package is a Follow-type PACK.
///
/// `target_form_id` carries the package's PTDT target FormID *only* when
/// its target type is `SpecificReference` or `ObjectId` — the two PTDT
/// target types that carry a resolvable FormID (mirrors
/// `TravelBehavior::target_form_id`'s same restriction for PLDT's
/// `NearReference`). `Other` (Object Type / Linked Reference) leaves this
/// `None` — no procedure implemented so far can resolve those.
///
/// `follow_distance` carries PTDT's `count_or_distance` field when
/// decoded and `> 0.0` — interpreted here as the stand-off distance
/// `follow_system` holds once close enough; `None` falls back to the
/// system's own default.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct FollowBehavior {
    pub target_form_id: Option<u32>,
    pub follow_distance: Option<f32>,
}

impl Component for FollowBehavior {
    type Storage = SparseSetStorage<Self>;
}

/// Runtime state for a [`FollowBehavior`] actor. `target_entity` is
/// resolved exactly once (lazily, the first tick `follow_system` sees the
/// actor) and then frozen — v0 does not retry a failed resolution on
/// later frames, mirroring `TravelState`'s "resolve once" discipline.
/// `None` means resolution failed (or there was no `target_form_id`); the
/// actor then never moves, rather than falling back to some other
/// behavior (an undocumented behavior swap Follow avoids on purpose).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct FollowState {
    pub target_entity: Option<EntityId>,
}

impl Component for FollowState {
    type Storage = SparseSetStorage<Self>;
}
