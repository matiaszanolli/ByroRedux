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
//! Four blocks run, on the events the engine already raises:
//!
//! | block | runs |
//! |-------|------|
//! | `GameMode` | every frame, for every loaded scripted reference |
//! | `OnLoad` | the frame the reference's cell loads ([`OnCellLoadEvent`]) |
//! | `OnDeath` | the frame an actor dies ([`OnDeathEvent`]) |
//! | `OnActivate` | the frame the reference is activated ([`ActivateEvent`]), with `GetActionRef` the activator |
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
use byroredux_plugin::esm::records::{ScriptRecord, ScriptType};

use crate::condition::entity_global_form_id;
use crate::events::{ActivateEvent, NativeActivation, OnCellLoadEvent, OnDeathEvent};
use crate::obscript_quests::ObScriptCommandHost;
use crate::obscript_vm::{
    BlockOutcome, ObScriptProgram, VmState, BLOCK_GAME_MODE, BLOCK_ON_ACTIVATE, BLOCK_ON_DEATH,
    BLOCK_ON_LOAD,
};
use crate::vm_state::ScriptVariables;
use byroredux_plugin::esm::records::condition::ConditionStringId;

/// One object script, with the blocks it can be asked to run.
pub struct ObjectScript {
    pub record: Arc<ScriptRecord>,
    pub blocks: Vec<u16>,
}

/// The load order's object scripts by SCPT FormID. `NOT_SAVED_BY_DESIGN`: rebuilt
/// from the records at every content load.
#[derive(Default)]
pub struct ObScriptObjectTable {
    pub scripts: HashMap<u32, ObjectScript>,
    /// `(script, block)` pairs that came back malformed: reported once, then
    /// skipped — a `GameMode` block would otherwise log every frame.
    malformed: HashSet<(u32, u16)>,
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
        let Some(blocks) = ObScriptProgram::new(script).block_types() else {
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

/// A reference to run blocks of this frame.
struct Due {
    entity: EntityId,
    form: u32,
    script: Arc<ScriptRecord>,
    blocks: Vec<u16>,
    /// Who activated the reference this frame, when it was activated.
    activator: Option<EntityId>,
}

/// Run this frame's object-script blocks. Exclusive: scripts write the quest,
/// enable and package state the other Update systems read, and the lock order
/// is the quest tick's (nothing is held across a run).
pub fn obscript_object_system(world: &World, dt: f32) {
    for Due { entity, form, script, blocks, activator } in collect_due(world) {
        for block in blocks {
            if block == BLOCK_ON_ACTIVATE {
                if let Some(activator) = activator {
                    deliver_activation(world, entity, activator, dt);
                }
            } else {
                run_block(world, entity, form, &script, block, dt, None);
            }
        }
    }
}

/// The references with something to run, and which blocks (`OnLoad`, `OnDeath`,
/// then `GameMode`), snapshotted before any runs.
fn collect_due(world: &World) -> Vec<Due> {
    let instances: Vec<(EntityId, u32)> = match world.query::<ObScriptInstance>() {
        Some(query) => query.iter().map(|(entity, instance)| (entity, instance.script)).collect(),
        None => return Vec::new(),
    };
    if instances.is_empty() {
        return Vec::new();
    }
    let loaded: HashSet<EntityId> = world
        .query::<OnCellLoadEvent>()
        .map(|events| events.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    // Read, not drained: `reference_script_event_system` owns the marker.
    let died: HashSet<EntityId> = world
        .query::<OnDeathEvent>()
        .map(|events| events.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    // Activations, minus the native-only ones: "no OnActivate event will be sent
    // to any scripts" for the default processing a script asked for.
    let activated: HashMap<EntityId, EntityId> = {
        let native: HashSet<EntityId> = world
            .query::<NativeActivation>()
            .map(|marks| marks.iter().map(|(entity, _)| entity).collect())
            .unwrap_or_default();
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
    let Some(table) = world.try_resource::<ObScriptObjectTable>() else {
        return Vec::new();
    };
    let mut due = Vec::new();
    for (entity, script_id) in instances {
        let Some(object) = table.scripts.get(&script_id) else { continue };
        let wanted = [
            (BLOCK_ON_LOAD, loaded.contains(&entity)),
            (BLOCK_ON_ACTIVATE, activated.contains_key(&entity)),
            (BLOCK_ON_DEATH, died.contains(&entity)),
            (BLOCK_GAME_MODE, true),
        ];
        let blocks: Vec<u16> = wanted
            .into_iter()
            .filter(|&(block, now)| {
                now && object.blocks.contains(&block) && !table.malformed.contains(&(script_id, block))
            })
            .map(|(block, _)| block)
            .collect();
        if blocks.is_empty() {
            continue;
        }
        let Some(form) = entity_global_form_id(world, entity) else { continue };
        due.push(Due { entity, form, script: Arc::clone(&object.record), blocks, activator: activated.get(&entity).copied() });
    }
    due
}

/// What one block run came to.
struct Ran {
    outcome: BlockOutcome,
    /// Commands the host declined during the run.
    declined: u32,
}

/// Run one block against `entity`, seeding the script's locals from its
/// [`ScriptVariables`] and writing back the ones it changed. A malformed block is
/// reported once and not run again. `action_ref` is the activator while an
/// activation block runs.
fn run_block(world: &World, entity: EntityId, form: u32, script: &ScriptRecord, block: u16, dt: f32, action_ref: Option<u32>) -> Ran {
    let mut state = VmState { locals: seed_locals(world, entity, script) };
    let before = state.locals.clone();
    let mut host = ObScriptCommandHost::new(world, form, Some(entity), dt, script);
    host.action_ref = action_ref;
    let outcome = ObScriptProgram::new(script).run_block(block, &mut state, &mut host);
    let declined = host.declined.get();
    store_locals(world, entity, script, &before, &state.locals);
    if let BlockOutcome::Malformed(at) = outcome {
        log::warn!("ObScript object script {:08X}: malformed bytecode at {at:#06x} (block {block:#04x})", script.form_id);
        if let Some(mut table) = world.try_resource_mut::<ObScriptObjectTable>() {
            table.malformed.insert((script.form_id, block));
        }
    }
    Ran { outcome, declined }
}

/// Whether `entity` runs a script that has an `OnActivate` block (and one that
/// has not come back malformed): the reference whose default activation that
/// block replaces.
pub fn has_activate_block(world: &World, entity: EntityId) -> bool {
    let Some(script) = world.get::<ObScriptInstance>(entity).map(|instance| instance.script) else {
        return false;
    };
    world.try_resource::<ObScriptObjectTable>().is_some_and(|table| {
        table.scripts.get(&script).is_some_and(|object| object.blocks.contains(&BLOCK_ON_ACTIVATE))
            && !table.malformed.contains(&(script, BLOCK_ON_ACTIVATE))
    })
}

/// The reference's activator as a FormID: the player is `0x14`.
fn activator_form(world: &World, activator: EntityId) -> Option<u32> {
    if crate::papyrus_demo::PapyrusPlayerEntity::get(world) == Some(activator) {
        return Some(crate::PLAYER_REFERENCE);
    }
    entity_global_form_id(world, activator)
}

/// How deep activations may nest (`A.Activate B 1` from inside `B`'s block
/// activating `A` ...): the GECK runs the target's script "immediately", so
/// a cycle would not return.
const MAX_ACTIVATION_DEPTH: u8 = 4;

thread_local! {
    static ACTIVATION_DEPTH: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// Run `entity`'s `OnActivate` block for `activator`; when the block cannot be
/// trusted to have decided (it declined a command, or is malformed), perform the
/// default activation, which is what the reference did before scripts ran.
/// Returns whether a block ran. `dt` is the frame's, `0` for a nested run.
fn deliver_activation(world: &World, entity: EntityId, activator: EntityId, dt: f32) -> bool {
    let Some(script_id) = world.get::<ObScriptInstance>(entity).map(|instance| instance.script) else {
        return false;
    };
    let Some(object) = world
        .try_resource::<ObScriptObjectTable>()
        .and_then(|table| table.scripts.get(&script_id).map(|o| (Arc::clone(&o.record), o.blocks.contains(&BLOCK_ON_ACTIVATE))))
    else {
        return false;
    };
    let (script, has_block) = object;
    let (Some(form), Some(activator_form)) = (entity_global_form_id(world, entity), activator_form(world, activator)) else {
        return false;
    };
    if !has_block || ACTIVATION_DEPTH.get() >= MAX_ACTIVATION_DEPTH {
        return false;
    }
    ACTIVATION_DEPTH.set(ACTIVATION_DEPTH.get() + 1);
    let ran = run_block(world, entity, form, &script, BLOCK_ON_ACTIVATE, dt, Some(activator_form));
    ACTIVATION_DEPTH.set(ACTIVATION_DEPTH.get() - 1);
    if ran.declined > 0 || matches!(ran.outcome, BlockOutcome::Malformed(_)) {
        log::debug!(
            "ObScript {form:08X} OnActivate: {} declined command(s): default activation goes ahead",
            ran.declined
        );
        crate::PendingFragmentActivations::queue_native(world, entity, activator);
    }
    true
}

/// `target.Activate <actor> 1`: run the target's `OnActivate` block now.
pub(crate) fn run_activate_block(world: &World, target: EntityId, activator: EntityId) -> bool {
    deliver_activation(world, target, activator, 0.0)
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
        let mut begin = block.to_le_bytes().to_vec();
        begin.extend_from_slice(&0u32.to_le_bytes());
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
            for script in index.scripts.values().filter(|s| s.script_type == ScriptType::Object && !s.compiled.is_empty()) {
                scripts += 1;
                let program = ObScriptProgram::new(script);
                if program.block_types().is_none() {
                    unwalkable.push(script.form_id);
                    continue;
                }
                for block in [BLOCK_GAME_MODE, BLOCK_ON_LOAD, BLOCK_ON_DEATH, BLOCK_ON_ACTIVATE] {
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
        }
    }
}
