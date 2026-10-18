//! ObScript bytecode interpreter (M47.3 — functional quests, phase 1: Oblivion).
//!
//! Executes the compiled `SCDA` statement stream of legacy (Oblivion/FO3/FNV)
//! scripts. The framing and encodings here are **empirically derived from
//! vanilla `Oblivion.esm`** (2 393 SCPTs, each with its `SCTX` source in
//! hand), not from a spec document:
//!
//! - Statements: `[op:u16][payload_len:u16][payload]`, packed back to back.
//!   Flow statements: `0x10` Begin (payload = `[block_type u16][body_len
//!   u32]` — body_len cross-checks as compiled_len − 14 on one-block
//!   scripts), `0x11` End, `0x15` Set, `0x16`/`0x18` If/ElseIf, `0x17`
//!   Else, `0x19` EndIf, `0x1d` ScriptName (empty), `0x1e` Return.
//! - **Statement calls** come in two forms: implicit-caller — the line
//!   opcode *is* the command id (`[cmd:u16][len:u16][args]`; e.g. `0x1039`
//!   SetStage inside a quest's own script) — and explicit-caller via the
//!   `0x1c` escape (`[1c][caller_ref:u16][cmd:u16][len:u16][args]`, e.g.
//!   `SEHaskillRef.MoveTo player`).
//! - **Expression payloads** (If/ElseIf conditions and Set right-hand
//!   sides) are reverse-polnish token streams over ASCII operators and
//!   literals (`26 >= `, `&&`) interleaved with space-prefixed binary
//!   escapes: `' r<n>'` ref variable, `' s<n>'`/`' f<n>'` local short/
//!   float, `' G<n>'` global (SCRO-resolved), `'n'`+i32 literal, and
//!   `'X'+cmd+u16 len+args` for command calls.
//! - **Call arguments**: `[u16 count]` then per-arg a u16 integer literal
//!   (first byte outside the escape set) or a tagged value from the escape
//!   set `'G' 'f' 'n' 'r' 's' 'z' 'Y' 'Z'` (`'z'` = a 64-bit double
//!   float literal, `'n'` = an i32). Literals whose bytes would
//!   collide with an escape tag are emitted by the compiler as `'n'` i32
//!   constants — which is why `'n'` dominates the corpus census.
//! - **Command ids** (SetStage `0x1039`, GetStage `0x103a`, GetStageDone
//!   `0x103b`, MessageBox `0x1000`, Message `0x1059`, …) were recovered by
//!   aligning each script's `SCTX` source with its compiled call stream
//!   across the corpus: 2 349 of 2 393 scripts and 12 620 of 12 732 calls
//!   decode cleanly; the residue is per-command string-argument encodings
//!   (parameter signatures this phase treats as traced no-ops).
//!
//! The interpreter is pure: every engine effect goes through
//! [`ObScriptHost`], so tests drive it with a stub and the ECS runtime
//! (`crate::obscript_quests`) supplies the real quest-state host.

use byroredux_plugin::esm::records::{ScriptRecord, ScriptRef};

/// Synthetic host command for global reads — real global accesses arrive
/// as `' G<n>'` escapes, never as bytecode command ids.
pub const CMD_GET_GLOBAL: u16 = u16::MAX - 1;
/// Synthetic host command for global writes (`set GameHour to …`).
pub const CMD_SET_GLOBAL: u16 = u16::MAX;

/// Synthetic host command for a statement the VM declined to run because a
/// reference it needs cannot be resolved or a write it cannot model (an
/// unresolved explicit caller, a remote-variable `Set`). `args[0]` carries
/// the opcode / command id that was declined, so the host can count it
/// under the same census as any other unmodeled command (#5629 / #5630).
pub const CMD_DECLINED: u16 = u16::MAX - 2;

/// Synthetic host command for a read of a **remote variable** (`Quest.var`,
/// `SomeRef.var` in an expression): `args` are the reference and the variable's ordinal in
/// that reference's script. The compiled form is `' r<ref u16>'` immediately followed by
/// `'s'` or `'f'` and the ordinal `u16` (read off the masters: `VCG01.bBeganTest == 0` is
/// `20 72 04 00 73 06 00`), and the statement form `set Quest.var to ...` is the target
/// `r<ref u16> <s|f><ordinal u16>` (#5630).
pub const CMD_GET_REMOTE_VARIABLE: u16 = u16::MAX - 3;
/// Synthetic host command for the write: `args` are the reference, the ordinal and the
/// value (already narrowed for a `short` / `long`).
pub const CMD_SET_REMOTE_VARIABLE: u16 = u16::MAX - 4;

/// The `Set` statement opcode, reported as the declined id for an
/// unmodeled remote-variable assignment.
const OP_SET: u16 = 0x15;

/// Block type of `Begin GameMode` in the compiled block enum (empirical:
/// every quest-script GameMode block carries `0x0000`).
pub const BLOCK_GAME_MODE: u16 = 0;
/// `Begin OnActivate`, `OnDeath` and `OnLoad`. The numbering was read off the
/// vanilla masters by aligning each script's `Begin <name>` lines with its
/// compiled blocks (`obscript_tools census`, "block names"): Oblivion, Fallout 3
/// and New Vegas agree on these (the Fallout pair on every block they share —
/// `OnTriggerEnter` is `0x1a` there and `OnTriggerActor` in Oblivion).
pub const BLOCK_ON_ACTIVATE: u16 = 0x02;
/// `Begin OnAdd`, `OnEquip`, `OnUnequip` and `OnDrop` — the blocks of an item's script:
/// `0x03` to `0x06` on all three games.
pub const BLOCK_ON_ADD: u16 = 0x03;
pub const BLOCK_ON_EQUIP: u16 = 0x04;
pub const BLOCK_ON_UNEQUIP: u16 = 0x05;
pub const BLOCK_ON_DROP: u16 = 0x06;
/// `Begin SayToDone [topic]`: `0x07` on all three games (GECK *SayToDone*).
pub const BLOCK_SAY_TO_DONE: u16 = 0x07;
pub const BLOCK_ON_DEATH: u16 = 0x0a;
pub const BLOCK_ON_LOAD: u16 = 0x15;
/// `Begin OnHit`, `OnHitWith`, `OnStartCombat` and `OnCombatEnd`, numbered by the same
/// alignment over the masters (`obscript_tools census`: `onhit` 154 on `0x08`,
/// `onhitwith` 17 on `0x09`, `oncombatend` 26 on `0x0c`, `onstartcombat` 108 on
/// `0x19`).
pub const BLOCK_ON_HIT: u16 = 0x08;
pub const BLOCK_ON_HIT_WITH: u16 = 0x09;
pub const BLOCK_ON_COMBAT_END: u16 = 0x0c;
pub const BLOCK_ON_START_COMBAT: u16 = 0x19;
/// `Begin OnPackageStart`, `OnPackageDone` (also spelled `OnPackageEnd`) and
/// `OnPackageChange`: `0x0f`, `0x10` and `0x14` on all three games.
pub const BLOCK_ON_PACKAGE_START: u16 = 0x0f;
pub const BLOCK_ON_PACKAGE_DONE: u16 = 0x10;
pub const BLOCK_ON_PACKAGE_CHANGE: u16 = 0x14;
/// `Begin OnTrigger`, `OnTriggerEnter` and `OnTriggerLeave`. `0x1a` is
/// `OnTriggerEnter` on Fallout 3 / New Vegas and `OnTriggerActor` on Oblivion
/// ("run once when an actor triggers", which the enter event also is); `0x1b` is
/// `OnTriggerLeave` on the Fallout pair and `OnTriggerMob` on Oblivion, so it is
/// only read as a leave there.
pub const BLOCK_ON_TRIGGER: u16 = 0x18;
pub const BLOCK_ON_TRIGGER_ENTER: u16 = 0x1a;
pub const BLOCK_ON_TRIGGER_LEAVE: u16 = 0x1b;
/// The scripted-magic-effect blocks (`ScriptEffectStart` 0x11, `Finish` 0x12, `Update` 0x13) and
/// `OnMagicEffectHit` (0x16): ids from the masters' `Begin` lines, as the other block ids above.
pub const BLOCK_SCRIPT_EFFECT_START: u16 = 0x11;
pub const BLOCK_SCRIPT_EFFECT_FINISH: u16 = 0x12;
pub const BLOCK_SCRIPT_EFFECT_UPDATE: u16 = 0x13;
pub const BLOCK_ON_MAGIC_EFFECT_HIT: u16 = 0x16;
/// `OnDestructionStageChange` (0x23).
pub const BLOCK_ON_DESTRUCTION_STAGE_CHANGE: u16 = 0x23;
/// `OnReset` (0x1e).
pub const BLOCK_ON_RESET: u16 = 0x1e;

/// The commands whose first parameter is an axis letter: `Rotate`, `GetPos`, `SetPos`, `GetAngle`,
/// `SetAngle`, `GetStartingPos`, `GetStartingAngle` (`0x1004`, `0x1006`..`0x100b`, the same on all
/// three games).
const AXIS_COMMANDS: [u16; 7] = [0x1004, 0x1006, 0x1007, 0x1008, 0x1009, 0x100a, 0x100b];

/// Maximum recursive nesting accepted from untrusted compiled ObScript.
const MAX_OBSCRIPT_VM_NESTING: usize = 32;

/// A runtime value crossing the host boundary. ObScript variables are
/// numerics (short/long/float — computed in f32, narrowed on assignment by
/// the declared local type); references ride as their form id.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObScriptValue {
    Num(f32),
    Ref(u32),
}

impl ObScriptValue {
    /// Numeric view of the value (refs coerce to their form id as f32) —
    /// public because command hosts lower arguments with it.
    pub fn as_num(&self) -> f32 {
        match self {
            ObScriptValue::Num(n) => *n,
            ObScriptValue::Ref(r) => *r as f32,
        }
    }
}

/// Result of running one script block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockOutcome {
    /// The block ran to its `End`.
    Completed,
    /// The block hit a `Return`.
    Returned,
    /// The stream was truncated/malformed at the reported byte offset.
    Malformed(usize),
}

/// Engine-side command dispatch. `caller` is the form id of the object the
/// command was invoked on (`None` = the script's own owner, e.g. the quest).
/// Returns the command's value, or `None` for pure statements.
pub trait ObScriptHost {
    fn call(
        &mut self,
        cmd: u16,
        args: &[ObScriptValue],
        caller: Option<u32>,
    ) -> Option<ObScriptValue>;

    /// `Message` / `MessageBox` with their decoded text, variables and
    /// buttons (M47.3 phase 2). A host without a message surface ignores it.
    fn message(&mut self, _message: ObScriptMessage) {}

    /// The value of the one text argument of a command that writes it as a string (`PlayIdle <idle
    /// editor ID>`, `GetGS <setting name>`, `ShowWarning "text"`): the form the name stands for, the
    /// setting's value, ... — whatever the command then receives as its argument. `None` when the host
    /// cannot say; the command is then declined.
    fn text_argument(&mut self, _cmd: u16, _text: &str, _rest: &[ObScriptValue]) -> Option<ObScriptValue> {
        None
    }

    /// The strings written after the reference of a command that takes several (`SwapTexture <ref>
    /// "<slot>" "<texture>"`), handed over just before the command is called.
    fn text_arguments(&mut self, _cmd: u16, _texts: &[String]) {}

    /// Whether the command just run ends the script, as a `Return` does: `RemoveMe`
    /// and `DropMe` "act as a return when called -- the lines of script following it will
    /// NOT be executed" (GECK), the script having just removed the object it runs on.
    /// Asked after every command statement; the answer is one-shot.
    fn ends_script(&mut self) -> bool {
        false
    }
}

/// Commands whose one argument is a string, not a form index or a number: `PlayIdle` (GECK: "takes the
/// actual editorID string of the idle as parameter"), `GetGS <setting>` and `ShowWarning "text"`.
const STRING_FORM_COMMANDS: [u16; 8] = [0x1190, 0x1100, 0x11b9, 0x1137, 0x1138, 0x11ff, 0x114d, 0x1114];

/// Commands that take a reference and then strings (`SwapTexture <ref> "<slot>" "<texture>"`), the
/// strings handed to the host through [`ObScriptHost::text_arguments`].
const REF_STRINGS_COMMANDS: [u16; 1] = [0x1269];

/// `TrapUpdate` (Oblivion).
pub const CMD_TRAP_UPDATE: u16 = 0x110c;

/// The variables a trap's script declares for `TrapUpdate`, in the order the command receives them.
const TRAP_VARIABLES: [&str; 6] = ["fTrapDamage", "fTrapPushBack", "fTrapDeathPushBack", "fTrapMinVelocity", "bTrapContinuous", "fLevelledDamage"];

/// `MessageBox` (`0x1000`).
pub const CMD_MESSAGE_BOX: u16 = 0x1000;
/// `Message` (`0x1059`).
pub const CMD_MESSAGE: u16 = 0x1059;

/// A decoded `Message` / `MessageBox` statement. UESP *Oblivion Mod:Mod File
/// Format/SCPT* §SCDA: `01 00`, the text as a `[u16 length][bytes]` string
/// parameter, `[u16 count]` variables in the set/if variable format, then for
/// `MessageBox` `[u16 count]` buttons, each `01 00` + a string parameter
/// (`Message` instead ends with its display seconds). Checked against
/// vanilla `Oblivion.esm` (`message "Rank %.0f Fireball", SpellRank, 10`,
/// `messageBox "…obelisk?", "Yes", "No"`).
#[derive(Debug, Clone, PartialEq)]
pub struct ObScriptMessage {
    pub text: String,
    pub values: Vec<f32>,
    pub buttons: Vec<String>,
    pub message_box: bool,
}

/// One `Begin` .. `End` block of a compiled script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockInfo {
    /// The compiled block type (`0` GameMode, `2` OnActivate, ...).
    pub block_type: u16,
    /// The one reference the block was written for (`Begin OnTrigger player`
    /// -> `0x14`), if its `Begin` names one.
    pub filter: Option<u32>,
    /// Offset of the block's first body statement.
    body: usize,
}

/// Mutable per-script state that outlives one block execution.
#[derive(Default)]
pub struct VmState {
    /// Local variable storage keyed by the 1-based `SLSD` index — compiled
    /// references are 1-based over that table.
    pub locals: std::collections::HashMap<u16, f32>,
    /// The reference each *reference variable* slot holds, exactly: a form id
    /// does not fit an `f32` past `2^24` (every DLC form), so a slot the script
    /// uses as a `ref` ([`ScriptRef::Variable`]) keeps its value here, with the
    /// nearest `f32` in [`Self::locals`] for the readers that only know numbers.
    /// A slot absent here holds no reference.
    pub ref_locals: std::collections::HashMap<u16, u32>,
}

/// The local slots `script` uses as reference variables: its `SCRV` entries.
pub fn ref_slots(script: &ScriptRecord) -> impl Iterator<Item = u16> + '_ {
    script.refs.iter().filter_map(|entry| match entry {
        ScriptRef::Variable(slot) => u16::try_from(*slot).ok(),
        ScriptRef::Object(_) => None,
    })
}

/// A form id as two `f32` halves that are each exactly representable, for the
/// stores that keep script variables as numbers (the saved `ScriptVariables`,
/// the quest locals). `0` is no reference.
pub fn form_halves(form: u32) -> (f32, f32) {
    ((form & 0xFFFF) as f32, (form >> 16) as f32)
}

/// The inverse of [`form_halves`].
pub fn form_from_halves(low: f32, high: f32) -> u32 {
    ((high as u32) << 16) | (low as u32 & 0xFFFF)
}

/// One compiled script prepared for execution. Borrows the parsed SCPT;
/// variable storage lives in the caller-provided [`VmState`], so successive
/// ticks observe previous writes.
pub struct ObScriptProgram<'a> {
    script: &'a ScriptRecord,
}

impl<'a> ObScriptProgram<'a> {
    pub fn new(script: &'a ScriptRecord) -> Self {
        Self { script }
    }

    /// The script's blocks, in order, without running any of it. `None` if the
    /// stream is malformed, so a caller never schedules a script it cannot walk.
    ///
    /// A script can have several blocks of one type (FO3's `Begin OnTriggerEnter
    /// player` / `... StickyRef` / `... RedRef`), each written for a different
    /// reference, so blocks are addressed by their entry here, not by type.
    pub fn blocks(&self) -> Option<Vec<BlockInfo>> {
        struct NoHost;
        impl ObScriptHost for NoHost {
            fn call(&mut self, _: u16, _: &[ObScriptValue], _: Option<u32>) -> Option<ObScriptValue> {
                None
            }
        }
        let mut state = VmState::default();
        let mut host = NoHost;
        let mut vm = Vm {
            bytes: &self.script.compiled,
            refs: &self.script.refs,
            locals: &self.script.locals,
            state: &mut state,
            host: &mut host,
            returned: false,
        };
        let bytes = vm.bytes;
        let mut blocks = Vec::new();
        let mut offset = 0usize;
        while let Some(op) = byroredux_core::bytes::le_u16_at(bytes, offset) {
            if op == 0x10 {
                let (payload_start, payload_end) = payload_span(bytes, offset)?;
                let block_type = byroredux_core::bytes::le_u16_at(bytes, payload_start)?;
                // `[block_type u16][body_len u32]`, then — for a block written
                // for one reference — the same `[u16 count][args]` blob a
                // command's arguments use: `Begin OnTrigger player` compiles to
                // `01 00 72 01 00` (one argument, reference variable 1).
                // GameMode and the like have none; an unparameterised
                // `OnActivate` a count of `0`.
                let params = payload_start + 6;
                let filter = if params < payload_end {
                    match vm.decode_args(params, payload_end)?.first() {
                        Some(ObScriptValue::Ref(form)) if *form != 0 => Some(*form),
                        _ => None,
                    }
                } else {
                    None
                };
                blocks.push(BlockInfo { block_type, filter, body: payload_end });
                offset = vm.skip_to_block_end(payload_end)?;
            } else {
                offset = next_statement(bytes, offset, op)?;
            }
        }
        Some(blocks)
    }

    /// The block types the script has, in order.
    pub fn block_types(&self) -> Option<Vec<u16>> {
        Some(self.blocks()?.iter().map(|block| block.block_type).collect())
    }

    /// Run the block `info` (from [`Self::blocks`]) of this script.
    pub fn run_at(&self, info: &BlockInfo, state: &mut VmState, host: &mut dyn ObScriptHost) -> BlockOutcome {
        let mut vm = Vm {
            bytes: &self.script.compiled,
            refs: &self.script.refs,
            locals: &self.script.locals,
            state,
            host,
            returned: false,
        };
        let outcome = vm.exec_block_body(info.body);
        if vm.returned {
            BlockOutcome::Returned
        } else {
            outcome
        }
    }

    /// Run one named block; blocks of any other type are skipped whole.
    pub fn run_block(
        &self,
        block_type: u16,
        state: &mut VmState,
        host: &mut dyn ObScriptHost,
    ) -> BlockOutcome {
        let mut vm = Vm {
            bytes: &self.script.compiled,
            refs: &self.script.refs,
            locals: &self.script.locals,
            state,
            host,
            returned: false,
        };
        let bytes = vm.bytes;
        let mut offset = 0usize;
        loop {
            let Some(op) = byroredux_core::bytes::le_u16_at(bytes, offset) else {
                // Stream exhausted without the requested block (ScriptName-
                // only scripts, or a later block type) — nothing to run.
                return BlockOutcome::Completed;
            };
            if op == 0x10 {
                let Some((payload_start, payload_end)) = payload_span(bytes, offset) else {
                    return BlockOutcome::Malformed(offset);
                };
                // Payload: [block_type u16][body_len u32].
                let this_block = byroredux_core::bytes::le_u16_at(bytes, payload_start).unwrap_or(0);
                offset = payload_end;
                if this_block == block_type {
                    let outcome = vm.exec_block_body(offset);
                    return if vm.returned {
                        BlockOutcome::Returned
                    } else {
                        outcome
                    };
                }
                let Some(after) = vm.skip_to_block_end(offset) else {
                    return BlockOutcome::Malformed(offset);
                };
                offset = after;
                continue;
            }
            offset = match next_statement(bytes, offset, op) {
                Some(o) => o,
                None => return BlockOutcome::Malformed(offset),
            };
        }
    }
}

struct Vm<'a> {
    bytes: &'a [u8],
    /// The script's reference list: `SCRO` and `SCRV` interleaved in
    /// subrecord order, which is what compiled reference indices address.
    refs: &'a [ScriptRef],
    /// The script's declared locals, for the commands that read variables by name (`TrapUpdate`).
    locals: &'a [byroredux_plugin::esm::records::ScriptLocalVar],
    state: &'a mut VmState,
    host: &'a mut dyn ObScriptHost,
    /// Set by a `Return` hit anywhere in the block (including inside
    /// if-arms — the standard early-exit idiom in vanilla scripts); every
    /// executor loop unwinds when it sees this.
    returned: bool,
}

/// Offset of the statement following the one at `offset`.
fn next_statement(bytes: &[u8], offset: usize, op: u16) -> Option<usize> {
    if op == 0x1c {
        // [1c][ref u16][cmd u16][len u16][args]
        let end = offset + 8 + byroredux_core::bytes::le_u16_at(bytes, offset + 6)? as usize;
        return (end <= bytes.len()).then_some(end);
    }
    let len = byroredux_core::bytes::le_u16_at(bytes, offset + 2)? as usize;
    let end = offset + 4 + len;
    (end <= bytes.len()).then_some(end)
}

fn payload_span(bytes: &[u8], offset: usize) -> Option<(usize, usize)> {
    let len = byroredux_core::bytes::le_u16_at(bytes, offset + 2)? as usize;
    let start = offset + 4;
    let end = start.checked_add(len)?;
    (end <= bytes.len()).then_some((start, end))
}

fn is_flow_op(op: u16) -> bool {
    matches!(
        op,
        0x10 | 0x11 | 0x12 | 0x13 | 0x14 | 0x16 | 0x17 | 0x18 | 0x19 | 0x1d | 0x1e
    )
}

/// The printable argument/expression escape tags recovered from the corpus.
/// Any other first byte in an argument stream is a u16 integer literal.
fn is_escape_tag(b: u8) -> bool {
    matches!(
        b,
        b'G' | b'X' | b'Y' | b'Z' | b'f' | b'n' | b'r' | b's' | b'z'
    )
}

impl<'a> Vm<'a> {
    /// Execute statements until this block's `End`.
    fn exec_block_body(&mut self, mut offset: usize) -> BlockOutcome {
        loop {
            if self.returned {
                return BlockOutcome::Returned;
            }
            let Some(op) = byroredux_core::bytes::le_u16_at(self.bytes, offset) else {
                return BlockOutcome::Malformed(offset);
            };
            match op {
                0x11 => return BlockOutcome::Completed,
                0x1e => {
                    self.returned = true;
                    return BlockOutcome::Returned;
                }
                0x16 => {
                    let Some(after) = self.exec_if_chain(offset, 1) else {
                        return BlockOutcome::Malformed(offset);
                    };
                    offset = after;
                }
                _ => {
                    let Some(after) = self.run_or_skip_statement(op, offset, true) else {
                        return BlockOutcome::Malformed(offset);
                    };
                    offset = after;
                }
            }
        }
    }

    /// Run (`run`) or step over one statement. Returns the next offset.
    fn run_or_skip_statement(&mut self, op: u16, offset: usize, run: bool) -> Option<usize> {
        if op == 0x1c {
            let caller_ref = byroredux_core::bytes::le_u16_at(self.bytes, offset + 2)?;
            let cmd = byroredux_core::bytes::le_u16_at(self.bytes, offset + 4)?;
            let len = byroredux_core::bytes::le_u16_at(self.bytes, offset + 6)? as usize;
            let args_start = offset + 8;
            let args_end = args_start.checked_add(len)?;
            if args_end > self.bytes.len() {
                return None;
            }
            if run {
                // An explicit caller that does not resolve (a reference
                // variable the VM never assigns, or an out-of-range slot)
                // must not run as an implicit-caller command: `None` means
                // "the script's own owner" to the host, which would apply
                // `<unresolved>.Cmd` to the owning quest.
                let Some(caller) = self.resolve_ref_var(caller_ref) else {
                    self.host
                        .call(CMD_DECLINED, &[ObScriptValue::Num(f32::from(cmd))], None);
                    return Some(args_end);
                };
                let args = self.decode_args_for(cmd, args_start, args_end)?;
                self.host.call(cmd, &args, Some(caller));
                if self.host.ends_script() {
                    self.returned = true;
                }
            }
            return Some(args_end);
        }
        let (start, end) = payload_span(self.bytes, offset)?;
        if run {
            match op {
                0x15 => self.exec_set(start, end),
                CMD_MESSAGE | CMD_MESSAGE_BOX => match self.decode_message(start, end, op) {
                    Some(message) => self.host.message(message),
                    // Fallout's `ShowMessage <MESG> [values]` shares `0x1059` with Oblivion's
                    // text `Message` but names a message record: the form, then its values.
                    None if op == CMD_MESSAGE => match self.decode_show_message(start, end) {
                        Some(args) => {
                            self.host.call(op, &args, None);
                        }
                        None => {
                            self.host.call(CMD_DECLINED, &[ObScriptValue::Num(f32::from(op))], None);
                        }
                    },
                    None => {
                        self.host
                            .call(CMD_DECLINED, &[ObScriptValue::Num(f32::from(op))], None);
                    }
                },
                op if !is_flow_op(op) => {
                    // Implicit-caller statement call: the line opcode is
                    // the command id.
                    let args = self.decode_args_for(op, start, end)?;
                    self.host.call(op, &args, None);
                    if self.host.ends_script() {
                        self.returned = true;
                    }
                }
                _ => {}
            }
        }
        Some(end)
    }

    /// If/ElseIf/Else chain rooted at the `0x16`/`0x18` in `offset`. Only
    /// `0x16` nests; `0x17`/`0x18` are arms at this level. Returns the
    /// offset just past the chain's EndIf.
    fn exec_if_chain(&mut self, mut offset: usize, depth: usize) -> Option<usize> {
        if depth > MAX_OBSCRIPT_VM_NESTING {
            return None;
        }
        loop {
            let op = byroredux_core::bytes::le_u16_at(self.bytes, offset)?;
            match op {
                0x16 | 0x18 => {
                    // Arm header: [op][len][u16][u16 expr_len][expr].
                    let expr_len = byroredux_core::bytes::le_u16_at(self.bytes, offset + 6)? as usize;
                    let cond_start = offset + 8;
                    let cond_end = cond_start.checked_add(expr_len)?;
                    if cond_end > self.bytes.len() {
                        return None;
                    }
                    let taken = self.eval_expr(cond_start, cond_end) != 0.0;
                    let after = next_statement(self.bytes, offset, op)?;
                    if taken {
                        return self.run_arm_body(after, depth);
                    }
                    offset = self.skip_to_next_arm(after)?;
                }
                0x17 => {
                    // Else — its body is the arm that runs when nothing
                    // before it matched.
                    let after = next_statement(self.bytes, offset, op)?;
                    return self.run_arm_body(after, depth);
                }
                0x19 => return next_statement(self.bytes, offset, op),
                _ => return None,
            }
        }
    }

    /// Run a taken arm's body until the chain's EndIf. Nested `0x16` chains
    /// recurse; reaching a sibling `0x17`/`0x18` header means the body is
    /// complete — skip the remaining arms.
    fn run_arm_body(&mut self, mut offset: usize, depth: usize) -> Option<usize> {
        loop {
            if self.returned {
                // Return inside an arm exits the whole block — skip the
                // rest of the chain so the caller's offset stays valid.
                return self.skip_to_chain_end(offset);
            }
            let op = byroredux_core::bytes::le_u16_at(self.bytes, offset)?;
            match op {
                0x19 => return next_statement(self.bytes, offset, op),
                0x17 | 0x18 => return self.skip_to_chain_end(offset),
                0x16 => {
                    offset = self.exec_if_chain(offset, depth + 1)?;
                }
                0x1e => {
                    self.returned = true;
                    return self.skip_to_chain_end(next_statement(self.bytes, offset, op)?);
                }
                0x11 => return None, // End cannot appear inside an arm
                _ => offset = self.run_or_skip_statement(op, offset, true)?,
            }
        }
    }

    /// Skip statements until the next chain arm header (`0x17`/`0x18`) or
    /// the chain's EndIf, at this nesting level.
    fn skip_to_next_arm(&mut self, mut offset: usize) -> Option<usize> {
        let mut depth = 0usize;
        loop {
            let op = byroredux_core::bytes::le_u16_at(self.bytes, offset)?;
            match op {
                0x16 => depth += 1,
                0x17 | 0x18 if depth == 0 => return Some(offset),
                0x19 => {
                    if depth == 0 {
                        return Some(offset);
                    }
                    depth -= 1;
                }
                _ => {}
            }
            offset = self.run_or_skip_statement(op, offset, false)?;
        }
    }

    /// Skip statements until the EndIf closing this chain (nested `0x16`
    /// chains nest; sibling arms do not). Returns past the EndIf.
    fn skip_to_chain_end(&mut self, mut offset: usize) -> Option<usize> {
        let mut depth = 0usize;
        loop {
            let op = byroredux_core::bytes::le_u16_at(self.bytes, offset)?;
            match op {
                0x16 => depth += 1,
                0x19 => {
                    if depth == 0 {
                        return next_statement(self.bytes, offset, op);
                    }
                    depth -= 1;
                }
                _ => {}
            }
            offset = self.run_or_skip_statement(op, offset, false)?;
        }
    }

    /// Skip from just after a Begin to just past its matching End.
    fn skip_to_block_end(&mut self, mut offset: usize) -> Option<usize> {
        let mut depth = 1usize;
        loop {
            let op = byroredux_core::bytes::le_u16_at(self.bytes, offset)?;
            match op {
                0x10 => depth += 1,
                0x11 => {
                    depth -= 1;
                    if depth == 0 {
                        return next_statement(self.bytes, offset, op);
                    }
                }
                _ => {}
            }
            offset = self.run_or_skip_statement(op, offset, false)?;
        }
    }

    /// `set <ref>.var to expr`: `[r ref u16][s|f ordinal u16][u16 expr_len][expr]`.
    fn exec_remote_set(&mut self, start: usize, end: usize) {
        let declined = |vm: &mut Self| {
            vm.host.call(CMD_DECLINED, &[ObScriptValue::Num(f32::from(OP_SET))], None);
        };
        let (Some(ref_index), Some(&kind), Some(ordinal), Some(expr_len)) = (
            byroredux_core::bytes::le_u16_at(self.bytes, start + 1),
            self.bytes.get(start + 3),
            byroredux_core::bytes::le_u16_at(self.bytes, start + 4),
            byroredux_core::bytes::le_u16_at(self.bytes, start + 6),
        ) else {
            return declined(self);
        };
        let (expr_start, expr_end) = (start + 8, start + 8 + usize::from(expr_len));
        if !matches!(kind, b's' | b'f') || expr_end > end {
            return declined(self);
        }
        let value = self.eval_expr(expr_start, expr_end);
        let Some(form) = self.resolve_ref_var(ref_index) else {
            return declined(self);
        };
        // A `short` / `long` narrows, a `float` keeps the value.
        let value = if kind == b's' { value.trunc() } else { value };
        self.host.call(
            CMD_SET_REMOTE_VARIABLE,
            &[ObScriptValue::Ref(form), ObScriptValue::Num(f32::from(ordinal)), ObScriptValue::Num(value)],
            None,
        );
    }

    /// `Set` payload: `[target escape][u16 expr_len][expr]`. Targets:
    /// `' s<n>'`/`' f<n>'` locals (narrowed per declared type) and
    /// `' G<n>'` globals (routed to the host as a global write).
    ///
    /// A tag-`r` target is NOT a reference variable: the 6-byte escape is
    /// `r<ref u16> <s|f><variable ordinal u16>` — a **remote variable**,
    /// `set Quest.var to expr`. In all 1 646 Oblivion / 4 240 FNV / 1 290
    /// FO3 vanilla `Set`s of that shape the trailing bytes are `s`/`f` plus
    /// a nonzero ordinal (not the zeroed "secondary index" this doc once
    /// claimed). The variable lives in another script: the host writes it
    /// ([`CMD_SET_REMOTE_VARIABLE`]) and a write it cannot make is declined and
    /// counted (#5630) — storing the value as the object's identity, as this
    /// used to, made every later `' r<n>'` naming that object resolve to a small
    /// integer.
    fn exec_set(&mut self, start: usize, end: usize) {
        let Some(&tag) = self.bytes.get(start) else {
            return;
        };
        if tag == b'r' {
            self.exec_remote_set(start, end);
            return;
        }
        // Target escape: `'s'+idx` is 3 bytes (the leading space of the
        // *expression* escapes is absent here — corpus: `73 07 00 | 05 00 |
        // " 1200"`), then `[u16 expr_len][expr]`.
        let idx = byroredux_core::bytes::le_u16_at(self.bytes, start + 1).unwrap_or(0);
        let expr_len_at = match tag {
            b's' | b'f' | b'G' => start + 3,
            _ => return,
        };
        if expr_len_at + 2 > end {
            return;
        }
        let expr_len = byroredux_core::bytes::le_u16_at(self.bytes, expr_len_at).unwrap_or(0) as usize;
        let expr_start = expr_len_at + 2;
        let expr_end = expr_start + expr_len;
        if expr_end > end {
            return;
        }
        let result = self.eval_expr_value(expr_start, expr_end);
        let value = result.as_num();
        // A reference variable keeps the exact reference: the number alone would
        // lose the low bits of a DLC form id.
        if matches!(tag, b's' | b'f') && self.is_ref_slot(idx) {
            let form = match result {
                ObScriptValue::Ref(form) => form,
                ObScriptValue::Num(n) if n >= 1.0 => n as u32,
                ObScriptValue::Num(_) => 0,
            };
            self.state.ref_locals.insert(idx, form);
            self.state.locals.insert(idx, form as f32);
            return;
        }
        match tag {
            b's' => {
                // short/long locals narrow; float locals keep the f32.
                self.state.locals.insert(idx, value.trunc());
            }
            b'f' => {
                self.state.locals.insert(idx, value);
            }
            b'G' => {
                if let Some(form) = self.resolve_ref_var(idx) {
                    self.host
                        .call(CMD_SET_GLOBAL, &[ObScriptValue::Num(value)], Some(form));
                }
            }
            _ => {}
        }
    }

    /// Evaluate an RPN expression token stream `[start, end)` to a numeric
    /// result (nonzero = true for conditions).
    fn eval_expr(&mut self, start: usize, end: usize) -> f32 {
        self.eval_expr_value(start, end).as_num()
    }

    /// [`Self::eval_expr`] keeping what the expression produced: a reference
    /// stays a reference (`set rTarget to GetLinkedRef`).
    fn eval_expr_value(&mut self, start: usize, end: usize) -> ObScriptValue {
        let mut stack: Vec<ObScriptValue> = Vec::with_capacity(8);
        let bytes = &self.bytes[start..end];
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == 0x20 && i + 1 < bytes.len() && is_escape_tag(bytes[i + 1]) {
                let tag = bytes[i + 1];
                let consumed = match tag {
                    // `' r<ref>'` + `'X'` + `<cmd u16> <len u16> <args>`: a call on a
                    // reference inside an expression (`Actor.GetDead`, `player.GetItemCount x`).
                    // The `X` has no space before it; an implicit-caller call is `' X'`.
                    b'r' if i + 9 <= bytes.len() && bytes[i + 4] == b'X' => {
                        let idx = byroredux_core::bytes::le_u16_at(bytes, i + 2).unwrap_or(0);
                        let cmd = byroredux_core::bytes::le_u16_at(bytes, i + 5).unwrap_or(0);
                        let arg_len = byroredux_core::bytes::le_u16_at(bytes, i + 7).unwrap_or(0) as usize;
                        let arg_start = i + 9;
                        let Some(arg_end) = arg_start.checked_add(arg_len) else {
                            break;
                        };
                        if arg_end > bytes.len() {
                            break;
                        }
                        let Some(args) = self.decode_args_for(cmd, start + arg_start, start + arg_end) else {
                            self.host.call(CMD_DECLINED, &[ObScriptValue::Num(f32::from(cmd))], None);
                            i += 9 + arg_len;
                            stack.push(ObScriptValue::Num(0.0));
                            continue;
                        };
                        // A caller that is not a reference (an unassigned variable) must not
                        // run as an implicit-caller call: `None` means the script's owner.
                        let result = match self.resolve_ref_var(idx) {
                            Some(caller) => self.host.call(cmd, &args, Some(caller)),
                            None => {
                                self.host.call(CMD_DECLINED, &[ObScriptValue::Num(f32::from(cmd))], None);
                                None
                            }
                        };
                        stack.push(result.unwrap_or(ObScriptValue::Num(0.0)));
                        Some(9 + arg_len)
                    }
                    // `' r<ref>'` + `'s'|'f'` + `<ordinal>`: a remote variable read
                    // (`Quest.var`), seven bytes. A reference on its own is the four.
                    b'r' if i + 7 <= bytes.len() && matches!(bytes[i + 4], b's' | b'f') => {
                        let idx = byroredux_core::bytes::le_u16_at(bytes, i + 2).unwrap_or(0);
                        let ordinal = byroredux_core::bytes::le_u16_at(bytes, i + 5).unwrap_or(0);
                        let value = match self.resolve_ref_var(idx) {
                            Some(form) => self
                                .host
                                .call(
                                    CMD_GET_REMOTE_VARIABLE,
                                    &[ObScriptValue::Ref(form), ObScriptValue::Num(f32::from(ordinal))],
                                    None,
                                )
                                .map_or(0.0, |v| v.as_num()),
                            None => {
                                self.host.call(CMD_DECLINED, &[ObScriptValue::Num(f32::from(CMD_GET_REMOTE_VARIABLE))], None);
                                0.0
                            }
                        };
                        stack.push(ObScriptValue::Num(value));
                        Some(7)
                    }
                    // `' r<ref>'` and `' Z<ref>'` on their own are a reference as a value
                    // (`Set rActor to SomeREF` is the `Z` form on the vanilla masters:
                    // `20 5a 16 00`; `GetActionRef == SomeREF` compares one).
                    b'r' | b'Z' if i + 4 <= bytes.len() => {
                        let idx = byroredux_core::bytes::le_u16_at(bytes, i + 2).unwrap_or(0);
                        let resolved = self.resolve_ref_var(idx).unwrap_or(0);
                        stack.push(ObScriptValue::Ref(resolved));
                        Some(4)
                    }
                    b's' | b'f' if i + 4 <= bytes.len() => {
                        let idx = byroredux_core::bytes::le_u16_at(bytes, i + 2).unwrap_or(0);
                        stack.push(ObScriptValue::Num(
                            self.state.locals.get(&idx).copied().unwrap_or(0.0),
                        ));
                        Some(4)
                    }
                    b'G' if i + 4 <= bytes.len() => {
                        let idx = byroredux_core::bytes::le_u16_at(bytes, i + 2).unwrap_or(0);
                        let form = self.resolve_ref_var(idx).unwrap_or(0);
                        let value = self
                            .host
                            .call(CMD_GET_GLOBAL, &[ObScriptValue::Ref(form)], Some(form))
                            .map(|v| v.as_num())
                            .unwrap_or(0.0);
                        stack.push(ObScriptValue::Num(value));
                        Some(4)
                    }
                    b'n' if i + 6 <= bytes.len() => {
                        let raw = i32::from_le_bytes([
                            bytes[i + 2],
                            bytes[i + 3],
                            bytes[i + 4],
                            bytes[i + 5],
                        ]);
                        stack.push(ObScriptValue::Num(raw as f32));
                        Some(6)
                    }
                    b'X' if i + 6 <= bytes.len() => {
                        let cmd = byroredux_core::bytes::le_u16_at(bytes, i + 2).unwrap_or(0);
                        let arg_len = byroredux_core::bytes::le_u16_at(bytes, i + 4).unwrap_or(0) as usize;
                        let arg_start = i + 6;
                        let Some(arg_end) = arg_start.checked_add(arg_len) else {
                            break;
                        };
                        if arg_end > bytes.len() {
                            break;
                        }
                        // `i` is slice-relative; decode_args indexes the
                        // whole script buffer, so translate to absolute.
                        let Some(args) = self.decode_args_for(cmd, start + arg_start, start + arg_end)
                        else {
                            // Arguments this VM cannot frame: counted, never run on guesses.
                            self.host.call(CMD_DECLINED, &[ObScriptValue::Num(f32::from(cmd))], None);
                            i += 6 + arg_len;
                            stack.push(ObScriptValue::Num(0.0));
                            continue;
                        };
                        let result = self.host.call(cmd, &args, None);
                        stack.push(result.unwrap_or(ObScriptValue::Num(0.0)));
                        Some(6 + arg_len)
                    }
                    _ => None,
                };
                if let Some(step) = consumed {
                    i += step;
                    continue;
                }
            }
            // ASCII run up to the next escape.
            let run_start = i;
            while i < bytes.len()
                && !(bytes[i] == 0x20 && i + 1 < bytes.len() && is_escape_tag(bytes[i + 1]))
            {
                i += 1;
            }
            if i == run_start {
                i += 1;
                continue;
            }
            for token in bytes[run_start..i].split(|&c| c == b' ') {
                if token.is_empty() {
                    continue;
                }
                let text = std::str::from_utf8(token).unwrap_or("");
                if let Ok(n) = text.parse::<f32>() {
                    stack.push(ObScriptValue::Num(n));
                } else {
                    apply_operator(&mut stack, text);
                }
            }
        }
        stack.last().copied().unwrap_or(ObScriptValue::Num(0.0))
    }

    /// Decode `[u16 count][count typed args]` call arguments.
    /// [`Self::decode_args`] for command `cmd`: the commands whose first parameter is an *axis*
    /// (`GetPos x`, `SetAngle z 90`) write it as one bare letter — `X`, `Y`, `Z` — which the
    /// generic decoder would take for the nested-call tag and the two zero-payload tags. Here the
    /// letter is the axis, answered as `0`, `1`, `2`.
    fn decode_args_for(&mut self, cmd: u16, start: usize, end: usize) -> Option<Vec<ObScriptValue>> {
        if STRING_FORM_COMMANDS.contains(&cmd) {
            return self.decode_string_form(cmd, start, end);
        }
        if REF_STRINGS_COMMANDS.contains(&cmd) {
            return self.decode_ref_strings(cmd, start, end);
        }
        if cmd == CMD_TRAP_UPDATE {
            return Some(self.trap_variables());
        }
        if !AXIS_COMMANDS.contains(&cmd) || start + 3 > end {
            return self.decode_args(start, end);
        }
        let count = byroredux_core::bytes::le_u16_at(self.bytes, start)?;
        let axis = match self.bytes[start + 2] {
            b'X' | b'x' => 0.0,
            b'Y' | b'y' => 1.0,
            b'Z' | b'z' => 2.0,
            _ => return self.decode_args(start, end),
        };
        let mut args = vec![ObScriptValue::Num(axis)];
        let mut o = start + 3;
        for _ in 1..count {
            let (value, next) = self.decode_arg_item(o, end, 0)?;
            args.push(value.unwrap_or(ObScriptValue::Num(0.0)));
            o = next;
        }
        Some(args)
    }

    /// A command whose one argument is an editor ID written as `[u16 1][u16 length][bytes]`
    /// (`PlayIdle LooseDogGrowl`: `01 00 0d 00 "LooseDogGrowl"`), answered as the form the host says it
    /// names — `Ref(0)` when it names none, so the host declines the call.
    fn decode_string_form(&mut self, cmd: u16, start: usize, end: usize) -> Option<Vec<ObScriptValue>> {
        if start == end {
            return Some(Vec::new());
        }
        let count = byroredux_core::bytes::le_u16_at(self.bytes, start)?;
        let length = usize::from(byroredux_core::bytes::le_u16_at(self.bytes, start + 2)?);
        let text_start = start + 4;
        let text_end = text_start.checked_add(length)?;
        if count == 0 || text_end > end {
            return self.decode_args_at_depth(start, end, 0);
        }
        // The arguments after the string (`ModPCMiscStat "Locks Picked" 1`): ordinary ones.
        let mut rest = Vec::new();
        let mut offset = text_end;
        for _ in 1..count {
            let (value, next) = self.decode_arg_item(offset, end, 0)?;
            rest.extend(value);
            offset = next;
        }
        let text = String::from_utf8_lossy(&self.bytes[text_start..text_end]).into_owned();
        let value = self.host.text_argument(cmd, &text, &rest).unwrap_or(ObScriptValue::Ref(0));
        let mut args = vec![value];
        args.extend(rest);
        Some(args)
    }

    /// A command of references and bare strings (`72 05 00 | 08 00 "##GS01:0" | 1e 00 "interface\..."`): the
    /// references are the arguments, the strings go to [`ObScriptHost::text_arguments`].
    fn decode_ref_strings(&mut self, cmd: u16, start: usize, end: usize) -> Option<Vec<ObScriptValue>> {
        if start == end {
            return Some(Vec::new());
        }
        let count = byroredux_core::bytes::le_u16_at(self.bytes, start)?;
        let mut offset = start + 2;
        let (mut args, mut texts) = (Vec::new(), Vec::new());
        // The reference first, then the strings.
        if count > 0 {
            let (value, next) = self.decode_arg_item(offset, end, 0)?;
            args.extend(value);
            offset = next;
        }
        for _ in 1..count {
            let length = usize::from(byroredux_core::bytes::le_u16_at(self.bytes, offset)?);
            let text_end = (offset + 2).checked_add(length).filter(|&text_end| text_end <= end)?;
            texts.push(String::from_utf8_lossy(&self.bytes[offset + 2..text_end]).into_owned());
            offset = text_end;
        }
        self.host.text_arguments(cmd, &texts);
        Some(args)
    }

    /// `TrapUpdate` takes no argument: the trap is described by variables its own script declares by
    /// name (CS *TrapUpdate*): `fTrapDamage`, `fTrapPushBack`, `fTrapDeathPushBack`, `fTrapMinVelocity`,
    /// `bTrapContinuous`, `fLevelledDamage`. Their current values, in that order (0 for one the script
    /// does not declare).
    fn trap_variables(&self) -> Vec<ObScriptValue> {
        TRAP_VARIABLES
            .iter()
            .map(|name| {
                let value = self
                    .locals
                    .iter()
                    .find(|local| local.name.eq_ignore_ascii_case(name))
                    .and_then(|local| u16::try_from(local.index).ok())
                    .and_then(|slot| self.state.locals.get(&slot).copied())
                    .unwrap_or(0.0);
                ObScriptValue::Num(value)
            })
            .collect()
    }

    fn decode_args(&mut self, start: usize, end: usize) -> Option<Vec<ObScriptValue>> {
        self.decode_args_at_depth(start, end, 0)
    }

    fn decode_args_at_depth(
        &mut self,
        start: usize,
        end: usize,
        depth: usize,
    ) -> Option<Vec<ObScriptValue>> {
        if depth > MAX_OBSCRIPT_VM_NESTING {
            return None;
        }
        let mut args = Vec::new();
        if start == end {
            return Some(args);
        }
        // #5655 — every read below is bounded by the *argument region*
        // `[start, end)`, not by the script buffer: the `le_u16_at` family
        // only knows the buffer, so an argument whose declared payload
        // straddles `end` would otherwise decode the next statement's bytes
        // as its own and hand the host a garbage argument instead of
        // reporting `Malformed`.
        if start + 2 > end {
            return None;
        }
        let count = byroredux_core::bytes::le_u16_at(self.bytes, start)?;
        let mut o = start + 2;
        for _ in 0..count {
            let (value, next) = self.decode_arg_item(o, end, depth)?;
            args.extend(value);
            o = next;
        }
        Some(args)
    }

    /// Fallout's `ShowMessage <message> [value ...]`: `[u16 1][the message form][u16 n][n values]`
    /// then a display-time word the engine does not use (`59 10 0b 00 | 01 00 72 01 00 | 00 00 00 00
    /// 00 00` on the masters). The form and the values, or `None` when the payload is not that shape.
    fn decode_show_message(&mut self, start: usize, end: usize) -> Option<Vec<ObScriptValue>> {
        if byroredux_core::bytes::le_u16_at(self.bytes, start)? != 1 {
            return None;
        }
        let (message, mut o) = self.decode_arg_item(start + 2, end, 0)?;
        let Some(ObScriptValue::Ref(form)) = message else {
            return None;
        };
        let mut args = vec![ObScriptValue::Ref(form)];
        if o + 2 <= end {
            let count = byroredux_core::bytes::le_u16_at(self.bytes, o)?;
            o += 2;
            for _ in 0..count {
                let (value, next) = self.decode_arg_item(o, end, 0)?;
                args.push(value.unwrap_or(ObScriptValue::Num(0.0)));
                o = next;
            }
        }
        Some(args)
    }

    /// A `Message` / `MessageBox` payload (see [`ObScriptMessage`]).
    fn decode_message(&mut self, start: usize, end: usize, op: u16) -> Option<ObScriptMessage> {
        let read_string = |vm: &Self, o: usize| -> Option<(String, usize)> {
            let len = usize::from(byroredux_core::bytes::le_u16_at(vm.bytes, o)?);
            let text_end = (o + 2).checked_add(len)?;
            if text_end > end {
                return None;
            }
            // Windows-1252 text; its ASCII range is all vanilla uses.
            let text = vm.bytes[o + 2..text_end].iter().map(|&c| c as char).collect();
            Some((text, text_end))
        };
        if byroredux_core::bytes::le_u16_at(self.bytes, start)? != 1 {
            return None;
        }
        let (text, mut o) = read_string(self, start + 2)?;
        let mut values = Vec::new();
        if o + 2 <= end {
            let count = byroredux_core::bytes::le_u16_at(self.bytes, o)?;
            o += 2;
            for _ in 0..count {
                let (value, next) = self.decode_arg_item(o, end, 0)?;
                values.push(value.map_or(0.0, |v| v.as_num()));
                o = next;
            }
        }
        let message_box = op == CMD_MESSAGE_BOX;
        let mut buttons = Vec::new();
        if message_box && o + 2 <= end {
            let count = byroredux_core::bytes::le_u16_at(self.bytes, o)?;
            o += 2;
            for _ in 0..count {
                let (button, next) = read_string(self, o + 2)?;
                buttons.push(button);
                o = next;
            }
        }
        Some(ObScriptMessage {
            text,
            values,
            buttons,
            message_box,
        })
    }

    /// One typed argument at `o` (bounded by `end`): its value — `None` for
    /// a string literal, which phase-1 hosts do not receive — and the offset
    /// just past it.
    fn decode_arg_item(
        &mut self,
        o: usize,
        end: usize,
        depth: usize,
    ) -> Option<(Option<ObScriptValue>, usize)> {
        let mut o = o;
        let value;
        if o >= end {
            return None;
        }
        let tag = self.bytes[o];
        match tag {
            b'r' => {
                if o + 3 > end {
                    return None;
                }
                let idx = byroredux_core::bytes::le_u16_at(self.bytes, o + 1)?;
                let form = self.resolve_ref_var(idx).unwrap_or(0);
                value = Some(ObScriptValue::Ref(form));
                o += 3;
            }
            b's' | b'f' => {
                if o + 3 > end {
                    return None;
                }
                let idx = byroredux_core::bytes::le_u16_at(self.bytes, o + 1)?;
                value = Some(ObScriptValue::Num(
                    self.state.locals.get(&idx).copied().unwrap_or(0.0),
                ));
                o += 3;
            }
            b'G' => {
                if o + 3 > end {
                    return None;
                }
                let idx = byroredux_core::bytes::le_u16_at(self.bytes, o + 1)?;
                let form = self.resolve_ref_var(idx).unwrap_or(0);
                let global = self
                    .host
                    .call(CMD_GET_GLOBAL, &[ObScriptValue::Ref(form)], Some(form))
                    .map(|v| v.as_num())
                    .unwrap_or(0.0);
                value = Some(ObScriptValue::Num(global));
                o += 3;
            }
            b'n' => {
                if o + 5 > end {
                    return None;
                }
                let raw = i32::from_le_bytes([
                    self.bytes[o + 1],
                    self.bytes[o + 2],
                    self.bytes[o + 3],
                    self.bytes[o + 4],
                ]);
                value = Some(ObScriptValue::Num(raw as f32));
                o += 5;
            }
            b'z' => {
                // Float literal: `7A` + a 64-bit double (UESP *Oblivion Mod:
                // Mod File Format/SCPT* §SCDA `[FloatFuncParam]`; the sibling
                // decoder in `obscript.rs` reads it the same way). FNV
                // `SetQuestDelay VERStalkersSouth01 0.1` compiles to
                // `7a 9a 99 99 99 99 99 b9 3f`. This arm used to read a
                // NUL-terminated string, so any script with a float literal
                // argument either misframed or came back `Malformed`.
                if o + 9 > end {
                    return None;
                }
                let mut raw = [0u8; 8];
                raw.copy_from_slice(&self.bytes[o + 1..o + 9]);
                value = Some(ObScriptValue::Num(f64::from_le_bytes(raw) as f32));
                o += 9;
            }
            b'Y' | b'Z' => {
                // Zero-payload tags ('Y' = the calling reference, 'Z' =
                // a null reference in every corpus context).
                value = Some(ObScriptValue::Ref(0));
                o += 1;
            }
            b'X' => {
                if o + 5 > end {
                    return None;
                }
                let cmd = byroredux_core::bytes::le_u16_at(self.bytes, o + 1)?;
                let inner_len = byroredux_core::bytes::le_u16_at(self.bytes, o + 3)? as usize;
                let inner_start = o + 5;
                let inner_end = inner_start.checked_add(inner_len)?;
                if inner_end > end {
                    return None;
                }
                let inner = self.decode_args_at_depth(inner_start, inner_end, depth + 1)?;
                let result = self.host.call(cmd, &inner, None);
                value = Some(result.unwrap_or(ObScriptValue::Num(0.0)));
                o = inner_end;
            }
            _ => {
                // u16 integer literal.
                if o + 2 > end {
                    return None;
                }
                let v = byroredux_core::bytes::le_u16_at(self.bytes, o)?;
                value = Some(ObScriptValue::Num(v as f32));
                o += 2;
            }
        }
        Some((value, o))
    }

    /// Resolve a 1-based reference index against the script's reference
    /// list (`SCRO` and `SCRV` interleaved, #5629). An object entry yields
    /// its form; a reference-variable entry yields `None` — the VM models
    /// no ref-valued command, so a ref local is never assigned a real
    /// reference and resolving it to anything would be a guess. So does an
    /// out-of-range index. Callers decline or read the null reference.
    ///
    /// A reference variable resolves to the reference its slot was last assigned
    /// ([`VmState::ref_locals`]); one never assigned (or assigned none) is `None`,
    /// as is one a host seeded only as a number that is not a form id.
    fn resolve_ref_var(&self, index: u16) -> Option<u32> {
        match self.refs.get(usize::from(index).checked_sub(1)?)? {
            ScriptRef::Object(form) => Some(*form),
            ScriptRef::Variable(slot) => {
                let slot = u16::try_from(*slot).ok()?;
                self.state.ref_locals.get(&slot).copied().filter(|&form| form != 0)
            }
        }
    }

    /// Whether local `slot` is one the script uses as a reference variable.
    fn is_ref_slot(&self, slot: u16) -> bool {
        self.refs.iter().any(|entry| matches!(entry, ScriptRef::Variable(s) if *s == u32::from(slot)))
    }
}

fn apply_operator(stack: &mut Vec<ObScriptValue>, token: &str) {
    let pop = |stack: &mut Vec<ObScriptValue>| stack.pop().map(|v| v.as_num()).unwrap_or(0.0);
    let truthy = |v: f32| v != 0.0;
    let flag = |b: bool| ObScriptValue::Num(b as i32 as f32);
    let value = match token {
        ">=" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            flag(lhs >= rhs)
        }
        "<=" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            flag(lhs <= rhs)
        }
        // Two references compare by their form ids, exactly: as `f32`s the ids past `2^24`
        // (every DLC form) would round into one another.
        "==" | "!=" => {
            let (rhs, lhs) = (stack.pop(), stack.pop());
            let equal = match (lhs, rhs) {
                (Some(ObScriptValue::Ref(a)), Some(ObScriptValue::Ref(b))) => a == b,
                _ => {
                    let num = |v: Option<ObScriptValue>| v.map_or(0.0, |v| v.as_num());
                    (num(lhs) - num(rhs)).abs() < f32::EPSILON
                }
            };
            flag(equal == (token == "=="))
        }
        ">" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            flag(lhs > rhs)
        }
        "<" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            flag(lhs < rhs)
        }
        "&&" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            flag(truthy(lhs) && truthy(rhs))
        }
        "||" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            flag(truthy(lhs) || truthy(rhs))
        }
        "+" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            ObScriptValue::Num(lhs + rhs)
        }
        "-" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            ObScriptValue::Num(lhs - rhs)
        }
        "*" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            ObScriptValue::Num(lhs * rhs)
        }
        "/" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            if rhs != 0.0 {
                ObScriptValue::Num(lhs / rhs)
            } else {
                ObScriptValue::Num(0.0)
            }
        }
        "!" => flag(!truthy(pop(stack))),
        _ => {
            // Unknown operator token: push a neutral zero to keep the
            // stack balanced. The corpus census covers every operator
            // vanilla quest scripts use, so this arm is defensive.
            ObScriptValue::Num(0.0)
        }
    };
    stack.push(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::{ScriptLocalVar, ScriptRecord, ScriptRef, ScriptType};

    /// Recording host stub — captures every dispatched call.
    #[derive(Default)]
    struct Recorder {
        calls: Vec<(u16, Vec<ObScriptValue>, Option<u32>)>,
        globals: std::collections::HashMap<u32, f32>,
        messages: Vec<ObScriptMessage>,
    }

    impl ObScriptHost for Recorder {
        fn message(&mut self, message: ObScriptMessage) {
            self.messages.push(message);
        }

        fn call(
            &mut self,
            cmd: u16,
            args: &[ObScriptValue],
            caller: Option<u32>,
        ) -> Option<ObScriptValue> {
            self.calls.push((cmd, args.to_vec(), caller));
            match cmd {
                CMD_GET_GLOBAL => {
                    let form = match args.first() {
                        Some(ObScriptValue::Ref(r)) => *r,
                        Some(ObScriptValue::Num(n)) => *n as u32,
                        None => 0,
                    };
                    Some(ObScriptValue::Num(
                        self.globals.get(&form).copied().unwrap_or(0.0),
                    ))
                }
                0x103a => Some(ObScriptValue::Num(42.0)), // GetStage stub
                _ => Some(ObScriptValue::Num(0.0)),
            }
        }
    }

    fn script(compiled: Vec<u8>, locals: Vec<ScriptLocalVar>, refs: Vec<u32>) -> ScriptRecord {
        ScriptRecord {
            form_id: 1,
            editor_id: "TestQuestScript".into(),
            num_refs: refs.len() as u32,
            compiled_size: compiled.len() as u32,
            var_count: locals.len() as u32,
            script_type: ScriptType::Quest,
            flags: 0,
            compiled,
            source: None,
            locals,
            refs: refs.iter().copied().map(ScriptRef::Object).collect(),
            ref_form_ids: refs,
            ref_var_indices: Vec::new(),
        }
    }

    fn local(index: u32, name: &str, var_type: u8) -> ScriptLocalVar {
        ScriptLocalVar {
            index,
            var_type,
            name: name.into(),
        }
    }

    /// `set <target> to <expr>` statement bytes. `pad` extends the target
    /// escape for the 6-byte `' r<n>'` form (its trailing secondary u16).
    fn set_stmt(var_tag: u8, idx: u16, expr: &[u8], pad: usize) -> Vec<u8> {
        let mut payload = vec![var_tag];
        payload.extend_from_slice(&idx.to_le_bytes());
        payload.extend(std::iter::repeat_n(0u8, pad)); // 'r' secondary u16
        payload.extend_from_slice(&(expr.len() as u16).to_le_bytes());
        payload.extend_from_slice(expr);
        let mut stmt = (0x15u16).to_le_bytes().to_vec();
        stmt.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        stmt.extend_from_slice(&payload);
        stmt
    }

    /// If/ElseIf header bytes around `expr`.
    fn arm_stmt(op: u16, expr: &[u8]) -> Vec<u8> {
        let mut payload = vec![0u8, 0]; // [u16 prefix]
        payload.extend_from_slice(&(expr.len() as u16).to_le_bytes());
        payload.extend_from_slice(expr);
        let mut stmt = op.to_le_bytes().to_vec();
        stmt.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        stmt.extend_from_slice(&payload);
        stmt
    }

    fn simple_stmt(op: u16) -> Vec<u8> {
        let mut stmt = op.to_le_bytes().to_vec();
        stmt.extend_from_slice(&0u16.to_le_bytes());
        stmt
    }

    fn begin(block: u16, body_len: u32) -> Vec<u8> {
        let mut payload = block.to_le_bytes().to_vec();
        payload.extend_from_slice(&body_len.to_le_bytes());
        let mut stmt = (0x10u16).to_le_bytes().to_vec();
        stmt.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        stmt.extend_from_slice(&payload);
        stmt
    }

    /// The block types a script has, in order, without running it — including
    /// a call statement and an `If` inside the first block, which the walk must
    /// frame past — and `None` for a truncated stream.
    #[test]
    fn block_types_lists_the_blocks_in_order_and_refuses_a_malformed_stream() {
        let mut bytes = (0x1du16).to_le_bytes().to_vec();
        bytes.extend_from_slice(&0u16.to_le_bytes());
        for (block, body) in [(0x15u16, vec![]), (BLOCK_GAME_MODE, simple_stmt(0x1000)), (0x02, vec![])] {
            bytes.extend(begin(block, body.len() as u32));
            bytes.extend(body);
            bytes.extend(simple_stmt(0x11));
        }
        let record = script(bytes.clone(), Vec::new(), Vec::new());
        assert_eq!(ObScriptProgram::new(&record).block_types(), Some(vec![0x15, 0x00, 0x02]));
        bytes.truncate(bytes.len() - 3);
        let record = script(bytes, Vec::new(), Vec::new());
        assert_eq!(ObScriptProgram::new(&record).block_types(), None);
        let none = script(Vec::new(), Vec::new(), Vec::new());
        assert_eq!(ObScriptProgram::new(&none).block_types(), Some(Vec::new()));
    }

    fn wrap_body(body: Vec<u8>) -> Vec<u8> {
        let mut bytes = (0x1du16).to_le_bytes().to_vec();
        bytes.extend_from_slice(&0u16.to_le_bytes()); // ScriptName, len 0
        bytes.extend(begin(BLOCK_GAME_MODE, body.len() as u32));
        bytes.extend(body);
        bytes.extend(simple_stmt(0x11)); // End
        bytes
    }

    const SETSTAGE: u16 = 0x1039;
    const GETSTAGE: u16 = 0x103a;

    /// Float-literal argument: `'z'` + a little-endian f64.
    fn za(v: f64) -> Vec<u8> {
        let mut bytes = vec![b'z'];
        bytes.extend_from_slice(&v.to_le_bytes());
        bytes
    }

    /// A `z` float literal is an 8-byte double and the statement after it
    /// still runs (FNV `SetQuestDelay <quest> 0.1`, command `0x11dc`).
    #[test]
    fn float_literal_argument_is_a_double_and_keeps_framing() {
        const SET_QUEST_DELAY: u16 = 0x11dc;
        let args = args_blob(&[ra(1), za(0.1)]);
        let mut body = Vec::new();
        // Implicit-caller statement: the line opcode is the command id.
        body.extend((SET_QUEST_DELAY).to_le_bytes());
        body.extend((args.len() as u16).to_le_bytes());
        body.extend(args);
        // A statement after it proves the framing survived.
        let after = args_blob(&[ra(1), vec![b'n', 7, 0, 0, 0]]);
        body.extend(SETSTAGE.to_le_bytes());
        body.extend((after.len() as u16).to_le_bytes());
        body.extend(after);

        let record = script(wrap_body(body), Vec::new(), vec![0x00AA_0001]);
        let mut host = Recorder::default();
        let outcome = ObScriptProgram::new(&record).run_block(
            BLOCK_GAME_MODE,
            &mut VmState::default(),
            &mut host,
        );
        assert_eq!(outcome, BlockOutcome::Completed);
        assert_eq!(host.calls.len(), 2);
        assert_eq!(host.calls[0].0, SET_QUEST_DELAY);
        let delay = host.calls[0].1[1].as_num();
        assert!((delay - 0.1).abs() < 1e-6, "delay decoded as {delay}");
        assert_eq!(host.calls[1].0, SETSTAGE);
        assert_eq!(host.calls[1].1[1].as_num(), 7.0);
    }

    /// Local escape bytes: `' s<n>'`.
    fn s(n: u16) -> Vec<u8> {
        let mut v = vec![b' ', b's'];
        v.extend_from_slice(&n.to_le_bytes());
        v
    }

    /// i32 literal escape bytes: `'n' + i32` (with the leading space).
    fn n(v: i32) -> Vec<u8> {
        let mut bytes = vec![b' ', b'n'];
        bytes.extend_from_slice(&v.to_le_bytes());
        bytes
    }

    /// X-call escape bytes: `'X' + cmd + u16 len + args`.
    fn x(cmd: u16, args: &[u8]) -> Vec<u8> {
        let mut bytes = vec![b' ', b'X'];
        bytes.extend_from_slice(&cmd.to_le_bytes());
        bytes.extend_from_slice(&(args.len() as u16).to_le_bytes());
        bytes.extend_from_slice(args);
        bytes
    }

    /// Call-argument blob: `[count][args...]`. Argument-stream tags carry
    /// NO leading space (corpus: `72 04 00 6e 14 00 00 00`) — unlike the
    /// expression escapes above.
    fn args_blob(parts: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = (parts.len() as u16).to_le_bytes().to_vec();
        for p in parts {
            bytes.extend_from_slice(p);
        }
        bytes
    }

    /// Ref argument: `'r'+u16`.
    fn ra(idx: u16) -> Vec<u8> {
        let mut v = vec![b'r'];
        v.extend_from_slice(&idx.to_le_bytes());
        v
    }

    /// i32 literal argument: `'n'+i32`.
    fn na(value: i32) -> Vec<u8> {
        let mut v = vec![b'n'];
        v.extend_from_slice(&value.to_le_bytes());
        v
    }

    fn stmt_call(cmd: u16, args: &[u8]) -> Vec<u8> {
        let mut stmt = cmd.to_le_bytes().to_vec();
        stmt.extend_from_slice(&(args.len() as u16).to_le_bytes());
        stmt.extend_from_slice(args);
        stmt
    }

    #[test]
    fn set_arithmetic_and_narrowing() {
        // set x to 2 + 3   (s1 = 5)
        let expr = b"2 3 + ";
        let body = set_stmt(b's', 1, expr.as_slice(), 0);
        let script = script(wrap_body(body), vec![local(1, "x", 1)], vec![]);
        let program = ObScriptProgram::new(&script);
        let mut state = VmState::default();
        let mut host = Recorder::default();
        let outcome = program.run_block(BLOCK_GAME_MODE, &mut state, &mut host);
        assert_eq!(outcome, BlockOutcome::Completed);
        assert_eq!(state.locals.get(&1), Some(&5.0));
    }

    #[test]
    fn if_elseif_else_dispatches_the_matching_arm() {
        // if (s1 == 1) → SetStage 10 ; elseif (s1 == 2) → SetStage 20 ;
        // else → SetStage 30 ; endif
        fn cond_s1_eq(v: i32) -> Vec<u8> {
            [s(1), n(v), b" == ".to_vec()].concat()
        }
        fn setstage_call(stage: i32) -> Vec<u8> {
            stmt_call(SETSTAGE, &args_blob(&[ra(1), na(stage)]))
        }
        let body = [
            arm_stmt(0x16, &cond_s1_eq(1)),
            setstage_call(10),
            arm_stmt(0x18, &cond_s1_eq(2)),
            setstage_call(20),
            simple_stmt(0x17),
            setstage_call(30),
            simple_stmt(0x19),
        ]
        .concat();
        let script = script(
            wrap_body(body),
            vec![local(1, "state", 1)],
            vec![0x00012345],
        );

        for (local_value, expected_stage) in [(1.0, 10i32), (2.0, 20), (9.0, 30)] {
            let program = ObScriptProgram::new(&script);
            let mut state = VmState {
                locals: [(1u16, local_value)].into_iter().collect(),
                ..Default::default()
            };
            let mut host = Recorder::default();
            program.run_block(BLOCK_GAME_MODE, &mut state, &mut host);
            let setstage: Vec<_> = host
                .calls
                .iter()
                .filter(|(cmd, _, _)| *cmd == SETSTAGE)
                .collect();
            assert_eq!(
                setstage.len(),
                1,
                "exactly one arm may run (local = {local_value})"
            );
            let stage = match setstage[0].1[1] {
                ObScriptValue::Num(v) => v as i32,
                _ => panic!("stage arg must be numeric"),
            };
            assert_eq!(stage, expected_stage);
        }
    }

    #[test]
    fn nested_if_and_skip_do_not_leak_statements() {
        // if (s1 == 1)
        //   if (s1 == 1) SetStage 11 endif
        //   SetStage 10
        // elseif (s1 == 2)
        //   SetStage 20
        // endif
        let body = [
            arm_stmt(0x16, &[s(1), n(1), b" == ".to_vec()].concat()),
            arm_stmt(0x16, &[s(1), n(1), b" == ".to_vec()].concat()),
            stmt_call(SETSTAGE, &args_blob(&[ra(1), na(11)])),
            simple_stmt(0x19),
            stmt_call(SETSTAGE, &args_blob(&[ra(1), na(10)])),
            arm_stmt(0x18, &[s(1), n(2), b" == ".to_vec()].concat()),
            stmt_call(SETSTAGE, &args_blob(&[ra(1), na(20)])),
            simple_stmt(0x19),
        ]
        .concat();
        let script = script(wrap_body(body), vec![local(1, "state", 1)], vec![0x1]);
        for (local_value, expected) in [(1.0, vec![11, 10]), (2.0, vec![20])] {
            let program = ObScriptProgram::new(&script);
            let mut state = VmState {
                locals: [(1u16, local_value)].into_iter().collect(),
                ..Default::default()
            };
            let mut host = Recorder::default();
            program.run_block(BLOCK_GAME_MODE, &mut state, &mut host);
            let stages: Vec<i32> = host
                .calls
                .iter()
                .filter(|(cmd, _, _)| *cmd == SETSTAGE)
                .map(|(_, args, _)| match args[1] {
                    ObScriptValue::Num(v) => v as i32,
                    _ => -1,
                })
                .collect();
            assert_eq!(stages, expected, "local = {local_value}");
        }
    }

    #[test]
    fn excessive_nested_if_chains_are_malformed() {
        let mut body = Vec::new();
        for _ in 0..=MAX_OBSCRIPT_VM_NESTING {
            body.extend(arm_stmt(0x16, b"1 "));
        }
        body.extend(simple_stmt(0x19));
        for _ in 0..=MAX_OBSCRIPT_VM_NESTING {
            body.extend(simple_stmt(0x19));
        }
        let script = script(wrap_body(body), vec![], vec![]);
        let mut state = VmState::default();
        let mut host = Recorder::default();
        assert!(matches!(
            ObScriptProgram::new(&script).run_block(BLOCK_GAME_MODE, &mut state, &mut host),
            BlockOutcome::Malformed(_)
        ));
    }

    #[test]
    fn excessive_nested_x_arguments_are_rejected() {
        let mut args = args_blob(&[]);
        for _ in 0..=MAX_OBSCRIPT_VM_NESTING {
            let inner = args;
            args = vec![1, 0, b'X', 0x39, 0x10];
            args.extend_from_slice(&(inner.len() as u16).to_le_bytes());
            args.extend(inner);
        }
        let body = stmt_call(SETSTAGE, &args);
        let script = script(wrap_body(body), vec![], vec![]);
        let mut state = VmState::default();
        let mut host = Recorder::default();
        assert!(matches!(
            ObScriptProgram::new(&script).run_block(BLOCK_GAME_MODE, &mut state, &mut host),
            BlockOutcome::Malformed(_)
        ));
    }

    #[test]
    fn expression_call_and_ref_arguments() {
        // set x to GetStage QuestRef + 1
        // args for the X call: count=1, [r1]
        let x_args = args_blob(&[ra(1)]);
        let expr = [x(GETSTAGE, &x_args), b"1 + ".to_vec()].concat();
        let body = set_stmt(b'f', 1, &expr, 0);
        let script = script(wrap_body(body), vec![local(1, "x", 0)], vec![0x000ABCDE]);
        let program = ObScriptProgram::new(&script);
        let mut state = VmState::default();
        let mut host = Recorder::default();
        let outcome = program.run_block(BLOCK_GAME_MODE, &mut state, &mut host);
        assert_eq!(outcome, BlockOutcome::Completed);
        assert_eq!(state.locals.get(&1), Some(&43.0)); // GetStage stub 42 + 1
        assert!(host.calls.iter().any(|(cmd, args, _)| {
            *cmd == GETSTAGE && matches!(args.first(), Some(ObScriptValue::Ref(0x000ABCDE)))
        }));
    }

    #[test]
    fn non_gamemode_blocks_are_skipped_and_globals_route_through_host() {
        // Begin MenuMode (block 9): set x to 1 — must not run.
        // Begin GameMode: set G1 to 7 — routes to the host's global write.
        let mut bytes = (0x1du16).to_le_bytes().to_vec();
        bytes.extend_from_slice(&0u16.to_le_bytes());
        let menu_body = set_stmt(b's', 1, b"1 ", 0);
        bytes.extend(begin(9, menu_body.len() as u32));
        bytes.extend(menu_body);
        bytes.extend(simple_stmt(0x11));
        let gm_body = set_stmt(b'G', 1, b"7 ", 0);
        bytes.extend(begin(BLOCK_GAME_MODE, gm_body.len() as u32));
        bytes.extend(gm_body);
        bytes.extend(simple_stmt(0x11));

        let script = script(bytes, vec![local(1, "x", 1)], vec![0x00033D62]);
        let program = ObScriptProgram::new(&script);
        let mut state = VmState::default();
        let mut host = Recorder::default();
        let outcome = program.run_block(BLOCK_GAME_MODE, &mut state, &mut host);
        assert_eq!(outcome, BlockOutcome::Completed);
        assert!(
            state.locals.is_empty(),
            "MenuMode body must be skipped, locals = {:?}",
            state.locals
        );
        let global_writes: Vec<_> = host
            .calls
            .iter()
            .filter(|(cmd, _, _)| *cmd == CMD_SET_GLOBAL)
            .collect();
        assert_eq!(global_writes.len(), 1);
        assert!(matches!(global_writes[0].1[0], ObScriptValue::Num(7.0)));
    }

    /// Explicit-caller statement bytes: `[1c][caller u16][cmd u16][len u16][args]`.
    fn caller_stmt(caller: u16, cmd: u16, args: &[u8]) -> Vec<u8> {
        let mut stmt = (0x1cu16).to_le_bytes().to_vec();
        stmt.extend_from_slice(&caller.to_le_bytes());
        stmt.extend_from_slice(&cmd.to_le_bytes());
        stmt.extend_from_slice(&(args.len() as u16).to_le_bytes());
        stmt.extend_from_slice(args);
        stmt
    }

    /// A script whose reference list is `[Object, Variable, Object]`, the
    /// FNV `Patch04Quest00SCRIPT` shape: a reference variable sits between
    /// two literal objects.
    fn interleaved_script(body: Vec<u8>) -> ScriptRecord {
        let mut script = script(wrap_body(body), vec![], vec![0xA, 0xB]);
        script.refs = vec![
            ScriptRef::Object(0xA),
            ScriptRef::Variable(1),
            ScriptRef::Object(0xB),
        ];
        script.num_refs = 3;
        script
    }

    /// #5629: compiled reference indices address `SCRO` and `SCRV` entries
    /// interleaved in subrecord order. With the list `[A, var, B]`, index 3
    /// is B — the SCRO-only table (`[A, B]`) called it out of range — and
    /// index 2, the variable slot, resolves to nothing (a null reference),
    /// never to a neighbouring object.
    #[test]
    fn reference_indices_address_the_interleaved_scro_scrv_list() {
        let body = [
            stmt_call(SETSTAGE, &args_blob(&[ra(1), na(10)])),
            stmt_call(SETSTAGE, &args_blob(&[ra(2), na(20)])),
            stmt_call(SETSTAGE, &args_blob(&[ra(3), na(30)])),
        ]
        .concat();
        let script = interleaved_script(body);
        let mut host = Recorder::default();
        ObScriptProgram::new(&script).run_block(
            BLOCK_GAME_MODE,
            &mut VmState::default(),
            &mut host,
        );
        let quests: Vec<ObScriptValue> = host
            .calls
            .iter()
            .filter(|(cmd, _, _)| *cmd == SETSTAGE)
            .map(|(_, args, _)| args[0])
            .collect();
        assert_eq!(
            quests,
            vec![
                ObScriptValue::Ref(0xA),
                ObScriptValue::Ref(0), // the variable slot: unresolved, never B
                ObScriptValue::Ref(0xB),
            ]
        );
    }

    /// #5629: an explicit caller that does not resolve must not run as an
    /// implicit-caller command (the host reads `None` as "the script's own
    /// owner"). It is declined, carrying the command id for the census.
    #[test]
    fn an_unresolved_explicit_caller_is_declined_not_run_on_the_owner() {
        let enable = 0x1021u16;
        let body = [
            caller_stmt(2, enable, &args_blob(&[])), // the variable slot
            caller_stmt(9, enable, &args_blob(&[])), // out of range
            caller_stmt(3, enable, &args_blob(&[])), // resolves to B
        ]
        .concat();
        let script = interleaved_script(body);
        let mut host = Recorder::default();
        ObScriptProgram::new(&script).run_block(
            BLOCK_GAME_MODE,
            &mut VmState::default(),
            &mut host,
        );
        assert_eq!(
            host.calls,
            vec![
                (CMD_DECLINED, vec![ObScriptValue::Num(f32::from(enable))], None),
                (CMD_DECLINED, vec![ObScriptValue::Num(f32::from(enable))], None),
                (enable, vec![], Some(0xB)),
            ]
        );
    }

    /// #5630: a tag-`r` `Set` is `set <ref>.<remote var> to expr`, a write
    /// into another script's variable. It used to be stored as `ref_vars[idx]
    /// = value`, so after `set Q.iFlag to 1` every later `' r<idx>'` naming
    /// `Q` resolved to form 1. It now reaches the host as a remote-variable write
    /// ([`CMD_SET_REMOTE_VARIABLE`]), and the object still resolves to itself.
    #[test]
    fn a_remote_variable_set_reaches_the_host_and_does_not_shadow_the_object() {
        // `r<ref 1> s<ordinal 1>` — the 6-byte target, then `[len][expr]`.
        let mut remote = vec![b'r'];
        remote.extend_from_slice(&1u16.to_le_bytes());
        remote.push(b's');
        remote.extend_from_slice(&1u16.to_le_bytes());
        let expr = b"1";
        remote.extend_from_slice(&(expr.len() as u16).to_le_bytes());
        remote.extend_from_slice(expr);
        let mut set = (0x15u16).to_le_bytes().to_vec();
        set.extend_from_slice(&(remote.len() as u16).to_le_bytes());
        set.extend_from_slice(&remote);

        let body = [set, stmt_call(SETSTAGE, &args_blob(&[ra(1), na(10)]))].concat();
        let script = interleaved_script(body);
        let mut host = Recorder::default();
        ObScriptProgram::new(&script).run_block(
            BLOCK_GAME_MODE,
            &mut VmState::default(),
            &mut host,
        );
        assert_eq!(
            host.calls,
            vec![
                (
                    CMD_SET_REMOTE_VARIABLE,
                    vec![ObScriptValue::Ref(0xA), ObScriptValue::Num(1.0), ObScriptValue::Num(1.0)],
                    None
                ),
                (
                    SETSTAGE,
                    vec![ObScriptValue::Ref(0xA), ObScriptValue::Num(10.0)],
                    None
                ),
            ],
            "the later SetStage still targets the object, not the value written"
        );
    }

    /// The bytes of `VCG01.bGiveTest` in vanilla New Vegas: `set VCG01.bGiveTest to 1` is
    /// `15 00 0a 00 | 72 04 00 73 0d 00 | 02 00 | 20 31`, and `VCG01.bBeganTest == 0` reads
    /// `20 72 04 00 73 06 00 20 30 20 3d 3d`. Both go to the host with the ordinal (13, 6)
    /// of the variable in the quest's own script — a read that used to see only the quest's
    /// form id, so `if Quest.var == 0` compared a FormID.
    #[test]
    fn remote_variables_read_and_write_by_ordinal_on_the_vanilla_bytes() {
        let set = [0x15, 0, 0x0a, 0, b'r', 4, 0, b's', 0x0d, 0, 2, 0, 0x20, b'1'].to_vec();
        // `set bLocal to VCG01.bBeganTest + 5` into local slot 3.
        let expr = [&[0x20, b'r', 4, 0, b's', 6, 0][..], &[0x20, b'n', 5, 0, 0, 0], b"+ "].concat();
        let read = set_stmt(b's', 3, &expr, 0);
        let mut script = script(wrap_body([set, read].concat()), vec![local(3, "bLocal", 1)], vec![]);
        script.refs = vec![ScriptRef::Object(0xA), ScriptRef::Variable(2), ScriptRef::Object(0xB), ScriptRef::Object(0xC)];
        let mut host = Recorder::default();
        let mut state = VmState::default();
        ObScriptProgram::new(&script).run_block(BLOCK_GAME_MODE, &mut state, &mut host);
        // Ref index 4 is the fourth entry: 0xC.
        assert_eq!(host.calls[0], (CMD_SET_REMOTE_VARIABLE, vec![ObScriptValue::Ref(0xC), ObScriptValue::Num(13.0), ObScriptValue::Num(1.0)], None));
        assert_eq!(host.calls[1], (CMD_GET_REMOTE_VARIABLE, vec![ObScriptValue::Ref(0xC), ObScriptValue::Num(6.0)], None));
        // The Recorder answers an unknown command with 0, so the local is 0 + 5.
        assert_eq!(state.locals.get(&3), Some(&5.0));
    }

    /// `if player.IsCurrentFurnitureRef DocMitchellCouchREF` in vanilla New Vegas
    /// (`VCG01DocMitchellCouchTriggerSCRIPT`): inside an expression a call on a reference is
    /// `' r<ref>'` immediately followed by `'X' <cmd> <len> <args>` — no space before the `X`,
    /// which is how an implicit-caller call (`' X'`) differs. The call went unseen before:
    /// the reference was read as a value and the rest of the escape skipped as text, so every
    /// `if Actor.GetDead` compared a form id.
    #[test]
    fn an_expression_call_on_a_reference_reaches_the_host_with_that_caller() {
        const IS_CURRENT_FURNITURE_REF: u16 = 0x10a2;
        let expr = [
            &[0x20, b'r', 1, 0, b'X', 0xa2, 0x10, 5, 0, 1, 0, b'r', 2, 0][..],
            b" 1 ==",
        ]
        .concat();
        let mut script = script(wrap_body(set_stmt(b'f', 3, &expr, 0)), vec![local(3, "result", 0)], vec![]);
        script.refs = vec![ScriptRef::Object(0x14), ScriptRef::Object(0xB)];
        let mut host = Recorder::default();
        let mut state = VmState::default();
        let outcome = ObScriptProgram::new(&script).run_block(BLOCK_GAME_MODE, &mut state, &mut host);
        assert_eq!(outcome, BlockOutcome::Completed);
        assert_eq!(
            host.calls,
            vec![(IS_CURRENT_FURNITURE_REF, vec![ObScriptValue::Ref(0xB)], Some(0x14))],
            "the call is on the player, with the furniture as its argument"
        );
        // The Recorder answers an unknown command with 0, so `0 == 1` is false: the local is the comparison, not a form id.
        assert_eq!(state.locals.get(&3), Some(&0.0));
    }

    /// A call on a reference variable nobody assigned has no caller to act on: it is declined and
    /// counted, and reads 0 — never run as an implicit-caller call on the script's owner.
    #[test]
    fn an_expression_call_on_an_unassigned_reference_variable_is_declined() {
        let expr = [&[0x20, b'r', 1, 0, b'X', 0xa2, 0x10, 2, 0, 0, 0][..], b" 1 +"].concat();
        let mut script = script(wrap_body(set_stmt(b'f', 3, &expr, 0)), vec![local(3, "result", 0), local(2, "who", 0)], vec![]);
        script.refs = vec![ScriptRef::Variable(2)];
        let mut host = Recorder::default();
        let mut state = VmState::default();
        ObScriptProgram::new(&script).run_block(BLOCK_GAME_MODE, &mut state, &mut host);
        assert_eq!(host.calls, vec![(CMD_DECLINED, vec![ObScriptValue::Num(f32::from(0x10a2u16))], None)]);
        assert_eq!(state.locals.get(&3), Some(&1.0), "0 + 1");
    }

    /// A host that answers `GetActionRef` (`0x10cd`) with a fixed reference.
    struct ActionRefHost(u32);

    impl ObScriptHost for ActionRefHost {
        fn call(&mut self, cmd: u16, _args: &[ObScriptValue], _caller: Option<u32>) -> Option<ObScriptValue> {
            (cmd == 0x10cd).then_some(ObScriptValue::Ref(self.0))
        }
    }

    /// `if GetActionRef == vHDNCRActivator` in vanilla New Vegas
    /// (`vHDGeneratorTriggerSCRIPT`): `20 58 cd 10 00 00 | 20 5a 01 00 | 20 3d 3d` — the
    /// `' Z<ref>'` escape is a reference as a value, which the VM used to skip as text. Two
    /// references are equal when their form ids are, exactly: `0x01000004` and `0x01000005` are
    /// one `f32`.
    #[test]
    fn a_reference_literal_compares_exactly() {
        let expr = [0x20, b'X', 0xcd, 0x10, 0, 0, 0x20, b'Z', 1, 0, 0x20, b'=', b'='].to_vec();
        for (answer, equal) in [(0x0100_0005u32, 1.0f32), (0x0100_0004, 0.0)] {
            let mut script = script(wrap_body(set_stmt(b's', 3, &expr, 0)), vec![local(3, "same", 1)], vec![]);
            script.refs = vec![ScriptRef::Object(0x0100_0005)];
            let mut state = VmState::default();
            ObScriptProgram::new(&script).run_block(BLOCK_GAME_MODE, &mut state, &mut ActionRefHost(answer));
            assert_eq!(state.locals.get(&3), Some(&equal), "answer {answer:08x}");
        }
    }

    /// `Set Actor1 to SomeREF` assigns the reference (`20 5a 16 00`), which a reference variable
    /// then holds exactly.
    #[test]
    fn a_reference_variable_is_assigned_a_reference_literal() {
        let expr = [0x20, b'Z', 1, 0].to_vec();
        let mut script = script(wrap_body(set_stmt(b'f', 2, &expr, 0)), vec![local(2, "rActor", 0)], vec![]);
        script.refs = vec![ScriptRef::Object(0x0100_0005), ScriptRef::Variable(2)];
        let mut state = VmState::default();
        ObScriptProgram::new(&script).run_block(BLOCK_GAME_MODE, &mut state, &mut Recorder::default());
        assert_eq!(state.ref_locals.get(&2), Some(&0x0100_0005));
    }

    /// `ShowMessage AbilityWellRestedStartMsg` in New Vegas
    /// (`DeathclawStunWeaponSCRIPT`): `59 10 0b 00 | 01 00 72 01 00 | 00 00 00 00 00 00`. The id
    /// `0x1059` is Oblivion's text `Message`, whose decoder cannot read a form: the call used to
    /// be declined, with every `ShowMessage` of the game. It reaches the host as the message
    /// form and its values.
    #[test]
    fn a_fallout_show_message_reaches_the_host_with_its_message_form() {
        let payload = [1u8, 0, b'r', 1, 0, 0, 0, 0, 0, 0, 0];
        let mut statement = vec![0x59, 0x10, payload.len() as u8, 0];
        statement.extend_from_slice(&payload);
        let mut script = script(wrap_body(statement), vec![], vec![]);
        script.refs = vec![ScriptRef::Object(0x0010_2030)];
        let mut host = Recorder::default();
        ObScriptProgram::new(&script).run_block(BLOCK_GAME_MODE, &mut VmState::default(), &mut host);
        assert_eq!(host.calls, vec![(CMD_MESSAGE, vec![ObScriptValue::Ref(0x0010_2030)], None)]);
        assert!(host.messages.is_empty(), "not a text message");
    }

    /// `Set antler_x to AntlerREF.GetPos x` in vanilla New Vegas (`REPCONDavisonDialogScript`):
    /// `20 72 02 00 | 58 06 10 03 00 | 01 00 58` — the axis is one bare letter `X`, which the generic
    /// argument decoder took for a nested-call tag and could not read at all (the whole expression
    /// was dropped). For the axis commands it is the axis, `0` for X, `1` for Y, `2` for Z.
    #[test]
    fn an_axis_argument_is_decoded_as_the_axis() {
        for (letter, axis) in [(0x58u8, 0.0f32), (0x59, 1.0), (0x5a, 2.0)] {
            let expr = [0x20, b'r', 2, 0, b'X', 0x06, 0x10, 3, 0, 1, 0, letter].to_vec();
            let mut script = script(wrap_body(set_stmt(b'f', 3, &expr, 0)), vec![local(3, "antler", 0)], vec![]);
            script.refs = vec![ScriptRef::Object(0xA), ScriptRef::Object(0xB)];
            let mut host = Recorder::default();
            ObScriptProgram::new(&script).run_block(BLOCK_GAME_MODE, &mut VmState::default(), &mut host);
            assert_eq!(host.calls, vec![(0x1006, vec![ObScriptValue::Num(axis)], Some(0xB))], "axis {letter:#x}");
        }
    }

    /// `SwapTexture OnScreenSlideRef "##GS01:0" "interface\\endgame\\endscrn_PF05"` in `NarratorScript`:
    /// `69 12 | len | 03 00 | 72 05 00 | 08 00 "##GS01:0" | 1e 00 "interface\\endgame\\..."` — a reference then two bare
    /// strings, which the host receives through `text_arguments`.
    #[test]
    fn a_reference_and_two_strings_are_decoded_with_the_strings_handed_to_the_host() {
        #[derive(Default)]
        struct Texts {
            calls: Vec<(u16, Vec<ObScriptValue>)>,
            texts: Vec<(u16, Vec<String>)>,
        }
        impl ObScriptHost for Texts {
            fn call(&mut self, cmd: u16, args: &[ObScriptValue], _: Option<u32>) -> Option<ObScriptValue> {
                self.calls.push((cmd, args.to_vec()));
                None
            }
            fn text_arguments(&mut self, cmd: u16, texts: &[String]) {
                self.texts.push((cmd, texts.to_vec()));
            }
        }
        let slot = "##GS01:0";
        let texture = "interface\\endgame\\endscrn_PF05";
        let mut args = vec![3u8, 0, b'r', 1, 0];
        for text in [slot, texture] {
            args.extend((text.len() as u16).to_le_bytes());
            args.extend(text.as_bytes());
        }
        let script = script(wrap_body(stmt_call(0x1269, &args)), vec![], vec![0x77]);
        let mut host = Texts::default();
        ObScriptProgram::new(&script).run_block(BLOCK_GAME_MODE, &mut VmState::default(), &mut host);
        assert_eq!(host.texts, vec![(0x1269, vec![slot.to_owned(), texture.to_owned()])]);
        assert_eq!(host.calls, vec![(0x1269, vec![ObScriptValue::Ref(0x77)])]);
    }

    /// `TrapUpdate` takes no argument: the VM hands the host the trap variables the script declares, by
    /// name, in the documented order (and 0 for a missing one).
    #[test]
    fn trap_update_receives_the_scripts_trap_variables_by_name() {
        let locals = vec![local(1, "fTrapDamage", 0), local(2, "bTrapContinuous", 0), local(3, "fTrapMinVelocity", 0)];
        let script = script(wrap_body(stmt_call(CMD_TRAP_UPDATE, &args_blob(&[]))), locals, vec![]);
        let mut state = VmState::default();
        state.locals.extend([(1, 3.0), (2, 1.0), (3, 20.0)]);
        let mut host = Recorder::default();
        ObScriptProgram::new(&script).run_block(BLOCK_GAME_MODE, &mut state, &mut host);
        assert_eq!(
            host.calls,
            vec![(CMD_TRAP_UPDATE, [3.0, 0.0, 0.0, 20.0, 1.0, 0.0].map(ObScriptValue::Num).to_vec(), None)]
        );
    }

    #[test]
    fn return_stops_the_block() {
        let body = [
            simple_stmt(0x1e),                                 // Return
            stmt_call(SETSTAGE, &args_blob(&[ra(1), na(99)])), // must not run
        ]
        .concat();
        let script = script(wrap_body(body), vec![], vec![0x1]);
        let program = ObScriptProgram::new(&script);
        let mut state = VmState::default();
        let mut host = Recorder::default();
        assert_eq!(
            program.run_block(BLOCK_GAME_MODE, &mut state, &mut host),
            BlockOutcome::Returned
        );
        assert!(host.calls.is_empty());
    }

    #[test]
    fn truncated_bytecode_reports_malformed() {
        let script = script(vec![0x15, 0x00, 0xFF, 0xFF], vec![], vec![]);
        let program = ObScriptProgram::new(&script);
        let mut state = VmState::default();
        let mut host = Recorder::default();
        assert_eq!(
            program.run_block(BLOCK_GAME_MODE, &mut state, &mut host),
            BlockOutcome::Malformed(0)
        );
    }

    /// #5655 — argument payload reads must be bounded by the statement's
    /// argument *region*, not by the script buffer: before the fix, an
    /// argument whose declared payload straddled the region end decoded
    /// the following statement's bytes as its own and dispatched the
    /// command with a garbage argument instead of reporting `Malformed`.
    /// Each leg truncates one argument shape mid-payload, with the bytes
    /// the old decode would have consumed present in the next statement.
    #[test]
    fn straddling_argument_payloads_report_malformed_not_next_statement_bytes() {
        struct Leg {
            name: &'static str,
            // The argument blob as declared (its own length is the region).
            args: Vec<u8>,
        }
        let legs = [
            // count=2: one complete i32 literal, then a tag-'s' local whose
            // u16 payload lies past the region end. (Hand-built count —
            // `args_blob` would declare only the literal.)
            Leg {
                name: "tag-s local",
                args: [vec![2u8, 0], na(5), vec![b's']].concat(),
            },
            // count=1: an X-call header tag with both u16s past the end.
            Leg {
                name: "tag-X header",
                args: [vec![1u16.to_le_bytes()[0], 1u16.to_le_bytes()[1]], vec![b'X']].concat(),
            },
            // count=1: a u16-literal tag byte with its payload past the end.
            Leg {
                name: "u16 literal",
                args: [vec![1u16.to_le_bytes()[0], 1u16.to_le_bytes()[1]], vec![b'Q']].concat(),
            },
            // The region cannot even hold the count header.
            Leg {
                name: "count header",
                args: vec![0x02],
            },
        ];
        for leg in legs {
            // A following statement supplies the bytes the buffer-bounded
            // decode used to read as payload — the End opcode (0x11 0x00),
            // which decodes as local index 0x0011 if the region bound is
            // missing.
            let body = [stmt_call(SETSTAGE, &leg.args), simple_stmt(0x11)].concat();
            let script = script(wrap_body(body), vec![], vec![]);
            let program = ObScriptProgram::new(&script);
            let mut state = VmState::default();
            let mut host = Recorder::default();
            let outcome = program.run_block(BLOCK_GAME_MODE, &mut state, &mut host);
            assert!(
                matches!(outcome, BlockOutcome::Malformed(_)),
                "{}: a straddling argument payload must be Malformed, got {outcome:?}",
                leg.name
            );
            assert!(
                !host.calls.iter().any(|(cmd, _, _)| *cmd == SETSTAGE),
                "{}: the command must not run with decoded next-statement bytes",
                leg.name
            );
        }
    }

    /// M47.3 phase 2 — `Message` / `MessageBox` payloads, byte for byte from
    /// vanilla `Oblivion.esm` (SEObeliskNewSCRIPT, ARTrapEvilStoneAUTOFire…).
    #[test]
    fn message_payloads_decode_text_variables_and_buttons() {
        let text = b"Do you want to place a Heart of Order into the obelisk?";
        let mut message_box = vec![0x00, 0x10, 0, 0, 0x01, 0x00];
        message_box.extend_from_slice(&(text.len() as u16).to_le_bytes());
        message_box.extend_from_slice(text);
        message_box.extend_from_slice(&[0x00, 0x00, 0x02, 0x00]);
        message_box.extend_from_slice(&[0x01, 0x00, 0x03, 0x00]);
        message_box.extend_from_slice(b"Yes");
        message_box.extend_from_slice(&[0x01, 0x00, 0x02, 0x00]);
        message_box.extend_from_slice(b"No");
        let len = (message_box.len() - 4) as u16;
        message_box[2..4].copy_from_slice(&len.to_le_bytes());

        let text = b"Rank %.0f Fireball";
        let mut message = vec![0x59, 0x10, 0, 0, 0x01, 0x00];
        message.extend_from_slice(&(text.len() as u16).to_le_bytes());
        message.extend_from_slice(text);
        // One variable — short local 1 — then the display seconds (10).
        message.extend_from_slice(&[0x01, 0x00, b's', 0x01, 0x00, 0x0a, 0x00, 0x00, 0x00]);
        let len = (message.len() - 4) as u16;
        message[2..4].copy_from_slice(&len.to_le_bytes());

        let mut body = message_box;
        body.extend(message);
        let record = script(wrap_body(body), vec![local(1, "SpellRank", 1)], Vec::new());
        let program = ObScriptProgram::new(&record);
        let mut state = VmState::default();
        state.locals.insert(1, 3.0);
        let mut host = Recorder::default();
        program.run_block(BLOCK_GAME_MODE, &mut state, &mut host);
        assert_eq!(
            host.messages,
            vec![
                ObScriptMessage {
                    text: "Do you want to place a Heart of Order into the obelisk?".into(),
                    values: Vec::new(),
                    buttons: vec!["Yes".into(), "No".into()],
                    message_box: true,
                },
                ObScriptMessage {
                    text: "Rank %.0f Fireball".into(),
                    values: vec![3.0],
                    buttons: Vec::new(),
                    message_box: false,
                },
            ]
        );
    }
}
