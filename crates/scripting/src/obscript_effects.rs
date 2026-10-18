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

use crate::obscript_actions::{Action, Context};
use crate::obscript_vm::ObScriptValue;
use crate::quest_stages::QuestFormId;
use crate::condition::{actor_value_view, ActorValueView};
use crate::translate::compose::ObjectRef;
use crate::translate::effects::{ActorBaseRef, ActorRef, ActorValueArg, Effect, ItemCount, ValueExpr};
use crate::FactionChange;

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

/// Every command this module runs.
pub(crate) fn handles(cmd: u16) -> bool {
    matches!(
        cmd,
        CMD_ADD_ITEM
            | CMD_REMOVE_ITEM
            | CMD_EQUIP_ITEM
            | CMD_KILL_ACTOR
            | CMD_SET_ENEMY
            | CMD_SET_ALLY
            | CMD_START_COMBAT
            | CMD_STOP_COMBAT
            | CMD_STOP_COMBAT_ALARM
            | CMD_SET_UNCONSCIOUS
            | CMD_SET_GHOST
            | CMD_LOCK
            | CMD_UNLOCK
            | CMD_SET_OPEN_STATE
            | CMD_ADD_TO_FACTION
            | CMD_REMOVE_FROM_FACTION
            | CMD_ADD_SPELL
            | CMD_REMOVE_SPELL
            | CMD_ADD_ACHIEVEMENT
            | CMD_SET_PLAYER_TEAMMATE
            | CMD_SET_ESSENTIAL
            | CMD_SET_AV
            | CMD_MOD_AV
            | CMD_FORCE_AV
            | CMD_DAMAGE_AV
            | CMD_RESTORE_AV
            | CMD_RESET_HEALTH
    )
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

/// The form a reference argument resolved to; `None` for an absent or null one.
fn form_arg(args: &[ObScriptValue], i: usize) -> Option<u32> {
    match args.get(i) {
        Some(ObScriptValue::Ref(form)) if *form != 0 => Some(*form),
        _ => None,
    }
}

fn num(args: &[ObScriptValue], i: usize) -> f32 {
    args.get(i).map_or(0.0, ObScriptValue::as_num)
}

/// Whether the actor `form` resolves to a live entity.
fn live(world: &World, form: u32) -> Option<EntityId> {
    crate::condition::resolve_entity_by_global_form_id(world, form)
}

/// Run `cmd` if this module lowers it.
pub(crate) fn run(world: &World, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    if !handles(cmd) {
        return Action::Unhandled;
    }
    // Faction, achievement and the like name no calling reference; everything
    // else acts on one.
    let needs_caller = !matches!(cmd, CMD_SET_ENEMY | CMD_SET_ALLY | CMD_ADD_ACHIEVEMENT | CMD_SET_ESSENTIAL);
    let caller = context.caller.filter(|&form| form != 0);
    if needs_caller && caller.is_none() {
        return Action::Declined;
    }
    let caller = caller.unwrap_or(0);
    // A target that is not in a loaded cell cannot be acted on.
    let caller_entity = if needs_caller { live(world, caller) } else { None };
    if needs_caller && caller_entity.is_none() {
        return Action::Declined;
    }
    let mut bind = Binder::default();
    let me = bind.actor(caller);
    let name = |what: &str| format!("{what} {caller:08X}");
    let effects: Vec<Effect> = match cmd {
        CMD_ADD_ITEM | CMD_REMOVE_ITEM => {
            let Some(item) = form_arg(args, 0) else { return Action::Declined };
            let count = num(args, 1).max(0.0) as u32;
            if count == 0 {
                return Action::Done { value: None, effect: String::new() };
            }
            // A form list stands for each of its members.
            let items = crate::form_lists::forms_of(world, item, &[]).unwrap_or_else(|| vec![item]);
            items
                .into_iter()
                .map(|item| {
                    let item = bind.object(item);
                    if cmd == CMD_ADD_ITEM {
                        Effect::AddItem {
                            container: me.clone(),
                            item,
                            count: ItemCount::Literal(count),
                            silent: num(args, 2) != 0.0,
                        }
                    } else {
                        Effect::RemoveItem { container: me.clone(), item, count: ItemCount::Literal(count), destination: None }
                    }
                })
                .collect()
        }
        CMD_EQUIP_ITEM => {
            let Some(item) = form_arg(args, 0) else { return Action::Declined };
            if num(args, 1) != 0.0 {
                return Action::Declined;
            }
            vec![Effect::EquipItem { actor: me, item: bind.object(item), silent: num(args, 2) != 0.0 }]
        }
        CMD_KILL_ACTOR => {
            let killer = form_arg(args, 0).map(|killer| bind.actor(killer));
            vec![Effect::Kill { actor: me, killer, silent: false }]
        }
        CMD_SET_ENEMY | CMD_SET_ALLY => {
            let (Some(first), Some(second)) = (form_arg(args, 0), form_arg(args, 1)) else {
                return Action::Declined;
            };
            let (faction, other_faction) = (bind.object(first), bind.object(second));
            let (self_flag, other_flag) = (num(args, 2) != 0.0, num(args, 3) != 0.0);
            vec![if cmd == CMD_SET_ENEMY {
                Effect::SetEnemy { faction, other_faction, self_neutral: self_flag, other_neutral: other_flag }
            } else {
                Effect::SetAlly { faction, other_faction, self_friend: self_flag, other_friend: other_flag }
            }]
        }
        CMD_START_COMBAT => {
            let Some(target) = form_arg(args, 0) else { return Action::Declined };
            if live(world, target).is_none() {
                return Action::Declined;
            }
            vec![Effect::StartCombat { actor: me, target: bind.actor(target) }]
        }
        CMD_STOP_COMBAT => {
            // An actor has one combat target: naming another one stops nothing.
            if let Some(target) = form_arg(args, 0) {
                let fighting = caller_entity.and_then(|actor| world.get::<crate::AiCombatState>(actor).map(|state| state.target));
                if fighting.is_none() || fighting != live(world, target) {
                    return Action::Done { value: None, effect: String::new() };
                }
            }
            vec![Effect::StopCombat { actor: me }]
        }
        CMD_STOP_COMBAT_ALARM => vec![Effect::StopCombatAlarm { actor: me }],
        CMD_SET_UNCONSCIOUS => vec![Effect::SetUnconscious { actor: me, unconscious: num(args, 0) != 0.0 }],
        CMD_SET_GHOST => vec![Effect::SetGhost { actor: me, ghost: num(args, 0) != 0.0 }],
        CMD_LOCK | CMD_UNLOCK => {
            let target = bind.object(caller);
            let mut effects = vec![Effect::SetLocked { target: target.clone(), locked: cmd == CMD_LOCK }];
            // GECK: "Lock 0 (default) will lock it but NOT change the lock level;
            // 1-255 will change the level of the lock AND lock it." The CS
            // Oblivion page has no such zero: the level written is the level.
            let level = num(args, 0);
            let sets_level = if crate::ScriptGame::is_fallout3_nv(world) { level >= 1.0 } else { !args.is_empty() };
            if cmd == CMD_LOCK && sets_level {
                effects.push(Effect::SetLockLevel { target, level: level.min(255.0) as u8 });
            }
            effects
        }
        CMD_SET_OPEN_STATE => vec![Effect::SetOpen { target: bind.object(caller), open: num(args, 0) != 0.0 }],
        CMD_ADD_TO_FACTION | CMD_REMOVE_FROM_FACTION => {
            let Some(faction) = form_arg(args, 0) else { return Action::Declined };
            let change = match cmd {
                CMD_REMOVE_FROM_FACTION => FactionChange::Remove,
                // `AddToFaction <faction> <rank>`.
                _ => FactionChange::SetRank(num(args, 1).clamp(f32::from(i8::MIN), f32::from(i8::MAX)) as i8),
            };
            vec![Effect::ChangeFaction { actor: me, faction: bind.object(faction), change }]
        }
        CMD_ADD_SPELL | CMD_REMOVE_SPELL => {
            let Some(spell) = form_arg(args, 0) else { return Action::Declined };
            let spell = bind.object(spell);
            vec![if cmd == CMD_ADD_SPELL { Effect::AddSpell { actor: me, spell } } else { Effect::RemoveSpell { actor: me, spell } }]
        }
        CMD_ADD_ACHIEVEMENT => vec![Effect::AddAchievement { id: num(args, 0) as i32 }],
        CMD_SET_PLAYER_TEAMMATE => {
            vec![Effect::SetPlayerTeammate { actor: me, teammate: num(args, 0) != 0.0, can_do_favor: false }]
        }
        // `SetEssential <ActorBase> <flag>`: the base is an argument, not the caller.
        CMD_SET_ESSENTIAL => {
            let Some(base) = form_arg(args, 0) else { return Action::Declined };
            vec![Effect::SetBaseProtection {
                base: ActorBaseRef::Form(bind.object(base)),
                protected: false,
                value: num(args, 1) != 0.0,
            }]
        }
        CMD_SET_AV | CMD_DAMAGE_AV | CMD_RESTORE_AV => {
            let Some(avif) = crate::obscript_actor_values::actor_value_form(world, num(args, 0)) else {
                return Action::Declined;
            };
            let actor_value = bind.object(avif);
            let value = ValueExpr::Literal(num(args, 1));
            vec![match cmd {
                CMD_SET_AV => Effect::SetActorValue { actor: me, actor_value, value },
                CMD_DAMAGE_AV => Effect::DamageActorValue {
                    actor: me,
                    actor_value: ActorValueArg::Form(actor_value),
                    amount: value,
                    restore: false,
                },
                _ => Effect::DamageActorValue {
                    actor: me,
                    actor_value: ActorValueArg::Form(actor_value),
                    amount: value,
                    restore: true,
                },
            }]
        }
        CMD_MOD_AV | CMD_FORCE_AV => {
            let Some(avif) = crate::obscript_actor_values::actor_value_form(world, num(args, 0)) else {
                return Action::Declined;
            };
            let Some(actor) = caller_entity else { return Action::Declined };
            // `ForceAV` is "the equivalent of ModAV by (value - GetAV)".
            let delta = if cmd == CMD_FORCE_AV {
                num(args, 1) - actor_value_view(world, actor, avif, ActorValueView::Current)
            } else {
                num(args, 1)
            };
            crate::fragment::mod_actor_value_permanent(world, actor, avif, delta);
            return Action::Done { value: None, effect: format!("{} {caller:08X} {avif:08X} {delta}", if cmd == CMD_MOD_AV { "ModAV" } else { "ForceAV" }) };
        }
        CMD_RESET_HEALTH => {
            let Some(actor) = caller_entity else { return Action::Declined };
            let vitals: Vec<u32> = crate::obscript_actor_values::RESET_HEALTH_CODES
                .iter()
                .filter_map(|&code| crate::obscript_actor_values::actor_value_form(world, code as f32))
                .collect();
            if vitals.is_empty() {
                return Action::Declined;
            }
            crate::fragment::clear_actor_value_damage(world, actor, &vitals);
            return Action::Done { value: None, effect: name("ResetHealth") };
        }
        _ => return Action::Unhandled,
    };
    let owner = QuestFormId(context.owner);
    let vmad = bind.vmad();
    let advances = crate::fragment::apply_fragment_guard_free(world, &effects, owner, Some(&vmad));
    crate::fragment::flush_advances(world, advances);
    Action::Done { value: None, effect: format!("ObScript command {cmd:#06x} on {caller:08X}: {} effect(s)", effects.len()) }
}

/// What a getter command made of a call.
#[derive(Debug, PartialEq)]
pub(crate) enum Getter {
    /// Not a getter this module answers.
    Unhandled,
    /// A number.
    Value(f32),
    /// A reference (its FormID, `0` for none).
    Ref(u32),
    /// A getter whose subject, argument or value could not be read.
    Declined,
}

const CMD_GET_ITEM_COUNT: u16 = 0x102f;
const CMD_IS_IN_COMBAT: u16 = 0x1121;
const CMD_GET_IS_REFERENCE: u16 = 0x1088;
const CMD_GET_PLAYER_TEAMMATE: u16 = 0x11c6;
const CMD_GET_LOCKED: u16 = 0x1005;
const CMD_GET_LOCK_LEVEL: u16 = 0x1041;
const CMD_GET_AV: u16 = 0x100e;
const CMD_GET_BASE_AV: u16 = 0x1115;
const CMD_GET_UNCONSCIOUS: u16 = 0x10f2;
const CMD_GET_COMBAT_TARGET: u16 = 0x10e8;

/// Every getter this module answers.
pub(crate) fn is_getter(cmd: u16) -> bool {
    matches!(
        cmd,
        CMD_GET_ITEM_COUNT
            | CMD_IS_IN_COMBAT
            | CMD_GET_IS_REFERENCE
            | CMD_GET_PLAYER_TEAMMATE
            | CMD_GET_LOCKED
            | CMD_GET_LOCK_LEVEL
            | CMD_GET_AV
            | CMD_GET_BASE_AV
            | CMD_GET_UNCONSCIOUS
            | CMD_GET_COMBAT_TARGET
    )
}

/// Answer getter `cmd` about the calling reference, from the engine's own reads:
/// the effect model's guards and values where it has one (`GetItemCount`,
/// `IsInCombat`, `GetIsReference`, `GetPlayerTeammate`, `GetAV`), the component
/// the runtime keeps otherwise (`Locked`, `ActorControlState`, `AiCombatState`).
/// A subject that is not loaded, or a read the runtime cannot make, is
/// [`Getter::Declined`] — never a made-up `0`.
pub(crate) fn getter(world: &World, cmd: u16, args: &[ObScriptValue], context: &Context) -> Getter {
    if !is_getter(cmd) {
        return Getter::Unhandled;
    }
    let Some(caller) = context.caller.filter(|&form| form != 0) else {
        return Getter::Declined;
    };
    let Some(subject) = live(world, caller) else {
        return Getter::Declined;
    };
    let owner = QuestFormId(context.owner);
    let mut bind = Binder::default();
    let me = bind.actor(caller);
    let value = |value: ValueExpr, bind: Binder| {
        crate::fragment::evaluate_value_guard_free(world, &value, owner, Some(&bind.vmad())).map_or(Getter::Declined, Getter::Value)
    };
    let guard = |guard: crate::translate::effects::FragmentGuard, bind: Binder| {
        crate::fragment::evaluate_guard_guard_free(world, &guard, owner, Some(&bind.vmad()))
            .map_or(Getter::Declined, |holds| Getter::Value(f32::from(u8::from(holds))))
    };
    match cmd {
        CMD_GET_ITEM_COUNT => {
            let Some(item) = form_arg(args, 0) else { return Getter::Declined };
            // A form list counts every one of its members.
            let items = crate::form_lists::forms_of(world, item, &[]).unwrap_or_else(|| vec![item]);
            let mut total = 0.0;
            for item in items {
                let item = bind.object(item);
                match crate::fragment::evaluate_value_guard_free(
                    world,
                    &ValueExpr::ItemCount(me.clone(), item),
                    owner,
                    Some(&bind.vmad()),
                ) {
                    Some(count) => total += count,
                    None => return Getter::Declined,
                }
            }
            Getter::Value(total)
        }
        CMD_IS_IN_COMBAT => guard(crate::translate::effects::FragmentGuard::ActorInCombat { actor: me, expected: true }, bind),
        CMD_GET_IS_REFERENCE => {
            let Some(other) = form_arg(args, 0) else { return Getter::Declined };
            let right = Some(bind.actor(other));
            guard(crate::translate::effects::FragmentGuard::SameReference { left: me, right, expected: true }, bind)
        }
        CMD_GET_PLAYER_TEAMMATE => guard(
            crate::translate::effects::FragmentGuard::ActorFollower { actor: me, flag: crate::FollowerFlag::Teammate, expected: true },
            bind,
        ),
        CMD_GET_LOCKED => Getter::Value(f32::from(u8::from(world.get::<byroredux_core::ecs::components::Locked>(subject).is_some()))),
        // `GetLockLevel`: an unlocked reference has no lock, so no level.
        CMD_GET_LOCK_LEVEL => {
            Getter::Value(world.get::<byroredux_core::ecs::components::Locked>(subject).map_or(0.0, |lock| f32::from(lock.lock_level)))
        }
        CMD_GET_AV | CMD_GET_BASE_AV => {
            let Some(avif) = crate::obscript_actor_values::actor_value_form(world, num(args, 0)) else {
                return Getter::Declined;
            };
            let actor_value = bind.object(avif);
            let expr = if cmd == CMD_GET_AV {
                ValueExpr::ActorValue(me, actor_value)
            } else {
                ValueExpr::ActorValueReading(crate::translate::effects::ActorValueReading::Base, me, ActorValueArg::Form(actor_value))
            };
            value(expr, bind)
        }
        CMD_GET_UNCONSCIOUS => {
            Getter::Value(f32::from(u8::from(world.get::<crate::ActorControlState>(subject).is_some_and(|state| state.unconscious))))
        }
        CMD_GET_COMBAT_TARGET => match world.get::<crate::AiCombatState>(subject).map(|state| state.target) {
            Some(target) => match crate::condition::entity_global_form_id(world, target) {
                Some(form) => Getter::Ref(form),
                None if crate::papyrus_demo::PapyrusPlayerEntity::get(world) == Some(target) => Getter::Ref(crate::PLAYER_REFERENCE),
                None => Getter::Declined,
            },
            None => Getter::Ref(0),
        },
        _ => Getter::Unhandled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{ActorValues, FormIdComponent, Inventory, Locked};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::{AvifRecord, EsmIndex};

    const ACTOR: u32 = 0x0001_4D8A;
    const OTHER: u32 = 0x0001_4D8B;
    const ITEM: u32 = 0x0002_0001;
    const LIST: u32 = 0x0002_0002;
    const STRENGTH: u32 = 0x3E8;
    const HEALTH: u32 = 0x450;

    /// A Fallout world with two placed actors, `ACTOR` and `OTHER`, and the
    /// Strength / Health AVIFs a code can name.
    fn world() -> (World, EntityId, EntityId) {
        let mut world = World::new();
        crate::register(&mut world);
        world.insert_resource(crate::quest_stages::QuestStageState::default());
        world.insert_resource(crate::quest_stages::QuestObjectiveState::default());
        world.register::<ActorValues>();
        world.register::<Inventory>();
        world.register::<Locked>();
        let mut pool = FormIdPool::new();
        let mut spawn = |world: &mut World, local: u32| {
            let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("FalloutNV.esm"), local: LocalFormId(local) });
            let entity = world.spawn();
            world.insert(entity, FormIdComponent(id));
            entity
        };
        let actor = spawn(&mut world, ACTOR);
        let other = spawn(&mut world, OTHER);
        world.insert_resource(pool);
        let mut index = EsmIndex::default();
        index.game = GameKind::Fallout3NV;
        for (form_id, editor_id) in [(STRENGTH, "AVStrength"), (HEALTH, "AVHealth")] {
            index.actor_values.insert(form_id, AvifRecord { form_id, editor_id: editor_id.into(), ..Default::default() });
        }
        crate::install_actor_value_names(&mut world, &index);
        (world, actor, other)
    }

    fn context(caller: Option<u32>) -> Context {
        Context { caller, known: None, action_ref: None, owner: 0x00AA_0001 }
    }

    fn done(action: &Action) -> bool {
        matches!(action, Action::Done { .. })
    }

    fn count(world: &World, entity: EntityId, item: u32) -> u32 {
        world
            .get::<Inventory>(entity)
            .map_or(0, |inventory| inventory.items.iter().filter(|stack| stack.base_form_id == item).map(|stack| stack.count).sum())
    }

    fn refs(forms: &[u32]) -> Vec<ObScriptValue> {
        forms.iter().map(|&form| ObScriptValue::Ref(form)).collect()
    }

    #[test]
    fn add_item_and_remove_item_move_the_stack_in_the_callers_inventory() {
        let (world, actor, _) = world();
        let mut args = refs(&[ITEM]);
        args.push(ObScriptValue::Num(5.0));
        assert!(done(&run(&world, CMD_ADD_ITEM, &args, &context(Some(ACTOR)))));
        assert_eq!(count(&world, actor, ITEM), 5);
        args[1] = ObScriptValue::Num(2.0);
        assert!(done(&run(&world, CMD_REMOVE_ITEM, &args, &context(Some(ACTOR)))));
        assert_eq!(count(&world, actor, ITEM), 3);
        assert_eq!(
            getter(&world, CMD_GET_ITEM_COUNT, &refs(&[ITEM]), &context(Some(ACTOR))),
            Getter::Value(3.0),
            "GetItemCount reads the same stack"
        );
    }

    #[test]
    fn a_form_list_stands_for_each_of_its_members() {
        const SECOND: u32 = 0x0002_0003;
        let (mut world, actor, _) = world();
        let mut index = EsmIndex::default();
        index.form_lists.insert(LIST, byroredux_plugin::esm::records::FlstRecord { form_id: LIST, entries: vec![ITEM, SECOND], ..Default::default() });
        crate::install_form_lists(&mut world, &index);
        let args = [ObScriptValue::Ref(LIST), ObScriptValue::Num(2.0)];
        assert!(done(&run(&world, CMD_ADD_ITEM, &args, &context(Some(ACTOR)))));
        assert_eq!((count(&world, actor, ITEM), count(&world, actor, SECOND)), (2, 2));
        assert_eq!(getter(&world, CMD_GET_ITEM_COUNT, &refs(&[LIST]), &context(Some(ACTOR))), Getter::Value(4.0));
    }

    #[test]
    fn a_command_on_a_reference_that_is_not_loaded_is_declined() {
        let (world, _, _) = world();
        let args = [ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0)];
        assert_eq!(run(&world, CMD_ADD_ITEM, &args, &context(Some(0x00FF_FFFF))), Action::Declined);
        assert_eq!(run(&world, CMD_ADD_ITEM, &args, &context(None)), Action::Declined, "a quest has no inventory");
        assert_eq!(getter(&world, CMD_GET_ITEM_COUNT, &refs(&[ITEM]), &context(Some(0x00FF_FFFF))), Getter::Declined);
        // A null item names nothing.
        assert_eq!(run(&world, CMD_ADD_ITEM, &[ObScriptValue::Ref(0), ObScriptValue::Num(1.0)], &context(Some(ACTOR))), Action::Declined);
    }

    #[test]
    fn lock_and_unlock_set_the_lock_and_lock_zero_keeps_the_level() {
        let (world, actor, _) = world();
        assert!(done(&run(&world, CMD_LOCK, &[], &context(Some(ACTOR)))));
        assert_eq!(getter(&world, CMD_GET_LOCKED, &[], &context(Some(ACTOR))), Getter::Value(1.0));
        assert!(done(&run(&world, CMD_LOCK, &[ObScriptValue::Num(75.0)], &context(Some(ACTOR)))));
        assert_eq!(world.get::<Locked>(actor).map(|lock| lock.lock_level), Some(75));
        assert_eq!(getter(&world, CMD_GET_LOCK_LEVEL, &[], &context(Some(ACTOR))), Getter::Value(75.0));
        assert!(done(&run(&world, CMD_UNLOCK, &[], &context(Some(ACTOR)))));
        assert_eq!(getter(&world, CMD_GET_LOCKED, &[], &context(Some(ACTOR))), Getter::Value(0.0));
        assert_eq!(getter(&world, CMD_GET_LOCK_LEVEL, &[], &context(Some(ACTOR))), Getter::Value(0.0));
    }

    #[test]
    fn actor_value_commands_translate_the_code_and_set_mod_damage_and_restore() {
        let (world, actor, _) = world();
        world.query_mut::<ActorValues>().unwrap().insert(actor, ActorValues::new());
        let ctx = context(Some(ACTOR));
        let av = |code: f32, value: f32| [ObScriptValue::Num(code), ObScriptValue::Num(value)];
        // SetAV Strength 6, ModAV Strength 2, DamageAV Strength 3, RestoreAV Strength 1.
        assert!(done(&run(&world, CMD_SET_AV, &av(5.0, 6.0), &ctx)));
        assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(5.0)], &ctx), Getter::Value(6.0));
        assert!(done(&run(&world, CMD_MOD_AV, &av(5.0, 2.0), &ctx)));
        assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(5.0)], &ctx), Getter::Value(8.0));
        assert!(done(&run(&world, CMD_DAMAGE_AV, &av(5.0, 3.0), &ctx)));
        assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(5.0)], &ctx), Getter::Value(5.0));
        assert!(done(&run(&world, CMD_RESTORE_AV, &av(5.0, 1.0), &ctx)));
        assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(5.0)], &ctx), Getter::Value(6.0));
        // GetBaseAV is the base alone: the +2 modifier and the damage are not in it.
        assert_eq!(getter(&world, CMD_GET_BASE_AV, &[ObScriptValue::Num(5.0)], &ctx), Getter::Value(6.0));
        // ForceAV 10 brings the current value to 10 through a permanent modifier.
        assert!(done(&run(&world, CMD_FORCE_AV, &av(5.0, 10.0), &ctx)));
        assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(5.0)], &ctx), Getter::Value(10.0));
        // ResetHealth heals the damage on Health.
        assert!(done(&run(&world, CMD_SET_AV, &av(16.0, 50.0), &ctx)));
        assert!(done(&run(&world, CMD_DAMAGE_AV, &av(16.0, 20.0), &ctx)));
        assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(16.0)], &ctx), Getter::Value(30.0));
        assert!(done(&run(&world, CMD_RESET_HEALTH, &[], &ctx)));
        assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(16.0)], &ctx), Getter::Value(50.0));
    }

    #[test]
    fn an_actor_value_code_with_no_avif_is_declined_not_invented() {
        let (world, _, _) = world();
        let ctx = context(Some(ACTOR));
        // 21 = SpeedMult has no AVIF; 99 is not a code.
        for code in [21.0, 99.0] {
            assert_eq!(run(&world, CMD_SET_AV, &[ObScriptValue::Num(code), ObScriptValue::Num(1.0)], &ctx), Action::Declined);
            assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(code)], &ctx), Getter::Declined);
        }
    }

    #[test]
    fn unconscious_and_ghost_set_the_actor_control_state() {
        let (world, actor, _) = world();
        let ctx = context(Some(ACTOR));
        assert!(done(&run(&world, CMD_SET_UNCONSCIOUS, &[ObScriptValue::Num(1.0)], &ctx)));
        assert!(world.get::<crate::ActorControlState>(actor).is_some_and(|state| state.unconscious));
        assert_eq!(getter(&world, CMD_GET_UNCONSCIOUS, &[], &ctx), Getter::Value(1.0));
        assert!(done(&run(&world, CMD_SET_UNCONSCIOUS, &[ObScriptValue::Num(0.0)], &ctx)));
        assert_eq!(getter(&world, CMD_GET_UNCONSCIOUS, &[], &ctx), Getter::Value(0.0));
        assert!(done(&run(&world, CMD_SET_GHOST, &[ObScriptValue::Num(1.0)], &ctx)));
        assert!(world.get::<crate::ActorControlState>(actor).is_some_and(|state| state.ghost));
    }

    #[test]
    fn get_is_reference_compares_form_ids() {
        let (world, _, _) = world();
        let ctx = context(Some(ACTOR));
        assert_eq!(getter(&world, CMD_GET_IS_REFERENCE, &refs(&[ACTOR]), &ctx), Getter::Value(1.0));
        assert_eq!(getter(&world, CMD_GET_IS_REFERENCE, &refs(&[OTHER]), &ctx), Getter::Value(0.0));
        assert_eq!(getter(&world, CMD_GET_IS_REFERENCE, &refs(&[0]), &ctx), Getter::Declined, "a null ref is not asked about");
    }

    #[test]
    fn stop_combat_with_a_target_stops_only_that_fight() {
        let (mut world, actor, other) = world();
        world.insert(actor, crate::AiCombatState { target: other, attack_cooldown_remaining: 0.0 });
        let ctx = context(Some(ACTOR));
        // Fighting OTHER: `StopCombat <some third ref>` leaves it alone.
        assert!(done(&run(&world, CMD_STOP_COMBAT, &refs(&[0x00CC_0001]), &ctx)));
        assert!(world.get::<crate::AiCombatState>(actor).is_some());
        assert_eq!(getter(&world, CMD_IS_IN_COMBAT, &[], &ctx), Getter::Value(1.0));
        assert_eq!(getter(&world, CMD_GET_COMBAT_TARGET, &[], &ctx), Getter::Ref(OTHER));
        assert!(done(&run(&world, CMD_STOP_COMBAT, &refs(&[OTHER]), &ctx)));
        assert!(world.get::<crate::AiCombatState>(actor).is_none());
        assert_eq!(getter(&world, CMD_IS_IN_COMBAT, &[], &ctx), Getter::Value(0.0));
        assert_eq!(getter(&world, CMD_GET_COMBAT_TARGET, &[], &ctx), Getter::Ref(0));
    }

    #[test]
    fn equip_item_with_the_no_unequip_flag_is_declined() {
        let (world, _, _) = world();
        let args = [ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0)];
        assert_eq!(run(&world, CMD_EQUIP_ITEM, &args, &context(Some(ACTOR))), Action::Declined);
    }

    #[test]
    fn a_command_this_module_does_not_run_is_left_for_the_host() {
        let (world, _, _) = world();
        assert_eq!(run(&world, 0x1fff, &[], &context(Some(ACTOR))), Action::Unhandled);
        assert_eq!(getter(&world, 0x1fff, &[], &context(Some(ACTOR))), Getter::Unhandled);
    }
}
