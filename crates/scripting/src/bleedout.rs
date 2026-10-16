//! Essential and Protected actors, and bleedout (M47.2).
//!
//! The Fallout 4 CK *Actor* page: "Essential actors cannot be killed. When
//! they reach 0 health, they go into a special 'bleedout' state and recover
//! over time. Protected actors are treated as essential to all damage except
//! that delivered by the player. The player is the only one allowed to kill a
//! Protected actor." The CK *Quest Alias Tab* adds that an alias's Essential /
//! Protected flag applies "while in the alias", and the *Kill* console page
//! calls the state "unconscious (in their normal temporary 'bleedout'
//! state)", so a bleeding actor counts as unconscious for every #5017
//! consumer ([`crate::is_unconscious`]).
//!
//! Recovery follows the GECK *fEssentialDeathTime* page: "the time (in
//! seconds) that an NPC marked essential will remain unconscious before
//! getting back up", with `fEssentialHealthPercentReGain` of its health (the
//! CK *Settings* table). [`install_bleedout_rules`] reads both settings from
//! the load order, else the engine defaults.

use byroredux_core::ecs::components::{ActorValues, ActorVitals, Dead};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, Resource, SparseSetStorage, World};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{EsmIndex, ALIAS_FLAG_ESSENTIAL, ALIAS_FLAG_PROTECTED};

/// `ACBS` flag bit 1, Essential, on every game's `NPC_` (and FO3/FNV
/// `CREA`) record (xEdit).
pub const ACBS_FLAG_ESSENTIAL: u32 = 0x0000_0002;
/// `ACBS` flag bit 11, Protected, on Skyrim and later. FO3/FNV use the bit
/// for "No Blood Spray" (xEdit), so it is not read there.
pub const ACBS_FLAG_PROTECTED: u32 = 0x0000_0800;

/// The base record's Essential / Protected flags, resolved through the
/// template's "Use Base Data" and stamped at spawn. Rebuilt from the load
/// order at every spawn, so not saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActorProtection {
    pub essential: bool,
    pub protected: bool,
}

impl Component for ActorProtection {
    type Storage = SparseSetStorage<Self>;
}

impl ActorProtection {
    /// Decode the `ACBS` flags of `game`'s records. `None` when neither is
    /// set, so unprotected actors carry no component.
    pub fn from_acbs_flags(flags: u32, game: GameKind) -> Option<Self> {
        let protected_bit = !matches!(game, GameKind::Oblivion | GameKind::Fallout3NV);
        let protection = Self {
            essential: flags & ACBS_FLAG_ESSENTIAL != 0,
            protected: protected_bit && flags & ACBS_FLAG_PROTECTED != 0,
        };
        (protection.essential || protection.protected).then_some(protection)
    }
}

/// Script `ActorBase.SetEssential` / `SetProtected` overrides, by actor
/// base FormID. Each flag is `None` until a script sets it; setting one flag
/// clears the other (CK *SetEssential - ActorBase*: "If the actor is protected
/// and you set the essential flag, the protected flag will be removed").
/// Saved.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ActorBaseProtectionOverrides(
    pub std::collections::HashMap<u32, (Option<bool>, Option<bool>)>,
);

impl Resource for ActorBaseProtectionOverrides {}

impl ActorBaseProtectionOverrides {
    /// `SetEssential(value)` (`protected == false`) or `SetProtected(value)`.
    pub fn set(&mut self, base: u32, protected: bool, value: bool) {
        let (essential, protect) = self.0.entry(base).or_default();
        let (flag, other) = if protected {
            (protect, essential)
        } else {
            (essential, protect)
        };
        *flag = Some(value);
        if value {
            *other = Some(false);
        }
    }
}

/// `entity`'s base flags: its spawn stamp under any script override of its
/// base (`SceneAliasCandidate::base_form_id`).
fn base_protection(world: &World, entity: EntityId) -> ActorProtection {
    let stamp = world
        .get::<ActorProtection>(entity)
        .map(|protection| *protection)
        .unwrap_or_default();
    let Some(base) = world
        .get::<crate::scene::SceneAliasCandidate>(entity)
        .map(|candidate| candidate.base_form_id)
    else {
        return stamp;
    };
    let Some((essential, protected)) = world
        .try_resource::<ActorBaseProtectionOverrides>()
        .and_then(|overrides| overrides.0.get(&base).copied())
    else {
        return stamp;
    };
    ActorProtection {
        essential: essential.unwrap_or(stamp.essential),
        protected: protected.unwrap_or(stamp.protected),
    }
}

/// An actor in bleedout: down, and back up when `remaining_secs` runs out.
/// Saved, so a save taken mid-bleedout reloads the actor still down.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct Bleedout {
    /// `None` when the game has no [`BleedoutRules`] (Starfield, FO76): the
    /// actor stays down. #5858 — this was `f32::INFINITY`, which a save
    /// writes as JSON `null` and the load then rejects.
    pub remaining_secs: Option<f32>,
}

impl Component for Bleedout {
    type Storage = SparseSetStorage<Self>;
}

/// `fEssentialDeathTime` and `fEssentialHealthPercentReGain`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BleedoutRules {
    pub death_time_secs: f32,
    pub health_regain: f32,
}

impl Resource for BleedoutRules {}

/// The engine defaults, read from each executable's setting table
/// (Fallout3.exe, SkyrimSE.exe, Fallout4.exe hold the value before a pointer
/// to the name; Oblivion.exe loads it as a constant beside the name). They
/// agree with the GECK (10 s) and the Skyrim CK *Settings* table (0.3).
/// FalloutNV.exe is encrypted on disk; New Vegas runs the Fallout 3 engine
/// and the GECK page documents both. Starfield's executable carries no
/// `fEssentialDeathTime`, so it has no rules and never recovers.
fn engine_defaults(game: GameKind) -> Option<BleedoutRules> {
    let death_time_secs = match game {
        GameKind::Oblivion | GameKind::Fallout3NV => 10.0,
        GameKind::Skyrim | GameKind::Fallout4 => 30.0,
        GameKind::Fallout76 | GameKind::Starfield => return None,
    };
    Some(BleedoutRules {
        death_time_secs,
        health_regain: 0.3,
    })
}

/// Install (or remove) [`BleedoutRules`] for a load order: its authored
/// settings (Skyrim.esm and Fallout4.esm author a 20 s death time), else the
/// engine defaults.
pub fn install_bleedout_rules(world: &mut World, index: &EsmIndex) {
    match engine_defaults(index.game) {
        Some(defaults) => {
            world.insert_resource(BleedoutRules {
                death_time_secs: index
                    .game_setting_float("fEssentialDeathTime")
                    .unwrap_or(defaults.death_time_secs),
                health_regain: index
                    .game_setting_float("fEssentialHealthPercentReGain")
                    .unwrap_or(defaults.health_regain),
            });
        }
        None => {
            world.remove_resource::<BleedoutRules>();
        }
    }
}

/// Whether `entity` is essential: its base record (under any script
/// override), or a running quest's Essential alias holding it.
pub fn is_essential(world: &World, entity: EntityId) -> bool {
    base_protection(world, entity).essential
        || crate::scene::running_alias_flags_of_entity(world, entity, ALIAS_FLAG_ESSENTIAL) != 0
}

/// Whether `entity` is protected: its base record, or a running quest's
/// Protected alias holding it.
pub fn is_protected(world: &World, entity: EntityId) -> bool {
    base_protection(world, entity).protected
        || crate::scene::running_alias_flags_of_entity(world, entity, ALIAS_FLAG_PROTECTED) != 0
}

pub fn is_bleeding_out(world: &World, entity: EntityId) -> bool {
    world.get::<Bleedout>(entity).is_some()
}

/// Called by every death producer when `victim`'s health reaches zero,
/// before it marks the actor [`Dead`]: an essential victim, or a protected
/// one not killed by the player, goes into (or stays in) bleedout instead,
/// and the producer skips the death. The player is never spared.
pub fn spare_from_death(world: &World, victim: EntityId, killer: Option<EntityId>) -> bool {
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world);
    if Some(victim) == player {
        return false;
    }
    let spared = is_essential(world, victim)
        || (killer.is_none_or(|killer| Some(killer) != player) && is_protected(world, victim));
    if !spared {
        if let Some(mut bleedouts) = world.query_mut::<Bleedout>() {
            bleedouts.remove(victim);
        }
        return false;
    }
    if is_bleeding_out(world, victim) {
        return true;
    }
    let remaining_secs = world
        .try_resource::<BleedoutRules>()
        .map(|rules| rules.death_time_secs);
    let Some(mut bleedouts) = world.query_mut::<Bleedout>() else {
        return true;
    };
    bleedouts.insert(victim, Bleedout { remaining_secs });
    match remaining_secs {
        Some(secs) => log::info!("Actor {victim} enters bleedout for {secs} s"),
        None => log::info!("Actor {victim} enters bleedout with no recovery time"),
    }
    true
}

/// Count every bleedout down and get the actor back up when it runs out,
/// with `fEssentialHealthPercentReGain` of its undamaged health.
pub fn bleedout_system(world: &World, dt: f32) {
    let Some(rules) = world.try_resource::<BleedoutRules>().map(|rules| *rules) else {
        return;
    };
    let mut recovered = Vec::new();
    if let Some(mut bleedouts) = world.query_mut::<Bleedout>() {
        for (entity, state) in bleedouts.iter_mut() {
            let Some(remaining_secs) = state.remaining_secs.as_mut() else {
                continue;
            };
            *remaining_secs -= dt;
            if *remaining_secs <= 0.0 {
                recovered.push(entity);
            }
        }
        for &entity in &recovered {
            bleedouts.remove(entity);
        }
    }
    for entity in recovered {
        if world.get::<Dead>(entity).is_some() {
            continue;
        }
        let Some(health) = world.get::<ActorVitals>(entity).map(|vitals| vitals.health) else {
            continue;
        };
        let Some(mut values) = world.query_mut::<ActorValues>() else {
            continue;
        };
        let Some(values) = values.get_mut(entity) else {
            continue;
        };
        let Some(undamaged) = values.get(health).map(|value| value.undamaged()) else {
            continue;
        };
        let target = rules.health_regain * undamaged;
        let current = values.current(health);
        if current < target {
            values.restore(health, target - current);
        }
        log::info!("Actor {entity} recovers from bleedout at {target:.1} health");
    }
}

pub fn register(world: &mut World) {
    world.register::<ActorProtection>();
    world.register::<Bleedout>();
    world.init_resource::<ActorBaseProtectionOverrides>();
}

#[cfg(test)]
mod tests;
