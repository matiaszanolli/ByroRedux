//! Fragment runtime state — the resources the dispatch systems read and the
//! interpreter mutates.
//!
//! Split out of the 2,713-line `fragment.rs` (#3854). The module's tests
//! already lived in the sibling `fragment/tests.rs`, so the directory existed
//! with exactly one file in it; the production side simply never followed.

use super::*;

/// Persistent enable/disable state for placed references targeted by
/// Papyrus fragments. Form IDs keep the state valid while the reference's
/// cell is unloaded; cell streaming can consult this resource when spawning
/// enable-parent chains.
///
/// #4813 — three states per reference, not two: scripted-disabled,
/// scripted-enabled, or untouched. "Untouched" defers to the placement's
/// authored "Initially Disabled" flag, which lives on the plugin record, not
/// here; a scripted `Enable()` of such a reference must survive the next
/// cell load, so an explicit enable is recorded rather than merely clearing
/// a disable. Both sets are required fields — a pre-v29 save is rejected by
/// the `FORMAT_MAJOR` gate, never default-filled (#4465).
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ReferenceEnableState {
    disabled: HashSet<u32>,
    enabled: HashSet<u32>,
}

impl Resource for ReferenceEnableState {}

/// M47.2 — every quest's own VMAD (the scripts attached to the QUST and
/// their property bindings), for [`Effect::QuestScope`](crate::Effect): a
/// quest-script function inlined into an INFO or scene fragment resolves its
/// properties here. Behind an `Arc` so a fragment run snapshots it before the
/// quest guards at the cost of a refcount. `NOT_SAVED_BY_DESIGN`: rebuilt from
/// the QUST records every boot.
#[derive(Debug, Default, Clone)]
pub struct QuestScriptInstances(
    pub std::sync::Arc<HashMap<QuestFormId, ScriptInstanceData>>,
);

impl Resource for QuestScriptInstances {}

/// Install the load order's [`QuestScriptInstances`]; returns how many quests
/// carry a VMAD.
pub fn install_quest_script_instances(
    world: &mut World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) -> usize {
    let instances: HashMap<QuestFormId, ScriptInstanceData> = index
        .quests
        .iter()
        .filter_map(|(&form_id, quest)| {
            Some((QuestFormId(form_id), quest.script_instance.clone()?))
        })
        .collect();
    let count = instances.len();
    world.insert_resource(QuestScriptInstances(std::sync::Arc::new(instances)));
    count
}

/// M47.2 — the load order's authored enable data: each reference's own
/// "Initially Disabled" flag (REFR 0x0800) and, for an enable-parent child,
/// its parent (`XESP`). `IsDisabled` / `IsEnabled` guards and the cell
/// loader's spawn gate resolve a reference's state through it with
/// [`reference_enabled_from`]. `NOT_SAVED_BY_DESIGN`: rebuilt from the REFRs
/// every boot.
///
/// The parent is also what ObScript's `GetParentRef` answers ("the reference
/// set in the Enable Parent tab").
#[derive(Debug, Default, Clone)]
pub struct AuthoredDisabledReferences {
    /// References whose own record carries "Initially Disabled". For a child
    /// the flag is inert: its state is its parent's (#6077).
    pub initially_disabled: HashSet<u32>,
    /// Child -> its `XESP` enable parent.
    pub enable_parents: HashMap<u32, byroredux_plugin::esm::cell::EnableParent>,
}

impl Resource for AuthoredDisabledReferences {}

/// Install the load order's [`AuthoredDisabledReferences`]; returns how many
/// references start disabled once enable parents are followed.
pub fn install_authored_disabled_references(
    world: &mut World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) -> usize {
    let mut authored = AuthoredDisabledReferences::default();
    let cells = index
        .cells
        .cells
        .values()
        .chain(index.cells.exterior_cells.values().flat_map(|grid| grid.values()))
        .chain(index.cells.worldspace_persistent_cells.values());
    for placed in cells.flat_map(|cell| cell.references.iter()) {
        if placed.initially_disabled {
            authored.initially_disabled.insert(placed.form_id);
        }
        if let Some(parent) = placed.enable_parent.filter(|parent| parent.form_id != 0) {
            authored.enable_parents.insert(placed.form_id, parent);
        }
    }
    let count = authored
        .initially_disabled
        .iter()
        .chain(authored.enable_parents.keys())
        .collect::<HashSet<_>>()
        .into_iter()
        .filter(|&&form_id| !reference_enabled_from(None, Some(&authored), form_id, None))
        .count();
    world.insert_resource(authored);
    count
}

/// Enable-parent hops followed before a chain is treated as a cycle. Vanilla
/// chains are short (17 of FNV's 1,876 disabled-parent children have a parent
/// that is itself a child).
const ENABLE_PARENT_MAX_DEPTH: usize = 16;

/// #6077 — a reference's effective enable state. A reference with an enable
/// parent "cannot be enabled or disabled independently of their parent, even
/// through script. Their disable/enable state is always determined by their
/// enable parent" (CK *Reference*, Enable Parent; GECK *Enable*: on such a
/// reference it "will have no effect"), the opposite of it when the child is
/// flagged "Set Enable State to Opposite of Parent". So the chain is followed
/// to its first parent-less reference, whose scripted `Enable()` /
/// `Disable()` wins over its authored flag, and the answer is flipped once
/// per inverted hop. That is also the cascade: a script toggling a parent
/// toggles every child read through here.
///
/// `first_hop` is the reference's own parent when the caller holds the
/// placement (the spawn gate), so a child resolves one hop even with no
/// authored table installed; `None` takes it from `authored`. A parent the
/// table does not hold counts as authored enabled. A cycle resolves to the
/// state the reference's own record gives it.
pub fn reference_enabled_from(
    scripted: Option<&ReferenceEnableState>,
    authored: Option<&AuthoredDisabledReferences>,
    form_id: u32,
    first_hop: Option<byroredux_plugin::esm::cell::EnableParent>,
) -> bool {
    let (root, inverted) = enable_root(authored, form_id, first_hop);
    let enabled = scripted
        .and_then(|state| state.override_for(root))
        .unwrap_or_else(|| !authored.is_some_and(|a| a.initially_disabled.contains(&root)));
    enabled != inverted
}

/// The reference whose own state decides `form_id`'s, and whether an odd
/// number of inverted hops lie between them; `(form_id, false)` for a
/// parent-less reference or a cycle.
fn enable_root(
    authored: Option<&AuthoredDisabledReferences>,
    form_id: u32,
    first_hop: Option<byroredux_plugin::esm::cell::EnableParent>,
) -> (u32, bool) {
    let parent_of = |form_id: u32| {
        authored
            .and_then(|a| a.enable_parents.get(&form_id).copied())
            .filter(|parent| parent.form_id != 0)
    };
    let mut current = form_id;
    let mut inverted = false;
    let mut next = first_hop.filter(|parent| parent.form_id != 0).or_else(|| parent_of(form_id));
    for _ in 0..ENABLE_PARENT_MAX_DEPTH {
        let Some(parent) = next else {
            return (current, inverted);
        };
        inverted ^= parent.inverted;
        current = parent.form_id;
        next = parent_of(current);
    }
    log::debug!("enable parent chain from {form_id:08X} exceeds {ENABLE_PARENT_MAX_DEPTH} hops; reading its own state");
    (form_id, false)
}

/// Whether reference `form_id` is enabled, with enable parents followed
/// ([`reference_enabled_from`]). Each guard is released before the next.
pub fn reference_enabled(world: &World, form_id: u32) -> bool {
    reference_enabled_with_parent(world, form_id, None)
}

/// [`reference_enabled`] with the reference's own parent supplied by a caller
/// that holds the placement (see [`reference_enabled_from`]'s `first_hop`).
pub fn reference_enabled_with_parent(
    world: &World,
    form_id: u32,
    first_hop: Option<byroredux_plugin::esm::cell::EnableParent>,
) -> bool {
    let (root, inverted) = {
        let authored = world.try_resource::<AuthoredDisabledReferences>();
        enable_root(authored.as_deref(), form_id, first_hop)
    };
    let scripted = world
        .try_resource::<ReferenceEnableState>()
        .and_then(|state| state.override_for(root));
    let enabled = scripted.unwrap_or_else(|| {
        !world
            .try_resource::<AuthoredDisabledReferences>()
            .is_some_and(|authored| authored.initially_disabled.contains(&root))
    });
    enabled != inverted
}

/// References whose default activation a script blocked (M47.2): the CK's
/// `BlockActivation` — "blocks, or unblocks, normal activation processing for
/// this reference. References with blocked activation will still send
/// OnActivate events to scripts attached to them." Keyed by reference FormID
/// so the block outlives the cell; saved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ReferenceActivationBlocks {
    blocked: HashSet<u32>,
}

impl Resource for ReferenceActivationBlocks {}

impl ReferenceActivationBlocks {
    pub fn set_blocked(&mut self, reference: u32, blocked: bool) {
        if blocked {
            self.blocked.insert(reference);
        } else {
            self.blocked.remove(&reference);
        }
    }

    pub fn is_blocked(&self, reference: u32) -> bool {
        self.blocked.contains(&reference)
    }
}

/// Whether `entity`'s default activation (a door's transition, a pickup, a
/// container, a conversation) is script-blocked. Its `OnActivate` handlers
/// still run.
pub fn default_activation_blocked(world: &World, entity: EntityId) -> bool {
    // `Activate(…, true)` "will ignore the blocked flag" (CK).
    if world.has::<crate::NativeActivation>(entity) {
        return false;
    }
    // A legacy reference whose script has an `OnActivate` block: the block runs
    // *instead of* default processing, which only its `Activate` command (a
    // native-only activation, above) performs.
    if crate::obscript_objects::has_activate_block(world, entity) {
        return true;
    }
    let Some(reference) = crate::condition::entity_global_form_id(world, entity) else {
        return false;
    };
    world
        .try_resource::<ReferenceActivationBlocks>()
        .is_some_and(|blocks| blocks.is_blocked(reference))
}

/// A fragment's `Game.GetPlayer().MoveTo(<reference>)` (M47.2): the
/// destination reference's global FormID. The engine drains it each frame —
/// moving the player in place when the reference is resident, otherwise
/// transitioning to the reference's cell at its placement. Not saved: it
/// lives for at most one frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PlayerMoveRequest(pub Option<u32>);

impl Resource for PlayerMoveRequest {}

/// One reference's scripted lock state, as recorded by
/// [`ReferenceLockState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum LockOverride {
    /// A script locked it. Mirrors `byroredux_core`'s `Locked` payload,
    /// carried by value rather than as that component so the component
    /// stays free of a serde derive it does not otherwise need — `Locked`
    /// remains correctly rederived-every-load, just from a source this
    /// ledger can override.
    Locked {
        lock_level: u8,
        key_form_id: Option<u32>,
    },
    /// A script unlocked it. Distinct from "absent": absent means no
    /// script has touched this reference, so the plugin's authored `XLOC`
    /// stands.
    Unlocked,
}

/// Persistent scripted lock/unlock state for placed references, keyed by
/// **local** form ID — the same keying [`ReferenceEnableState`] uses, and
/// for the same reason.
///
/// #4136. `Effect::SetLocked` / `Effect::SetLockLevel` (#3159) made
/// `Locked` runtime-mutable, but the mutation lived only on the component.
/// That loses the change twice over:
///
/// - across a save/load, because `Locked` is not a registered column; and
/// - across an ordinary in-session cell revisit, because leaving the cell
///   despawns the entity outright and the loader re-stamps the plugin's
///   authored `XLOC` onto a brand-new placement root.
///
/// The second is why this is a resource rather than a saved component. A
/// component cannot survive its own entity's despawn, so registering
/// `Locked` for save would have fixed the save/load half and left a player
/// who picks a lock and walks back through the door facing it locked
/// again. A form-ID-keyed ledger outlives the entity, which is exactly the
/// shape `ReferenceEnableState` already uses for `Enable`/`Disable`
/// (#3278/#3789) — `cell_loader::spawn` consults it at the stamp the same
/// way `placement_is_disabled` consults its sibling.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ReferenceLockState {
    overrides: HashMap<u32, LockOverride>,
}

impl Resource for ReferenceLockState {}

impl ReferenceLockState {
    /// Record that a script locked `form_id`.
    pub fn set_locked(&mut self, form_id: u32, lock_level: u8, key_form_id: Option<u32>) {
        self.overrides.insert(
            form_id,
            LockOverride::Locked {
                lock_level,
                key_form_id,
            },
        );
    }

    /// Record that a script unlocked `form_id`.
    pub fn set_unlocked(&mut self, form_id: u32) {
        self.overrides.insert(form_id, LockOverride::Unlocked);
    }

    /// Update only the difficulty of an already-recorded lock, mirroring
    /// `Effect::SetLockLevel`'s "never locks or unlocks" contract.
    ///
    /// A no-op when the reference is scripted-*unlocked* (there is no lock
    /// record to carry a difficulty on) and when no override exists yet
    /// (#4329). In the second case the plugin's authored lock is still in
    /// force, but its key is known only to the live component, so recording
    /// a lock here would invent `key_form_id: None` — and a ledger entry
    /// outranks the authored `XLOC` on the next load. The dispatcher records
    /// a first difficulty change as the component's full outcome through
    /// [`Self::set_locked`] instead.
    pub fn set_lock_level(&mut self, form_id: u32, lock_level: u8) {
        if let Some(LockOverride::Locked { lock_level: l, .. }) = self.overrides.get_mut(&form_id) {
            *l = lock_level;
        }
    }

    /// The scripted override for `form_id`, if any. `None` means no
    /// script has touched this reference and the authored `XLOC` stands.
    pub fn override_for(&self, form_id: u32) -> Option<LockOverride> {
        self.overrides.get(&form_id).copied()
    }

    /// Number of references carrying a scripted override. Diagnostic.
    pub fn len(&self) -> usize {
        self.overrides.len()
    }

    /// Returns `true` when no script has changed any lock.
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }
}

impl ReferenceEnableState {
    /// Whether no script has disabled this reference. Ignores the authored
    /// "Initially Disabled" default — spawn paths use
    /// [`Self::is_enabled_with_default`].
    pub fn is_enabled(&self, form_id: u32) -> bool {
        self.override_for(form_id).unwrap_or(true)
    }

    /// The scripted enable state, or `None` when no script has touched this
    /// reference and its authored state stands (#4813).
    pub fn override_for(&self, form_id: u32) -> Option<bool> {
        if self.disabled.contains(&form_id) {
            Some(false)
        } else if self.enabled.contains(&form_id) {
            Some(true)
        } else {
            None
        }
    }

    /// Effective enable state of a placement whose record carries
    /// `initially_disabled`: a scripted `Enable()`/`Disable()` wins, else
    /// the authored flag (#4813).
    pub fn is_enabled_with_default(&self, form_id: u32, initially_disabled: bool) -> bool {
        self.override_for(form_id).unwrap_or(!initially_disabled)
    }

    pub fn set_enabled(&mut self, form_id: u32, enabled: bool) {
        if enabled {
            self.disabled.remove(&form_id);
            self.enabled.insert(form_id);
        } else {
            self.enabled.remove(&form_id);
            self.disabled.insert(form_id);
        }
    }
}

/// #4334 — references whose attached once-only Papyrus script has parked
/// itself in a terminal state via `GotoState` (vanilla
/// `defaultSetStageTRIGSpecificActor`'s `onlyOnce` →
/// `GotoState("hasBeenTriggered")`, an empty state that keeps the
/// reference enabled). Recording `Disable()` here would be wrong —
/// vanilla never disables the reference — so this ledger exists
/// separately from [`ReferenceEnableState`]. Keyed by the same
/// local form id, for the same reason: the state must outlive the cell
/// unload that despawns the entity, and the attach path consults it so
/// the recognizer does not re-arm an already-fired trigger on the next
/// cell load or save load.
///
/// Deliberately coarse (per reference, not per (reference, script)
/// pair): the once-only family attaches exactly one trigger script per
/// reference. A reference carrying several scripts where only one parks
/// would need a keyed refinement — the consult site
/// (`recognize_specific_actor_trigger`'s spawn closure) is the place to
/// tighten if that content ever lands.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ReferenceScriptState {
    script_parked: HashSet<u32>,
}

impl Resource for ReferenceScriptState {}

impl ReferenceScriptState {
    /// Record that this reference's once-only script reached its
    /// terminal state — the trigger must not re-arm on reload.
    pub fn park(&mut self, reference_form_id: u32) {
        self.script_parked.insert(reference_form_id);
    }

    /// Whether this reference's script already reached its terminal
    /// state. Consulted by the attach path before re-inserting a
    /// once-only recognizer's component.
    pub fn is_parked(&self, reference_form_id: u32) -> bool {
        self.script_parked.contains(&reference_form_id)
    }

    /// Clear the parked record (a script explicitly returning to its
    /// auto state would call this; no vanilla content does).
    pub fn rearm(&mut self, reference_form_id: u32) {
        self.script_parked.remove(&reference_form_id);
    }
}

/// Lowered quest-stage fragments, keyed by `(quest, stage)`. Populated at
/// cell load by [`populate_quest_fragments_from_pex`] from the QUST `VMAD`
/// fragment bindings the decoder recovers; consumed by
/// [`quest_fragment_dispatch_system`].
#[derive(Debug, Clone, Default)]
pub struct QuestStageFragments {
    map: Arc<HashMap<(QuestFormId, u16), Vec<Effect>>>,
    /// M47.2 — the scripts the Papyrus VM runs (`Effect::Interpret`
    /// fragments and the quest scripts they call), by lowercased name.
    vm_scripts: Arc<HashMap<String, Arc<byroredux_papyrus::ast::Script>>>,
    /// The QF_ script's own property table per quest — the same VMAD
    /// scripts-section bytes the QUST record's fragment section is read
    /// alongside. Lets a fragment's cross-quest `Property`-targeted
    /// effect (`SomeOtherQuest.SetStage(..)` via a `Quest Property`)
    /// resolve at dispatch time instead of always skipping.
    vmad: Arc<HashMap<QuestFormId, ScriptInstanceData>>,
    /// Whether the QUST walk that fills the two maps above has already run
    /// for this session.
    ///
    /// #3161 — callers used to gate the walk on `is_empty()`, which reads
    /// only `map`. But the walk fills `map` and `vmad` independently:
    /// `insert_vmad` runs for every scripted quest before any `.pex` is
    /// resolved, while `insert` runs only on a successful lowering. A
    /// session where the VMAD side populates but no `QF_` `.pex` resolves —
    /// a wrong or missing `--scripts-bsa`, exactly what the smoke harness's
    /// WARN text anticipates — leaves `map` empty forever, so the full
    /// 845-quest walk, with a per-quest `HashMap` build and an archive
    /// `extract_pex` per script name, re-ran on every exterior cell
    /// `begin` for the rest of the session. A dedicated latch also stays
    /// correct in the legitimately-empty cases (pre-Papyrus game, no script
    /// archive) where both maps end empty and `is_empty()` would re-walk
    /// just the same.
    populated: bool,
}

impl Resource for QuestStageFragments {}

impl QuestStageFragments {
    /// M47.2 — register a script the Papyrus VM runs.
    pub fn insert_vm_script(&mut self, script: Arc<byroredux_papyrus::ast::Script>) {
        Arc::make_mut(&mut self.vm_scripts).insert(script.name.node.0.to_ascii_lowercase(), script);
    }

    /// M47.2 — every script the Papyrus VM runs.
    pub fn vm_scripts(&self) -> Arc<HashMap<String, Arc<byroredux_papyrus::ast::Script>>> {
        Arc::clone(&self.vm_scripts)
    }

    /// Register a stage's lowered fragment effects.
    pub fn insert(&mut self, quest: QuestFormId, stage: u16, effects: Vec<Effect>) {
        Arc::make_mut(&mut self.map).insert((quest, stage), effects);
    }

    /// The lowered effects for a `(quest, stage)`, if any.
    pub fn get(&self, quest: QuestFormId, stage: u16) -> Option<&[Effect]> {
        self.map.get(&(quest, stage)).map(Vec::as_slice)
    }

    /// Register a quest's own VMAD scripts section (its declared
    /// property bindings), so `Property`-targeted effects in its
    /// fragments can resolve. A no-op for a VMAD with no attached
    /// scripts — nothing a `Property` lookup could ever match.
    pub fn insert_vmad(&mut self, quest: QuestFormId, vmad: ScriptInstanceData) {
        if vmad.has_script() {
            Arc::make_mut(&mut self.vmad).insert(quest, vmad);
        }
    }

    /// The registered VMAD for `quest`, if any.
    pub fn vmad(&self, quest: QuestFormId) -> Option<&ScriptInstanceData> {
        self.vmad.get(&quest)
    }

    /// Number of registered stage fragments.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Whether the QUST fragment walk has already run this session — see
    /// [`Self::populated`]. Callers deciding whether to walk must use this
    /// rather than [`Self::is_empty`].
    pub fn is_populated(&self) -> bool {
        self.populated
    }

    /// Latch the walk as done, whatever it found. Idempotent.
    pub fn mark_populated(&mut self) {
        self.populated = true;
    }
}

/// Lowered Papyrus fragments attached to authored `SCEN` lifecycle events.
///
/// A scene event has at most one fragment binding in the VMAD format, so the
/// canonical key is `(scene FormID, event)`. The value retains the owning
/// quest and scene VMAD property table needed by the shared effect executor.
#[derive(Debug, Clone, Default)]
pub struct SceneFragments {
    map: Arc<HashMap<(u32, SceneFragmentEvent), SceneFragmentEffects>>,
}

impl Resource for SceneFragments {}

#[derive(Debug, Clone)]
pub(crate) struct SceneFragmentEffects {
    pub(crate) context: QuestFormId,
    pub(crate) vmad: Option<ScriptInstanceData>,
    pub(crate) effects: Vec<Effect>,
}

impl SceneFragments {
    pub fn insert(
        &mut self,
        scene_form_id: u32,
        event: SceneFragmentEvent,
        context: QuestFormId,
        vmad: Option<ScriptInstanceData>,
        effects: Vec<Effect>,
    ) {
        Arc::make_mut(&mut self.map).insert(
            (scene_form_id, event),
            SceneFragmentEffects {
                context,
                vmad,
                effects,
            },
        );
    }

    pub(crate) fn get(
        &self,
        scene_form_id: u32,
        event: SceneFragmentEvent,
    ) -> Option<&SceneFragmentEffects> {
        self.map.get(&(scene_form_id, event))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Lowered Papyrus fragments attached to authored `INFO` responses — the
/// `TIF_` topic-info scripts (#5152). Keyed by the INFO's FormID; the value
/// carries the owning quest (the topic's first `quest_ref`, the same
/// precedence the dialogue selection uses) and the INFO's own VMAD property
/// table so the shared effect executor resolves `Property`-targeted effects
/// exactly as the quest/scene dispatchers do.
///
/// The OnBegin binding runs when the response is selected (spoken) — the
/// activation- and topic-click paths both route through that selection. The
/// OnEnd binding runs when the line stops being the spoken one: another
/// selection replaces it, or the conversation surface closes (the same
/// dispatch path as OnBegin; `byroredux`'s npc_dialogue
/// `speak_info_end_fragment`, #5152). INFOs with neither binding stay inert
/// (logged at dispatch-lookup time by the populate walk's absent-begin
/// diagnostic), never guessed.
#[derive(Debug, Clone, Default)]
pub struct DialogueInfoFragments {
    map: Arc<HashMap<u32, InfoFragmentEffects>>,
    /// Session latch for the populate walk — same rationale as
    /// [`QuestStageFragments::populated`]: a legitimately-empty table (no
    /// `--scripts-bsa`, pre-Papyrus game) must not re-walk every cell load.
    populated: bool,
}

impl Resource for DialogueInfoFragments {}

#[derive(Debug, Clone)]
pub(crate) struct InfoFragmentEffects {
    pub(crate) context: QuestFormId,
    pub(crate) vmad: Option<ScriptInstanceData>,
    pub(crate) begin: Option<Vec<Effect>>,
    pub(crate) end: Option<Vec<Effect>>,
}

impl DialogueInfoFragments {
    /// Register one INFO's lowered fragment effects. `None` sides stay
    /// absent — an OnBegin-only INFO (the common quest-driving shape)
    /// inserts with `end: None`.
    pub fn insert(
        &mut self,
        info_form_id: u32,
        context: QuestFormId,
        vmad: Option<ScriptInstanceData>,
        begin: Option<Vec<Effect>>,
        end: Option<Vec<Effect>>,
    ) {
        Arc::make_mut(&mut self.map).insert(
            info_form_id,
            InfoFragmentEffects {
                context,
                vmad,
                begin,
                end,
            },
        );
    }

    pub(crate) fn get(&self, info_form_id: u32) -> Option<&InfoFragmentEffects> {
        self.map.get(&info_form_id)
    }

    /// The OnBegin (`begin = true`) or OnEnd binding of one INFO, packaged
    /// for dispatch: the lowered effects, the owning quest (the effect
    /// context), and the INFO's own VMAD property table. `None` when the
    /// table has no entry for the line or the line carries no such
    /// binding. The bin's dialogue system dispatches through this —
    /// `InfoFragmentEffects` stays crate-private.
    pub fn spoken_effects(
        &self,
        info_form_id: u32,
        begin: bool,
    ) -> Option<(Vec<Effect>, QuestFormId, Option<ScriptInstanceData>)> {
        let entry = self.get(info_form_id)?;
        let effects = if begin {
            entry.begin.as_ref()?
        } else {
            entry.end.as_ref()?
        };
        Some((effects.clone(), entry.context, entry.vmad.clone()))
    }

    /// Number of registered INFO fragment entries.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Whether the INFO fragment walk has already run this session.
    pub fn is_populated(&self) -> bool {
        self.populated
    }

    /// Latch the walk as done, whatever it found. Idempotent.
    pub fn mark_populated(&mut self) {
        self.populated = true;
    }
}

/// One suspended latent fragment tail. The VMAD snapshot is retained so a
/// continuation resolves properties exactly as the original dispatch did,
/// even if the installed fragment table changes before the wait expires.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct PendingFragmentExecution {
    pub(crate) context: QuestFormId,
    pub(crate) vmad: Option<ScriptInstanceData>,
    pub(crate) effects: Vec<Effect>,
    pub(crate) remaining_seconds: f32,
    pub(crate) resume_when: FragmentResumeCondition,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub(crate) enum FragmentResumeCondition {
    DelayElapsed,
    Actors3DLoaded {
        actors: Vec<ActorRef>,
        poll_seconds: f32,
        /// Total seconds this entry has spent re-polling with the actors
        /// still unresolved. #2288 (SCR-D6-NEW5-02) — capped by
        /// [`MAX_ACTORS_3D_LOADED_WAIT_SECONDS`] so a permanently-unloadable
        /// alias target (or a quest that was reset out from under the
        /// suspended tail) doesn't retry forever with no eviction path,
        /// unlike `quest_fragment_dispatch_system`'s bounded `MAX_CASCADE`.
        elapsed_seconds: f32,
    },
}

/// Maximum total time (across every re-poll) a suspended
/// `WaitForActors3DLoaded` continuation is allowed to wait before the
/// tail is declined outright. #2288 (SCR-D6-NEW5-02) — real content polls
/// on the order of tenths of a second, so 30s is generous headroom while
/// still guaranteeing every entry eventually leaves the queue.
pub(crate) const MAX_ACTORS_3D_LOADED_WAIT_SECONDS: f32 = 30.0;
pub(crate) const MAX_PROVIDER_FRAGMENT_BARRIERS: usize = 64;
/// #5681/#5984 — bound on `FragmentExecutionQueue` entries, the fragment-path
/// sibling of the provider queue's `MAX_PROVIDER_CONTINUATIONS`. Overflow
/// drops the *newest* tails (the entries pushed latest), so a mod that
/// re-fires an INFO or scene fragment per player action cannot grow the
/// queue — saved with the world — without limit. Real cells park single
/// digits; 4 096 is orders of magnitude above any legitimate backlog.
pub(crate) const MAX_FRAGMENT_CONTINUATIONS: usize = 4_096;

/// Runtime queue for latent time waits and bounded-work `Is3DLoaded` polling
/// continuations.
///
/// #2381 (SAVE-D1-16) — registered in
/// `byroredux::save_io::build_save_registry`. A suspended tail's remaining
/// effects and resume condition exist only here; a save taken mid-
/// `Utility.Wait`/`WaitForActors3DLoaded` previously dropped the pending
/// continuation silently on load.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct FragmentExecutionQueue {
    pub(crate) pending: Vec<PendingFragmentExecution>,
}

impl Resource for FragmentExecutionQueue {}

impl FragmentExecutionQueue {
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Queue one continuation unless the queue is at
    /// [`MAX_FRAGMENT_CONTINUATIONS`]; the *newest* entry is the one dropped
    /// (#5984). Every producer goes through here (or [`Self::extend_capped`]):
    /// a zero-delay budget deferral, a custom-event delivery or an
    /// `Interpret` yield is just as able to flood the saved queue as a timed
    /// wait. Returns whether the entry was kept.
    pub(crate) fn push(&mut self, entry: PendingFragmentExecution) -> bool {
        if self.pending.len() >= MAX_FRAGMENT_CONTINUATIONS {
            log::warn!(
                "fragment continuation queue is full ({MAX_FRAGMENT_CONTINUATIONS}); \
                 dropping the newest tail"
            );
            return false;
        }
        self.pending.push(entry);
        true
    }

    /// [`Self::push`] for a batch, dropping whatever does not fit.
    pub(crate) fn extend_capped(&mut self, entries: impl IntoIterator<Item = PendingFragmentExecution>) {
        let mut dropped = 0usize;
        for entry in entries {
            if self.pending.len() < MAX_FRAGMENT_CONTINUATIONS {
                self.pending.push(entry);
            } else {
                dropped += 1;
            }
        }
        if dropped > 0 {
            log::warn!(
                "fragment continuation queue exceeded {MAX_FRAGMENT_CONTINUATIONS}; \
                 dropping {dropped} newest tails"
            );
        }
    }
}

/// `Effect::Activate` targets awaiting delivery as `ActivateEvent`, drained
/// by [`fragment_activation_flush_system`] at the head of the next frame.
///
/// Why a frame of latency instead of a direct insert: `Stage::Update`
/// schedules the `ActivateEvent` consumers (`container_loot_system`,
/// `rumble_on_activate_dispatch`, `quest_advance_system`,
/// `two_state_activator_system`, `mg07_on_activate_system`) *before*
/// `quest_fragment_dispatch_system`, because fragment dispatch consumes the
/// `QuestStageAdvanced` markers `quest_advance_system` produces — the order
/// cannot simply be swapped. With `event_cleanup_system` draining the marker
/// at `Stage::Late`, a marker inserted during dispatch was visible to
/// neither this frame's earlier consumers nor the next frame's (#2654).
/// Flushing at the head of the next frame delivers it to all of them,
/// exactly once.
#[derive(Debug, Clone, Default)]
pub struct PendingFragmentActivations(
    pub(crate) Vec<(EntityId, EntityId)>,
    /// The ones raised by `Activate(…, true)`: default processing only, flagged
    /// with [`crate::NativeActivation`] when delivered.
    pub(crate) Vec<(EntityId, EntityId)>,
);

impl PendingFragmentActivations {
    /// Queue a default-processing-only activation of `target` by `activator`
    /// (delivered next frame with [`crate::NativeActivation`]). `false` when the
    /// queue was never installed.
    pub(crate) fn queue_native(world: &World, target: EntityId, activator: EntityId) -> bool {
        match world.try_resource_mut::<Self>() {
            Some(mut pending) => {
                pending.1.push((target, activator));
                true
            }
            None => false,
        }
    }
}

impl Resource for PendingFragmentActivations {}

impl PendingFragmentActivations {
    /// Queue one activation for delivery at the head of the next frame.
    ///
    /// The supported way for any producer that runs *after* an
    /// `ActivateEvent` consumer to reach every consumer exactly once. A
    /// direct `ActivateEvent` insert from such a producer is delivered to
    /// whichever consumers happen to be scheduled later and drained by
    /// `event_cleanup_system` the same frame (#2654 for quest fragments,
    /// #3936 for scene-package `Activate` procedures).
    pub fn push(&mut self, target: EntityId, activator: EntityId) {
        self.0.push((target, activator));
    }

    /// Queue an activation that runs the engine's default processing only
    /// (`Activate(akActivator, true)`): the scripts hear no `OnActivate` and the
    /// block flag is ignored.
    pub fn push_native(&mut self, target: EntityId, activator: EntityId) {
        self.1.push((target, activator));
    }

    pub fn len(&self) -> usize {
        self.0.len() + self.1.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty() && self.1.is_empty()
    }
}

/// M47.2 — every IDLE FormID an installed fragment's `PlayIdle` names
/// (quest-stage, scene and topic-info fragments, `If` branches included),
/// resolved through each fragment's VMAD. The app decodes those IDLEs' HKX
/// clips at cell load, so a scripted `PlayIdle` is playable without an eager
/// decode of every IDLE in the load order.
pub fn fragment_idle_forms(world: &World) -> HashSet<u32> {
    fn collect(effects: &[Effect], vmad: Option<&ScriptInstanceData>, out: &mut HashSet<u32>) {
        for effect in effects {
            match effect {
                Effect::PlayIdle { idle, .. } => {
                    if let Some(form_id) = resolve_property_form_id(vmad, idle.property_name()) {
                        out.insert(form_id);
                    }
                }
                Effect::Conditional {
                    then_effects,
                    else_effects,
                    ..
                } => {
                    collect(then_effects, vmad, out);
                    collect(else_effects, vmad, out);
                }
                _ => {}
            }
        }
    }
    let mut out = HashSet::new();
    if let Some(quests) = world.try_resource::<QuestStageFragments>() {
        for ((quest, _stage), effects) in quests.map.iter() {
            collect(effects, quests.vmad.get(quest), &mut out);
        }
    }
    if let Some(scenes) = world.try_resource::<SceneFragments>() {
        for fragment in scenes.map.values() {
            collect(&fragment.effects, fragment.vmad.as_ref(), &mut out);
        }
    }
    if let Some(infos) = world.try_resource::<DialogueInfoFragments>() {
        for fragment in infos.map.values() {
            for effects in [&fragment.begin, &fragment.end].into_iter().flatten() {
                collect(effects, fragment.vmad.as_ref(), &mut out);
            }
        }
    }
    out
}

#[cfg(test)]
mod reference_lock_state_tests {
    use super::{LockOverride, ReferenceLockState};

    const DOOR: u32 = 0x0001_ABCD;

    /// #4136 — absent means "no script has touched it", which is what lets
    /// the spawn path fall back to the plugin's authored XLOC. It must be
    /// distinguishable from a recorded unlock.
    #[test]
    fn absent_is_distinct_from_scripted_unlocked() {
        let mut state = ReferenceLockState::default();
        assert_eq!(state.override_for(DOOR), None, "untouched");
        state.set_unlocked(DOOR);
        assert_eq!(
            state.override_for(DOOR),
            Some(LockOverride::Unlocked),
            "a scripted unlock must be recorded, not merely left absent — \
             absent would let the cell loader re-stamp the authored lock"
        );
    }

    #[test]
    fn a_scripted_lock_records_its_level_and_key() {
        let mut state = ReferenceLockState::default();
        state.set_locked(DOOR, 75, Some(0x1234));
        assert_eq!(
            state.override_for(DOOR),
            Some(LockOverride::Locked {
                lock_level: 75,
                key_form_id: Some(0x1234)
            })
        );
    }

    /// #4136 — `SetLockLevel` sets difficulty and must never lock or
    /// unlock, mirroring `Effect::SetLockLevel`'s own contract. Applying it
    /// to a scripted-unlocked reference is a no-op, not an implicit lock.
    #[test]
    fn set_lock_level_never_locks_an_unlocked_reference() {
        let mut state = ReferenceLockState::default();
        state.set_unlocked(DOOR);
        state.set_lock_level(DOOR, 100);
        assert_eq!(
            state.override_for(DOOR),
            Some(LockOverride::Unlocked),
            "raising the difficulty of an unlocked door must not re-lock it"
        );
    }

    #[test]
    fn set_lock_level_updates_an_existing_lock_in_place() {
        let mut state = ReferenceLockState::default();
        state.set_locked(DOOR, 25, Some(0x1234));
        state.set_lock_level(DOOR, 100);
        assert_eq!(
            state.override_for(DOOR),
            Some(LockOverride::Locked {
                lock_level: 100,
                key_form_id: Some(0x1234)
            }),
            "the key must survive a difficulty change"
        );
    }

    /// #4329 — an untouched reference is still under its authored lock,
    /// whose key this ledger cannot see, so a bare level records nothing
    /// rather than a keyless lock that would outrank the authored `XLOC` on
    /// reload. This used to record `Locked { key_form_id: None }`.
    #[test]
    fn set_lock_level_on_an_untouched_reference_records_nothing() {
        let mut state = ReferenceLockState::default();
        state.set_lock_level(DOOR, 50);
        assert_eq!(state.override_for(DOOR), None);
    }

    #[test]
    fn the_ledger_starts_empty_so_untouched_worlds_keep_authored_locks() {
        let state = ReferenceLockState::default();
        assert!(state.is_empty());
        assert_eq!(state.len(), 0);
    }
}

