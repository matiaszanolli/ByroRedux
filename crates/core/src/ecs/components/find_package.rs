//! Find AI-behavior markers (M42, FO3/FNV procedure 0).
//!
//! GECK *Find Package*: "used to get an actor to search for and pick up
//! objects. Can also be used to 'find' other actors". The actor travels to
//! the target and then acts on it: an inventory object is picked up, an
//! activator is activated, a creature or hostile actor is attacked, a
//! friendly actor is talked to. A generic target (an object type, or a base
//! object) is looked for inside the package's search area.
//!
//! [`FindPackageBehavior`] is the package, rebuilt from the winning PACK at
//! spawn; [`FindProgress`] is the run's own state (the target it settled on,
//! what it has taken) and is not saved - what an actor already took stays
//! taken through the picked-up tombstones, so a load only restarts the hunt.

use super::eat_sleep::EatSleepLocation;
use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};

/// The object types a Find package can search for that the engine can
/// classify from the item records alone. The `PTDT` object-type numbers are
/// fopdoc's, and these three agree with the vanilla data; the rest of the
/// enum (flora, furniture, the weapon classes whose numbering the data
/// contradicts) is not searched for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum FindItemKind {
    /// Object type 13.
    Ammo,
    /// Object type 17: an `ALCH` without the Food Item flag.
    Alchemy,
    /// Object type 18: an `ALCH` with the Food Item flag.
    Food,
}

/// What a Find package looks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum FindTarget {
    /// `PTDT` type 0: this reference (`0x14` is the player).
    Reference(u32),
    /// `PTDT` type 1: any reference of this base object, found in the area.
    Base(u32),
    /// `PTDT` type 2: any placed item of this kind, found in the area.
    Kind(FindItemKind),
}

/// Marks an actor that runs the Find procedure.
///
/// `count` is the object count the package asks for (at least 1): the
/// package is done once it has taken that many. `area` and `area_radius` are
/// the `PLDT` search area a generic target is looked for in.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct FindPackageBehavior {
    pub target: FindTarget,
    pub count: u32,
    pub area: EatSleepLocation,
    pub area_radius: f32,
}

impl Component for FindPackageBehavior {
    type Storage = SparseSetStorage<Self>;
}

/// A Find run in progress. `NOT_SAVED_BY_DESIGN`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct FindProgress {
    /// The entity the actor is going to, once it has settled on one.
    pub target: Option<EntityId>,
    /// Items taken so far.
    pub taken: u32,
    /// The package is finished: it took its count, acted on its target, or
    /// has nothing left to do.
    pub done: bool,
}

impl Component for FindProgress {
    type Storage = SparseSetStorage<Self>;
}
