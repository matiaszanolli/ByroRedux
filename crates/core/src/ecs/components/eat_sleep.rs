//! M42 — Eat (FO3/FNV `PKDT` procedure 3) and Sleep (procedure 4)
//! behavior markers.
//!
//! Both procedures share one v0 runtime (`byroredux/src/systems/
//! eat_sleep.rs`): walk once to the package's `PLDT` location (dining
//! area / bedroom — resolved through the same `NearReference` helper
//! Travel uses, with the hash-picked radius fallback), then occupy the
//! nearest furniture marker — a sit marker for Eat, a sleep marker for
//! Sleep (falling back to sit when a cell's beds carry no sleep
//! markers) — through the same reservation + sit-enter park the
//! Sandbox seat system applies. The lie-down clip Sleep describes does
//! not exist in any archive this engine reads; parking the seated pose
//! at the sleep marker's authored entry position is the documented v0
//! approximation, same posture as the other six v0 procedures.
//!
//! `EatSleepState` is runtime-only and deliberately absent from the
//! save registry: a save taken mid-walk re-selects the package on load
//! (`reseat_ambient_packages_after_restore`) and re-resolves the
//! destination on the next tick, which is the behavior the one-shot
//! walk describes anyway.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// Marker: this actor's active package is an Eat procedure — walk to
/// the `PLDT` location, then sit at the nearest dining furniture.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct EatBehavior {
    /// `PLDT` search radius around the target (game units).
    pub radius: Option<f32>,
    /// `PLDT` `NearReference` FormID for the walk destination.
    pub target_form_id: Option<u32>,
    /// The actor's own base FormID — the hash seed for the fallback
    /// destination pick, mirroring `WanderBehavior`/`TravelBehavior`.
    pub form_id: u32,
}

impl Component for EatBehavior {
    type Storage = SparseSetStorage<Self>;
}

/// Marker: this actor's active package is a Sleep procedure — walk to
/// the `PLDT` location, then occupy the nearest bed's sleep marker.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct SleepBehavior {
    pub radius: Option<f32>,
    pub target_form_id: Option<u32>,
    pub form_id: u32,
}

impl Component for SleepBehavior {
    type Storage = SparseSetStorage<Self>;
}

/// The once-resolved walk destination for an Eat/Sleep actor. Inserted
/// on first sight, removed by the package handover's behavior clear.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct EatSleepState {
    pub destination: Vec3,
}

impl Component for EatSleepState {
    type Storage = SparseSetStorage<Self>;
}
