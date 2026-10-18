//! The lowering [`Scope`] (#5801, split from `effects.rs`): the local
//! environment a fragment body is lowered against.

use super::*;

/// The local-variable scope built while lowering a fragment body.
///
/// Distinguishes three name kinds so an effect receiver resolves
/// correctly: a local bound to a quest (`quest_locals`), a local of some
/// other type (`decl_locals` — `ObjectReference k = …`), and a bare
/// identifier that is neither (a script *property*, classified directly
/// by [`quest_via`]). The distinction matters: a declared local used as
/// an effect receiver but not quest-bound must **decline**, never be
/// misread as a same-named `Quest Property`.
#[derive(Clone, Default)]
pub(super) struct Scope<'lib> {
    /// M47.2 — where a call into the quest's own script (`kmyQuest.Startup()`)
    /// finds the callee's decompiled body to inline. `None` in every bare
    /// harness.
    pub(super) library: Option<&'lib dyn ScriptLibrary>,
    /// M47.2 — whether the fragment's VMAD is its quest's, so that the
    /// quest script's properties resolve through it. True for quest fragments
    /// and inlined quest-script bodies. On the INFO and scene paths (an
    /// INFO's or SCEN's own VMAD) an inlined callee is wrapped in
    /// [`Effect::QuestScope`] instead, and a quest script's own property
    /// (`kmyQuest.CW`) declines.
    pub(super) vmad_is_quest: bool,
    /// M47.2 — set while an INFO / scene statement is re-lowered against its
    /// quest's VMAD ([`quest_vmad_statement`]): the fragment's own properties
    /// (and locals bound from them) would resolve against the wrong VMAD
    /// there, so they decline.
    pub(super) fragment_properties_off: bool,
    /// The declared object type (lowercased script name) of each local
    /// (`MyQuestScript kmyQuest`), for resolving an inlined call's callee.
    pub(super) local_types: HashMap<String, String>,
    /// `(script, function)` frames currently being inlined — the cycle guard
    /// and depth cap of [`inline_quest_script_call`].
    pub(super) inline_stack: Vec<(String, String)>,
    /// M47.2 — inside an inlined quest-script body, the lowercased names of
    /// that script's own variables and properties (its quest variables).
    pub(super) script_variables: HashSet<String>,
    pub(super) quest_locals: HashMap<String, QuestRef>,
    pub(super) object_locals: HashMap<String, ObjectRef>,
    pub(super) player_locals: HashSet<String>,
    pub(super) decl_locals: HashSet<String>,
    /// M47.2 — locals (lowercased) holding a run-time number
    /// ([`Effect::SetLocal`]).
    pub(super) runtime_locals: HashSet<String>,
    /// M47.2 — names (lowercased) that denote the dialogue line's speaker:
    /// an INFO fragment's `akSpeakerRef` parameter and every local bound
    /// from it (`akSpeaker = akSpeakerRef as Actor`). Empty outside INFO
    /// fragments.
    pub(super) speaker_locals: HashSet<String>,
    /// Set when a receiver lowered to [`ActorRef::Speaker`], so the INFO
    /// entry point can refuse a speaker fragment with a latent tail. Shared
    /// (not copied) by the clones an `If` branch lowers against.
    pub(super) uses_speaker: std::rc::Rc<std::cell::Cell<bool>>,
    /// #5970 — statements lowered so far for the fragment being lowered, shared
    /// (not copied) by every clone and every inlined callee scope. Inlining
    /// re-lowers a callee body at each call site, so a chain of distinct
    /// functions each calling the next N times expands to N^depth effects; the
    /// budget ([`MAX_LOWERED_STATEMENTS`]) declines the whole fragment instead.
    pub(super) lowered_statements: std::rc::Rc<std::cell::Cell<usize>>,
    /// #2538 / SCR-D5-NEW10-01 — lowercased names of the containing
    /// script's `Quest Property` declarations (from VMAD-equivalent
    /// property-type metadata, threaded in from `&Script` at
    /// `lower_fragment_with_quest_properties`'s entry point). Nothing in
    /// the AST shape alone distinguishes a bare `Quest Property`
    /// reference from a bare scene-form property — `Quest.Start()` and
    /// `Scene.Start()` produce the identical zero-arg method-call AST —
    /// so `receiver_object` (the resolver `prim_start_scene`/
    /// `prim_stop_scene` and friends share) declines any bare identifier
    /// that appears here, rather than letting a name we *know* is a
    /// `Quest` property fall through and get silently accepted as a
    /// scene reference. Empty (via `lower_fragment`'s thin wrapper) for
    /// every call site that doesn't have script-level property metadata
    /// available — matches the pre-fix behavior for those.
    pub(super) known_quest_properties: HashSet<String>,
    /// #5407 — lowercased names of the containing script's own ARGLESS
    /// functions, threaded from `&Script` the way
    /// [`Self::known_quest_properties`] is. `AddRaceSpells` /
    /// `RemoveRaceSpells` are script-defined helpers (MQ101QuestScript
    /// authors both), not `actor.psc` natives: the race-spells
    /// primitives accept the call only when the calling script itself
    /// defines that name with zero parameters, so a mod's same-named
    /// helper with other semantics never lowers onto the player. Empty
    /// for call sites without script metadata — pre-fix behavior.
    pub(super) own_argless_functions: HashSet<String>,
}

/// What a declared local currently means to the lowerer — the identity
/// [`Scope::rebound_by`] compares across a branch.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalBinding {
    Quest(QuestRef),
    Object(ObjectRef),
    Player,
    Speaker,
    Plain,
}

/// Most statements one fragment may lower, counting every inlined callee body
/// (#5970). Population runs this at cell load on mod-supplied `.pex` data; a
/// real fragment is tens of statements, so this is a backstop against
/// multiplicative inlining, not a tuned limit.
pub(super) const MAX_LOWERED_STATEMENTS: usize = 10_000;

impl Scope<'_> {
    /// Charge `count` statements against the fragment's lowering budget;
    /// `None` once it is spent (the caller declines the fragment).
    pub(super) fn spend_statements(&self, count: usize) -> Option<()> {
        let spent = self.lowered_statements.get().saturating_add(count);
        self.lowered_statements.set(spent);
        (spent <= MAX_LOWERED_STATEMENTS).then_some(())
    }

    /// The binding of a name declared in this function, or `None` when no
    /// `VarDecl`/`Assign` has bound it.
    fn binding_of(&self, name: &str) -> Option<LocalBinding> {
        if let Some(via) = self.quest_locals.get(name) {
            Some(LocalBinding::Quest(via.clone()))
        } else if let Some(via) = self.object_locals.get(name) {
            Some(LocalBinding::Object(via.clone()))
        } else if self.player_locals.contains(name) {
            Some(LocalBinding::Player)
        } else if self.speaker_locals.contains(name) {
            Some(LocalBinding::Speaker)
        } else if self.decl_locals.contains(name) {
            Some(LocalBinding::Plain)
        } else {
            None
        }
    }

    /// Whether `name` is a local declared by this function (as opposed to a
    /// script variable or property).
    pub(super) fn declares(&self, name: &str) -> bool {
        self.binding_of(name).is_some()
    }

    /// Whether `branch` — a clone of `self` that a nested body was lowered
    /// against — rebinds any name `self` already declares.
    pub(super) fn rebound_by(&self, branch: &Scope) -> bool {
        let names = self
            .quest_locals
            .keys()
            .chain(self.object_locals.keys())
            .chain(self.player_locals.iter())
            .chain(self.speaker_locals.iter())
            .chain(self.decl_locals.iter());
        names.into_iter().any(|name| self.binding_of(name) != branch.binding_of(name))
    }
}
