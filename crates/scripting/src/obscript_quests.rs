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
//! | `0x1059` / `0x1000` | Message / MessageBox | effect-log entry (HUD is later work) |
//! | `0x100c` | GetSecondsPassed | per-script elapsed since last tick |
//! | `0x101f` | GetButtonPressed | −1 (no message menu this phase) |
//! | everything else | — | traced no-op returning 0, counted in [`ObScriptQuestTimers`] |

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::{QustRecord, ScriptRecord};

use crate::globals::Globals;
use crate::obscript_vm::{BlockOutcome, ObScriptHost, ObScriptProgram, ObScriptValue, VmState};
use crate::quest_stages::{QuestFormId, QuestStageState, QuestStatus};

/// Vanilla quest-script cadence: the original engine re-runs each running
/// quest's GameMode block every 5 seconds (`fQuestProcessInterval`).
pub const QUEST_SCRIPT_INTERVAL_SECS: f32 = 5.0;

/// One quest's compiled script, plus the SCRO reference table needed to
/// resolve its `' r<n>'`/argument references at execution time.
pub struct QuestScript {
    pub script: Arc<ScriptRecord>,
    /// Quest FormID owning the script (the implicit caller).
    pub quest: QuestFormId,
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

/// The local variables quest scripts hold between GameMode runs: quest FormID to
/// the 1-based `SLSD` slot and its value. A variable no run assigned is absent and
/// reads `0`, the value every ObScript local starts at. `GetQuestVariable`
/// (condition function 79) reads it. Saved: a quest script's variables are
/// gameplay state.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ObScriptQuestLocals {
    values: std::collections::BTreeMap<u32, std::collections::BTreeMap<u16, f32>>,
}

impl Resource for ObScriptQuestLocals {}

impl ObScriptQuestLocals {
    /// A quest's variables, to seed a run's [`VmState`].
    pub fn of(&self, quest: u32) -> HashMap<u16, f32> {
        self.values.get(&quest).map(|slots| slots.iter().map(|(&slot, &value)| (slot, value)).collect()).unwrap_or_default()
    }

    /// Keep a run's variables for the next run.
    pub fn store(&mut self, quest: u32, locals: &HashMap<u16, f32>) {
        if locals.is_empty() {
            self.values.remove(&quest);
        } else {
            self.values.insert(quest, locals.iter().map(|(&slot, &value)| (slot, value)).collect());
        }
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
            },
        );
        installed += 1;
    }
    if installed == 0 {
        return;
    }
    if world.try_resource::<QuestScriptTable>().is_none() {
        world.insert_resource(QuestScriptTable::default());
    }
    if world.try_resource::<ObScriptQuestTimers>().is_none() {
        world.insert_resource(ObScriptQuestTimers::default());
    }
    if world.try_resource::<ObScriptQuestLocals>().is_none() {
        world.insert_resource(ObScriptQuestLocals::default());
    }
    if world.try_resource::<ObScriptEffectLog>().is_none() {
        world.insert_resource(ObScriptEffectLog::default());
    }
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
    let mut state = VmState {
        locals: world
            .try_resource::<ObScriptQuestLocals>()
            .map(|locals| locals.of(quest_form_id))
            .unwrap_or_default(),
    };
    let mut host = QuestCommandHost {
        world,
        quest: quest_form_id,
        elapsed,
        script: &script,
    };
    let outcome = program.run_block(0, &mut state, &mut host);
    // `host` holds no guard (Drop is not implemented) — the borrow ends
    // here by scope, which is what the old `drop(host)` extended.
    if let Some(mut locals) = world.try_resource_mut::<ObScriptQuestLocals>() {
        locals.store(quest_form_id, &state.locals);
    }
    Some(outcome)
}

/// ECS host: lowers the phase-1 command set onto [`QuestStageState`],
/// [`Globals`], and the effect log. Unknown commands are traced no-ops
/// returning 0 and counted per quest.
pub struct QuestCommandHost<'a> {
    pub world: &'a World,
    pub quest: u32,
    pub elapsed: f32,
    pub script: &'a ScriptRecord,
}

impl<'a> ObScriptHost for QuestCommandHost<'a> {
    fn message(&mut self, message: crate::obscript_vm::ObScriptMessage) {
        let kind = if message.message_box { "MessageBox" } else { "Message" };
        log::debug!("quest {:08X} {kind}: {:?}", self.quest, message.text);
        crate::messages::show_obscript_message(self.world, QuestFormId(self.quest), message);
    }

    fn call(
        &mut self,
        cmd: u16,
        args: &[ObScriptValue],
        caller: Option<u32>,
    ) -> Option<ObScriptValue> {
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
        let own_quest = self.quest;
        let quest_target = |i: usize| match args.get(i) {
            Some(ObScriptValue::Ref(0)) => None,
            _ => Some(ref_arg(i).unwrap_or(own_quest)),
        };
        let effect = |host: &mut Self, text: String| {
            if let Some(mut log) = host.world.try_resource_mut::<ObScriptEffectLog>() {
                log.push(format!("{:08X}", host.quest), text);
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
            // GetButtonPressed — the quest's answered box, once (M47.3).
            0x101f => Some(ObScriptValue::Num(
                crate::messages::take_button(self.world, QuestFormId(self.quest)) as f32,
            )),
            // GetSelf — the owning quest's form id (quest scripts).
            0x10ce => Some(ObScriptValue::Ref(self.quest)),
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
            crate::obscript_vm::CMD_SET_GLOBAL => {
                let form = caller.unwrap_or(0);
                let value = num(0);
                if let Some(mut globals) = self.world.try_resource_mut::<Globals>() {
                    globals.set(form, value);
                }
                None
            }
            _ => {
                // Traced no-op. The census counter keeps phase-1 honest about
                // coverage instead of silently faking behavior.
                log::trace!(
                    "ObScript {:08X}: unknown command {cmd:#06x} ({} args) — no-op",
                    self.quest,
                    args.len(),
                );
                self.decline(cmd)
            }
        }
    }
}

impl QuestCommandHost<'_> {
    /// Record `cmd` as not executed — an unknown command, or a known one
    /// whose reference could not be resolved — under the per-quest census,
    /// and answer like an unknown command (value 0). Never faked.
    fn decline(&self, cmd: u16) -> Option<ObScriptValue> {
        if let Some(mut timers) = self.world.try_resource_mut::<ObScriptQuestTimers>() {
            *timers
                .unknown_commands
                .entry(self.quest)
                .or_default()
                .entry(cmd)
                .or_insert(0) += 1;
        }
        Some(ObScriptValue::Num(0.0))
    }
}

/// Advance every running quest's script on the vanilla cadence. Exclusive
/// in `Stage::Update`, beside `ambient_ai_package_system` — it reads the
/// quest resources and runs pure interpreter work, so exclusivity costs
/// nothing and keeps the timer maps race-free.
pub fn obscript_quest_tick_system(world: &World, dt: f32) {
    let running: Vec<u32> = match world.try_resource::<QuestStageState>() {
        Some(stages) => stages
            .iter()
            .filter(|(_, data)| data.status == QuestStatus::Running)
            .map(|(id, _)| id.0)
            .collect(),
        None => return,
    };
    if running.is_empty() {
        return;
    }
    let table = world.try_resource::<QuestScriptTable>();
    let Some(table) = table else { return };
    let due: Vec<(u32, f32)> = {
        let Some(mut timers) = world.try_resource_mut::<ObScriptQuestTimers>() else {
            return;
        };
        running
            .iter()
            .filter_map(|id| {
                if !table.scripts.contains_key(id) {
                    return None;
                }
                let slot = timers.elapsed.entry(*id).or_insert(0.0);
                *slot += dt;
                if *slot >= QUEST_SCRIPT_INTERVAL_SECS {
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

/// Host-level behaviour of [`QuestCommandHost`] on synthetic calls.
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
        let mut host = QuestCommandHost {
            world,
            quest: OWN,
            elapsed: 0.0,
            script: &script,
        };
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
        // `GetStageDone X == n == 0` open their SetStage — measured 13 on
        // the 2026-09-18 corpus (MS12 ×7, MS23 ×3, SQ07 ×2, …). Assert the
        // measured floor, not a guess.
        assert!(
            setstage_scripts >= 10,
            "only {setstage_scripts} quest scripts issued SetStage — conditions never open"
        );

        // Targeted: MS23Script gates on `GetStageDone MS23 N == 0`, which
        // the zero-stub satisfies — its GameMode must complete and fire
        // SetStage on the first tick, exactly as it does in-game on a
        // fresh save.
        let ms23 = index
            .scripts
            .values()
            .find(|s| s.editor_id.eq_ignore_ascii_case("MS23Script"))
            .expect("MS23Script present in vanilla Oblivion.esm");
        let program = ObScriptProgram::new(ms23);
        let mut state = crate::obscript_vm::VmState::default();
        let mut host = Recorder::default();
        assert_eq!(
            program.run_block(BLOCK_GAME_MODE, &mut state, &mut host),
            BlockOutcome::Completed
        );
        assert!(
            host.calls.iter().any(|(cmd, ..)| *cmd == 0x1039),
            "MS23 must issue SetStage under a zeroed world"
        );
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
}
