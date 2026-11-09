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
    /// The weapon base, or [`USE_ACTORS_OWN_WEAPON`] for object type 12 (Weapons: Any): the actor's equipped
    /// weapon, else its first inventory weapon (AIP-UW-008).
    pub weapon_form_id: u32,
    /// The target reference, or [`USE_FACING_DIRECTION`] for a package with no `PTD2`: the actor fires along
    /// its own facing (AIP-UW-007).
    pub target_form_id: u32,
    pub bursts: Option<u32>,
    pub volley_shots: u32,
    /// Do No Damage: the shots hit but harm nothing.
    pub harmless: bool,
    /// The `PLDT` location and its radius the actor must stand inside before the volley is raised (AIP-UW-001);
    /// `None` when the package authors none.
    pub place: Option<(super::eat_sleep::EatSleepLocation, f32)>,
    /// The `PLD2` target location and radius the target must be inside before the volley is raised
    /// (AIP-UW-002); `None` when the package authors none.
    pub target_place: Option<(super::eat_sleep::EatSleepLocation, f32)>,
    /// Always Hit: every shot resolves as a hit on the target, whatever stands in the way (AIP-UW-004).
    pub always_hit: bool,
    /// Hold Fire When Blocked: no shot while a movable object blocks the line of fire (AIP-UW-003).
    pub hold_fire_when_blocked: bool,
    /// Seconds between volleys: the midpoint of the `PKW3` pause range (AIP-UW-005, PRD §6 row 5).
    pub pause_secs: f32,
}

/// [`UseWeaponPackageBehavior::weapon_form_id`] for a package that names a weapon class rather than one weapon:
/// no weapon base is the null FormID.
pub const USE_ACTORS_OWN_WEAPON: u32 = 0;

/// [`UseWeaponPackageBehavior::weapon_form_id`] for object type 22 (Weapons: Melee): the actor's own weapon, but
/// only one whose `WEAP` animation type is Melee (1 Hand) or Melee (2 Hand) (AIP-UW-009, decision Q1). Hand to
/// Hand does not count until Phase 0 settles it. FormID 1 is never a weapon base.
pub const USE_ACTORS_OWN_MELEE_WEAPON: u32 = 1;

/// [`UseWeaponPackageBehavior::target_form_id`] for a package that names no target: no reference is the null
/// FormID.
pub const USE_FACING_DIRECTION: u32 = 0;

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
