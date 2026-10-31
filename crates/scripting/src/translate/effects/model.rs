//! The effect IR (#5801, split from `effects.rs`): [`Effect`] and the
//! vocabulary it is built from — [`FragmentGuard`] / [`GuardComparison`],
//! [`ValueExpr`] / [`ArithOp`], the [`ActorRef`] / [`ActorBaseRef`]
//! receivers and the argument types an effect carries ([`ItemCount`],
//! [`ActorValueArg`], [`ActorValueReading`], [`FragmentProviderCall`]).
//!
//! Every serde-derived type here is saved — a queued fragment tail
//! serializes its `Effect`s — so a shape change is a `FORMAT_MAJOR`
//! question (`byroredux/src/save_io/serde_default_guard_tests.rs`).

use super::*;

/// A comparison operator of a fragment guard, applied as
/// `reading <op> value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum GuardComparison {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl GuardComparison {
    pub(super) fn from_op(op: byroredux_papyrus::ast::BinaryOp) -> Option<Self> {
        use byroredux_papyrus::ast::BinaryOp as B;
        Some(match op {
            B::Eq => Self::Eq,
            B::Ne => Self::Ne,
            B::Lt => Self::Lt,
            B::Le => Self::Le,
            B::Gt => Self::Gt,
            B::Ge => Self::Ge,
            _ => return None,
        })
    }

    /// The operator with its operands swapped (`5 < x` is `x > 5`).
    pub(super) fn swapped(self) -> Self {
        match self {
            Self::Eq | Self::Ne => self,
            Self::Lt => Self::Gt,
            Self::Le => Self::Ge,
            Self::Gt => Self::Lt,
            Self::Ge => Self::Le,
        }
    }

    /// The logical negation (`!(x < 5)` is `x >= 5`).
    pub(super) fn negated(self) -> Self {
        match self {
            Self::Eq => Self::Ne,
            Self::Ne => Self::Eq,
            Self::Lt => Self::Ge,
            Self::Le => Self::Gt,
            Self::Gt => Self::Le,
            Self::Ge => Self::Lt,
        }
    }

    pub fn apply(self, reading: f32, value: f32) -> bool {
        match self {
            Self::Eq => reading == value,
            Self::Ne => reading != value,
            Self::Lt => reading < value,
            Self::Le => reading <= value,
            Self::Gt => reading > value,
            Self::Ge => reading >= value,
        }
    }
}

/// One conservatively lowered predicate guarding a fragment branch (M47.2
/// widened it from `GetStageDone` alone). Every guard reads quest, objective,
/// global or quest-variable state the runtime holds; anything else declines.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum FragmentGuard {
    /// `<quest>.GetStageDone(stage)` (FO4: `IsStageDone`) is `done`.
    StageDone {
        quest: QuestRef,
        stage: u16,
        done: bool,
    },
    /// `<quest>.IsRunning()` is `running`.
    QuestRunning { quest: QuestRef, running: bool },
    /// `<quest>.GetStage() <op> value`.
    Stage {
        quest: QuestRef,
        comparison: GuardComparison,
        value: f32,
    },
    /// `<quest>.IsObjectiveDisplayed(objective)` is `expected`.
    ObjectiveDisplayed {
        quest: QuestRef,
        objective: i32,
        expected: bool,
    },
    /// `<quest>.IsObjectiveCompleted(objective)` is `expected`.
    ObjectiveCompleted {
        quest: QuestRef,
        objective: i32,
        expected: bool,
    },
    /// `<global>.GetValue() / .Value <op> value` on the stored float. A
    /// truncated read (`GetValueInt()`, `as Int`) lowers to exact bounds on it
    /// (#6215): `GetValueInt() == 12` is `>= 12` and `< 13`.
    Global {
        global: ObjectRef,
        comparison: GuardComparison,
        value: f32,
    },
    /// `<quest>.<Property> <op> value` — a quest script variable
    /// ([`Effect::SetQuestVariable`]'s read side).
    QuestVariable {
        quest: QuestRef,
        variable: String,
        comparison: GuardComparison,
        value: f32,
    },
    /// M47.2 — a disjunction: holds when every guard of at least one
    /// alternative holds (`A || (B && C)`).
    Any(Vec<Vec<FragmentGuard>>),
    /// M47.2 — two run-time numbers compared (`GetLevel() < Global.GetValue()`,
    /// `Utility.RandomInt(1, 3) == 1`) when no specific guard fits.
    Compare {
        left: ValueExpr,
        comparison: GuardComparison,
        right: ValueExpr,
    },
    /// `<actor>.IsDead()` is `dead`.
    ActorDead { actor: ActorRef, dead: bool },
    /// `<actor>.HasPerk(perk)` is `expected`.
    HasPerk {
        actor: ActorRef,
        perk: ObjectRef,
        expected: bool,
    },
    /// `<container>.GetItemCount(item) <op> value`.
    ItemCount {
        container: ActorRef,
        item: ObjectRef,
        comparison: GuardComparison,
        value: f32,
    },
    /// `<quest>.IsCompleted()` is `completed`.
    QuestCompleted { quest: QuestRef, completed: bool },
    /// `<scene>.IsPlaying()` is `playing`.
    ScenePlaying { scene: ObjectRef, playing: bool },
    /// `<marker>.IsMapMarkerVisible()` is `expected`.
    MapMarkerVisible { marker: ObjectRef, expected: bool },
    /// `<marker>.CanFastTravelToMarker()` is `expected`.
    MapMarkerFastTravel { marker: ObjectRef, expected: bool },
    /// `<location>.IsCleared()` is `expected`.
    LocationCleared { location: ObjectRef, expected: bool },
    /// `<actor>.IsBleedingOut()` is `expected`.
    ActorBleedingOut { actor: ActorRef, expected: bool },
    /// `<actor>.IsInCombat()` is `expected` ([`crate::is_in_combat`]).
    ActorInCombat { actor: ActorRef, expected: bool },
    /// `<actor>.IsPlayerTeammate()` / `IsDoingFavor()` is `expected`
    /// ([`crate::FollowerState`]).
    ActorFollower {
        actor: ActorRef,
        flag: crate::FollowerFlag,
        expected: bool,
    },
    /// `<reference>.IsEnabled()` is `expected` (`IsDisabled()` inverted).
    ReferenceEnabled { object: ObjectRef, expected: bool },
    /// `<actor>.IsInFaction(faction)` is `expected`.
    InFaction {
        actor: ActorRef,
        faction: ObjectRef,
        expected: bool,
    },
    /// `<reference>.Is3DLoaded()` is `expected`.
    Loaded3D { object: ObjectRef, expected: bool },
    /// `<reference>.HasLocRefType(refType)` is `expected`: the placement's
    /// XLRT tags ([`crate::scene::SceneAliasCandidate`]).
    HasLocRefType {
        actor: ActorRef,
        ref_type: ObjectRef,
        expected: bool,
    },
    /// `<actor>.GetRelationshipRank(<other>)` compared with `value`.
    RelationshipRank {
        actor: ActorRef,
        other: ActorRef,
        comparison: GuardComparison,
        value: f32,
    },
    /// `<from>.GetDistance(<to>)` compared with `value`.
    Distance {
        from: ActorRef,
        to: ActorRef,
        comparison: GuardComparison,
        value: f32,
    },
    /// `<actor>.IsEssential()` is `expected`.
    ActorEssential { actor: ActorRef, expected: bool },
    /// `<actor>.GetCurrentLocation().HasKeyword(keyword)` is `expected` — the
    /// actor's current location itself (the M47.2 location runtime).
    LocationKeyword {
        actor: ActorRef,
        keyword: ObjectRef,
        expected: bool,
    },
    /// M47.2 — `<reference>.HasKeyword(keyword)` is `expected`
    /// ([`crate::keywords`]).
    HasKeyword {
        object: ActorRef,
        keyword: ObjectRef,
        expected: bool,
    },
    /// M47.2 — `<reference> == <reference>` (or `== None`, `right: None`) is
    /// `expected`: the two name the same reference, by FormID, so a direct
    /// reference compares whether or not it is loaded and an unfilled alias
    /// is `None`.
    SameReference {
        left: ActorRef,
        right: Option<ActorRef>,
        expected: bool,
    },
}

/// M47.2 — a number a fragment reads at run time, for
/// [`FragmentGuard::Compare`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ValueExpr {
    Literal(f32),
    /// `<quest>.GetStage()`.
    Stage(QuestRef),
    /// `<global>.GetValue()` / `.Value`.
    Global(ObjectRef),
    /// `<quest>.<Variable>`, a quest script variable.
    QuestVariable(QuestRef, String),
    /// `<container>.GetItemCount(item)`.
    ItemCount(ActorRef, ObjectRef),
    /// `<from>.GetDistance(<to>)`.
    Distance(ActorRef, ActorRef),
    /// `<actor>.GetRelationshipRank(<other>)`.
    RelationshipRank(ActorRef, ActorRef),
    /// `<actor>.GetLevel()`.
    Level(ActorRef),
    /// `Utility.RandomInt(min, max)`, both inclusive.
    RandomInt(i32, i32),
    /// A fragment local assigned a number ([`Effect::SetLocal`]); 0 until
    /// assigned, Papyrus's default.
    Local(String),
    /// Arithmetic over two values.
    Arith(Box<ValueExpr>, ArithOp, Box<ValueExpr>),
    /// M47.2 — `<actor>.GetValue(<actor value>)`, the current value.
    ActorValue(ActorRef, ObjectRef),
    /// M47.2 — `<actor>.GetAV("Name")` / `GetValue(Game.GetHealthAV())`, an
    /// actor value by name ([`crate::ActorValueNames`]).
    ActorValueNamed(ActorRef, String),
    /// M47.2 — `<actor>.GetBaseValue(av)` / `GetValuePercentage(av)` and
    /// Skyrim's `GetBaseActorValue("Name")` / `GetActorValuePercentage("Name")`.
    ActorValueReading(ActorValueReading, ActorRef, ActorValueArg),
    /// M47.2 — `Game.QueryStat("Houses Owned")`, a script-kept misc stat
    /// ([`crate::misc_stats::SCRIPT_STATS`]).
    MiscStat(String),
    /// M47.2 — `<location>.GetKeywordData(keyword)`
    /// ([`crate::LocationKeywordData`]).
    LocationKeywordData(ObjectRef, ObjectRef),
    /// #5973 — the magnitude of a value. Skyrim's `DamageActorValue` /
    /// `RestoreActorValue` discard the sign of the amount (CK: "Negative
    /// numbers will be converted to positive"), so a negative or run-time
    /// amount lowers through this. Appended last: the saved `Effect` enum is
    /// tagged by variant name, so older documents still decode.
    Abs(Box<ValueExpr>),
}

/// The arithmetic a [`ValueExpr::Arith`] applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl ArithOp {
    pub fn apply(self, left: f32, right: f32) -> Option<f32> {
        Some(match self {
            Self::Add => left + right,
            Self::Sub => left - right,
            Self::Mul => left * right,
            Self::Div if right == 0.0 => return None,
            Self::Div => left / right,
        })
    }
}

/// Typed provider invocation deferred until fragment ECS guards are released.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct FragmentProviderCall {
    pub route: String,
    pub arguments: Vec<byroredux_sdk::script_function::ScriptValue>,
    pub principal: Option<byroredux_sdk::identity::PrincipalId>,
}

/// Attribute every provider barrier in a lowered fragment, including calls in
/// nested conditional branches, to the archive package that supplied its PEX.
pub(crate) fn attribute_provider_calls(
    effects: &mut [Effect],
    principal: &byroredux_sdk::identity::PrincipalId,
) {
    for effect in effects {
        match effect {
            Effect::ProviderCall(call) => call.principal = Some(principal.clone()),
            Effect::Conditional {
                then_effects,
                else_effects,
                ..
            } => {
                attribute_provider_calls(then_effects, principal);
                attribute_provider_calls(else_effects, principal);
            }
            _ => {}
        }
    }
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
    /// `If <guard && ...> ... [Else ...] EndIf`. Only conjunctions of
    /// [`FragmentGuard`]s lower; elseif/disjunction and latent effects inside
    /// either branch remain decline-on-unknown.
    Conditional {
        guards: Vec<FragmentGuard>,
        then_effects: Vec<Effect>,
        else_effects: Vec<Effect>,
    },
    /// `<global>.SetValue(value)` — resolves the VMAD-bound GLOB FormID and
    /// updates the canonical runtime [`crate::Globals`] table.
    SetGlobalValue {
        global: ObjectRef,
        /// M47.2 — a run-time number since FORMAT 60: a literal, a read
        /// global plus an offset (`G.Mod(x)`, `G.SetValue(S.GetValue() + 1)`),
        /// or any other [`ValueExpr`].
        value: ValueExpr,
    },
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
    /// optional `abForce` 3rd arg doesn't affect the stored state, but is
    /// accepted only as a literal (declining non-literal 3rd expressions,
    /// #5652).
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
    /// `<container>.AddItem(<item>, <count = 1>[, abSilent = false])`. The
    /// optional 3rd (`abSilent`) argument is accepted only as a literal and
    /// carried as `silent`: unless it is set, an add to the player raises the
    /// pickup notice (#5913). A non-literal 3rd arg, or a 4th, declines the
    /// whole fragment (the `bool_arg` discipline `SetObjectiveDisplayed`
    /// already uses).
    /// `item` resolves only to a FormID at dispatch, never to an entity —
    /// it names a *base record* (the item type), not a placed reference.
    /// The container is an [`ActorRef`] so `Game.GetPlayer().AddItem(..)` —
    /// the corpus's commonest reward shape — lowers (M47.2).
    AddItem {
        container: ActorRef,
        item: ObjectRef,
        count: ItemCount,
        silent: bool,
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
    /// `<moved>.MoveTo(<destination>[, x, y, z, matchRotation])` (CK *MoveTo -
    /// ObjectReference*): the destination's position plus the offset (Gamebryo
    /// Z-up world axes), with the destination's rotation when `match_rotation`
    /// (the default). Starfield's `abRotateOffset` is not modelled and declines
    /// (see [`prim_move_to`]).
    MoveTo {
        moved: ObjectRef,
        destination: ObjectRef,
        offset: [f32; 3],
        match_rotation: bool,
    },
    /// `Game.GetPlayer().MoveTo(<destination>)` (M47.2) — the player's
    /// teleport, with [`Self::MoveTo`]'s default-offsets rule. The player
    /// may land in another cell, so the runtime records the destination
    /// reference for the engine ([`crate::fragment::PlayerMoveRequest`]),
    /// which moves the player in place or transitions to the reference's
    /// cell.
    MovePlayerTo { destination: ObjectRef },
    /// `<object>.Disable([fadeOut])` — records the placed reference as
    /// disabled even when its cell/entity is not currently loaded.
    Disable { object: ObjectRef, fade_out: bool },
    /// `<object>.Enable([fadeIn])` — the symmetric counterpart to
    /// [`Self::Disable`]; clears the same [`crate::fragment::
    /// ReferenceEnableState`] entry so a reference a script disabled can be
    /// re-enabled by script. #3489 — shipped alongside `Disable` (`5f38402e`)
    /// without this half, so `ReferenceEnableState::set_enabled`'s
    /// `enabled == true` branch had no production caller and every
    /// `Enable()` call (3,005 in the real corpus, more common than
    /// `Disable()`'s 2,587) declined the whole fragment.
    Enable { object: ObjectRef, fade_in: bool },
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
        /// `abDefaultProcessingOnly`: the engine's own activation runs, no script hears
        /// `OnActivate` and the block flag is ignored.
        native_only: bool,
    },
    /// `<target>.SetOpen(open)` — synchronizes the canonical two-state
    /// activator and its `::isOpen_var` CTDA-visible VM variable.
    SetOpen { target: ObjectRef, open: bool },
    /// `<target>.Lock(abLock)` — sets or clears the authored lock on a door
    /// or container.
    ///
    /// Papyrus defaults `abLock` to `true`, so a bare `Lock()` locks.
    /// `Lock(false)` is the unlock that #3159 was filed for: before this
    /// primitive existed nothing in the engine could remove a `Locked`
    /// component, and a fragment containing the call declined *wholesale* —
    /// taking its sibling `SetStage` / `SetObjectiveCompleted` with it.
    /// The declared second parameter `abAsOwner` (cell public/private
    /// flagging, #5653) is parsed as a literal and not applied.
    SetLocked { target: ObjectRef, locked: bool },
    /// `<target>.SetLockLevel(aiLockLevel)` — sets lock *difficulty*.
    ///
    /// Deliberately a separate effect from [`Self::SetLocked`], not a second
    /// spelling of it: in Papyrus `SetLockLevel` does **not** lock or unlock
    /// anything, it only changes how hard an already-locked object is to
    /// pick. Folding the two together would make a fragment that merely
    /// re-tunes a lock silently lock or unlock a door.
    ///
    /// Applying it to an unlocked object is a no-op, matching the component
    /// model: `Locked`'s absence *is* the unlocked state, so there is no
    /// record to carry a difficulty on.
    SetLockLevel { target: ObjectRef, level: u8 },
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
    /// `Game.SetInChargen(abDisableSaving, abDisableWaiting,
    /// abShowControlsDisabledMessage)` — Skyrim's own declaration (#4322).
    /// The aliases decode saves written under the earlier invented names,
    /// which held the same positional arguments.
    SetInChargen {
        #[cfg_attr(feature = "save", serde(alias = "enabled"))]
        disable_saving: bool,
        #[cfg_attr(feature = "save", serde(alias = "wait_for_race_sex"))]
        disable_waiting: bool,
        #[cfg_attr(feature = "save", serde(alias = "stay_in_first_person"))]
        show_controls_disabled_message: bool,
    },
    /// `Game.ShowRaceMenu()`. No interactive race-menu UI exists yet (a full
    /// slider-based character creator is its own milestone); the runtime
    /// treats the call as an instant auto-accept of the player's current
    /// appearance so the fragment it's authored in — MQ101's execution-block
    /// scene — no longer declines wholesale over an unmodeled statement.
    ShowRaceMenu,
    /// `Game.RequestSave()` / `Game.RequestAutoSave()`. Deliberately does
    /// not write a save file — see `CinematicPresentationState::
    /// request_save`'s doc for why a mid-fragment write is out of scope.
    RequestSave { auto: bool },
    /// `<faction>.SetEnemy(<other_faction>, abSelfIsNeutralToOther,
    /// abOtherIsNeutralToSelf)`. Each flag makes its direction *neutral*
    /// instead of hostile (#4318); the two directions are independent, which
    /// [`crate::FactionRelations`] records as directed reactions (#4414).
    SetEnemy {
        faction: ObjectRef,
        other_faction: ObjectRef,
        self_neutral: bool,
        other_neutral: bool,
    },
    /// M47.2 — `<faction>.SetAlly(<other>, abSelfIsFriendToOther = false,
    /// abOtherIsFriendToSelf = false)`: each direction becomes an ally, or a
    /// friend when its flag is set (CK *SetAlly - Faction*).
    SetAlly {
        faction: ObjectRef,
        other_faction: ObjectRef,
        self_friend: bool,
        other_friend: bool,
    },
    /// `<actor>.StartCombat(<target>)`. See [`crate::AiCombatState`] for
    /// the runtime chase-and-strike behavior this arms.
    StartCombat { actor: ActorRef, target: ActorRef },
    /// M47.2 — `<reference>.<Variable> = literal` on a script attached to a
    /// reference (`::MQ203PlateTrapMarker_var.plateTriggered = False`): the
    /// reference's `ScriptVariables`, which `GetVMScriptVariable` reads.
    SetReferenceVariable {
        object: ObjectRef,
        variable: String,
        value: f32,
    },
    /// M47.2 — `<message>.Show([afArg1 … afArg9])` as a statement: the
    /// message's notification or message box, its `printf` spots filled from
    /// the literal arguments ([`crate::messages`]).
    ShowMessage { message: ObjectRef, args: Vec<f32> },
    /// M47.2 — `<message>.ShowAsHelpMessage(asEvent, …)`: the help text,
    /// shown once (the repeat interval and the ending event are not
    /// modelled).
    ShowHelpMessage { message: ObjectRef },
    /// M47.2 — `Debug.Notification(asText)` with a literal text.
    Notification { text: String },
    /// M47.2 — a fragment the lowering declines, or a reference script's
    /// event, run by the Papyrus VM ([`crate::fragment::vm`]): `script`'s
    /// `function` (or event), installed with the scripts it reaches.
    Interpret {
        script: String,
        function: String,
        /// The event's arguments (`akActionRef`); none for a fragment.
        args: Vec<VmStored>,
        /// Where a run suspended by `Utility.Wait` resumes; `None` to start.
        resume: Option<VmResume>,
    },
    /// M47.2 — `<reference>.DeleteWhenAble()`: a created reference is deleted
    /// once its cell is gone ([`crate::CreatedDeletion`]); an authored one
    /// cannot be deleted.
    DeleteWhenAble { object: ObjectRef },
    /// M47.2 — `<actor>.ShowBarterMenu()`: open the barter menu with the
    /// actor's vendor faction ([`crate::menus`]).
    ShowBarterMenu { vendor: ActorRef },
    /// M47.2 — `<reference>.AddKeyword(kw)` / `RemoveKeyword(kw)` /
    /// `ResetKeyword(kw)` ([`crate::keywords`]).
    ChangeKeyword {
        object: ActorRef,
        keyword: ObjectRef,
        change: crate::keywords::KeywordChange,
    },
    /// M47.2 — a local assigned a number computed at run time
    /// (`int n = Utility.RandomInt(0, 3)`), read by later [`ValueExpr::Local`]s
    /// in the same run. A fragment holding one never suspends.
    SetLocal { local: String, value: ValueExpr },
    /// M47.2 — effects inlined from the quest script's function
    /// (`kmyQuest.UpdateRadio()`) into an INFO or scene fragment: they run
    /// with `quest` as their context and its VMAD for their properties.
    /// Never holds a latent effect.
    ///
    /// `bindings` pass the inlined function's object arguments that name the
    /// *fragment's* properties: `(parameter property, fragment property)`.
    /// At run time each becomes a property of the quest's VMAD, valued as the
    /// fragment's (`MoveSpouse(akSpeaker, SolitudeHouse)` from an INFO).
    QuestScope {
        quest: QuestRef,
        effects: Vec<Effect>,
        bindings: Vec<(String, String)>,
    },
    /// M47.2 — `Game.RewardPlayerXP(auiXPAmount, abDirect = false)` (FO4 /
    /// Starfield): experience through the engine's level roll; `direct`
    /// bypasses the XP modifiers.
    RewardPlayerXp { amount: ItemCount, direct: bool },
    /// M47.2 — `Game.IncrementStat(asStatName, aiModAmount = 1)` with
    /// literal arguments ([`crate::misc_stats`]).
    IncrementStat { stat: String, amount: i32 },
    /// M47.2 — `Game.AddAchievement(aiAchievementID)` with a literal ID
    /// ([`crate::Achievements`]).
    AddAchievement { id: i32 },
    /// M47.2 — `<location>.SetKeywordData(keyword, value)`.
    SetLocationKeywordData {
        location: ObjectRef,
        keyword: ObjectRef,
        value: ValueExpr,
    },
    /// M47.2 — `<reference>.MoveToMyEditorLocation()`: back to where the plugin
    /// placed it ([`crate::EditorLocation`]).
    MoveToEditorLocation { moved: ObjectRef },
    /// M47.2 — `<reference>.SetLinkedRef(apRef, apKeyword = None)`
    /// ([`crate::LinkedRefOverrides`]); a `None` target clears the link.
    SetLinkedRef {
        object: ObjectRef,
        target: Option<ObjectRef>,
        keyword: Option<ObjectRef>,
    },
    /// M47.2 — `<actor>.DamageValue(av, amount)` / `RestoreValue(av, amount)`
    /// (Skyrim: `DamageActorValue` / `RestoreActorValue("Name", amount)`): the
    /// actor value's damage layer grows by `amount`, or `restore` takes up to
    /// `amount` of it back (CK *DamageValue* / *RestoreValue - ObjectReference*).
    DamageActorValue {
        actor: ActorRef,
        actor_value: ActorValueArg,
        amount: ValueExpr,
        restore: bool,
    },
    /// M47.2 — `<reference>.SetLocRefType(akLoc, akRefType)`
    /// ([`crate::LocRefTypeOverrides`]); a `None` ref type clears what scripts
    /// assigned. The location is not part of the key.
    SetLocRefType {
        object: ObjectRef,
        ref_type: Option<ObjectRef>,
    },
    /// M47.2 — `Game.AdvanceSkill(asSkillName, afMagnitude)` with literal
    /// arguments ([`crate::skills`]).
    AdvanceSkill { skill: String, magnitude: f32 },
    /// M43 — `Game.TeachWord(akWord)` / `Game.UnlockWord(akWord)`: the player obtains a word of
    /// power (the Story Manager's `NVPE`). The two differ only in the shout item `TeachWord` adds,
    /// which this engine does not model, so one effect covers both.
    ObtainWord { word: ObjectRef },
    /// M47.2 — `<actor>.SetPlayerTeammate([abTeammate], [abCanDoFavor])`
    /// ([`crate::FollowerState`]).
    SetPlayerTeammate {
        actor: ActorRef,
        teammate: bool,
        can_do_favor: bool,
    },
    /// M47.2 — `<actor>.SetDoingFavor([abDoingFavor])`.
    SetDoingFavor { actor: ActorRef, doing: bool },
    /// M47.2 — `Game.GivePlayerCaps(n)` / `RemovePlayerCaps(n)` (FO4): the
    /// script bodies add / remove `n` of `Game.GetCaps()` (form `0xF`) on the
    /// player when `n > 0`. Negative `delta` removes. `silent` is the add's
    /// `abSilent` (`AddItem(GetCaps(), n, abSilent)` carries it; the `Game`
    /// calls have no such parameter and are not silent): a non-silent add
    /// raises the pickup notice (#5913).
    PlayerCaps { delta: i32, silent: bool },
    /// M47.2 — `<actor>.SetRelationshipRank(akOther, aiRank)` (+4 Lover …
    /// −4 Archnemesis), on the two actors' bases.
    SetRelationshipRank {
        actor: ActorRef,
        other: ActorRef,
        rank: i8,
    },
    /// M47.2 — `<actor>.Kill([akKiller = None])`: the actor dies through the
    /// engine's death path, or goes into bleedout when essential (CK *Kill*).
    /// `silent` is `KillSilent`: the same death "without a kill event" (CK
    /// *KillSilent - Actor*), so the Story Manager's KILL event is not raised.
    Kill {
        actor: ActorRef,
        killer: Option<ActorRef>,
        silent: bool,
    },
    /// #4816 — `<actor>.StopCombat()`: end the combat [`Self::StartCombat`]
    /// (or ambient faction hostility) armed on this actor. The ambient
    /// hostility system may start a fresh one if the actor still sees
    /// someone it attacks, as the original does.
    StopCombat { actor: ActorRef },
    /// M47.2 — `<actor>.SetCrimeFaction(akFaction)` ([`crate::CrimeFactionOverrides`]);
    /// `None` clears it. A faction the actor is not in is added (CK *SetCrimeFaction
    /// - Actor*).
    SetCrimeFaction {
        actor: ActorRef,
        faction: Option<ObjectRef>,
    },
    /// M47.2 — `<actor>.StopCombatAlarm()` (CK: "Stop all combat and alarms
    /// against this actor"): every actor fighting this one stands down. The CK's
    /// alarms are the crime system's, which the engine does not have.
    StopCombatAlarm { actor: ActorRef },
    /// #5017 — `<actor>.SetUnconscious([abUnconscious = true])`. See
    /// [`crate::ActorControlState::unconscious`] for what the state
    /// suppresses. Wakes FO4's dormant "Starts Unconscious" robots and
    /// turrets.
    SetUnconscious { actor: ActorRef, unconscious: bool },
    /// M47.2 — `<actor>.SetGhost([abIsGhost = true])`
    /// ([`crate::ActorControlState::ghost`]).
    SetGhost { actor: ActorRef, ghost: bool },
    /// #4415 — `<actor>.AddSpell(<spell>[, abVerbose])`: put the spell on the
    /// actor's `SpellList`, applying a constant spell's value changes. The
    /// verbose flag only chooses whether a HUD message shows.
    AddSpell { actor: ActorRef, spell: ObjectRef },
    /// #4415 — `<actor>.RemoveSpell(<spell>)`, undoing the above.
    RemoveSpell { actor: ActorRef, spell: ObjectRef },
    /// #4415 — `AddRaceSpells()` / `<actor>.AddRaceSpells()`: re-apply the
    /// actor's RACE-authored `SPLO` set (level-resolved at spawn and held
    /// as the `RaceSpells` component). Vanilla calls it unqualified from a
    /// quest script's own helper (MQ101QuestScript), which is why the
    /// unqualified spelling is the one that must lower.
    AddRaceSpells { actor: ActorRef },
    /// #4415 — `RemoveRaceSpells()`: clear exactly the racial set above
    /// from the actor's live `SpellList` (a scripted `AddSpell` survives).
    RemoveRaceSpells { actor: ActorRef },
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
    /// An [`ActorRef`] since M47.2, so a dialogue line's speaker re-evaluates.
    EvaluatePackage { actor: ActorRef },
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
    /// M47.2 — `While <condition>` / `Utility.Wait(poll)` / `EndWhile`: while
    /// the guards hold the fragment suspends for `poll_seconds` and checks
    /// again; once they fail it continues. A guard that cannot be read ends
    /// the wait.
    WaitWhile {
        guards: Vec<FragmentGuard>,
        poll_seconds: f32,
    },
    /// `<container>.RemoveItem(<item>, <count = 1>, <silent = false>,
    /// <other container = None>)` (M47.2). Removes up to `count` of the base
    /// item (Papyrus removes what is there when there is less); with an
    /// other container the removed items move there instead of vanishing.
    /// `abSilent` is accepted as a literal and not applied (nothing is announced on a remove).
    RemoveItem {
        container: ActorRef,
        item: ObjectRef,
        count: ItemCount,
        destination: Option<ActorRef>,
    },
    /// M47.2 — `<quest>.<Property> = <literal>`: a write to a quest script's
    /// auto property, recorded in [`crate::QuestScriptVariables`] under the
    /// backing `::Property_var` name `GetVMQuestVariable` conditions read.
    /// The value is a numeric / Bool literal (Bool as 0 / 1).
    /// M47.2 — `<quest>.<Property> = <quest>.<Property> ± delta`: a relative
    /// quest-script variable write (counters).
    ModQuestVariable {
        quest: QuestRef,
        variable: String,
        delta: f32,
    },
    /// M47.2 — `to = from` between two of a quest script's own variables
    /// (`::LastSong02_var = ::LastSong01_var` in an inlined quest-script
    /// body). A source with no value leaves the target untouched.
    CopyQuestVariable {
        quest: QuestRef,
        from: String,
        to: String,
    },
    SetQuestVariable {
        quest: QuestRef,
        variable: String,
        value: f32,
    },
    /// M47.2 — `<actor>.AddToFaction(f)` / `.RemoveFromFaction(f)` /
    /// `.SetFactionRank(f, rank)`: the live `FactionRanks` edit, kept by the
    /// saved [`crate::ScriptedFactionChanges`] ledger across re-derivation.
    ChangeFaction {
        actor: ActorRef,
        faction: ObjectRef,
        change: crate::FactionChange,
    },
    /// M47.2 — FO4 / Starfield `<actor>.SetValue(akActorValue, afValue)`:
    /// sets the actor value's base (CK *SetValue - ObjectReference*). The
    /// actor value is a VMAD `ActorValue` property (an AVIF FormID).
    SetActorValue {
        actor: ActorRef,
        actor_value: ObjectRef,
        /// M47.2 — a run-time number since FORMAT 59 (`GetValue(av) + 1`).
        value: ValueExpr,
    },
    /// M47.2 — `<marker>.AddToMap([abFastTravel = false])`.
    AddToMap {
        marker: ObjectRef,
        fast_travel: bool,
    },
    /// M47.2 — `<location>.SetCleared([abCleared = true])`: a location
    /// property, or a location alias's `GetLocation()`.
    SetLocationCleared { location: ObjectRef, cleared: bool },
    /// M47.2 — `<base>.SetEssential([abEssential = true])` (`protected:
    /// false`) or `SetProtected([abProtected = true])`. Setting one flag
    /// clears the other (CK *SetEssential* / *SetProtected - ActorBase*).
    SetBaseProtection {
        base: ActorBaseRef,
        protected: bool,
        value: bool,
    },
    /// M47.2 — `<keyword>.SendStoryEvent(akLoc, akRef1, akRef2, aiValue1,
    /// aiValue2)`: raises a script (`SCPT`) Story Manager event carrying the
    /// keyword, the location, the two references and the two integers.
    SendStoryEvent {
        keyword: ObjectRef,
        location: Option<ObjectRef>,
        references: [Option<ActorRef>; 2],
        values: [i32; 2],
    },
    /// M47.2 — `<object>.BlockActivation([abBlocked = true][,
    /// abHideActivateText])`: blocks or unblocks the reference's default
    /// activation, recorded in [`crate::ReferenceActivationBlocks`].
    BlockActivation { object: ObjectRef, blocked: bool },
    /// M47.2 — Skyrim's `<actor>.SetAV("Name", value)` /
    /// `SetActorValue("Name", value)`: [`Self::SetActorValue`] with the actor
    /// value named as Papyrus spells it, resolved through
    /// [`crate::ActorValueNames`].
    SetActorValueNamed {
        actor: ActorRef,
        name: String,
        value: ValueExpr,
    },
    /// M47.2 — `<alias>.ForceRefTo(akRef)` (`target` = the reference) or
    /// `<alias>.Clear()` (`target` = `None`): a scripted alias override,
    /// recorded in [`crate::AliasOverrides`] for the alias refresh.
    ForceAlias {
        alias: ObjectRef,
        target: Option<ActorRef>,
    },
    /// M47.2 — `<location alias>.ForceLocationTo(akLocation)`: the location
    /// alias holds the location until the quest stops (CK *ForceLocationTo -
    /// LocationAlias*).
    ForceLocationTo {
        alias: ObjectRef,
        location: ObjectRef,
    },
    /// M47.2 — `<actor>.AddPerk(akPerk[, abNotify])` / `RemovePerk(akPerk)`:
    /// the CK's "adds the specified perk to this actor" (rank 1 when not
    /// owned, else unchanged) and its removal, on the actor's `Perks`.
    ChangePerk {
        actor: ActorRef,
        perk: ObjectRef,
        add: bool,
    },
    /// A manifest or engine-owned static provider call used as a sequencing
    /// barrier. Runtime dispatch occurs only after quest/object ECS guards are
    /// released, then resumes the remaining top-level effects in order.
    ProviderCall(FragmentProviderCall),
}

/// M47.2 — an `ActorBase` receiver: an actor's `GetActorBase()`, or an
/// ActorBase property.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ActorBaseRef {
    OfActor(ActorRef),
    Form(ObjectRef),
}

/// An actor receiver is either the canonical player or a VMAD-resolved
/// object/alias. Keeping the player explicit avoids inventing a VMAD property
/// for `Game.GetPlayer()`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ActorRef {
    Player,
    Object(ObjectRef),
    /// M47.2 — the speaker of the dialogue line whose `TIF` fragment this is
    /// (`akSpeakerRef`, and the `akSpeaker = akSpeakerRef as Actor` local
    /// Champollion emits). Only INFO fragments lower it, and never alongside
    /// a latent tail, so it is always resolved from the live dialogue call.
    Speaker,
}

impl Effect {
    /// A fresh [`Effect::Interpret`] run of `script.function(args)` — `resume: None`, the shape every
    /// fragment / event dispatcher starts a VM run with (#6154).
    pub fn interpret(script: impl Into<String>, function: impl Into<String>, args: Vec<VmStored>) -> Self {
        Effect::Interpret { script: script.into(), function: function.into(), args, resume: None }
    }
}

impl Effect {
    /// #5807 — the variant-name table the two conformance harnesses
    /// (`mq101_conformance/`, `fragment_coverage.rs`) each pasted as a
    /// ~99-line match, edited by hand on every new variant (a missed copy
    /// mis-reports that harness's coverage). One home, next to
    /// `is_placeholder`; the exhaustive match means a new variant without
    /// a row here is a compile error.
    pub fn kind(&self) -> &'static str {
        match self {
            Effect::Conditional { .. } => "Conditional",
            Effect::SetGlobalValue { .. } => "SetGlobalValue",
            Effect::SetStage { .. } => "SetStage",
            Effect::StartQuest { .. } => "StartQuest",
            Effect::StopQuest { .. } => "StopQuest",
            Effect::CompleteQuest { .. } => "CompleteQuest",
            Effect::ResetQuest { .. } => "ResetQuest",
            Effect::SetQuestActive { .. } => "SetQuestActive",
            Effect::SetObjectiveDisplayed { .. } => "SetObjectiveDisplayed",
            Effect::SetObjectiveCompleted { .. } => "SetObjectiveCompleted",
            Effect::SetObjectiveFailed { .. } => "SetObjectiveFailed",
            Effect::CompleteAllObjectives { .. } => "CompleteAllObjectives",
            Effect::FailAllObjectives { .. } => "FailAllObjectives",
            Effect::AddItem { .. } => "AddItem",
            Effect::RemoveItem { .. } => "RemoveItem",
            Effect::SetQuestVariable { .. } => "SetQuestVariable",
            Effect::ChangeFaction { .. } => "ChangeFaction",
            Effect::SetActorValue { .. } => "SetActorValue",
            Effect::EquipItem { .. } => "EquipItem",
            Effect::MoveTo { .. } => "MoveTo",
            Effect::MovePlayerTo { .. } => "MovePlayerTo",
            Effect::ChangePerk { .. } => "ChangePerk",
            Effect::ForceAlias { .. } => "ForceAlias",
            Effect::ModQuestVariable { .. } => "ModQuestVariable",
            Effect::SetActorValueNamed { .. } => "SetActorValueNamed",
            Effect::DamageActorValue { .. } => "DamageActorValue",
            Effect::BlockActivation { .. } => "BlockActivation",
            Effect::SendStoryEvent { .. } => "SendStoryEvent",
            Effect::AddToMap { .. } => "AddToMap",
            Effect::SetLocationCleared { .. } => "SetCleared",
            Effect::SetBaseProtection { .. } => "SetEssential",
            Effect::Kill { .. } => "Kill",
            Effect::QuestScope { .. } => "QuestScope",
            Effect::CopyQuestVariable { .. } => "CopyQuestVariable",
            Effect::SetRelationshipRank { .. } => "SetRelationshipRank",
            Effect::RewardPlayerXp { .. } => "RewardPlayerXP",
            Effect::IncrementStat { .. } => "IncrementStat",
            Effect::AddAchievement { .. } => "AddAchievement",
            Effect::AdvanceSkill { .. } => "AdvanceSkill",
            Effect::ObtainWord { .. } => "ObtainWord",
            Effect::SetLinkedRef { .. } => "SetLinkedRef",
            Effect::SetLocRefType { .. } => "SetLocRefType",
            Effect::MoveToEditorLocation { .. } => "MoveToEditorLocation",
            Effect::SetLocationKeywordData { .. } => "SetLocationKeywordData",
            Effect::ForceLocationTo { .. } => "ForceLocationTo",
            Effect::SetPlayerTeammate { .. } => "SetPlayerTeammate",
            Effect::SetDoingFavor { .. } => "SetDoingFavor",
            Effect::PlayerCaps { .. } => "PlayerCaps",
            Effect::SetLocal { .. } => "SetLocal",
            Effect::SetReferenceVariable { .. } => "SetReferenceVariable",
            Effect::ShowMessage { .. } => "ShowMessage",
            Effect::ShowHelpMessage { .. } => "ShowHelpMessage",
            Effect::Notification { .. } => "Notification",
            Effect::ShowBarterMenu { .. } => "ShowBarterMenu",
            Effect::ChangeKeyword { .. } => "ChangeKeyword",
            Effect::SetGhost { .. } => "SetGhost",
            Effect::SetAlly { .. } => "SetAlly",
            Effect::DeleteWhenAble { .. } => "DeleteWhenAble",
            Effect::WaitWhile { .. } => "WaitWhile",
            Effect::Interpret { .. } => "Interpret",
            Effect::Disable { .. } => "Disable",
            Effect::Enable { .. } => "Enable",
            Effect::StartScene { .. } => "StartScene",
            Effect::StopScene { .. } => "StopScene",
            Effect::Activate { .. } => "Activate",
            Effect::SetOpen { .. } => "SetOpen",
            Effect::SetLocked { .. } => "SetLocked",
            Effect::SetLockLevel { .. } => "SetLockLevel",
            Effect::SetPlayerRestrained { .. } => "SetPlayerRestrained",
            Effect::SetPlayerControls { .. } => "SetPlayerControls",
            Effect::SetPlayerAiDriven { .. } => "SetPlayerAiDriven",
            Effect::SetHudCartMode { .. } => "SetHudCartMode",
            Effect::SetInChargen { .. } => "SetInChargen",
            Effect::ShowRaceMenu => "ShowRaceMenu",
            Effect::RequestSave { .. } => "RequestSave",
            Effect::SetEnemy { .. } => "SetEnemy",
            Effect::StartCombat { .. } => "StartCombat",
            Effect::StopCombat { .. } => "StopCombat",
            Effect::StopCombatAlarm { .. } => "StopCombatAlarm",
            Effect::SetCrimeFaction { .. } => "SetCrimeFaction",
            Effect::AddSpell { .. } => "AddSpell",
            Effect::RemoveSpell { .. } => "RemoveSpell",
            Effect::PlayIdle { .. } => "PlayIdle",
            Effect::SetVehicle { .. } => "SetVehicle",
            Effect::TetherToHorse { .. } => "TetherToHorse",
            Effect::SetMotionType { .. } => "SetMotionType",
            Effect::SetSittingRotation { .. } => "SetSittingRotation",
            Effect::ExitCart { .. } => "ExitCart",
            Effect::RegisterPlayerAnimationEvent { .. } => "RegisterPlayerAnimationEvent",
            Effect::EvaluatePackage { .. } => "EvaluatePackage",
            Effect::Wait { .. } => "Wait",
            Effect::WaitForActors3DLoaded { .. } => "WaitForActors3DLoaded",
            Effect::ProviderCall(_) => "ProviderCall",
            Effect::SetUnconscious { .. } => "SetUnconscious",
            Effect::AddRaceSpells { .. } => "AddRaceSpells",
            Effect::RemoveRaceSpells { .. } => "RemoveRaceSpells",
        }
    }

    /// Whether this effect's runtime arm only *records* that the call ran —
    /// a counter or flag nothing reads — without doing what the call does
    /// (#4328). `Game.ShowRaceMenu` opens no menu; `Game.RequestSave` and
    /// `RequestAutoSave` write no save; `Game.SetHudCartMode` sets
    /// `PlayerControlState::hud_cart_mode`, which no HUD reads (#4372).
    /// `Game.SetInChargen` is not a placeholder: its `abDisableSaving` flag
    /// gates the save command.
    ///
    /// Lowering them is deliberate: declining would discard every sibling
    /// effect in MQ101's chargen fragments. But a fragment claimed only by
    /// way of one is coverage with a hole in it, so the coverage harnesses
    /// count those fragments separately rather than letting the stub pass
    /// for the real behavior.
    ///
    /// #5975 — `Game.AddAchievement` is one too: its arm records into
    /// [`crate::misc_stats::Achievements`], which nothing reads (the platform
    /// profile owns real achievements). This is the flat test on one effect;
    /// a harness asks [`Self::contains_placeholder`] so one inside an
    /// `If` / `QuestScope` counts.
    pub fn is_placeholder(&self) -> bool {
        matches!(
            self,
            Effect::ShowRaceMenu
                | Effect::RequestSave { .. }
                | Effect::SetHudCartMode { .. }
                | Effect::AddAchievement { .. }
        )
    }

    /// Call `visit` on this effect and every effect nested under it (the
    /// `then` / `else` arms of a [`Effect::Conditional`], the body of a
    /// [`Effect::QuestScope`]), outermost first (#5975).
    pub fn visit_nested(&self, visit: &mut impl FnMut(&Effect)) {
        visit(self);
        match self {
            Effect::Conditional {
                then_effects,
                else_effects,
                ..
            } => {
                for effect in then_effects.iter().chain(else_effects) {
                    effect.visit_nested(visit);
                }
            }
            Effect::QuestScope { effects, .. } => {
                for effect in effects {
                    effect.visit_nested(visit);
                }
            }
            _ => {}
        }
    }

    /// [`Self::is_placeholder`] for this effect or anything nested under it.
    /// The coverage harnesses use this: a `ShowRaceMenu` inside an `If` is
    /// still a stub claimed as coverage (#5975).
    pub fn contains_placeholder(&self) -> bool {
        let mut found = false;
        self.visit_nested(&mut |effect| found |= effect.is_placeholder());
        found
    }
}

/// How many items an `AddItem` / `RemoveItem` moves (M47.2): a literal, or a
/// global's value read when the effect applies — `<global>.GetValueInt()`,
/// `<global>.GetValue() as Int` or `<global>.Value as Int`, the spellings
/// that carry ~210 vanilla Skyrim / FO4 / Starfield price and ransom counts.
/// The read truncates toward zero, as Papyrus's `Float` → `Int` cast does; a
/// negative value skips the effect.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ItemCount {
    Literal(u32),
    Global(ObjectRef),
    /// M47.2 — a number computed at run time (`aiCount * 2`,
    /// `Amount.GetValueInt() * Reward.GetValueInt()`).
    Value(ValueExpr),
}

/// FO4's `<actor>.SetValue(akAV, afValue)` and its alias spelling
/// `<alias>.TryToSetActorValue(akAV, afValue)` ("sets the actor value on the
/// reference in this alias, if any" — an empty alias is a runtime no-op), and
/// — M47.2 — Skyrim's `SetAV` / `SetActorValue("Name", afValue)`.
/// M47.2 — how a call names an actor value: an `ActorValue` property, or a
/// name — Skyrim's string (`SetAV("Health", …)`) or FO4's
/// `Game.Get<Name>AV()` accessor (`GetHealthAV` names `Health`).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ActorValueArg {
    Form(ObjectRef),
    Named(String),
}

/// M47.2 — which reading of an actor value a [`ValueExpr::ActorValueReading`]
/// takes besides the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ActorValueReading {
    /// `GetBaseValue` / `GetBaseActorValue`: the base, without modifiers.
    Base,
    /// `GetValuePercentage` / `GetActorValuePercentage`: the current value as
    /// a fraction (0 to 1) of its maximum.
    Percent,
}
