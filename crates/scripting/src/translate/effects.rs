//! The effect-primitive table + fragment lowerer (b2) — the scaling
//! lever from [`docs/engine/m47-2-recognizer-scaling.md`].
//!
//! The corpus survey found 43,818 behavioral quest/scene/dialogue
//! `Fragment_*` functions (69.5% of the corpus), and that they are far
//! more compressible than event handlers: a ~500-primitive effect
//! vocabulary fully covers ~78% of them, because a fragment is almost
//! always a flat sequence of canonical effects (it is pre-gated by the
//! quest-stage contract, so it carries little control flow).
//!
//! This module lowers a fragment body to a `Vec<Effect>` through an
//! [`EffectPrimitive`] table — the effect sibling of
//! [`compose::GUARD_PRIMITIVES`](crate::translate::compose). It reuses
//! the same AST toolkit and the same **decline-on-any-unmodeled-term**
//! invariant: [`lower_fragment`] returns `None` the instant it meets a
//! statement no primitive claims, so a partially-understood fragment is
//! never partially applied.
//!
//! ## Scope (this increment)
//!
//! Quest-scoped stage/objective operations and a conservative set of
//! VMAD-resolved object effects are modelled. Object receivers can be direct
//! FormID properties, quest-alias `GetRef()`/`GetActorRef()` results, or locals
//! proven to hold those results; every other shape still declines as a whole.
//! The table covers quest/object state, scene control, player controls, and the
//! MQ101 cart cinematic boundary (idle, vehicle, motion, sitting, latent wait).
//!
//! ## Local binding
//!
//! Champollion emits `Quest k = GetOwningQuest()` / `Quest k = MyQuestProp`
//! then `k.SetStage(..)`. [`lower_fragment`] tracks those binding
//! assignments in a small local environment so a later effect on `k`
//! resolves to the right [`QuestRef`]. A binding to anything it can't
//! classify (a non-quest expression) is itself an unmodeled statement →
//! decline.

use std::collections::{HashMap, HashSet};

use byroredux_core::ecs::components::MotionType;
use byroredux_papyrus::ast::{BinaryOp, Expr, Stmt, UnaryOp};
use byroredux_papyrus::span::Spanned;

use crate::cinematic::CinematicAnimationEvent;
use crate::papyrus_provider::{lower_provider_call, PapyrusProviderCatalog};
use crate::player_control::PlayerControlSelection;
use crate::translate::compose::{
    as_num, classify_guard_atom, int_arg, is_game_get_player, method_call, quest_via, split_and,
    GuardMatch, ObjectRef, QuestRef,
};

/// One conservatively lowered `Quest.GetStageDone` predicate guarding a
/// fragment branch.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct StageDoneGuard {
    pub quest: QuestRef,
    pub stage: u16,
    pub done: bool,
}

/// Typed provider invocation deferred until fragment ECS guards are released.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct FragmentProviderCall {
    pub route: String,
    pub arguments: Vec<byroredux_sdk::script_function::ScriptValue>,
}

/// A canonical effect a fragment statement lowers to. The runtime applies
/// quest-scoped variants against [`QuestStageState`] / [`QuestObjectiveState`]
/// and object variants through the fragment's VMAD; see [`crate::fragment`].
///
/// [`QuestStageState`]: crate::quest_stages::QuestStageState
/// [`QuestObjectiveState`]: crate::quest_stages::QuestObjectiveState
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum Effect {
    /// `If <GetStageDone && ...> ... [Else ...] EndIf`. Only conjunctions
    /// of exact boolean stage predicates lower; elseif/disjunction and latent
    /// effects inside either branch remain decline-on-unknown.
    Conditional {
        guards: Vec<StageDoneGuard>,
        then_effects: Vec<Effect>,
        else_effects: Vec<Effect>,
    },
    /// `<global>.SetValue(value)` — resolves the VMAD-bound GLOB FormID and
    /// updates the canonical runtime [`crate::Globals`] table.
    SetGlobalValue { global: ObjectRef, value: f32 },
    /// `<quest>.SetStage(stage)`.
    SetStage { quest: QuestRef, stage: u16 },
    /// `<quest>.Start()`.
    StartQuest { quest: QuestRef },
    /// `<quest>.Stop()`.
    StopQuest { quest: QuestRef },
    /// `<quest>.CompleteQuest()`.
    CompleteQuest { quest: QuestRef },
    /// `<quest>.Reset()`.
    ResetQuest { quest: QuestRef },
    /// `<quest>.SetActive(active)`.
    SetQuestActive { quest: QuestRef, active: bool },
    /// `<quest>.SetObjectiveDisplayed(objective, displayed)`. Papyrus's
    /// optional `abForce` 3rd arg doesn't affect the stored state.
    SetObjectiveDisplayed {
        quest: QuestRef,
        objective: i32,
        displayed: bool,
    },
    /// `<quest>.SetObjectiveCompleted(objective, completed)`.
    SetObjectiveCompleted {
        quest: QuestRef,
        objective: i32,
        completed: bool,
    },
    /// `<quest>.SetObjectiveFailed(objective, failed)`.
    SetObjectiveFailed {
        quest: QuestRef,
        objective: i32,
        failed: bool,
    },
    /// `<quest>.CompleteAllObjectives()`.
    CompleteAllObjectives { quest: QuestRef },
    /// `<quest>.FailAllObjectives()`.
    FailAllObjectives { quest: QuestRef },
    /// `<container>.AddItem(<item>, <count>)`. The optional 3rd
    /// (`abSilent`) argument is accepted (parsed, not applied — no pickup
    /// notification UI exists to suppress) but only as a literal; a
    /// non-literal 3rd arg, or a 4th, declines the whole fragment (the
    /// `bool_arg` discipline `SetObjectiveDisplayed` already uses).
    /// `item` resolves only to a FormID at dispatch, never to an entity —
    /// it names a *base record* (the item type), not a placed reference.
    AddItem {
        container: ObjectRef,
        item: ObjectRef,
        count: u32,
    },
    /// `<actor>.EquipItem(<item>, false, <silent>)`. The supported shape
    /// deliberately declines `abPreventUnequip=true` until locked equipment
    /// has canonical state; the runtime updates `Inventory` +
    /// `EquipmentSlots` using the plugin-derived `EquipItemCatalog`.
    EquipItem {
        actor: ActorRef,
        item: ObjectRef,
        silent: bool,
    },
    /// `<moved>.MoveTo(<destination>)` — the conservative 2-arg shape
    /// only. A 3rd+ argument (offsets / match-rotation) declines the
    /// whole fragment rather than silently dropping the offset and
    /// misplacing the object.
    MoveTo {
        moved: ObjectRef,
        destination: ObjectRef,
    },
    /// `<object>.Disable([fadeOut])` — records the placed reference as
    /// disabled even when its cell/entity is not currently loaded.
    Disable { object: ObjectRef, fade_out: bool },
    /// `<scene>.Start()` — resolves a VMAD-bound SCEN FormID and queues the
    /// canonical scene start request at dispatch.
    StartScene { scene: ObjectRef },
    /// `<scene>.Stop()` — the symmetric explicit stop request.
    StopScene { scene: ObjectRef },
    /// `<target>.Activate()` (or an explicit `Game.GetPlayer()` activator).
    /// Dispatch emits the canonical one-frame [`crate::ActivateEvent`].
    Activate {
        target: ObjectRef,
        /// `None` means the player/default activator; `Some` resolves a
        /// VMAD property or quest-alias `GetRef()` expression.
        activator: Option<ObjectRef>,
    },
    /// `<target>.SetOpen(open)` — synchronizes the canonical two-state
    /// activator and its `::isOpen_var` CTDA-visible VM variable.
    SetOpen { target: ObjectRef, open: bool },
    /// `Game.GetPlayer().SetRestrained(restrained)`.
    SetPlayerRestrained { restrained: bool },
    /// Selectively enable/disable the domains named by the two Papyrus
    /// `*PlayerControls` global functions.
    SetPlayerControls {
        enabled: bool,
        selection: PlayerControlSelection,
    },
    /// `Game.SetPlayerAIDriven(ai_driven)`.
    SetPlayerAiDriven { ai_driven: bool },
    /// `Game.SetHudCartMode(cart_mode)` presentation state.
    SetHudCartMode { cart_mode: bool },
    /// `<actor>.PlayIdle(<idle>)`. The runtime preserves the IDLE FormID as
    /// an animation-backend request even when the current game uses HKX.
    PlayIdle { actor: ActorRef, idle: ObjectRef },
    /// `<actor>.SetVehicle(<vehicle>)`; `None` detaches the actor.
    SetVehicle {
        actor: ActorRef,
        vehicle: Option<ObjectRef>,
    },
    /// `<cart>.TetherToHorse(<horse>)`. The app preserves the captured cart
    /// pose relative to the package-driven horse.
    TetherToHorse { cart: ObjectRef, horse: ObjectRef },
    /// `<object>.SetMotionType(...)`, bridged to the app physics layer by a
    /// one-shot `MotionTypeChangeRequest`.
    SetMotionType {
        target: ObjectRef,
        motion_type: MotionType,
        allow_activate: bool,
    },
    /// `Game.SetSittingRotation(degrees)` camera/presentation state.
    SetSittingRotation { degrees: f32 },
    /// MQ101's `ExitCart(alias, seat)` helper. It detaches the actor and
    /// requests the helper's exact seat-specific exit IDLE.
    ExitCart { actor: ObjectRef, seat: u8 },
    /// MQ101 helper registrations for `PlayImod` / `IdleFurnitureExit`.
    RegisterPlayerAnimationEvent { event: CinematicAnimationEvent },
    /// `<actor>.EvaluatePackage()` — queues the actor's active scene package
    /// for condition re-selection and command restart on the next package tick.
    EvaluatePackage { actor: ObjectRef },
    /// Latent `Utility.Wait(seconds)`. Production dispatch pauses here and
    /// resumes the remaining effects through `FragmentExecutionQueue`.
    Wait { seconds: f32 },
    /// The exact bounded-work polling loop used by MQ101 Fragment_175:
    /// `While !actor.Is3DLoaded() || ...; Utility.Wait(poll); EndWhile`.
    /// Each continuation tick evaluates the list once and reschedules itself,
    /// avoiding a blocking script loop while preserving the authored gate.
    WaitForActors3DLoaded {
        actors: Vec<ActorRef>,
        poll_seconds: f32,
    },
    /// A manifest or engine-owned static provider call used as a sequencing
    /// barrier. Runtime dispatch occurs only after quest/object ECS guards are
    /// released, then resumes the remaining top-level effects in order.
    ProviderCall(FragmentProviderCall),
}

/// An actor receiver is either the canonical player or a VMAD-resolved
/// object/alias. Keeping the player explicit avoids inventing a VMAD property
/// for `Game.GetPlayer()`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ActorRef {
    Player,
    Object(ObjectRef),
}

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
struct Scope {
    quest_locals: HashMap<String, QuestRef>,
    object_locals: HashMap<String, ObjectRef>,
    player_locals: HashSet<String>,
    decl_locals: HashSet<String>,
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
    known_quest_properties: HashSet<String>,
}

/// Lower a predominantly flat fragment body to its canonical effects, or
/// decline.
///
/// Returns `None` (decline, the whole fragment) if the body contains any
/// control flow outside MQ101's exact `Is3DLoaded` polling-loop shape, or any
/// statement no effect primitive claims — never a partial lowering. An empty
/// body lowers to an empty effect list (a no-op fragment is trivially
/// understood).
///
/// Thin wrapper over [`lower_fragment_with_quest_properties`] with an
/// empty quest-property set, for the many call sites (mostly tests) with
/// no script-level property metadata available — preserves this
/// function's pre-#2538 behavior exactly for those.
///
/// #2658 (SCR-D5-NEW11-03) — `#[doc(hidden)]` because the context-free
/// path is test-only by design: the single production caller
/// (`fragment::populate_quest_fragments_from_script`) always calls
/// [`lower_fragment_with_quest_properties`] with a real set instead. Two
/// coverage/conformance measurement harnesses called this one anyway,
/// which for a time made them measure a lowering path production never
/// runs — hiding it from docs is a nudge against a future call site
/// picking it by mistake, not an access restriction (both examples and
/// every test in this crate can still see and call it fine).
#[doc(hidden)]
pub fn lower_fragment(body: &[Spanned<Stmt>]) -> Option<Vec<Effect>> {
    lower_fragment_with_quest_properties(body, &HashSet::new())
}

/// [`lower_fragment`], with the containing script's `Quest Property`
/// names (lowercased) made available to break the `Quest.Start()` vs
/// `Scene.Start()` ambiguity — see [`Scope::known_quest_properties`].
/// #2538 / SCR-D5-NEW10-01.
pub fn lower_fragment_with_quest_properties(
    body: &[Spanned<Stmt>],
    quest_property_names: &HashSet<String>,
) -> Option<Vec<Effect>> {
    lower_fragment_with_quest_properties_and_providers(body, quest_property_names, None)
}

/// Lower a fragment with an exact provider catalog. Top-level provider calls
/// become sequencing barriers; the runtime resumes later effects only after
/// guard-free host dispatch completes.
pub fn lower_fragment_with_quest_properties_and_providers(
    body: &[Spanned<Stmt>],
    quest_property_names: &HashSet<String>,
    providers: Option<&PapyrusProviderCatalog>,
) -> Option<Vec<Effect>> {
    let mut scope = Scope {
        known_quest_properties: quest_property_names.clone(),
        ..Scope::default()
    };
    lower_statements(body, &mut scope, providers)
}

fn lower_statements(
    body: &[Spanned<Stmt>],
    scope: &mut Scope,
    providers: Option<&PapyrusProviderCatalog>,
) -> Option<Vec<Effect>> {
    let mut effects = Vec::new();
    for stmt in body {
        match &stmt.node {
            // `Quest k = <quest-expr>` — a local quest binding. Other-typed
            // local decls are recorded (so a later misuse declines) but
            // contribute no effect. A bare decl (no initializer) is a
            // plain local.
            Stmt::VarDecl(var) => {
                let name = var.name.node.0.to_ascii_lowercase();
                match &var.initial_value {
                    Some(init) => bind_local(scope, name, &init.node)?,
                    None => {
                        scope.decl_locals.insert(name);
                    }
                }
            }
            // Re-assignment to an existing local: same binding rule.
            Stmt::Assign { target, value, .. } => {
                let Expr::Ident(name) = &target.node else {
                    return None; // assignment to a field/index — unmodeled
                };
                bind_local(scope, name.0.to_ascii_lowercase(), &value.node)?;
            }
            // `Return` with no value is Champollion's fragment terminator.
            Stmt::Return(None) => {}
            Stmt::ExprStmt(e) => {
                effects.push(classify_effect_with_providers(&e.node, scope, providers)?)
            }
            Stmt::While { condition, body } => {
                effects.push(lower_3d_loaded_wait(&condition.node, body, scope)?);
            }
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            } => {
                if !elseif_clauses.is_empty() {
                    return None;
                }
                let mut atoms = Vec::new();
                split_and(&condition.node, &mut atoms);
                let guards = atoms
                    .into_iter()
                    .map(|atom| match classify_guard_atom(atom, None)? {
                        GuardMatch::StageDone {
                            via,
                            stage,
                            expected,
                        } if expected == 0.0 || expected == 1.0 => Some(StageDoneGuard {
                            quest: via,
                            stage,
                            done: expected == 1.0,
                        }),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let mut then_scope = scope.clone();
                let then_effects = lower_statements(body, &mut then_scope, None)?;
                let mut else_scope = scope.clone();
                let else_effects = match else_body.as_deref() {
                    Some(body) => lower_statements(body, &mut else_scope, None)?,
                    None => Vec::new(),
                };
                let has_latent = |branch: &[Effect]| {
                    branch.iter().any(|effect| {
                        matches!(
                            effect,
                            Effect::Wait { .. } | Effect::WaitForActors3DLoaded { .. }
                        )
                    })
                };
                if has_latent(&then_effects) || has_latent(&else_effects) {
                    return None;
                }
                effects.push(Effect::Conditional {
                    guards,
                    then_effects,
                    else_effects,
                });
            }
            // Other control flow / valued return remain outside this
            // increment's conservative sequence model — decline.
            _ => return None,
        }
    }
    Some(effects)
}

/// Record a local's binding, or decline the whole fragment.
///
/// - A quest expression → `quest_locals`.
/// - A side-effect-free non-quest value (a literal, ident, member read,
///   arithmetic) → `decl_locals` (a plain local; a later misuse as a
///   quest receiver still declines via [`receiver_quest`]).
/// - A non-quest *side-effecting* initializer (e.g.
///   `k = akActor.PlaceAtMe(...)`) is an unmodeled statement whose effect
///   this table can't lower — decline rather than silently drop the
///   side-effect (#1907), matching the flat-sequence decline contract.
fn bind_local(scope: &mut Scope, name: String, init: &Expr) -> Option<()> {
    enum Binding {
        Quest(QuestRef),
        Player,
        Object(ObjectRef),
        Plain,
    }
    let binding = if let Some(via) = quest_expr_ref(init, scope) {
        Binding::Quest(via)
    } else if player_expr_ref(init, scope) {
        Binding::Player
    } else if let Some(via) = object_expr_ref(init, scope) {
        Binding::Object(via)
    } else if is_side_effect_free(init) {
        Binding::Plain
    } else {
        return None;
    };

    scope.quest_locals.remove(&name);
    scope.object_locals.remove(&name);
    scope.player_locals.remove(&name);
    scope.decl_locals.remove(&name);
    match binding {
        Binding::Quest(via) => {
            scope.quest_locals.insert(name, via);
        }
        Binding::Player => {
            scope.player_locals.insert(name);
        }
        Binding::Object(via) => {
            scope.object_locals.insert(name, via);
        }
        Binding::Plain => {
            scope.decl_locals.insert(name);
        }
    }
    Some(())
}

/// Whether evaluating `e` invokes no function/method call. Papyrus side
/// effects come from calls, so a non-quest initializer that contains a
/// `Call` can't be lowered to an effect and must decline (#1907).
fn is_side_effect_free(e: &Expr) -> bool {
    match e {
        Expr::Call { .. } => false,
        Expr::MemberAccess { object, .. } => is_side_effect_free(&object.node),
        Expr::Index { object, index } => {
            is_side_effect_free(&object.node) && is_side_effect_free(&index.node)
        }
        Expr::UnaryOp { operand, .. } => is_side_effect_free(&operand.node),
        Expr::BinaryOp { left, right, .. } => {
            is_side_effect_free(&left.node) && is_side_effect_free(&right.node)
        }
        Expr::Cast { expr, .. } => is_side_effect_free(&expr.node),
        Expr::New { size, .. } => is_side_effect_free(&size.node),
        Expr::ArrayLit(items) => items.iter().all(|i| is_side_effect_free(&i.node)),
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::BoolLit(_)
        | Expr::StringLit(_)
        | Expr::NoneLit
        | Expr::Ident(_)
        | Expr::ParentAccess => true,
    }
}

/// Classify a single effect statement against the primitive table.
fn classify_effect(e: &Expr, scope: &Scope) -> Option<Effect> {
    EFFECT_PRIMITIVES.iter().find_map(|p| p(e, scope))
}

fn classify_effect_with_providers(
    expression: &Expr,
    scope: &Scope,
    providers: Option<&PapyrusProviderCatalog>,
) -> Option<Effect> {
    if let Some(providers) = providers {
        match lower_provider_call(expression, providers) {
            Ok(Some(call)) => {
                return Some(Effect::ProviderCall(FragmentProviderCall {
                    route: call.route.qualified_name().to_owned(),
                    arguments: call.arguments,
                }));
            }
            Ok(None) => {}
            Err(_) => return None,
        }
    }
    classify_effect(expression, scope)
}

/// An effect primitive: matches one effect-call shape and binds its
/// holes (resolving the receiver to a [`QuestRef`] via `scope`), or
/// declines. Internal — the public surface is [`lower_fragment`].
type EffectPrimitive = fn(&Expr, &Scope) -> Option<Effect>;

/// The effect-primitive table. First match wins.
const EFFECT_PRIMITIVES: &[EffectPrimitive] = &[
    prim_set_global_value,
    prim_set_stage,
    prim_start_quest,
    prim_stop_quest,
    prim_complete_quest,
    prim_reset_quest,
    prim_set_quest_active,
    prim_set_objective_displayed,
    prim_set_objective_completed,
    prim_set_objective_failed,
    prim_complete_all_objectives,
    prim_fail_all_objectives,
    prim_add_item,
    prim_equip_item,
    prim_move_to,
    prim_disable,
    prim_start_scene,
    prim_stop_scene,
    prim_activate,
    prim_set_open,
    prim_set_player_restrained,
    prim_disable_player_controls,
    prim_enable_player_controls,
    prim_set_player_ai_driven,
    prim_set_hud_cart_mode,
    prim_play_idle,
    prim_set_vehicle,
    prim_tether_to_horse,
    prim_set_motion_type,
    prim_set_sitting_rotation,
    prim_exit_cart,
    prim_player_imod_animation,
    prim_player_furniture_animation,
    prim_evaluate_package,
    prim_wait,
];

/// Recognize MQ101's actor-load gate without opening the general-purpose
/// control-flow surface. The condition must be an OR tree whose leaves are
/// exactly `!<actor>.Is3DLoaded()`, and the loop body must be one positive
/// `Utility.Wait` call.
fn lower_3d_loaded_wait(condition: &Expr, body: &[Spanned<Stmt>], scope: &Scope) -> Option<Effect> {
    let [statement] = body else {
        return None;
    };
    let Stmt::ExprStmt(wait) = &statement.node else {
        return None;
    };
    let Effect::Wait {
        seconds: poll_seconds,
    } = prim_wait(&wait.node, scope)?
    else {
        return None;
    };
    if poll_seconds <= 0.0 {
        return None;
    }

    let mut actors = Vec::new();
    collect_not_3d_loaded_actors(condition, scope, &mut actors)?;
    (!actors.is_empty()).then_some(Effect::WaitForActors3DLoaded {
        actors,
        poll_seconds,
    })
}

fn collect_not_3d_loaded_actors(
    condition: &Expr,
    scope: &Scope,
    actors: &mut Vec<ActorRef>,
) -> Option<()> {
    if let Expr::BinaryOp {
        left,
        op: BinaryOp::Or,
        right,
    } = condition
    {
        collect_not_3d_loaded_actors(&left.node, scope, actors)?;
        collect_not_3d_loaded_actors(&right.node, scope, actors)?;
        return Some(());
    }

    let Expr::UnaryOp {
        op: UnaryOp::Not,
        operand,
    } = condition
    else {
        return None;
    };
    let (receiver, args) = method_call(&operand.node, "Is3DLoaded")?;
    if !args.is_empty() {
        return None;
    }
    actors.push(receiver_actor(receiver, scope)?);
    Some(())
}

// ── Effect primitives ────────────────────────────────────────────────

fn prim_set_global_value(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetValue")?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::SetGlobalValue {
        global: receiver_object(object, scope)?,
        value: as_num(&args[0].value.node)?,
    })
}

fn prim_set_stage(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetStage")?;
    let stage = u16::try_from(int_arg(args, 0)?).ok()?;
    Some(Effect::SetStage {
        quest: receiver_quest(object, scope)?,
        stage,
    })
}

/// Lookup key for [`Scope::known_quest_properties`].
///
/// A decompiled `.pex` reads an auto-property through its *backing
/// variable* (`::MQ101_var`), not the authored name (`MQ101`) the property
/// table is keyed by — the same `::`/`_var` decoration
/// [`QuestRef::property_name`]/[`ObjectRef::property_name`] strip at
/// dispatch. Normalizing here is what makes the set match on real input
/// rather than only on `.psc`-built test bodies (#2657); the `.psc` `Ident`
/// regex cannot even produce a `::X_var` identifier.
fn quest_property_key(name: &str) -> String {
    name.strip_prefix("::")
        .and_then(|n| n.strip_suffix("_var"))
        .unwrap_or(name)
        .to_ascii_lowercase()
}

/// A receiver that is *unambiguously* a quest: `Self`, `GetOwningQuest()`,
/// a local explicitly declared `Quest k = …`, or a bare identifier the
/// containing script declares as a `Quest Property`.
///
/// Everything else declines. This exists because several modeled method
/// names (`Start`, `Stop`, `Reset`, `SetActive`) are declared on more than
/// one receiver type in the Papyrus API, and nothing in the AST shape
/// distinguishes them — `<ident>.Reset()` is `Quest.Reset()`,
/// `ObjectReference.Reset()` or `Cell.Reset()` depending only on the
/// property's declared type.
fn explicit_quest_receiver(object: &Expr, scope: &Scope) -> Option<QuestRef> {
    let quest = receiver_quest(object, scope)?;
    match &quest {
        QuestRef::SelfRef | QuestRef::OwningQuest => Some(quest),
        QuestRef::Property(_) => {
            let Expr::Ident(name) = object else {
                return None;
            };
            let key = quest_property_key(&name.0);
            (scope.quest_locals.contains_key(&key)
                || scope
                    .known_quest_properties
                    .contains(&quest_property_key(&name.0)))
            .then_some(quest)
        }
    }
}

fn prim_start_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Start")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StartQuest {
        quest: explicit_quest_receiver(object, scope)?,
    })
}

fn prim_stop_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Stop")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StopQuest {
        quest: explicit_quest_receiver(object, scope)?,
    })
}

fn prim_complete_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "CompleteQuest")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::CompleteQuest {
        quest: receiver_quest(object, scope)?,
    })
}

/// `Reset` is declared on `Quest`, `ObjectReference` *and* `Cell`
/// (SCR-D5-NEW11-02 / #2653), so the receiver must be unambiguously a
/// quest — a bare `ObjectReference Property` would otherwise be claimed
/// here and its real `Reset()` semantics silently dropped.
fn prim_reset_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Reset")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::ResetQuest {
        quest: explicit_quest_receiver(object, scope)?,
    })
}

/// `SetActive` is declared on `Quest` *and* `Weather`
/// (SCR-D5-NEW11-02 / #2653) — same narrow-receiver requirement as
/// [`prim_reset_quest`].
fn prim_set_quest_active(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetActive")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetQuestActive {
        quest: explicit_quest_receiver(object, scope)?,
        active: bool_arg(args, 0)?.unwrap_or(true),
    })
}

fn prim_set_objective_displayed(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetObjectiveDisplayed")?;
    let objective = i32::try_from(int_arg(args, 0)?).ok()?;
    // Optional 2nd arg `abDisplayed` defaults to true in Papyrus.
    let displayed = bool_arg(args, 1)?.unwrap_or(true);
    Some(Effect::SetObjectiveDisplayed {
        quest: receiver_quest(object, scope)?,
        objective,
        displayed,
    })
}

fn prim_set_objective_completed(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetObjectiveCompleted")?;
    let objective = i32::try_from(int_arg(args, 0)?).ok()?;
    let completed = bool_arg(args, 1)?.unwrap_or(true);
    Some(Effect::SetObjectiveCompleted {
        quest: receiver_quest(object, scope)?,
        objective,
        completed,
    })
}

fn prim_set_objective_failed(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetObjectiveFailed")?;
    let objective = i32::try_from(int_arg(args, 0)?).ok()?;
    let failed = bool_arg(args, 1)?.unwrap_or(true);
    Some(Effect::SetObjectiveFailed {
        quest: receiver_quest(object, scope)?,
        objective,
        failed,
    })
}

fn prim_complete_all_objectives(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "CompleteAllObjectives")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::CompleteAllObjectives {
        quest: receiver_quest(object, scope)?,
    })
}

fn prim_fail_all_objectives(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "FailAllObjectives")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::FailAllObjectives {
        quest: receiver_quest(object, scope)?,
    })
}

fn prim_add_item(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "AddItem")?;
    let container = receiver_object(object, scope)?;
    let item = receiver_object(&args.first()?.value.node, scope)?;
    let count = u32::try_from(int_arg(args, 1)?).ok()?;
    // Optional 3rd arg (`abSilent`) — accepted only as a literal (parsed,
    // not applied; see the `Effect::AddItem` doc). A present-but-
    // non-literal 3rd arg declines via `bool_arg`'s `None`; a 4th arg
    // entirely is outside this primitive's understood shape.
    bool_arg(args, 2)?;
    if args.len() > 3 {
        return None;
    }
    Some(Effect::AddItem {
        container,
        item,
        count,
    })
}

fn prim_equip_item(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "EquipItem")?;
    if args.is_empty() || args.len() > 3 {
        return None;
    }
    // Prevent-unequip needs persistent locked-equipment state. Decline that
    // shape instead of claiming it and silently losing the lock contract.
    if bool_arg(args, 1)?.unwrap_or(false) {
        return None;
    }
    Some(Effect::EquipItem {
        actor: receiver_actor(object, scope)?,
        item: receiver_object(&args[0].value.node, scope)?,
        silent: bool_arg(args, 2)?.unwrap_or(false),
    })
}

fn prim_move_to(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "MoveTo")?;
    // The conservative 2-arg shape only (receiver + destination) — a 3rd+
    // argument (offsets / match-rotation) declines rather than silently
    // dropping it and misplacing the object.
    if args.len() != 1 {
        return None;
    }
    let moved = receiver_object(object, scope)?;
    let destination = receiver_object(&args[0].value.node, scope)?;
    Some(Effect::MoveTo { moved, destination })
}

fn prim_disable(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Disable")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::Disable {
        object: receiver_object(object, scope)?,
        fade_out: bool_arg(args, 0)?.unwrap_or(false),
    })
}

fn prim_start_scene(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Start")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StartScene {
        scene: receiver_object(object, scope)?,
    })
}

fn prim_stop_scene(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Stop")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StopScene {
        scene: receiver_object(object, scope)?,
    })
}

fn prim_activate(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Activate")?;
    if args.len() > 2 {
        return None;
    }
    // ObjectReference.Activate's optional first arg is the activator. The
    // MQ101 fragment corpus uses either the default or Game.GetPlayer();
    // decline other runtime expressions rather than invent their identity.
    let activator = match args.first().map(|arg| &arg.value.node) {
        None | Some(Expr::NoneLit) => None,
        Some(expression) if is_game_get_player(expression) => None,
        Some(expression) => Some(receiver_object(expression, scope)?),
    };
    // `abDefaultProcessingOnly=true` explicitly bypasses attached-script
    // OnActivate handling, which this canonical event represents. Accept
    // only the default/false shape until native-only activation exists.
    if bool_arg(args, 1)? == Some(true) {
        return None;
    }
    Some(Effect::Activate {
        target: receiver_object(object, scope)?,
        activator,
    })
}

fn prim_set_open(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetOpen")?;
    if args.len() > 1 {
        return None;
    }
    let open = bool_arg(args, 0)?.unwrap_or(true);
    Some(Effect::SetOpen {
        target: receiver_object(object, scope)?,
        open,
    })
}

fn prim_set_player_restrained(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetRestrained")?;
    if !is_game_get_player(object) || args.len() > 1 {
        return None;
    }
    Some(Effect::SetPlayerRestrained {
        restrained: bool_arg(args, 0)?.unwrap_or(true),
    })
}

fn prim_disable_player_controls(e: &Expr, _scope: &Scope) -> Option<Effect> {
    prim_player_controls(e, "DisablePlayerControls", false)
}

fn prim_enable_player_controls(e: &Expr, _scope: &Scope) -> Option<Effect> {
    prim_player_controls(e, "EnablePlayerControls", true)
}

fn prim_player_controls(e: &Expr, method: &str, enabled: bool) -> Option<Effect> {
    let args = game_call(e, method)?;
    if args.len() > 9 {
        return None;
    }
    let defaults = PlayerControlSelection::PAPYRUS_DEFAULT;
    let selection = PlayerControlSelection {
        movement: bool_arg(args, 0)?.unwrap_or(defaults.movement),
        fighting: bool_arg(args, 1)?.unwrap_or(defaults.fighting),
        camera_switching: bool_arg(args, 2)?.unwrap_or(defaults.camera_switching),
        looking: bool_arg(args, 3)?.unwrap_or(defaults.looking),
        sneaking: bool_arg(args, 4)?.unwrap_or(defaults.sneaking),
        menu: bool_arg(args, 5)?.unwrap_or(defaults.menu),
        activation: bool_arg(args, 6)?.unwrap_or(defaults.activation),
        journal_tabs: bool_arg(args, 7)?.unwrap_or(defaults.journal_tabs),
        pov_type: optional_int_arg(args, 8, defaults.pov_type)?,
    };
    Some(Effect::SetPlayerControls { enabled, selection })
}

fn prim_set_player_ai_driven(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "SetPlayerAIDriven")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetPlayerAiDriven {
        ai_driven: bool_arg(args, 0)?.unwrap_or(true),
    })
}

fn prim_set_hud_cart_mode(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "SetHudCartMode")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetHudCartMode {
        cart_mode: bool_arg(args, 0)?.unwrap_or(true),
    })
}

fn prim_play_idle(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "PlayIdle")?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::PlayIdle {
        actor: receiver_actor(object, scope)?,
        idle: receiver_object(&args[0].value.node, scope)?,
    })
}

fn prim_set_vehicle(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetVehicle")?;
    if args.len() != 1 {
        return None;
    }
    let vehicle = match &args[0].value.node {
        Expr::NoneLit => None,
        value => Some(receiver_object(value, scope)?),
    };
    Some(Effect::SetVehicle {
        actor: receiver_actor(object, scope)?,
        vehicle,
    })
}

fn prim_tether_to_horse(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "TetherToHorse")?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::TetherToHorse {
        cart: receiver_object(object, scope)?,
        horse: receiver_object(&args[0].value.node, scope)?,
    })
}

fn prim_set_motion_type(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetMotionType")?;
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    let target = receiver_object(object, scope)?;
    let motion_type = motion_type_arg(&args[0].value.node, &target, scope)?;
    let allow_activate = bool_arg(args, 1)?.unwrap_or(true);
    Some(Effect::SetMotionType {
        target,
        motion_type,
        allow_activate,
    })
}

fn motion_type_arg(value: &Expr, target: &ObjectRef, scope: &Scope) -> Option<MotionType> {
    if let Expr::Cast { expr, .. } = value {
        return motion_type_arg(&expr.node, target, scope);
    }
    if let Expr::IntLit(raw) = value {
        return match *raw {
            // Canonical hkpMotion::MotionType table. Keep this in lockstep
            // with the NIF collision importer (#1652 / #2286).
            1..=5 | 8 => Some(MotionType::Dynamic),
            6 => Some(MotionType::Keyframed),
            7 => Some(MotionType::Static),
            9 => Some(MotionType::CharacterKinematic),
            _ => None,
        };
    }
    let Expr::MemberAccess { object, member } = value else {
        return None;
    };
    if receiver_object(&object.node, scope).as_ref() != Some(target) {
        return None;
    }
    match member.node.0.to_ascii_lowercase().as_str() {
        "motion_dynamic" => Some(MotionType::Dynamic),
        "motion_keyframed" => Some(MotionType::Keyframed),
        "motion_fixed" => Some(MotionType::Static),
        "motion_character" => Some(MotionType::CharacterKinematic),
        _ => None,
    }
}

fn prim_set_sitting_rotation(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "SetSittingRotation")?;
    if args.len() != 1 {
        return None;
    }
    let degrees = as_num(&args[0].value.node)?;
    degrees
        .is_finite()
        .then_some(Effect::SetSittingRotation { degrees })
}

fn prim_exit_cart(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "ExitCart")?;
    if args.len() != 2 {
        return None;
    }
    // `ExitCart` is an MQ101 quest helper, so require a proven quest receiver
    // rather than accepting an arbitrary object method with the same name.
    receiver_quest(object, scope)?;
    let seat = u8::try_from(int_arg(args, 1)?).ok()?;
    if !(1..=5).contains(&seat) {
        return None;
    }
    Some(Effect::ExitCart {
        actor: receiver_object(&args[0].value.node, scope)?,
        seat,
    })
}

fn prim_player_imod_animation(e: &Expr, scope: &Scope) -> Option<Effect> {
    prim_player_animation_event(
        e,
        scope,
        "PlayerImodAnimation",
        CinematicAnimationEvent::PlayImod,
    )
}

fn prim_player_furniture_animation(e: &Expr, scope: &Scope) -> Option<Effect> {
    prim_player_animation_event(
        e,
        scope,
        "PlayerFurnitureAnimation",
        CinematicAnimationEvent::IdleFurnitureExit,
    )
}

fn prim_player_animation_event(
    e: &Expr,
    scope: &Scope,
    method: &str,
    event: CinematicAnimationEvent,
) -> Option<Effect> {
    let (object, args) = method_call(e, method)?;
    if !args.is_empty() {
        return None;
    }
    receiver_quest(object, scope)?;
    Some(Effect::RegisterPlayerAnimationEvent { event })
}

fn prim_evaluate_package(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "EvaluatePackage")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::EvaluatePackage {
        actor: receiver_object(object, scope)?,
    })
}

fn prim_wait(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Utility", "Wait")?;
    if args.len() != 1 {
        return None;
    }
    let seconds = as_num(&args[0].value.node)?;
    (seconds.is_finite() && seconds >= 0.0).then_some(Effect::Wait { seconds })
}

fn game_call<'a>(e: &'a Expr, method: &str) -> Option<&'a [byroredux_papyrus::ast::CallArg]> {
    static_call(e, "Game", method)
}

fn static_call<'a>(
    e: &'a Expr,
    type_name: &str,
    method: &str,
) -> Option<&'a [byroredux_papyrus::ast::CallArg]> {
    let (object, args) = method_call(e, method)?;
    matches!(object, Expr::Ident(name) if name.0.eq_ignore_ascii_case(type_name)).then_some(args)
}

fn optional_int_arg(
    args: &[byroredux_papyrus::ast::CallArg],
    idx: usize,
    default: i32,
) -> Option<i32> {
    match args.get(idx) {
        None => Some(default),
        Some(_) => i32::try_from(int_arg(args, idx)?).ok(),
    }
}

// ── Receiver / quest-expr resolution ─────────────────────────────────

/// Resolve a call receiver to a [`QuestRef`]:
/// - a local bound to a quest → its bound `QuestRef`;
/// - a declared-but-not-quest-bound local used as a receiver → `None`
///   (decline — it is *not* a same-named property);
/// - otherwise classify directly (`Self` / `Self.GetOwningQuest()` / a
///   `Quest Property`).
fn receiver_quest(object: &Expr, scope: &Scope) -> Option<QuestRef> {
    if let Expr::Ident(name) = object {
        let key = name.0.to_ascii_lowercase();
        if let Some(via) = scope.quest_locals.get(&key) {
            return Some(via.clone());
        }
        if scope.decl_locals.contains(&key) {
            return None;
        }
    }
    quest_via(object)
}

/// Classify the RHS of a `local = <expr>` binding as a quest reference,
/// resolving a local-to-local copy through `scope`.
fn quest_expr_ref(value: &Expr, scope: &Scope) -> Option<QuestRef> {
    match value {
        Expr::Cast { expr, .. } => quest_expr_ref(&expr.node, scope),
        _ => receiver_quest(value, scope),
    }
}

/// Resolve a pure object lookup used as a local initializer. Bare properties
/// are deliberately excluded here because the decompiled local declaration's
/// type is not carried into this helper; the unambiguous supported shapes are
/// alias `GetRef`/`GetActorRef` calls and copies of an already-known object
/// local.
fn object_expr_ref(value: &Expr, scope: &Scope) -> Option<ObjectRef> {
    if let Expr::Cast { expr, .. } = value {
        return object_expr_ref(&expr.node, scope);
    }
    if let Expr::Ident(name) = value {
        return scope
            .object_locals
            .get(&name.0.to_ascii_lowercase())
            .cloned();
    }
    let (alias, args) =
        method_call(value, "GetRef").or_else(|| method_call(value, "GetActorRef"))?;
    if !args.is_empty() {
        return None;
    }
    receiver_object(alias, scope)
}

fn player_expr_ref(value: &Expr, scope: &Scope) -> bool {
    match value {
        Expr::Cast { expr, .. } => player_expr_ref(&expr.node, scope),
        Expr::Ident(name) => scope.player_locals.contains(&name.0.to_ascii_lowercase()),
        _ => is_game_get_player(value),
    }
}

/// Resolve an object-targeting effect's receiver or argument to an
/// [`ObjectRef`]. Unlike [`receiver_quest`], there is no unambiguous
/// bare-identifier case (no `Self`/`GetOwningQuest()` equivalent — see
/// [`ObjectRef`]'s doc). A bare property is accepted, and a local is accepted
/// only when [`bind_local`] proved it came from a VMAD alias getter; unrelated
/// or plain locals still decline.
fn receiver_object(object: &Expr, scope: &Scope) -> Option<ObjectRef> {
    if let Expr::Cast { expr, .. } = object {
        return receiver_object(&expr.node, scope);
    }
    let alias_getter = method_call(object, "GetRef").or_else(|| method_call(object, "GetActorRef"));
    if let Some((alias, args)) = alias_getter {
        if !args.is_empty() {
            return None;
        }
        return receiver_object(alias, scope);
    }
    let Expr::Ident(name) = object else {
        return None;
    };
    let key = name.0.to_ascii_lowercase();
    if let Some(via) = scope.object_locals.get(&key) {
        return Some(via.clone());
    }
    // `Self` is never the object here (see `ObjectRef`'s doc) — decline
    // explicitly rather than relying on no VMAD ever naming a property
    // "self".
    //
    // #2538 / SCR-D5-NEW10-01 — a bare identifier known (from the
    // containing script's own property declarations) to be `Quest`-typed
    // must decline here too: `Quest.Start()`/`Scene.Start()` share the
    // identical zero-arg AST shape, and without this check a Quest
    // Property that `explicit_quest_receiver` correctly declined (not
    // `Self`/`GetOwningQuest()`/a bound local) would fall through the
    // primitive table and get silently misclassified as a scene
    // reference instead of the whole statement declining.
    if key == "self"
        || scope.quest_locals.contains_key(&key)
        || scope.player_locals.contains(&key)
        || scope.decl_locals.contains(&key)
        || scope
            .known_quest_properties
            .contains(&quest_property_key(&name.0))
    {
        return None;
    }
    Some(ObjectRef::Property(name.0.clone()))
}

fn receiver_actor(object: &Expr, scope: &Scope) -> Option<ActorRef> {
    if player_expr_ref(object, scope) {
        Some(ActorRef::Player)
    } else {
        receiver_object(object, scope).map(ActorRef::Object)
    }
}

/// A boolean positional argument — `Bool`/`Int` literal, unwrapping a
/// cast (mirrors [`as_num`]'s tolerance).
///
/// Three-case contract (#2023 / SCR-D5-NEW2-01), matching the fix
/// [`rumble::bool_prop`]/`float_prop` already got under #1909: `None`
/// when the argument slot is present but `as_num` can't evaluate it (a
/// local variable, `Not(...)`, a copy-propagated temp) — decline, the
/// caller must NOT assume the Papyrus-side default applies, since the
/// real runtime value is unknown. `Some(None)` when the slot is
/// genuinely absent (the call omitted the optional argument) — safe to
/// apply the default. `Some(Some(v))` when present and a literal.
///
/// Pre-fix, both "absent" and "present but non-literal" collapsed into
/// a single `None` that every call site turned into the Papyrus default
/// via `.unwrap_or(true)` — silently discarding a real (just unresolved)
/// runtime value.
fn bool_arg(args: &[byroredux_papyrus::ast::CallArg], idx: usize) -> Option<Option<bool>> {
    match args.get(idx) {
        None => Some(None),
        Some(arg) => as_num(&arg.value.node).map(|n| Some(n != 0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_papyrus::ast::{CallArg, Identifier, ScriptItem, StateItem};
    use byroredux_papyrus::parse_script;

    /// Wrap a node with a dummy span, for ASTs the `.psc` parser cannot
    /// express (e.g. a `::X_var` backing-variable identifier).
    fn sp<T>(node: T) -> Spanned<T> {
        Spanned::new(node, byroredux_papyrus::span::Span::new(0, 0))
    }

    /// Parse a script and return the body of its first function/event
    /// named like a fragment (or just the first function), to drive
    /// lowering on realistic shapes.
    fn first_fn_body(src: &str) -> Vec<Spanned<Stmt>> {
        let (script, errs) = parse_script(src).expect("parses");
        assert!(errs.is_empty(), "{errs:?}");
        for item in &script.body {
            match &item.node {
                ScriptItem::Function(f) => return f.body.clone(),
                ScriptItem::Event(e) => return e.body.clone(),
                ScriptItem::State(st) => {
                    if let Some(si) = st.body.first() {
                        match &si.node {
                            StateItem::Function(f) => return f.body.clone(),
                            StateItem::Event(e) => return e.body.clone(),
                        }
                    }
                }
                _ => {}
            }
        }
        panic!("no function/event body found");
    }

    #[test]
    fn lowers_self_set_stage() {
        // The `{self.setstage(#)}` family — Self is the quest.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n Self.SetStage(20)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 20
            }])
        );
    }

    #[test]
    fn lowers_global_set_value_before_stage_handoff() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             GlobalVariable Property GameHour Auto\n\
             Function Fragment_0()\n\
             GameHour.SetValue(7.0)\n\
             Self.SetStage(10)\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetGlobalValue {
                    global: ObjectRef::Property("GameHour".into()),
                    value: 7.0,
                },
                Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 10,
                },
            ])
        );
    }

    #[test]
    fn provider_calls_preserve_top_level_fragment_order() {
        let providers = PapyrusProviderCatalog::engine_compatibility();
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Self.SetStage(10)\n\
             Game.GetModCount()\n\
             Game.IsPluginInstalled(\"Update.esm\")\n\
             EndFunction\n",
        );
        let effects = lower_fragment_with_quest_properties_and_providers(
            &body,
            &HashSet::new(),
            Some(&providers),
        )
        .unwrap();
        assert!(matches!(effects[0], Effect::SetStage { stage: 10, .. }));
        assert!(matches!(
            &effects[1],
            Effect::ProviderCall(call)
                if call.route == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_COUNT_ROUTE
        ));
        assert!(matches!(
            &effects[2],
            Effect::ProviderCall(call)
                if call.route
                    == byroredux_sdk::compatibility::PAPYRUS_GAME_IS_PLUGIN_INSTALLED_ROUTE
        ));

        let reordered = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Game.GetModCount()\n\
             Self.SetStage(10)\n\
             EndFunction\n",
        );
        let reordered_effects = lower_fragment_with_quest_properties_and_providers(
            &reordered,
            &HashSet::new(),
            Some(&providers),
        )
        .unwrap();
        assert!(matches!(reordered_effects[0], Effect::ProviderCall(_)));
        assert!(matches!(
            reordered_effects[1],
            Effect::SetStage { stage: 10, .. }
        ));
    }

    #[test]
    fn lowers_quest_lifecycle_and_bulk_objective_effects() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Self.Start()\n\
             Self.SetActive(true)\n\
             Self.FailAllObjectives()\n\
             Self.CompleteQuest()\n\
             Self.Stop()\n\
             Self.Reset()\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::StartQuest {
                    quest: QuestRef::SelfRef,
                },
                Effect::SetQuestActive {
                    quest: QuestRef::SelfRef,
                    active: true,
                },
                Effect::FailAllObjectives {
                    quest: QuestRef::SelfRef,
                },
                Effect::CompleteQuest {
                    quest: QuestRef::SelfRef,
                },
                Effect::StopQuest {
                    quest: QuestRef::SelfRef,
                },
                Effect::ResetQuest {
                    quest: QuestRef::SelfRef,
                },
            ])
        );
    }

    #[test]
    fn lowers_bound_local_then_set_stage() {
        // The top fragment template `{$=$;$.setstage(#)}`: a quest local
        // bound from GetOwningQuest then used.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             Quest kmyQuest = Self.GetOwningQuest()\n\
             kmyQuest.SetStage(30)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetStage {
                quest: QuestRef::OwningQuest,
                stage: 30
            }])
        );
    }

    #[test]
    fn lowers_objective_pair() {
        // `{self.setobjectivecompleted(#,#);self.setobjectivedisplayed(#,#,#)}`.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_2()\n\
             Self.SetObjectiveCompleted(10)\n\
             Self.SetObjectiveDisplayed(20)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetObjectiveCompleted {
                    quest: QuestRef::SelfRef,
                    objective: 10,
                    completed: true,
                },
                Effect::SetObjectiveDisplayed {
                    quest: QuestRef::SelfRef,
                    objective: 20,
                    displayed: true,
                },
            ])
        );
    }

    #[test]
    fn objective_explicit_false_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_3()\n Self.SetObjectiveDisplayed(5, false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetObjectiveDisplayed {
                quest: QuestRef::SelfRef,
                objective: 5,
                displayed: false,
            }])
        );
    }

    #[test]
    fn objective_non_literal_arg_declines_whole_fragment() {
        // #2023 / SCR-D5-NEW2-01 — a present-but-non-literal 2nd argument
        // (an ordinary local bool, unconstrained Papyrus unlike the
        // auto-property-initializer case `bool_prop` guards) must NOT
        // silently collapse into the Papyrus-side default `true`. Pre-fix
        // `bool_arg` returned a single `None` for both "absent" and
        // "present but unevaluable," so `.unwrap_or(true)` masked this
        // exact case — asserting `completed: true` here would have
        // passed both before and after the fix, so the meaningful
        // assertion is that the whole fragment declines (`None`), not a
        // specific wrong value.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             Bool bWasSuccessful = true\n\
             Self.SetObjectiveCompleted(20, bWasSuccessful)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn declines_on_unmodeled_effect() {
        // An object-targeting effect (Enable) isn't in this increment's
        // table — the whole fragment declines, never partially applies.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_4()\n\
             Self.SetStage(10)\n\
             akTarget.Enable()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn declines_on_side_effecting_binding() {
        // A non-quest binding whose initializer is a side-effecting call
        // (the spawn) must decline the whole fragment — otherwise the
        // spawn is silently dropped while the SetStage still applies (#1907).
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             ObjectReference k = akActor.PlaceAtMe(SomeForm)\n\
             Self.SetStage(20)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn side_effect_free_binding_is_recorded_not_declined() {
        // A pure-value non-quest local (an ident copy) has no side-effect
        // to drop, so it is recorded and lowering continues (#1907).
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_6()\n\
             ObjectReference k = akActor\n\
             Self.SetStage(20)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 20
            }])
        );
    }

    #[test]
    fn lowers_add_item_on_bare_properties() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_7()\n\
             SomeContainer.AddItem(SomeItem, 5)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::AddItem {
                container: ObjectRef::Property("SomeContainer".into()),
                item: ObjectRef::Property("SomeItem".into()),
                count: 5,
            }])
        );
    }

    #[test]
    fn lowers_add_item_with_literal_silent_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             SomeContainer.AddItem(SomeItem, 5, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::AddItem {
                container: ObjectRef::Property("SomeContainer".into()),
                item: ObjectRef::Property("SomeItem".into()),
                count: 5,
            }])
        );
    }

    #[test]
    fn add_item_declines_with_non_literal_silent_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_9()\n\
             Bool bQuiet = true\n\
             SomeContainer.AddItem(SomeItem, 5, bQuiet)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn add_item_declines_on_local_receiver() {
        // A local copy of a property is not tracked back to its name in
        // this increment — the receiver must be a bare property.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_10()\n\
             ObjectReference k = SomeContainer\n\
             k.AddItem(SomeItem, 5)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn lowers_move_to_two_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_11()\n\
             SomeRef.MoveTo(SomeMarker)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::MoveTo {
                moved: ObjectRef::Property("SomeRef".into()),
                destination: ObjectRef::Property("SomeMarker".into()),
            }])
        );
    }

    #[test]
    fn lowers_disable_with_optional_fade_out() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             SomeMarker.Disable(false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::Disable {
                object: ObjectRef::Property("SomeMarker".into()),
                fade_out: false,
            }])
        );
    }

    #[test]
    fn move_to_declines_with_offset_args() {
        // The conservative 2-arg shape only — offsets/match-rotation
        // decline rather than silently misplacing the object.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             SomeRef.MoveTo(SomeMarker, 0.0, 0.0, 10.0)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn lowers_scene_start_and_stop_requests() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_13()\n\
             IntroScene.Start()\n\
             OldScene.Stop()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::StartScene {
                    scene: ObjectRef::Property("IntroScene".into()),
                },
                Effect::StopScene {
                    scene: ObjectRef::Property("OldScene".into()),
                },
            ])
        );
    }

    /// Regression for #2538 / SCR-D5-NEW10-01. `Quest Property
    /// MQ101 Auto` called with `.Start()` shares the identical bare-
    /// identifier `.Start()` AST shape `lowers_scene_start_and_stop_requests`
    /// above pins for a genuine scene property — nothing in the AST
    /// distinguishes them. `lower_fragment` (no property-type context —
    /// the state every call site had pre-#2538) cannot tell them apart
    /// and reproduces the original bug's exact symptom: this assertion
    /// pins that documented limitation of the context-free path, not a
    /// desired outcome — it's why `lower_fragment_with_quest_properties`
    /// (below) exists. `populate_quest_fragments_from_script` (the real
    /// production caller) always supplies context, so this path is
    /// exercised by tests only.
    #[test]
    fn quest_start_on_a_direct_property_declines_rather_than_mislowering_to_scene() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Quest Property MQ101 Auto\n\
             Function Fragment_99()\n\
             MQ101.Start()\n EndFunction\n",
        );
        // Without quest-property context, the ambiguity genuinely can't
        // be resolved from the AST alone — this is the same "silently
        // becomes StartScene" outcome #2538 reported, still reachable
        // whenever no context is available. Pinned here so a future
        // change to the context-free fallback is a deliberate choice,
        // not an accident.
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::StartScene {
                scene: ObjectRef::Property("MQ101".into()),
            }])
        );

        // With quest-property context (the real `populate_quest_fragments_
        // from_script` call path), the ambiguity is resolved *positively*:
        // the receiver is a known `Quest` property, so it lowers to
        // `StartQuest` — not to `StartScene` (the #2538 bug) and not to a
        // whole-fragment decline (#2538's original fix, which discarded
        // every sibling effect to avoid guessing).
        let quest_properties: HashSet<String> = ["mq101".to_string()].into_iter().collect();
        assert_eq!(
            lower_fragment_with_quest_properties(&body, &quest_properties),
            Some(vec![Effect::StartQuest {
                quest: QuestRef::Property("MQ101".into()),
            }]),
            "a known Quest Property called with .Start() must lower to StartQuest"
        );

        // Same statement as the decompiler actually emits it — an
        // auto-property read arrives as its backing variable `::MQ101_var`,
        // which the `.psc` Ident regex cannot express, so this shape has to
        // be built by hand (#2657). It must resolve identically.
        let decompiled = Expr::Call {
            callee: Box::new(sp(Expr::MemberAccess {
                object: Box::new(sp(Expr::Ident(Identifier("::MQ101_var".into())))),
                member: sp(Identifier("Start".into())),
            })),
            args: vec![],
        };
        assert_eq!(
            classify_effect(
                &decompiled,
                &Scope {
                    known_quest_properties: quest_properties.clone(),
                    ..Scope::default()
                }
            ),
            Some(Effect::StartQuest {
                quest: QuestRef::Property("::MQ101_var".into()),
            }),
            "the backing-variable form the decompiler emits must resolve too"
        );
    }

    /// SCR-D5-NEW11-02 (#2653) — `Reset` is declared on `Quest`,
    /// `ObjectReference` and `Cell`; `SetActive` on `Quest` and `Weather`.
    /// The zero-arg AST shape is identical, so a bare non-Quest property
    /// receiver must decline rather than be claimed as a quest effect
    /// (which would also claim every sibling effect in the fragment).
    #[test]
    fn reset_and_set_active_decline_on_a_non_quest_property_receiver() {
        let reset = first_fn_body(
            "ScriptName QF extends Quest\n\
             ObjectReference Property MyContainer Auto\n\
             Function Fragment_99()\n\
             MyContainer.Reset()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&reset),
            None,
            "ObjectReference.Reset() must not be claimed as ResetQuest"
        );

        let set_active = first_fn_body(
            "ScriptName QF extends Quest\n\
             Weather Property SomeWeather Auto\n\
             Function Fragment_99()\n\
             SomeWeather.SetActive(false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&set_active),
            None,
            "Weather.SetActive() must not be claimed as SetQuestActive"
        );
    }

    /// The narrowing must not cost the genuine cases: a real `Quest`
    /// property, `Self`, and `GetOwningQuest()` all still lower.
    #[test]
    fn reset_and_set_active_still_lower_on_an_unambiguous_quest_receiver() {
        let quest_properties: HashSet<String> = ["mq101".to_string()].into_iter().collect();

        let reset = first_fn_body(
            "ScriptName QF extends Quest\n\
             Quest Property MQ101 Auto\n\
             Function Fragment_99()\n\
             MQ101.Reset()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment_with_quest_properties(&reset, &quest_properties),
            Some(vec![Effect::ResetQuest {
                quest: QuestRef::Property("MQ101".into()),
            }])
        );

        let self_active = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_99()\n\
             Self.SetActive(true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&self_active),
            Some(vec![Effect::SetQuestActive {
                quest: QuestRef::SelfRef,
                active: true,
            }])
        );
    }

    /// Companion: a genuinely scene-typed property with the same context
    /// present (but not naming the scene property) must still lower
    /// correctly — the fix must not make `prim_start_scene` overly broad
    /// in its refusal.
    #[test]
    fn scene_start_still_lowers_when_quest_property_context_is_present_but_unrelated() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Quest Property MQ101 Auto\n\
             Function Fragment_99()\n\
             IntroScene.Start()\n EndFunction\n",
        );
        let quest_properties: HashSet<String> = ["mq101".to_string()].into_iter().collect();
        assert_eq!(
            lower_fragment_with_quest_properties(&body, &quest_properties),
            Some(vec![Effect::StartScene {
                scene: ObjectRef::Property("IntroScene".into()),
            }])
        );
    }

    #[test]
    fn lowers_activate_and_set_open_gate_sequence() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_14()\n\
             KeepGate.Activate(Game.GetPlayer())\n\
             KeepGate.SetOpen()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::Activate {
                    target: ObjectRef::Property("KeepGate".into()),
                    activator: None,
                },
                Effect::SetOpen {
                    target: ObjectRef::Property("KeepGate".into()),
                    open: true,
                },
            ])
        );
    }

    #[test]
    fn activate_default_processing_only_declines() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_15()\n\
             KeepGate.Activate(Game.GetPlayer(), true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn lowers_real_mq101_alias_activate_and_cast_set_open_shape() {
        // Parse the two statements separately: the source parser deliberately
        // discards newlines, so two adjacent chained calls are ambiguous in
        // handwritten PSC. The PEX decompiler supplies two distinct AST
        // statements, which is the production shape this combined body models.
        let mut body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_76()\n\
             Alias_KeepLever1.GetRef().Activate(Alias_Soldier.GetRef(), false)\n EndFunction\n",
        );
        body.extend(first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_76()\n\
             (KeepGate1 as default2stateactivator).SetOpen(true)\n EndFunction\n",
        ));
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::Activate {
                    target: ObjectRef::Property("Alias_KeepLever1".into()),
                    activator: Some(ObjectRef::Property("Alias_Soldier".into())),
                },
                Effect::SetOpen {
                    target: ObjectRef::Property("KeepGate1".into()),
                    open: true,
                },
            ])
        );
    }

    #[test]
    fn lowers_exact_mq101_player_control_selection() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_191()\n\
             Game.DisablePlayerControls(false, true, true, false, false, false, true, true, 0)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetPlayerControls {
                enabled: false,
                selection: PlayerControlSelection {
                    movement: false,
                    fighting: true,
                    camera_switching: true,
                    looking: false,
                    sneaking: false,
                    menu: false,
                    activation: true,
                    journal_tabs: true,
                    pov_type: 0,
                },
            }])
        );
    }

    #[test]
    fn lowers_player_release_and_cinematic_state_calls() {
        let mut body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_316()\n\
             Game.GetPlayer().SetRestrained(false)\n EndFunction\n",
        );
        body.extend(first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_316()\n\
             Game.SetPlayerAIDriven(false)\n EndFunction\n",
        ));
        body.extend(first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_316()\n\
             Game.SetHudCartMode(false)\n EndFunction\n",
        ));
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetPlayerRestrained { restrained: false },
                Effect::SetPlayerAiDriven { ai_driven: false },
                Effect::SetHudCartMode { cart_mode: false },
            ])
        );
    }

    #[test]
    fn lowers_alias_actor_evaluate_package() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_292()\n\
             Alias_Hadvar.GetActorRef().EvaluatePackage()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::EvaluatePackage {
                actor: ObjectRef::Property("Alias_Hadvar".into()),
            }])
        );
    }

    #[test]
    fn lowers_mq101_player_idle_and_animation_event_helpers() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_295()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             Game.GetPlayer().PlayIdle(IdleExecutionerAlduinReactionDeath)\n\
             kmyQuest.PlayerImodAnimation()\n\
             kmyQuest.PlayerFurnitureAnimation()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::PlayIdle {
                    actor: ActorRef::Player,
                    idle: ObjectRef::Property("IdleExecutionerAlduinReactionDeath".into()),
                },
                Effect::RegisterPlayerAnimationEvent {
                    event: CinematicAnimationEvent::PlayImod,
                },
                Effect::RegisterPlayerAnimationEvent {
                    event: CinematicAnimationEvent::IdleFurnitureExit,
                },
            ])
        );
    }

    #[test]
    fn lowers_mq101_cart_exit_motion_and_sitting_rotation() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_177()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.ExitCart(Alias_Prisoner, 3)\n\
             Alias_Cart.GetRef().SetMotionType(Alias_Cart.GetRef().Motion_Keyframed, true)\n\
             Game.SetSittingRotation(-55.0)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::ExitCart {
                    actor: ObjectRef::Property("Alias_Prisoner".into()),
                    seat: 3,
                },
                Effect::SetMotionType {
                    target: ObjectRef::Property("Alias_Cart".into()),
                    motion_type: MotionType::Keyframed,
                    allow_activate: true,
                },
                Effect::SetSittingRotation { degrees: -55.0 },
            ])
        );
    }

    /// #2286 — vanilla PEX commonly folds AutoReadOnly Motion_* constants
    /// into raw integer literals. Pin the canonical Havok values through the
    /// complete parser-to-lowering path rather than only testing named fields.
    #[test]
    fn lowers_literal_havok_motion_types() {
        for (raw, expected) in [
            (4, MotionType::Dynamic),
            (5, MotionType::Dynamic),
            (6, MotionType::Keyframed),
            (7, MotionType::Static),
            (9, MotionType::CharacterKinematic),
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_177()\n\
                 Cart.SetMotionType({raw}, true)\n\
                 EndFunction\n"
            ));
            assert_eq!(
                lower_fragment(&body),
                Some(vec![Effect::SetMotionType {
                    target: ObjectRef::Property("Cart".into()),
                    motion_type: expected,
                    allow_activate: true,
                }]),
                "literal hkpMotion value {raw} must use the canonical mapping",
            );
        }
    }

    #[test]
    fn tracks_actor_and_vehicle_locals_across_a_latent_wait() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_175()\n\
             Actor rider = Alias_Rider.GetActorRef()\n\
             ObjectReference cart = Alias_Cart.GetRef()\n\
             rider.SetVehicle(cart)\n\
             Utility.Wait(0.25)\n\
             rider.SetVehicle(None)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetVehicle {
                    actor: ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                    vehicle: Some(ObjectRef::Property("Alias_Cart".into())),
                },
                Effect::Wait { seconds: 0.25 },
                Effect::SetVehicle {
                    actor: ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                    vehicle: None,
                },
            ])
        );
    }

    #[test]
    fn lowers_mq101_cart_load_gate_tether_and_equipment_sequence() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_175()\n\
             Actor player = Game.GetPlayer()\n\
             Actor rider = Alias_Rider.GetActorRef()\n\
             ObjectReference cart = Alias_Cart.GetRef()\n\
             While !player.Is3DLoaded() || !rider.Is3DLoaded()\n\
                 Utility.Wait(0.2)\n\
             EndWhile\n\
             cart.TetherToHorse(Alias_Horse.GetActorRef() as ObjectReference)\n\
             rider.EquipItem(ArmorGag as Form, false, true)\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::WaitForActors3DLoaded {
                    actors: vec![
                        ActorRef::Player,
                        ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                    ],
                    poll_seconds: 0.2,
                },
                Effect::TetherToHorse {
                    cart: ObjectRef::Property("Alias_Cart".into()),
                    horse: ObjectRef::Property("Alias_Horse".into()),
                },
                Effect::EquipItem {
                    actor: ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                    item: ObjectRef::Property("ArmorGag".into()),
                    silent: true,
                },
            ])
        );
    }

    #[test]
    fn declines_unmodeled_loop_and_prevent_unequip() {
        let arbitrary_loop = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             While Self.GetStage() < 10\n Utility.Wait(0.2)\n EndWhile\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&arbitrary_loop), None);

        let locked_item = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_2()\n\
             Alias_Rider.GetActorRef().EquipItem(ArmorGag, true, false)\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&locked_item), None);
    }

    #[test]
    fn lowers_get_stage_done_conditional() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             If Self.GetStageDone(2) == false && Self.GetStageDone(6) == 0\n\
               Self.SetStage(35)\n\
             Else\n\
               Self.SetStage(7)\n\
             EndIf\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::Conditional {
                guards: vec![
                    StageDoneGuard {
                        quest: QuestRef::SelfRef,
                        stage: 2,
                        done: false,
                    },
                    StageDoneGuard {
                        quest: QuestRef::SelfRef,
                        stage: 6,
                        done: false,
                    },
                ],
                then_effects: vec![Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 35,
                }],
                else_effects: vec![Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 7,
                }],
            }]),
        );
    }

    #[test]
    fn declines_unmodeled_conditional_guard() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             If Self.GetStage() >= 5\n Self.SetStage(10)\n EndIf\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn empty_fragment_is_understood_as_noop() {
        let body =
            first_fn_body("ScriptName QF extends Quest\n Function Fragment_6()\n EndFunction\n");
        assert_eq!(lower_fragment(&body), Some(vec![]));
    }

    // ── Decline-path coverage for SCR-D5-NEW5-02 / #2289 ────────────────
    // One `?`/arg-count/arg-type guard test per primitive that previously
    // shipped with a positive-path test only.

    #[test]
    fn set_open_declines_with_extra_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_20()\n\
             KeepGate.SetOpen(true, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn set_player_restrained_declines_on_non_player_receiver_and_extra_arg() {
        let non_player = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_21()\n\
             SomeActor.SetRestrained(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&non_player), None);

        let extra_arg = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_45()\n\
             Game.GetPlayer().SetRestrained(true, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&extra_arg), None);
    }

    #[test]
    fn player_controls_toggles_decline_with_too_many_args() {
        // Shared `prim_player_controls`'s `args.len() > 9` guard — covers
        // both DisablePlayerControls and EnablePlayerControls.
        let disable = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_22()\n\
             Game.DisablePlayerControls(false, true, true, false, false, false, true, true, 0, false)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&disable), None);

        let enable = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_23()\n\
             Game.EnablePlayerControls(false, true, true, false, false, false, true, true, 0, false)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&enable), None);
    }

    #[test]
    fn set_player_ai_driven_declines_with_extra_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_24()\n\
             Game.SetPlayerAIDriven(true, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn set_hud_cart_mode_declines_with_extra_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_25()\n\
             Game.SetHudCartMode(true, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn play_idle_declines_on_wrong_arg_count() {
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_26()\n\
             Game.GetPlayer().PlayIdle()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let two_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_27()\n\
             Game.GetPlayer().PlayIdle(SomeIdle, SomeIdle)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&two_args), None);
    }

    #[test]
    fn set_vehicle_declines_on_wrong_arg_count() {
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_28()\n\
             Alias_Rider.GetActorRef().SetVehicle()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let two_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_29()\n\
             Alias_Rider.GetActorRef().SetVehicle(Alias_Cart.GetRef(), Alias_Cart.GetRef())\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&two_args), None);
    }

    #[test]
    fn tether_to_horse_declines_on_wrong_arg_count() {
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_30()\n\
             Alias_Cart.GetRef().TetherToHorse()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let two_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_47()\n\
             Alias_Cart.GetRef().TetherToHorse(Alias_Horse.GetActorRef() as ObjectReference, Alias_Horse.GetActorRef() as ObjectReference)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&two_args), None);
    }

    #[test]
    fn set_motion_type_declines_on_arg_count_and_unrecognized_member() {
        // #2289 flags this as the most structurally intricate untested case.
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_31()\n\
             Cart.SetMotionType()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let three_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_32()\n\
             Cart.SetMotionType(6, true, false)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&three_args), None);

        // An unrecognized member access on the (correctly matched) receiver
        // falls through `motion_type_arg`'s match arm to `None`.
        let unrecognized_member = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_33()\n\
             Cart.SetMotionType(Cart.Motion_NotARealValue, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&unrecognized_member), None);
    }

    #[test]
    fn set_sitting_rotation_declines_on_wrong_arg_count() {
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_34()\n\
             Game.SetSittingRotation()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let two_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_48()\n\
             Game.SetSittingRotation(-55.0, 1.0)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&two_args), None);
    }

    #[test]
    fn exit_cart_declines_on_wrong_arg_count_and_seat_out_of_range() {
        let wrong_arg_count = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_35()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.ExitCart(Alias_Prisoner)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&wrong_arg_count), None);

        let seat_too_low = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_36()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.ExitCart(Alias_Prisoner, 0)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&seat_too_low), None);

        let seat_too_high = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_37()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.ExitCart(Alias_Prisoner, 6)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&seat_too_high), None);
    }

    #[test]
    fn player_animation_events_decline_with_args() {
        let imod = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_38()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.PlayerImodAnimation(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&imod), None);

        let furniture = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_39()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.PlayerFurnitureAnimation(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&furniture), None);
    }

    #[test]
    fn evaluate_package_declines_with_args() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_40()\n\
             Alias_Hadvar.GetActorRef().EvaluatePackage(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn wait_declines_on_wrong_arg_count_and_negative_seconds() {
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_41()\n\
             Utility.Wait()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let negative = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_42()\n\
             Utility.Wait(-1.0)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&negative), None);
    }

    #[test]
    fn start_and_stop_scene_decline_with_args() {
        let start = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_43()\n\
             IntroScene.Start(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&start), None);

        let stop = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_44()\n\
             OldScene.Stop(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&stop), None);
    }

    // ── #2540 / SCR-D5-NEW10-02 — the objective-index `u16`→`i32` widen ──

    /// A genuinely negative index (i32 on FO3/FNV per `QuestObjective::
    /// index`'s doc) arrives from the PEX decompiler as a direct signed
    /// `IntLit`, never as a `.psc`-source `UnaryOp::Neg` — the `.psc` text
    /// parser lexes `-N` as `Minus` + `IntLit(N)`, which `int_arg` does not
    /// unwrap. Matching #2657's `::X_var` precedent, this decompiled shape
    /// has to be built by hand rather than round-tripped through the `.psc`
    /// parser to exercise the real input this primitive actually receives.
    #[test]
    fn objective_primitives_lower_a_negative_index() {
        for (method, expected) in [
            (
                "SetObjectiveDisplayed",
                Effect::SetObjectiveDisplayed {
                    quest: QuestRef::SelfRef,
                    objective: -5,
                    displayed: true,
                },
            ),
            (
                "SetObjectiveCompleted",
                Effect::SetObjectiveCompleted {
                    quest: QuestRef::SelfRef,
                    objective: -5,
                    completed: true,
                },
            ),
            (
                "SetObjectiveFailed",
                Effect::SetObjectiveFailed {
                    quest: QuestRef::SelfRef,
                    objective: -5,
                    failed: true,
                },
            ),
        ] {
            let call = Expr::Call {
                callee: Box::new(sp(Expr::MemberAccess {
                    object: Box::new(sp(Expr::Ident(Identifier("Self".into())))),
                    member: sp(Identifier(method.into())),
                })),
                args: vec![CallArg {
                    name: None,
                    value: sp(Expr::IntLit(-5)),
                }],
            };
            assert_eq!(
                classify_effect(&call, &Scope::default()),
                Some(expected),
                "{method} must lower a negative index correctly"
            );
        }
    }

    /// An index literal outside `i32`'s range must still decline via
    /// `i32::try_from(..).ok()?`, not silently truncate.
    #[test]
    fn objective_primitives_decline_on_index_overflow() {
        for method in [
            "SetObjectiveDisplayed",
            "SetObjectiveCompleted",
            "SetObjectiveFailed",
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_46()\n\
                 Self.{method}(5000000000)\n EndFunction\n"
            ));
            assert_eq!(
                lower_fragment(&body),
                None,
                "{method} must decline an i32-overflowing index"
            );
        }
    }
}
