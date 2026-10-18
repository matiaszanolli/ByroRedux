//! Object scripts — the `SCRI` scripts on placed references (M47.3 phase 2).
//!
//! Oblivion, Fallout 3 and New Vegas attach a script to a base record (`STAT`,
//! `ACTI`, `CONT`, `NPC_`, ...), and every placed reference of it runs that
//! script. The quest VM ([`crate::obscript_quests`]) already runs the same
//! bytecode for quests; this module runs it for references, on
//! [`ObScriptCommandHost`] with the reference as its owner, so a command written
//! with no calling reference (`Disable`, `GetDead`, `MoveTo player`) acts on the
//! script's own reference.
//!
//! Blocks run on the events the engine already raises:
//!
//! | block | runs |
//! |-------|------|
//! | `GameMode` | every frame, for every loaded scripted reference |
//! | `OnLoad` | the frame the reference's cell loads ([`OnCellLoadEvent`]) |
//! | `OnDeath` | the frame an actor dies ([`OnDeathEvent`]) |
//! | `OnActivate` | the frame the reference is activated ([`ActivateEvent`]), with `GetActionRef` the activator |
//! | `OnTriggerEnter` | once per reference entering the volume ([`OnTriggerEnterEvent`]), `GetActionRef` the one entering |
//! | `OnTriggerLeave` | once per reference leaving it ([`OnTriggerLeaveEvent`]), `GetActionRef` the one leaving — Fallout 3 / New Vegas only (`0x1b` is `OnTriggerMob` in Oblivion) |
//! | `OnTrigger` | every frame while anything is inside; the action ref is NOT set (GECK) |
//!
//! `0x1a` is `OnTriggerEnter` on the Fallout pair and `OnTriggerActor` in Oblivion
//! ("run once when an actor triggers"), which the enter event is too. Only the
//! player and the tracked quest/scene actors are detected entering a volume
//! today, so a trigger another NPC would set off stays quiet.
//!
//! **A block can be written for one reference** (`Begin OnTrigger player`), and a
//! script can have several blocks of one type, each for a different reference
//! (FO3's `Begin OnTriggerEnter player` / `... StickyRef` / `... RedRef`). Blocks
//! are therefore addressed individually ([`BlockInfo`]), and one written for a
//! reference runs only for it.
//!
//! **`OnActivate` replaces default processing.** The GECK *Activate* page: a
//! reference's `OnActivate` block runs "instead of the default activation", and
//! the script's own `Activate` command performs the default one. So a reference
//! whose script has the block counts as default-activation-blocked
//! ([`has_activate_block`], read by [`crate::default_activation_blocked`], the
//! gate every native consumer — doors, containers, pickups, dialogue — already
//! uses for Papyrus's `BlockActivation`), and `Activate` queues the native-only
//! activation those consumers then honour. An activation that already carries
//! [`NativeActivation`] is the default one: the block does not run again.
//!
//! **A block that cannot be trusted does not lock the reference.** If an
//! `OnActivate` run declined any command (one the host does not model) or came
//! back malformed, whether the script meant to call `Activate` is unknown, and
//! staying silent would leave a door nothing can open. The default activation
//! goes ahead then — what the reference did before scripts ran — so only a block
//! the host ran in full can withhold it.
//!
//! **Variables.** A script's locals live in the reference's [`ScriptVariables`],
//! by name — the saved, delta-restored component the Papyrus path and the
//! `GetVMScriptVariable` condition already use — so they survive a cell unload and
//! a save without a second ledger.
//!
//! **Kill switch.** `BYRO_NO_OBSCRIPT_OBJECTS` leaves the table uninstalled, so no
//! object script attaches or runs (quest scripts are unaffected).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, Resource, SparseSetStorage, World};
use byroredux_plugin::esm::records::condition::ConditionStringId;
use byroredux_plugin::esm::records::{ScriptRecord, ScriptType};

use crate::condition::entity_global_form_id;
use crate::events::{
    ActivateEvent, EquipmentEventBatch, HitEvent, ItemEventBatch, NativeActivation, OnCellLoadEvent, OnDeathEvent, OnTriggerEnterEvent,
    OnTriggerLeaveEvent,
};
use crate::obscript_quests::ObScriptCommandHost;
use crate::obscript_vm::{
    BlockInfo, BlockOutcome, ObScriptProgram, VmState, BLOCK_GAME_MODE, BLOCK_ON_ACTIVATE, BLOCK_ON_ADD,
    BLOCK_ON_DROP, BLOCK_ON_EQUIP, BLOCK_ON_UNEQUIP, BLOCK_SAY_TO_DONE,
    BLOCK_ON_COMBAT_END, BLOCK_ON_DEATH, BLOCK_ON_HIT, BLOCK_ON_HIT_WITH, BLOCK_ON_LOAD,
    BLOCK_ON_PACKAGE_CHANGE, BLOCK_ON_PACKAGE_DONE, BLOCK_ON_PACKAGE_START, BLOCK_ON_START_COMBAT,
    BLOCK_ON_TRIGGER, BLOCK_ON_TRIGGER_ENTER, BLOCK_ON_TRIGGER_LEAVE,
};
use crate::vm_state::ScriptVariables;

/// One object script, with the blocks it can be asked to run.
pub struct ObjectScript {
    pub record: Arc<ScriptRecord>,
    pub blocks: Vec<BlockInfo>,
}

impl ObjectScript {
    /// The blocks of `block_type`, in order.
    fn of_type(&self, block_type: u16) -> impl Iterator<Item = &BlockInfo> {
        self.blocks.iter().filter(move |info| info.block_type == block_type)
    }
}

/// The load order's object scripts by SCPT FormID. `NOT_SAVED_BY_DESIGN`: rebuilt
/// from the records at every content load.
#[derive(Default)]
pub struct ObScriptObjectTable {
    pub scripts: HashMap<u32, ObjectScript>,
    /// `(script, block)` pairs that came back malformed: reported once, then
    /// skipped — a `GameMode` block would otherwise log every frame.
    malformed: HashSet<(u32, BlockInfo)>,
}

impl Resource for ObScriptObjectTable {}

/// The script a placed reference runs. `NOT_SAVED_BY_DESIGN`: stamped by the cell
/// loader from the base record's `SCRI` on every spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObScriptInstance {
    pub script: u32,
}

impl Component for ObScriptInstance {
    type Storage = SparseSetStorage<Self>;
}

/// Marks an actor whose combat the script has been told about: it has run
/// `OnStartCombat` for the fight it is in. `NOT_SAVED_BY_DESIGN`: after a load an
/// actor that is still in combat is told again, as the original does on a load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObScriptInCombat;

impl Component for ObScriptInCombat {
    type Storage = SparseSetStorage<Self>;
}

/// The script each item base record carries (`SCRI`), for the items that have an
/// installed object script. An item in an inventory is no placed reference, so its
/// script is found by its base form. `NOT_SAVED_BY_DESIGN`: rebuilt from the records at
/// every content load.
#[derive(Debug, Default, Clone)]
pub struct ObScriptItemScripts(pub HashMap<u32, u32>);

impl Resource for ObScriptItemScripts {}

/// Index the scripts of the load order's items. `0` when none of them has an installed
/// object script (or `BYRO_NO_OBSCRIPT_OBJECTS` kept the table out). Call after
/// [`install_object_scripts`].
pub fn install_item_scripts(world: &mut World, index: &byroredux_plugin::esm::records::EsmIndex) -> usize {
    let Some(table) = world.try_resource::<ObScriptObjectTable>() else {
        return 0;
    };
    let items: HashMap<u32, u32> = index
        .items
        .values()
        .filter(|item| item.common.script_form_id != 0 && table.scripts.contains_key(&item.common.script_form_id))
        .map(|item| (item.form_id, item.common.script_form_id))
        .collect();
    drop(table);
    let count = items.len();
    if count > 0 {
        world.insert_resource(ObScriptItemScripts(items));
        log::info!("Indexed {count} ObScript item scripts");
    }
    count
}

/// Index the object scripts that have bytecode and walk cleanly. Returns how many
/// there are; `0` (and no table) when `BYRO_NO_OBSCRIPT_OBJECTS` is set.
pub fn install_object_scripts(world: &mut World, scripts: &HashMap<u32, ScriptRecord>) -> usize {
    if std::env::var_os("BYRO_NO_OBSCRIPT_OBJECTS").is_some() {
        log::info!("ObScript object scripts disabled by BYRO_NO_OBSCRIPT_OBJECTS");
        return 0;
    }
    let mut table = ObScriptObjectTable::default();
    for script in scripts.values() {
        if script.script_type != ScriptType::Object || script.compiled.is_empty() {
            continue;
        }
        // A script that cannot be walked would only ever come back malformed.
        let Some(blocks) = ObScriptProgram::new(script).blocks() else {
            log::debug!("ObScript object script {:08X}: malformed bytecode, not installed", script.form_id);
            continue;
        };
        table.scripts.insert(script.form_id, ObjectScript { record: Arc::new(script.clone()), blocks });
    }
    let count = table.scripts.len();
    if count == 0 {
        return 0;
    }
    world.register::<ObScriptInstance>();
    world.register::<ObScriptInCombat>();
    world.insert_resource(table);
    log::info!("Installed {count} ObScript object scripts");
    count
}

/// Give `entity` the object script `script`, if the table has it. `true` when it
/// attached.
pub fn attach_object_script(world: &mut World, entity: EntityId, script: u32) -> bool {
    let known = world
        .try_resource::<ObScriptObjectTable>()
        .is_some_and(|table| table.scripts.contains_key(&script));
    if !known {
        return false;
    }
    world.insert(entity, ObScriptInstance { script });
    if !world.has::<ScriptVariables>(entity) {
        world.insert(entity, ScriptVariables::default());
    }
    true
}

/// One block to run, and the reference that caused it: the activator for
/// `OnActivate`, the one entering or leaving for the trigger blocks, `None` for
/// the blocks no reference causes.
struct Run {
    block: BlockInfo,
    who: Option<EntityId>,
}

/// A reference to run blocks of this frame.
struct Due {
    entity: EntityId,
    form: u32,
    script: Arc<ScriptRecord>,
    runs: Vec<Run>,
}

/// Run this frame's object-script blocks. Exclusive: scripts write the quest,
/// enable and package state the other Update systems read, and the lock order
/// is the quest tick's (nothing is held across a run).
pub fn obscript_object_system(world: &World, dt: f32) {
    for Due { entity, form, script, runs } in collect_due(world) {
        for Run { block, who } in runs {
            match (block.block_type, who) {
                (BLOCK_ON_ACTIVATE, Some(activator)) => {
                    deliver_activation(world, entity, activator, dt, Some(block));
                }
                // The enter and leave blocks set the action ref to the reference
                // crossing the volume; `OnTrigger` does not ("The action ref is
                // NOT set").
                (BLOCK_ON_TRIGGER_ENTER | BLOCK_ON_TRIGGER_LEAVE, Some(who)) => {
                    run_block(world, entity, form, &script, block, dt, form_of(world, who));
                }
                _ => {
                    run_block(world, entity, form, &script, block, dt, None);
                }
            }
        }
    }
}

/// The references with something to run, and which blocks, snapshotted before any
/// runs.
fn collect_due(world: &World) -> Vec<Due> {
    let instances: Vec<(EntityId, u32)> = match world.query::<ObScriptInstance>() {
        Some(query) => query.iter().map(|(entity, instance)| (entity, instance.script)).collect(),
        None => return Vec::new(),
    };
    if instances.is_empty() {
        return Vec::new();
    }
    let entities_with = |events: Option<Vec<EntityId>>| -> HashSet<EntityId> { events.unwrap_or_default().into_iter().collect() };
    let loaded = entities_with(world.query::<OnCellLoadEvent>().map(|e| e.iter().map(|(entity, _)| entity).collect()));
    // Read, not drained: `reference_script_event_system` owns the marker.
    let died = entities_with(world.query::<OnDeathEvent>().map(|e| e.iter().map(|(entity, _)| entity).collect()));
    // Activations, minus the native-only ones: "no OnActivate event will be sent
    // to any scripts" for the default processing a script asked for.
    let activated: HashMap<EntityId, EntityId> = {
        let native = entities_with(world.query::<NativeActivation>().map(|m| m.iter().map(|(entity, _)| entity).collect()));
        world
            .query::<ActivateEvent>()
            .map(|events| {
                events
                    .iter()
                    .filter(|(entity, _)| !native.contains(entity))
                    .map(|(entity, event)| (entity, event.activator))
                    .collect()
            })
            .unwrap_or_default()
    };
    let entering: HashMap<EntityId, Vec<EntityId>> = world
        .query::<OnTriggerEnterEvent>()
        .map(|events| events.iter().map(|(entity, event)| (entity, event.triggerers.clone())).collect())
        .unwrap_or_default();
    // `0x1b` is `OnTriggerLeave` on Fallout 3 / New Vegas and `OnTriggerMob` on
    // Oblivion: only the former is the leave event.
    let leaving: HashMap<EntityId, Vec<EntityId>> = if crate::ScriptGame::is_fallout3_nv(world) {
        world
            .query::<OnTriggerLeaveEvent>()
            .map(|events| events.iter().map(|(entity, event)| (entity, event.triggerers.clone())).collect())
            .unwrap_or_default()
    } else {
        HashMap::new()
    };
    let Some(table) = world.try_resource::<ObScriptObjectTable>() else {
        return Vec::new();
    };
    let mut due = Vec::new();
    for (entity, script_id) in instances {
        let Some(object) = table.scripts.get(&script_id) else { continue };
        let mut runs = Vec::new();
        // Occupants are read once, and only for a volume that has an `OnTrigger`.
        let mut occupants: Option<Vec<EntityId>> = None;
        for &block in &object.blocks {
            if table.malformed.contains(&(script_id, block)) {
                continue;
            }
            // A block written for one reference (`Begin OnTrigger player`) runs
            // for that reference only.
            let wanted = |who: EntityId| block.filter.is_none_or(|filter| form_of(world, who) == Some(filter));
            let events_of = |events: &HashMap<EntityId, Vec<EntityId>>| -> Vec<EntityId> {
                events.get(&entity).into_iter().flatten().copied().filter(|&who| wanted(who)).collect()
            };
            match block.block_type {
                BLOCK_ON_LOAD if loaded.contains(&entity) => runs.push(Run { block, who: None }),
                BLOCK_ON_ACTIVATE => {
                    if let Some(&activator) = activated.get(&entity).filter(|&&a| wanted(a)) {
                        runs.push(Run { block, who: Some(activator) });
                    }
                }
                BLOCK_ON_TRIGGER_ENTER => runs.extend(events_of(&entering).into_iter().map(|who| Run { block, who: Some(who) })),
                BLOCK_ON_TRIGGER_LEAVE => runs.extend(events_of(&leaving).into_iter().map(|who| Run { block, who: Some(who) })),
                // Every frame while something (the written reference, if one is) is in it.
                BLOCK_ON_TRIGGER => {
                    let inside = occupants.get_or_insert_with(|| crate::trigger::trigger_occupants(world, entity));
                    if inside.iter().any(|&who| wanted(who)) {
                        runs.push(Run { block, who: None });
                    }
                }
                BLOCK_ON_DEATH if died.contains(&entity) => runs.push(Run { block, who: None }),
                BLOCK_GAME_MODE => runs.push(Run { block, who: None }),
                _ => {}
            }
        }
        if runs.is_empty() {
            continue;
        }
        let Some(form) = entity_global_form_id(world, entity) else { continue };
        due.push(Due { entity, form, script: Arc::clone(&object.record), runs });
    }
    due
}

/// Run `entity`'s blocks of `block_types` that `wanted` accepts — in the order the
/// script wrote them, skipping any that came back malformed — with `action_ref` as
/// the reference `GetActionRef` answers. The one tail of the event systems below.
fn run_event_blocks(
    world: &World,
    entity: EntityId,
    block_types: &[u16],
    wanted: impl Fn(&BlockInfo) -> bool,
    action_ref: Option<u32>,
    dt: f32,
) {
    let Some(script_id) = world.get::<ObScriptInstance>(entity).map(|instance| instance.script) else {
        return;
    };
    let Some((script, blocks)) = world.try_resource::<ObScriptObjectTable>().and_then(|table| {
        let object = table.scripts.get(&script_id)?;
        let blocks: Vec<BlockInfo> = object
            .blocks
            .iter()
            .filter(|block| block_types.contains(&block.block_type) && !table.malformed.contains(&(script_id, **block)) && wanted(block))
            .copied()
            .collect();
        Some((Arc::clone(&object.record), blocks))
    }) else {
        return;
    };
    if blocks.is_empty() {
        return;
    }
    let Some(form) = entity_global_form_id(world, entity) else {
        return;
    };
    for block in blocks {
        run_block(world, entity, form, &script, block, dt, action_ref);
    }
}

/// `OnHit` and `OnHitWith`: the frame an actor is struck, for the actor's own script.
///
/// The CS *OnHit* page: "run once when the scripted actor is hit by the specified
/// ActorID. If no parameter is used, the block will run when the scripted actor is
/// hit by anyone" and "only works on NPCs and Creatures". (The GECK page adds that
/// New Vegas changed the block "to run when a weapon hits a target" — a weapon's own
/// script, which is an inventory item's, not a placed reference's.) *OnHitWith* is the
/// same for "a weapon": its parameter is the weapon, here the attacker's equipped one.
/// `GetActionRef` is not documented for either, so it stays unset.
///
/// A separate system from [`obscript_object_system`] because the `HitEvent` markers
/// are produced late in the stage (projectiles and traps after the object system) and
/// stand until end-of-frame cleanup: this one runs beside `hit_event_system`, once
/// every producer has run.
pub fn obscript_hit_system(world: &World, dt: f32) {
    let hits: Vec<(EntityId, HitEvent)> = match world.query::<HitEvent>() {
        Some(events) => events.iter().map(|(victim, event)| (victim, *event)).collect(),
        None => return,
    };
    for (victim, hit) in hits {
        let attacker = form_of(world, hit.aggressor);
        let weapon = world
            .get::<byroredux_core::ecs::components::EquippedWeapon>(hit.aggressor)
            .map(|weapon| weapon.base_form_id)
            // A projectile's own weapon is not recorded on the event: only an equipped one is known.
            .filter(|_| hit.projectile == 0);
        run_event_blocks(
            world,
            victim,
            &[BLOCK_ON_HIT, BLOCK_ON_HIT_WITH],
            |block| match (block.block_type, block.filter) {
                (_, None) => true,
                (BLOCK_ON_HIT, Some(filter)) => attacker == Some(filter),
                (_, Some(filter)) => weapon == Some(filter),
            },
            None,
            dt,
        );
    }
}

/// Run the `block_types` blocks of the script of item `item`, held by `holder`, that
/// `wanted` accepts. The blocks of an item's script run when something happens to the
/// item (the event systems below), with the item and its holder on the host: a command
/// written with no caller has nothing to act on, `GetContainer` is the holder, `RemoveMe`
/// takes the item out of it and ends the block. Locals do not outlive a run — an item in
/// an inventory has no reference to keep them on.
fn run_item_blocks(world: &World, item: u32, holder: u32, block_types: &[u16], dt: f32) {
    let Some(script_id) = world.try_resource::<ObScriptItemScripts>().and_then(|items| items.0.get(&item).copied()) else {
        return;
    };
    let Some((script, blocks)) = world.try_resource::<ObScriptObjectTable>().and_then(|table| {
        let object = table.scripts.get(&script_id)?;
        let blocks: Vec<BlockInfo> = object
            .blocks
            .iter()
            .filter(|block| block_types.contains(&block.block_type) && !table.malformed.contains(&(script_id, **block)))
            // A block written for one container or actor (`Begin OnAdd player`).
            .filter(|block| block.filter.is_none_or(|filter| filter == holder))
            .copied()
            .collect();
        Some((Arc::clone(&object.record), blocks))
    }) else {
        return;
    };
    for block in blocks {
        let mut state = VmState::default();
        let mut host = ObScriptCommandHost::new(world, item, None, dt, &script);
        host.item = Some(crate::obscript_quests::ItemScriptContext { item, container: holder });
        if let BlockOutcome::Malformed(at) = ObScriptProgram::new(&script).run_at(&block, &mut state, &mut host) {
            log::warn!("ObScript item script {:08X}: malformed bytecode at {at:#06x} (block {:#04x})", script.form_id, block.block_type);
            if let Some(mut table) = world.try_resource_mut::<ObScriptObjectTable>() {
                table.malformed.insert((script.form_id, block));
            }
        }
    }
}

/// `OnAdd`, `OnDrop`, `OnEquip` and `OnUnequip` of an item's script, from the item and
/// equipment batches the frame's wearers carry.
///
/// The GECK / CS pages: `OnAdd` runs "once when the scripted object is added to the
/// container's inventory", `OnDrop` when it "is removed from the container's inventory"
/// (any removal, not only a drop on the ground), `OnEquip` / `OnUnequip` when "equipped /
/// unequipped by the specified actor"; a parameter names the container or actor, and none
/// means any. The batches are the engine's own transfer and equipment events, so a looted,
/// picked-up, scripted or equipped item reaches its script the frame it happens. Run in
/// `Late`, ahead of `event_cleanup_system`, so the batches of the whole frame are there.
pub fn obscript_item_system(world: &World, dt: f32) {
    if world.try_resource::<ObScriptItemScripts>().is_none() {
        return;
    }
    let transfers: Vec<(EntityId, Vec<crate::ItemTransfer>)> = world
        .query::<ItemEventBatch>()
        .map(|batches| batches.iter().map(|(wearer, batch)| (wearer, batch.0.clone())).collect())
        .unwrap_or_default();
    for (wearer, batch) in transfers {
        let Some(holder) = form_of(world, wearer) else { continue };
        for transfer in batch {
            let block = if transfer.added { BLOCK_ON_ADD } else { BLOCK_ON_DROP };
            run_item_blocks(world, transfer.item_form_id, holder, &[block], dt);
        }
    }
    let changes: Vec<(EntityId, Vec<crate::EquipmentChange>)> = world
        .query::<EquipmentEventBatch>()
        .map(|batches| batches.iter().map(|(wearer, batch)| (wearer, batch.0.clone())).collect())
        .unwrap_or_default();
    for (wearer, batch) in changes {
        let Some(holder) = form_of(world, wearer) else { continue };
        for change in batch {
            let block = if change.equipped { BLOCK_ON_EQUIP } else { BLOCK_ON_UNEQUIP };
            run_item_blocks(world, change.item_form_id, holder, &[block], dt);
        }
    }
}

/// `OnPackageStart`, `OnPackageDone` and `OnPackageChange`, from the package reports the
/// AI raised this frame. Called by [`crate::package_fragment_system`] with what it just
/// took off the queues, so the one consumer of those queues keeps its place.
///
/// The CS pages: *OnPackageStart* "will be run once when the scripted actor starts
/// running the specified package"; *OnPackageDone* (spelled `OnPackageEnd` too) "when the
/// scripted actor completes the specified package" — a package that cannot complete
/// (wander, sleep, eat) never raises it, as the AI reports only real completions;
/// *OnPackageChange* "when the scripted actor changes from the specified package to some
/// other package", not for a package that completes and is chosen again. A block with no
/// package written runs for every package.
pub(crate) fn deliver_package_events(
    world: &World,
    transitions: &[crate::PackageTransition],
    ends: &[crate::fragment::PackageEnd],
    dt: f32,
) {
    for end in ends {
        run_event_blocks(
            world,
            end.actor,
            &[BLOCK_ON_PACKAGE_DONE],
            |block| block.filter.is_none_or(|package| package == end.package),
            None,
            dt,
        );
    }
    for transition in transitions {
        if let Some(from) = transition.from {
            run_event_blocks(
                world,
                transition.actor,
                &[BLOCK_ON_PACKAGE_CHANGE],
                |block| block.filter.is_none_or(|package| package == from),
                None,
                dt,
            );
        }
        if let Some(to) = transition.to {
            run_event_blocks(
                world,
                transition.actor,
                &[BLOCK_ON_PACKAGE_START],
                |block| block.filter.is_none_or(|package| package == to),
                None,
                dt,
            );
        }
    }
}

/// `SayToDone`: the actor finished a line a script made it say. GECK: "triggered when a Say
/// or SayTo is complete", with an optional topic: a block written for one topic waits for it.
pub(crate) fn deliver_say_to_done(world: &World, speaker: EntityId, topic: u32, dt: f32) {
    run_event_blocks(world, speaker, &[BLOCK_SAY_TO_DONE], |block| block.filter.is_none_or(|filter| filter == topic), None, dt);
}

/// `OnStartCombat` and `OnCombatEnd`: an actor whose script has the block is told
/// when it enters a fight and when it drops out of one "for any reason" (GECK
/// *OnCombatEnd*). The engine raises no event for either, so the actor's combat
/// state ([`crate::is_in_combat`]) is compared with the [`ObScriptInCombat`] mark the
/// last transition left.
pub fn obscript_combat_system(world: &World, dt: f32) {
    let instances: Vec<(EntityId, u32)> = match world.query::<ObScriptInstance>() {
        Some(query) => query.iter().map(|(entity, instance)| (entity, instance.script)).collect(),
        None => return,
    };
    for (entity, script_id) in instances {
        let listens = world.try_resource::<ObScriptObjectTable>().is_some_and(|table| {
            table.scripts.get(&script_id).is_some_and(|object| {
                object.blocks.iter().any(|block| matches!(block.block_type, BLOCK_ON_START_COMBAT | BLOCK_ON_COMBAT_END))
            })
        });
        if !listens {
            continue;
        }
        let fighting = crate::is_in_combat(world, entity);
        let told = world.has::<ObScriptInCombat>(entity);
        match (fighting, told) {
            (true, false) => {
                if let Some(mut marks) = world.query_mut::<ObScriptInCombat>() {
                    marks.insert(entity, ObScriptInCombat);
                }
                run_event_blocks(world, entity, &[BLOCK_ON_START_COMBAT], |_| true, None, dt);
            }
            (false, true) => {
                if let Some(mut marks) = world.query_mut::<ObScriptInCombat>() {
                    marks.remove(entity);
                }
                run_event_blocks(world, entity, &[BLOCK_ON_COMBAT_END], |_| true, None, dt);
            }
            _ => {}
        }
    }
}

/// What one block run came to.
struct Ran {
    outcome: BlockOutcome,
    /// Commands the host declined during the run.
    declined: u32,
}

/// Run one block against `entity`, seeding the script's locals from its
/// [`ScriptVariables`] and writing back the ones it changed. A malformed block is
/// reported once and not run again. `action_ref` is the activator (or the one
/// crossing a trigger volume) while such a block runs.
fn run_block(world: &World, entity: EntityId, form: u32, script: &ScriptRecord, block: BlockInfo, dt: f32, action_ref: Option<u32>) -> Ran {
    let mut state = VmState { locals: seed_locals(world, entity, script), ref_locals: seed_refs(world, entity, script) };
    let before = state.locals.clone();
    let before_refs = state.ref_locals.clone();
    let mut host = ObScriptCommandHost::new(world, form, Some(entity), dt, script);
    host.action_ref = action_ref;
    let outcome = ObScriptProgram::new(script).run_at(&block, &mut state, &mut host);
    let declined = host.declined.get();
    store_locals(world, entity, script, &before, &state.locals);
    store_refs(world, entity, script, &before_refs, &state.ref_locals);
    if let BlockOutcome::Malformed(at) = outcome {
        log::warn!("ObScript object script {:08X}: malformed bytecode at {at:#06x} (block {:#04x})", script.form_id, block.block_type);
        if let Some(mut table) = world.try_resource_mut::<ObScriptObjectTable>() {
            table.malformed.insert((script.form_id, block));
        }
    }
    Ran { outcome, declined }
}

/// Whether `entity` runs a script that has a usable `OnActivate` block (one that
/// has not come back malformed): the reference whose default activation that
/// block replaces.
pub fn has_activate_block(world: &World, entity: EntityId) -> bool {
    let Some(script) = world.get::<ObScriptInstance>(entity).map(|instance| instance.script) else {
        return false;
    };
    world.try_resource::<ObScriptObjectTable>().is_some_and(|table| {
        table.scripts.get(&script).is_some_and(|object| {
            object.of_type(BLOCK_ON_ACTIVATE).any(|&block| !table.malformed.contains(&(script, block)))
        })
    })
}

/// A reference's FormID: the player is `0x14`.
fn form_of(world: &World, entity: EntityId) -> Option<u32> {
    if crate::papyrus_demo::PapyrusPlayerEntity::get(world) == Some(entity) {
        return Some(crate::PLAYER_REFERENCE);
    }
    entity_global_form_id(world, entity)
}

/// How deep activations may nest (`A.Activate B 1` from inside `B`'s block
/// activating `A` ...): the GECK runs the target's script "immediately", so
/// a cycle would not return.
const MAX_ACTIVATION_DEPTH: u8 = 4;

thread_local! {
    static ACTIVATION_DEPTH: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// Run `entity`'s `OnActivate` block for `activator` — the one named, or each one
/// written for this activator (or for nobody in particular) — and, when a run
/// cannot be trusted to have decided (it declined a command, or is malformed),
/// perform the default activation, which is what the reference did before scripts
/// ran. Returns whether a block ran. `dt` is the frame's, `0` for a nested run.
fn deliver_activation(world: &World, entity: EntityId, activator: EntityId, dt: f32, only: Option<BlockInfo>) -> bool {
    let Some(script_id) = world.get::<ObScriptInstance>(entity).map(|instance| instance.script) else {
        return false;
    };
    let Some((script, blocks)) = world.try_resource::<ObScriptObjectTable>().and_then(|table| {
        let object = table.scripts.get(&script_id)?;
        let blocks: Vec<BlockInfo> = object
            .of_type(BLOCK_ON_ACTIVATE)
            .copied()
            .filter(|block| !table.malformed.contains(&(script_id, *block)))
            .collect();
        Some((Arc::clone(&object.record), blocks))
    }) else {
        return false;
    };
    let (Some(form), Some(activator_form)) = (entity_global_form_id(world, entity), form_of(world, activator)) else {
        return false;
    };
    let blocks: Vec<BlockInfo> = match only {
        Some(only) => vec![only],
        None => blocks.into_iter().filter(|block| block.filter.is_none_or(|filter| filter == activator_form)).collect(),
    };
    if blocks.is_empty() || ACTIVATION_DEPTH.get() >= MAX_ACTIVATION_DEPTH {
        return false;
    }
    ACTIVATION_DEPTH.set(ACTIVATION_DEPTH.get() + 1);
    let (mut declined, mut malformed) = (0u32, false);
    for block in blocks {
        let ran = run_block(world, entity, form, &script, block, dt, Some(activator_form));
        declined += ran.declined;
        malformed |= matches!(ran.outcome, BlockOutcome::Malformed(_));
    }
    ACTIVATION_DEPTH.set(ACTIVATION_DEPTH.get() - 1);
    if declined > 0 || malformed {
        log::debug!("ObScript {form:08X} OnActivate: {declined} declined command(s): default activation goes ahead");
        crate::PendingFragmentActivations::queue_native(world, entity, activator);
    }
    true
}

/// `target.Activate <actor> 1`: run the target's `OnActivate` block now.
pub(crate) fn run_activate_block(world: &World, target: EntityId, activator: EntityId) -> bool {
    deliver_activation(world, target, activator, 0.0, None)
}

fn seed_locals(world: &World, entity: EntityId, script: &ScriptRecord) -> HashMap<u16, f32> {
    let Some(variables) = world.get::<ScriptVariables>(entity) else {
        return HashMap::new();
    };
    script
        .locals
        .iter()
        .filter_map(|local| {
            let slot = u16::try_from(local.index).ok()?;
            Some((slot, variables.get(ConditionStringId::from_text(&local.name))?))
        })
        .collect()
}

/// The name a reference variable's exact form id is kept under on the
/// reference's `ScriptVariables`: its low and high 16 bits, each exact as an
/// `f32`. The variable itself holds the nearest `f32`, as every local does.
fn ref_variable_halves(name: &str) -> (String, String) {
    (format!("{name}#lo"), format!("{name}#hi"))
}

/// The references the script's reference variables hold, from the reference's
/// `ScriptVariables`: the exact halves when stored, else the variable's number
/// (a value that predates them) when it is a plausible form id.
fn seed_refs(world: &World, entity: EntityId, script: &ScriptRecord) -> HashMap<u16, u32> {
    let Some(variables) = world.get::<ScriptVariables>(entity) else {
        return HashMap::new();
    };
    let mut refs = HashMap::new();
    for slot in crate::obscript_vm::ref_slots(script) {
        let Some(local) = script.locals.iter().find(|local| local.index == u32::from(slot)) else {
            continue;
        };
        let (low, high) = ref_variable_halves(&local.name);
        let halves = variables
            .get(ConditionStringId::from_text(&low))
            .zip(variables.get(ConditionStringId::from_text(&high)));
        let form = match halves {
            Some((low, high)) => crate::obscript_vm::form_from_halves(low, high),
            None => variables.get(ConditionStringId::from_text(&local.name)).filter(|n| *n >= 1.0).map_or(0, |n| n as u32),
        };
        if form != 0 {
            refs.insert(slot, form);
        }
    }
    refs
}

/// Write back the reference variables a run changed.
fn store_refs(world: &World, entity: EntityId, script: &ScriptRecord, before: &HashMap<u16, u32>, after: &HashMap<u16, u32>) {
    let changed: Vec<(String, u32)> = crate::obscript_vm::ref_slots(script)
        .filter(|slot| before.get(slot).copied().unwrap_or(0) != after.get(slot).copied().unwrap_or(0))
        .filter_map(|slot| {
            let local = script.locals.iter().find(|local| local.index == u32::from(slot))?;
            Some((local.name.clone(), after.get(&slot).copied().unwrap_or(0)))
        })
        .collect();
    if changed.is_empty() {
        return;
    }
    if let Some(mut variables) = world.query_mut::<ScriptVariables>() {
        if let Some(variables) = variables.get_mut(entity) {
            for (name, form) in changed {
                let (low_name, high_name) = ref_variable_halves(&name);
                let (low, high) = crate::obscript_vm::form_halves(form);
                variables.set_by_name(&low_name, low);
                variables.set_by_name(&high_name, high);
            }
        }
    }
}

fn store_locals(world: &World, entity: EntityId, script: &ScriptRecord, before: &HashMap<u16, f32>, after: &HashMap<u16, f32>) {
    let changed: Vec<(&str, f32)> = script
        .locals
        .iter()
        .filter_map(|local| {
            let slot = u16::try_from(local.index).ok()?;
            let value = *after.get(&slot)?;
            // Never-set locals read 0, so a 0 that was not stored is no change.
            (before.get(&slot).copied().unwrap_or(0.0) != value).then_some((local.name.as_str(), value))
        })
        .collect();
    if changed.is_empty() {
        return;
    }
    if let Some(mut variables) = world.query_mut::<ScriptVariables>() {
        if let Some(variables) = variables.get_mut(entity) {
            for (name, value) in changed {
                variables.set_by_name(name, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::FormIdComponent;
    use crate::obscript_vm::BLOCK_ON_ACTIVATE;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_plugin::esm::records::{ScriptLocalVar, ScriptRef};

    const REF: u32 = 0x0002_0001;
    const SCRIPT: u32 = 0x0003_0001;

    fn statement(op: u16, payload: &[u8]) -> Vec<u8> {
        let mut out = op.to_le_bytes().to_vec();
        out.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    /// `set counter to counter + 1` — a local `counter` (slot 1) incremented, the
    /// smallest body whose effect survives between runs.
    fn counting_body() -> Vec<u8> {
        // `' s1'  ' n1'  +` : reverse-polish `counter 1 +`.
        let mut expr = vec![b' ', b's', 1, 0];
        expr.extend_from_slice(&[b' ', b'n', 1, 0, 0, 0]);
        expr.extend_from_slice(b"+ ");
        let mut set = vec![b's', 1, 0];
        set.extend_from_slice(&(expr.len() as u16).to_le_bytes());
        set.extend_from_slice(&expr);
        statement(0x15, &set)
    }

    /// `Begin <block>` .. body .. `End`.
    fn block_with(block: u16, body: Vec<u8>) -> Vec<u8> {
        block_with_param(block, body, None)
    }

    /// `Begin <block> <ref>` .. body .. `End`: the parameter is the same
    /// `[u16 count]['r' idx]` blob a command's arguments use (`01 00 72 01 00`).
    fn block_with_param(block: u16, body: Vec<u8>, reference: Option<u16>) -> Vec<u8> {
        let mut begin = block.to_le_bytes().to_vec();
        begin.extend_from_slice(&0u32.to_le_bytes());
        if let Some(index) = reference {
            begin.extend_from_slice(&[1, 0, b'r']);
            begin.extend_from_slice(&index.to_le_bytes());
        }
        let mut bytes = statement(0x10, &begin);
        bytes.extend(body);
        bytes.extend(statement(0x11, &[]));
        bytes
    }

    fn script_with(blocks: &[(u16, Vec<u8>)]) -> ScriptRecord {
        let mut compiled = statement(0x1d, &[]);
        for (block, body) in blocks {
            compiled.extend(block_with(*block, body.clone()));
        }
        ScriptRecord {
            form_id: SCRIPT,
            editor_id: "TestObjectScript".into(),
            script_type: ScriptType::Object,
            compiled_size: compiled.len() as u32,
            compiled,
            locals: vec![ScriptLocalVar { index: 1, var_type: 2, name: "counter".into() }],
            refs: Vec::<ScriptRef>::new(),
            ..Default::default()
        }
    }

    fn script(blocks: &[u16]) -> ScriptRecord {
        script_with(&blocks.iter().map(|&block| (block, counting_body())).collect::<Vec<_>>())
    }

    fn world_with(blocks: &[u16]) -> (World, EntityId) {
        world_for(script(blocks))
    }

    fn world_for(script: ScriptRecord) -> (World, EntityId) {
        let mut world = World::new();
        crate::register(&mut world);
        let mut pool = FormIdPool::new();
        let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("FalloutNV.esm"), local: LocalFormId(REF) });
        world.insert_resource(pool);
        world.insert_resource(crate::PendingFragmentActivations::default());
        assert_eq!(install_object_scripts(&mut world, &HashMap::from([(SCRIPT, script)])), 1);
        let entity = world.spawn();
        world.insert(entity, FormIdComponent(id));
        assert!(attach_object_script(&mut world, entity, SCRIPT));
        (world, entity)
    }

    fn counter(world: &World, entity: EntityId) -> f32 {
        world.get::<ScriptVariables>(entity).and_then(|v| v.get_by_name("counter")).unwrap_or(0.0)
    }

    /// `GameMode` runs every frame and its local persists in the reference's
    /// `ScriptVariables`, by name.
    #[test]
    fn game_mode_runs_each_frame_and_keeps_its_locals_on_the_reference() {
        let (world, entity) = world_with(&[BLOCK_GAME_MODE]);
        for _ in 0..3 {
            obscript_object_system(&world, 0.016);
        }
        assert_eq!(counter(&world, entity), 3.0);
    }

    /// `OnLoad` runs only on the frame the cell-load marker is there, `OnDeath`
    /// only on a death marker; the marker is left for its own drain.
    #[test]
    fn event_blocks_run_only_on_their_event() {
        let (mut world, entity) = world_with(&[BLOCK_ON_LOAD, BLOCK_ON_DEATH]);
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, entity), 0.0, "no event, no GameMode block: nothing runs");
        world.insert(entity, OnCellLoadEvent);
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, entity), 1.0);
        world.insert(entity, OnDeathEvent { killer: None });
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, entity), 3.0, "OnLoad is still marked, and OnDeath joins it");
        assert!(world.has::<OnDeathEvent>(entity), "the death marker belongs to its own consumer");
    }

    /// A script the table does not hold attaches nothing; a script that cannot be
    /// walked is not installed.
    #[test]
    fn unknown_and_malformed_scripts_do_not_attach() {
        let (mut world, _) = world_with(&[BLOCK_GAME_MODE]);
        let other = world.spawn();
        assert!(!attach_object_script(&mut world, other, 0x00FF_FFFF));
        assert!(!world.has::<ObScriptInstance>(other));
        let mut broken = script(&[BLOCK_GAME_MODE]);
        broken.compiled.truncate(broken.compiled.len() - 3);
        let mut fresh = World::new();
        assert_eq!(install_object_scripts(&mut fresh, &HashMap::from([(SCRIPT, broken)])), 0);
        assert!(fresh.try_resource::<ObScriptObjectTable>().is_none());
    }

    /// Quest scripts and scripts with no bytecode are not object scripts.
    #[test]
    fn only_object_scripts_with_bytecode_are_installed() {
        let mut quest = script(&[BLOCK_GAME_MODE]);
        quest.script_type = ScriptType::Quest;
        let mut empty = script(&[]);
        empty.compiled.clear();
        let mut world = World::new();
        assert_eq!(install_object_scripts(&mut world, &HashMap::from([(1, quest), (2, empty)])), 0);
    }

    /// A reference with no `FormIdComponent` has no identity to run under: skipped.
    #[test]
    fn a_reference_without_a_form_id_is_skipped() {
        let (mut world, entity) = world_with(&[BLOCK_GAME_MODE]);
        world.remove::<FormIdComponent>(entity);
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, entity), 0.0);
    }

    /// `OnDeathEnableLinkedRef` from FalloutNV.esm, byte for byte: `ref linkedRef`,
    /// `Begin OnDeath`, `set linkedRef to getLinkedRef`, `linkedRef.enable`. The
    /// reference variable is slot 2 and its `SCRV` entry makes it a reference, so the
    /// command written on it acts on what `GetLinkedRef` answered.
    #[test]
    fn a_reference_variable_holds_a_reference_and_is_the_caller_of_a_later_command() {
        const LINKED: u32 = 0x0100_0007;
        let compiled: Vec<u8> = [
            &[0x1d, 0, 0, 0][..],
            &[0x10, 0, 0x08, 0, 0x0a, 0, 0x1d, 0, 0, 0, 0, 0],
            &[0x15, 0, 0x0b, 0, 0x66, 2, 0, 0x06, 0, 0x20, 0x58, 0x6b, 0x11, 0, 0],
            &[0x1c, 0, 0x01, 0, 0x21, 0x10, 0x02, 0, 0, 0],
            &[0x11, 0, 0, 0],
        ]
        .concat();
        let record = ScriptRecord {
            form_id: SCRIPT,
            editor_id: "OnDeathEnableLinkedRef".into(),
            script_type: ScriptType::Object,
            compiled_size: compiled.len() as u32,
            compiled,
            locals: vec![ScriptLocalVar { index: 2, var_type: 0, name: "linkedRef".into() }],
            refs: vec![ScriptRef::Variable(2)],
            ..Default::default()
        };
        let (mut world, entity) = world_for(record);
        world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Fallout3NV));
        world.insert_resource(crate::ReferenceEnableState::default());
        let mut links = crate::LinkedRefOverrides::default();
        links.set(REF, 0, LINKED);
        world.insert_resource(links);
        world.try_resource_mut::<crate::ReferenceEnableState>().unwrap().set_enabled(LINKED, false);
        assert!(!crate::reference_enabled(&world, LINKED));
        world.insert(entity, OnDeathEvent { killer: None });
        obscript_object_system(&world, 0.016);
        assert!(crate::reference_enabled(&world, LINKED), "linkedRef.enable ran on the linked reference");
        // The variable keeps the exact form (0x01000007 is past 2^24, which an f32 would round) on the reference.
        let variables = world.get::<ScriptVariables>(entity).unwrap();
        assert_eq!(variables.get_by_name("linkedRef#lo"), Some(7.0));
        assert_eq!(variables.get_by_name("linkedRef#hi"), Some(256.0));
        drop(variables);
        let stored = Arc::clone(&world.try_resource::<ObScriptObjectTable>().unwrap().scripts[&SCRIPT].record);
        let refs = seed_refs(&world, entity, &stored);
        assert_eq!(refs.get(&2), Some(&LINKED));
    }

    const ACTIVATOR: u32 = 0x0002_0002;

    /// A world with the scripted reference plus an activator entity.
    fn world_with_activator(script: ScriptRecord) -> (World, EntityId, EntityId) {
        let (mut world, entity) = world_for(script);
        let mut pool = world.resource_mut::<FormIdPool>();
        let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("FalloutNV.esm"), local: LocalFormId(ACTIVATOR) });
        drop(pool);
        let activator = world.spawn();
        world.insert(activator, FormIdComponent(id));
        (world, entity, activator)
    }

    fn native_queue(world: &World) -> Vec<(EntityId, EntityId)> {
        world.resource::<crate::PendingFragmentActivations>().1.clone()
    }

    /// `Activate` (no arguments) inside the block performs the default activation
    /// with the block's own activator; the block itself withholds it until then.
    #[test]
    fn on_activate_replaces_default_processing_until_the_script_activates() {
        let body = [counting_body(), statement(0x100d, &[0, 0])].concat();
        let (mut world, entity, activator) = world_with_activator(script_with(&[(BLOCK_ON_ACTIVATE, body)]));
        assert!(has_activate_block(&world, entity));
        assert!(crate::default_activation_blocked(&world, entity), "the block replaces native processing");

        world.insert_resource(crate::obscript_quests::ObScriptEffectLog::default());
        world.insert_resource(crate::obscript_quests::ObScriptQuestTimers::default());
        world.insert(entity, ActivateEvent { activator });
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, entity), 1.0, "the block ran");
        assert_eq!(native_queue(&world), vec![(entity, activator)], "its Activate asked for the default");
        // The queue entry is the script's own `Activate`, not the declined-command
        // fallback: nothing was declined.
        assert!(world.resource::<crate::obscript_quests::ObScriptQuestTimers>().unknown_commands.is_empty());
        assert_eq!(
            world.resource::<crate::obscript_quests::ObScriptEffectLog>().entries.back().map(|(_, text)| text.as_str()),
            Some("Activate 00020001 by 00020002"),
        );
    }

    /// A block the host ran in full that never calls `Activate` leaves the
    /// reference un-activated: the scripted lock.
    #[test]
    fn a_fully_modelled_block_without_activate_withholds_default_processing() {
        let (mut world, entity, activator) = world_with_activator(script_with(&[(BLOCK_ON_ACTIVATE, counting_body())]));
        world.insert(entity, ActivateEvent { activator });
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, entity), 1.0);
        assert!(native_queue(&world).is_empty());
    }

    /// A block that declined a command cannot be trusted to have decided, so the
    /// default activation goes ahead instead of leaving the reference stuck.
    #[test]
    fn a_block_that_declined_a_command_falls_back_to_default_processing() {
        let body = [counting_body(), statement(0x2fff, &[0, 0])].concat();
        let (mut world, entity, activator) = world_with_activator(script_with(&[(BLOCK_ON_ACTIVATE, body)]));
        world.insert(entity, ActivateEvent { activator });
        obscript_object_system(&world, 0.016);
        assert_eq!(native_queue(&world), vec![(entity, activator)]);
    }

    /// The default activation a script asked for is not an `OnActivate` event: the
    /// block does not run for it, and the native consumers are no longer blocked.
    #[test]
    fn the_native_activation_a_script_asked_for_does_not_rerun_the_block() {
        let (mut world, entity, activator) = world_with_activator(script_with(&[(BLOCK_ON_ACTIVATE, counting_body())]));
        world.insert(entity, ActivateEvent { activator });
        world.insert(entity, NativeActivation);
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, entity), 0.0);
        assert!(!crate::default_activation_blocked(&world, entity), "default processing goes ahead");
    }

    /// `target.Activate <actor> 1` runs the target's block now, as `GetActionRef`
    /// for that actor; a target with no block runs nothing.
    #[test]
    fn activate_with_the_run_block_flag_runs_the_targets_block_immediately() {
        let (world, entity, activator) = world_with_activator(script_with(&[(BLOCK_ON_ACTIVATE, counting_body())]));
        assert!(run_activate_block(&world, entity, activator));
        assert_eq!(counter(&world, entity), 1.0);
        let (world, entity, activator) = world_with_activator(script(&[BLOCK_GAME_MODE]));
        assert!(!run_activate_block(&world, entity, activator), "no OnActivate block");
        assert_eq!(counter(&world, entity), 0.0);
    }

    /// A script with one block, optionally written for the reference at SCRO 1.
    fn trigger_script(block: u16, written_for: Option<u32>) -> ScriptRecord {
        let mut record = script_with(&[]);
        record.compiled.extend(block_with_param(block, counting_body(), written_for.map(|_| 1)));
        record.compiled_size = record.compiled.len() as u32;
        record.refs = written_for.into_iter().map(ScriptRef::Object).collect();
        record
    }

    fn entering(world: &mut World, trigger: EntityId, who: &[EntityId]) {
        world.insert(trigger, OnTriggerEnterEvent { triggerers: who.to_vec() });
    }

    /// `OnTriggerEnter` runs once per reference that entered, and the begin's
    /// parameter is read back from the bytes.
    #[test]
    fn on_trigger_enter_runs_once_per_entering_reference() {
        let (mut world, trigger, a) = world_with_activator(trigger_script(BLOCK_ON_TRIGGER_ENTER, None));
        let b = world.spawn();
        entering(&mut world, trigger, &[a, b]);
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, trigger), 2.0);
    }

    /// `Begin OnTriggerEnter <ref>` runs for that reference only.
    #[test]
    fn a_trigger_block_written_for_a_reference_ignores_the_others() {
        let script = trigger_script(BLOCK_ON_TRIGGER_ENTER, Some(ACTIVATOR));
        assert_eq!(ObScriptProgram::new(&script).blocks().unwrap()[0].filter, Some(ACTIVATOR));
        assert_eq!(ObScriptProgram::new(&script_with(&[(BLOCK_ON_TRIGGER_ENTER, counting_body())])).blocks().unwrap()[0].filter, None);
        let (mut world, trigger, a) = world_with_activator(script);
        let stranger = world.spawn();
        let mut pool = world.resource_mut::<FormIdPool>();
        let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("FalloutNV.esm"), local: LocalFormId(0x0002_0003) });
        drop(pool);
        world.insert(stranger, FormIdComponent(id));
        entering(&mut world, trigger, &[stranger, a]);
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, trigger), 1.0, "only the written-for reference ran it");
    }

    /// FO3 scripts carry several blocks of one type, each written for a
    /// different reference (`Begin OnTriggerEnter player` / `... StickyRef`): each
    /// is its own block, and a reference runs only the ones written for it (or for
    /// nobody).
    #[test]
    fn several_blocks_of_one_type_each_run_for_their_own_reference() {
        const STRANGER: u32 = 0x0002_0003;
        let mut record = script_with(&[]);
        record.compiled.extend(block_with_param(BLOCK_ON_TRIGGER_ENTER, counting_body(), Some(1)));
        record.compiled.extend(block_with_param(BLOCK_ON_TRIGGER_ENTER, counting_body(), Some(2)));
        record.compiled.extend(block_with(BLOCK_ON_TRIGGER_ENTER, counting_body()));
        record.refs = vec![ScriptRef::Object(ACTIVATOR), ScriptRef::Object(STRANGER)];
        let blocks = ObScriptProgram::new(&record).blocks().unwrap();
        assert_eq!(blocks.iter().map(|b| b.filter).collect::<Vec<_>>(), vec![Some(ACTIVATOR), Some(STRANGER), None]);
        let (mut world, trigger, activator) = world_with_activator(record);
        entering(&mut world, trigger, &[activator]);
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, trigger), 2.0, "the one for the activator and the unwritten one; not the stranger's");
    }

    fn hit(world: &mut World, victim: EntityId, aggressor: EntityId) {
        world.register::<crate::HitEvent>();
        world.insert(
            victim,
            crate::HitEvent {
                aggressor,
                source: aggressor,
                projectile: 0,
                damage: 5.0,
                power_attack: false,
                sneak_attack: false,
                bash_attack: false,
                blocked: false,
            },
        );
    }

    /// CS *OnHit*: "run once when the scripted actor is hit by the specified ActorID.
    /// If no parameter is used, the block will run when the scripted actor is hit by
    /// anyone": a block written for one attacker waits for that one.
    #[test]
    fn on_hit_runs_for_the_struck_actor_and_a_written_attacker_filters_it() {
        const STRANGER: u32 = 0x0002_0003;
        let mut record = script_with(&[]);
        record.compiled.extend(block_with_param(BLOCK_ON_HIT, counting_body(), Some(1)));
        record.compiled.extend(block_with_param(BLOCK_ON_HIT, counting_body(), Some(2)));
        record.compiled.extend(block_with(BLOCK_ON_HIT, counting_body()));
        record.refs = vec![ScriptRef::Object(ACTIVATOR), ScriptRef::Object(STRANGER)];
        let (mut world, victim, attacker) = world_with_activator(record);
        obscript_hit_system(&world, 0.016);
        assert_eq!(counter(&world, victim), 0.0, "no hit, no run");
        hit(&mut world, victim, attacker);
        obscript_hit_system(&world, 0.016);
        assert_eq!(counter(&world, victim), 2.0, "the block written for this attacker and the unwritten one, not the stranger's");
    }

    /// `OnHitWith` runs for the attacker's equipped weapon, `GetActionRef` stays unset.
    #[test]
    fn on_hit_with_waits_for_the_attackers_equipped_weapon() {
        use byroredux_core::ecs::components::{EquippedWeapon, InventoryIndex};
        const SWORD: u32 = 0x0004_0001;
        let mut record = script_with(&[]);
        record.compiled.extend(block_with_param(BLOCK_ON_HIT_WITH, counting_body(), Some(1)));
        record.refs = vec![ScriptRef::Object(SWORD)];
        let (mut world, victim, attacker) = world_with_activator(record);
        world.register::<EquippedWeapon>();
        hit(&mut world, victim, attacker);
        obscript_hit_system(&world, 0.016);
        assert_eq!(counter(&world, victim), 0.0, "an unarmed attacker is not hitting with the sword");
        world.insert(attacker, EquippedWeapon { inventory_index: InventoryIndex(0), base_form_id: SWORD, damage: 5.0, reach: 0.0, speed: 0.0 });
        obscript_hit_system(&world, 0.016);
        assert_eq!(counter(&world, victim), 1.0);
    }

    /// `OnStartCombat` once when the actor enters a fight, `OnCombatEnd` once when it
    /// leaves it ("drops out of combat for any reason").
    #[test]
    fn combat_transitions_run_their_blocks_once_each() {
        let (mut world, actor, enemy) = world_with_activator(script_with(&[(BLOCK_ON_START_COMBAT, counting_body()), (BLOCK_ON_COMBAT_END, counting_body())]));
        obscript_combat_system(&world, 0.016);
        assert_eq!(counter(&world, actor), 0.0, "not fighting");
        world.insert(actor, crate::AiCombatState { target: enemy, attack_cooldown_remaining: 0.0 });
        obscript_combat_system(&world, 0.016);
        obscript_combat_system(&world, 0.016);
        assert_eq!(counter(&world, actor), 1.0, "OnStartCombat ran once for the fight");
        world.query_mut::<crate::AiCombatState>().unwrap().remove(actor);
        obscript_combat_system(&world, 0.016);
        obscript_combat_system(&world, 0.016);
        assert_eq!(counter(&world, actor), 2.0, "OnCombatEnd ran once when it ended");
    }

    /// CS *OnPackageStart* / *OnPackageDone* / *OnPackageChange*: Start for the package
    /// the actor begins, Change for the one it leaves, Done for the one it completes; a
    /// block written for another package stays quiet.
    #[test]
    fn package_blocks_run_for_the_package_they_name() {
        const WALK: u32 = 0x0005_0001;
        const SLEEP: u32 = 0x0005_0002;
        let mut record = script_with(&[]);
        for block in [BLOCK_ON_PACKAGE_START, BLOCK_ON_PACKAGE_CHANGE, BLOCK_ON_PACKAGE_DONE] {
            record.compiled.extend(block_with_param(block, counting_body(), Some(1)));
            record.compiled.extend(block_with_param(block, counting_body(), Some(2)));
        }
        record.refs = vec![ScriptRef::Object(WALK), ScriptRef::Object(SLEEP)];
        let (world, actor, _) = world_with_activator(record);
        let to_walk = crate::PackageTransition { actor, from: Some(SLEEP), to: Some(WALK) };
        deliver_package_events(&world, &[to_walk], &[], 0.016);
        assert_eq!(counter(&world, actor), 2.0, "Change for Sleep (left) and Start for Walk (begun)");
        deliver_package_events(&world, &[], &[crate::fragment::PackageEnd { actor, package: WALK }], 0.016);
        assert_eq!(counter(&world, actor), 3.0, "Done for Walk only");
    }

    fn item_world(blocks: &[(u16, Vec<u8>)]) -> (World, EntityId) {
        use byroredux_core::ecs::components::{Inventory, ItemStack};
        let (mut world, holder) = world_for(script_with(blocks));
        world.insert_resource(ObScriptItemScripts([(ITEM, SCRIPT)].into()));
        world.insert_resource(crate::obscript_quests::ObScriptQuestTimers::default());
        world.register::<Inventory>();
        world.register::<ItemEventBatch>();
        world.register::<EquipmentEventBatch>();
        let mut inventory = Inventory::new();
        inventory.add(ItemStack::new(ITEM, 3));
        world.insert(holder, inventory);
        (world, holder)
    }

    const ITEM: u32 = 0x0007_0001;

    fn stack(world: &World, holder: EntityId) -> u32 {
        world.get::<byroredux_core::ecs::components::Inventory>(holder).map_or(0, |inv| inv.items.iter().filter(|s| s.base_form_id == ITEM).map(|s| s.count).sum())
    }

    fn declined_of(world: &World, owner: u32, cmd: u16) -> u32 {
        world.try_resource::<crate::obscript_quests::ObScriptQuestTimers>().unwrap().unknown_commands.get(&owner).and_then(|m| m.get(&cmd)).copied().unwrap_or(0)
    }

    /// A reference's own variables are remote-readable by ordinal through the name of its script's
    /// local at that ordinal: `Q.bRunTimer` on an object script reads and writes the same saved
    /// variable the reference's own `GameMode` sees.
    #[test]
    fn a_remote_variable_reads_and_writes_a_references_object_script_locals() {
        use crate::obscript_vm::{CMD_GET_REMOTE_VARIABLE, CMD_SET_REMOTE_VARIABLE};
        use crate::obscript_vm::{ObScriptHost, ObScriptValue};
        let (mut world, target, other) = world_with_activator(script_with(&[(BLOCK_GAME_MODE, counting_body())]));
        // The caller is a different reference: `target`'s script is the one with `counter` at slot 1.
        world.insert_resource(crate::obscript_quests::ObScriptQuestTimers::default());
        let script = ScriptRecord::default();
        let mut host = ObScriptCommandHost::new(&world, ACTIVATOR, Some(other), 0.0, &script);
        let target_ref = ObScriptValue::Ref(REF);
        let counter_slot = ObScriptValue::Num(1.0);
        assert_eq!(host.call(CMD_GET_REMOTE_VARIABLE, &[target_ref, counter_slot], None), Some(ObScriptValue::Num(0.0)));
        host.call(CMD_SET_REMOTE_VARIABLE, &[target_ref, counter_slot, ObScriptValue::Num(7.0)], None);
        assert_eq!(counter(&world, target), 7.0, "the variable the reference's own script reads");
        assert_eq!(host.call(CMD_GET_REMOTE_VARIABLE, &[target_ref, counter_slot], None), Some(ObScriptValue::Num(7.0)));
        // A slot the script does not have is declined.
        host.call(CMD_GET_REMOTE_VARIABLE, &[target_ref, ObScriptValue::Num(9.0)], None);
        assert_eq!(host.declined.get(), 1);
        // The script's own reference is not read this way: its variables are mid-run in the VM.
        let mut own = ObScriptCommandHost::new(&world, REF, Some(target), 0.0, &script);
        own.call(CMD_GET_REMOTE_VARIABLE, &[target_ref, counter_slot], None);
        assert_eq!(own.declined.get(), 1);
    }

    /// `Say` makes the actor speak one line of the topic and answers how long it takes; when
    /// that has passed the actor's `SayToDone` blocks run, each for the topic it names or any.
    #[test]
    fn a_line_an_actor_is_made_to_say_ends_with_its_say_to_done_block() {
        use byroredux_plugin::esm::records::misc::dialogue::{DialRecord, InfoRecord};
        use crate::obscript_speech::{obscript_speech_system, say, PendingScriptedLines};
        const TOPIC: u32 = 0x0008_0001;
        const OTHER_TOPIC: u32 = 0x0008_0002;
        let mut record = script_with(&[]);
        record.compiled.extend(block_with_param(BLOCK_SAY_TO_DONE, counting_body(), Some(1)));
        record.compiled.extend(block_with_param(BLOCK_SAY_TO_DONE, counting_body(), Some(2)));
        record.compiled.extend(block_with(BLOCK_SAY_TO_DONE, counting_body()));
        record.refs = vec![ScriptRef::Object(TOPIC), ScriptRef::Object(OTHER_TOPIC)];
        let (mut world, speaker) = world_for(record);
        let player = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        let info = InfoRecord { form_id: 0x401, response_text: "Welcome, traveller, to my humble home.".into(), ..Default::default() };
        crate::install_dialogue_records(&mut world, vec![DialRecord { form_id: TOPIC, infos: vec![info], ..Default::default() }]);

        let seconds = say(&world, speaker, TOPIC, None).expect("the topic has a line to say");
        assert!(seconds > 0.5, "a spoken line takes time: {seconds}");
        assert_eq!(world.try_resource::<PendingScriptedLines>().unwrap().pending.len(), 1, "waiting for the voice player");
        obscript_speech_system(&world, seconds - 0.1);
        assert_eq!(counter(&world, speaker), 0.0, "still speaking");
        obscript_speech_system(&world, 0.2);
        assert_eq!(counter(&world, speaker), 2.0, "the block for this topic and the unwritten one, not the other topic's");
        obscript_speech_system(&world, 5.0);
        assert_eq!(counter(&world, speaker), 2.0, "once");
        // A topic that is not there says nothing, and nothing is pending.
        assert_eq!(say(&world, speaker, 0x00FF_0001, None), None);
    }

    /// An item's `OnAdd` block runs the frame the item is added, `RemoveMe` takes one from the
    /// holder and ends the block ("acts as a return"): the statement after it never runs.
    #[test]
    fn an_items_on_add_block_runs_and_remove_me_takes_it_out_and_returns() {
        // A command with no caller has nothing to act on in an item's script: `Disable` is
        // declined, which is how the test sees whether the statement ran.
        let probe = statement(0x1022, &[0, 0]);
        let (mut world, holder) = item_world(&[(BLOCK_ON_ADD, [statement(0x10a5, &[0, 0]), probe.clone()].concat())]);
        world.insert(holder, ItemEventBatch(vec![crate::ItemTransfer { item_form_id: ITEM, count: 1, added: true, stolen: false }]));
        obscript_item_system(&world, 0.016);
        assert_eq!(stack(&world, holder), 2, "RemoveMe removed one");
        assert_eq!(declined_of(&world, ITEM, 0x1022), 0, "and returned before the next statement");
        // Without RemoveMe the next statement runs (and is declined: no caller).
        let (mut world, holder) = item_world(&[(BLOCK_ON_ADD, probe)]);
        world.insert(holder, ItemEventBatch(vec![crate::ItemTransfer { item_form_id: ITEM, count: 1, added: true, stolen: false }]));
        obscript_item_system(&world, 0.016);
        assert_eq!((stack(&world, holder), declined_of(&world, ITEM, 0x1022)), (3, 1));
    }

    /// `OnDrop` runs for a removal, `OnEquip` / `OnUnequip` for the equipment batch; a transfer in
    /// the other direction does not run the block.
    #[test]
    fn item_blocks_follow_the_item_and_equipment_batches() {
        let probe = |cmd: u16| statement(cmd, &[0, 0]);
        let (mut world, holder) = item_world(&[(BLOCK_ON_ADD, probe(0x1021)), (BLOCK_ON_DROP, probe(0x1022)), (BLOCK_ON_EQUIP, probe(0x1023)), (BLOCK_ON_UNEQUIP, probe(0x1024))]);
        world.insert(holder, ItemEventBatch(vec![crate::ItemTransfer { item_form_id: ITEM, count: 1, added: false, stolen: false }]));
        world.insert(holder, EquipmentEventBatch(vec![crate::EquipmentChange { item_form_id: ITEM, equipped: true }]));
        obscript_item_system(&world, 0.016);
        let ran = |cmd| declined_of(&world, ITEM, cmd);
        assert_eq!((ran(0x1021), ran(0x1022), ran(0x1023), ran(0x1024)), (0, 1, 1, 0), "OnDrop and OnEquip only");
    }

    /// `0x1b` is `OnTriggerLeave` on Fallout 3 / New Vegas and `OnTriggerMob` on
    /// Oblivion (or with no game known): only the former is run on a leave.
    #[test]
    fn the_leave_block_runs_on_the_fallout_pair_only() {
        for (game, runs) in [
            (None, 0.0),
            (Some(byroredux_plugin::esm::reader::GameKind::Oblivion), 0.0),
            (Some(byroredux_plugin::esm::reader::GameKind::Fallout3NV), 1.0),
        ] {
            let (mut world, trigger, a) = world_with_activator(trigger_script(BLOCK_ON_TRIGGER_LEAVE, None));
            if let Some(game) = game {
                world.insert_resource(crate::ScriptGame(game));
            }
            world.insert(trigger, OnTriggerLeaveEvent { triggerers: vec![a] });
            obscript_object_system(&world, 0.016);
            assert_eq!(counter(&world, trigger), runs, "{game:?}");
        }
    }

    /// `OnTrigger` runs every frame while something is inside, and stops when it
    /// leaves; written for a reference, it waits for that one.
    #[test]
    fn on_trigger_runs_every_frame_while_an_occupant_is_inside() {
        use crate::trigger::{TriggerShape, TriggerVolume};
        let volume = |inside| TriggerVolume {
            center: byroredux_core::math::Vec3::ZERO,
            half_extents: byroredux_core::math::Vec3::ONE,
            rotation: byroredux_core::math::Quat::IDENTITY,
            shape: TriggerShape::Box,
            occupant_inside: Some(inside),
        };
        let (mut world, trigger, player) = world_with_activator(trigger_script(BLOCK_ON_TRIGGER, None));
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        world.insert(trigger, volume(false));
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, trigger), 0.0, "nobody inside");
        world.insert(trigger, volume(true));
        for _ in 0..3 {
            obscript_object_system(&world, 0.016);
        }
        assert_eq!(counter(&world, trigger), 3.0);
        world.insert(trigger, volume(false));
        obscript_object_system(&world, 0.016);
        assert_eq!(counter(&world, trigger), 3.0);

        // Written for the player (`Begin OnTrigger player`): runs, being the occupant;
        // written for someone else it does not.
        for (written_for, expected) in [(crate::PLAYER_REFERENCE, 1.0), (0x00AA_BBCC, 0.0)] {
            let (mut world, trigger, player) = world_with_activator(trigger_script(BLOCK_ON_TRIGGER, Some(written_for)));
            world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
            world.insert(trigger, volume(true));
            obscript_object_system(&world, 0.016);
            assert_eq!(counter(&world, trigger), expected, "written for {written_for:08X}");
        }
    }

    /// Every object script of the three legacy masters walks cleanly, and the
    /// four blocks this runtime runs decode to their `End` with every command
    /// answering 0 and with every command answering 1 (both arms of each gate).
    #[test]
    #[ignore = "needs Oblivion.esm, Fallout3.esm and FalloutNV.esm on disk"]
    fn legacy_object_scripts_walk_and_run_without_malformed_bytecode() {
        use crate::obscript_vm::{ObScriptHost, ObScriptValue};
        /// Answers every command with a fixed value and notes which ids it was asked.
        struct Fixed(f32, std::collections::BTreeSet<u16>);
        impl ObScriptHost for Fixed {
            fn call(&mut self, cmd: u16, _: &[ObScriptValue], _: Option<u32>) -> Option<ObScriptValue> {
                self.1.insert(cmd);
                Some(ObScriptValue::Num(self.0))
            }
        }
        use byroredux_plugin::esm::test_paths;
        for (name, path) in [
            ("Oblivion.esm", test_paths::oblivion_esm()),
            ("Fallout3.esm", test_paths::fo3_esm()),
            ("FalloutNV.esm", test_paths::fnv_esm()),
        ] {
            if !path.is_file() {
                if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
                    panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but {name} was not found at {path:?}");
                }
                continue;
            }
            let index = byroredux_plugin::esm::parse_esm(&std::fs::read(&path).unwrap()).unwrap();
            let (mut scripts, mut unwalkable, mut malformed) = (0usize, Vec::new(), Vec::new());
            // OnActivate blocks with every gate true: do they ever reach `Activate`?
            let (mut activate_blocks, mut calls_activate) = (0usize, 0usize);
            let (mut trigger_begins, mut filter_mismatches) = (0usize, Vec::new());
            for script in index.scripts.values().filter(|s| s.script_type == ScriptType::Object && !s.compiled.is_empty()) {
                scripts += 1;
                let program = ObScriptProgram::new(script);
                if program.block_types().is_none() {
                    unwalkable.push(script.form_id);
                    continue;
                }
                // `Begin OnTrigger* <ref>`: the parameter in the source is the filter in
                // the bytes, block for block (a script can have several of a type).
                if let Some(source) = &script.source {
                    let written: Vec<(u16, bool)> = source
                        .lines()
                        .filter_map(|line| {
                            let mut words = line.split(';').next().unwrap_or("").split_whitespace().map(str::to_ascii_lowercase);
                            let (begin, name) = (words.next()?, words.next()?);
                            let block = match (begin.as_str(), name.as_str()) {
                                ("begin", "ontrigger") => BLOCK_ON_TRIGGER,
                                ("begin", "ontriggerenter" | "ontriggeractor") => BLOCK_ON_TRIGGER_ENTER,
                                ("begin", "ontriggerleave" | "ontriggermob") => BLOCK_ON_TRIGGER_LEAVE,
                                _ => return None,
                            };
                            Some((block, words.next().is_some()))
                        })
                        .collect();
                    let compiled: Vec<(u16, bool)> = program
                        .blocks()
                        .unwrap_or_default()
                        .iter()
                        .filter(|b| [BLOCK_ON_TRIGGER, BLOCK_ON_TRIGGER_ENTER, BLOCK_ON_TRIGGER_LEAVE].contains(&b.block_type))
                        .map(|b| (b.block_type, b.filter.is_some()))
                        .collect();
                    trigger_begins += written.len();
                    if written != compiled {
                        filter_mismatches.push((script.form_id, written, compiled));
                    }
                }
                for block in [BLOCK_GAME_MODE, BLOCK_ON_LOAD, BLOCK_ON_DEATH, BLOCK_ON_ACTIVATE, BLOCK_ON_TRIGGER, BLOCK_ON_TRIGGER_ENTER, BLOCK_ON_TRIGGER_LEAVE] {
                    for answer in [0.0f32, 1.0] {
                        let mut state = VmState::default();
                        let mut host = Fixed(answer, Default::default());
                        if let BlockOutcome::Malformed(at) = program.run_block(block, &mut state, &mut host) {
                            malformed.push((script.form_id, block, at));
                        }
                        if block == BLOCK_ON_ACTIVATE && answer == 1.0 && program.block_types().is_some_and(|b| b.contains(&block)) {
                            activate_blocks += 1;
                            calls_activate += usize::from(host.1.contains(&0x100d));
                        }
                    }
                }
            }
            eprintln!("{name}: {scripts} object scripts; {activate_blocks} OnActivate blocks, {calls_activate} reach Activate with every gate true");
            assert!(scripts > 1000, "{name}: only {scripts} object scripts");
            assert!(unwalkable.is_empty(), "{name}: unwalkable object scripts {unwalkable:x?}");
            assert!(malformed.is_empty(), "{name}: malformed bytecode in {malformed:x?}");
            eprintln!("{name}: {trigger_begins} OnTrigger* begins, {} whose parameter disagrees with the bytes", filter_mismatches.len());
            assert!(filter_mismatches.is_empty(), "{name}: trigger parameter mismatches {filter_mismatches:?}");
        }
    }
}
