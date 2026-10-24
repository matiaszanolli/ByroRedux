//! UseWeapon AI-behavior marker (M42, FO3/FNV procedure 16).
//!
//! An actor whose active PACK is a **UseWeapon** package gets a
//! [`UseWeaponPackageBehavior`] at spawn: fire a weapon at a target. The
//! binary crate's `use_weapon_package_system` raises the existing
//! `UseWeaponRequest` (the ObScript `UseWeapon` command's runtime) once for
//! it, and marks the actor [`UseWeaponPackageStarted`] so the package fires
//! one volley set per install rather than every frame.
//!
//! In the vanilla data `PTDT` is the weapon, `PTD2` the target and `PKW3`
//! the fire pattern (see `PackRecord::use_weapon`).

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Marks an actor that runs the UseWeapon procedure.
///
/// `weapon_form_id` is the specific `WEAP` the package names (`PTDT` type 1).
/// Packages that name only a weapon *class* (`PTDT` type 2, an object-type
/// enum whose fopdoc numbering the vanilla data contradicts) are not
/// installed.
///
/// `bursts` is `None` for Repeat Fire (the package never finishes).
/// `volley_shots` is the shots in one volley-fire burst; `0` means Auto
/// Fire, whose burst is a full clip.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct UseWeaponPackageBehavior {
    pub weapon_form_id: u32,
    pub target_form_id: u32,
    pub bursts: Option<u32>,
    pub volley_shots: u32,
    /// Do No Damage: the shots hit but harm nothing.
    pub harmless: bool,
}

impl Component for UseWeaponPackageBehavior {
    type Storage = SparseSetStorage<Self>;
}

/// The one-shot guard: the package's volleys have been requested for this
/// install. Removed with the behavior when the package hands over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct UseWeaponPackageStarted;

impl Component for UseWeaponPackageStarted {
    type Storage = SparseSetStorage<Self>;
}
