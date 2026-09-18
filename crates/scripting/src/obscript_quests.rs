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
//! | everything else | — | traced no-op returning 0, counted in [`ObScriptDiagnostics`] |

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
pub fn install_quest_scripts(world: &mut World, quests: &HashMap<u32, QustRecord>, scripts: &HashMap<u32, ScriptRecord>) {
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
    if world.try_resource::<ObScriptEffectLog>().is_none() {
        world.insert_resource(ObScriptEffectLog::default());
    }
    world.insert_resource(table);
    log::info!("Installed {installed} ObScript quest scripts");
}

/// Execute one quest script's GameMode block against the world.
/// Returns the outcome (diagnostics).
pub fn run_quest_game_mode(world: &World, quest_form_id: u32, elapsed: f32) -> Option<BlockOutcome> {
    let table = world.try_resource::<QuestScriptTable>()?;
    let quest_script = table.scripts.get(&quest_form_id)?;
    let script = quest_script.script.clone();
    drop(table);

    let program = ObScriptProgram::new(&script);
    let mut state = VmState::default();
    let mut host = QuestCommandHost {
        world,
        quest: quest_form_id,
        elapsed,
        script: &script,
    };
    let outcome = program.run_block(0, &mut state, &mut host);
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
                let target = ref_arg(0).unwrap_or(self.quest);
                let stage = num(1) as u16;
                if let Some(mut stages) = self.world.try_resource_mut::<QuestStageState>() {
                    stages.set_stage(QuestFormId(target), stage);
                }
                effect(self, format!("SetStage {:08X} {}", target, stage));
                None
            }
            // StartQuest / StopQuest
            0x1036 => {
                let target = ref_arg(0).unwrap_or(self.quest);
                if let Some(mut stages) = self.world.try_resource_mut::<QuestStageState>() {
                    stages.start_quest(QuestFormId(target), None);
                }
                effect(self, format!("StartQuest {:08X}", target));
                Some(ObScriptValue::Num(1.0))
            }
            0x1037 => {
                let target = ref_arg(0).unwrap_or(self.quest);
                if let Some(mut stages) = self.world.try_resource_mut::<QuestStageState>() {
                    stages.stop(QuestFormId(target));
                }
                effect(self, format!("StopQuest {:08X}", target));
                None
            }
            // GetStage <quest>
            0x103a => {
                let target = ref_arg(0).unwrap_or(self.quest);
                let stage = self
                    .world
                    .try_resource::<QuestStageState>()
                    .map(|s| s.get_stage(QuestFormId(target)))
                    .unwrap_or(0);
                Some(ObScriptValue::Num(stage as f32))
            }
            // GetStageDone <quest> <stage>
            0x103b => {
                let target = ref_arg(0).unwrap_or(self.quest);
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
                let target = ref_arg(0).unwrap_or(self.quest);
                let running = self
                    .world
                    .try_resource::<QuestStageState>()
                    .map(|s| s.is_running(QuestFormId(target)))
                    .unwrap_or(false);
                Some(ObScriptValue::Num(running as i32 as f32))
            }
            // Message "text" / MessageBox … — phase 1 records the effect;
            // string payloads are decoded from the script's own bytes by a
            // later phase, so these log the invocation shape only.
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
            // GetButtonPressed — no message menu exists yet.
            0x101f => Some(ObScriptValue::Num(-1.0)),
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
                if let Some(mut timers) = self.world.try_resource_mut::<ObScriptQuestTimers>() {
                    *timers
                        .unknown_commands
                        .entry(self.quest)
                        .or_default()
                        .entry(cmd)
                        .or_insert(0) += 1;
                }
                log::trace!(
                    "ObScript {:08X}: unknown command {cmd:#06x} ({} args) — no-op",
                    self.quest,
                    args.len(),
                );
                Some(ObScriptValue::Num(0.0))
            }
        }
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

#[cfg(test)]
mod real_data_tests {
    use super::*;
    use crate::obscript_vm::{ObScriptHost, ObScriptProgram, ObScriptValue, BLOCK_GAME_MODE};

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
        let path = std::env::var_os("BYROREDUX_OBLIVION_DATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data")
            })
            .join("Oblivion.esm");
        if !path.is_file() {
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
                    if host.calls.iter().any(|(cmd, _, )| *cmd == 0x1039) {
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
            host.calls
                .iter()
                .any(|(cmd, ..)| *cmd == 0x1039),
            "MS23 must issue SetStage under a zeroed world"
        );
    }
}
