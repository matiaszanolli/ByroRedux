//! M42 — Eat (FO3/FNV `PKDT` procedure 3) and Sleep (procedure 4)
//! behavior markers.
//!
//! Both procedures share one v0 runtime (`byroredux/src/systems/
//! eat_sleep.rs`): walk once to the package's `PLDT` anchor (an
//! [`EatSleepLocation`] — the reference, the actor's
//! [`EditorPlacement`], or where it stands; #5391), then occupy the
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
//! destination on the next tick from the same anchor — a reference or
//! the editor placement resolve to the same point, and "near current
//! location" is by definition where the restored actor stands.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// #5391 — where an Eat/Sleep package's `PLDT` anchors the actor: the
/// walk destination and the centre of the seat search ("any chair within
/// the location radius" — GECK *Eat Package* / *Sleep Package*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum EatSleepLocation {
    /// Location type 0 — the reference's live position.
    NearReference(u32),
    /// Location type 1 — the resident CELL is the location: the GECK
    /// greys out Radius for it ("If 'In Cell' is selected, Radius is
    /// greyed out"), so the seat search spans the whole cell rather than
    /// a radius around wherever the actor stands (#5500 — all 70 FNV
    /// In-Cell Eat/Sleep packages author radius 0). The actor idles
    /// while the cell is not the resident interior.
    InCell(u32),
    /// Location type 3 — the actor's [`EditorPlacement`].
    NearEditorLocation,
    /// #5500 — location type 6 (Near Linked Reference): the target of
    /// the actor's own XLKR edge, resolved through the
    /// `PackageTargetRegistry`. Unresolved until the link (or its
    /// position) is resident — retried next tick, never cached as the
    /// actor's current position.
    NearLinkedReference,
    /// Location type 2, and every type with no resolvable anchor here
    /// (4 Object ID, 5 Object Type, 7 Package Location, no `PLDT`):
    /// where the actor stands when the package takes over.
    #[default]
    NearCurrentLocation,
}

/// #5391 — the actor's authored placement (its `ACHR`/`ACRE` position),
/// stamped once at spawn and never moved: the anchor of a "near editor
/// location" package, which must not follow the actor wherever an
/// earlier package walked it. Re-stamped from the ESM on every spawn, so
/// it needs no save.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct EditorPlacement {
    pub translation: Vec3,
}

impl Component for EditorPlacement {
    type Storage = SparseSetStorage<Self>;
}

/// Marker: this actor's active package is an Eat procedure — walk to
/// the `PLDT` location, then sit at the nearest dining furniture.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct EatBehavior {
    /// `PLDT` search radius around the anchor (game units).
    pub radius: Option<f32>,
    /// `PLDT` anchor (#5391).
    pub location: EatSleepLocation,
    /// The actor's own base FormID (diagnostics — `[m42]` logs and the
    /// debug inspector). #5391 retired the hash-picked fallback walk it
    /// used to seed.
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
    pub location: EatSleepLocation,
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
