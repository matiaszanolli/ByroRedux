//! ObScript quest runtime (M47.3 — functional quests, phase 1: Oblivion).
//!
//! Wires [`crate::obscript_vm`] to live quest state: a
//! [`QuestScriptTable`] resource maps quest FormIDs to their parsed SCPT
//! (built once per load order beside `install_package_records`), and
//! [`obscript_quest_tick_system`] executes each *running* quest's GameMode
//! block on the vanilla cadence — one execution every
//! [`QUEST_SCRIPT_INTERVAL_SECS`] seconds, matching the original engine's
//! `fQuestProcessInterval` default of 5.
//!
//! Phase-1 command semantics (ids empirically derived — see
//! [`crate::obscript_vm`]'s module doc):
//!
//! | id | command | phase-1 lowering |
//! |----|---------|------------------|
//! | `0x1039` | SetStage | [`QuestStageState::set_stage`] on the target quest |
//! | `0x1036` / `0x1037` | StartQuest / StopQuest | start/stop + script-loop attach/detach |
//! | `0x103a` / `0x103b` | GetStage / GetStageDone | [`QuestStageState`] reads |
//! | `0x1038` | GetQuestRunning | 1 when the quest is running |
//! | `0x1059` / `0x1000` | Message / MessageBox | queued in the native message UI; MessageBox waits for an answer |
//! | `0x100c` | GetSecondsPassed | per-script elapsed since last tick |
//! | `0x101f` | GetButtonPressed | consumes the pending answer, or −1 when none is available |
//! | condition 79 | GetQuestVariable | reads the quest's persisted [`ObScriptQuestLocals`] slot |
//! | `0x10ce` | GetSelf | the script owner's form id |
//! | everything else | — | traced no-op returning 0, counted in [`ObScriptQuestTimers`] |
//!
//! # Evidence per game (#6015)
//!
//! "Shared across the three games" is measured, not assumed:
//! `obscript_tools names <ESM> all 1` aligns each script's `SCTX` source with its
//! compiled statements. Number of vanilla-master scripts (every script type;
//! 2026-10-19) whose compiled id aligned to the named command:
//!
//! | id | command | Oblivion | Fallout 3 | New Vegas |
//! |----|---------|---------:|----------:|----------:|
//! | `0x1039` | SetStage | 726 | 181 | 357 |
//! | `0x103a` | GetStage | 790 | 201 | 171 |
//! | `0x103b` | GetStageDone | 123 | 62 | 100 |
//! | `0x1036` | StartQuest | 10 | 5 | 47 |
//! | `0x1037` | StopQuest | 25 | 10 | 65 |
//! | `0x1038` | GetQuestRunning | 27 | 18 | 96 (incl. alias `GetQR`) |
//! | `0x1059` | Message (Oblivion) / ShowMessage (Fallout) | 152 | 221 | 329 |
//! | `0x100c` | GetSecondsPassed | 516 | 236 | 255 |
//! | `0x101f` | GetButtonPressed | 46 | 58 | 129 |
//! | `0x10ce` | GetSelf | 438 | 40 | 122 (87 % by name; the rest is the `this` alias) |
//! | `0x1000` | MessageBox | 264 | **0** | **0** |
//!
//! Every id agrees across the three games except `0x1000`: no Fallout script
//! compiles to it, so the host declines it there rather than run an Oblivion
//! meaning the Fallout games never evidenced.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::{QustRecord, ScriptRecord};

use crate::condition::{evaluate_script_getter, resolve_entity_by_global_form_id, ConditionFunction};
use crate::globals::Globals;
use crate::obscript_actions::Action;
use crate::obscript_vm::{form_from_halves, form_halves, BlockOutcome, ObScriptHost, ObScriptProgram, ObScriptValue, VmState};
use crate::quest_stages::{QuestFormId, QuestObjectiveState, QuestStageState, QuestStatus};

/// Vanilla quest-script cadence: the original engine re-runs each running
/// quest's GameMode block every 5 seconds (`fQuestProcessInterval`).
pub const QUEST_SCRIPT_INTERVAL_SECS: f32 = 5.0;

/// `CompleteQuest` (all three legacy games).
const CMD_COMPLETE_QUEST: u16 = 0x1071;
/// Fallout 3 / New Vegas quest commands, ids recovered by aligning vanilla
/// `SCTX` with the compiled stream: `SetObjectiveCompleted`,
/// `SetObjectiveDisplayed`, `GetObjectiveCompleted`, `GetObjectiveDisplayed`,
/// `SetQuestDelay`, `GetQuestCompleted` (= `0x1000` + CTDA 546).
const CMD_SET_OBJECTIVE_COMPLETED: u16 = 0x11a2;
const CMD_SET_OBJECTIVE_DISPLAYED: u16 = 0x11a3;
const CMD_GET_OBJECTIVE_COMPLETED: u16 = 0x11a4;
const CMD_GET_OBJECTIVE_DISPLAYED: u16 = 0x11a5;
const CMD_SET_QUEST_DELAY: u16 = 0x11dc;
const CMD_GET_QUEST_COMPLETED: u16 = 0x1222;

/// Getter commands that are condition functions: the command id is `0x1000 +
/// the CTDA function index`, so the engine's one condition evaluator answers
/// them (`evaluate_script_getter`) instead of this host growing a second copy
/// of `GetDead`, `GetDistance`, ... Each was confirmed by aligning vanilla
/// `SCTX` source with the compiled stream on Oblivion, FO3 and FNV (every id
/// names the same command in all three). A function outside this list stays a
/// counted decline even when the catalog knows its index: a catalog index is
/// not proof that the same number is a *getter* command in a legacy game.
const SCRIPT_GETTERS: &[ConditionFunction] = &[
    ConditionFunction::GetDistance,       // 0x1001
    ConditionFunction::GetActorValue,     // 0x100e (declined: the ObScript `GetAV` code goes through `obscript_actor_values`)
    ConditionFunction::GetDead,           // 0x102e
    ConditionFunction::GetInCell,         // 0x1043
    ConditionFunction::GetInFaction,      // 0x1047
    ConditionFunction::GetIsID,           // 0x1048
    ConditionFunction::GetFactionRank,    // 0x1049
    ConditionFunction::GetGlobalValue,    // 0x104a
    ConditionFunction::GetRandomPercent,  // 0x104d
    ConditionFunction::GetLevel,          // 0x1050
    ConditionFunction::GetEquipped,       // 0x10b6
    ConditionFunction::GetReputation,     // 0x123d (FNV)
    ConditionFunction::GetReputationThreshold, // 0x123f (FNV)
    ConditionFunction::HasPerk,           // 0x11c1 (FO3 / FNV)
    ConditionFunction::IsHardcore,        // 0x124a (FNV)
];

/// Whether the host models command `cmd` in the loaded game family — answers it
/// or lowers it rather than declining it. What `obscript_tools names` marks
/// open. `fallout` is Fallout 3 / New Vegas (their objective and reputation
/// commands are in no other table).
pub fn command_is_modeled(cmd: u16, fallout: bool) -> bool {
    // The quest and message commands `call` matches by id, and the synthetic ones.
    // `0x15` is the `Set` statement the VM itself runs (the census attributes a script's `Set` lines to it).
    const VM_LEVEL: &[u16] = &[
        0x0015, 0x1039, 0x1036, 0x1037, 0x103a, 0x103b, 0x1038, 0x1059, 0x1000, 0x100c, 0x101f, 0x10ce,
        0x10cd, 0x1069, CMD_COMPLETE_QUEST, 0x10cf, 0x10a5, 0x10a6,
    ];
    const FALLOUT_ONLY: &[u16] = &[
        CMD_SET_OBJECTIVE_COMPLETED, CMD_SET_OBJECTIVE_DISPLAYED, CMD_GET_OBJECTIVE_COMPLETED,
        CMD_GET_OBJECTIVE_DISPLAYED, CMD_SET_QUEST_DELAY, CMD_GET_QUEST_COMPLETED,
    ];
    if VM_LEVEL.contains(&cmd) || (fallout && FALLOUT_ONLY.contains(&cmd)) {
        return true;
    }
    // `MoveToMarkerWithFade` and `GetLinkedRef` are Fallout commands; the numbers are
    // other commands in Oblivion (`PurgeCellBuffers` for `0x116b`).
    if crate::obscript_more::models(cmd, fallout) {
        return true;
    }
    if !fallout && matches!(cmd, 0x124f | 0x116b | 0x11af) {
        return false;
    }
    if crate::obscript_actions::handles(cmd)
        || crate::obscript_effects::handles(cmd)
        || crate::obscript_effects::is_getter(cmd)
    {
        return true;
    }
    cmd.checked_sub(0x1000)
        .map(|index| ConditionFunction::from_index(u32::from(index)))
        .is_some_and(|function| SCRIPT_GETTERS.contains(&function) && function != ConditionFunction::GetActorValue)
}

/// One quest's compiled script, plus the SCRO reference table needed to
/// resolve its `' r<n>'`/argument references at execution time.
pub struct QuestScript {
    pub script: Arc<ScriptRecord>,
    /// Quest FormID owning the script (the implicit caller).
    pub quest: QuestFormId,
    /// The `QUST` record's authored script interval in seconds (FO3/FNV
    /// `DATA` quest delay; `0` when unauthored or on Oblivion).
    pub authored_delay: f32,
}

/// The seconds between a quest script's runs. A runtime `SetQuestDelay`
/// overrides the authored `QUST` delay, and either one being `0` (or
/// negative) means the engine default: the GECK *SetQuestDelay* page says
/// "Setting delay to 0 tells the quest to process at the INI setting default
/// (5 seconds)" — the INI default, not the authored delay.
pub fn quest_script_interval(authored: f32, runtime_override: Option<f32>) -> f32 {
    let chosen = runtime_override.unwrap_or(authored);
    if chosen.is_finite() && chosen > 0.0 {
        chosen
    } else {
        QUEST_SCRIPT_INTERVAL_SECS
    }
}

/// Quest FormID → compiled script. Built once per load order; quests whose
/// `script_ref` doesn't resolve to a parsed SCPT are simply absent.
#[derive(Default)]
pub struct QuestScriptTable {
    pub scripts: HashMap<u32, QuestScript>,
}

impl Resource for QuestScriptTable {}

/// Ring of the most recent ObScript side effects, for byro-dbg inspection
/// and live verification. Bounded so unbounded message spam can't grow it.
#[derive(Default)]
pub struct ObScriptEffectLog {
    pub entries: std::collections::VecDeque<(String, String)>,
}

impl Resource for ObScriptEffectLog {}

impl ObScriptEffectLog {
    const CAP: usize = 64;

    fn push(&mut self, quest: String, effect: String) {
        if self.entries.len() == Self::CAP {
            self.entries.pop_front();
        }
        self.entries.push_back((quest, effect));
    }
}

/// Where a reference variable's exact form id is kept among a quest's variables:
/// the low and high 16 bits of variable `n` are slots `REF_LOW_BASE + n` and
/// `REF_HIGH_BASE + n`. A script has far fewer than `0x4000` variables.
const REF_LOW_BASE: u16 = 0x4000;
const REF_HIGH_BASE: u16 = 0x8000;

/// The local variables quest scripts hold between GameMode runs: quest FormID to
/// the 1-based `SLSD` slot and its value. A variable no run assigned is absent and
/// reads `0`, the value every ObScript local starts at. `GetQuestVariable`
/// (condition function 79) reads it. Saved: a quest script's variables are
/// gameplay state.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ObScriptQuestLocals {
    values: std::collections::BTreeMap<u32, std::collections::BTreeMap<u16, f32>>,
    /// Quest FormID → the interval a script's `SetQuestDelay` chose. Gameplay
    /// state like the locals: a quest that slowed itself down must not speed
    /// back up on a reload.
    delays: std::collections::BTreeMap<u32, f32>,
}

impl Resource for ObScriptQuestLocals {}

impl ObScriptQuestLocals {
    /// A quest's variables, to seed a run's [`VmState`].
    pub fn of(&self, quest: u32) -> HashMap<u16, f32> {
        self.values
            .get(&quest)
            .map(|slots| slots.iter().filter(|(&slot, _)| slot < REF_LOW_BASE).map(|(&slot, &value)| (slot, value)).collect())
            .unwrap_or_default()
    }

    /// The references a quest's reference variables hold, exactly.
    pub fn refs_of(&self, quest: u32) -> HashMap<u16, u32> {
        let Some(slots) = self.values.get(&quest) else {
            return HashMap::new();
        };
        slots
            .iter()
            .filter(|(&slot, _)| (REF_LOW_BASE..REF_HIGH_BASE).contains(&slot))
            .map(|(&slot, &low)| {
                let variable = slot - REF_LOW_BASE;
                (variable, form_from_halves(low, slots.get(&(REF_HIGH_BASE + variable)).copied().unwrap_or(0.0)))
            })
            .filter(|&(_, form)| form != 0)
            .collect()
    }

    /// Keep a run's variables for the next run.
    pub fn store(&mut self, quest: u32, locals: &HashMap<u16, f32>) {
        self.store_state(quest, locals, &HashMap::new());
    }

    /// [`Self::store`] with the references the reference variables hold: each is
    /// kept as two exact `f32` halves under slots no variable uses, so the saved
    /// shape is the one it always was.
    pub fn store_state(&mut self, quest: u32, locals: &HashMap<u16, f32>, refs: &HashMap<u16, u32>) {
        let mut slots: std::collections::BTreeMap<u16, f32> = locals.iter().map(|(&slot, &value)| (slot, value)).collect();
        for (&variable, &form) in refs.iter().filter(|&(&variable, &form)| form != 0 && variable < REF_LOW_BASE) {
            let (low, high) = form_halves(form);
            slots.insert(REF_LOW_BASE + variable, low);
            slots.insert(REF_HIGH_BASE + variable, high);
        }
        if slots.is_empty() {
            self.values.remove(&quest);
        } else {
            self.values.insert(quest, slots);
        }
    }

    /// The interval a script chose for `quest` with `SetQuestDelay`, if any.
    pub fn delay_override(&self, quest: u32) -> Option<f32> {
        self.delays.get(&quest).copied()
    }

    /// Record a script's `SetQuestDelay` for `quest`.
    pub fn set_delay(&mut self, quest: u32, seconds: f32) {
        self.delays.insert(quest, seconds);
    }

    /// Write `quest`'s variable `slot`: a remote `set Quest.var to ...` from another script.
    pub fn set(&mut self, quest: u32, slot: u16, value: f32) {
        self.values.entry(quest).or_default().insert(slot, value);
    }

    /// The value of `quest`'s variable `slot` (0 when never assigned).
    pub fn get(&self, quest: u32, slot: u16) -> f32 {
        self.values.get(&quest).and_then(|slots| slots.get(&slot)).copied().unwrap_or(0.0)
    }
}

/// Per-quest script-loop timers + execution counters.
#[derive(Default)]
pub struct ObScriptQuestTimers {
    /// Seconds accumulated toward the next GameMode run per quest.
    pub elapsed: HashMap<u32, f32>,
    /// Total GameMode executions per quest (diagnostics).
    pub runs: HashMap<u32, u32>,
    /// Unknown-command hit counts per quest (diagnostics).
    pub unknown_commands: HashMap<u32, HashMap<u16, u32>>,
}

impl Resource for ObScriptQuestTimers {}

/// Install quest scripts from a parsed load order into the world. Called
/// once per load beside [`crate::install_package_records`]'s call site,
/// where the `EsmIndex` is in hand. Start-game-enabled quests are started
/// immediately (their script loops begin ticking on the next scheduler run),
/// matching the original engine's `Start Game Enabled` flag.
pub fn install_quest_scripts(
    world: &mut World,
    quests: &HashMap<u32, QustRecord>,
    scripts: &HashMap<u32, ScriptRecord>,
) {
    let mut table = QuestScriptTable::default();
    let mut installed = 0usize;
    for quest in quests.values() {
        if quest.script_ref == 0 {
            continue;
        }
        let Some(script) = scripts.get(&quest.script_ref) else {
            continue;
        };
        if script.compiled.is_empty() {
            continue;
        }
        let form = QuestFormId(quest.form_id);
        table.scripts.insert(
            quest.form_id,
            QuestScript {
                script: Arc::new(script.clone()),
                quest: form,
                authored_delay: quest.quest_delay.unwrap_or(0.0),
            },
        );
        installed += 1;
    }
    if installed == 0 {
        return;
    }
    world.init_resource::<QuestScriptTable>();
    world.init_resource::<ObScriptQuestTimers>();
    world.init_resource::<ObScriptQuestLocals>();
    world.init_resource::<ObScriptEffectLog>();
    world.insert_resource(table);
    log::info!("Installed {installed} ObScript quest scripts");
}

/// Execute one quest script's GameMode block against the world.
/// Returns the outcome (diagnostics).
pub fn run_quest_game_mode(
    world: &World,
    quest_form_id: u32,
    elapsed: f32,
) -> Option<BlockOutcome> {
    let table = world.try_resource::<QuestScriptTable>()?;
    let quest_script = table.scripts.get(&quest_form_id)?;
    let script = quest_script.script.clone();
    drop(table);

    let program = ObScriptProgram::new(&script);
    let mut state = world
        .try_resource::<ObScriptQuestLocals>()
        .map(|locals| VmState { locals: locals.of(quest_form_id), ref_locals: locals.refs_of(quest_form_id) })
        .unwrap_or_default();
    let mut host = ObScriptCommandHost::new(world, quest_form_id, None, elapsed, &script);
    let outcome = program.run_block(0, &mut state, &mut host);
    // `host` holds no guard (Drop is not implemented) — the borrow ends
    // here by scope, which is what the old `drop(host)` extended.
    if let Some(mut locals) = world.try_resource_mut::<ObScriptQuestLocals>() {
        locals.store_state(quest_form_id, &state.locals, &state.ref_locals);
    }
    Some(outcome)
}

/// ECS host: lowers the phase-1 command set onto [`QuestStageState`],
/// [`Globals`], and the effect log. Unknown commands are traced no-ops
/// returning 0 and counted per quest.
pub struct ObScriptCommandHost<'a> {
    pub world: &'a World,
    /// The FormID of the script's owner: its quest, or the reference it is
    /// attached to. Keys the effect log and the decline census.
    pub owner: u32,
    /// The entity of an object script's reference; `None` for a quest script.
    /// A command written with no calling reference acts on it, and a quest
    /// command written with no quest has no quest to default to.
    pub reference: Option<byroredux_core::ecs::storage::EntityId>,
    pub elapsed: f32,
    pub script: &'a ScriptRecord,
    /// The reference that activated or entered the owner, while an event block
    /// that has one (`OnActivate`) runs: what `GetActionRef` / `IsActionRef`
    /// answer and what an `Activate` with no actor argument activates by. `None`
    /// everywhere else — the GECK says `GetActionRef` "will only return a value"
    /// inside such a block.
    pub action_ref: Option<u32>,
    /// How many commands this host has declined so far: a run that declined any
    /// is one whose outcome is not fully known.
    pub declined: std::cell::Cell<u32>,
    /// Set while an item's script runs: the item and the container that holds it. An
    /// item in an inventory is no reference, so a command written with no caller has
    /// nothing to act on, `GetContainer` answers the holder and `RemoveMe` removes the
    /// item from it.
    pub item: Option<ItemScriptContext>,
    /// Set by a command that ends the script (`RemoveMe`), read once by the VM.
    ends: std::cell::Cell<bool>,
}

/// What an item's script knows about where it is (see [`ObScriptCommandHost::item`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemScriptContext {
    /// The item's base FormID.
    pub item: u32,
    /// The holder's FormID (`0x14` for the player).
    pub container: u32,
}

impl<'a> ObScriptCommandHost<'a> {
    pub fn new(world: &'a World, owner: u32, reference: Option<byroredux_core::ecs::storage::EntityId>, elapsed: f32, script: &'a ScriptRecord) -> Self {
        Self { world, owner, reference, elapsed, script, action_ref: None, declined: std::cell::Cell::new(0), item: None, ends: std::cell::Cell::new(false) }
    }
}

/// The music type the editor ID `name` names.
fn world_music(world: &World, name: &str) -> Option<u32> {
    world.try_resource::<crate::obscript_more::MusicTracks>()?.by_editor_id(name).map(|track| track.form_id)
}

impl<'a> ObScriptHost for ObScriptCommandHost<'a> {
    fn ends_script(&mut self) -> bool {
        self.ends.replace(false)
    }

    fn text_argument(&mut self, cmd: u16, text: &str, rest: &[ObScriptValue]) -> Option<ObScriptValue> {
        match cmd {
            // `PlayIdle <idle>`: the IDLE the editor ID names.
            0x1190 => crate::idle_names::idle_by_editor_id(self.world, text).map(ObScriptValue::Ref),
            // `GetGS <setting>`: the game setting's value.
            0x1100 => crate::obscript_state::game_setting(self.world, text).map(ObScriptValue::Num),
            // `GetPCMiscStat "name"`: the player's count of that stat (the command then answers it).
            0x1138 => Some(ObScriptValue::Num(crate::obscript_more::misc_stat(self.world, text) as f32)),
            // `ModPCMiscStat "name" <amount>`: add to that stat.
            0x1137 => {
                crate::obscript_more::mod_misc_stat(self.world, text, rest.first().map_or(0.0, |v| v.as_num()) as i32);
                Some(ObScriptValue::Num(0.0))
            }
            // `PlayMusic <music>`: the music type (MUSC) the editor ID names.
            0x11ff => world_music(self.world, text).map(ObScriptValue::Ref),
            // `EssentialDeathReload "message"` / `PlayBink "file"`: the string is the command's own argument.
            0x114d | 0x1114 => {
                if let Some(mut texts) = self.world.try_resource_mut::<crate::obscript_more::TextArguments>() {
                    texts.0 = vec![text.to_owned()];
                }
                Some(ObScriptValue::Num(0.0))
            }
            // `ShowWarning "text"`: the warning is written to the log and shown as a notification.
            0x11b9 => {
                log::warn!("ObScript ShowWarning: {text}");
                crate::obscript_state::show_warning(self.world, text);
                Some(ObScriptValue::Num(0.0))
            }
            _ => None,
        }
    }

    fn text_arguments(&mut self, _cmd: u16, texts: &[String]) {
        if let Some(mut stored) = self.world.try_resource_mut::<crate::obscript_more::TextArguments>() {
            stored.0 = texts.to_vec();
        }
    }

    fn message(&mut self, message: crate::obscript_vm::ObScriptMessage) {
        // #6015 — `0x1000` is Oblivion's `MessageBox`; no Fallout 3 / New Vegas script
        // compiles to it (see the evidence table above), so a box decoded from that id
        // there is a lookalike, not a MessageBox.
        if message.message_box && crate::ScriptGame::is_fallout3_nv(self.world) {
            self.decline(crate::obscript_vm::CMD_MESSAGE_BOX);
            return;
        }
        let kind = if message.message_box { "MessageBox" } else { "Message" };
        log::debug!("quest {:08X} {kind}: {:?}", self.owner, message.text);
        crate::messages::show_obscript_message(self.world, QuestFormId(self.owner), message);
    }

    fn call(
        &mut self,
        cmd: u16,
        args: &[ObScriptValue],
        caller: Option<u32>,
    ) -> Option<ObScriptValue> {
        // A command written with no calling reference acts on the reference an
        // object script is attached to; a quest script has none.
        let caller = caller.or_else(|| self.reference.map(|_| self.owner));
        let num = |i: usize| args.get(i).map(|v| v.as_num()).unwrap_or(0.0);
        let ref_arg = |i: usize| match args.get(i) {
            Some(ObScriptValue::Ref(r)) => Some(*r),
            Some(ObScriptValue::Num(n)) if *n > 0.0 => Some(*n as u32),
            _ => None,
        };
        // The quest a quest command targets: the explicit reference argument,
        // or the script's own quest when none was written. A reference that
        // IS written but is null — the VM hands over `Ref(0)` for an
        // unresolved slot or the `Z` null tag — has no quest to act on, so
        // the command is declined: defaulting it to the own quest, or acting
        // on form 0, would stage a quest the script never named (#5629).
        let own_quest = (self.reference.is_none() && self.item.is_none()).then_some(self.owner);
        let quest_target = |i: usize| match args.get(i) {
            Some(ObScriptValue::Ref(0)) => None,
            _ => ref_arg(i).or(own_quest),
        };
        let effect = |host: &mut Self, text: String| {
            if let Some(mut log) = host.world.try_resource_mut::<ObScriptEffectLog>() {
                log.push(format!("{:08X}", host.owner), text);
            }
        };
        match cmd {
            // SetStage <quest> <stage> — statement form (0x1039). The caller
            // slot may carry the quest when authored as `<quest>.SetStage`;
            // the first ref argument is the quest in the flat form.
            0x1039 => {
                let Some(target) = quest_target(0) else {
                    return self.decline(cmd);
                };
                let stage = num(1) as u16;
                if let Some(mut stages) = self.world.try_resource_mut::<QuestStageState>() {
                    stages.set_stage(QuestFormId(target), stage);
                }
                effect(self, format!("SetStage {:08X} {}", target, stage));
                None
            }
            // StartQuest / StopQuest
            0x1036 => {
                let Some(target) = quest_target(0) else {
                    return self.decline(cmd);
                };
                if let Some(mut stages) = self.world.try_resource_mut::<QuestStageState>() {
                    stages.start_quest(QuestFormId(target), None);
                }
                effect(self, format!("StartQuest {:08X}", target));
                Some(ObScriptValue::Num(1.0))
            }
            0x1037 => {
                let Some(target) = quest_target(0) else {
                    return self.decline(cmd);
                };
                if let Some(mut stages) = self.world.try_resource_mut::<QuestStageState>() {
                    stages.stop(QuestFormId(target));
                }
                effect(self, format!("StopQuest {:08X}", target));
                None
            }
            // GetStage <quest>
            0x103a => {
                let Some(target) = quest_target(0) else {
                    return self.decline(cmd);
                };
                let stage = self
                    .world
                    .try_resource::<QuestStageState>()
                    .map(|s| s.get_stage(QuestFormId(target)))
                    .unwrap_or(0);
                Some(ObScriptValue::Num(stage as f32))
            }
            // GetStageDone <quest> <stage>
            0x103b => {
                let Some(target) = quest_target(0) else {
                    return self.decline(cmd);
                };
                let stage = num(1) as u16;
                let done = self
                    .world
                    .try_resource::<QuestStageState>()
                    .map(|s| s.get_stage_done(QuestFormId(target), stage))
                    .unwrap_or(false);
                Some(ObScriptValue::Num(done as i32 as f32))
            }
            // GetQuestRunning <quest>
            0x1038 => {
                let Some(target) = quest_target(0) else {
                    return self.decline(cmd);
                };
                let running = self
                    .world
                    .try_resource::<QuestStageState>()
                    .map(|s| s.is_running(QuestFormId(target)))
                    .unwrap_or(false);
                Some(ObScriptValue::Num(running as i32 as f32))
            }
            // Message / MessageBox arrive through `message` once decoded; a
            // payload the VM could not decode lands here and is logged.
            // `ShowMessage <message form>` (Fallout): the effect runtime shows the record.
            0x1059 if matches!(args.first(), Some(ObScriptValue::Ref(form)) if *form != 0) && crate::ScriptGame::is_fallout3_nv(self.world) => {
                match crate::obscript_effects::run(
                    self.world,
                    cmd,
                    args,
                    &crate::obscript_actions::Context { caller, known: None, action_ref: None, owner: self.owner, elapsed: self.elapsed },
                ) {
                    Action::Done { .. } => None,
                    _ => self.decline(cmd),
                }
            }
            // #6015 — Oblivion-only id (no FO3/FNV alignment), declined there.
            0x1000 if crate::ScriptGame::is_fallout3_nv(self.world) => self.decline(cmd),
            0x1059 | 0x1000 => {
                effect(
                    self,
                    format!(
                        "{1:#06x} fired with {0} arg(s)",
                        args.len(),
                        if cmd == 0x1059 { 0x1059 } else { 0x1000 },
                    ),
                );
                // MessageBox returns the pressed button; no menu this phase.
                Some(ObScriptValue::Num(-1.0))
            }
            // GetSecondsPassed
            0x100c => Some(ObScriptValue::Num(self.elapsed)),
            // GetButtonPressed — the last answered box, once (M47.3).
            0x101f => Some(ObScriptValue::Num(crate::messages::take_button(self.world) as f32)),
            // GetSelf — the script owner's form id.
            // `GetContainer`: "the container the object is in" — the holder while an item's
            // script runs, nothing for a reference lying in the world.
            0x10cf => Some(ObScriptValue::Ref(self.item.map_or(0, |item| item.container))),
            // `RemoveMe [target container]`: take the item out of the container it is in
            // (to the target, if one is named) and end the script.
            0x10a5 if self.item.is_some() => {
                let item = self.item.expect("checked above");
                let destination = match args.first() {
                    Some(ObScriptValue::Ref(form)) if *form != 0 => resolve_entity_by_global_form_id(self.world, *form),
                    _ => None,
                };
                let Some(holder) = resolve_entity_by_global_form_id(self.world, item.container) else {
                    return self.decline(cmd);
                };
                crate::fragment::remove_items(self.world, holder, item.item, 1, destination);
                self.ends.set(true);
                None
            }
            // `DropMe`: "drops the calling item from its container ... The dropped item will appear
            // next to the container or the owner" (GECK) — taken out of the holder, placed in the world
            // at the holder, and, like `RemoveMe`, it ends the script (the item is gone from the
            // inventory it ran in). Without a holder it does nothing.
            0x10a6 if self.item.is_some() => {
                let item = self.item.expect("checked above");
                let Some(holder) = resolve_entity_by_global_form_id(self.world, item.container) else {
                    return self.decline(cmd);
                };
                crate::fragment::remove_items(self.world, holder, item.item, 1, None);
                let ids = self.world.try_resource::<crate::CreatedReferences>().map(|created| created.scripted_ids());
                if let Some(form) = ids.and_then(|ids| ids.next()) {
                    if let Some(mut created) = self.world.try_resource_mut::<crate::CreatedReferences>() {
                        created.create_scripted(form, item.item, item.container, false);
                    }
                }
                self.ends.set(true);
                None
            }
            0x10ce => Some(ObScriptValue::Ref(self.owner)),
            // GetActionRef / IsActionRef — the reference that activated the
            // owner, inside the event block that has one; a null reference
            // (and false) everywhere else.
            0x10cd => Some(ObScriptValue::Ref(self.action_ref.unwrap_or(0))),
            0x1069 => {
                let asked = ref_arg(0).unwrap_or(0);
                Some(ObScriptValue::Num((asked != 0 && Some(asked) == self.action_ref) as i32 as f32))
            }
            // GetGlobalValue-style global reads arrive via the ' G<n>' escape
            // as CMD_GET_GLOBAL; direct numeric global reads by form id:
            crate::obscript_vm::CMD_GET_GLOBAL => {
                let form = ref_arg(0).unwrap_or(0);
                let value = self
                    .world
                    .try_resource::<Globals>()
                    .and_then(|g| g.get(form))
                    .unwrap_or(0.0);
                Some(ObScriptValue::Num(value))
            }
            // The VM declined a statement it could not model (unresolved
            // explicit caller, remote-variable `Set`): count it with the
            // other unmodeled commands under the id it carries.
            crate::obscript_vm::CMD_DECLINED => {
                let declined = args.first().map_or(0.0, |v| v.as_num()) as u16;
                self.decline(declined);
                None
            }
            // `Quest.var` read / `set Quest.var to ...`: a variable in another script.
            crate::obscript_vm::CMD_GET_REMOTE_VARIABLE => {
                match self.remote_variable(ref_arg(0).unwrap_or(0), num(1) as u16, None) {
                    Some(value) => Some(ObScriptValue::Num(value)),
                    None => self.decline(cmd),
                }
            }
            crate::obscript_vm::CMD_SET_REMOTE_VARIABLE => {
                if self.remote_variable(ref_arg(0).unwrap_or(0), num(1) as u16, Some(num(2))).is_none() {
                    self.decline(cmd);
                }
                None
            }
            crate::obscript_vm::CMD_SET_GLOBAL => {
                let form = caller.unwrap_or(0);
                let value = num(0);
                if let Some(mut globals) = self.world.try_resource_mut::<Globals>() {
                    globals.set(form, value);
                }
                None
            }
            // CompleteQuest <quest> — `QuestStageState::complete`, as Papyrus's
            // `Quest.CompleteQuest()` does (a command in all three games).
            CMD_COMPLETE_QUEST => {
                let Some(target) = quest_target(0) else {
                    return self.decline(cmd);
                };
                if let Some(mut stages) = self.world.try_resource_mut::<QuestStageState>() {
                    stages.complete(QuestFormId(target));
                }
                effect(self, format!("CompleteQuest {:08X}", target));
                None
            }
            // The FO3/FNV quest commands. Their ids exist only in that opcode
            // table (found by source alignment on FalloutNV.esm and
            // Fallout3.esm), so another game's number is never read as one.
            CMD_SET_OBJECTIVE_DISPLAYED | CMD_SET_OBJECTIVE_COMPLETED
            | CMD_GET_OBJECTIVE_DISPLAYED | CMD_GET_OBJECTIVE_COMPLETED
            | CMD_SET_QUEST_DELAY | CMD_GET_QUEST_COMPLETED
                if crate::ScriptGame::is_fallout3_nv(self.world) =>
            {
                let Some(target) = quest_target(0) else {
                    return self.decline(cmd);
                };
                self.fallout_quest_command(cmd, QuestFormId(target), args)
            }
            // The commands `obscript_more` models, per game: the opcode numbers overlap between the
            // Oblivion and Fallout tables, so it is asked with the loaded game (it precedes the
            // getters so an Oblivion `PurgeCellBuffers` is not read as Fallout's `GetLinkedRef`).
            _ if crate::obscript_more::models(cmd, crate::ScriptGame::is_fallout3_nv(self.world)) => {
                match crate::obscript_more::run(
                    self.world,
                    cmd,
                    args,
                    &crate::obscript_actions::Context {
                        caller,
                        known: self.reference.map(|entity| (self.owner, entity)),
                        action_ref: self.action_ref,
                        owner: self.owner,
                        elapsed: self.elapsed,
                    },
                ) {
                    Action::Done { value, effect: text } => {
                        if !text.is_empty() {
                            effect(self, text);
                        }
                        value
                    }
                    _ => self.decline(cmd),
                }
            }
            _ if crate::obscript_effects::is_getter(cmd) => {
                match crate::obscript_effects::getter(
                    self.world,
                    cmd,
                    args,
                    &crate::obscript_actions::Context {
                        caller,
                        known: self.reference.map(|entity| (self.owner, entity)),
                        action_ref: self.action_ref,
                        owner: self.owner,
                        elapsed: self.elapsed,
                    },
                ) {
                    crate::obscript_effects::Getter::Value(value) => Some(ObScriptValue::Num(value)),
                    crate::obscript_effects::Getter::Ref(form) => Some(ObScriptValue::Ref(form)),
                    _ => self.decline(cmd),
                }
            }
            _ if self.script_getter(cmd).is_some() => {
                let function_index = u32::from(cmd) - 0x1000;
                // A form-id first parameter that did not resolve (`Ref(0)`) is
                // not a question the evaluator can answer.
                let unresolved = ConditionFunction::from_index(function_index).first_param_is_form_id()
                    && matches!(args.first(), Some(ObScriptValue::Ref(0)));
                let param = |i: usize| match args.get(i) {
                    Some(ObScriptValue::Ref(form)) => *form,
                    Some(ObScriptValue::Num(n)) => *n as u32,
                    None => 0,
                };
                let subject = caller
                    .and_then(|form| self.entity_of(form))
                    .or_else(|| {
                        // The reputation getters are about the player (GECK
                        // *GetReputationThreshold*: "Returns 1 if the player is
                        // currently on the good axis with the ... faction"), so
                        // written with no reference they ask about the player.
                        // Every other getter has no such default.
                        matches!(
                            ConditionFunction::from_index(function_index),
                            ConditionFunction::GetReputation | ConditionFunction::GetReputationThreshold
                        )
                        .then(|| crate::papyrus_demo::PapyrusPlayerEntity::get(self.world))
                        .flatten()
                    });
                let answered = if unresolved {
                    None
                } else {
                    evaluate_script_getter(self.world, function_index, param(0), param(1), subject)
                };
                match answered {
                    Some(value) => Some(ObScriptValue::Num(value)),
                    None => self.decline(cmd),
                }
            }
            _ => match crate::obscript_actions::run(
                self.world,
                cmd,
                args,
                &crate::obscript_actions::Context {
                    caller,
                    known: self.reference.map(|entity| (self.owner, entity)),
                    action_ref: self.action_ref,
                    owner: self.owner,
                    elapsed: self.elapsed,
                },
            ) {
                Action::Done { value, effect: text } => {
                    if !text.is_empty() {
                        effect(self, text);
                    }
                    value
                }
                Action::Declined => self.decline(cmd),
                Action::Unhandled => {
                    // Traced no-op. The census counter keeps phase-1 honest about
                    // coverage instead of silently faking behavior.
                    log::trace!(
                        "ObScript {:08X}: unknown command {cmd:#06x} ({} args) — no-op",
                        self.owner,
                        args.len(),
                    );
                    self.decline(cmd)
                }
            },
        }
    }
}

impl ObScriptCommandHost<'_> {
    /// The entity of reference `form`: the script's own when it is that one,
    /// else a scan of the placed references.
    fn entity_of(&self, form: u32) -> Option<byroredux_core::ecs::storage::EntityId> {
        match self.reference {
            Some(entity) if form == self.owner => Some(entity),
            _ => resolve_entity_by_global_form_id(self.world, form),
        }
    }

    /// `SetObjectiveDisplayed/Completed`, `GetObjectiveDisplayed/Completed`,
    /// `SetQuestDelay` and `GetQuestCompleted` against `quest`, with the GECK
    /// semantics (*SetObjectiveDisplayed*, *SetObjectiveCompleted*,
    /// *SetQuestDelay*, *GetQuestCompleted* pages). Arguments follow the quest
    /// reference: `<index> <flag>`, `<index>`, or `<seconds>`.
    fn fallout_quest_command(
        &self,
        cmd: u16,
        quest: QuestFormId,
        args: &[ObScriptValue],
    ) -> Option<ObScriptValue> {
        let num = |i: usize| args.get(i).map(|v| v.as_num()).unwrap_or(0.0);
        let objective = num(1) as i32;
        let log = |text: String| {
            if let Some(mut log) = self.world.try_resource_mut::<ObScriptEffectLog>() {
                log.push(format!("{:08X}", self.owner), text);
            }
        };
        match cmd {
            CMD_SET_OBJECTIVE_DISPLAYED | CMD_SET_OBJECTIVE_COMPLETED => {
                if !self.objective_is_defined(quest, objective) {
                    // "only accepts indices that are defined in the quest's
                    // Quest Objectives tab ... nothing will happen."
                    log::debug!("quest {:08X}: objective {objective} of {:08X} is not authored", self.owner, quest.0);
                    return None;
                }
                let on = num(2) != 0.0;
                if cmd == CMD_SET_OBJECTIVE_DISPLAYED {
                    if on {
                        // "If the quest is not currently running,
                        // SetObjectiveDisplayed ... 1 will automatically start it."
                        // The definitions are read first and released: the quest
                        // systems take `QuestDefinitionRegistry` before
                        // `QuestStageState`, so nesting it under the stage lock
                        // would add the opposite edge to the lock-order graph.
                        let start_up = self
                            .world
                            .try_resource::<crate::quest_stages::QuestDefinitionRegistry>()
                            .and_then(|definitions| definitions.start_up_stage(quest));
                        if let Some(mut stages) = self.world.try_resource_mut::<QuestStageState>() {
                            if !stages.is_running(quest) {
                                stages.start_quest(quest, start_up);
                            }
                        }
                    }
                    if let Some(mut objectives) = self.world.try_resource_mut::<QuestObjectiveState>() {
                        objectives.set_displayed(quest, objective, on);
                    }
                    log(format!("SetObjectiveDisplayed {:08X} {objective} {}", quest.0, on as u8));
                } else {
                    if let Some(mut objectives) = self.world.try_resource_mut::<QuestObjectiveState>() {
                        objectives.set_completed(quest, objective, on);
                    }
                    log(format!("SetObjectiveCompleted {:08X} {objective} {}", quest.0, on as u8));
                }
                None
            }
            CMD_GET_OBJECTIVE_DISPLAYED | CMD_GET_OBJECTIVE_COMPLETED => {
                let status = self
                    .world
                    .try_resource::<QuestObjectiveState>()
                    .map(|objectives| objectives.get(quest, objective))
                    .unwrap_or_default();
                let flag = if cmd == CMD_GET_OBJECTIVE_DISPLAYED { status.displayed } else { status.completed };
                Some(ObScriptValue::Num(flag as i32 as f32))
            }
            CMD_SET_QUEST_DELAY => {
                let seconds = num(1);
                if let Some(mut locals) = self.world.try_resource_mut::<ObScriptQuestLocals>() {
                    locals.set_delay(quest.0, seconds);
                }
                log(format!("SetQuestDelay {:08X} {seconds}", quest.0));
                None
            }
            // "does not require the quest to be running"
            _ => Some(ObScriptValue::Num(
                self.world
                    .try_resource::<QuestStageState>()
                    .is_some_and(|stages| stages.is_completed(quest)) as i32 as f32,
            )),
        }
    }

    /// Whether `objective` is one `quest` authors. A quest the definitions do
    /// not list, or one with no listed objectives, is given the benefit of the
    /// doubt: the registry being empty is not evidence the index is undefined.
    fn objective_is_defined(&self, quest: QuestFormId, objective: i32) -> bool {
        let Some(definitions) = self.world.try_resource::<crate::quest_stages::QuestDefinitionRegistry>() else {
            return true;
        };
        let authored = definitions.objectives(quest);
        authored.is_empty() || authored.contains(&objective)
    }

    /// A variable of another script, by its ordinal in that script's local table (what a
    /// compiled `Quest.var` carries): the quest's, or the one of the reference's own object
    /// script. Read it, or — with `write` — set it and answer the value written. `None`
    /// when the script is not one this host holds the variables of: an unknown form, a
    /// reference that is not loaded or has no object script, or the script running now (its
    /// own variables are mid-run in the VM, and a second copy here would be overwritten when
    /// the run stores them).
    fn remote_variable(&self, form: u32, ordinal: u16, write: Option<f32>) -> Option<f32> {
        if form == 0 || form == self.owner {
            return None;
        }
        if self.world.try_resource::<QuestScriptTable>().is_some_and(|table| table.scripts.contains_key(&form)) {
            let mut locals = self.world.try_resource_mut::<ObScriptQuestLocals>()?;
            return Some(match write {
                Some(value) => {
                    locals.set(form, ordinal, value);
                    value
                }
                None => locals.get(form, ordinal),
            });
        }
        let entity = resolve_entity_by_global_form_id(self.world, form)?;
        let script_id = self.world.get::<crate::obscript_objects::ObScriptInstance>(entity)?.script;
        let name = {
            let table = self.world.try_resource::<crate::obscript_objects::ObScriptObjectTable>()?;
            let record = &table.scripts.get(&script_id)?.record;
            record.locals.iter().find(|local| local.index == u32::from(ordinal))?.name.clone()
        };
        match write {
            Some(value) => {
                let mut variables = self.world.query_mut::<crate::vm_state::ScriptVariables>()?;
                variables.get_mut(entity)?.set_by_name(&name, value);
                Some(value)
            }
            None => Some(
                self.world
                    .get::<crate::vm_state::ScriptVariables>(entity)?
                    .get(byroredux_plugin::esm::records::condition::ConditionStringId::from_text(&name))
                    .unwrap_or(0.0),
            ),
        }
    }

    /// The condition function `cmd` names when it is one of [`SCRIPT_GETTERS`].
    fn script_getter(&self, cmd: u16) -> Option<ConditionFunction> {
        let index = u32::from(cmd).checked_sub(0x1000)?;
        let function = ConditionFunction::from_index(index);
        SCRIPT_GETTERS.contains(&function).then_some(function)
    }

    /// Record `cmd` as not executed — an unknown command, or a known one
    /// whose reference could not be resolved — under the per-quest census,
    /// and answer like an unknown command (value 0). Never faked.
    fn decline(&self, cmd: u16) -> Option<ObScriptValue> {
        self.declined.set(self.declined.get() + 1);
        if let Some(mut timers) = self.world.try_resource_mut::<ObScriptQuestTimers>() {
            *timers
                .unknown_commands
                .entry(self.owner)
                .or_default()
                .entry(cmd)
                .or_insert(0) += 1;
        }
        Some(ObScriptValue::Num(0.0))
    }
}

/// Run one ObScript command through the real host, as a script statement would — `caller` the reference
/// it is called on (an object script's implicit one), `args` its decoded arguments. For the console
/// (`obscript.cmd`): the answer the command gave (`None` for a command with no answer or one the host
/// declined) and how many commands the host declined.
pub fn run_console_command(world: &World, caller: Option<u32>, cmd: u16, args: &[ObScriptValue]) -> (Option<ObScriptValue>, u32) {
    let script = ScriptRecord::default();
    let reference = caller.and_then(|form| resolve_entity_by_global_form_id(world, form));
    let mut host = ObScriptCommandHost::new(world, caller.unwrap_or(0), reference, 0.0, &script);
    let answer = host.call(cmd, args, caller);
    (answer, host.declined.get())
}

/// The running quests by FormID. `QuestStageState` iterates in `HashMap` order
/// (random per process), and quests that share a tick share global slots
/// (`GetButtonPressed`) and see each other's `SetStage`, so the run order must
/// not follow it (#6013).
fn running_quests_in_order(stages: &QuestStageState) -> Vec<u32> {
    let mut running: Vec<u32> = stages
        .iter()
        .filter(|(_, data)| data.status == QuestStatus::Running)
        .map(|(id, _)| id.0)
        .collect();
    running.sort_unstable();
    running
}

/// Advance every running quest's script on the vanilla cadence. Exclusive
/// in `Stage::Update`, beside `ambient_ai_package_system` — it reads the
/// quest resources and runs pure interpreter work, so exclusivity costs
/// nothing and keeps the timer maps race-free.
pub fn obscript_quest_tick_system(world: &World, dt: f32) {
    let running: Vec<u32> = match world.try_resource::<QuestStageState>() {
        Some(stages) => running_quests_in_order(&stages),
        None => return,
    };
    if running.is_empty() {
        return;
    }
    let table = world.try_resource::<QuestScriptTable>();
    let Some(table) = table else { return };
    // Each running script's interval, read before the timers lock is taken
    // (`SetQuestDelay` lives in the locals resource; holding both at once
    // would add a lock-order edge for nothing).
    let intervals: HashMap<u32, f32> = {
        let locals = world.try_resource::<ObScriptQuestLocals>();
        running
            .iter()
            .filter_map(|id| {
                let authored = table.scripts.get(id)?.authored_delay;
                let runtime = locals.as_ref().and_then(|l| l.delay_override(*id));
                Some((*id, quest_script_interval(authored, runtime)))
            })
            .collect()
    };
    let due: Vec<(u32, f32)> = {
        let Some(mut timers) = world.try_resource_mut::<ObScriptQuestTimers>() else {
            return;
        };
        running
            .iter()
            .filter_map(|id| {
                let interval = *intervals.get(id)?;
                let slot = timers.elapsed.entry(*id).or_insert(0.0);
                *slot += dt;
                if *slot >= interval {
                    let elapsed = *slot;
                    *slot = 0.0;
                    Some((*id, elapsed))
                } else {
                    None
                }
            })
            .collect()
    };
    drop(table);
    for (id, elapsed) in due {
        let outcome = run_quest_game_mode(world, id, elapsed);
        if let Some(mut timers) = world.try_resource_mut::<ObScriptQuestTimers>() {
            *timers.runs.entry(id).or_insert(0) += 1;
        }
        if let Some(BlockOutcome::Malformed(at)) = outcome {
            log::warn!("ObScript quest {id:08X}: malformed bytecode at {at:#06x}");
        }
    }
}

/// Host-level behaviour of [`ObScriptCommandHost`] on synthetic calls.
#[cfg(test)]
mod host_tests {
    use super::*;
    use crate::obscript_vm::{ObScriptHost, ObScriptValue, CMD_DECLINED};

    const OWN: u32 = 0x00AA_0001;
    const OTHER: u32 = 0x00BB_0002;
    const SET_STAGE: u16 = 0x1039;
    const START_QUEST: u16 = 0x1036;

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(QuestStageState::default());
        world.insert_resource(ObScriptQuestTimers::default());
        world
    }

    fn call(
        world: &World,
        cmd: u16,
        args: &[ObScriptValue],
    ) -> Option<ObScriptValue> {
        let script = ScriptRecord::default();
        let mut host = ObScriptCommandHost::new(world, OWN, None, 0.0, &script);
        host.call(cmd, args, None)
    }

    fn declined(world: &World, cmd: u16) -> u32 {
        world
            .try_resource::<ObScriptQuestTimers>()
            .unwrap()
            .unknown_commands
            .get(&OWN)
            .and_then(|commands| commands.get(&cmd))
            .copied()
            .unwrap_or(0)
    }

    /// #5629: a quest argument that IS written but is null (the VM's
    /// `Ref(0)` for an unresolved slot or the `Z` tag) has no quest to act
    /// on. It used to stage form 0 — or, once absent-defaulting is applied
    /// carelessly, the script's own quest. It is declined and counted.
    #[test]
    fn a_null_quest_reference_declines_instead_of_staging_form_zero_or_the_owner() {
        let world = world();
        for cmd in [SET_STAGE, START_QUEST, 0x1037, 0x103a, 0x103b, 0x1038] {
            let args = [ObScriptValue::Ref(0), ObScriptValue::Num(10.0)];
            assert_eq!(
                call(&world, cmd, &args),
                Some(ObScriptValue::Num(0.0)),
                "{cmd:#06x} answers like an unknown command"
            );
            assert_eq!(declined(&world, cmd), 1, "{cmd:#06x} counted");
        }
        let stages = world.try_resource::<QuestStageState>().unwrap();
        assert!(!stages.is_started(QuestFormId(0)), "form 0 never staged");
        assert!(!stages.is_started(QuestFormId(OWN)), "own quest never staged");
    }

    /// #6015 — `0x1000` (`MessageBox`) is an Oblivion id with no Fallout alignment: a
    /// Fallout host declines (and counts) it, whether it arrives as a decoded box or
    /// as a bare call, while Oblivion still answers the MessageBox "no button" value.
    #[test]
    fn message_box_id_is_declined_on_fallout_and_runs_on_oblivion() {
        use byroredux_plugin::esm::reader::GameKind;
        let boxed = || crate::obscript_vm::ObScriptMessage {
            text: "hi".into(),
            values: Vec::new(),
            buttons: Vec::new(),
            message_box: true,
        };
        let script = ScriptRecord::default();

        let mut fallout = world();
        fallout.insert_resource(crate::ScriptGame(GameKind::Fallout3NV));
        assert_eq!(call(&fallout, 0x1000, &[]), Some(ObScriptValue::Num(0.0)));
        ObScriptCommandHost::new(&fallout, OWN, None, 0.0, &script).message(boxed());
        assert_eq!(declined(&fallout, 0x1000), 2, "call and decoded box both counted");

        let mut oblivion = world();
        oblivion.insert_resource(crate::ScriptGame(GameKind::Oblivion));
        assert_eq!(call(&oblivion, 0x1000, &[]), Some(ObScriptValue::Num(-1.0)));
        assert_eq!(declined(&oblivion, 0x1000), 0);
    }

    /// The decline must not disturb the resolved and defaulted forms.
    #[test]
    fn a_resolved_quest_still_stages_and_an_absent_one_defaults_to_the_owner() {
        let world = world();
        call(
            &world,
            SET_STAGE,
            &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(10.0)],
        );
        call(&world, START_QUEST, &[]); // no argument written → own quest
        let stages = world.try_resource::<QuestStageState>().unwrap();
        assert_eq!(stages.get_stage(QuestFormId(OTHER)), 10);
        assert!(stages.is_started(QuestFormId(OWN)));
        assert_eq!(declined(&world, SET_STAGE), 0);
        assert_eq!(declined(&world, START_QUEST), 0);
    }

    /// `<ref>.Cmd` — the explicit-caller form, which carries the caller's form id.
    fn call_on(world: &World, cmd: u16, args: &[ObScriptValue], caller: u32) -> Option<ObScriptValue> {
        let script = ScriptRecord::default();
        let mut host = ObScriptCommandHost::new(world, OWN, None, 0.0, &script);
        host.call(cmd, args, Some(caller))
    }

    const GET_DEAD: u16 = 0x102e;
    const GET_IN_CELL: u16 = 0x1043;
    const GET_ACTOR_VALUE: u16 = 0x100e;
    const ACHR: u32 = 0x0001_4D8A;

    /// A world with one placed actor, `ACHR`, resolvable by its form id.
    fn world_with_actor() -> (World, byroredux_core::ecs::storage::EntityId) {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        let mut world = world();
        let mut pool = FormIdPool::new();
        let id = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(ACHR),
        });
        world.insert_resource(pool);
        let actor = world.spawn();
        world.insert(actor, FormIdComponent(id));
        (world, actor)
    }

    /// M47.3 phase 2: a getter command is the CTDA function of the same index
    /// (`0x1000 + 46` = `GetDead`), so the host answers it from the engine's one
    /// condition evaluator instead of declining it.
    #[test]
    fn a_getter_command_is_answered_by_the_condition_evaluator() {
        let (mut world, actor) = world_with_actor();
        assert_eq!(call_on(&world, GET_DEAD, &[], ACHR), Some(ObScriptValue::Num(0.0)));
        world.insert(actor, byroredux_core::ecs::components::Dead);
        assert_eq!(call_on(&world, GET_DEAD, &[], ACHR), Some(ObScriptValue::Num(1.0)));
        assert_eq!(declined(&world, GET_DEAD), 0, "answered, not declined");
    }

    /// `GetActionRef` / `IsActionRef` answer only inside the event block that has
    /// an actor ("It will only return a value during the frame after the object
    /// has been interacted with").
    #[test]
    fn the_action_ref_commands_answer_only_while_a_block_has_an_activator() {
        const GET_ACTION_REF: u16 = 0x10cd;
        const IS_ACTION_REF: u16 = 0x1069;
        let world = world();
        let script = ScriptRecord::default();
        let mut host = ObScriptCommandHost::new(&world, OWN, None, 0.0, &script);
        assert_eq!(host.call(GET_ACTION_REF, &[], None), Some(ObScriptValue::Ref(0)));
        assert_eq!(host.call(IS_ACTION_REF, &[ObScriptValue::Ref(0x14)], None), Some(ObScriptValue::Num(0.0)));
        host.action_ref = Some(0x14);
        assert_eq!(host.call(GET_ACTION_REF, &[], None), Some(ObScriptValue::Ref(0x14)));
        assert_eq!(host.call(IS_ACTION_REF, &[ObScriptValue::Ref(0x14)], None), Some(ObScriptValue::Num(1.0)));
        assert_eq!(host.call(IS_ACTION_REF, &[ObScriptValue::Ref(0x15)], None), Some(ObScriptValue::Num(0.0)));
        assert_eq!(host.call(IS_ACTION_REF, &[ObScriptValue::Ref(0)], None), Some(ObScriptValue::Num(0.0)), "a null ref is nobody");
        assert_eq!(host.declined.get(), 0);
    }

    /// M47.3 phase 2 actions go through the host: a done action is logged and
    /// answers its value, one that cannot be carried out is a counted decline.
    #[test]
    fn a_reference_bound_action_runs_or_is_declined_and_counted() {
        const DISABLE: u16 = 0x1022;
        let (mut world, _) = world_with_actor();
        world.insert_resource(ObScriptEffectLog::default());
        world.insert_resource(crate::ReferenceEnableState::default());
        assert_eq!(call_on(&world, DISABLE, &[], ACHR), None);
        assert!(!crate::reference_enabled(&world, ACHR));
        assert_eq!(declined(&world, DISABLE), 0);
        assert_eq!(
            world.try_resource::<ObScriptEffectLog>().unwrap().entries.back().map(|(_, text)| text.as_str()),
            Some("Disable 00014D8A"),
        );
        // Written on the quest itself there is no reference to disable.
        assert_eq!(call(&world, DISABLE, &[]), Some(ObScriptValue::Num(0.0)));
        assert_eq!(declined(&world, DISABLE), 1);
    }

    /// A remote variable names a variable of another script by its ordinal in that script's
    /// local table: a quest's are its saved locals, so `set Q.var to 3` in one script is what
    /// `Q`'s own script, and `GetQuestVariable`, read next.
    #[test]
    fn a_remote_variable_reads_and_writes_the_quests_locals() {
        const READ: u16 = crate::obscript_vm::CMD_GET_REMOTE_VARIABLE;
        const WRITE: u16 = crate::obscript_vm::CMD_SET_REMOTE_VARIABLE;
        let mut world = world();
        world.insert_resource(ObScriptQuestLocals::default());
        let mut table = QuestScriptTable::default();
        table.scripts.insert(
            OTHER,
            QuestScript { script: Arc::new(ScriptRecord::default()), quest: QuestFormId(OTHER), authored_delay: 0.0 },
        );
        world.insert_resource(table);
        let quest = ObScriptValue::Ref(OTHER);
        assert_eq!(call(&world, READ, &[quest, ObScriptValue::Num(6.0)]), Some(ObScriptValue::Num(0.0)), "never assigned reads 0");
        assert_eq!(declined(&world, READ), 0);
        call(&world, WRITE, &[quest, ObScriptValue::Num(6.0), ObScriptValue::Num(3.0)]);
        assert_eq!(call(&world, READ, &[quest, ObScriptValue::Num(6.0)]), Some(ObScriptValue::Num(3.0)));
        assert_eq!(world.try_resource::<ObScriptQuestLocals>().unwrap().get(OTHER, 6), 3.0, "the quest's own slot");
        // A form whose variables this host does not hold is a counted decline, never a made-up 0.
        call(&world, READ, &[ObScriptValue::Ref(0x00FF_FFFF), ObScriptValue::Num(1.0)]);
        call(&world, WRITE, &[ObScriptValue::Ref(0x00FF_FFFF), ObScriptValue::Num(1.0), ObScriptValue::Num(1.0)]);
        assert_eq!((declined(&world, READ), declined(&world, WRITE)), (1, 1));
    }

    /// A caller that is not a live reference cannot be asked: it is a counted
    /// decline, never an invented "alive".
    #[test]
    fn a_getter_on_an_unloaded_reference_is_declined_and_counted() {
        let (world, _) = world_with_actor();
        assert_eq!(call_on(&world, GET_DEAD, &[], 0x00FF_FFFF), Some(ObScriptValue::Num(0.0)));
        assert_eq!(declined(&world, GET_DEAD), 1);
        // No caller at all: the owning quest is not an actor.
        assert_eq!(call(&world, GET_DEAD, &[]), Some(ObScriptValue::Num(0.0)));
        assert_eq!(declined(&world, GET_DEAD), 2);
    }

    /// An unresolved form-id argument (`Ref(0)`) is declined, not evaluated as form 0.
    #[test]
    fn a_getter_with_a_null_form_argument_is_declined() {
        let (world, _) = world_with_actor();
        call_on(&world, GET_IN_CELL, &[ObScriptValue::Ref(0)], ACHR);
        assert_eq!(declined(&world, GET_IN_CELL), 1);
    }

    /// A function the evaluator cannot read in this world stays declined:
    /// `GetActorValue`'s authored argument is an enum code on the legacy games
    /// (#5989), which would be misread as a FormID.
    #[test]
    fn get_actor_value_stays_declined_while_its_argument_is_an_enum_code() {
        let (mut world, _) = world_with_actor();
        world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Fallout3NV));
        call_on(&world, GET_ACTOR_VALUE, &[ObScriptValue::Num(16.0)], ACHR);
        assert_eq!(declined(&world, GET_ACTOR_VALUE), 1);
    }

    /// A reputation getter written with no reference asks about the player.
    #[test]
    fn a_reputation_getter_with_no_caller_asks_about_the_player() {
        let mut world = world_with_actor().0;
        // No player entity yet: nothing to ask, so it is declined.
        call(&world, 0x123f, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(1.0)]);
        assert_eq!(declined(&world, 0x123f), 1);
        let player = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Fallout3NV));
        call(&world, 0x123f, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(1.0)]);
        assert_eq!(declined(&world, 0x123f), 1, "answered for the player, no new decline");
        // A getter with no such default still needs its reference.
        call(&world, GET_DEAD, &[]);
        assert_eq!(declined(&world, GET_DEAD), 1);
    }

    /// A command that is not on the getter list is never read as a CTDA
    /// function, even when its number is a catalogued index: `0x1021` (Enable,
    /// index 33) must not become a getter.
    #[test]
    fn a_statement_command_is_not_mistaken_for_a_getter() {
        let (world, _) = world_with_actor();
        call_on(&world, 0x1021, &[], ACHR);
        assert_eq!(declined(&world, 0x1021), 1);
    }

    fn fnv_world() -> World {
        let mut world = world();
        world.insert_resource(QuestObjectiveState::default());
        world.insert_resource(ObScriptQuestLocals::default());
        world.insert_resource(ObScriptEffectLog::default());
        world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Fallout3NV));
        world
    }

    fn objective(world: &World, quest: u32, index: i32) -> crate::quest_stages::ObjectiveStatus {
        world.try_resource::<QuestObjectiveState>().unwrap().get(QuestFormId(quest), index)
    }

    /// GECK *SetObjectiveDisplayed*: turns an objective on or off, and "if the
    /// quest is not currently running ... 1 will automatically start it".
    #[test]
    fn set_objective_displayed_flips_the_flag_and_starts_a_stopped_quest() {
        let world = fnv_world();
        let args = |flag: f32| [ObScriptValue::Ref(OTHER), ObScriptValue::Num(10.0), ObScriptValue::Num(flag)];
        assert_eq!(call(&world, CMD_SET_OBJECTIVE_DISPLAYED, &args(1.0)), None);
        assert!(objective(&world, OTHER, 10).displayed);
        assert!(world.try_resource::<QuestStageState>().unwrap().is_running(QuestFormId(OTHER)));
        call(&world, CMD_SET_OBJECTIVE_DISPLAYED, &args(0.0));
        assert!(!objective(&world, OTHER, 10).displayed);
        assert_eq!(declined(&world, CMD_SET_OBJECTIVE_DISPLAYED), 0);
    }

    /// Hiding an objective must not start a quest.
    #[test]
    fn hiding_an_objective_does_not_start_the_quest() {
        let world = fnv_world();
        call(&world, CMD_SET_OBJECTIVE_DISPLAYED, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(10.0), ObScriptValue::Num(0.0)]);
        assert!(!world.try_resource::<QuestStageState>().unwrap().is_started(QuestFormId(OTHER)));
    }

    /// GECK *GetObjectiveDisplayed* / *GetObjectiveCompleted* read the two flags
    /// independently ("regardless of whether the objective ... has been marked as
    /// completed").
    #[test]
    fn objective_getters_read_the_two_flags_independently() {
        let world = fnv_world();
        let at = |cmd| call(&world, cmd, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(20.0)]);
        assert_eq!(at(CMD_GET_OBJECTIVE_DISPLAYED), Some(ObScriptValue::Num(0.0)));
        call(&world, CMD_SET_OBJECTIVE_DISPLAYED, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(20.0), ObScriptValue::Num(1.0)]);
        call(&world, CMD_SET_OBJECTIVE_COMPLETED, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(20.0), ObScriptValue::Num(1.0)]);
        assert_eq!(at(CMD_GET_OBJECTIVE_DISPLAYED), Some(ObScriptValue::Num(1.0)));
        assert_eq!(at(CMD_GET_OBJECTIVE_COMPLETED), Some(ObScriptValue::Num(1.0)));
        call(&world, CMD_SET_OBJECTIVE_COMPLETED, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(20.0), ObScriptValue::Num(0.0)]);
        assert_eq!(at(CMD_GET_OBJECTIVE_COMPLETED), Some(ObScriptValue::Num(0.0)));
        assert_eq!(at(CMD_GET_OBJECTIVE_DISPLAYED), Some(ObScriptValue::Num(1.0)));
    }

    /// An objective the quest does not author is ignored ("nothing will happen").
    #[test]
    fn an_unauthored_objective_index_is_ignored() {
        use byroredux_plugin::esm::records::QustRecord;
        let mut world = fnv_world();
        let mut quest = QustRecord { form_id: OTHER, ..Default::default() };
        quest.objectives.push(byroredux_plugin::esm::records::QuestObjective { index: 10, ..Default::default() });
        crate::quest_stages::register(&mut world);
        crate::quest_stages::install_start_game_quests(&mut world, [quest]);
        call(&world, CMD_SET_OBJECTIVE_DISPLAYED, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(99.0), ObScriptValue::Num(1.0)]);
        assert!(!objective(&world, OTHER, 99).displayed);
        assert!(!world.try_resource::<QuestStageState>().unwrap().is_started(QuestFormId(OTHER)), "an ignored call starts nothing");
        call(&world, CMD_SET_OBJECTIVE_DISPLAYED, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(10.0), ObScriptValue::Num(1.0)]);
        assert!(objective(&world, OTHER, 10).displayed);
    }

    /// The FO3/FNV ids are not read in another game's opcode table.
    #[test]
    fn fallout_quest_commands_are_declined_outside_fallout() {
        let mut world = fnv_world();
        world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Oblivion));
        call(&world, CMD_SET_OBJECTIVE_DISPLAYED, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(10.0), ObScriptValue::Num(1.0)]);
        assert!(!objective(&world, OTHER, 10).displayed);
        assert_eq!(declined(&world, CMD_SET_OBJECTIVE_DISPLAYED), 1);
    }

    /// GECK *GetQuestCompleted*: does not need the quest to be running.
    #[test]
    fn get_quest_completed_follows_the_completed_status() {
        let world = fnv_world();
        let at = || call(&world, CMD_GET_QUEST_COMPLETED, &[ObScriptValue::Ref(OTHER)]);
        assert_eq!(at(), Some(ObScriptValue::Num(0.0)));
        call(&world, START_QUEST, &[ObScriptValue::Ref(OTHER)]);
        call(&world, CMD_COMPLETE_QUEST, &[ObScriptValue::Ref(OTHER)]);
        assert_eq!(at(), Some(ObScriptValue::Num(1.0)));
        call(&world, 0x1037, &[ObScriptValue::Ref(OTHER)]);
        assert_eq!(at(), Some(ObScriptValue::Num(1.0)), "stopping does not undo completion");
    }

    /// GECK *SetQuestDelay*: "Setting delay to 0 tells the quest to process at
    /// the INI setting default (5 seconds)" — not the authored delay.
    #[test]
    fn set_quest_delay_overrides_the_authored_interval_and_zero_means_the_ini_default() {
        let world = fnv_world();
        let delay = || world.try_resource::<ObScriptQuestLocals>().unwrap().delay_override(OTHER);
        assert_eq!(delay(), None);
        call(&world, CMD_SET_QUEST_DELAY, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(0.1)]);
        assert!((delay().unwrap() - 0.1).abs() < 1e-6);
        assert!((quest_script_interval(7.0, delay()) - 0.1).abs() < 1e-6, "runtime beats authored");
        call(&world, CMD_SET_QUEST_DELAY, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(0.0)]);
        assert_eq!(quest_script_interval(7.0, delay()), QUEST_SCRIPT_INTERVAL_SECS);
        assert_eq!(quest_script_interval(0.5, None), 0.5, "no override: the authored delay");
        assert_eq!(quest_script_interval(0.0, None), QUEST_SCRIPT_INTERVAL_SECS, "unauthored: the default");
        assert_eq!(quest_script_interval(f32::NAN, None), QUEST_SCRIPT_INTERVAL_SECS);
    }

    /// `CompleteQuest` is a command in every legacy game, and goes through the
    /// same `QuestStageState::complete` Papyrus uses.
    #[test]
    fn complete_quest_completes_the_quest() {
        let world = world();
        call(&world, START_QUEST, &[ObScriptValue::Ref(OTHER)]);
        call(&world, CMD_COMPLETE_QUEST, &[ObScriptValue::Ref(OTHER)]);
        assert!(world.try_resource::<QuestStageState>().unwrap().is_completed(QuestFormId(OTHER)));
    }

    /// The VM's `CMD_DECLINED` is counted under the id it carries.
    #[test]
    fn the_vm_decline_command_is_counted_under_the_declined_id() {
        let world = world();
        assert_eq!(
            call(&world, CMD_DECLINED, &[ObScriptValue::Num(0x15 as f32)]),
            None
        );
        assert_eq!(declined(&world, 0x15), 1, "a remote-variable Set (0x15)");
    }
}

#[cfg(test)]
mod persisted_locals_tests {
    use super::*;
    use crate::obscript_vm::BLOCK_GAME_MODE;
    use byroredux_plugin::esm::records::{ScriptLocalVar, ScriptType};

    const QUEST: u32 = 0x0001_2345;

    fn statement(op: u16, payload: &[u8]) -> Vec<u8> {
        let mut bytes = op.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn set_short_local_to_incremented_value() -> Vec<u8> {
        // `set s1 to s1 + 1`: the first run reads the absent local as 0;
        // each later run must seed the VM from the value stored by the prior run.
        let expression = [b' ', b's', 1, 0, b' ', b'1', b' ', b'+'];
        let mut payload = vec![b's'];
        payload.extend_from_slice(&1u16.to_le_bytes());
        payload.extend_from_slice(&(expression.len() as u16).to_le_bytes());
        payload.extend_from_slice(&expression);
        let assignment = statement(0x15, &payload);

        let mut begin_payload = BLOCK_GAME_MODE.to_le_bytes().to_vec();
        begin_payload.extend_from_slice(&(assignment.len() as u32).to_le_bytes());
        let mut bytes = statement(0x10, &begin_payload);
        bytes.extend(assignment);
        bytes.extend(statement(0x11, &[]));
        bytes
    }

    #[test]
    fn quest_game_mode_local_is_read_on_the_next_tick_and_by_condition_79() {
        use byroredux_plugin::esm::records::condition::{
            ComparisonOp, Condition, ConditionValue, RunOn,
        };
        use byroredux_plugin::esm::reader::GameKind;

        let mut world = World::new();
        let script = ScriptRecord {
            form_id: 1,
            editor_id: "TestQuestScript".into(),
            script_type: ScriptType::Quest,
            compiled: set_short_local_to_incremented_value(),
            locals: vec![ScriptLocalVar {
                index: 1,
                var_type: 1,
                name: "counter".into(),
            }],
            ..Default::default()
        };
        let mut table = QuestScriptTable::default();
        table.scripts.insert(
            QUEST,
            QuestScript {
                script: Arc::new(script),
                quest: QuestFormId(QUEST),
                authored_delay: 0.0,
            },
        );
        world.insert_resource(table);
        world.insert_resource(ObScriptQuestLocals::default());
        world.insert_resource(crate::ScriptGame(GameKind::Oblivion));

        assert_eq!(
            run_quest_game_mode(&world, QUEST, 5.0),
            Some(BlockOutcome::Completed)
        );
        assert_eq!(
            run_quest_game_mode(&world, QUEST, 5.0),
            Some(BlockOutcome::Completed)
        );
        assert_eq!(
            world
                .try_resource::<ObScriptQuestLocals>()
                .unwrap()
                .get(QUEST, 1),
            2.0,
            "tick two must start from the slot saved by tick one"
        );

        let condition = Condition {
            function_index: 79,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(2.0),
            param_1: QUEST,
            param_2: 1,
            run_on: RunOn::Subject,
            ..Default::default()
        };
        let subject = world.spawn();
        assert!(crate::condition::evaluate_condition(
            &condition,
            &world,
            &crate::condition::ConditionContext::for_subject(subject),
        ));
    }
}

#[cfg(test)]
mod real_data_tests {
    use super::*;
    use crate::obscript_vm::{BLOCK_GAME_MODE, ObScriptHost, ObScriptProgram, ObScriptValue};

    /// Real-data harness: executes vanilla `Oblivion.esm` quest scripts
    /// through the interpreter with a recording host. Opt-in (`#[ignore]`)
    /// per the repo convention for on-disk game data.
    ///
    /// Run with:
    ///   cargo test -p byroredux-scripting --lib obscript_quests -- --ignored
    #[derive(Default)]
    struct Recorder {
        calls: Vec<(u16, Vec<ObScriptValue>)>,
    }

    impl ObScriptHost for Recorder {
        fn call(
            &mut self,
            cmd: u16,
            args: &[ObScriptValue],
            _caller: Option<u32>,
        ) -> Option<ObScriptValue> {
            self.calls.push((cmd, args.to_vec()));
            match cmd {
                // GetStage SE46 → report stage 0 so the "< 70" gate opens.
                0x103a => Some(ObScriptValue::Num(0.0)),
                // GetStageDone → not done.
                0x103b => Some(ObScriptValue::Num(0.0)),
                _ => Some(ObScriptValue::Num(0.0)),
            }
        }
    }

    #[test]
    #[ignore = "needs Oblivion.esm on disk (~160 MB resident)"]
    fn vanilla_oblivion_quest_scripts_execute_and_setstage() {
        // #5088 — resolve through the shared helper (same env var and
        // default as every other real-data test), and under the strict
        // lane an absent master fails the test instead of returning a
        // green no-op. Pre-fix this `return`ed silently even with
        // `BYROREDUX_REQUIRE_GAME_DATA=1`, and no CI lane built this
        // crate at all — the only guard over the 255 vanilla quest
        // scripts could go green without touching a byte of data.
        let path = byroredux_plugin::esm::test_paths::oblivion_esm();
        if !path.is_file() {
            if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
                panic!(
                    "BYROREDUX_REQUIRE_GAME_DATA is set, but Oblivion.esm was not \
                     found at {path:?} — BYROREDUX_OBLIVION_DATA is unset or its \
                     directory lacks the master"
                );
            }
            return;
        }
        let esm = std::fs::read(&path).unwrap();
        let index = byroredux_plugin::esm::parse_esm(&esm).unwrap();

        // Corpus-wide: every quest script's GameMode block must decode
        // without Malformed (some scripts legitimately have no GameMode).
        let mut executed = 0usize;
        let mut malformed = 0usize;
        let mut setstage_scripts = 0usize;
        for quest in index.quests.values() {
            if quest.script_ref == 0 {
                continue;
            }
            let Some(script) = index.scripts.get(&quest.script_ref) else {
                continue;
            };
            if script.compiled.is_empty() {
                continue;
            }
            let program = ObScriptProgram::new(script);
            let mut state = crate::obscript_vm::VmState::default();
            let mut host = Recorder::default();
            let outcome = program.run_block(BLOCK_GAME_MODE, &mut state, &mut host);
            match outcome {
                BlockOutcome::Malformed(at) => {
                    malformed += 1;
                    eprintln!(
                        "MALFORMED quest {:08X} script {:08X} at {at:#06x}",
                        quest.form_id, script.form_id
                    );
                }
                BlockOutcome::Completed | BlockOutcome::Returned => {
                    executed += 1;
                    if host.calls.iter().any(|(cmd, _)| *cmd == 0x1039) {
                        setstage_scripts += 1;
                    }
                }
            }
        }
        // 255 quest scripts resolve; expect effectively all to decode.
        assert!(
            executed + malformed >= 200,
            "only {executed}+{malformed} quest scripts reached execution"
        );
        assert!(
            malformed <= executed / 20,
            "{malformed} of {} quest scripts malformed — framing regression",
            executed + malformed
        );
        // Under an all-zero world (every actor state, item count, and
        // stage reads 0), only scripts whose gates are `GetStage X == 0` /
        // `GetStageDone X == n == 0` open their SetStage. 13 were measured on
        // the 2026-09-18 corpus (MS12 ×7, MS23 ×3, SQ07 ×2, …) — but that count
        // included gates that were open by accident: a call on a reference inside
        // an expression (`player.GetDead`, `Actor.GetStage Q`) was read as the
        // reference's form id, which is nonzero. They now read what the host
        // answers (0 here), so those gates stay shut: 7 on 2026-10-18. Assert the
        // measured floor, not a guess.
        assert!(
            setstage_scripts >= 5,
            "only {setstage_scripts} quest scripts issued SetStage — conditions never open"
        );

        // Targeted: MS23Script's first gate is `( GetStage MS23 == 40 ) &&
        // ( Player.GetDistance MS23RolandsHouseMarker <= 300 )` -> `SetStage MS23 45`. Under
        // an all-zero world it stays shut (the stage reads 0); when the stage is 40 and the
        // player is 100 units from the marker it opens, once, naming MS23 and stage 45.
        // (This gate used to look open under zeros only because `Player.GetDistance` inside the
        // expression was read as a form id.)
        let ms23 = index
            .scripts
            .values()
            .find(|s| s.editor_id.eq_ignore_ascii_case("MS23Script"))
            .expect("MS23Script present in vanilla Oblivion.esm");
        let program = ObScriptProgram::new(ms23);
        let mut state = crate::obscript_vm::VmState::default();
        let mut host = Recorder::default();
        assert_eq!(program.run_block(BLOCK_GAME_MODE, &mut state, &mut host), BlockOutcome::Completed);
        assert!(!host.calls.iter().any(|(cmd, ..)| *cmd == 0x1039), "MS23 stays shut under a zeroed world");

        struct Answers(Vec<(u16, Vec<ObScriptValue>)>);
        impl ObScriptHost for Answers {
            fn call(&mut self, cmd: u16, args: &[ObScriptValue], _caller: Option<u32>) -> Option<ObScriptValue> {
                self.0.push((cmd, args.to_vec()));
                Some(ObScriptValue::Num(match cmd {
                    0x103a => 40.0, // GetStage
                    0x1001 => 100.0, // GetDistance
                    _ => 0.0,
                }))
            }
        }
        let mut answers = Answers(Vec::new());
        assert_eq!(program.run_block(BLOCK_GAME_MODE, &mut crate::obscript_vm::VmState::default(), &mut answers), BlockOutcome::Completed);
        let set_stages: Vec<_> = answers.0.iter().filter(|(cmd, _)| *cmd == 0x1039).collect();
        assert_eq!(set_stages.len(), 1, "only the stage-40 gate opens: {set_stages:?}");
        assert_eq!(set_stages[0].1.get(1), Some(&ObScriptValue::Num(45.0)));
    }

    /// #5629 — the gate above only checks that a `SetStage` was *issued*,
    /// not which quest it named, so it was blind to references resolving
    /// against the SCRO-only list. FNV `Patch04Quest00SCRIPT` has a
    /// reference variable (`rCountryRadio`, an SCRV entry) between literal
    /// objects, which shifted every later reference: with the interleaved
    /// list it must start exactly the five quests its source names and stop
    /// its own quest, and the `.Enable` on the never-assigned reference
    /// variable must be declined rather than run on the owner.
    #[test]
    #[ignore = "needs FalloutNV.esm on disk"]
    fn fnv_patch04_quest_script_targets_its_own_scro_quests() {
        let path = byroredux_plugin::esm::test_paths::fnv_esm();
        if !path.is_file() {
            if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
                panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but FalloutNV.esm was not found at {path:?}");
            }
            return;
        }
        let esm = std::fs::read(&path).unwrap();
        let index = byroredux_plugin::esm::parse_esm(&esm).unwrap();
        let script = index
            .scripts
            .values()
            .find(|s| s.editor_id.eq_ignore_ascii_case("Patch04Quest00SCRIPT"))
            .expect("Patch04Quest00SCRIPT present in vanilla FalloutNV.esm");
        let mut state = crate::obscript_vm::VmState::default();
        let mut host = Recorder::default();
        let outcome =
            ObScriptProgram::new(script).run_block(BLOCK_GAME_MODE, &mut state, &mut host);
        assert_eq!(outcome, BlockOutcome::Completed);

        let targets = |cmd: u16| -> Vec<u32> {
            host.calls
                .iter()
                .filter(|(c, _)| *c == cmd)
                .map(|(_, args)| match args.first() {
                    Some(ObScriptValue::Ref(form)) => *form,
                    other => panic!("quest command {cmd:#06x} without a ref arg: {other:?}"),
                })
                .collect()
        };
        assert_eq!(
            targets(0x1036),
            [0x0016_3427, 0x0016_3429, 0x0016_3428, 0x0015_FB7D, 0x0015_F4FF],
            "the five StartQuests name the five SCRO quests that follow the SCRV slot"
        );
        assert_eq!(
            targets(0x1037),
            [0x0017_BA15],
            "StopQuest names the script's own quest, not form 0"
        );
        assert!(
            host.calls.iter().any(|(c, args)| *c
                == crate::obscript_vm::CMD_DECLINED
                && args.first() == Some(&ObScriptValue::Num(f32::from(0x1021u16)))),
            "`rCountryRadio.Enable` (caller = the SCRV slot) is declined, not run on the owner"
        );
    }

    /// M47.3 phase 2: every FO3 and FNV quest script's `GameMode` block decodes
    /// to its `End`. Before the `z` float-literal fix (a 64-bit double, not a
    /// NUL-terminated string) five FNV quest scripts came back `Malformed` at
    /// their first `SetQuestDelay <quest> 0.1`, so they never ran at all.
    #[test]
    #[ignore = "needs FalloutNV.esm and Fallout3.esm on disk"]
    fn fallout_quest_scripts_decode_without_malformed_bytecode() {
        for (name, path) in [
            ("FalloutNV.esm", byroredux_plugin::esm::test_paths::fnv_esm()),
            ("Fallout3.esm", byroredux_plugin::esm::test_paths::fo3_esm()),
        ] {
            if !path.is_file() {
                if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
                    panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but {name} was not found at {path:?}");
                }
                continue;
            }
            let index =
                byroredux_plugin::esm::parse_esm(&std::fs::read(&path).unwrap()).unwrap();
            let (mut ran, mut malformed) = (0usize, Vec::new());
            for quest in index.quests.values().filter(|q| q.script_ref != 0) {
                let Some(script) = index.scripts.get(&quest.script_ref) else { continue };
                if script.compiled.is_empty() {
                    continue;
                }
                // Both arms of every gate are walked: all-0 and all-1 answers.
                for answer in [0.0f32, 1.0] {
                    struct Fixed(f32);
                    impl ObScriptHost for Fixed {
                        fn call(&mut self, _: u16, _: &[ObScriptValue], _: Option<u32>) -> Option<ObScriptValue> {
                            Some(ObScriptValue::Num(self.0))
                        }
                    }
                    let outcome = ObScriptProgram::new(script).run_block(
                        BLOCK_GAME_MODE,
                        &mut crate::obscript_vm::VmState::default(),
                        &mut Fixed(answer),
                    );
                    if let BlockOutcome::Malformed(at) = outcome {
                        malformed.push((script.form_id, at));
                    }
                }
                ran += 1;
            }
            assert!(ran > 90, "{name}: only {ran} quest scripts ran");
            assert!(malformed.is_empty(), "{name}: malformed GameMode bytecode in {malformed:x?}");
        }
    }
}

#[cfg(test)]
mod tick_interval_tests {
    use super::*;
    use byroredux_plugin::esm::records::ScriptType;

    const QUEST: u32 = 0x0001_2345;

    /// A running quest whose script is an empty `GameMode` block.
    fn world_with_quest(authored_delay: f32) -> World {
        let mut world = World::new();
        let mut script = ScriptRecord { form_id: 1, script_type: ScriptType::Quest, ..Default::default() };
        // `Begin GameMode` / `End`.
        script.compiled = [0x10u16.to_le_bytes(), 6u16.to_le_bytes(), 0u16.to_le_bytes(), 0u16.to_le_bytes(), 0u16.to_le_bytes(), 0x11u16.to_le_bytes(), 0u16.to_le_bytes()]
            .concat();
        let mut table = QuestScriptTable::default();
        table.scripts.insert(QUEST, QuestScript { script: Arc::new(script), quest: QuestFormId(QUEST), authored_delay });
        world.insert_resource(table);
        world.insert_resource(ObScriptQuestTimers::default());
        world.insert_resource(ObScriptQuestLocals::default());
        let mut stages = QuestStageState::default();
        stages.start_quest(QuestFormId(QUEST), None);
        world.insert_resource(stages);
        world
    }

    fn runs(world: &World) -> u32 {
        world.try_resource::<ObScriptQuestTimers>().unwrap().runs.get(&QUEST).copied().unwrap_or(0)
    }

    /// Quests sharing a tick run in FormID order, not `HashMap` order (#6013).
    #[test]
    fn running_quests_are_ordered_by_form_id() {
        let mut stages = QuestStageState::default();
        let ids: Vec<u32> = (0..64).map(|i| 0x0001_0000 + (i * 37) % 64).collect();
        for id in &ids {
            stages.start_quest(QuestFormId(*id), None);
        }
        let mut expected = ids.clone();
        expected.sort_unstable();
        assert_eq!(running_quests_in_order(&stages), expected);
    }

    /// The vanilla cadence still applies when nothing is authored.
    #[test]
    fn an_unauthored_quest_runs_every_five_seconds() {
        let world = world_with_quest(0.0);
        for _ in 0..4 {
            obscript_quest_tick_system(&world, 1.0);
        }
        assert_eq!(runs(&world), 0, "4 s is not due");
        obscript_quest_tick_system(&world, 1.0);
        assert_eq!(runs(&world), 1);
    }

    /// FNV ships ~80 scripted quests with a `QUST` delay (21 at 0.1 s); the
    /// tick must honour it instead of running them at the default.
    #[test]
    fn an_authored_quest_delay_sets_the_cadence() {
        let world = world_with_quest(0.5);
        for _ in 0..10 {
            obscript_quest_tick_system(&world, 0.25);
        }
        assert_eq!(runs(&world), 5, "10 x 0.25 s at a 0.5 s interval");
    }

    /// `SetQuestDelay` beats the authored delay, and 0 returns to the INI default.
    #[test]
    fn set_quest_delay_changes_the_cadence_at_runtime() {
        let world = world_with_quest(5.0);
        world.try_resource_mut::<ObScriptQuestLocals>().unwrap().set_delay(QUEST, 1.0);
        for _ in 0..3 {
            obscript_quest_tick_system(&world, 1.0);
        }
        assert_eq!(runs(&world), 3);
        world.try_resource_mut::<ObScriptQuestLocals>().unwrap().set_delay(QUEST, 0.0);
        for _ in 0..4 {
            obscript_quest_tick_system(&world, 1.0);
        }
        assert_eq!(runs(&world), 3, "back on the 5 s default");
        obscript_quest_tick_system(&world, 1.0);
        assert_eq!(runs(&world), 4);
    }
}
