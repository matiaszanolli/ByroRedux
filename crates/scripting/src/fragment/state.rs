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
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ReferenceEnableState {
    disabled: HashSet<u32>,
}

impl Resource for ReferenceEnableState {}

impl ReferenceEnableState {
    pub fn is_enabled(&self, form_id: u32) -> bool {
        !self.disabled.contains(&form_id)
    }

    pub fn set_enabled(&mut self, form_id: u32, enabled: bool) {
        if enabled {
            self.disabled.remove(&form_id);
        } else {
            self.disabled.insert(form_id);
        }
    }
}

/// Lowered quest-stage fragments, keyed by `(quest, stage)`. Populated at
/// cell load by [`populate_quest_fragments_from_pex`] from the QUST `VMAD`
/// fragment bindings the decoder recovers; consumed by
/// [`quest_fragment_dispatch_system`].
#[derive(Debug, Clone, Default)]
pub struct QuestStageFragments {
    map: Arc<HashMap<(QuestFormId, u16), Vec<Effect>>>,
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
}

/// `Effect::Activate` targets awaiting delivery as `ActivateEvent`, drained
/// by [`fragment_activation_flush_system`] at the head of the next frame.
///
/// Why a frame of latency instead of a direct insert: `Stage::Update`
/// schedules the `ActivateEvent` consumers (`rumble_on_activate_dispatch`,
/// `quest_advance_system`, `two_state_activator_system`) *before*
/// `quest_fragment_dispatch_system`, because fragment dispatch consumes the
/// `QuestStageAdvanced` markers `quest_advance_system` produces — the order
/// cannot simply be swapped. With `event_cleanup_system` draining the marker
/// at `Stage::Late`, a marker inserted during dispatch was visible to
/// neither this frame's earlier consumers nor the next frame's (#2654).
/// Flushing at the head of the next frame delivers it to all of them,
/// exactly once.
#[derive(Debug, Clone, Default)]
pub struct PendingFragmentActivations(pub(crate) Vec<(EntityId, EntityId)>);

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

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
