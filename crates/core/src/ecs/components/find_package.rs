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

/// The object classes a Find package can search for that the engine can classify from the base records: the
/// item records (`ItemKind`) and the placed `FLOR` / `FURN` / `CONT` record types. The `PTDT` object-type number
/// is translated to one of these per game at the plugin decode (`PackObjectClass`, AIP-FIND-006); the weapon
/// sub-classes (FO3 / FNV 21-23) are not here: their labels are an open Phase 0 verify item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum FindItemKind {
    /// Object type 2: an `ARMO`.
    Armor,
    /// Object type 3: a `BOOK`.
    Book,
    /// Object type 7: an `INGR`.
    Ingredient,
    /// Object type 9: a `MISC`.
    Misc,
    /// Object type 10: a `FLOR` placed reference. Not an item: arrival activates it (the harvest).
    Flora,
    /// Object type 11: a `FURN` placed reference. Not an item: arrival sits the actor on one of its markers.
    Furniture,
    /// Object type 12: a `WEAP` (Weapons: Any).
    Weapon,
    /// Object type 13.
    Ammo,
    /// Object type 16: a `KEYM`.
    Key,
    /// Object type 17: an `ALCH` without the Food Item flag.
    Alchemy,
    /// Object type 18: an `ALCH` with the Food Item flag.
    Food,
    /// An `APPA` (Oblivion object type 2).
    Apparatus,
    /// A `CONT` placed reference (Oblivion object type 6). Not an item: arrival activates it, "Open it" (CS).
    Container,
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
    /// `PTDT` type 2, object type 14 (NPCs), 15 (Creatures) or Actors: Any: the nearest loaded live actor of
    /// that class in the area (GECK *Find Package*: "can also 'find' other actors").
    Actors(FindActorClass),
}

/// The actor class of an actor-valued Find target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum FindActorClass {
    /// Object type 14.
    Npcs,
    /// Object type 15.
    Creatures,
    /// Actors: Any (28 on FNV, 29 in xEdit's FO3 table).
    Any,
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
