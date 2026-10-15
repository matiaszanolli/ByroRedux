//! M47.2 — Papyrus reference scripts on the VM: a script attached to a placed
//! reference (its own `VMAD` or its base record's) that no recognizer models
//! runs its events through [`super::vm`].
//!
//! The cell loader attaches such a script when [`check_reference_script`]
//! accepts every handler it has of the [`DELIVERED_EVENTS`]; the
//! [`PapyrusReferenceScripts`] component names it with its properties. The
//! script's own state — its variables and its Papyrus state — lives in the
//! saved script-variable stores under the reference's FormID, so it outlives
//! the cell unload that despawns the entity, as the component, re-derived at
//! the next attach, does not need to.
//!
//! [`reference_script_event_system`] turns the engine's event markers into
//! Papyrus events (CK *OnInit*, *OnLoad - ObjectReference*, *OnCellLoad -
//! ObjectReference*): a reference's cell load delivers `OnInit` the first
//! time the reference loads, then `OnLoad` and `OnCellLoad` every time;
//! `ActivateEvent` delivers `OnActivate(akActionRef)`; a trigger crossing
//! `OnTriggerEnter(akActionRef)`; a `HitEvent` `OnHit`; an `OnDeathEvent`
//! `OnDeath(akKiller)`. Each runs as an `Effect::Interpret` owned
//! by the reference, through the fragment runtime — so its natives, its
//! waits and its quest-stage advances behave as a quest fragment's do.
//!
//! Not modelled: `OnInit` holding back the script's other events while it
//! waits, and the variable reset a cell reset brings.

use super::*;
use crate::translate::effects::VmStored;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_plugin::esm::records::condition::ConditionStringId;
use byroredux_plugin::esm::records::script_instance::ScriptInstance;

/// The events reference scripts receive, in the order one cell load
/// delivers the first four (CK *OnCellAttach*: "reliably called before
/// OnCellLoad"; attach and load are one moment here).
pub const DELIVERED_EVENTS: [&str; 18] = [
    "OnInit",
    "OnCellAttach",
    "OnLoad",
    "OnCellLoad",
    "OnActivate",
    "OnTriggerEnter",
    "OnHit",
    "OnEnterBleedout",
    "OnCombatStateChanged",
    "OnLocationChange",
    "OnDying",
    "OnDeath",
    "OnCellDetach",
    "OnUnload",
    // Raised outside a script run, by `raise_reference_event`: a destructible
    // object's stage worsening, and a trap body touching a live actor.
    "OnDestructionStageChanged",
    "OnTrapHit",
    "OnTrapHitStart",
    "OnTrapHitStop",
];

/// An event the runtime delivers — to a reference, or an update to the
/// script that registered for it. One the script does not handle in its
/// current state does nothing.
pub(super) fn is_delivered_event(name: &str) -> bool {
    DELIVERED_EVENTS
        .iter()
        .chain(&["OnUpdate", "OnUpdateGameTime", "OnTimer", "OnTimerGameTime", "OnAnimationEvent", "OnAnimationEventUnregistered"])
        .any(|event| event.eq_ignore_ascii_case(name))
}

pub use super::vm::check_reference_script;

/// One Papyrus script the VM runs on a reference.
#[derive(Debug, Clone)]
pub struct ReferenceScript {
    /// The script's name, as the VM's script store keys it.
    pub script: String,
    /// The script's properties: its own `VMAD` entry.
    pub vmad: Arc<ScriptInstanceData>,
    /// The [`DELIVERED_EVENTS`] it handles.
    pub events: Vec<String>,
}

impl ReferenceScript {
    fn handles(&self, event: &str) -> bool {
        self.events.iter().any(|handled| handled.eq_ignore_ascii_case(event))
    }
}

/// M47.2 — the Papyrus scripts the VM runs on this reference.
#[derive(Debug, Clone, Default)]
pub struct PapyrusReferenceScripts(pub Vec<ReferenceScript>);

impl Component for PapyrusReferenceScripts {
    type Storage = SparseSetStorage<Self>;
}

/// FormID → entity for the references that carry [`PapyrusReferenceScripts`]
/// (#5763). `script_update_system` and the VM's cross-reference calls used to
/// find a reference by scanning every attached entity (and `entity_of` scans
/// every FormID-tagged one) each time; this answers in one hash lookup.
///
/// Maintained on attach: [`attach_reference_script`] marks the set changed, and
/// the next lookup rebuilds the map once from the attached components — the
/// FormID is read then, so a reference whose `FormIdComponent` lands after its
/// scripts is still found. Unloading is not tracked: a cell unload despawns
/// the entity, and [`ReferenceScriptIndex::lookup`] re-checks the entity it
/// finds, so a despawned or reused one reads as not loaded. A reload attaches
/// again and the next lookup picks it up. Transient: never saved.
#[derive(Debug, Default)]
pub struct ReferenceScriptIndex {
    by_form: HashMap<u32, EntityId>,
    /// Bumped by every attach; the map is current when `built == attached`.
    attached: u64,
    built: u64,
}

impl Resource for ReferenceScriptIndex {}

impl ReferenceScriptIndex {
    fn note_attached(&mut self) {
        self.attached += 1;
    }

    /// Rebuild the map when an attach has happened since it was built.
    fn refresh(world: &World) {
        let stale = world
            .try_resource::<ReferenceScriptIndex>()
            .is_some_and(|index| index.built != index.attached);
        if !stale {
            return;
        }
        let mut by_form = HashMap::new();
        if let Some(attached) = world.query::<PapyrusReferenceScripts>() {
            for (entity, _) in attached.iter() {
                if let Some(form) = crate::condition::entity_global_form_id(world, entity) {
                    by_form.insert(form, entity);
                }
            }
        }
        if let Some(mut index) = world.try_resource_mut::<ReferenceScriptIndex>() {
            index.by_form = by_form;
            index.built = index.attached;
        }
    }

    /// The loaded reference with FormID `form` that carries VM scripts;
    /// `None` when there is none (never attached, or its cell unloaded).
    pub fn lookup(world: &World, form: u32) -> Option<EntityId> {
        Self::refresh(world);
        let entity = world.try_resource::<ReferenceScriptIndex>()?.by_form.get(&form).copied()?;
        // Re-check what the map says: the entity may have been despawned (its
        // cell unloaded) or its id reused since the map was built.
        world.get::<PapyrusReferenceScripts>(entity)?;
        (crate::condition::entity_global_form_id(world, entity) == Some(form)).then_some(entity)
    }
}

/// Attach `instance` — a script [`check_reference_script`] accepted, handling
/// `events`, with the `scripts` its runs need — to `entity`.
pub fn attach_reference_script(
    world: &mut World,
    entity: EntityId,
    instance: &ScriptInstance,
    events: Vec<String>,
    scripts: Vec<Arc<byroredux_papyrus::ast::Script>>,
) {
    {
        let mut fragments = world.resource_mut::<QuestStageFragments>();
        for script in scripts {
            fragments.insert_vm_script(script);
        }
    }
    // The stores the script's state lives in.
    if world.try_resource::<crate::QuestScriptVariables>().is_none() {
        world.insert_resource(crate::QuestScriptVariables::default());
    }
    if world.try_resource::<crate::QuestScriptObjects>().is_none() {
        world.insert_resource(crate::QuestScriptObjects::default());
    }
    world.register::<PapyrusReferenceScripts>();
    let script = ReferenceScript {
        script: instance.name.clone(),
        vmad: Arc::new(ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![instance.clone()],
        }),
        events,
    };
    {
        let mut attached = world.query_mut::<PapyrusReferenceScripts>().expect("registered above");
        match attached.get_mut(entity) {
            Some(scripts) => scripts.0.push(script),
            None => {
                attached.insert(entity, PapyrusReferenceScripts(vec![script]));
            }
        }
    }
    if world.try_resource::<ReferenceScriptIndex>().is_none() {
        world.insert_resource(ReferenceScriptIndex::default());
    }
    world.resource_mut::<ReferenceScriptIndex>().note_attached();
}

/// Where whether `script` has run its `OnInit` on a reference is kept, among
/// the reference's script variables.
fn initialized_key(script: &str) -> ConditionStringId {
    ConditionStringId::from_text(&format!("::init::{}", script.to_ascii_lowercase()))
}

/// One event delivery.
struct Delivery {
    owner: u32,
    /// The script's name and properties; its handled-event list stays with the
    /// component (cloning it per delivery was most of a burst's cost).
    script: String,
    vmad: Arc<ScriptInstanceData>,
    event: &'static str,
    args: Vec<VmStored>,
    /// Whether `OnInit` runs once per reference (the reference scripts'
    /// first-load gate); an alias script's is gated by its quest starting.
    init_gated: bool,
}

/// Deliver this frame's events to the reference scripts the VM runs.
pub fn reference_script_event_system(world: &World, _dt: f32) {
    super::events::prune_stopped_quests(world);
    relay_engine_events(world);
    let mut deliveries = world
        .try_resource_mut::<PendingUnloadDeliveries>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    deliveries.extend(collect_deliveries(world));
    run_deliveries(world, deliveries);
}

/// The unload events raised for references about to be despawned, delivered by
/// the next [`reference_script_event_system`] run. Transient: never saved.
#[derive(Default)]
pub struct PendingUnloadDeliveries(Vec<Delivery>);

impl Resource for PendingUnloadDeliveries {}

/// Raise `OnCellDetach` and `OnUnload` (CK *OnCellDetach*, *OnUnload*) for the
/// `victims` of a cell unload — references the scripts attached to, and the
/// ones quest aliases are filled with, before they despawn. Delivered with the
/// next frame's events.
pub fn raise_unload_events(world: &World, victims: &[EntityId]) {
    super::anim_events::unregister_unloaded(world, victims);
    // The alias standing takes the quest-state guard: before the reference
    // scripts' query is held (see `collect_deliveries`).
    let aliases = alias_script_targets(world);
    let Some(attached) = world.query::<PapyrusReferenceScripts>() else {
        if aliases.is_empty() {
            return;
        }
        return raise_alias_unloads(world, victims, &aliases);
    };
    let mut pending = Vec::new();
    for &victim in victims {
        let Some(scripts) = attached.get(victim) else {
            continue;
        };
        let Some(owner) = crate::condition::entity_global_form_id(world, victim) else {
            continue;
        };
        for script in &scripts.0 {
            for event in UNLOAD_EVENTS {
                if script.handles(event) {
                    pending.push(Delivery {
                        owner,
                        script: script.script.clone(),
                        vmad: Arc::clone(&script.vmad),
                        event,
                        args: Vec::new(),
                        init_gated: true,
                    });
                }
            }
        }
    }
    drop(attached);
    if !pending.is_empty() {
        if let Some(mut queue) = world.try_resource_mut::<PendingUnloadDeliveries>() {
            queue.0.append(&mut pending);
        }
    }
    if !aliases.is_empty() {
        raise_alias_unloads(world, victims, &aliases);
    }
}

/// Queue `event` with `args` for the scripts on `entity` that handle it, delivered
/// by the next [`reference_script_event_system`] run (an event the engine raises
/// outside a script run, such as a destruction stage changing).
pub fn raise_reference_event(world: &World, entity: EntityId, event: &'static str, args: Vec<VmStored>) {
    let Some(owner) = crate::condition::entity_global_form_id(world, entity) else {
        return;
    };
    let pending: Vec<Delivery> = {
        let Some(attached) = world.query::<PapyrusReferenceScripts>() else {
            return;
        };
        let Some(scripts) = attached.get(entity) else {
            return;
        };
        scripts
            .0
            .iter()
            .filter(|script| script.handles(event))
            .map(|script| Delivery {
                owner,
                script: script.script.clone(),
                vmad: Arc::clone(&script.vmad),
                event,
                args: args.clone(),
                init_gated: false,
            })
            .collect()
    };
    if let Some(mut queue) = world.try_resource_mut::<PendingUnloadDeliveries>() {
        queue.0.extend(pending);
    }
}

/// Queue a call of `function` on every script the VM runs on `entity`, delivered by
/// the next [`reference_script_event_system`] run: what a script's own call to its
/// function does, for the debug console (`script.call`). Returns how many scripts
/// were queued. The function name is leaked (a handful of console calls).
pub fn raise_reference_call(world: &World, entity: EntityId, function: &str, args: Vec<VmStored>) -> usize {
    let Some(owner) = crate::condition::entity_global_form_id(world, entity) else {
        return 0;
    };
    let function: &'static str = Box::leak(function.to_owned().into_boxed_str());
    let pending: Vec<Delivery> = {
        let Some(attached) = world.query::<PapyrusReferenceScripts>() else {
            return 0;
        };
        let Some(scripts) = attached.get(entity) else {
            return 0;
        };
        scripts
            .0
            .iter()
            .map(|script| Delivery {
                owner,
                script: script.script.clone(),
                vmad: Arc::clone(&script.vmad),
                event: function,
                args: args.clone(),
                init_gated: false,
            })
            .collect()
    };
    let count = pending.len();
    if let Some(mut queue) = world.try_resource_mut::<PendingUnloadDeliveries>() {
        queue.0.extend(pending);
    }
    count
}

/// Run `function` of the quest script `script` now, with `args` (the debug console's
/// `script.qcall`: what a dialogue fragment's call into its quest script does).
/// The quest is the first whose `VMAD` carries `script`; returns its FormID.
pub fn run_quest_script_function(world: &World, script: &str, function: &str, args: Vec<VmStored>) -> Result<u32, String> {
    let (quest, vmad) = {
        let instances = world
            .try_resource::<crate::QuestScriptInstances>()
            .ok_or_else(|| "no quest script instances installed".to_owned())?;
        instances
            .0
            .iter()
            .filter(|(_, vmad)| vmad.scripts.iter().any(|s| s.name.eq_ignore_ascii_case(script)))
            .min_by_key(|(quest, _)| quest.0)
            .map(|(quest, vmad)| (*quest, vmad.clone()))
            .ok_or_else(|| format!("no quest carries a script named {script}"))?
    };
    let effect = Effect::Interpret {
        script: script.to_owned(),
        function: function.to_owned(),
        args,
        resume: None,
    };
    let advances = apply_fragment_guard_free(world, std::slice::from_ref(&effect), quest, Some(&vmad));
    flush_advances(world, advances);
    Ok(quest.0)
}

/// Run one Papyrus statement now (the debug console's `script.exec`): it is wrapped in a
/// quest script's function, parsed, installed for the VM and run with no properties, so it can
/// name `Game` calls and what they return (`Game.GetPlayer().AddItem(Game.GetCredits() as
/// Form, 5)`) but no script property. A statement the VM cannot run stops the run with its
/// usual warning in the log.
pub fn run_debug_statement(world: &World, statement: &str) -> Result<(), String> {
    let source = format!("ScriptName DebugExec extends Quest\nFunction Run()\n{statement}\nEndFunction\n");
    let (script, _) = byroredux_papyrus::parse_script(&source)
        .map_err(|errors| format!("does not parse: {}", errors.first().map_or(String::new(), |e| e.to_string())))?;
    {
        let mut fragments = world
            .try_resource_mut::<crate::QuestStageFragments>()
            .ok_or_else(|| "no quest fragments resource".to_owned())?;
        fragments.insert_vm_script(Arc::new(script));
    }
    let effect = Effect::Interpret {
        script: "DebugExec".to_owned(),
        function: "Run".to_owned(),
        args: Vec::new(),
        resume: None,
    };
    apply_fragment_guard_free(world, std::slice::from_ref(&effect), QuestFormId(0), None);
    Ok(())
}

/// The events a reference receives when its cell unloads, in order.
const UNLOAD_EVENTS: [&str; 2] = ["OnCellDetach", "OnUnload"];

fn raise_alias_unloads(world: &World, victims: &[EntityId], aliases: &HashMap<EntityId, Vec<(u32, AliasScript)>>) {
    let mut pending = Vec::new();
    for victim in victims {
        for (owner, script) in aliases.get(victim).into_iter().flatten() {
            for event in UNLOAD_EVENTS {
                if script.handles(event) {
                    pending.push(Delivery {
                        owner: *owner,
                        script: script.script.clone(),
                        vmad: Arc::clone(&script.vmad),
                        event,
                        args: Vec::new(),
                        init_gated: false,
                    });
                }
            }
        }
    }
    if !pending.is_empty() {
        if let Some(mut queue) = world.try_resource_mut::<PendingUnloadDeliveries>() {
            queue.0.append(&mut pending);
        }
    }
}

/// Run `deliveries` in order, each as an interpreted event of its owner.
fn run_deliveries(world: &World, deliveries: Vec<Delivery>) {
    if deliveries.is_empty() {
        return;
    }
    let mut advances = Vec::new();
    for delivery in deliveries {
        let owner = QuestFormId(delivery.owner);
        if delivery.event == "OnInit" && delivery.init_gated {
            let key = initialized_key(&delivery.script);
            let Some(mut variables) = world.try_resource_mut::<crate::QuestScriptVariables>() else {
                continue;
            };
            if variables.get(owner, key).is_some() {
                continue;
            }
            variables.set(owner, key, 1.0);
        }
        log::debug!(
            "reference {:08X}: {}.{}",
            delivery.owner,
            delivery.script,
            delivery.event
        );
        let effect = Effect::Interpret {
            script: delivery.script.clone(),
            function: delivery.event.to_owned(),
            args: delivery.args,
            resume: None,
        };
        // The frame-budget gate and the advance flush live in one place
        // (`dispatch`) — this used to be a hand-copied 25-line tail.
        advances.extend(run_papyrus_call(world, owner, Some(&delivery.vmad), effect));
    }
    flush_advances(world, advances);
}

/// What changed about the actors since the reference scripts last looked: the
/// state `OnEnterBleedout`, `OnCombatStateChanged` and `OnLocationChange` are
/// delivered for. Compared against [`ActorWatch`].
#[derive(Debug, Default)]
struct ActorChanges {
    /// Actors that went into bleedout.
    bleedout: HashSet<EntityId>,
    /// Actors whose combat target changed, with the new target (`None`: they
    /// left combat).
    combat: HashMap<EntityId, Option<EntityId>>,
    /// The player's location, if it changed: (old, new).
    player_location: Option<(Option<u32>, Option<u32>)>,
}

fn actor_changes(world: &World) -> ActorChanges {
    let mut changes = ActorChanges::default();
    let bleeding: HashSet<EntityId> = world
        .query::<crate::bleedout::Bleedout>()
        .map(|bleedouts| bleedouts.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    let combat: HashMap<EntityId, EntityId> = world
        .query::<crate::AiCombatState>()
        .map(|states| states.iter().map(|(entity, state)| (entity, state.target)).collect())
        .unwrap_or_default();
    let location = world.try_resource::<crate::location::PlayerLocation>().map(|l| l.0);
    let Some(mut watch) = world.try_resource_mut::<ActorWatch>() else {
        return changes;
    };
    changes.bleedout = bleeding.difference(&watch.bleeding).copied().collect();
    for (&entity, &target) in &combat {
        if watch.combat.get(&entity) != Some(&target) {
            changes.combat.insert(entity, Some(target));
        }
    }
    for &entity in watch.combat.keys() {
        if !combat.contains_key(&entity) {
            changes.combat.insert(entity, None);
        }
    }
    if let Some(location) = location {
        if watch.player_location != Some(location) {
            // The first sight of the location is a baseline, not a change.
            if let Some(old) = watch.player_location {
                changes.player_location = Some((old, location));
            }
            watch.player_location = Some(location);
        }
    }
    watch.bleeding = bleeding;
    watch.combat = combat;
    changes
}

/// What the reference scripts last saw of the actors. Transient.
#[derive(Debug, Default)]
pub struct ActorWatch {
    bleeding: HashSet<EntityId>,
    combat: HashMap<EntityId, EntityId>,
    player_location: Option<Option<u32>>,
}

impl Resource for ActorWatch {}

/// The alias scripts of the quests that run, by the entity their alias is
/// filled with: (owner id, script). Only loaded fills can receive events.
fn alias_script_targets(world: &World) -> HashMap<EntityId, Vec<(u32, AliasScript)>> {
    let mut targets: HashMap<EntityId, Vec<(u32, AliasScript)>> = HashMap::new();
    let Some(alias_scripts) = world.try_resource::<QuestAliasScripts>() else {
        return targets;
    };
    let quests: Vec<(QuestFormId, Arc<Vec<AliasScript>>)> = alias_scripts
        .checked_quests()
        .filter_map(|quest| Some((quest, Arc::clone(alias_scripts.scripts(quest)?))))
        .collect();
    drop(alias_scripts);
    if quests.is_empty() {
        return targets;
    }
    let running: Vec<bool> = {
        let stages = world.resource::<QuestStageState>();
        quests.iter().map(|(quest, _)| stages.is_running(*quest)).collect()
    };
    let (Some(bindings), Some(owners)) = (
        world.try_resource::<crate::SceneActorBindings>(),
        world.try_resource::<AliasScriptOwners>(),
    ) else {
        return targets;
    };
    for ((quest, scripts), running) in quests.iter().zip(running) {
        if !running {
            continue;
        }
        for script in scripts.iter() {
            let Some(entity) = bindings.resolve(*quest, i32::from(script.alias)) else {
                continue;
            };
            let Some(owner) = owners.owner_of(*quest, script.alias) else {
                continue;
            };
            targets.entry(entity).or_default().push((owner, script.clone()));
        }
    }
    targets
}

/// The deliveries this frame's markers make, snapshotted before any runs:
/// a run takes the quest guards and may write the marker storages. A marker
/// on a reference reaches the scripts attached to it and the alias scripts of
/// the aliases it fills.
fn collect_deliveries(world: &World) -> Vec<Delivery> {
    // Pattern B: this system owns `OnDeathEvent`; drain it first, whatever
    // follows.
    let deaths: HashMap<EntityId, Option<EntityId>> = world
        .query::<crate::OnDeathEvent>()
        .map(|q| q.iter().map(|(entity, event)| (entity, event.killer)).collect())
        .unwrap_or_default();
    crate::scene::playback::drain::<crate::OnDeathEvent>(world);
    // Before `attached` is held: the quest standing read takes the quest-state
    // guard, which a VM run holds when it reaches for a reference's scripts.
    let aliases = alias_script_targets(world);
    let changes = actor_changes(world);
    let attached = world.query::<PapyrusReferenceScripts>();
    let player = world
        .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
        .map(|player| player.0);
    let form_of = |entity: EntityId| -> Option<u32> {
        if Some(entity) == player {
            return Some(0x14);
        }
        crate::condition::entity_global_form_id(world, entity)
    };
    let cell_loads = world.query::<crate::OnCellLoadEvent>();
    let activations = world.query::<crate::ActivateEvent>();
    let native = world.query::<crate::NativeActivation>();
    let triggers = world.query::<crate::OnTriggerEnterEvent>();
    let hits = world.query::<crate::HitEvent>();
    let weapons = world.query::<byroredux_core::ecs::components::EquippedWeapon>();
    // Every entity a script is attached to or an alias script follows.
    let mut entities: Vec<EntityId> = attached
        .as_ref()
        .map(|attached| attached.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    entities.extend(aliases.keys().copied().filter(|entity| attached.as_ref().is_none_or(|a| a.get(*entity).is_none())));
    let mut deliveries = Vec::new();
    for entity in entities {
        let loaded = cell_loads.as_ref().is_some_and(|q| q.get(entity).is_some());
        // (`Activate(…, true)` is default processing only: no `OnActivate`.)
        let activator = activations
            .as_ref()
            .and_then(|q| q.get(entity))
            .filter(|_| native.as_ref().is_none_or(|marks| marks.get(entity).is_none()))
            .map(|event| event.activator);
        let triggerers: Vec<EntityId> = triggers
            .as_ref()
            .and_then(|q| q.get(entity))
            .map(|event| event.triggerers.clone())
            .unwrap_or_default();
        let death = deaths.get(&entity).copied();
        let hit = hits.as_ref().and_then(|q| q.get(entity)).copied();
        let bled = changes.bleedout.contains(&entity);
        let combat = changes.combat.get(&entity).copied();
        let relocated = (Some(entity) == player).then_some(changes.player_location).flatten();
        if !loaded
            && activator.is_none()
            && triggerers.is_empty()
            && death.is_none()
            && hit.is_none()
            && !bled
            && combat.is_none()
            && relocated.is_none()
        {
            continue;
        }
        let reference = |entity: EntityId| VmStored::Form(form_of(entity).unwrap_or(0));
        // The events this reference received, in delivery order.
        let mut events: Vec<(&'static str, Vec<VmStored>)> = Vec::new();
        if loaded {
            for event in ["OnInit", "OnCellAttach", "OnLoad", "OnCellLoad"] {
                events.push((event, Vec::new()));
            }
        }
        if let Some(activator) = activator {
            events.push(("OnActivate", vec![reference(activator)]));
        }
        for &triggerer in &triggerers {
            events.push(("OnTriggerEnter", vec![reference(triggerer)]));
        }
        // `OnHit(akAggressor, akSource, akProjectile, abPowerAttack,
        // abSneakAttack, abBashAttack, abHitBlocked)`: the source is the
        // aggressor's equipped weapon (None unarmed — no weapon form is
        // invented); projectiles are not modelled.
        if let Some(hit) = hit {
            let source = weapons
                .as_ref()
                .and_then(|q| q.get(hit.aggressor))
                .map_or(VmStored::None, |weapon| VmStored::Form(weapon.base_form_id));
            events.push((
                "OnHit",
                vec![
                    reference(hit.aggressor),
                    source,
                    VmStored::None,
                    VmStored::Bool(hit.power_attack),
                    VmStored::Bool(hit.sneak_attack),
                    VmStored::Bool(hit.bash_attack),
                    VmStored::Bool(hit.blocked),
                ],
            ));
        }
        if bled {
            // CK *OnEnterBleedout - Actor*.
            events.push(("OnEnterBleedout", Vec::new()));
        }
        if let Some(target) = combat {
            // CK *OnCombatStateChanged - Actor*: the target (None leaving
            // combat) and the state entered — 0 not in combat, 1 in combat; 2
            // (searching) is not modelled.
            events.push((
                "OnCombatStateChanged",
                vec![target.map_or(VmStored::None, reference), VmStored::Int(i32::from(target.is_some()))],
            ));
        }
        if let Some((old, new)) = relocated {
            // CK *OnLocationChange - Actor*: the locations left and entered.
            // Only the player's location is tracked.
            let location = |location: Option<u32>| location.map_or(VmStored::None, VmStored::Form);
            events.push(("OnLocationChange", vec![location(old), location(new)]));
        }
        if let Some(killer) = death {
            let killer = killer.map_or(VmStored::None, reference);
            // CK *OnDying*: the actor "is starting to die", ahead of `OnDeath`;
            // both arrive with the death here.
            events.push(("OnDying", vec![killer.clone()]));
            events.push(("OnDeath", vec![killer]));
        }
        if let Some(scripts) = attached.as_ref().and_then(|attached| attached.get(entity)) {
            let Some(owner) = form_of(entity) else {
                log::debug!("reference script events on {entity:?} skipped: the reference has no FormID");
                continue;
            };
            for script in &scripts.0 {
                for (event, args) in &events {
                    if script.handles(event) {
                        deliveries.push(Delivery {
                            owner,
                            script: script.script.clone(),
                            vmad: Arc::clone(&script.vmad),
                            event,
                            args: args.clone(),
                            init_gated: true,
                        });
                    }
                }
            }
        }
        for (owner, script) in aliases.get(&entity).into_iter().flatten() {
            for (event, args) in &events {
                // An alias script starts with its quest, not with its
                // reference loading.
                if *event != "OnInit" && script.handles(event) {
                    deliveries.push(Delivery {
                        owner: *owner,
                        script: script.script.clone(),
                        vmad: Arc::clone(&script.vmad),
                        event,
                        args: args.clone(),
                        init_gated: false,
                    });
                }
            }
        }
    }
    deliveries
}

/// Start and shut down the alias scripts of the quests that run: a quest that
/// starts delivers `OnInit` and then `OnAliasInit` to each alias script (CK
/// *OnAliasInit*: "Other aliases in the quest will have also been filled by
/// this point"), and one that stops delivers `OnAliasShutdown`.
pub fn alias_script_event_system(world: &World, _dt: f32) {
    let (quests, standing) = {
        let Some(alias_scripts) = world.try_resource::<QuestAliasScripts>() else {
            return;
        };
        let mut quests: Vec<(QuestFormId, Arc<Vec<AliasScript>>)> = alias_scripts
            .checked_quests()
            .filter_map(|quest| Some((quest, Arc::clone(alias_scripts.scripts(quest)?))))
            .collect();
        quests.sort_unstable_by_key(|(quest, _)| quest.0);
        drop(alias_scripts);
        let stages = world.resource::<QuestStageState>();
        let standing: Vec<bool> = quests.iter().map(|(quest, _)| stages.is_running(*quest)).collect();
        (quests, standing)
    };
    if quests.is_empty() {
        return;
    }
    // A fill still pending would make `OnAliasInit` read an empty alias.
    let filling = world
        .try_resource::<crate::SceneActorBindings>()
        .is_some_and(|bindings| bindings.is_dirty());
    let mut deliveries = Vec::new();
    for ((quest, scripts), running) in quests.into_iter().zip(standing) {
        let started = world.try_resource::<AliasScriptOwners>().is_some_and(|o| o.is_started(quest));
        let phases: &[&'static str] = match (running, started) {
            (true, false) if !filling => &["OnInit", "OnAliasInit"],
            (false, true) => &["OnAliasShutdown"],
            _ => continue,
        };
        let Some(mut owners) = world.try_resource_mut::<AliasScriptOwners>() else {
            return;
        };
        if running {
            owners.mark_started(quest);
        } else {
            owners.mark_stopped(quest);
        }
        for event in phases {
            for script in scripts.iter() {
                if !script.handles(event) {
                    continue;
                }
                let Some(owner) = owners.ensure(quest, script.alias) else {
                    continue;
                };
                deliveries.push(Delivery {
                    owner,
                    script: script.script.clone(),
                    vmad: Arc::clone(&script.vmad),
                    event,
                    args: Vec::new(),
                    init_gated: false,
                });
            }
        }
    }
    run_deliveries(world, deliveries);
}

/// Relay this frame's engine events to the scripts registered for them
/// remotely (`RegisterForRemoteEvent`): the event a reference received, handled
/// by the registrant with the sender first and the event's own arguments after.
/// Read without draining: the consumers below and end-of-frame cleanup own the
/// markers.
fn relay_engine_events(world: &World) {
    let any = world
        .try_resource::<super::EventRegistrations>()
        .is_some_and(|registrations| registrations.entries().iter().any(|e| e.remote));
    if !any {
        return;
    }
    let player = world
        .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
        .map(|player| player.0);
    let form_of = |entity: EntityId| -> Option<u32> {
        if Some(entity) == player {
            return Some(0x14);
        }
        crate::condition::entity_global_form_id(world, entity)
    };
    let reference = |entity: EntityId| VmStored::Form(form_of(entity).unwrap_or(0));
    let mut relays: Vec<(u32, &'static str, Vec<VmStored>)> = Vec::new();
    if let Some(events) = world.query::<crate::ActivateEvent>() {
        // (Acquired after `ActivateEvent`, the order of the delivery pass above.)
        let native = world.query::<crate::NativeActivation>();
        for (entity, event) in events.iter() {
            if native.as_ref().is_some_and(|marks| marks.get(entity).is_some()) {
                continue;
            }
            if let Some(form) = form_of(entity) {
                relays.push((form, "OnActivate", vec![reference(event.activator)]));
            }
        }
    }
    if let Some(events) = world.query::<crate::OnTriggerEnterEvent>() {
        for (entity, event) in events.iter() {
            if let Some(form) = form_of(entity) {
                for &triggerer in &event.triggerers {
                    relays.push((form, "OnTriggerEnter", vec![reference(triggerer)]));
                }
            }
        }
    }
    if let Some(events) = world.query::<crate::OnDeathEvent>() {
        for (entity, event) in events.iter() {
            if let Some(form) = form_of(entity) {
                relays.push((form, "OnDeath", vec![event.killer.map_or(VmStored::None, reference)]));
            }
        }
    }
    if let Some(events) = world.query::<crate::HitEvent>() {
        for (entity, hit) in events.iter() {
            if let Some(form) = form_of(entity) {
                relays.push((
                    form,
                    "OnHit",
                    vec![
                        reference(hit.aggressor),
                        VmStored::None,
                        VmStored::None,
                        VmStored::Bool(hit.power_attack),
                        VmStored::Bool(hit.sneak_attack),
                        VmStored::Bool(hit.bash_attack),
                        VmStored::Bool(hit.blocked),
                    ],
                ));
            }
        }
    }
    for (sender, event, args) in relays {
        super::events::relay_remote(world, sender, event, &args);
    }
}
