//! Animation graphs of placed objects (M47.2): `PlayAnimation`,
//! `PlayAnimationAndWait`, `RegisterForAnimationEvent` and
//! `SetAnimationVariable*`.
//!
//! `ObjectReference.PlayAnimation(event)` "sends the named event to the object's
//! animation graph" (CK *PlayAnimation*). Skyrim drives levers, traps, doors and
//! rigged props that way from a Havok behavior graph: a state machine whose
//! transitions fire on named events and whose states each play a clip. The NIF
//! names the graph (`BSBehaviorGraphExtraData`, stamped as [`BehaviorGraphRef`]),
//! the binary decodes it (`byroredux_hkx::decode_behavior_graph`) and attaches an
//! [`ObjectAnimationGraph`] with its clips registered in the animation registry.
//!
//! What runs: events switch the machine's state (the state's own transitions
//! first, then the machine's wildcard transitions; disabled transitions never
//! fire), the new state's clip starts on the object's `AnimationPlayer` (single
//! play holds the last pose, looping loops, a user-controlled clip is held at
//! its authored fraction), and the clip's trigger events fire on schedule and
//! drive further transitions — a lever that finishes `PullDown` raises `Pulled`
//! and settles in the "Pulled" state. A state whose generator is a *parametric
//! blender* (an actor's locomotion: walk and run by `Speed`, a heading by
//! `Direction`) plays every clip of its blend tree at once as an `AnimationStack`,
//! weighted from the graph's variables each frame, and a transition with a blending
//! effect fades the old pose out as the new one fades in, and a transition gated on a
//! condition (`iSyncIdleLocomotion == 1`, `!IsPlayer`) fires only while it holds over
//! the variables. Layered (non-parametric) blenders blend by their
//! bone-weight masks, and the modifiers that act on the graph itself run
//! (`animation_modifiers.rs`). Not run: root motion; the pose modifiers
//! (look-at, twist, foot IK, ragdoll, jiggle) and the modifiers whose
//! formula the files do not give; and the `SoundPlayAt.<name>.<node>`
//! node-attached sound form (plain `SoundPlay.<name>` plays).
//!
//! The CK's results apply: `PlayAnimation` returns true when the graph accepted
//! the event; `PlayAnimationAndWait` suspends until the named event is raised
//! (the schedule is deterministic, so the wait is computed up front) and "if the
//! event you send it doesn't exist, the function will never return" — here it
//! returns false at once instead; a reference with no graph answers the way the
//! CK documents a graph that is not loaded (`false`, no wait).
//! `SetAnimationVariable*` / `GetAnimationVariable*` write and read the graph's
//! variables (decoded with their authored initial values); a blender state's weights, a
//! clip's playback speed and a transition's blend duration follow them.

use std::collections::HashMap;
use std::sync::Arc;

use crate::animation_modifiers::{coerce, ModifierState, Vars};

use byroredux_core::animation::{AnimationLayer, AnimationPlayer, AnimationStack};
use byroredux_core::string::FixedString;
use rustc_hash::FxHashMap;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_hkx::{
    BehaviorGraph, Blender, ClipMode, BlendChild, BoneWeights, Generator, ModifierNode, State, StateClip, StateMachine, Transition, VariableKind,
    TRANSITION_FLAG_DISABLED,
};

/// The project file a placed object's NIF names (`BSBehaviorGraphExtraData`),
/// relative to `meshes\`. Stamped by the cell loader; the binary resolves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BehaviorGraphRef(pub Arc<str>);

impl Component for BehaviorGraphRef {
    type Storage = SparseSetStorage<Self>;
}

/// A registered clip of a graph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphClip {
    pub handle: u32,
    /// Seconds, at the clip's authored speed of 1.
    pub duration: f32,
}

/// The `NiControllerSequence`s of a placed object's own NIF, registered in the
/// animation registry (lower-cased name -> clip): what the graph's Gamebryo
/// sequence generators play. Stamped beside [`BehaviorGraphRef`].
#[derive(Debug, Clone, Default)]
pub struct GraphSequences(pub Arc<Vec<(String, GraphClip)>>);

impl Component for GraphSequences {
    type Storage = SparseSetStorage<Self>;
}

/// A decoded behavior graph with its clips registered. Shared by every object
/// that uses the project.
#[derive(Debug, Clone)]
pub struct LoadedGraph {
    pub graph: BehaviorGraph,
    /// `(lower-cased animation path, looping)` -> the registered clip.
    pub clips: HashMap<(String, bool), GraphClip>,
    /// Lower-cased `NiControllerSequence` name -> the registered clip, for the
    /// Gamebryo sequence generators (`BGSGamebryoSequenceGenerator`) a graph
    /// plays from the object's own NIF.
    pub sequences: HashMap<String, GraphClip>,
    /// Clips resolved on demand (an actor's graph names hundreds of clips, and which
    /// file each is depends on the actor's animation folders): the host provides what
    /// the graph asked for. `None` for a graph whose clips are all registered.
    pub lazy: Option<Arc<LazyClips>>,
}

/// Clips a graph asked for and the host has resolved (or could not).
#[derive(Debug, Default)]
pub struct LazyClips {
    ready: std::sync::Mutex<HashMap<(String, bool), GraphClip>>,
    wanted: std::sync::Mutex<Vec<(String, bool)>>,
    missing: std::sync::Mutex<std::collections::HashSet<(String, bool)>>,
    /// Sync points of the provided clips by handle: the times of their `SyncLeft` and
    /// `SyncRight` annotations (see [`sync_time`]).
    sync: std::sync::Mutex<HashMap<u32, [f32; 2]>>,
    /// The skeleton's bones by channel name, for the bone masks of layered blends.
    bones: std::sync::Mutex<Option<Arc<FxHashMap<FixedString, u16>>>>,
}

impl LazyClips {
    fn lookup(&self, key: &(String, bool)) -> Option<GraphClip> {
        if let Some(clip) = self.ready.lock().ok()?.get(key) {
            return Some(*clip);
        }
        let missing = self.missing.lock().ok()?.contains(key);
        if !missing {
            let mut wanted = self.wanted.lock().ok()?;
            if !wanted.contains(key) {
                wanted.push(key.clone());
            }
        }
        None
    }

    /// Whether the graph asked for a clip not yet taken.
    pub fn peek_wanted(&self) -> bool {
        self.wanted.lock().map(|wanted| !wanted.is_empty()).unwrap_or(false)
    }

    /// The `(lower-cased animation path, looping)` pairs the graph asked for since the
    /// last call.
    pub fn take_wanted(&self) -> Vec<(String, bool)> {
        self.wanted.lock().map(|mut wanted| std::mem::take(&mut *wanted)).unwrap_or_default()
    }

    /// The host resolved `key` to `clip`.
    pub fn provide(&self, key: (String, bool), clip: GraphClip) {
        if let Ok(mut ready) = self.ready.lock() {
            ready.insert(key, clip);
        }
    }

    /// The sync points of the clip registered as `handle`.
    pub fn provide_sync(&self, handle: u32, points: [f32; 2]) {
        if let Ok(mut sync) = self.sync.lock() {
            sync.insert(handle, points);
        }
    }

    fn sync_of(&self, handle: u32) -> Option<[f32; 2]> {
        self.sync.lock().ok()?.get(&handle).copied()
    }

    /// The skeleton the graph's clips animate: bone channel name -> bone index, which
    /// the bone weights of a layered blend are addressed by.
    pub fn provide_bones(&self, bones: Arc<FxHashMap<FixedString, u16>>) {
        if let Ok(mut slot) = self.bones.lock() {
            *slot = Some(bones);
        }
    }

    /// Whether the skeleton has been provided.
    pub fn has_bones(&self) -> bool {
        self.bones.lock().map(|bones| bones.is_some()).unwrap_or(false)
    }

    fn bones(&self) -> Option<Arc<FxHashMap<FixedString, u16>>> {
        self.bones.lock().ok()?.clone()
    }

    /// The host could not resolve `key`: it is not asked for again.
    pub fn fail(&self, key: (String, bool)) {
        if let Ok(mut missing) = self.missing.lock() {
            missing.insert(key);
        }
    }
}

impl LoadedGraph {
    fn clip_for<'a>(&self, clip: &'a StateClip) -> Option<(&'a StateClip, GraphClip)> {
        let looping = clip.mode == ClipMode::Looping;
        let key = (clip.animation.to_ascii_lowercase(), looping);
        let registered = match self.clips.get(&key) {
            Some(registered) => *registered,
            None => self.lazy.as_ref()?.lookup(&key)?,
        };
        Some((clip, registered))
    }

    /// Where the clip registered as `handle` has its `SyncLeft` and `SyncRight` points.
    fn sync_of(&self, handle: u32) -> Option<[f32; 2]> {
        self.lazy.as_ref()?.sync_of(handle)
    }

    fn bones(&self) -> Option<Arc<FxHashMap<FixedString, u16>>> {
        self.lazy.as_ref()?.bones()
    }

    fn sequence(&self, name: &str) -> Option<GraphClip> {
        self.sequences.get(&name.to_ascii_lowercase()).copied()
    }
}

/// One clip of a blended pose: how much of the pose it is and how fast it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct PoseLayer {
    pub handle: u32,
    pub weight: f32,
    /// Playback speed multiplier (the clip's, scaled so every layer of an unsynced blend
    /// advances through its own length at the same rate). 0 for a synced layer.
    pub speed: f32,
    /// The clip time a synced layer is at: the graph's shared sync phase mapped through
    /// the clip's own sync points. `None` for a layer that advances by `speed`.
    pub time: Option<f32>,
    /// A layered blend's per-bone weights for this layer (see
    /// [`byroredux_core::animation::AnimationLayer::bone_weights`]).
    pub mask: Option<Arc<Vec<f32>>>,
}

/// How the animation player must run after a state change.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Playback {
    pub handle: u32,
    pub speed: f32,
    /// Clip time the playback starts at.
    pub start_time: f32,
    /// A user-controlled clip: held at this clip time instead of playing.
    pub hold_at: Option<f32>,
    /// Seconds the previous pose fades out over while this one fades in (the
    /// transition's blending effect); 0 switches at once.
    pub fade: f32,
    /// A blended pose (a state whose generator is a blender): one layer per clip of
    /// the blend tree, which replaces `handle`/`speed`. Empty for a single clip.
    pub pose: Vec<PoseLayer>,
    /// The skeleton index the pose's masks are addressed by.
    pub bones: Option<Arc<FxHashMap<FixedString, u16>>>,
}

/// One state machine's current state, and the runs of the nested machines of that
/// state (one per `StateContent::machines`, each in its own current state).
#[derive(Debug, Clone, PartialEq)]
struct MachineRun {
    state: i32,
    nested: Vec<MachineRun>,
}

impl MachineRun {
    /// `machine` in `state`, its nested machines at their start states.
    fn enter(machine: &StateMachine, state: i32, variables: &[f32]) -> Self {
        let nested = machine
            .states
            .iter()
            .find(|candidate| candidate.id == state)
            .map(|state| {
                state
                    .content
                    .machines
                    .iter()
                    .map(|inner| Self::enter(inner, start_state_of(inner, variables), variables))
                    .collect()
            })
            .unwrap_or_default();
        Self { state, nested }
    }
}

/// The state `machine` starts in: the one its `startStateId` variable names (the vanilla
/// graphs pick the idle / locomotion / sneak entry that way), else the authored one.
fn start_state_of(machine: &StateMachine, variables: &[f32]) -> i32 {
    machine
        .start_state_variable
        .and_then(|variable| variables.get(variable))
        .map(|value| *value as i32)
        .filter(|id| machine.states.iter().any(|state| state.id == *id))
        .unwrap_or(machine.start_state)
}

/// An event waiting for its time: raised by a clip trigger of the machine at
/// `path` (indices into the current states' nested machines, from the root).
#[derive(Debug, Clone)]
struct Scheduled {
    at: f32,
    event: i32,
    path: Vec<u16>,
    /// A looping clip's loop length in seconds: the trigger comes round again
    /// that long after it fires, until the machine leaves the state.
    period: Option<f32>,
}

/// One clip (or Gamebryo sequence) of the active generator tree, with everything the pose
/// needs of it: the weight the blenders above give it, the bone mask they apply, whether it
/// is in a synced group, and - for the plain single-clip case - how its generator plays it.
#[derive(Debug, Clone)]
struct Term {
    handle: u32,
    weight: f32,
    /// The clip's playback speed (its bound variable, else authored).
    speed: f32,
    duration: f32,
    /// Where the clip has its `SyncLeft` and `SyncRight` annotations.
    sync: Option<[f32; 2]>,
    /// Whether a synced blender is above it.
    synced: bool,
    mask: Option<Arc<Vec<f32>>>,
    /// The machine whose current state holds it (indices into nested machines).
    path: Vec<u16>,
    sequence: bool,
    start_time: f32,
    /// A user-controlled clip: held at this clip time instead of playing.
    hold_at: Option<f32>,
}

/// The active generator tree: its clips, and the modifiers on the active path.
#[derive(Debug, Default)]
struct Evaluation {
    terms: Vec<Term>,
    modifiers: Vec<Arc<ModifierNode>>,
}

/// What the blenders above a generator contribute to its clips.
#[derive(Debug, Clone)]
struct TermScope {
    weight: f32,
    mask: Option<Arc<Vec<f32>>>,
    synced: bool,
}

impl Default for TermScope {
    fn default() -> Self {
        Self { weight: 1.0, mask: None, synced: false }
    }
}

/// What [`ObjectAnimationGraph::pose_now`] found: the same clips with new weights and
/// times, or a different set of clips that must be started.
#[derive(Debug, Clone, PartialEq)]
pub enum PoseUpdate {
    Layers(Vec<PoseLayer>),
    Restart(Playback),
}

/// A placed object's running behavior graph.
#[derive(Debug, Clone)]
pub struct ObjectAnimationGraph {
    graph: Arc<LoadedGraph>,
    /// The entity that owns the graph and its `AnimationPlayer` (the placement root).
    pub root: EntityId,
    /// The entity whose subtree the clips animate, when it is not `root` (an actor's
    /// skeleton root).
    pub animated_root: Option<EntityId>,
    run: MachineRun,
    /// The graph's own clock, seconds.
    now: f32,
    /// Trigger events waiting for their time, in no particular order.
    scheduled: Vec<Scheduled>,
    /// The graph's variables (`Speed`, `Direction`, ...), by index into
    /// [`BehaviorGraph::variables`]: bool as 0/1, int as its value.
    variables: Vec<f32>,
    /// The longest blend duration among the transitions the last event(s) took.
    pending_fade: f32,
    /// The shared phase of a synced blend: 0 at the clips' `SyncLeft`, 1 at their
    /// `SyncRight`, wrapping at 2.
    sync_phase: f32,
    /// Whether the state the graph is in plays a blended pose (the weights follow the
    /// variables every frame).
    blend_active: bool,
    /// The clips of the pose last started, in order: a change means a new pose.
    applied: Vec<u32>,
    /// A variable changed since the pose was last evaluated.
    dirty: bool,
    /// The modifiers on the active path and what they remember.
    modifiers: ModifierState,
    /// Whether the pose has clips in a synced group (their times are written every frame).
    synced_pose: bool,
    /// Events the graph raised since [`take_raised`](Self::take_raised): the
    /// ones a script's accepted `PlayAnimation` sent and the ones a clip's
    /// triggers fired (a trigger raises its event whether or not a transition
    /// answers it, as an animation notify does). Runtime only.
    raised: Vec<String>,
}

impl Component for ObjectAnimationGraph {
    type Storage = SparseSetStorage<Self>;
}

/// Walk `path` down from the root machine to the machine and run it names.
fn locate<'a>(
    mut machine: &'a StateMachine,
    mut run: &'a MachineRun,
    path: &[u16],
) -> Option<(&'a StateMachine, &'a MachineRun)> {
    for &index in path {
        let state = machine.states.iter().find(|state| state.id == run.state)?;
        machine = state.content.machines.get(usize::from(index))?;
        run = run.nested.get(usize::from(index))?;
    }
    Some((machine, run))
}

/// `run`'s mutable counterpart: the run `path` names.
fn locate_mut<'a>(mut run: &'a mut MachineRun, path: &[u16]) -> Option<&'a mut MachineRun> {
    for &index in path {
        run = run.nested.get_mut(usize::from(index))?;
    }
    Some(run)
}

impl ObjectAnimationGraph {
    /// The graph in its start state, with the start state's clip not yet playing.
    pub fn new(graph: Arc<LoadedGraph>, root: EntityId) -> Option<Self> {
        let machine = graph.graph.machine.as_ref()?;
        let variables: Vec<f32> = graph.graph.variables.iter().map(|variable| variable.initial).collect();
        let run = MachineRun::enter(machine, start_state_of(machine, &variables), &variables);
        Some(Self {
            graph,
            root,
            animated_root: None,
            run,
            now: 0.0,
            scheduled: Vec::new(),
            raised: Vec::new(),
            variables,
            pending_fade: 0.0,
            sync_phase: 0.0,
            blend_active: false,
            applied: Vec::new(),
            dirty: true,
            synced_pose: false,
            modifiers: ModifierState::default(),
        })
    }

    /// The graph animating `animated` (an actor's skeleton root) instead of its owner.
    pub fn with_animated_root(mut self, animated: EntityId) -> Self {
        self.animated_root = Some(animated);
        self
    }

    /// The entity the graph's clips animate.
    pub fn animated(&self) -> EntityId {
        self.animated_root.unwrap_or(self.root)
    }

    /// The root machine's current state.
    pub fn state(&self) -> i32 {
        self.run.state
    }

    pub fn graph(&self) -> &LoadedGraph {
        &self.graph
    }

    /// The shared graph, for building a variant of it (tests).
    #[doc(hidden)]
    pub fn graph_arc(&self) -> Arc<LoadedGraph> {
        Arc::clone(&self.graph)
    }

    /// The value of the graph variable named `name` (case-insensitive): bool as 0/1,
    /// int as its value.
    pub fn variable(&self, name: &str) -> Option<f32> {
        self.variables.get(self.graph.graph.variable_id(name)?).copied()
    }

    /// Every variable with its current value, in graph order.
    pub fn variables(&self) -> impl Iterator<Item = (&str, f32)> {
        self.graph.graph.variables.iter().zip(&self.variables).map(|(variable, value)| (variable.name.as_str(), *value))
    }

    /// Set the graph variable named `name` (`SetAnimationVariable*`); whether the graph
    /// has it. A bool variable keeps 0/1 and an int keeps whole numbers; a variable
    /// of a kind the graph does not evaluate (a vector) is left alone.
    pub fn set_variable(&mut self, name: &str, value: f32) -> bool {
        let Some(index) = self.graph.graph.variable_id(name) else { return false };
        let Some(value) = coerce(self.graph.graph.variables[index].kind, value) else { return false };
        if self.variables[index] != value {
            self.variables[index] = value;
            self.dirty = true;
        }
        true
    }

    /// Run `f` with the graph's variables as the modifiers see them.
    fn with_modifiers<R>(&mut self, f: impl FnOnce(&mut ModifierState, &mut Vars) -> R) -> R {
        let graph = Arc::clone(&self.graph);
        let mut vars = Vars { values: &mut self.variables, dirty: &mut self.dirty, graph: &graph.graph };
        f(&mut self.modifiers, &mut vars)
    }

    /// Bring the active modifiers in line with the active path: new ones activate (their
    /// assignments land), gone ones deactivate. Events they raise go to `events`.
    fn refresh_modifiers(&mut self, events: &mut Vec<i32>) {
        let active = self.evaluate().modifiers;
        self.with_modifiers(|state, vars| state.sync(active, vars, events));
    }

    /// The seconds a taken transition fades the old pose out over: the blending
    /// effect's duration, from its bound variable when it has one.
    fn transition_fade(&self, transition: &Transition) -> f32 {
        let duration = match transition.blend_duration_variable.and_then(|index| self.variables.get(index)) {
            Some(value) => *value,
            None => transition.blend_duration,
        };
        if duration.is_finite() && duration > 0.0 {
            duration
        } else {
            0.0
        }
    }

    /// The clips of the active generator tree (see [`Term`]) under the current variables.
    fn terms(&self) -> Vec<Term> {
        self.evaluate().terms
    }

    /// The active generator tree: its clips and the modifiers on the active path.
    fn evaluate(&self) -> Evaluation {
        let Some(root) = self.graph.graph.machine.as_ref() else { return Evaluation::default() };
        let mut out = Evaluation::default();
        let mut path = Vec::new();
        self.eval_machine(root, &self.run, &mut path, &TermScope::default(), &mut out);
        // A graph that blends a Gamebryo sequence with trigger-only clips plays the
        // sequence: the clips are there for their events.
        if let Some(first) = out.terms.iter().position(|term| term.sequence) {
            let sequence = out.terms.swap_remove(first);
            out.terms = vec![sequence];
        }
        out
    }

    fn eval_machine(
        &self,
        machine: &StateMachine,
        run: &MachineRun,
        path: &mut Vec<u16>,
        scope: &TermScope,
        out: &mut Evaluation,
    ) {
        let Some(state) = machine.states.iter().find(|state| state.id == run.state) else { return };
        match &state.content.generator {
            Some(generator) => self.eval_generator(generator, state, run, path, scope, out),
            // A state built by hand (no decoded tree): its first sequence, else first clip.
            None => {
                let mut leaves = Evaluation::default();
                for clip in state.clip.iter().chain(&state.content.blended_clips) {
                    leaves.terms.extend(self.clip_term(clip, path, scope));
                }
                for name in &state.content.sequences {
                    leaves.terms.extend(self.sequence_term(name, path, scope));
                }
                for (index, inner) in state.content.machines.iter().enumerate() {
                    if let Some(inner_run) = run.nested.get(index) {
                        path.push(index as u16);
                        self.eval_machine(inner, inner_run, path, scope, &mut leaves);
                        path.pop();
                    }
                }
                let chosen = leaves.terms.iter().position(|term| term.sequence).or(if leaves.terms.is_empty() { None } else { Some(0) });
                if let Some(index) = chosen {
                    out.terms.push(leaves.terms.swap_remove(index));
                }
                out.modifiers.extend(leaves.modifiers);
            }
        }
    }

    fn clip_term(&self, clip: &StateClip, path: &[u16], scope: &TermScope) -> Option<Term> {
        let (clip, registered) = self.graph.clip_for(clip)?;
        let hold = (clip.mode == ClipMode::UserControlled).then(|| clip.user_fraction.clamp(0.0, 1.0) * registered.duration);
        let sync = self.graph.sync_of(registered.handle).filter(|_| registered.duration > 0.0);
        Some(Term {
            handle: registered.handle,
            weight: scope.weight,
            speed: clip_speed_of(&self.variables, clip),
            duration: registered.duration,
            sync,
            // Only a clip that has sync points takes part in a synced group; the others
            // play on at their own pace.
            synced: scope.synced && sync.is_some(),
            mask: scope.mask.clone(),
            path: path.to_vec(),
            sequence: false,
            start_time: clip.start_time,
            hold_at: hold,
        })
    }

    fn sequence_term(&self, name: &str, path: &[u16], scope: &TermScope) -> Option<Term> {
        let registered = self.graph.sequence(name)?;
        Some(Term {
            handle: registered.handle,
            weight: scope.weight,
            speed: 1.0,
            duration: registered.duration,
            sync: None,
            synced: false,
            mask: scope.mask.clone(),
            path: path.to_vec(),
            sequence: true,
            start_time: 0.0,
            hold_at: None,
        })
    }

    fn eval_generator(
        &self,
        generator: &Generator,
        state: &State,
        run: &MachineRun,
        path: &mut Vec<u16>,
        scope: &TermScope,
        out: &mut Evaluation,
    ) {
        match generator {
            Generator::Clip(clip) => out.terms.extend(self.clip_term(clip, path, scope)),
            Generator::Sequence(name) => out.terms.extend(self.sequence_term(name, path, scope)),
            Generator::Modified { modifiers, inner } => {
                out.modifiers.extend(modifiers.iter().cloned());
                if let Some(inner) = inner {
                    self.eval_generator(inner, state, run, path, scope, out);
                }
            }
            Generator::Machine(index) => self.eval_nested(state, run, *index, path, scope, out),
            Generator::Reference(ordinal) => {
                if let Some(Some(index)) = state.content.reference_machines.get(*ordinal) {
                    self.eval_nested(state, run, *index, path, scope, out);
                }
            }
            Generator::Selector { index, index_variable, children } => {
                let picked = index_variable.and_then(|variable| self.variables.get(variable)).map_or(*index, |value| *value as i32);
                if let Some(child) = usize::try_from(picked).ok().and_then(|picked| children.get(picked)).or(children.first()) {
                    self.eval_generator(child, state, run, path, scope, out);
                }
            }
            Generator::Blender(blender) => {
                let weights = blender_weights(blender, &self.variables);
                let synced = scope.synced || blender.sync;
                for (child, share) in blender.children.iter().zip(weights) {
                    // A boolean bound to the child's `boneWeights` switches the child
                    // (see [`BlendChild::bone_weights_variable`]).
                    let share = share * self.child_switch(child);
                    let mask = match &child.bone_weights {
                        Some(bones) => Some(combine_masks(scope.mask.as_ref(), &bone_weights_now(bones, &self.variables))),
                        None => scope.mask.clone(),
                    };
                    let inner = TermScope { weight: scope.weight * share, mask, synced };
                    self.eval_generator(&child.generator, state, run, path, &inner, out);
                }
            }
        }
    }

    /// 1 or 0: whether a blender child is switched on by the boolean bound to its
    /// `boneWeights`. A child with no such binding, or a binding to an int or a real (whose
    /// meaning the files do not give), is always on.
    fn child_switch(&self, child: &BlendChild) -> f32 {
        let Some(variable) = child.bone_weights_variable else { return 1.0 };
        let is_bool = self.graph.graph.variables.get(variable).is_some_and(|variable| variable.kind == VariableKind::Bool);
        match (is_bool, self.variables.get(variable)) {
            (true, Some(value)) => f32::from(*value != 0.0),
            _ => 1.0,
        }
    }

    /// The nested machine `index` of `state`, in the state the run has it in.
    fn eval_nested(
        &self,
        state: &State,
        run: &MachineRun,
        index: usize,
        path: &mut Vec<u16>,
        scope: &TermScope,
        out: &mut Evaluation,
    ) {
        let (Some(inner), Some(inner_run)) = (state.content.machines.get(index), run.nested.get(index)) else { return };
        path.push(index as u16);
        self.eval_machine(inner, inner_run, path, scope, out);
        path.pop();
    }

    /// How the clips of `terms` play: one plain clip as before, anything else (a blend, a
    /// layered mask) as a pose that fades in over `fade`.
    fn playback_of(&self, terms: &[Term], fade: f32) -> Option<Playback> {
        let first = terms.first()?;
        if terms.len() == 1 && first.mask.is_none() {
            return Some(match first.hold_at {
                Some(at) => Playback { handle: first.handle, speed: first.speed, start_time: 0.0, hold_at: Some(at), fade, ..Default::default() },
                None => Playback { handle: first.handle, speed: first.speed, start_time: first.start_time, fade, ..Default::default() },
            });
        }
        let eval = pose_layers(terms, self.sync_phase_start());
        Some(Playback {
            handle: first.handle,
            speed: 1.0,
            fade,
            pose: eval.layers,
            bones: self.graph.bones(),
            ..Default::default()
        })
    }

    /// Where a pose's sync phase starts: 0, the clips' `SyncLeft`.
    fn sync_phase_start(&self) -> f32 {
        0.0
    }

    /// The clips a graph plays in order, for telling a changed pose from a re-weighted one.
    fn signature(terms: &[Term]) -> Vec<u32> {
        terms.iter().map(|term| term.handle).collect()
    }

    /// The pose the graph plays right now when it is a blend (the weights move with the
    /// variables), else `None`. A synced blend's shared phase moves on by `dt` first, at
    /// the weighted mean of its clips' rates. When the clips themselves changed without
    /// an event (a selector's variable moved, a machine's start state was picked), the
    /// new pose is returned as a restart.
    pub fn pose_now(&mut self, dt: f32) -> Option<PoseUpdate> {
        // Nothing moves a pose between events except the variables and a synced phase.
        if !self.dirty && !(self.blend_active && self.synced_pose) {
            return None;
        }
        self.dirty = false;
        let terms = self.terms();
        if terms.is_empty() {
            return None;
        }
        if Self::signature(&terms) != self.applied {
            let playback = self.playback_of(&terms, 0.0)?;
            self.applied = Self::signature(&terms);
            self.blend_active = !playback.pose.is_empty();
            self.sync_phase = 0.0;
            self.synced_pose = terms.iter().any(|term| term.synced);
            return Some(PoseUpdate::Restart(playback));
        }
        if !self.blend_active {
            return None;
        }
        let rate = pose_layers(&terms, self.sync_phase).sync_rate;
        self.synced_pose = rate.is_some();
        if let Some(rate) = rate {
            let phase = (self.sync_phase + rate * dt.max(0.0)).rem_euclid(2.0);
            if phase.is_finite() {
                self.sync_phase = phase;
            }
        }
        Some(PoseUpdate::Layers(pose_layers(&terms, self.sync_phase).layers))
    }

    /// The events raised since the last call, oldest first.
    pub fn take_raised(&mut self) -> Vec<String> {
        std::mem::take(&mut self.raised)
    }

    /// Raise an event by name without a transition (tests).
    #[cfg(test)]
    pub(crate) fn push_raised_for_test(&mut self, name: &str) {
        self.raised.push(name.to_owned());
    }

    fn note_raised(&mut self, event: i32) {
        if let Some(name) = self.graph.graph.event_name(event) {
            self.raised.push(name.to_owned());
        }
    }

    /// Schedule the trigger events of every clip in the state(s) `path`'s machine
    /// is in, and of the nested machines below, from the graph's current time.
    fn schedule_from(&mut self, path: &[u16]) {
        let graph = Arc::clone(&self.graph);
        let Some(root) = graph.graph.machine.as_ref() else { return };
        let Some((machine, run)) = locate(root, &self.run, path) else { return };
        let mut fresh = Vec::new();
        let now = self.now;
        fn walk(
            graph: &LoadedGraph,
            machine: &StateMachine,
            run: &MachineRun,
            path: &mut Vec<u16>,
            now: f32,
            out: &mut Vec<Scheduled>,
        ) {
            let Some(state) = machine.states.iter().find(|state| state.id == run.state) else {
                return;
            };
            for clip in state.clip.iter().chain(&state.content.blended_clips) {
                let Some((clip, registered)) = graph.clip_for(clip) else { continue };
                if clip.mode == ClipMode::UserControlled {
                    continue;
                }
                let speed = if clip.playback_speed.is_finite() && clip.playback_speed > 0.0 { clip.playback_speed } else { 1.0 };
                let period = (clip.mode == ClipMode::Looping && registered.duration > 0.0)
                    .then_some(registered.duration / speed);
                for trigger in &clip.triggers {
                    let at = if trigger.relative_to_end { registered.duration + trigger.local_time } else { trigger.local_time };
                    let real = (at - clip.start_time) / speed;
                    if real >= 0.0 && real.is_finite() {
                        out.push(Scheduled { at: now + real, event: trigger.event, path: path.clone(), period });
                    }
                }
            }
            for (index, inner) in state.content.machines.iter().enumerate() {
                if let Some(inner_run) = run.nested.get(index) {
                    path.push(index as u16);
                    walk(graph, inner, inner_run, path, now, out);
                    path.pop();
                }
            }
        }
        walk(&graph, machine, run, &mut path.to_vec(), now, &mut fresh);
        self.scheduled.extend(fresh);
    }

    /// The playback of the current configuration, started fresh (every trigger
    /// of every clip in it scheduled from now).
    pub fn start(&mut self) -> Option<Playback> {
        self.scheduled.clear();
        self.schedule_from(&[]);
        self.pending_fade = 0.0;
        self.sync_phase = 0.0;
        // The modifiers of the start path activate (their assignments land) before the
        // pose is read; an event they raise is answered at once.
        let mut raised = Vec::new();
        self.refresh_modifiers(&mut raised);
        if !raised.is_empty() {
            let mut changed = Vec::new();
            self.run_events(raised, &mut changed);
        }
        let terms = self.terms();
        let playback = self.playback_of(&terms, 0.0);
        self.blend_active = playback.as_ref().is_some_and(|playback| !playback.pose.is_empty());
        self.applied = Self::signature(&terms);
        self.synced_pose = terms.iter().any(|term| term.synced);
        self.dirty = false;
        playback
    }

    /// The transition `event` takes in the machine at `machine`/`run`: the current
    /// state's own first, then the machine's wildcard; disabled ones never fire, and one
    /// gated on a condition fires only while the condition holds over the variables.
    fn transition_of(&self, machine: &StateMachine, run: &MachineRun, event: i32) -> Option<Transition> {
        if event < 0 {
            return None;
        }
        let state = machine.states.iter().find(|state| state.id == run.state)?;
        let usable = |t: &&Transition| {
            t.event == event
                && t.flags & TRANSITION_FLAG_DISABLED == 0
                && t.condition.as_ref().is_none_or(|condition| condition.holds(&|name| self.variable(name)))
        };
        state.transitions.iter().find(usable).or_else(|| machine.wildcard.iter().find(usable)).cloned()
    }

    /// Give `event` to the machine at `path`: its own transition if it has one,
    /// else to the nested machines of its current state. The paths of the machines
    /// that changed state are appended to `changed`.
    fn dispatch(&mut self, path: &mut Vec<u16>, event: i32, changed: &mut Vec<Vec<u16>>) {
        let graph = Arc::clone(&self.graph);
        let Some(root) = graph.graph.machine.as_ref() else { return };
        let Some((machine, run)) = locate(root, &self.run, path) else { return };
        if let Some(transition) = self.transition_of(machine, run, event) {
            self.pending_fade = self.pending_fade.max(self.transition_fade(&transition));
            let entered = MachineRun::enter(machine, transition.to_state, &self.variables);
            if let Some(slot) = locate_mut(&mut self.run, path) {
                *slot = entered;
            }
            changed.push(path.clone());
            return;
        }
        let inner = run.nested.len();
        for index in 0..inner {
            path.push(index as u16);
            self.dispatch(path, event, changed);
            path.pop();
        }
    }

    /// Raise `event`: `None` when no machine answers it; otherwise the playback to
    /// start (`Some(None)` when nothing needs restarting).
    pub fn send_event(&mut self, event: i32) -> Option<Option<Playback>> {
        let before = Self::signature(&self.terms());
        let mut changed = Vec::new();
        self.run_events(vec![event], &mut changed);
        if changed.is_empty() {
            return None;
        }
        self.dirty = true;
        for path in &changed {
            self.scheduled.retain(|scheduled| !scheduled.path.starts_with(path));
        }
        for path in &changed {
            self.schedule_from(path);
        }
        // Something restarts when the clips differ, or a clip sits under a machine that
        // changed state (a transition back into the same state plays it again).
        let terms = self.terms();
        let signature = Self::signature(&terms);
        let restarts = signature != before || terms.iter().any(|term| changed.iter().any(|change| term.path.starts_with(change)));
        let playback = if restarts {
            self.sync_phase = 0.0;
            let playback = self.playback_of(&terms, self.pending_fade);
            self.applied = signature;
            self.synced_pose = terms.iter().any(|term| term.synced);
            if let Some(playback) = &playback {
                self.blend_active = !playback.pose.is_empty();
            }
            playback
        } else {
            None
        };
        self.pending_fade = 0.0;
        Some(playback)
    }

    /// Give `events` to the graph, and the events the modifiers raise in answer to them to
    /// it in turn (a bounded chain): the modifiers see each event, the machines take their
    /// transitions, and the modifiers of the new active path activate. The paths of the
    /// machines that changed state are appended to `changed`.
    fn run_events(&mut self, events: Vec<i32>, changed: &mut Vec<Vec<u16>>) {
        const MAX_CHAIN: usize = 32;
        let mut queue: std::collections::VecDeque<i32> = events.into();
        let mut handled = 0;
        while let Some(event) = queue.pop_front() {
            handled += 1;
            if handled > MAX_CHAIN {
                log::debug!("behavior graph: event chain cut after {MAX_CHAIN} events");
                break;
            }
            let mut raised = Vec::new();
            self.with_modifiers(|state, vars| state.on_event(event, vars, &mut raised));
            let known = changed.len();
            self.dispatch(&mut Vec::new(), event, changed);
            if changed.len() > known {
                self.refresh_modifiers(&mut raised);
            }
            for event in raised {
                self.note_raised(event);
                queue.push_back(event);
            }
        }
    }

    /// A looping clip's trigger that just fired comes round again a loop later.
    fn rearm(&mut self, fired: &Scheduled) {
        if let Some(period) = fired.period {
            self.scheduled.push(Scheduled { at: fired.at + period, ..fired.clone() });
        }
    }

    /// The earliest scheduled event due by `limit`, removed.
    fn pop_due(&mut self, limit: f32) -> Option<Scheduled> {
        let index = self
            .scheduled
            .iter()
            .enumerate()
            .filter(|(_, scheduled)| scheduled.at <= limit)
            .min_by(|a, b| a.1.at.total_cmp(&b.1.at))
            .map(|(index, _)| index)?;
        Some(self.scheduled.remove(index))
    }

    /// Seconds from now until `target` is raised, following the machines through
    /// the transitions the schedule's own events take; `None` when it never is.
    pub fn seconds_until(&self, target: i32) -> Option<f32> {
        let mut sim = self.clone();
        let origin = sim.now;
        for _ in 0..64 {
            let due = sim.pop_due(f32::INFINITY)?;
            sim.now = sim.now.max(due.at);
            if due.event == target {
                return Some((due.at - origin).max(0.0));
            }
            sim.rearm(&due);
            sim.send_event(due.event);
        }
        None
    }

    /// Advance the schedule by `dt`: the events now due raise in order, and each
    /// that changes a state with something to restart yields the playback to start.
    pub fn step(&mut self, dt: f32) -> Vec<Playback> {
        let mut started = Vec::new();
        let limit = self.now + dt.max(0.0);
        while let Some(due) = self.pop_due(limit) {
            self.now = self.now.max(due.at);
            self.note_raised(due.event);
            self.rearm(&due);
            if let Some(Some(playback)) = self.send_event(due.event) {
                started.push(playback);
            }
        }
        self.now = limit;
        // The modifiers that watch the variables run once a frame; what they raise is
        // taken like a trigger's event.
        let mut raised = Vec::new();
        self.with_modifiers(|state, vars| state.tick(dt, vars, &mut raised));
        for event in raised {
            self.note_raised(event);
            if let Some(Some(playback)) = self.send_event(event) {
                started.push(playback);
            }
        }
        started
    }
}

/// The clip's playback speed: its bound variable's value (when it is a usable speed),
/// else the authored one.
fn clip_speed_of(variables: &[f32], clip: &StateClip) -> f32 {
    let bound = clip.playback_speed_variable.and_then(|index| variables.get(index)).copied();
    let speed = bound.unwrap_or(clip.playback_speed);
    if speed.is_finite() && speed > 0.0 {
        speed
    } else {
        1.0
    }
}

/// How much of each child of a blender is in the pose.
///
/// A *parametric* blender's child weights are positions on one axis and the blend
/// parameter (the authored one, or its bound variable) falls between two of them: those
/// two children share the pose in proportion to how near the parameter is, and beyond
/// the ends the end child has all of it. A *cyclic* one treats the axis as a circle
/// of `min..max` (a heading), so the last child blends back round into the first. Any
/// other blender's weights are per-child layer amounts over bone masks, which are not
/// modelled: its first child has the pose.
pub(crate) fn blender_weights(blender: &Blender, variables: &[f32]) -> Vec<f32> {
    let count = blender.children.len();
    let mut weights = vec![0.0; count];
    if count == 0 {
        return weights;
    }
    if count == 1 {
        weights[0] = 1.0;
        return weights;
    }
    if !blender.parametric {
        for (weight, child) in weights.iter_mut().zip(&blender.children) {
            let value = child.weight_variable.and_then(|variable| variables.get(variable)).copied().unwrap_or(child.weight);
            *weight = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
        }
        return weights;
    }
    let value = |variable: Option<usize>, authored: f32| variable.and_then(|index| variables.get(index)).copied().unwrap_or(authored);
    let parameter = value(blender.parameter_variable, blender.parameter);
    let parameter = if parameter.is_finite() { parameter } else { 0.0 };
    let mut order: Vec<(f32, usize)> = blender
        .children
        .iter()
        .enumerate()
        .map(|(index, child)| (value(child.weight_variable, child.weight), index))
        .collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let first = order[0];
    let last = order[count - 1];
    let range = blender.max_cyclic - blender.min_cyclic;
    if blender.cyclic && range > 0.0 {
        let mut at = blender.min_cyclic + (parameter - blender.min_cyclic).rem_euclid(range);
        if at < first.0 {
            at += range;
        }
        // The last child at or before `at`, and the one after it (round the circle).
        let before = order.iter().rposition(|(position, _)| *position <= at).unwrap_or(count - 1);
        let (from, to, to_position) = if before + 1 < count {
            (order[before], order[before + 1].1, order[before + 1].0)
        } else {
            (order[before], first.1, first.0 + range)
        };
        let span = to_position - from.0;
        let t = if span > 0.0 { ((at - from.0) / span).clamp(0.0, 1.0) } else { 0.0 };
        weights[from.1] += 1.0 - t;
        weights[to] += t;
        return weights;
    }
    if parameter <= first.0 {
        weights[first.1] = 1.0;
    } else if parameter >= last.0 {
        weights[last.1] = 1.0;
    } else {
        let before = order.iter().rposition(|(position, _)| *position <= parameter).unwrap_or(0);
        let (low, high) = (order[before], order[before + 1]);
        let span = high.0 - low.0;
        let t = if span > 0.0 { ((parameter - low.0) / span).clamp(0.0, 1.0) } else { 0.0 };
        weights[low.1] += 1.0 - t;
        weights[high.1] += t;
    }
    weights
}

/// The clip time at sync phase `phase` for a clip of `duration` whose `SyncLeft` and
/// `SyncRight` points are at `sync`: phase 0 is its `SyncLeft`, 1 its `SyncRight`, and 2
/// comes back round to `SyncLeft` (the clip loops), moving evenly between the points.
pub(crate) fn sync_time(sync: [f32; 2], duration: f32, phase: f32) -> f32 {
    let [left, right] = sync;
    let first = (right - left).rem_euclid(duration);
    let phase = phase.rem_euclid(2.0);
    let time = if phase < 1.0 { left + phase * first } else { right + (phase - 1.0) * (duration - first) };
    time.rem_euclid(duration)
}

/// How long the clip takes to cross the sync segment `phase` is in, at speed 1.
fn sync_segment(sync: [f32; 2], duration: f32, phase: f32) -> f32 {
    let first = (sync[1] - sync[0]).rem_euclid(duration);
    if phase.rem_euclid(2.0) < 1.0 {
        first
    } else {
        duration - first
    }
}

/// A pose: its layers, and how fast its shared sync phase runs.
struct PoseEval {
    layers: Vec<PoseLayer>,
    /// Sync phase units a second when the pose has synced clips.
    sync_rate: Option<f32>,
}

/// The per-bone weights of a blender child now: its authored array, with the elements a
/// variable drives replaced by the variable.
fn bone_weights_now(bones: &BoneWeights, variables: &[f32]) -> Arc<Vec<f32>> {
    if bones.bound.is_empty() {
        return Arc::clone(&bones.weights);
    }
    let mut weights = (*bones.weights).clone();
    for &(bone, variable) in &bones.bound {
        if let (Some(slot), Some(value)) = (weights.get_mut(bone), variables.get(variable)) {
            *slot = *value;
        }
    }
    Arc::new(weights)
}

/// A child's mask under the mask of the layers above it: bone by bone, the product.
fn combine_masks(outer: Option<&Arc<Vec<f32>>>, inner: &Arc<Vec<f32>>) -> Arc<Vec<f32>> {
    match outer {
        None => Arc::clone(inner),
        Some(outer) => Arc::new(inner.iter().enumerate().map(|(bone, weight)| weight * outer.get(bone).copied().unwrap_or(1.0)).collect()),
    }
}

/// The clips of the active generator tree as a pose, every clip the graph has (the
/// zero-weight ones too, so a clip that comes into the blend is already in step).
///
/// A *synced* group (clips with a `SyncLeft` and a `SyncRight` annotation under a blender
/// with the sync flag) shares one phase: 0 at each clip's `SyncLeft`, 1 at its `SyncRight`,
/// wrapping at 2, so a walk that plants its left foot at 0.33 s and a run that plants its
/// right foot first stay foot-for-foot together. Each clip is put at the time that phase
/// maps to through its own sync points, and the phase runs at the weighted mean of the
/// group's clips' rates. Every other clip (an idle pose, a gesture) plays at its own speed.
fn pose_layers(terms: &[Term], phase: f32) -> PoseEval {
    let group: Vec<&Term> = terms.iter().filter(|term| term.synced).collect();
    let group_weight: f32 = group.iter().map(|term| term.weight).sum();
    // Phase units a second for one clip: its speed over the length of the sync segment it
    // is in (a clip with no sync pair crosses both in one cycle).
    let rate_of = |term: &Term| -> f32 {
        if term.duration <= 0.0 {
            return 0.0;
        }
        match term.sync {
            Some(sync) => term.speed / sync_segment(sync, term.duration, phase).max(1e-3),
            None => 2.0 * term.speed / term.duration,
        }
    };
    let sync_rate = (!group.is_empty()).then(|| {
        if group_weight > 0.0 {
            group.iter().map(|term| term.weight * rate_of(term)).sum::<f32>() / group_weight
        } else {
            rate_of(group[0])
        }
    });
    let layers = terms
        .iter()
        .map(|term| match term.sync.filter(|_| term.synced) {
            Some(sync) => PoseLayer {
                handle: term.handle,
                weight: term.weight,
                speed: 0.0,
                time: Some(sync_time(sync, term.duration, phase)),
                mask: term.mask.clone(),
            },
            // A clip outside the synced group plays at its own speed.
            None => PoseLayer { handle: term.handle, weight: term.weight, speed: term.speed, time: None, mask: term.mask.clone() },
        })
        .collect();
    PoseEval { layers, sync_rate }
}

/// Start `playback` on the graph's root: the object's `AnimationPlayer`.
pub fn apply_playback(world: &World, root: EntityId, playback: Playback) {
    apply_playback_for(world, root, root, playback);
}

/// [`apply_playback`] for a graph owned by `owner` whose clips animate `animated`.
///
/// A single clip that switches at once is the object's `AnimationPlayer`. A blended pose,
/// or a transition that fades (the old layers fade out as the new ones fade in), is an
/// `AnimationStack` on the owner instead, which the animation system blends by weight;
/// the two never coexist on the owner.
pub fn apply_playback_for(world: &World, owner: EntityId, animated: EntityId, playback: Playback) {
    let root = owner;
    let stacked = !playback.pose.is_empty() || playback.fade > 0.0;
    if !stacked {
        let mut player = AnimationPlayer::new(playback.handle).with_root(animated);
        player.speed = playback.speed;
        match playback.hold_at {
            Some(at) => {
                player.local_time = at;
                player.prev_time = at;
                player.playing = false;
            }
            None => {
                player.local_time = playback.start_time;
                player.prev_time = playback.start_time;
            }
        }
        if let Some(mut stacks) = world.query_mut::<AnimationStack>() {
            stacks.remove(root);
        }
        if let Some(mut players) = world.query_mut::<AnimationPlayer>() {
            players.insert(root, player);
        }
        return;
    }

    let mut stack = world
        .query_mut::<AnimationStack>()
        .and_then(|mut stacks| stacks.remove(root))
        .unwrap_or_default();
    stack.root_entity = Some(animated);
    if playback.bones.is_some() {
        stack.bone_index = playback.bones.clone();
    }
    // A plain playing clip is the pose that fades out.
    if let Some(player) = world.query_mut::<AnimationPlayer>().and_then(|mut players| players.remove(root)) {
        let mut layer = AnimationLayer::new(player.clip_handle);
        layer.local_time = player.local_time;
        layer.prev_time = player.local_time;
        layer.speed = player.speed;
        layer.playing = player.playing;
        stack.layers.push(layer);
    }
    if playback.fade > 0.0 {
        for layer in &mut stack.layers {
            if layer.blend_out_remaining <= 0.0 {
                layer.blend_out_remaining = playback.fade;
                layer.blend_out_total = playback.fade;
            }
        }
    } else {
        stack.layers.clear();
    }
    let fresh: Vec<AnimationLayer> = if playback.pose.is_empty() {
        let mut layer = AnimationLayer::new(playback.handle);
        layer.speed = playback.speed;
        match playback.hold_at {
            Some(at) => {
                layer.local_time = at;
                layer.prev_time = at;
                layer.playing = false;
            }
            None => {
                layer.local_time = playback.start_time;
                layer.prev_time = playback.start_time;
            }
        }
        vec![layer]
    } else {
        playback
            .pose
            .iter()
            .map(|pose| {
                let mut layer = AnimationLayer::new(pose.handle);
                layer.speed = pose.speed;
                layer.weight = pose.weight;
                layer.bone_weights = pose.mask.clone();
                if let Some(time) = pose.time {
                    layer.local_time = time;
                    layer.prev_time = time;
                }
                layer
            })
            .collect()
    };
    for layer in fresh {
        stack.layers.push(if playback.fade > 0.0 { layer.with_blend_in(playback.fade) } else { layer });
    }
    if let Some(mut stacks) = world.query_mut::<AnimationStack>() {
        stacks.insert(root, stack);
    }
}

/// A graph that has faded into one plain clip goes back to the object's
/// `AnimationPlayer`: a stack with a single settled layer is a player in all but name,
/// and the systems that look for a player (a seat, a walk) must find it.
fn settle_stack(world: &World, root: EntityId) {
    let layer = {
        let Some(stacks) = world.query::<AnimationStack>() else { return };
        let Some(stack) = stacks.get(root) else { return };
        let [layer] = stack.layers.as_slice() else { return };
        if layer.blend_in_remaining > 0.0 || layer.blend_out_remaining > 0.0 || layer.bone_weights.is_some() {
            return;
        }
        (layer.clone(), stack.root_entity)
    };
    if world.query::<AnimationPlayer>().is_some_and(|players| players.get(root).is_some()) {
        return;
    }
    let (layer, animated) = layer;
    let mut player = AnimationPlayer::new(layer.clip_handle).with_root(animated.unwrap_or(root));
    player.local_time = layer.local_time;
    player.prev_time = layer.prev_time;
    player.speed = layer.speed;
    player.playing = layer.playing;
    if let Some(mut stacks) = world.query_mut::<AnimationStack>() {
        stacks.remove(root);
    }
    if let Some(mut players) = world.query_mut::<AnimationPlayer>() {
        players.insert(root, player);
    }
}

/// Move the weights and speeds of the blended pose on `root`'s stack to `pose`: the
/// pose is the stack's newest layers (older ones are fading out).
fn update_pose(stack: &mut AnimationStack, pose: &[PoseLayer]) {
    let Some(first) = stack.layers.len().checked_sub(pose.len()) else { return };
    for (layer, target) in stack.layers[first..].iter_mut().zip(pose) {
        if layer.clip_handle != target.handle {
            return;
        }
    }
    for (layer, target) in stack.layers[first..].iter_mut().zip(pose) {
        layer.speed = target.speed;
        layer.bone_weights = target.mask.clone();
        if let Some(time) = target.time {
            layer.prev_time = layer.local_time;
            layer.local_time = time;
        }
        layer.blend_in_target = target.weight;
        if layer.blend_in_remaining <= 0.0 {
            layer.weight = target.weight;
        }
    }
}

/// `reference.SetAnimationVariable*(name, value)`: write the variable of the
/// reference's graph; whether it has such a variable.
pub fn set_variable_value(world: &World, entity: EntityId, name: &str, value: f32) -> bool {
    let Some(mut graphs) = world.query_mut::<ObjectAnimationGraph>() else { return false };
    graphs.get_mut(entity).is_some_and(|graph| graph.set_variable(name, value))
}

/// `reference.GetAnimationVariable*(name)`: the variable's current value.
pub fn variable_value(world: &World, entity: EntityId, name: &str) -> Option<f32> {
    world.query::<ObjectAnimationGraph>()?.get(entity)?.variable(name)
}

/// `reference.PlayAnimation(event)`: whether the graph accepted the event.
pub fn play_animation(world: &World, entity: EntityId, event: &str) -> bool {
    send(world, entity, event).is_some()
}

/// `reference.PlayAnimationAndWait(event, waitEvent)`: the seconds until
/// `wait_event` is raised once `event` is accepted; `None` when the graph
/// rejects `event` or never raises `wait_event`.
pub fn play_animation_and_wait(world: &World, entity: EntityId, event: &str, wait_event: &str) -> Option<f32> {
    send(world, entity, event)?;
    let graphs = world.query::<ObjectAnimationGraph>()?;
    let graph = graphs.get(entity)?;
    let target = graph.graph.graph.event_id(wait_event)?;
    graph.seconds_until(target)
}

/// Whether `entity` runs a behavior graph.
pub fn has_graph(world: &World, entity: EntityId) -> bool {
    world.query::<ObjectAnimationGraph>().is_some_and(|graphs| graphs.get(entity).is_some())
}

/// `reference.WaitForAnimationEvent(event)`: the seconds until the graph's own
/// schedule raises `event`; `None` when the object has no graph or it never does.
pub fn wait_for_animation_event(world: &World, entity: EntityId, event: &str) -> Option<f32> {
    let graphs = world.query::<ObjectAnimationGraph>()?;
    let graph = graphs.get(entity)?;
    let target = graph.graph.graph.event_id(event)?;
    graph.seconds_until(target)
}

fn send(world: &World, entity: EntityId, event: &str) -> Option<()> {
    let (root, animated, playback) = {
        let mut graphs = world.query_mut::<ObjectAnimationGraph>()?;
        let graph = graphs.get_mut(entity)?;
        let id = graph.graph.graph.event_id(event)?;
        let before = graph.state();
        let sent = graph.send_event(id);
        if sent.is_some() {
            graph.note_raised(id);
        }
        log::debug!(
            "PlayAnimation '{event}' on entity {entity}: {} (state {before} -> {})",
            if sent.is_some() { "accepted" } else { "rejected" },
            graph.state()
        );
        (graph.root, graph.animated(), sent?)
    };
    if let Some(playback) = playback {
        apply_playback_for(world, root, animated, playback);
    }
    Some(())
}

/// Step every object graph's trigger schedule (a clip's end raises its
/// completion event, which moves the machine on). Run each frame.
pub fn object_graph_step_system(world: &World, dt: f32) {
    type Poses = Vec<(EntityId, Vec<PoseLayer>)>;
    let (started, poses, plain): (Vec<(EntityId, EntityId, Playback)>, Poses, Vec<EntityId>) = {
        let Some(mut graphs) = world.query_mut::<ObjectAnimationGraph>() else {
            return;
        };
        let mut started = Vec::new();
        let mut poses = Vec::new();
        let mut plain = Vec::new();
        for (_, graph) in graphs.iter_mut() {
            if let Some(playback) = graph.step(dt).pop() {
                started.push((graph.root, graph.animated(), playback));
                continue;
            }
            match graph.pose_now(dt) {
                // The variables move the weights of a blended pose between events.
                Some(PoseUpdate::Layers(pose)) => poses.push((graph.root, pose)),
                // The clips themselves changed (a selector, a start state).
                Some(PoseUpdate::Restart(playback)) => started.push((graph.root, graph.animated(), playback)),
                None => plain.push(graph.root),
            }
        }
        (started, poses, plain)
    };
    for (root, animated, playback) in started {
        apply_playback_for(world, root, animated, playback);
    }
    for root in plain {
        settle_stack(world, root);
    }
    if poses.is_empty() {
        return;
    }
    let Some(mut stacks) = world.query_mut::<AnimationStack>() else { return };
    let taken_over: Vec<EntityId> = match world.query::<AnimationPlayer>() {
        Some(players) => poses.iter().map(|(root, _)| *root).filter(|root| players.get(*root).is_some()).collect(),
        None => Vec::new(),
    };
    for (root, pose) in &poses {
        if taken_over.contains(root) {
            // Another system gave the owner a clip of its own (a seat, a walk): the
            // graph's stack must not fight it.
            stacks.remove(*root);
        } else if let Some(stack) = stacks.get_mut(*root) {
            update_pose(stack, pose);
        }
    }
}

/// `Actor.ChangeAnimArchetype(keyword)` (Fallout 4; CK: "adds the specified
/// AnimArchetype keyword to this actor", and "removes any existing AnimArchetype
/// keyword from the actor (sets actor back to 'Neutral')" when called with none).
/// The archetype picks which animation set the actor's graph runs, so it is
/// recorded per actor reference for the graph runtime to read; the authored
/// archetype is the base `NPC_`'s keyword. Saved.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimArchetypeOverrides {
    set: std::collections::BTreeMap<u32, u32>,
}

impl byroredux_core::ecs::resource::Resource for AnimArchetypeOverrides {}

impl AnimArchetypeOverrides {
    /// Set `actor`'s archetype keyword; `0` is "back to Neutral".
    pub fn set(&mut self, actor: u32, keyword: u32) {
        self.set.insert(actor, keyword);
    }

    /// The archetype a script gave `actor`: `Some(0)` after a clear, `None`
    /// when no script changed it.
    pub fn get(&self, actor: u32) -> Option<u32> {
        self.set.get(&actor).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_hkx::{ClipTrigger, State};

    fn transition(event: i32, to_state: i32) -> Transition {
        Transition { event, to_state, priority: 0, flags: 0, ..Default::default() }
    }

    fn clip(animation: &str, mode: ClipMode, speed: f32, triggers: Vec<ClipTrigger>) -> StateClip {
        StateClip {
            animation: animation.into(),
            mode,
            playback_speed: speed,
            start_time: 0.0,
            user_fraction: 0.0,
            triggers,
            playback_speed_variable: None,
        }
    }

    /// A lever shaped like the Nordic one: events Down(0) / Pulled(1) / Up(2) /
    /// Rest(3, wildcard); state 0 rests (held pose), state 2 plays the 0.8 s
    /// pull-down clip at speed 2 and raises Pulled at its end, state 1 rests pulled.
    fn lever() -> ObjectAnimationGraph {
        let trigger = |event, local_time, relative_to_end| ClipTrigger { local_time, event, relative_to_end, annotation: false };
        let machine = StateMachine {
            start_state: 0,
            start_state_variable: None,
            states: vec![
                State {
                    id: 0,
                    name: "Mid".into(),
                    transitions: vec![transition(0, 2)],
                    clip: Some(clip("a/pose.hkx", ClipMode::UserControlled, 1.0, Vec::new())),
                    content: Default::default(),
                },
                State {
                    id: 1,
                    name: "Pulled".into(),
                    transitions: vec![transition(2, 0)],
                    clip: Some(clip("a/pose.hkx", ClipMode::UserControlled, 1.0, Vec::new())),
                    content: Default::default(),
                },
                State {
                    id: 2,
                    name: "MidToPulled".into(),
                    transitions: vec![transition(1, 1), Transition { flags: TRANSITION_FLAG_DISABLED, ..transition(2, 0) }],
                    clip: Some(clip(
                        "a/pull.hkx",
                        ClipMode::SinglePlay,
                        2.0,
                        vec![trigger(1, 0.0, true)],
                    )),
                    content: Default::default(),
                },
            ],
            wildcard: vec![transition(3, 0)],
        };
        let graph = BehaviorGraph {
            variables: Vec::new(),
            events: vec!["Down".into(), "Pulled".into(), "Up".into(), "Rest".into()],
            machine: Some(machine),
        };
        let mut clips = HashMap::new();
        clips.insert(("a/pose.hkx".to_owned(), false), GraphClip { handle: 7, duration: 0.8 });
        clips.insert(("a/pull.hkx".to_owned(), false), GraphClip { handle: 9, duration: 0.8 });
        ObjectAnimationGraph::new(Arc::new(LoadedGraph { graph, clips, sequences: HashMap::new(), lazy: None }), 5).unwrap()
    }

    // ── variables, blenders and transition fades ──────────────────────

    use byroredux_hkx::{BlendChild, Variable};

    fn blend_clip(animation: &str) -> Generator {
        Generator::Clip(clip(animation, ClipMode::Looping, 1.0, Vec::new()))
    }

    /// The vanilla locomotion shape: a cyclic direction blender (children at 0, 0.25, 0.5,
    /// 0.75 of a turn) over speed blenders (walk at 80, run at 370 units/s).
    fn locomotion_blend() -> Generator {
        let speed = |walk: &str, run: &str| {
            Generator::Blender(Blender {
                parametric: true,
                cyclic: false,
                sync: true,
                parameter: 0.0,
                parameter_variable: Some(0),
                min_cyclic: 0.0,
                max_cyclic: 1.0,
                children: vec![
                    BlendChild { weight: 80.0, weight_variable: None, bone_weights: None, bone_weights_variable: None, generator: blend_clip(walk) },
                    BlendChild { weight: 370.0, weight_variable: None, bone_weights: None, bone_weights_variable: None, generator: blend_clip(run) },
                ],
            })
        };
        Generator::Blender(Blender {
            parametric: true,
            cyclic: true,
            sync: true,
            parameter: 0.0,
            parameter_variable: Some(1),
            min_cyclic: 0.0,
            max_cyclic: 1.0,
            children: ["forward", "right", "back", "left"]
                .iter()
                .enumerate()
                .map(|(index, name)| BlendChild {
                    weight: index as f32 * 0.25,
                    weight_variable: None,
                    bone_weights: None,
                    bone_weights_variable: None,
                    generator: speed(&format!("walk_{name}"), &format!("run_{name}")),
                })
                .collect(),
        })
    }

    /// An idle (Idle, state 0) that `Move` (event 0) takes to the locomotion blend
    /// (Move, state 1) over a quarter second, and `Stop` (event 1) back over 0.1 s.
    /// Variables: `Speed` (0), `Direction` (1), `bFlag` (2, bool), `iMode` (3, int).
    fn locomotion() -> ObjectAnimationGraph {
        let fading = |event, to_state, blend_duration| Transition { blend_duration, ..transition(event, to_state) };
        let machine = StateMachine {
            start_state: 0,
            start_state_variable: None,
            states: vec![
                State {
                    id: 0,
                    name: "Idle".into(),
                    transitions: vec![fading(0, 1, 0.25)],
                    clip: Some(clip("idle", ClipMode::Looping, 1.0, Vec::new())),
                    content: Default::default(),
                },
                State {
                    id: 1,
                    name: "Move".into(),
                    transitions: vec![fading(1, 0, 0.1)],
                    clip: Some(clip("walk_forward", ClipMode::Looping, 1.0, Vec::new())),
                    content: byroredux_hkx::StateContent { generator: Some(locomotion_blend()), ..Default::default() },
                },
            ],
            wildcard: Vec::new(),
        };
        let real = |name: &str, initial| Variable { name: name.into(), kind: byroredux_hkx::VariableKind::Real, initial };
        let graph = BehaviorGraph {
            variables: vec![
                real("Speed", 0.0),
                real("Direction", 0.0),
                Variable { name: "bFlag".into(), kind: byroredux_hkx::VariableKind::Bool, initial: 0.0 },
                Variable { name: "iMode".into(), kind: byroredux_hkx::VariableKind::Int, initial: 2.0 },
            ],
            events: vec!["Move".into(), "Stop".into()],
            machine: Some(machine),
        };
        let mut clips = HashMap::new();
        clips.insert(("idle".to_owned(), true), GraphClip { handle: 1, duration: 2.0 });
        for (index, name) in ["forward", "right", "back", "left"].iter().enumerate() {
            clips.insert((format!("walk_{name}"), true), GraphClip { handle: 10 + index as u32, duration: 1.0 });
            clips.insert((format!("run_{name}"), true), GraphClip { handle: 20 + index as u32, duration: 0.5 });
        }
        ObjectAnimationGraph::new(Arc::new(LoadedGraph { graph, clips, sequences: HashMap::new(), lazy: None }), 3).unwrap()
    }

    /// The re-weighted layers of a pose update (the clips did not change).
    fn layers_of(update: Option<PoseUpdate>) -> Vec<PoseLayer> {
        match update {
            Some(PoseUpdate::Layers(layers)) => layers,
            other => panic!("expected re-weighted layers, got {other:?}"),
        }
    }

    fn weight_of(pose: &[PoseLayer], handle: u32) -> f32 {
        pose.iter().find(|layer| layer.handle == handle).map_or(0.0, |layer| layer.weight)
    }

    /// A parametric blender mixes the two children its parameter falls between and
    /// holds the end child beyond the ends; a cyclic one wraps round from the last
    /// child to the first.
    #[test]
    fn parametric_blenders_mix_the_neighbours_and_cyclic_ones_wrap() {
        let Generator::Blender(direction) = locomotion_blend() else { unreachable!() };
        let Generator::Blender(speed) = &direction.children[0].generator else { unreachable!() };
        let speeds = |value: f32| blender_weights(speed, &[value, 0.0]);
        assert_eq!(speeds(0.0), vec![1.0, 0.0], "slower than the walk: the walk");
        assert_eq!(speeds(80.0), vec![1.0, 0.0]);
        assert_eq!(speeds(225.0), vec![0.5, 0.5], "half way between walk and run");
        assert_eq!(speeds(370.0), vec![0.0, 1.0]);
        assert_eq!(speeds(900.0), vec![0.0, 1.0], "faster than the run: the run");

        let headings = |value: f32| blender_weights(&direction, &[0.0, value]);
        assert_eq!(headings(0.0), vec![1.0, 0.0, 0.0, 0.0]);
        assert_eq!(headings(0.125), vec![0.5, 0.5, 0.0, 0.0]);
        assert_eq!(headings(0.5), vec![0.0, 0.0, 1.0, 0.0]);
        let round = headings(0.875);
        assert!((round[3] - 0.5).abs() < 1e-6 && (round[0] - 0.5).abs() < 1e-6, "between the last child and the first: {round:?}");
        assert_eq!(headings(1.0), headings(0.0), "a full turn is no turn");
        assert_eq!(headings(-0.25), headings(0.75), "negative headings wrap");

        // Not parametric: each child's own weight (0..1), here the positions 0, .25, .5, .75.
        let layered = Blender { parametric: false, ..direction.clone() };
        assert_eq!(blender_weights(&layered, &[0.0, 0.5]), vec![0.0, 0.25, 0.5, 0.75]);
    }

    #[test]
    fn variables_have_the_graphs_initial_values_and_keep_their_kind() {
        let mut graph = locomotion();
        assert_eq!(graph.variable("speed"), Some(0.0), "names are case-insensitive");
        assert_eq!(graph.variable("iMode"), Some(2.0), "the authored initial value");
        assert!(graph.set_variable("Speed", 225.0));
        assert_eq!(graph.variable("Speed"), Some(225.0));
        assert!(graph.set_variable("bFlag", 7.0));
        assert_eq!(graph.variable("bFlag"), Some(1.0), "a bool is 0 or 1");
        assert!(graph.set_variable("iMode", 3.9));
        assert_eq!(graph.variable("iMode"), Some(3.0), "an int is whole");
        assert!(!graph.set_variable("NoSuchVariable", 1.0));
        assert!(!graph.set_variable("Speed", f32::NAN));
        assert_eq!(graph.variable("Speed"), Some(225.0));
    }

    /// Set the sync flag of every blender of `node`.
    fn set_sync(node: &mut Generator, sync: bool) {
        if let Generator::Blender(blender) = node {
            blender.sync = sync;
            for child in &mut blender.children {
                set_sync(&mut child.generator, sync);
            }
        }
    }

    /// `locomotion()` with its blend tree synced or not.
    fn locomotion_synced(sync: bool) -> ObjectAnimationGraph {
        let graph = locomotion();
        let mut shared = (*graph.graph).clone();
        let state = &mut shared.graph.machine.as_mut().unwrap().states[1];
        set_sync(state.content.generator.as_mut().unwrap(), sync);
        ObjectAnimationGraph::new(Arc::new(shared), 3).unwrap()
    }

    /// Entering the locomotion state yields a blended pose (every clip of the tree,
    /// weighted by its variables), faded in over the transition's duration; the
    /// variables then move the weights without another event; unsynced clips each play at
    /// their own speed.
    #[test]
    fn a_blend_state_plays_a_weighted_pose_that_follows_its_variables() {
        let mut graph = locomotion_synced(false);
        graph.start();
        assert!(graph.pose_now(0.0).is_none(), "the idle is a single clip");
        graph.set_variable("Speed", 225.0);
        graph.set_variable("Direction", 0.125);
        let playback = graph.send_event(0).expect("Move is taken").expect("the state plays");
        assert_eq!(playback.fade, 0.25, "the transition's blending effect");
        assert_eq!(playback.pose.len(), 8, "every clip of the blend tree: 4 headings x walk and run");
        // Heading 0.125 is half forward, half right; 225 is half walk, half run.
        for handle in [10, 11, 20, 21] {
            assert!((weight_of(&playback.pose, handle) - 0.25).abs() < 1e-6, "handle {handle}");
        }
        assert_eq!(weight_of(&playback.pose, 12), 0.0);
        let total: f32 = playback.pose.iter().map(|layer| layer.weight).sum();
        assert!((total - 1.0).abs() < 1e-5);
        // Unsynced clips play on at their own speed: a blend does not couple them.
        assert!(playback.pose.iter().all(|layer| layer.speed == 1.0 && layer.time.is_none()));

        graph.set_variable("Speed", 370.0);
        graph.set_variable("Direction", 0.5);
        let moved = layers_of(graph.pose_now(0.0));
        assert_eq!(weight_of(&moved, 22), 1.0, "running backwards");
        assert_eq!(weight_of(&moved, 10), 0.0);
    }

    /// A synced tree keeps one phase (0 at `SyncLeft`, 1 at `SyncRight`): each clip is
    /// put at the time that phase maps to through its own sync points, whichever foot it
    /// plants first, and the phase runs at the weighted mean of the clips' rates.
    #[test]
    fn a_synced_blend_aligns_clips_by_their_sync_points() {
        // Walk: 1.2 s, SyncLeft at 0.33, SyncRight at 0.93 (the vanilla WalkForward
        // shape). Run: 0.9 s, SyncRight first at 0.27, SyncLeft at 0.63.
        let (walk, run) = ([0.33, 0.93], [0.63, 0.27]);
        assert!((sync_time(walk, 1.2, 0.0) - 0.33).abs() < 1e-5, "phase 0 is the left plant");
        assert!((sync_time(walk, 1.2, 1.0) - 0.93).abs() < 1e-5, "phase 1 is the right plant");
        assert!((sync_time(walk, 1.2, 0.5) - 0.63).abs() < 1e-5, "half way between");
        assert!((sync_time(walk, 1.2, 1.5) - 0.03).abs() < 1e-4, "half way from the right plant round to the left: wraps the loop");
        assert!((sync_time(walk, 1.2, 2.0) - 0.33).abs() < 1e-5, "two units is a full loop");
        assert!((sync_time(run, 0.9, 0.0) - 0.63).abs() < 1e-5, "the run's left plant is its later point");
        assert!((sync_time(run, 0.9, 1.0) - 0.27).abs() < 1e-5);
        // From the run's left plant (0.63) the next right plant is 0.54 s later (wrapping the loop to 0.27).
        assert!((sync_time(run, 0.9, 0.25) - 0.765).abs() < 1e-4);

        let mut graph = locomotion_synced(true);
        {
            let lazy = Arc::new(LazyClips::default());
            let mut shared = (*graph.graph).clone();
            shared.lazy = Some(Arc::clone(&lazy));
            lazy.provide_sync(10, [0.2, 0.7]); // walk_forward: left 0.2 s, right 0.7 s of 1 s
            lazy.provide_sync(20, [0.35, 0.1]); // run_forward: right first, 0.5 s
            graph.graph = Arc::new(shared);
        }
        graph.start();
        graph.set_variable("Speed", 80.0);
        let playback = graph.send_event(0).unwrap().unwrap();
        let time = |pose: &[PoseLayer], handle: u32| pose.iter().find(|l| l.handle == handle).unwrap().time.unwrap();
        // Phase 0: the synced clips sit on their left plants; a clip without sync points
        // is not part of the group and just plays on.
        assert!((time(&playback.pose, 10) - 0.2).abs() < 1e-5);
        assert!((time(&playback.pose, 20) - 0.35).abs() < 1e-5);
        let loose = playback.pose.iter().find(|layer| layer.handle == 11).unwrap();
        assert_eq!((loose.time, loose.speed), (None, 1.0));
        assert!(playback.pose.iter().filter(|layer| layer.time.is_some()).all(|layer| layer.speed == 0.0), "synced layers are placed, not advanced");

        // The phase advances at the mean rate: forward walk only (80 units a second):
        // 1 phase unit takes 0.5 s (left plant 0.2 to right plant 0.7), so 0.25 s is half way.
        let layers = layers_of(graph.pose_now(0.25));
        assert!((time(&layers, 10) - 0.45).abs() < 1e-4, "{}", time(&layers, 10));
        // The run clip, at weight 0, is carried to the same point of its own stride: half way
        // from its left plant (0.35) to its right plant (0.1 of the next loop = 0.6): 0.475.
        assert!((time(&layers, 20) - 0.475).abs() < 1e-4, "{}", time(&layers, 20));
    }

    /// A transition with a duration fades: the old pose's layers fade out and the new
    /// pose's fade in on an `AnimationStack`; an instant one into a single clip goes
    /// back to a plain `AnimationPlayer`.
    #[test]
    fn a_fading_transition_stacks_the_poses_and_an_instant_one_unstacks_them() {
        let mut world = World::new();
        world.register::<AnimationPlayer>();
        world.register::<AnimationStack>();
        let mut graph = locomotion();
        let idle = graph.start().expect("the idle plays");
        apply_playback_for(&world, 3, 3, idle);
        assert!(world.get::<AnimationPlayer>(3).is_some() && world.get::<AnimationStack>(3).is_none());

        graph.set_variable("Speed", 80.0);
        let moving = graph.send_event(0).unwrap().unwrap();
        apply_playback_for(&world, 3, 3, moving);
        assert!(world.get::<AnimationPlayer>(3).is_none(), "the stack replaces the player");
        {
            let stack = world.get::<AnimationStack>(3).expect("a stack");
            assert_eq!(stack.layers.len(), 1 + 8, "the idle fading out, then the pose");
            assert_eq!(stack.layers[0].clip_handle, 1);
            assert_eq!((stack.layers[0].blend_out_total, stack.layers[1].blend_in_total), (0.25, 0.25));
            // The walk-forward layer ramps to a full weight, the others to none.
            assert_eq!(stack.layers[1].blend_in_target, 1.0);
            assert_eq!(stack.layers[2].blend_in_target, 0.0);
        }

        // Between events, the variables retarget the newest layers.
        graph.set_variable("Speed", 370.0);
        let pose = layers_of(graph.pose_now(0.0));
        update_pose(&mut world.get_mut::<AnimationStack>(3).unwrap(), &pose);
        let stack = world.get::<AnimationStack>(3).unwrap();
        assert_eq!(stack.layers[1].blend_in_target, 0.0, "no longer the walk");
        assert_eq!(stack.layers[2].clip_handle, 20, "run_forward");
        assert_eq!(stack.layers[2].blend_in_target, 1.0, "the run, ramping in");
        drop(stack);

        // Back to the idle over 0.1 s: still a fade, so still a stack...
        let stopping = graph.send_event(1).unwrap().unwrap();
        assert_eq!((stopping.fade, stopping.pose.len()), (0.1, 0));
        apply_playback_for(&world, 3, 3, stopping);
        assert!(world.get::<AnimationStack>(3).is_some());
        // ... and an instant playback with no pose is a plain player again.
        apply_playback_for(&world, 3, 3, Playback { handle: 1, speed: 1.0, ..Default::default() });
        assert!(world.get::<AnimationStack>(3).is_none());
        assert_eq!(world.get::<AnimationPlayer>(3).unwrap().clip_handle, 1);
    }

    /// Once a fade has finished and one plain clip is left, the stack goes back to a
    /// plain player (so the systems that look for a player find the clip).
    #[test]
    fn a_settled_single_layer_stack_becomes_a_player_again() {
        let mut world = World::new();
        world.register::<AnimationPlayer>();
        world.register::<AnimationStack>();
        let mut graph = locomotion();
        apply_playback_for(&world, 3, 3, graph.start().unwrap());
        graph.set_variable("Speed", 80.0);
        apply_playback_for(&world, 3, 3, graph.send_event(0).unwrap().unwrap());
        apply_playback_for(&world, 3, 3, graph.send_event(1).unwrap().unwrap());
        // Mid-fade: nothing settles.
        settle_stack(&world, 3);
        assert!(world.get::<AnimationStack>(3).is_some());
        // The fade completes: the old layers are gone and the new one is at full weight.
        {
            let stack = &mut *world.get_mut::<AnimationStack>(3).unwrap();
            stack.layers.retain(|layer| layer.blend_in_total > 0.0 && layer.clip_handle == 1);
            let layer = &mut stack.layers[0];
            layer.blend_in_remaining = 0.0;
            layer.weight = 1.0;
            layer.local_time = 0.75;
        }
        settle_stack(&world, 3);
        assert!(world.get::<AnimationStack>(3).is_none());
        let player = world.get::<AnimationPlayer>(3).expect("a player");
        assert_eq!((player.clip_handle, player.local_time, player.root_entity), (1, 0.75, Some(3)));
    }

    /// A transition gated on a condition fires only while the expression holds over the
    /// graph's variables; one without a condition (or whose text is unreadable) always
    /// does.
    #[test]
    fn a_conditioned_transition_waits_for_its_condition() {
        let mut graph = locomotion();
        let mut shared = (*graph.graph).clone();
        let machine = shared.graph.machine.as_mut().unwrap();
        machine.states[0].transitions[0].condition = Some(Arc::new(byroredux_hkx::Condition::parse("Speed > 50 && !bFlag")));
        graph.graph = Arc::new(shared);
        graph.start();
        assert_eq!(graph.send_event(0), None, "Speed is 0: Move is refused");
        graph.set_variable("Speed", 80.0);
        graph.set_variable("bFlag", 1.0);
        assert_eq!(graph.send_event(0), None, "bFlag blocks it");
        graph.set_variable("bFlag", 0.0);
        assert!(graph.send_event(0).is_some(), "now it holds");
        assert_eq!(graph.state(), 1);

        // Text the grammar cannot read does not block.
        let mut graph = locomotion();
        let mut shared = (*graph.graph).clone();
        shared.graph.machine.as_mut().unwrap().states[0].transitions[0].condition =
            Some(Arc::new(byroredux_hkx::Condition::parse("iMode=18")));
        graph.graph = Arc::new(shared);
        graph.start();
        assert!(graph.send_event(0).is_some());
    }

    /// A graph in one state (id 0) whose generator is `generator` over `machines`, with
    /// `variables` (name, initial), every clip of `clips` registered (handle = 100 + index,
    /// 1 s long).
    fn rooted(
        generator: Generator,
        machines: Vec<StateMachine>,
        variables: &[(&str, f32)],
        clips: &[&str],
    ) -> ObjectAnimationGraph {
        let state = State {
            id: 0,
            name: "Root".into(),
            transitions: Vec::new(),
            clip: None,
            content: byroredux_hkx::StateContent { machines, generator: Some(generator), ..Default::default() },
        };
        let graph = BehaviorGraph {
            variables: variables
                .iter()
                .map(|(name, initial)| Variable { name: (*name).into(), kind: byroredux_hkx::VariableKind::Real, initial: *initial })
                .collect(),
            events: vec!["Wave".into(), "Select".into()],
            machine: Some(StateMachine { start_state: 0, start_state_variable: None, states: vec![state], wildcard: Vec::new() }),
        };
        let mut registered = HashMap::new();
        for (index, name) in clips.iter().enumerate() {
            registered.insert((name.to_string(), true), GraphClip { handle: 100 + index as u32, duration: 1.0 });
        }
        ObjectAnimationGraph::new(Arc::new(LoadedGraph { graph, clips: registered, sequences: HashMap::new(), lazy: None }), 7).unwrap()
    }

    /// A machine of plain looping-clip states; state `i` plays `clips[i]`, and event 0 takes
    /// state 0 to state 1.
    fn machine_of(clips: &[&str]) -> StateMachine {
        let states = clips
            .iter()
            .enumerate()
            .map(|(index, name)| State {
                id: index as i32,
                name: name.to_string(),
                transitions: if index == 0 && clips.len() > 1 { vec![transition(0, 1)] } else { Vec::new() },
                clip: Some(clip(name, ClipMode::Looping, 1.0, Vec::new())),
                content: byroredux_hkx::StateContent { generator: Some(Generator::Clip(clip(name, ClipMode::Looping, 1.0, Vec::new()))), ..Default::default() },
            })
            .collect();
        StateMachine { start_state: 0, start_state_variable: None, states, wildcard: Vec::new() }
    }

    /// The vanilla `MTRoot` shape: a layered (non-parametric) blender whose children are
    /// whole nested machines, each through a bone mask. The pose is the active clip of
    /// every machine, masked bone by bone; an event inside one machine swaps only its clip.
    #[test]
    fn a_layered_blend_masks_the_active_clips_of_nested_machines() {
        let child = |weight: f32, mask: Vec<f32>, machine: usize| BlendChild {
            weight,
            weight_variable: None,
            bone_weights: Some(byroredux_hkx::BoneWeights { weights: Arc::new(mask), bound: Vec::new() }),
            bone_weights_variable: None,
            generator: Generator::Machine(machine),
        };
        let blender = Generator::Blender(Blender {
            parametric: false,
            cyclic: false,
            sync: false,
            parameter: 0.0,
            parameter_variable: None,
            min_cyclic: 0.0,
            max_cyclic: 1.0,
            children: vec![child(1.0, vec![1.0, 1.0, 0.0, 0.0], 0), child(1.0, vec![0.0, 0.0, 1.0, 1.0], 1)],
        });
        let mut graph = rooted(blender, vec![machine_of(&["upper_idle", "upper_wave"]), machine_of(&["lower_stand"])], &[], &["upper_idle", "upper_wave", "lower_stand"]);
        let playback = graph.start().expect("plays");
        assert_eq!(playback.pose.len(), 2, "one clip from each machine");
        assert_eq!((playback.pose[0].handle, playback.pose[1].handle), (100, 102));
        let mask = |layer: &PoseLayer| layer.mask.as_ref().map(|mask| mask.to_vec());
        assert_eq!(mask(&playback.pose[0]), Some(vec![1.0, 1.0, 0.0, 0.0]), "the upper machine owns the upper bones");
        assert_eq!(mask(&playback.pose[1]), Some(vec![0.0, 0.0, 1.0, 1.0]));

        // `Wave` moves the upper machine only; the lower clip is untouched.
        let waved = graph.send_event(0).expect("Wave is answered").expect("a clip changed");
        assert_eq!((waved.pose[0].handle, waved.pose[1].handle), (101, 102));
    }

    /// A mask element can be driven by a variable (the vanilla upper-body mask follows
    /// `bEquipOk`); the layer's mask moves with it between events.
    #[test]
    fn a_bone_weight_bound_to_a_variable_follows_it() {
        let blender = Generator::Blender(Blender {
            parametric: false,
            cyclic: false,
            sync: false,
            parameter: 0.0,
            parameter_variable: None,
            min_cyclic: 0.0,
            max_cyclic: 1.0,
            children: vec![
                BlendChild {
                    weight: 1.0,
                    weight_variable: None,
                    bone_weights: Some(byroredux_hkx::BoneWeights { weights: Arc::new(vec![1.0, 1.0]), bound: vec![(1, 0)] }),
                    bone_weights_variable: None,
                    generator: Generator::Clip(clip("a", ClipMode::Looping, 1.0, Vec::new())),
                },
                BlendChild { weight: 1.0, weight_variable: None, bone_weights: None, bone_weights_variable: None, generator: Generator::Clip(clip("b", ClipMode::Looping, 1.0, Vec::new())) },
            ],
        });
        let mut graph = rooted(blender, Vec::new(), &[("bEquipOk", 1.0)], &["a", "b"]);
        let playback = graph.start().unwrap();
        assert_eq!(playback.pose[0].mask.as_ref().unwrap().to_vec(), vec![1.0, 1.0]);
        graph.set_variable("bEquipOk", 0.0);
        let layers = layers_of(graph.pose_now(0.0));
        assert_eq!(layers[0].mask.as_ref().unwrap().to_vec(), vec![1.0, 0.0], "bone 1 now follows the variable");
    }

    /// A selector picks its child by a variable, and a machine starts in the state its
    /// `startStateId` variable names; a variable that changes which clip plays restarts the
    /// pose without any event.
    #[test]
    fn selectors_and_start_state_variables_choose_what_plays() {
        let selector = Generator::Selector {
            index: 0,
            index_variable: Some(0),
            children: vec![
                Generator::Clip(clip("npc", ClipMode::Looping, 1.0, Vec::new())),
                Generator::Clip(clip("player", ClipMode::Looping, 1.0, Vec::new())),
            ],
        };
        let mut graph = rooted(selector, Vec::new(), &[("iIsPlayer", 0.0)], &["npc", "player"]);
        assert_eq!(graph.start().unwrap().handle, 100, "child 0 while the variable is 0");
        graph.set_variable("iIsPlayer", 1.0);
        match graph.pose_now(0.016) {
            Some(PoseUpdate::Restart(playback)) => assert_eq!(playback.handle, 101),
            other => panic!("the selector's variable changed the clip: {other:?}"),
        }
        assert!(graph.pose_now(0.016).is_none(), "nothing moved since");

        // Start state by variable: the machine's states 0 and 1 play different clips.
        let mut machine = machine_of(&["walk", "sneak"]);
        machine.start_state_variable = Some(0);
        let mut graph = rooted(Generator::Machine(0), vec![machine], &[("iIsInSneak", 1.0)], &["walk", "sneak"]);
        assert_eq!(graph.start().unwrap().handle, 101, "started in the state the variable names");
    }

    /// A boolean bound to a blender child's `boneWeights` switches the child (the camera
    /// machine of the vanilla root blender is off while `bAnimationDriven` is 0); a real
    /// bound there is not interpreted.
    #[test]
    fn a_boolean_bound_to_a_childs_bone_weights_switches_it() {
        let child = |name: &str, variable: Option<usize>| BlendChild {
            weight: 1.0,
            weight_variable: None,
            bone_weights: None,
            bone_weights_variable: variable,
            generator: Generator::Clip(clip(name, ClipMode::Looping, 1.0, Vec::new())),
        };
        let blender = Generator::Blender(Blender {
            parametric: false,
            cyclic: false,
            sync: false,
            parameter: 0.0,
            parameter_variable: None,
            min_cyclic: 0.0,
            max_cyclic: 1.0,
            children: vec![child("body", None), child("camera", Some(0)), child("lean", Some(1))],
        });
        let mut graph = rooted(blender, Vec::new(), &[("bAnimationDriven", 0.0), ("Speed", 0.0)], &["body", "camera", "lean"]);
        let mut shared = (*graph.graph).clone();
        shared.graph.variables[0].kind = byroredux_hkx::VariableKind::Bool;
        graph.graph = Arc::new(shared);
        let weights = |graph: &mut ObjectAnimationGraph| graph.start().unwrap().pose.iter().map(|layer| layer.weight).collect::<Vec<_>>();
        assert_eq!(weights(&mut graph), vec![1.0, 0.0, 1.0], "the camera child is off, the real-bound one is left alone");
        graph.set_variable("bAnimationDriven", 1.0);
        assert_eq!(weights(&mut graph), vec![1.0, 1.0, 1.0]);
    }

    /// The graph moves itself: an events-from-range modifier on the idle state watches
    /// `SpeedSmoothed` and raises `walkStart` when it enters the walk range, which the
    /// idle's own transition takes - no script and no host event involved. Entering the walk
    /// state activates its assign-variables modifier.
    #[test]
    fn a_modifier_raises_the_event_that_moves_the_graph() {
        use byroredux_hkx::{EventRange, Modifier, ModifierNode};
        let idle_clip = clip("idle", ClipMode::Looping, 1.0, Vec::new());
        let walk_clip = clip("walk", ClipMode::Looping, 1.0, Vec::new());
        let watch = Arc::new(ModifierNode {
            id: 1,
            kind: Modifier::EventsFromRange {
                input: 0.0,
                input_variable: Some(0),
                lower_bound: 0.0,
                ranges: vec![EventRange { upper: 150.0, event: 0, when_in_range: false }],
            },
        });
        let assign = Arc::new(ModifierNode { id: 2, kind: Modifier::AssignVariables { floats: Vec::new(), ints: vec![(1, 5)] } });
        let state = |id, name: &str, transitions, generator| State {
            id,
            name: name.into(),
            transitions,
            clip: None,
            content: byroredux_hkx::StateContent { generator: Some(generator), ..Default::default() },
        };
        let machine = StateMachine {
            start_state: 0,
            start_state_variable: None,
            states: vec![
                state(
                    0,
                    "Idle",
                    vec![transition(0, 1)],
                    Generator::Modified { modifiers: vec![watch], inner: Some(Box::new(Generator::Clip(idle_clip))) },
                ),
                state(
                    1,
                    "Walk",
                    Vec::new(),
                    Generator::Modified { modifiers: vec![assign], inner: Some(Box::new(Generator::Clip(walk_clip))) },
                ),
            ],
            wildcard: Vec::new(),
        };
        let variable = |name: &str, kind| Variable { name: name.into(), kind, initial: 0.0 };
        let graph = BehaviorGraph {
            variables: vec![variable("SpeedSmoothed", byroredux_hkx::VariableKind::Real), variable("iState", byroredux_hkx::VariableKind::Int)],
            events: vec!["walkStart".into()],
            machine: Some(machine),
        };
        let mut clips = HashMap::new();
        clips.insert(("idle".to_owned(), true), GraphClip { handle: 1, duration: 2.0 });
        clips.insert(("walk".to_owned(), true), GraphClip { handle: 2, duration: 1.0 });
        let mut graph = ObjectAnimationGraph::new(Arc::new(LoadedGraph { graph, clips, sequences: HashMap::new(), lazy: None }), 9).unwrap();
        assert_eq!(graph.start().unwrap().handle, 1);
        assert_eq!(graph.state(), 0);
        // A watcher raises an event when the input is *in* a range, so the input starts past
        // every range (a standing actor at speed 0 would be in the walk range already).
        graph.set_variable("SpeedSmoothed", 400.0);
        assert!(graph.step(0.016).is_empty(), "past every range: nothing raised");
        assert_eq!(graph.state(), 0);
        graph.set_variable("SpeedSmoothed", 80.0);
        let started = graph.step(0.016);
        assert_eq!(graph.state(), 1, "entering the walk range raised walkStart and the idle took it");
        assert_eq!(started.last().map(|playback| playback.handle), Some(2));
        assert_eq!(graph.variable("iState"), Some(5.0), "the walk state's assign-variables modifier activated");
    }

    /// A transition's duration comes from its bound variable when it has one.
    #[test]
    fn a_bound_duration_reads_its_variable() {
        let mut graph = locomotion();
        let mut shared = (*graph.graph).clone();
        let machine = shared.graph.machine.as_mut().unwrap();
        machine.states[0].transitions[0].blend_duration_variable = Some(0);
        graph.graph = Arc::new(shared);
        graph.start();
        graph.set_variable("Speed", 0.6);
        let playback = graph.send_event(0).unwrap().unwrap();
        assert_eq!(playback.fade, 0.6, "the variable, not the authored 0.25");
    }

    #[test]
    fn an_event_moves_the_machine_and_a_clip_end_raises_the_next() {
        let mut graph = lever();
        assert_eq!(graph.send_event(1), None, "Pulled has no transition in the start state");
        let playback = graph.send_event(0).expect("Down is taken").expect("the new state plays a clip");
        assert_eq!((playback.handle, playback.speed, playback.hold_at), (9, 2.0, None));
        assert_eq!(graph.state(), 2);
        // The clip is 0.8 s at speed 2, so 0.4 s of real time.
        assert!(graph.step(0.3).is_empty());
        assert_eq!(graph.state(), 2);
        let started = graph.step(0.2);
        assert_eq!(graph.state(), 1, "the end-of-clip Pulled event moved it on");
        assert_eq!(started[0].hold_at, Some(0.0), "settling into a held pose");
    }

    /// The swinging blade's shape: a root state holding two inner machines, one
    /// playing a looping trigger clip that raises `Apex` half a second in, the other
    /// idling until `Single` starts Gamebryo sequence `playanim01` and `Reset` ends it.
    fn blade() -> ObjectAnimationGraph {
        use byroredux_hkx::StateContent;
        let (single, reset, apex) = (0, 1, 2);
        let trigger_clip = clip(
            "a/trigger.hkx",
            ClipMode::Looping,
            1.0,
            vec![ClipTrigger { local_time: 0.5, event: apex, relative_to_end: false, annotation: true }],
        );
        let inner_trigger = StateMachine {
            start_state: 0,
            start_state_variable: None,
            states: vec![State {
                id: 0,
                name: "State00".into(),
                transitions: vec![],
                clip: Some(trigger_clip),
                content: StateContent::default(),
            }],
            wildcard: vec![],
        };
        let swing = |id, name: &str, sequence: Option<&str>, transitions| State {
            id,
            name: name.into(),
            transitions,
            clip: None,
            content: StateContent { sequences: sequence.map(str::to_owned).into_iter().collect(), ..Default::default() },
        };
        let inner_swing = StateMachine {
            start_state: 6,
            start_state_variable: None,
            states: vec![
                swing(6, "State06", None, vec![transition(single, 3)]),
                swing(3, "State03", Some("PlayAnim01"), vec![transition(reset, 6)]),
            ],
            wildcard: vec![],
        };
        let root = StateMachine {
            start_state: 5,
            start_state_variable: None,
            states: vec![State {
                id: 5,
                name: "State05".into(),
                transitions: vec![],
                clip: None,
                content: StateContent { machines: vec![inner_trigger, inner_swing], ..Default::default() },
            }],
            wildcard: vec![],
        };
        let graph = BehaviorGraph { variables: Vec::new(), events: vec!["Single".into(), "Reset".into(), "Apex".into()], machine: Some(root) };
        let mut clips = HashMap::new();
        clips.insert(("a/trigger.hkx".to_owned(), true), GraphClip { handle: 11, duration: 1.0 });
        let mut sequences = HashMap::new();
        sequences.insert("playanim01".to_owned(), GraphClip { handle: 21, duration: 2.0 });
        let mut graph = ObjectAnimationGraph::new(Arc::new(LoadedGraph { graph, clips, sequences, lazy: None }), 5).unwrap();
        graph.start();
        graph
    }

    /// An event the root machine does not answer reaches the nested machines; the
    /// nested machine that answers it changes state, and the motion becomes the
    /// Gamebryo sequence (the trigger-only clip stays as the idle pose and
    /// supplies the schedule); `Reset` returns to the clip.
    #[test]
    fn events_reach_nested_machines_and_a_sequence_is_the_motion() {
        let mut graph = blade();
        assert_eq!(graph.state(), 5, "the root machine never moves");
        let swing = graph.send_event(0).expect("the swing machine answers Single").expect("a sequence starts");
        assert_eq!((swing.handle, swing.speed, swing.hold_at), (21, 1.0, None));
        assert_eq!(graph.send_event(0), None, "State03 has no Single transition");
        let back = graph.send_event(1).expect("Reset ends the swing").expect("the idle clip resumes");
        assert_eq!(back.handle, 11, "the trigger clip is the motion again");
    }

    /// A graph can name a Gamebryo sequence its NIF does not carry (the swinging
    /// blade's `playanim01`); that leaf does not play and the clip is the motion.
    #[test]
    fn a_sequence_the_nif_lacks_is_not_the_motion() {
        let mut graph = blade();
        let mut shared = (*graph.graph).clone();
        shared.sequences.clear();
        graph.graph = Arc::new(shared);
        let swing = graph.send_event(0);
        assert!(swing.is_some(), "the machine still answers Single");
        assert_eq!(graph.start().map(|p| p.handle), Some(11), "the trigger clip plays");
    }

    /// A clip trigger in one nested machine raises its event on schedule while the
    /// other machine's state changes; the events are not lost when the root idles.
    #[test]
    fn nested_clip_triggers_keep_their_schedule_across_a_sibling_transition() {
        let mut graph = blade();
        graph.send_event(0).unwrap();
        assert!(graph.step(0.4).is_empty());
        graph.take_raised();
        graph.step(0.2);
        assert_eq!(graph.take_raised(), ["Apex"], "the trigger clip's 0.5 s event, despite the swing");
        let seconds = graph.seconds_until(2).expect("the next Apex is 1 s on");
        assert!((seconds - 0.9).abs() < 1.0e-4 || (seconds - 1.0).abs() < 1.0e-4, "{seconds}");
    }

    #[test]
    fn a_disabled_transition_never_fires_and_a_wildcard_answers_from_anywhere() {
        let mut graph = lever();
        graph.send_event(0).unwrap();
        assert_eq!(graph.send_event(2), None, "state 2's Up transition is disabled");
        assert!(graph.send_event(3).is_some(), "the wildcard Rest works from any state");
        assert_eq!(graph.state(), 0);
    }

    #[test]
    fn the_wait_until_an_event_is_computed_through_the_machine() {
        let mut graph = lever();
        graph.send_event(0).unwrap();
        let seconds = graph.seconds_until(1).expect("Pulled is raised at the clip's end");
        assert!((seconds - 0.4).abs() < 1.0e-5, "{seconds}");
        assert_eq!(graph.seconds_until(2), None, "Up is never raised");
    }

    #[test]
    fn a_started_playback_drives_the_objects_animation_player() {
        let mut world = World::new();
        world.register::<AnimationPlayer>();
        let root = world.spawn();
        apply_playback(
            &world,
            root,
            Playback { handle: 9, speed: 2.0, start_time: 0.25, hold_at: None, ..Default::default() },
        );
        {
            let player = world.get::<AnimationPlayer>(root).unwrap();
            assert_eq!((player.clip_handle, player.speed, player.local_time, player.playing), (9, 2.0, 0.25, true));
            assert_eq!(player.root_entity, Some(root));
        }
        apply_playback(&world, root, Playback { handle: 7, speed: 1.0, start_time: 0.0, hold_at: Some(0.4), ..Default::default() });
        let held = world.get::<AnimationPlayer>(root).unwrap();
        assert_eq!((held.clip_handle, held.local_time, held.playing), (7, 0.4, false));
    }

    #[test]
    fn a_script_archetype_overrides_and_zero_is_neutral() {
        let mut overrides = AnimArchetypeOverrides::default();
        assert_eq!(overrides.get(7), None);
        overrides.set(7, 0x22E49);
        assert_eq!(overrides.get(7), Some(0x22E49));
        overrides.set(7, 0);
        assert_eq!(overrides.get(7), Some(0));
    }

    /// An actor's graph resolves clips on demand: a state whose clip is not yet provided
    /// plays nothing and asks for it; once the host provides it, restarting the graph
    /// starts the clip, and a clip the host cannot resolve is not asked for again.
    #[test]
    fn a_lazy_clip_is_requested_then_played_once_provided() {
        let graph = BehaviorGraph {
            variables: Vec::new(),
            events: vec!["go".into()],
            machine: Some(StateMachine {
                start_state: 0,
                start_state_variable: None,
                states: vec![
                    State {
                        id: 0,
                        name: "A".into(),
                        transitions: vec![transition(0, 1)],
                        clip: Some(clip("Animations\\A.hkt", ClipMode::Looping, 1.0, Vec::new())),
                        content: Default::default(),
                    },
                    State {
                        id: 1,
                        name: "B".into(),
                        transitions: Vec::new(),
                        clip: Some(clip("Animations\\B.hkt", ClipMode::SinglePlay, 1.0, Vec::new())),
                        content: Default::default(),
                    },
                ],
                wildcard: Vec::new(),
            }),
        };
        let lazy = Arc::new(LazyClips::default());
        let loaded = Arc::new(LoadedGraph { graph, clips: HashMap::new(), sequences: HashMap::new(), lazy: Some(Arc::clone(&lazy)) });
        let mut run = ObjectAnimationGraph::new(loaded, 5).unwrap().with_animated_root(9);
        assert_eq!(run.animated(), 9);
        assert_eq!(run.start(), None, "nothing to play until the clip is provided");
        assert_eq!(lazy.take_wanted(), [("animations\\a.hkt".to_owned(), true)]);
        assert!(lazy.take_wanted().is_empty(), "asked once");
        lazy.provide(("animations\\a.hkt".to_owned(), true), GraphClip { handle: 3, duration: 2.0 });
        assert_eq!(run.start().map(|playback| playback.handle), Some(3));
        // The event moves to a clip nobody can resolve: no playback, and it is not re-asked.
        run.send_event(0);
        assert_eq!(lazy.take_wanted(), [("animations\\b.hkt".to_owned(), false)]);
        lazy.fail(("animations\\b.hkt".to_owned(), false));
        run.start();
        assert!(lazy.take_wanted().is_empty());
    }
}
