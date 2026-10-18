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
/// `Karma` in the Fallout actor-value table.
const KARMA_CODE: f32 = 23.0;

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
            | CMD_ENABLE_PLAYER_CONTROLS
            | CMD_DISABLE_PLAYER_CONTROLS
            | CMD_ADD_PERK
            | CMD_REMOVE_PERK
            | CMD_SET_FACTION_RANK
            | CMD_SET_DESTROYED
            | CMD_MARK_FOR_DELETE
            | CMD_SET_RESTRAINED
            | CMD_REWARD_XP
            | CMD_REWARD_KARMA
            | CMD_REMOVE_ALL_ITEMS
            | CMD_PLACE_AT_ME
            | CMD_RESET_AI
            | CMD_SET_OWNERSHIP
            | CMD_CLEAR_OWNERSHIP
            | CMD_SAY
            | CMD_SAY_TO
            | CMD_SHOW_MESSAGE
            | CMD_START_CONVERSATION
            | CMD_UNEQUIP_ITEM
            | CMD_PLAY_SOUND_3D
            | CMD_ADD_REPUTATION
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
    let needs_caller = !matches!(
        cmd,
        CMD_SET_ENEMY
            | CMD_SET_ALLY
            | CMD_ADD_ACHIEVEMENT
            | CMD_SET_ESSENTIAL
            | CMD_ENABLE_PLAYER_CONTROLS
            | CMD_DISABLE_PLAYER_CONTROLS
            | CMD_REWARD_XP
            | CMD_REWARD_KARMA
            | CMD_SHOW_MESSAGE
            | CMD_ADD_REPUTATION
    );
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
    // `0x1059` is Oblivion's text `Message` (which the VM decodes itself): a message form is
    // `ShowMessage` on the Fallout pair.
    if cmd == CMD_SHOW_MESSAGE && !crate::ScriptGame::is_fallout3_nv(world) {
        return Action::Unhandled;
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
        // GECK: the flags are Movement, PipBoy, Fighting, POV, Looking, RolloverText,
        // Sneaking; with none written, `EnablePlayerControls` is `1 1 1 1 1 1 1` and
        // `DisablePlayerControls` is `1 1 1 1 0 0 0`. A flag names a domain and the
        // call assigns it. The page's definitions give the mapping: Movement also
        // takes world activation, PipBoy the pip-boy/wait menus (`menu`), POV camera
        // switching. RolloverText has no domain here and is not applied.
        CMD_ENABLE_PLAYER_CONTROLS | CMD_DISABLE_PLAYER_CONTROLS => {
            let enable = cmd == CMD_ENABLE_PLAYER_CONTROLS;
            let flag = |i: usize, default: bool| if args.len() > i { num(args, i) != 0.0 } else { default };
            let defaults = if enable { [true; 7] } else { [true, true, true, true, false, false, false] };
            let [movement, pipboy, fighting, pov, looking, _rollover, sneaking] = std::array::from_fn(|i| flag(i, defaults[i]));
            vec![Effect::SetPlayerControls {
                enabled: enable,
                selection: crate::player_control::PlayerControlSelection {
                    movement,
                    fighting,
                    camera_switching: pov,
                    looking,
                    sneaking,
                    menu: pipboy,
                    activation: movement,
                    journal_tabs: false,
                    pov_type: 0,
                },
            }]
        }
        CMD_ADD_PERK | CMD_REMOVE_PERK => {
            let Some(perk) = form_arg(args, 0) else { return Action::Declined };
            vec![Effect::ChangePerk { actor: me, perk: bind.object(perk), add: cmd == CMD_ADD_PERK }]
        }
        // `SetFactionRank <faction> <rank>`: rank `-1` removes the actor ("This will
        // remove the player from the faction"). The GECK also sets it on the base, so
        // every actor spawned from it later has it; here only this reference changes.
        CMD_SET_FACTION_RANK => {
            let Some(faction) = form_arg(args, 0) else { return Action::Declined };
            let rank = num(args, 1);
            let change = if rank < 0.0 { FactionChange::Remove } else { FactionChange::SetRank(rank.min(f32::from(i8::MAX)) as i8) };
            vec![Effect::ChangeFaction { actor: me, faction: bind.object(faction), change }]
        }
        CMD_SET_DESTROYED => {
            crate::destruction::set_destroyed(world, caller, num(args, 0) != 0.0);
            return Action::Done { value: None, effect: format!("SetDestroyed {caller:08X} {}", num(args, 0) != 0.0) };
        }
        // GECK: "set the delete flag on a disabled reference ... It will not work on
        // enabled references". A created reference is deleted when its cell goes; an
        // authored one is not truly deleted, but "the script on such objects is
        // removed and no longer runs".
        CMD_MARK_FOR_DELETE => {
            if crate::reference_enabled(world, caller) {
                return Action::Done { value: None, effect: String::new() };
            }
            let created = world
                .try_resource_mut::<crate::CreatedReferences>()
                .is_some_and(|mut created| created.delete_when_able(caller));
            if !created {
                if let Some(mut instances) = world.query_mut::<crate::obscript_objects::ObScriptInstance>() {
                    instances.remove(caller_entity.expect("checked live above"));
                }
            }
            return Action::Done { value: None, effect: format!("MarkForDelete {caller:08X}") };
        }
        CMD_SET_RESTRAINED => {
            let Some(actor) = caller_entity else { return Action::Declined };
            let restrained = num(args, 0) != 0.0;
            if !crate::update_actor_control(world, actor, |state| state.set_restrained(restrained)) {
                return Action::Declined;
            }
            return Action::Done { value: None, effect: format!("SetRestrained {caller:08X} {restrained}") };
        }
        CMD_REWARD_XP => vec![Effect::RewardPlayerXp { amount: ItemCount::Literal(num(args, 0).max(0.0) as u32), direct: false }],
        // `RewardKarma <points>`: karma is an actor value on the player.
        CMD_REWARD_KARMA => {
            let Some(avif) = crate::obscript_actor_values::actor_value_form(world, KARMA_CODE) else {
                return Action::Declined;
            };
            let Some(player) = crate::papyrus_demo::PapyrusPlayerEntity::get(world) else {
                return Action::Declined;
            };
            crate::fragment::mod_actor_value_permanent(world, player, avif, num(args, 0));
            return Action::Done { value: None, effect: format!("RewardKarma {}", num(args, 0)) };
        }
        // GECK: "Removes all items from the calling reference's inventory. If a
        // target container is specified, the items are moved ... otherwise the items
        // are destroyed". The retain-ownership flag has no ledger to carry it.
        CMD_REMOVE_ALL_ITEMS => {
            let Some(source) = caller_entity else { return Action::Declined };
            let destination = match form_arg(args, 0) {
                Some(form) => match live(world, form) {
                    Some(entity) => Some(entity),
                    None => return Action::Declined,
                },
                None => None,
            };
            let stacks: Vec<(u32, u32)> = world
                .get::<byroredux_core::ecs::components::Inventory>(source)
                .map(|inventory| inventory.items.iter().filter(|stack| stack.count > 0).map(|stack| (stack.base_form_id, stack.count)).collect())
                .unwrap_or_default();
            for (item, count) in stacks {
                crate::fragment::remove_items(world, source, item, count, destination);
            }
            return Action::Done { value: None, effect: format!("RemoveAllItems {caller:08X}") };
        }
        // `PlaceAtMe <base> [count] [distance] [direction]` at the calling
        // reference: the created references are the ones `CreatedReferences` spawns
        // when their cell loads. The offset is not modelled (the Papyrus path has none
        // either), so a distance is declined rather than dropped.
        CMD_PLACE_AT_ME => {
            let Some(base) = form_arg(args, 0) else { return Action::Declined };
            let count = if args.len() > 1 { num(args, 1).clamp(0.0, 32.0) as u32 } else { 1 };
            if num(args, 2) != 0.0 {
                return Action::Declined;
            }
            let Some(ids) = world.try_resource::<crate::CreatedReferences>().map(|created| created.scripted_ids()) else {
                return Action::Declined;
            };
            let mut last = 0;
            for _ in 0..count {
                let Some(form) = ids.next() else { break };
                if let Some(mut created) = world.try_resource_mut::<crate::CreatedReferences>() {
                    created.create_scripted(form, base, caller, false);
                }
                last = form;
            }
            return Action::Done {
                value: Some(ObScriptValue::Ref(last)),
                effect: format!("PlaceAtMe {base:08X} x{count} at {caller:08X}"),
            };
        }
        // GECK: "clears all current AI behaviors from an actor. Combat, pathing, and
        // packages are re-evaluated."
        CMD_RESET_AI => {
            let Some(actor) = caller_entity else { return Action::Declined };
            if let Some(mut states) = world.query_mut::<crate::AiCombatState>() {
                states.remove(actor);
            }
            crate::script_packages::remove_script_package(world, actor);
            crate::package::request_package_evaluation(world, actor);
            return Action::Done { value: None, effect: format!("ResetAI {caller:08X}") };
        }
        // `SetOwnership [owner]`: the calling reference is owned by the NPC or faction
        // named, or — "if the parameter is omitted" — by the player, whose things anyone
        // may take: no owner, which is how a cleared one is read. `ClearOwnership` is the
        // second spelling of that.
        CMD_SET_OWNERSHIP | CMD_CLEAR_OWNERSHIP => {
            let owner = if cmd == CMD_SET_OWNERSHIP { form_arg(args, 0).unwrap_or(0) } else { 0 };
            let Some(mut overrides) = world.try_resource_mut::<crate::OwnerOverrides>() else {
                return Action::Declined;
            };
            overrides.set(caller, owner);
            return Action::Done { value: None, effect: format!("SetOwnership {caller:08X} {owner:08X}") };
        }
        // `Say <topic> [force subtitle] [speaker] [position flag]` and `SayTo <target> <topic>
        // [force subtitle] [no target look]`: the calling actor speaks one line of the topic,
        // and the call answers how many seconds it takes (CS: "useful for timing followups").
        // A topic with nothing eligible to say answers 0 and speaks nothing.
        CMD_SAY | CMD_SAY_TO => {
            let Some(speaker) = caller_entity else { return Action::Declined };
            let (target, topic) = if cmd == CMD_SAY {
                (None, form_arg(args, 0))
            } else {
                match form_arg(args, 0).map(|form| live(world, form)) {
                    Some(Some(target)) => (Some(target), form_arg(args, 1)),
                    _ => return Action::Declined,
                }
            };
            let Some(topic) = topic else { return Action::Declined };
            let seconds = crate::obscript_speech::say(world, speaker, topic, target).unwrap_or(0.0);
            return Action::Done {
                value: Some(ObScriptValue::Num(seconds)),
                effect: format!("{} {caller:08X} topic {topic:08X}", if cmd == CMD_SAY { "Say" } else { "SayTo" }),
            };
        }
        // `ShowMessage <message> [value ...]`: the message record's notification or box, its
        // `%` spots filled from the values (the Papyrus `Message.Show` effect).
        CMD_SHOW_MESSAGE => {
            let Some(message) = form_arg(args, 0) else { return Action::Declined };
            let values: Vec<f32> = args.iter().skip(1).map(ObScriptValue::as_num).collect();
            vec![Effect::ShowMessage { message: bind.object(message), args: values }]
        }
        // `StartConversation <target> [topic]` ("force a conversation ... starting with a specified
        // topic", and "set the player as the TargetActor" to force-greet): the actor walks up to the
        // player and opens the topic, through the force-greet the dialogue runtime already has. A
        // conversation between two NPCs has no runtime, so another target is declined.
        CMD_START_CONVERSATION => {
            let Some(speaker) = caller_entity else { return Action::Declined };
            if form_arg(args, 0) != Some(crate::PLAYER_REFERENCE) {
                return Action::Declined;
            }
            let Some(mut lines) = world.try_resource_mut::<crate::obscript_speech::PendingScriptedLines>() else {
                return Action::Declined;
            };
            lines.conversations.push((speaker, form_arg(args, 1)));
            return Action::Done { value: None, effect: format!("StartConversation {caller:08X}") };
        }
        // `UnequipItem <item> [no-equip flag] [hide message]`: the actor takes the item off. The
        // no-equip flag ("unable to equip the item until EquipItem is called") has no state to
        // carry it, so a set flag declines, as `EquipItem`'s no-unequip flag does.
        CMD_UNEQUIP_ITEM => {
            use byroredux_core::ecs::components::{EquipmentSlots, Inventory, InventoryIndex};
            let (Some(item), Some(actor)) = (form_arg(args, 0), caller_entity) else {
                return Action::Declined;
            };
            if num(args, 1) != 0.0 {
                return Action::Declined;
            }
            let released = {
                let Some((inventories, mut equipment)) = world.query_2_mut::<Inventory, EquipmentSlots>().map(|(i, e)| (i, e)) else {
                    return Action::Declined;
                };
                let Some(index) = inventories
                    .get(actor)
                    .and_then(|inventory| inventory.items.iter().position(|stack| stack.base_form_id == item && stack.count > 0))
                    .and_then(|index| u32::try_from(index).ok())
                    .map(InventoryIndex)
                else {
                    return Action::Declined;
                };
                equipment.get_mut(actor).is_some_and(|slots| slots.release(index))
            };
            if released {
                crate::emit_equipment_changes(world, actor, [crate::EquipmentChange { item_form_id: item, equipped: false }]);
            }
            return Action::Done { value: None, effect: format!("UnequipItem {caller:08X} {item:08X}") };
        }
        // `PlaySound3D <sound>`: "plays the specified sound, at the location of the calling reference".
        CMD_PLAY_SOUND_3D => {
            let Some(sound) = form_arg(args, 0) else { return Action::Declined };
            let Some(mut pending) = world.try_resource_mut::<crate::PendingSoundPlays>() else {
                return Action::Declined;
            };
            pending.0.push(crate::SoundPlay { sound, source: caller });
            return Action::Done { value: None, effect: format!("PlaySound3D {sound:08X} at {caller:08X}") };
        }
        // `AddReputation <REPU> <fame 1 | infamy 0> <amount 1-5>`: the player's reputation with the
        // faction rises by the points the amount names (1, 2, 4, 7, 12 by default).
        CMD_ADD_REPUTATION => {
            let (Some(repu), Some(player)) = (form_arg(args, 0), crate::papyrus_demo::PapyrusPlayerEntity::get(world)) else {
                return Action::Declined;
            };
            let amount = num(args, 2);
            if !(1.0..=5.0).contains(&amount) || !matches!(num(args, 1) as i32, 0 | 1) {
                return Action::Declined;
            }
            let points = u16::from(byroredux_core::character::reputation_bump_points(amount as u8));
            let Some(mut standings) = world.query_mut::<byroredux_core::character::FactionReputation>() else {
                return Action::Declined;
            };
            if standings.get_mut(player).is_none() {
                standings.insert(player, Default::default());
            }
            let standing = standings.get_mut(player).expect("present or inserted");
            if num(args, 1) as i32 == 1 {
                standing.add_fame(repu, points);
            } else {
                standing.add_infamy(repu, points);
            }
            return Action::Done { value: None, effect: format!("AddReputation {repu:08X} +{points}") };
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
const CMD_GET_LINKED_REF: u16 = 0x116b;
const CMD_GET_PARENT_REF: u16 = 0x1113;
const CMD_GET_DESTROYED: u16 = 0x10cb;
const CMD_GET_RESTRAINED: u16 = 0x10f4;
const CMD_GET_IN_SAME_CELL: u16 = 0x1020;
const CMD_GET_IS_CURRENT_PACKAGE: u16 = 0x10a1;
const CMD_GET_HEALTH_PERCENTAGE: u16 = 0x11af;
const CMD_GET_CURRENT_TIME: u16 = 0x1012;

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
            | CMD_GET_PARENT_REF
            | CMD_GET_DESTROYED
            | CMD_GET_RESTRAINED
            | CMD_GET_IN_SAME_CELL
            | CMD_GET_LINKED_REF
            | CMD_GET_IS_CURRENT_PACKAGE
            | CMD_GET_CURRENT_TIME
            | CMD_GET_HEALTH_PERCENTAGE
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
    // CS *GetCurrentTime*: "the current game time, in decimal format, e.g. 4:30 am = 4.5,
    // 7:45 pm = 19.75" — the hour of the game clock the update system reports each frame.
    // No reference is involved.
    if cmd == CMD_GET_CURRENT_TIME {
        return world
            .try_resource::<crate::fragment::ScriptUpdates>()
            .and_then(|updates| updates.game_hours())
            .map_or(Getter::Declined, |hours| Getter::Value(hours.rem_euclid(24.0) as f32));
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
        // `GetLinkedRef`: the reference's linked-ref (`XLKR`) target. `0x116b` is
        // `PurgeCellBuffers` in Oblivion, so it is only this on the Fallout pair.
        CMD_GET_LINKED_REF if crate::ScriptGame::is_fallout3_nv(world) => {
            Getter::Ref(crate::linked_ref(world, caller, 0).unwrap_or(0))
        }
        CMD_GET_LINKED_REF => Getter::Unhandled,
        // CS *GetParentRef*: "the calling object's parent reference (0 if no
        // parent)" — the reference in its Enable Parent tab (`XESP`).
        CMD_GET_PARENT_REF => Getter::Ref(
            world
                .try_resource::<crate::AuthoredDisabledReferences>()
                .and_then(|authored| authored.1.get(&caller).copied())
                .unwrap_or(0),
        ),
        // "Returns 1 if the actor is currently running the specified package": the package
        // the AI last reported for the actor.
        CMD_GET_IS_CURRENT_PACKAGE => {
            let Some(package) = form_arg(args, 0) else { return Getter::Declined };
            let current = world.try_resource::<crate::fragment::ActivePackages>().and_then(|active| active.0.get(&subject).copied());
            Getter::Value(f32::from(u8::from(current == Some(package))))
        }
        // GECK *GetHealthPercentage*: "how injured an actor is ... a value from 0 to 1.0" — the
        // current Health over the undamaged ceiling. Fallout only: `0x11af` is another command in Oblivion.
        CMD_GET_HEALTH_PERCENTAGE if crate::ScriptGame::is_fallout3_nv(world) => {
            let Some(health) = world.get::<byroredux_core::ecs::components::ActorVitals>(subject).map(|vitals| vitals.health) else {
                return Getter::Declined;
            };
            let current = actor_value_view(world, subject, health, ActorValueView::Current);
            let ceiling = actor_value_view(world, subject, health, ActorValueView::Undamaged);
            Getter::Value(if ceiling > 0.0 { (current / ceiling).clamp(0.0, 1.0) } else { 0.0 })
        }
        CMD_GET_HEALTH_PERCENTAGE => Getter::Unhandled,
        CMD_GET_DESTROYED => Getter::Value(f32::from(u8::from(crate::destruction::is_destroyed(world, caller)))),
        CMD_GET_RESTRAINED => {
            Getter::Value(f32::from(u8::from(world.get::<crate::ActorControlState>(subject).is_some_and(|state| state.restrained))))
        }
        // "Returns true if the calling reference is in the same cell as the target
        // actor": two loaded references with the same, known cell.
        CMD_GET_IN_SAME_CELL => {
            use byroredux_core::ecs::components::{CellFormId, CellRoot};
            let Some(other) = form_arg(args, 0).and_then(|form| live(world, form)) else {
                return Getter::Declined;
            };
            let cell_of = |entity: EntityId| {
                let root = world.get::<CellRoot>(entity).map_or(entity, |root| root.0);
                world.get::<CellFormId>(root).map(|cell| cell.0)
            };
            match (cell_of(subject), cell_of(other)) {
                (Some(a), Some(b)) => Getter::Value(f32::from(u8::from(a == b))),
                _ => Getter::Declined,
            }
        }
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

    fn av_world_with_player() -> (World, EntityId, EntityId) {
        let (mut world, actor, _other) = world();
        let player = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        (world, actor, player)
    }

    #[test]
    fn add_perk_and_remove_perk_change_the_actors_perks() {
        const PERK: u32 = 0x0003_0001;
        let (mut world, actor, _) = world();
        world.register::<byroredux_core::character::Perks>();
        let ctx = context(Some(ACTOR));
        assert!(done(&run(&world, CMD_ADD_PERK, &refs(&[PERK]), &ctx)));
        assert_eq!(world.get::<byroredux_core::character::Perks>(actor).map(|perks| perks.rank(PERK)), Some(1));
        assert!(done(&run(&world, CMD_REMOVE_PERK, &refs(&[PERK]), &ctx)));
        assert_eq!(world.get::<byroredux_core::character::Perks>(actor).map(|perks| perks.rank(PERK)), Some(0));
    }

    #[test]
    fn destroyed_and_restrained_are_set_and_read_back() {
        let (world, actor, _) = world();
        let ctx = context(Some(ACTOR));
        assert_eq!(getter(&world, CMD_GET_DESTROYED, &[], &ctx), Getter::Value(0.0));
        assert!(done(&run(&world, CMD_SET_DESTROYED, &[ObScriptValue::Num(1.0)], &ctx)));
        assert_eq!(getter(&world, CMD_GET_DESTROYED, &[], &ctx), Getter::Value(1.0));
        assert!(done(&run(&world, CMD_SET_DESTROYED, &[ObScriptValue::Num(0.0)], &ctx)));
        assert_eq!(getter(&world, CMD_GET_DESTROYED, &[], &ctx), Getter::Value(0.0));
        assert!(done(&run(&world, CMD_SET_RESTRAINED, &[ObScriptValue::Num(1.0)], &ctx)));
        assert!(world.get::<crate::ActorControlState>(actor).is_some_and(|state| state.restrained));
        assert_eq!(getter(&world, CMD_GET_RESTRAINED, &[], &ctx), Getter::Value(1.0));
    }

    #[test]
    fn remove_all_items_empties_the_inventory_or_moves_it() {
        let (world, actor, other) = world();
        let ctx = context(Some(ACTOR));
        run(&world, CMD_ADD_ITEM, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(4.0)], &ctx);
        // Moved to OTHER.
        assert!(done(&run(&world, CMD_REMOVE_ALL_ITEMS, &refs(&[OTHER]), &ctx)));
        assert_eq!((count(&world, actor, ITEM), count(&world, other, ITEM)), (0, 4));
        // Destroyed.
        assert!(done(&run(&world, CMD_REMOVE_ALL_ITEMS, &[], &context(Some(OTHER)))));
        assert_eq!(count(&world, other, ITEM), 0);
    }

    #[test]
    fn place_at_me_creates_a_reference_and_answers_it() {
        let (mut world, _, _) = world();
        world.insert_resource(crate::CreatedReferences::default());
        let ctx = context(Some(ACTOR));
        let Action::Done { value: Some(ObScriptValue::Ref(created)), .. } = run(&world, CMD_PLACE_AT_ME, &refs(&[ITEM]), &ctx) else {
            panic!("PlaceAtMe answers the reference it made");
        };
        assert_ne!(created, 0);
        let created_refs = world.try_resource::<crate::CreatedReferences>().unwrap();
        assert!(created_refs.iter().any(|record| record.form_id == created && record.base == ITEM && record.at_reference == ACTOR));
        drop(created_refs);
        // An offset is not modelled: declined, not dropped.
        let offset = [ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0), ObScriptValue::Num(64.0)];
        assert_eq!(run(&world, CMD_PLACE_AT_ME, &offset, &ctx), Action::Declined);
    }

    #[test]
    fn mark_for_delete_works_only_on_a_disabled_reference() {
        use crate::obscript_objects::ObScriptInstance;
        let (mut world, actor, _) = world();
        world.insert_resource(crate::ReferenceEnableState::default());
        world.insert_resource(crate::CreatedReferences::default());
        world.register::<ObScriptInstance>();
        world.insert(actor, ObScriptInstance { script: 1 });
        let ctx = context(Some(ACTOR));
        // Enabled: "It will not work on enabled references" — nothing happens.
        assert!(done(&run(&world, CMD_MARK_FOR_DELETE, &[], &ctx)));
        assert!(world.has::<ObScriptInstance>(actor), "an enabled reference keeps its script");
        // Disabled and authored: "the script on such objects is removed and no longer runs".
        world.try_resource_mut::<crate::ReferenceEnableState>().unwrap().set_enabled(ACTOR, false);
        assert!(done(&run(&world, CMD_MARK_FOR_DELETE, &[], &ctx)));
        assert!(!world.has::<ObScriptInstance>(actor));
        // Disabled and created by a script: it is deleted once its cell goes.
        world.try_resource_mut::<crate::CreatedReferences>().unwrap().create_scripted(OTHER, ITEM, ACTOR, false);
        world.try_resource_mut::<crate::ReferenceEnableState>().unwrap().set_enabled(OTHER, false);
        assert!(done(&run(&world, CMD_MARK_FOR_DELETE, &[], &context(Some(OTHER)))));
        let created = world.try_resource::<crate::CreatedReferences>().unwrap();
        let record = created.iter().find(|record| record.form_id == OTHER).unwrap();
        assert_eq!(record.deletion, crate::CreatedDeletion::WhenAble);
    }

    #[test]
    fn player_controls_assign_the_domains_the_flags_name() {
        let (mut world, _, _) = av_world_with_player();
        world.insert_resource(crate::PlayerControlState::default());
        let ctx = context(None);
        // DisablePlayerControls with its defaults: Movement, PipBoy, Fighting, POV.
        assert!(done(&run(&world, CMD_DISABLE_PLAYER_CONTROLS, &[], &ctx)));
        let state = *world.try_resource::<crate::PlayerControlState>().unwrap();
        assert!(!state.movement_enabled && !state.menu_enabled && !state.fighting_enabled && !state.camera_switching_enabled);
        assert!(state.looking_enabled && state.sneaking_enabled, "Looking and Sneaking are not in the default");
        assert!(!state.activation_enabled, "Movement takes world activation with it");
        // `EnablePlayerControls 0 0 1`: only Fighting comes back.
        let flags = [0.0, 0.0, 1.0].map(ObScriptValue::Num);
        assert!(done(&run(&world, CMD_ENABLE_PLAYER_CONTROLS, &flags, &ctx)));
        let state = *world.try_resource::<crate::PlayerControlState>().unwrap();
        assert!(state.fighting_enabled && !state.movement_enabled);
    }

    #[test]
    fn get_in_same_cell_compares_the_two_references_cells() {
        use byroredux_core::ecs::components::CellFormId;
        let (mut world, actor, other) = world();
        world.register::<CellFormId>();
        let ctx = context(Some(ACTOR));
        assert_eq!(getter(&world, CMD_GET_IN_SAME_CELL, &refs(&[OTHER]), &ctx), Getter::Declined, "no cell is known yet");
        world.insert(actor, CellFormId(0x5A));
        world.insert(other, CellFormId(0x5A));
        assert_eq!(getter(&world, CMD_GET_IN_SAME_CELL, &refs(&[OTHER]), &ctx), Getter::Value(1.0));
        world.insert(other, CellFormId(0x5B));
        assert_eq!(getter(&world, CMD_GET_IN_SAME_CELL, &refs(&[OTHER]), &ctx), Getter::Value(0.0));
    }

    #[test]
    fn get_linked_ref_and_get_parent_ref_answer_the_linked_and_parent_references() {
        let (mut world, _, _) = world();
        let ctx = context(Some(ACTOR));
        let mut links = crate::LinkedRefOverrides::default();
        links.set(ACTOR, 0, OTHER);
        world.insert_resource(links);
        assert_eq!(getter(&world, CMD_GET_LINKED_REF, &[], &ctx), Getter::Ref(OTHER));
        assert_eq!(getter(&world, CMD_GET_PARENT_REF, &[], &ctx), Getter::Ref(0), "no enable parent");
        world.insert_resource(crate::AuthoredDisabledReferences(Default::default(), [(ACTOR, ITEM)].into()));
        assert_eq!(getter(&world, CMD_GET_PARENT_REF, &[], &ctx), Getter::Ref(ITEM));
    }

    #[test]
    fn set_ownership_names_the_owner_and_no_argument_is_the_player() {
        const FACTION: u32 = 0x0006_0001;
        let (mut world, _, _) = world();
        world.insert_resource(crate::OwnerOverrides::default());
        let ctx = context(Some(ACTOR));
        assert!(done(&run(&world, CMD_SET_OWNERSHIP, &refs(&[FACTION]), &ctx)));
        assert_eq!(world.try_resource::<crate::OwnerOverrides>().unwrap().get(ACTOR), Some(FACTION));
        // "If the parameter is omitted, the reference will be set to be owned by the player."
        assert!(done(&run(&world, CMD_SET_OWNERSHIP, &[], &ctx)));
        assert_eq!(world.try_resource::<crate::OwnerOverrides>().unwrap().get(ACTOR), Some(0));
    }

    #[test]
    fn get_is_current_package_reads_the_package_the_ai_reported() {
        const WALK: u32 = 0x0005_0001;
        let (world, actor, _) = world();
        let ctx = context(Some(ACTOR));
        assert_eq!(getter(&world, CMD_GET_IS_CURRENT_PACKAGE, &refs(&[WALK]), &ctx), Getter::Value(0.0));
        crate::fragment::report_package_transition(&world, actor, None, Some(WALK));
        assert_eq!(getter(&world, CMD_GET_IS_CURRENT_PACKAGE, &refs(&[WALK]), &ctx), Getter::Value(1.0));
        assert_eq!(getter(&world, CMD_GET_IS_CURRENT_PACKAGE, &refs(&[WALK + 1]), &ctx), Getter::Value(0.0));
    }

    #[test]
    fn get_health_percentage_is_current_over_the_undamaged_ceiling() {
        use byroredux_core::ecs::components::ActorVitals;
        let (mut world, actor, _) = world();
        world.register::<ActorVitals>();
        world.insert(actor, ActorVitals { health: HEALTH });
        let ctx = context(Some(ACTOR));
        run(&world, CMD_SET_AV, &[ObScriptValue::Num(16.0), ObScriptValue::Num(80.0)], &ctx);
        assert_eq!(getter(&world, CMD_GET_HEALTH_PERCENTAGE, &[], &ctx), Getter::Value(1.0));
        run(&world, CMD_DAMAGE_AV, &[ObScriptValue::Num(16.0), ObScriptValue::Num(20.0)], &ctx);
        assert_eq!(getter(&world, CMD_GET_HEALTH_PERCENTAGE, &[], &ctx), Getter::Value(0.75));
    }

    #[test]
    fn unequip_item_releases_the_slot_and_announces_it() {
        use byroredux_core::ecs::components::{EquipmentSlots, InventoryIndex};
        let (mut world, actor, _) = world();
        world.register::<EquipmentSlots>();
        world.register::<crate::EquipmentEventBatch>();
        let ctx = context(Some(ACTOR));
        run(&world, CMD_ADD_ITEM, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0)], &ctx);
        let mut slots = EquipmentSlots::new();
        slots.equip(0b1, InventoryIndex(0));
        world.insert(actor, slots);
        assert!(done(&run(&world, CMD_UNEQUIP_ITEM, &refs(&[ITEM]), &ctx)));
        assert!(!world.get::<EquipmentSlots>(actor).unwrap().is_equipped(InventoryIndex(0)));
        assert_eq!(
            world.get::<crate::EquipmentEventBatch>(actor).map(|batch| batch.0.clone()),
            Some(vec![crate::EquipmentChange { item_form_id: ITEM, equipped: false }])
        );
        // An item the actor does not carry, and the no-equip flag, are declined.
        assert_eq!(run(&world, CMD_UNEQUIP_ITEM, &refs(&[ITEM + 1]), &ctx), Action::Declined);
        assert_eq!(run(&world, CMD_UNEQUIP_ITEM, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0)], &ctx), Action::Declined);
    }

    #[test]
    fn add_reputation_adds_the_points_the_amount_names_to_the_players_standing() {
        use byroredux_core::character::FactionReputation;
        const REPU: u32 = 0x0009_0001;
        let (mut world, _, player) = av_world_with_player();
        world.register::<FactionReputation>();
        let ctx = context(None);
        let add = |axis: f32, amount: f32| [ObScriptValue::Ref(REPU), ObScriptValue::Num(axis), ObScriptValue::Num(amount)];
        // "AddReputation RepNVGoodsprings 0 5": infamy, the very major bump (12 by default).
        assert!(done(&run(&world, CMD_ADD_REPUTATION, &add(0.0, 5.0), &ctx)));
        assert!(done(&run(&world, CMD_ADD_REPUTATION, &add(1.0, 3.0), &ctx)));
        let standing = world.get::<FactionReputation>(player).unwrap();
        assert_eq!((standing.infamy(REPU), standing.fame(REPU)), (12, 4));
        drop(standing);
        // "it only allows 1-5", and the axis is 0 or 1.
        assert_eq!(run(&world, CMD_ADD_REPUTATION, &add(1.0, 6.0), &ctx), Action::Declined);
        assert_eq!(run(&world, CMD_ADD_REPUTATION, &add(2.0, 3.0), &ctx), Action::Declined);
    }

    #[test]
    fn play_sound_3d_plays_at_the_calling_reference() {
        const SOUND: u32 = 0x0009_0002;
        let (mut world, _, _) = world();
        world.insert_resource(crate::PendingSoundPlays::default());
        assert!(done(&run(&world, CMD_PLAY_SOUND_3D, &[ObScriptValue::Ref(SOUND)], &context(Some(ACTOR)))));
        assert_eq!(world.try_resource::<crate::PendingSoundPlays>().unwrap().0, vec![crate::SoundPlay { sound: SOUND, source: ACTOR }]);
        // It plays "at the location of the calling reference": with none, there is no place.
        assert_eq!(run(&world, CMD_PLAY_SOUND_3D, &[ObScriptValue::Ref(SOUND)], &context(None)), Action::Declined);
    }

    #[test]
    fn start_conversation_with_the_player_asks_for_a_force_greet() {
        const TOPIC: u32 = 0x0009_0003;
        let (world, actor, _) = world();
        let ctx = context(Some(ACTOR));
        assert!(done(&run(&world, CMD_START_CONVERSATION, &refs(&[crate::PLAYER_REFERENCE, TOPIC]), &ctx)));
        assert!(done(&run(&world, CMD_START_CONVERSATION, &refs(&[crate::PLAYER_REFERENCE]), &ctx)));
        let lines = world.try_resource::<crate::obscript_speech::PendingScriptedLines>().unwrap();
        assert_eq!(lines.conversations, vec![(actor, Some(TOPIC)), (actor, None)], "a topic, then the generic greeting");
        drop(lines);
        // A conversation between two NPCs has no runtime.
        assert_eq!(run(&world, CMD_START_CONVERSATION, &refs(&[OTHER, TOPIC]), &ctx), Action::Declined);
    }

    #[test]
    fn show_message_is_a_fallout_command_that_queues_the_message_record() {
        const MESSAGE: u32 = 0x0010_2030;
        let (world, _, _) = world();
        assert!(done(&run(&world, CMD_SHOW_MESSAGE, &[ObScriptValue::Ref(MESSAGE), ObScriptValue::Num(3.0)], &context(None))));
        // A null form names no message; and in Oblivion the id is the text `Message`, not this.
        assert_eq!(run(&world, CMD_SHOW_MESSAGE, &[ObScriptValue::Ref(0)], &context(None)), Action::Declined);
        let mut oblivion = World::new();
        oblivion.insert_resource(crate::ScriptGame(GameKind::Oblivion));
        assert_eq!(run(&oblivion, CMD_SHOW_MESSAGE, &[ObScriptValue::Ref(MESSAGE)], &context(None)), Action::Unhandled);
    }

    #[test]
    fn get_current_time_is_the_hour_of_the_game_clock() {
        let (world, _, _) = world();
        // No clock reported yet: nothing to read, so it is declined, not midnight.
        assert_eq!(getter(&world, CMD_GET_CURRENT_TIME, &[], &context(None)), Getter::Declined);
        // Day 1, 06:30 (30.5 game hours since the runtime began): "4:30 am = 4.5".
        crate::script_update_system(&world, 0.0, Some(30.5));
        assert_eq!(getter(&world, CMD_GET_CURRENT_TIME, &[], &context(None)), Getter::Value(6.5));
    }

    #[test]
    fn a_command_this_module_does_not_run_is_left_for_the_host() {
        let (world, _, _) = world();
        assert_eq!(run(&world, 0x1fff, &[], &context(Some(ACTOR))), Action::Unhandled);
        assert_eq!(getter(&world, 0x1fff, &[], &context(Some(ACTOR))), Getter::Unhandled);
    }
}
