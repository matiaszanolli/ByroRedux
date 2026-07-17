//! Patrol AI-behavior markers (M42.8).
//!
//! An actor whose active PACK is a **Patrol** package (FO3/FNV procedure
//! type 13) gets a [`PatrolBehavior`] marker at spawn. Real Bethesda
//! Patrol packages walk a route defined by linked patrol-idle markers —
//! data this codebase decodes nowhere (it lives outside `PACK`'s own
//! sub-records). Absent that, v0 Patrol reduces to exactly
//! [`super::wander::WanderBehavior`]'s random-point-in-`PLDT`-radius
//! algorithm — see `crates/plugin`'s `PROCEDURE_PATROL` doc for the
//! rationale, and `systems::patrol`'s module doc for the runtime, which
//! calls the same shared oscillating-walk core `wander_system` uses
//! (`systems::wander::step_oscillating_wander`) rather than duplicating
//! it.
//!
//! [`PatrolState`] reuses [`super::wander::WanderPhase`] directly — the
//! oscillation shape (walk to a point, pause, repeat) is identical, so a
//! second copy of the same two-variant enum would add nothing.
//!
//! Both `PatrolBehavior`/`PatrolState` are `SparseSetStorage`: only
//! patrolling actors carry them, a small fraction of entities.

use super::wander::WanderPhase;
use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// Marks an actor that runs the Patrol idle procedure. Attached at NPC
/// spawn when the actor's active package is a Patrol-type PACK.
///
/// `patrol_radius`/`form_id` mirror
/// [`super::wander::WanderBehavior`]'s `wander_radius`/`form_id` exactly —
/// kept as a separate field name/component (rather than literally reusing
/// `WanderBehavior`) so Patrol stays independently selectable/inspectable,
/// consistent with every other M42 procedure having its own component.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct PatrolBehavior {
    pub patrol_radius: Option<f32>,
    pub form_id: u32,
}

impl Component for PatrolBehavior {
    type Storage = SparseSetStorage<Self>;
}

/// Continuously-updated runtime state for a [`PatrolBehavior`] actor.
/// Field-for-field identical to [`super::wander::WanderState`] (same
/// algorithm, see this module's doc) — kept as a separate component so
/// Patrol and Wander actors remain independently queryable.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct PatrolState {
    pub home: Vec3,
    pub target: Vec3,
    pub phase: WanderPhase,
    pub pick_count: u32,
}

impl Component for PatrolState {
    type Storage = SparseSetStorage<Self>;
}
