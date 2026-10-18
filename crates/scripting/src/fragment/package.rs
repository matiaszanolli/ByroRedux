//! M47.2 — package (`PACK`) fragments: the `PF_` script functions a package
//! binds to its Begin / End / Change events, run on the Papyrus VM with
//! `akActor` bound to the actor running the package.
//!
//! The CK's *Package Fragments*: "run when the package begins, when the
//! package ends, and when the package changes"; the FO4 CK's *OnEnd* /
//! *OnChange - Package*: End is a normal end ("but not if it is
//! preempted"), Change is either. The host reports each actor's package
//! transitions ([`PendingPackageTransitions`]); a switch from one package to
//! another is a preemption — this engine re-selects packages by their
//! conditions and schedules and does not model a procedure completing — so
//! it runs the old package's Change fragment and the new one's Begin. End
//! runs when a procedure completes on its own ([`report_package_end`]); the
//! only completing procedure modelled is Travel's arrival — Sandbox, Wander
//! and the rest never finish here, so their End fragments never run.
//!
//! Fragments install only when the VM's check passes (the lowering's INFO
//! speaker binding does not cover `akActor`); the fragment runs with the
//! package as its `Self`.

use super::*;
use crate::translate::effects::VmStored;
use byroredux_plugin::esm::records::script_instance::PackageFragmentEvent;

/// One record's installed fragments, each bound to an event of type `E`
/// (a package's Begin / End / Change, a terminal's menu item).
#[derive(Debug, Clone)]
pub struct FragmentSet<E> {
    /// The fragment script's properties.
    pub vmad: Option<ScriptInstanceData>,
    /// (event, script, function) per installed fragment.
    pub fragments: Vec<(E, String, String)>,
}

/// One package's installed fragments.
pub type PackageFragmentSet = FragmentSet<PackageFragmentEvent>;

/// Every record's installed fragments, by record FormID.
#[derive(Debug, Clone)]
pub struct FragmentTable<E> {
    map: Arc<HashMap<u32, FragmentSet<E>>>,
    populated: bool,
}

impl<E> Default for FragmentTable<E> {
    fn default() -> Self {
        Self { map: Arc::default(), populated: false }
    }
}

impl<E: Send + Sync + 'static> Resource for FragmentTable<E> {}

/// Every package's installed fragments, by package FormID.
pub type PackageFragments = FragmentTable<PackageFragmentEvent>;

impl<E: Clone> FragmentTable<E> {
    pub fn insert(&mut self, record: u32, set: FragmentSet<E>) {
        Arc::make_mut(&mut self.map).insert(record, set);
    }
}

impl<E> FragmentTable<E> {
    pub fn get(&self, record: u32) -> Option<&FragmentSet<E>> {
        self.map.get(&record)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn is_populated(&self) -> bool {
        self.populated
    }

    pub fn mark_populated(&mut self) {
        self.populated = true;
    }
}

/// Install `record`'s fragments from its decompiled fragment `script`: the
/// `bindings` whose function the VM's check accepts, with the scripts their
/// runs need handed to `install`. Returns how many installed.
pub fn populate_fragments<E>(
    frags: &mut FragmentTable<E>,
    record: u32,
    vmad: Option<&ScriptInstanceData>,
    script: Arc<Script>,
    bindings: &[(E, &str)],
    library: &dyn crate::translate::effects::ScriptLibrary,
    install: &mut dyn FnMut(Arc<Script>),
) -> usize
where
    E: Copy,
{
    let mut fragments = Vec::new();
    for (event, function) in bindings {
        let Some(scripts) = super::vm::check_function(Arc::clone(&script), function, Some(library)) else {
            continue;
        };
        for needed in scripts {
            install(needed);
        }
        fragments.push((*event, script.name.node.0.clone(), (*function).to_owned()));
    }
    let installed = fragments.len();
    if installed > 0 {
        frags.insert(
            record,
            FragmentSet {
                vmad: vmad.cloned(),
                fragments,
            },
        );
    }
    installed
}

/// [`populate_fragments`] for a package.
pub fn populate_package_fragments(
    frags: &mut PackageFragments,
    package: u32,
    vmad: Option<&ScriptInstanceData>,
    script: Arc<Script>,
    bindings: &[(PackageFragmentEvent, &str)],
    library: &dyn crate::translate::effects::ScriptLibrary,
    install: &mut dyn FnMut(Arc<Script>),
) -> usize {
    populate_fragments(frags, package, vmad, script, bindings, library, install)
}

/// An actor's package changed: `from` stopped running, `to` started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageTransition {
    pub actor: EntityId,
    pub from: Option<u32>,
    pub to: Option<u32>,
}

/// The package transitions the host reported this frame, drained by
/// [`package_fragment_system`].
#[derive(Debug, Default)]
pub struct PendingPackageTransitions(pub Vec<PackageTransition>);

impl Resource for PendingPackageTransitions {}

/// An actor finished running `package` (a procedure completed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageEnd {
    pub actor: EntityId,
    pub package: u32,
}

/// The package completions the host reported this frame.
#[derive(Debug, Default)]
pub struct PendingPackageEnds(pub Vec<PackageEnd>);

impl Resource for PendingPackageEnds {}

/// Report that `actor` finished running `package`: the procedure completed
/// on its own (a Travel arrival), as opposed to being preempted.
pub fn report_package_end(world: &World, actor: EntityId, package: u32) {
    if let Some(mut pending) = world.try_resource_mut::<PendingPackageEnds>() {
        pending.0.push(PackageEnd { actor, package });
    }
}

/// The package each actor is running now, by entity, kept from the
/// transitions the host reports (`Actor.GetCurrentPackage`). `NOT_SAVED_BY_DESIGN`:
/// the AI reports every actor's package again when it spawns after a load.
#[derive(Debug, Default)]
pub struct ActivePackages(pub HashMap<EntityId, u32>);

impl Resource for ActivePackages {}

/// Report that `actor`'s package changed from `from` to `to`.
pub fn report_package_transition(world: &World, actor: EntityId, from: Option<u32>, to: Option<u32>) {
    // Before the no-change early return: a spawn that reports no package must
    // still clear what an earlier occupant of a reused entity id left.
    if let Some(mut active) = world.try_resource_mut::<ActivePackages>() {
        match to {
            Some(package) => {
                active.0.insert(actor, package);
            }
            None => {
                active.0.remove(&actor);
            }
        }
    }
    if from == to {
        return;
    }
    if let Some(mut pending) = world.try_resource_mut::<PendingPackageTransitions>() {
        pending.0.push(PackageTransition { actor, from, to });
    }
}

/// The package the host last reported `actor` running (`None`: none, or
/// never reported). The `from` of the next transition is this, not what the
/// AI's own bookkeeping believes: a package only began once it was reported
/// (#5864).
pub fn reported_package(world: &World, actor: EntityId) -> Option<Option<u32>> {
    world
        .try_resource::<ActivePackages>()
        .map(|active| active.0.get(&actor).copied())
}

/// Forget `actors` in [`ActivePackages`] without running any fragment: they
/// are leaving the world (cell unload), not changing package (#5870). A dead
/// actor reports a real `→ None` transition instead.
pub fn retire_active_packages(world: &World, actors: &[EntityId]) {
    if actors.is_empty() {
        return;
    }
    if let Some(mut active) = world.try_resource_mut::<ActivePackages>() {
        for actor in actors {
            active.0.remove(actor);
        }
    }
}

/// A save load replaced the session: the package reports still queued, and the
/// per-entity package map, describe actors that no longer exist (their
/// `EntityId`s may be reused by unrelated ones). The reload's own spawns and
/// first-tick confirmations report afresh (#5864).
pub fn discard_package_reports(world: &World) {
    if let Some(mut pending) = world.try_resource_mut::<PendingPackageTransitions>() {
        pending.0.clear();
    }
    if let Some(mut pending) = world.try_resource_mut::<PendingPackageEnds>() {
        pending.0.clear();
    }
    if let Some(mut active) = world.try_resource_mut::<ActivePackages>() {
        active.0.clear();
    }
}

/// Run the package fragments of this frame's transitions.
pub fn package_fragment_system(world: &World, dt: f32) {
    let transitions = match world.try_resource_mut::<PendingPackageTransitions>() {
        Some(mut pending) if !pending.0.is_empty() => std::mem::take(&mut pending.0),
        _ => Vec::new(),
    };
    let ends = match world.try_resource_mut::<PendingPackageEnds>() {
        Some(mut pending) if !pending.0.is_empty() => std::mem::take(&mut pending.0),
        _ => Vec::new(),
    };
    if transitions.is_empty() && ends.is_empty() {
        return;
    }
    // M47.3 — the ObScript `OnPackage*` blocks of the actors that changed package.
    crate::obscript_objects::deliver_package_events(world, &transitions, &ends, dt);
    let fragments = world.resource::<PackageFragments>().clone();
    log::debug!(
        "{} package transition(s), {} completion(s), {} package(s) with fragments",
        transitions.len(),
        ends.len(),
        fragments.len()
    );
    if fragments.is_empty() {
        return;
    }
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world);
    // (actor, package, event) in report order: completions first (they
    // happened during the frame the transitions were decided after).
    let mut runs: Vec<(EntityId, u32, PackageFragmentEvent)> = Vec::new();
    for end in ends {
        runs.push((end.actor, end.package, PackageFragmentEvent::End));
    }
    for transition in transitions {
        if let Some(from) = transition.from {
            runs.push((transition.actor, from, PackageFragmentEvent::Change));
        }
        if let Some(to) = transition.to {
            runs.push((transition.actor, to, PackageFragmentEvent::Begin));
        }
    }
    let mut advances = Vec::new();
    for (entity, package, event) in runs {
        let actor = if Some(entity) == player {
            Some(0x14)
        } else {
            crate::condition::entity_global_form_id(world, entity)
        };
        let Some(actor) = actor else {
            continue;
        };
        let Some(set) = fragments.get(package) else {
            continue;
        };
        for (_, script, function) in set.fragments.iter().filter(|(on, _, _)| *on == event) {
            log::debug!("package {package:08X} {event:?} on {actor:08X}: {script}.{function}");
            let effect = Effect::Interpret {
                script: script.clone(),
                function: function.clone(),
                args: vec![VmStored::Form(actor)],
                resume: None,
            };
            advances.extend(run_papyrus_call(
                world,
                QuestFormId(package),
                set.vmad.as_ref(),
                effect,
            ));
        }
    }
    flush_advances(world, advances);
}
