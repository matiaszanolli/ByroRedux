//! `Weapon.Fire(akSource, akAmmo)` requests (M47.2).
//!
//! CK *Fire - Weapon*: "Fires the weapon from the specified object reference,
//! using the specified ammo." `akAmmo` "If None is passed, it will use the
//! current ammo on the reference (this will throw a warning if the reference is
//! not an actor)." A script run records the request; the host's
//! `projectile_launch_system` resolves the ammo's projectile and starts it.
//! Transient: a request is launched the frame it is made or dropped.

use byroredux_core::ecs::resource::Resource;

/// One `Weapon.Fire` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectileLaunch {
    /// The reference the weapon fires from (`0x14` is the player).
    pub source: u32,
    /// The weapon form.
    pub weapon: u32,
    /// The ammo form, or 0 for "the current ammo".
    pub ammo: u32,
    /// The reference to aim at (`UseWeapon`'s target), or 0 to fire along the source's forward.
    pub aim_at: u32,
    /// The shot hits but deals no damage (`UseWeapon`'s Do No Damage).
    pub harmless: bool,
}

/// The `Weapon.Fire` calls awaiting the host.
#[derive(Debug, Default)]
pub struct PendingProjectileLaunches(pub Vec<ProjectileLaunch>);

impl Resource for PendingProjectileLaunches {}
