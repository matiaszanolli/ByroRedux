//! ObScript commands lowered onto the Papyrus effect runtime (M47.3).
//!
//! Most of what a legacy script *does* — hand out an item, start a fight, put an
//! actor in a faction, lock a door — is something the Papyrus effect runtime
//! ([`crate::translate::effects::Effect`], applied by
//! [`crate::fragment::apply_fragment_guard_free`]) already performs, with the
//! engine's own sinks behind it (`Inventory`, `AiCombatState`, `FactionRanks`,
//! `Locked`, the scripted-kill queue, ...). So an ObScript command is lowered to
//! the `Effect` its Papyrus namesake builds and applied through that one path:
//! never a second copy of the behaviour.
//!
//! The effect model names its targets by VMAD property, so the command's resolved
//! references are bound as the properties of a throwaway
//! [`ScriptInstanceData`] — the same device the Papyrus VM uses for a native call.
//!
//! | id | command | lowers to |
//! |----|---------|-----------|
//! | `0x1002` / `0x1052` | AddItem / RemoveItem | [`Effect::AddItem`] / [`Effect::RemoveItem`] (a form list adds / removes each member) |
//! | `0x10ee` | EquipItem | [`Effect::EquipItem`] |
//! | `0x108b` | KillActor | [`Effect::Kill`] |
//! | `0x1078` / `0x1079` | SetEnemy / SetAlly | [`Effect::SetEnemy`] / [`Effect::SetAlly`] |
//! | `0x1016` / `0x1017` | StartCombat / StopCombat | [`Effect::StartCombat`] / [`Effect::StopCombat`] |
//! | `0x1101` | StopCombatAlarmOnActor | [`Effect::StopCombatAlarm`] |
//! | `0x10f1` / `0x10ec` | SetUnconscious / SetGhost | [`Effect::SetUnconscious`] / [`Effect::SetGhost`] |
//! | `0x1072` / `0x1073` | Lock / Unlock | [`Effect::SetLocked`] (+ [`Effect::SetLockLevel`]) |
//! | `0x10dd` | SetOpenState | [`Effect::SetOpen`] |
//! | `0x117f` / `0x1180` | AddToFaction / RemoveFromFaction | [`Effect::ChangeFaction`] |
//! | `0x101c` / `0x101d` | AddSpell / RemoveSpell | [`Effect::AddSpell`] / [`Effect::RemoveSpell`] |
//! | `0x114a` | AddAchievement | [`Effect::AddAchievement`] |
//! | `0x11c5` | SetPlayerTeammate | [`Effect::SetPlayerTeammate`] |
//! | `0x1003` | SetEssential `<base> <flag>` | [`Effect::SetBaseProtection`] |
//! | `0x100f` / `0x1181` / `0x1182` | SetAV / DamageAV / RestoreAV | [`Effect::SetActorValue`] / [`Effect::DamageActorValue`] |
//! | `0x1010` / `0x110e` / `0x1150` | ModAV / ForceAV / ResetHealth | the permanent-modifier and damage sinks of [`crate::fragment`] |
//!
//! The actor-value commands name their value by the GECK's integer code
//! ([`crate::obscript_actor_values`]). Every semantic here is the GECK / CS
//! wiki page's; what a page does not say is declined, never approximated:
//!
//! - a target that is not resolved (an unloaded cell, a null reference) is
//!   [`Action::Declined`];
//! - `EquipItem`'s no-unequip flag has no canonical state to carry it, so a set
//!   flag declines (the effect declines `abPreventUnequip` for the same reason);
//! - `StopCombat <target>` ends the combat only when the actor is fighting that
//!   target (an actor has one combat target), `StopCombat` alone ends it;
//! - the cosmetic arguments — `KillActor`'s limb and cause of death, the hide
//!   message flag of `RemoveItem`, `EquipItem`'s equip message — are read and
//!   ignored.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_plugin::esm::records::script_instance::{
    PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
};

use crate::obscript_actions::{caller_of, Action, Context};
use crate::obscript_cmd::{Command, Games};
use crate::obscript_vm::ObScriptValue;
use crate::quest_stages::QuestFormId;
use crate::condition::{actor_value_view, ActorValueView};
use crate::translate::compose::ObjectRef;
use crate::translate::effects::{ActorBaseRef, ActorRef, ActorValueArg, Effect, ItemCount, ValueExpr};
use crate::FactionChange;

mod actor_state;
mod combat;
mod getters;
mod inventory;
mod presentation;
#[cfg(test)]
mod tests;
mod world_state;

use actor_state::*;
use combat::*;
use inventory::*;
use presentation::*;
use world_state::*;

pub(crate) use getters::GETTER_COMMANDS;
/// The getter ids are what the tests name their rows by.
#[cfg(test)]
use getters::*;

/// Every production file of this module, for the source-shape scans in `tests` (a scan of `mod.rs`
/// alone would find none of the handlers). Test-bearing files go last (none do today).
#[cfg(test)]
const SOURCES: &str = concat!(
    include_str!("mod.rs"),
    include_str!("actor_state.rs"),
    include_str!("combat.rs"),
    include_str!("getters.rs"),
    include_str!("inventory.rs"),
    include_str!("presentation.rs"),
    include_str!("world_state.rs"),
);

const CMD_ADD_ITEM: u16 = 0x1002;
const CMD_REMOVE_ITEM: u16 = 0x1052;
const CMD_EQUIP_ITEM: u16 = 0x10ee;
const CMD_KILL_ACTOR: u16 = 0x108b;
const CMD_SET_ENEMY: u16 = 0x1078;
const CMD_SET_ALLY: u16 = 0x1079;
const CMD_START_COMBAT: u16 = 0x1016;
const CMD_STOP_COMBAT: u16 = 0x1017;
const CMD_STOP_COMBAT_ALARM: u16 = 0x1101;
const CMD_SET_UNCONSCIOUS: u16 = 0x10f1;
const CMD_SET_GHOST: u16 = 0x10ec;
const CMD_LOCK: u16 = 0x1072;
const CMD_UNLOCK: u16 = 0x1073;
const CMD_SET_OPEN_STATE: u16 = 0x10dd;
const CMD_ADD_TO_FACTION: u16 = 0x117f;
const CMD_REMOVE_FROM_FACTION: u16 = 0x1180;
const CMD_ADD_SPELL: u16 = 0x101c;
const CMD_REMOVE_SPELL: u16 = 0x101d;
const CMD_ADD_ACHIEVEMENT: u16 = 0x114a;
const CMD_SET_PLAYER_TEAMMATE: u16 = 0x11c5;
const CMD_SET_ESSENTIAL: u16 = 0x1003;
pub(crate) const CMD_SET_AV: u16 = 0x100f;
pub(crate) const CMD_MOD_AV: u16 = 0x1010;
pub(crate) const CMD_FORCE_AV: u16 = 0x110e;
pub(crate) const CMD_DAMAGE_AV: u16 = 0x1181;
pub(crate) const CMD_RESTORE_AV: u16 = 0x1182;
pub(crate) const CMD_RESET_HEALTH: u16 = 0x1150;
const CMD_ENABLE_PLAYER_CONTROLS: u16 = 0x1060;
const CMD_DISABLE_PLAYER_CONTROLS: u16 = 0x1061;
const CMD_ADD_PERK: u16 = 0x1176;
const CMD_REMOVE_PERK: u16 = 0x11a8;
const CMD_SET_FACTION_RANK: u16 = 0x1089;
const CMD_SET_DESTROYED: u16 = 0x10cc;
const CMD_MARK_FOR_DELETE: u16 = 0x11bb;
const CMD_SET_RESTRAINED: u16 = 0x10f3;
const CMD_REWARD_XP: u16 = 0x1177;
const CMD_REWARD_KARMA: u16 = 0x1204;
const CMD_REMOVE_ALL_ITEMS: u16 = 0x10ad;
const CMD_REMOVE_ALL_TYPED_ITEMS: u16 = 0x1249;
const CMD_PLACE_AT_ME: u16 = 0x1025;
const CMD_RESET_AI: u16 = 0x11fa;
const CMD_SAY: u16 = 0x1033;
const CMD_SHOW_MESSAGE: u16 = 0x1059;
const CMD_START_CONVERSATION: u16 = 0x1056;
const CMD_UNEQUIP_ITEM: u16 = 0x10ef;
const CMD_PLAY_SOUND_3D: u16 = 0x10b2;
const CMD_ADD_REPUTATION: u16 = 0x1239;
const CMD_SAY_TO: u16 = 0x1034;
const CMD_SET_OWNERSHIP: u16 = 0x1117;
const CMD_CLEAR_OWNERSHIP: u16 = 0x118c;
const CMD_IMOD: u16 = 0x1191;
const CMD_RIMOD: u16 = 0x1192;
const CMD_ROTATE: u16 = 0x1004;
const CMD_SET_ANGLE: u16 = 0x1009;
const CMD_SET_POS: u16 = 0x1007;
const CMD_SET_SCALE: u16 = 0x113c;
const CMD_PLAY_GROUP: u16 = 0x1013;
const CMD_AUTO_SAVE: u16 = 0x115e;
const CMD_FORCE_SAVE: u16 = 0x1217;
const CMD_SYSTEM_SAVE: u16 = 0x127b;
const CMD_PLAY_IDLE: u16 = 0x1190;
const CMD_PICK_IDLE: u16 = 0x1064;
const CMD_RESET_3D_STATE: u16 = 0x1146;
const CMD_ADD_NOTE: u16 = 0x117c;
const CMD_REMOVE_NOTE: u16 = 0x117d;
const CMD_SHOW_WARNING: u16 = 0x11b9;
const CMD_SET_ALERT: u16 = 0x105a;
const CMD_IGNORE_CRIME: u16 = 0x10ab;
const CMD_SEND_ASSAULT_ALARM: u16 = 0x105f;
const CMD_FORCE_WEATHER: u16 = 0x112d;
const CMD_SET_WEATHER: u16 = 0x1104;
const CMD_RELEASE_WEATHER_OVERRIDE: u16 = 0x1158;
const CMD_CAST: u16 = 0x101e;
const CMD_TRIGGER_HIT_SHADER: u16 = 0x1144;
const CMD_RESURRECT: u16 = 0x108c;
const CMD_LOOK: u16 = 0x105c;
const CMD_STOP_LOOK: u16 = 0x105d;
const CMD_PLAY_MAGIC_SHADER: u16 = 0x1123;
const CMD_PLAY_MAGIC_EFFECT: u16 = 0x1124;
const CMD_STOP_MAGIC_SHADER: u16 = 0x1125;
const CMD_STOP_MAGIC_EFFECT: u16 = 0x1126;
const CMD_CAST_IMMEDIATE_ON_SELF: u16 = 0x11d9;
const CMD_DISPEL: u16 = 0x1142;
/// `Karma` in the Fallout actor-value table.
const KARMA_CODE: f32 = 23.0;

/// An angle as the scripts read it: degrees, a heading (`Z`) in `[0, 360)` — "measured with the
/// positive y-axis as 0 and increasing clockwise" (GECK *GetAngle*) — and the other two in
/// `(-180, 180]`.
fn script_angle_degrees(axis: usize, radians: f32) -> f32 {
    let degrees = radians.to_degrees();
    if axis == 2 {
        degrees.rem_euclid(360.0)
    } else {
        degrees
    }
}

/// Binds resolved references as the properties of a throwaway VMAD, so the
/// effect model can name them.
#[derive(Default)]
struct Binder {
    properties: Vec<ScriptProperty>,
}

impl Binder {
    fn object(&mut self, form: u32) -> ObjectRef {
        let name = format!("__ob{}", self.properties.len());
        self.properties.push(ScriptProperty {
            name: name.clone(),
            status: 1,
            value: PropertyValue::Object { form_id: form, alias: -1 },
        });
        ObjectRef::Property(name)
    }

    fn actor(&mut self, form: u32) -> ActorRef {
        ActorRef::Object(self.object(form))
    }

    fn vmad(&self) -> ScriptInstanceData {
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance { name: "<ObScript>".into(), status: 0, properties: self.properties.clone() }],
        }
    }
}

/// The player's `FactionReputation` standing with `repu` on one axis (`fame`, else infamy), raised by
/// `points` or (`set`) set to them — what `AddReputation`, `AddReputationExact` and `SetReputation`
/// share (#6151); the standing is created on first use. `None` when the storage is not registered.
pub(crate) fn adjust_standing(world: &World, player: EntityId, repu: u32, fame: bool, points: u16, set: bool) -> Option<()> {
    let mut standings = world.query_mut::<byroredux_core::character::FactionReputation>()?;
    if standings.get_mut(player).is_none() {
        standings.insert(player, Default::default());
    }
    let standing = standings.get_mut(player).expect("present or inserted");
    match (fame, set) {
        (true, false) => standing.add_fame(repu, points),
        (false, false) => standing.add_infamy(repu, points),
        (true, true) => standing.set_fame(repu, points),
        (false, true) => standing.set_infamy(repu, points),
    }
    Some(())
}

/// The form a reference argument resolved to; `None` for an absent or null one.
pub(crate) fn form_arg(args: &[ObScriptValue], i: usize) -> Option<u32> {
    match args.get(i) {
        Some(ObScriptValue::Ref(form)) if *form != 0 => Some(*form),
        _ => None,
    }
}

/// Insert or remove a marker component by presence.
fn set_flag<T: byroredux_core::ecs::storage::Component>(world: &World, entity: EntityId, on: bool, marker: T) {
    if let Some(mut flags) = world.query_mut::<T>() {
        if on {
            flags.insert(entity, marker);
        } else {
            flags.remove(entity);
        }
    }
}

pub(crate) fn num(args: &[ObScriptValue], i: usize) -> f32 {
    args.get(i).map_or(0.0, ObScriptValue::as_num)
}

/// Whether the actor `form` resolves to a live entity.
pub(crate) fn live(world: &World, form: u32) -> Option<EntityId> {
    crate::condition::resolve_entity_by_global_form_id(world, form)
}

/// The calling actor's form and its live entity, or `None` when the caller is
/// not loaded. Every handler that acts on the calling actor declines on `None`;
/// this is that prologue, once (#6365).
fn live_caller(world: &World, context: &Context) -> Option<(u32, EntityId)> {
    let caller = caller_of(context);
    live(world, caller).map(|entity| (caller, entity))
}

/// [`live_caller`] plus a fresh [`Binder`] with the caller already bound as its
/// actor: the opening of an effects-emitting handler, which then binds its other
/// operands on the same `Binder` and hands both to `finish_effects`.
fn open_actor_effects(world: &World, context: &Context) -> Option<(u32, EntityId, Binder, ActorRef)> {
    let (caller, entity) = live_caller(world, context)?;
    let mut bind = Binder::default();
    let me = bind.actor(caller);
    Some((caller, entity, bind, me))
}

/// Apply the effects a handler built to the world, as the Papyrus effect runtime would, and report them.
fn finish_effects(world: &World, cmd: u16, context: &Context, caller: u32, bind: &Binder, effects: Vec<Effect>) -> Action {
    let owner = QuestFormId(context.owner);
    let vmad = bind.vmad();
    let advances = crate::fragment::apply_fragment_guard_free(world, &effects, owner, Some(&vmad));
    crate::fragment::flush_advances(world, advances);
    Action::Done { value: None, effect: format!("ObScript command {cmd:#06x} on {caller:08X}: {} effect(s)", effects.len()) }
}

/// The effect-runtime commands (#6139). The rows that act on a calling reference are declined without a live
/// one, as `run` declined them before its match.
pub(crate) const EFFECT_COMMANDS: &[Command] = &[
    Command { id: CMD_ADD_ITEM, games: Games::Any, needs_caller: true, handler: add_item },
    Command { id: CMD_REMOVE_ITEM, games: Games::Any, needs_caller: true, handler: add_item },
    Command { id: CMD_EQUIP_ITEM, games: Games::Any, needs_caller: true, handler: equip_item },
    Command { id: CMD_KILL_ACTOR, games: Games::Any, needs_caller: true, handler: kill_actor },
    Command { id: CMD_SET_ENEMY, games: Games::Fallout, needs_caller: false, handler: set_enemy },
    Command { id: CMD_SET_ALLY, games: Games::Fallout, needs_caller: false, handler: set_enemy },
    Command { id: CMD_START_COMBAT, games: Games::Any, needs_caller: true, handler: start_combat },
    Command { id: CMD_STOP_COMBAT, games: Games::Any, needs_caller: true, handler: stop_combat },
    Command { id: CMD_STOP_COMBAT_ALARM, games: Games::Any, needs_caller: true, handler: stop_combat_alarm },
    Command { id: CMD_SET_UNCONSCIOUS, games: Games::Any, needs_caller: true, handler: set_unconscious },
    Command { id: CMD_SET_GHOST, games: Games::Any, needs_caller: true, handler: set_ghost },
    Command { id: CMD_LOCK, games: Games::Any, needs_caller: true, handler: lock },
    Command { id: CMD_UNLOCK, games: Games::Any, needs_caller: true, handler: lock },
    Command { id: CMD_AUTO_SAVE, games: Games::Any, needs_caller: false, handler: auto_save },
    Command { id: CMD_FORCE_SAVE, games: Games::Any, needs_caller: false, handler: force_save },
    Command { id: CMD_SYSTEM_SAVE, games: Games::Any, needs_caller: false, handler: force_save },
    Command { id: CMD_SET_OPEN_STATE, games: Games::Any, needs_caller: true, handler: set_open_state },
    Command { id: CMD_ADD_TO_FACTION, games: Games::Any, needs_caller: true, handler: add_to_faction },
    Command { id: CMD_REMOVE_FROM_FACTION, games: Games::Any, needs_caller: true, handler: add_to_faction },
    Command { id: CMD_PLAY_IDLE, games: Games::Any, needs_caller: true, handler: play_idle },
    Command { id: CMD_ADD_NOTE, games: Games::Any, needs_caller: false, handler: add_note },
    Command { id: CMD_REMOVE_NOTE, games: Games::Any, needs_caller: false, handler: add_note },
    Command { id: CMD_SET_ALERT, games: Games::Any, needs_caller: true, handler: set_alert },
    Command { id: CMD_IGNORE_CRIME, games: Games::Fallout, needs_caller: true, handler: set_alert },
    Command { id: CMD_SEND_ASSAULT_ALARM, games: Games::Fallout, needs_caller: false, handler: send_assault_alarm },
    Command { id: CMD_FORCE_WEATHER, games: Games::Any, needs_caller: false, handler: force_weather },
    Command { id: CMD_SET_WEATHER, games: Games::Any, needs_caller: false, handler: force_weather },
    Command { id: CMD_RELEASE_WEATHER_OVERRIDE, games: Games::Any, needs_caller: false, handler: release_weather_override },
    Command { id: CMD_CAST, games: Games::Any, needs_caller: true, handler: cast },
    Command { id: CMD_CAST_IMMEDIATE_ON_SELF, games: Games::Any, needs_caller: true, handler: cast },
    Command { id: CMD_PLAY_MAGIC_SHADER, games: Games::Any, needs_caller: true, handler: play_magic_shader },
    Command { id: CMD_PLAY_MAGIC_EFFECT, games: Games::Any, needs_caller: true, handler: play_magic_shader },
    Command { id: CMD_STOP_MAGIC_SHADER, games: Games::Any, needs_caller: true, handler: stop_magic_shader },
    Command { id: CMD_STOP_MAGIC_EFFECT, games: Games::Any, needs_caller: true, handler: stop_magic_shader },
    Command { id: CMD_LOOK, games: Games::Any, needs_caller: true, handler: look },
    Command { id: CMD_STOP_LOOK, games: Games::Any, needs_caller: true, handler: stop_look },
    Command { id: CMD_RESURRECT, games: Games::Any, needs_caller: true, handler: resurrect },
    Command { id: CMD_TRIGGER_HIT_SHADER, games: Games::Any, needs_caller: false, handler: trigger_hit_shader },
    Command { id: CMD_SHOW_WARNING, games: Games::Any, needs_caller: false, handler: show_warning },
    Command { id: CMD_DISPEL, games: Games::Any, needs_caller: true, handler: dispel },
    Command { id: CMD_RESET_3D_STATE, games: Games::Any, needs_caller: true, handler: reset_3d_state },
    Command { id: CMD_PICK_IDLE, games: Games::Any, needs_caller: true, handler: pick_idle },
    Command { id: CMD_ADD_SPELL, games: Games::Any, needs_caller: true, handler: add_spell },
    Command { id: CMD_REMOVE_SPELL, games: Games::Any, needs_caller: true, handler: add_spell },
    Command { id: CMD_ADD_ACHIEVEMENT, games: Games::Any, needs_caller: false, handler: add_achievement },
    Command { id: CMD_SET_PLAYER_TEAMMATE, games: Games::Any, needs_caller: true, handler: set_player_teammate },
    Command { id: CMD_SET_ESSENTIAL, games: Games::Any, needs_caller: false, handler: set_essential },
    Command { id: CMD_SET_AV, games: Games::Any, needs_caller: true, handler: set_av },
    Command { id: CMD_DAMAGE_AV, games: Games::Any, needs_caller: true, handler: set_av },
    Command { id: CMD_RESTORE_AV, games: Games::Any, needs_caller: true, handler: set_av },
    Command { id: CMD_MOD_AV, games: Games::Any, needs_caller: true, handler: mod_av },
    Command { id: CMD_FORCE_AV, games: Games::Any, needs_caller: true, handler: mod_av },
    Command { id: CMD_ENABLE_PLAYER_CONTROLS, games: Games::Any, needs_caller: false, handler: enable_player_controls },
    Command { id: CMD_DISABLE_PLAYER_CONTROLS, games: Games::Any, needs_caller: false, handler: enable_player_controls },
    Command { id: CMD_ADD_PERK, games: Games::Any, needs_caller: true, handler: add_perk },
    Command { id: CMD_REMOVE_PERK, games: Games::Any, needs_caller: true, handler: add_perk },
    Command { id: CMD_SET_FACTION_RANK, games: Games::Any, needs_caller: true, handler: set_faction_rank },
    Command { id: CMD_SET_DESTROYED, games: Games::Any, needs_caller: true, handler: set_destroyed },
    Command { id: CMD_MARK_FOR_DELETE, games: Games::Any, needs_caller: true, handler: mark_for_delete },
    Command { id: CMD_SET_RESTRAINED, games: Games::Any, needs_caller: true, handler: set_restrained },
    Command { id: CMD_REWARD_XP, games: Games::Any, needs_caller: false, handler: reward_xp },
    Command { id: CMD_REWARD_KARMA, games: Games::Any, needs_caller: false, handler: reward_karma },
    Command { id: CMD_REMOVE_ALL_ITEMS, games: Games::Any, needs_caller: true, handler: remove_all_items },
    Command { id: CMD_REMOVE_ALL_TYPED_ITEMS, games: Games::Any, needs_caller: true, handler: remove_all_typed_items },
    Command { id: CMD_PLACE_AT_ME, games: Games::Any, needs_caller: true, handler: place_at_me },
    Command { id: CMD_RESET_AI, games: Games::Any, needs_caller: true, handler: reset_ai },
    Command { id: CMD_SET_OWNERSHIP, games: Games::Any, needs_caller: true, handler: set_ownership },
    Command { id: CMD_CLEAR_OWNERSHIP, games: Games::Any, needs_caller: true, handler: set_ownership },
    Command { id: CMD_SAY, games: Games::Any, needs_caller: true, handler: say },
    Command { id: CMD_SAY_TO, games: Games::Any, needs_caller: true, handler: say },
    Command { id: CMD_SHOW_MESSAGE, games: Games::Fallout, needs_caller: false, handler: show_message },
    Command { id: CMD_START_CONVERSATION, games: Games::Any, needs_caller: true, handler: start_conversation },
    Command { id: CMD_UNEQUIP_ITEM, games: Games::Any, needs_caller: true, handler: unequip_item },
    Command { id: CMD_PLAY_SOUND_3D, games: Games::Any, needs_caller: true, handler: play_sound_3d },
    Command { id: CMD_ADD_REPUTATION, games: Games::Any, needs_caller: false, handler: add_reputation },
    Command { id: CMD_SET_POS, games: Games::Any, needs_caller: true, handler: set_pos },
    Command { id: CMD_SET_ANGLE, games: Games::Any, needs_caller: true, handler: set_angle },
    Command { id: CMD_ROTATE, games: Games::Any, needs_caller: true, handler: set_angle },
    Command { id: CMD_PLAY_GROUP, games: Games::Any, needs_caller: true, handler: play_group },
    Command { id: CMD_SET_SCALE, games: Games::Any, needs_caller: true, handler: set_scale },
    Command { id: CMD_IMOD, games: Games::Fallout, needs_caller: false, handler: imod },
    Command { id: CMD_RIMOD, games: Games::Fallout, needs_caller: false, handler: imod },
    Command { id: CMD_RESET_HEALTH, games: Games::Any, needs_caller: true, handler: reset_health },
];
