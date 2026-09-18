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
//!   set `'G' 'f' 'n' 'r' 's' 'z' 'Y' 'Z'`. Literals whose bytes would
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

use byroredux_plugin::esm::records::ScriptRecord;

/// Synthetic host command for global reads — real global accesses arrive
/// as `' G<n>'` escapes, never as bytecode command ids.
pub const CMD_GET_GLOBAL: u16 = u16::MAX - 1;
/// Synthetic host command for global writes (`set GameHour to …`).
pub const CMD_SET_GLOBAL: u16 = u16::MAX;

/// Block type of `Begin GameMode` in the compiled block enum (empirical:
/// every quest-script GameMode block carries `0x0000`).
pub const BLOCK_GAME_MODE: u16 = 0;

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
}

/// Mutable per-script state that outlives one block execution.
#[derive(Default)]
pub struct VmState {
    /// Local variable storage keyed by the 1-based `SLSD` index — compiled
    /// references are 1-based over that table.
    pub locals: std::collections::HashMap<u16, f32>,
    /// Reference variables assigned by `set RefVar to <ref>`.
    pub ref_vars: std::collections::HashMap<u16, u32>,
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

    /// Run one named block; blocks of any other type are skipped whole.
    pub fn run_block(
        &self,
        block_type: u16,
        state: &mut VmState,
        host: &mut dyn ObScriptHost,
    ) -> BlockOutcome {
        let mut vm = Vm {
            bytes: &self.script.compiled,
            refs: &self.script.ref_form_ids,
            state,
            host,
            last_ref: None,
            returned: false,
        };
        let bytes = vm.bytes;
        let mut offset = 0usize;
        loop {
            let Some(op) = read_u16(bytes, offset) else {
                // Stream exhausted without the requested block (ScriptName-
                // only scripts, or a later block type) — nothing to run.
                return BlockOutcome::Completed;
            };
            if op == 0x10 {
                let Some((payload_start, payload_end)) = payload_span(bytes, offset) else {
                    return BlockOutcome::Malformed(offset);
                };
                // Payload: [block_type u16][body_len u32].
                let this_block = read_u16(bytes, payload_start).unwrap_or(0);
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
    refs: &'a [u32],
    state: &'a mut VmState,
    host: &'a mut dyn ObScriptHost,
    /// The most recent ref value pushed by an `' r<n>'` escape — what
    /// `set RefVar to SomeRef` stores.
    last_ref: Option<u32>,
    /// Set by a `Return` hit anywhere in the block (including inside
    /// if-arms — the standard early-exit idiom in vanilla scripts); every
    /// executor loop unwinds when it sees this.
    returned: bool,
}

fn read_u16(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}

/// Offset of the statement following the one at `offset`.
fn next_statement(bytes: &[u8], offset: usize, op: u16) -> Option<usize> {
    if op == 0x1c {
        // [1c][ref u16][cmd u16][len u16][args]
        let end = offset + 8 + read_u16(bytes, offset + 6)? as usize;
        return (end <= bytes.len()).then_some(end);
    }
    let len = read_u16(bytes, offset + 2)? as usize;
    let end = offset + 4 + len;
    (end <= bytes.len()).then_some(end)
}

fn payload_span(bytes: &[u8], offset: usize) -> Option<(usize, usize)> {
    let len = read_u16(bytes, offset + 2)? as usize;
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
    matches!(b, b'G' | b'X' | b'Y' | b'Z' | b'f' | b'n' | b'r' | b's' | b'z')
}

impl<'a> Vm<'a> {
    /// Execute statements until this block's `End`.
    fn exec_block_body(&mut self, mut offset: usize) -> BlockOutcome {
        loop {
            if self.returned {
                return BlockOutcome::Returned;
            }
            let Some(op) = read_u16(self.bytes, offset) else {
                return BlockOutcome::Malformed(offset);
            };
            match op {
                0x11 => return BlockOutcome::Completed,
                0x1e => {
                    self.returned = true;
                    return BlockOutcome::Returned;
                }
                0x16 => {
                    let Some(after) = self.exec_if_chain(offset) else {
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
            let caller_ref = read_u16(self.bytes, offset + 2)?;
            let cmd = read_u16(self.bytes, offset + 4)?;
            let len = read_u16(self.bytes, offset + 6)? as usize;
            let args_start = offset + 8;
            let args_end = args_start.checked_add(len)?;
            if args_end > self.bytes.len() {
                return None;
            }
            if run {
                let caller = self.resolve_ref_var(caller_ref);
                let args = self.decode_args(args_start, args_end)?;
                self.host.call(cmd, &args, caller);
            }
            return Some(args_end);
        }
        let (start, end) = payload_span(self.bytes, offset)?;
        if run {
            match op {
                0x15 => self.exec_set(start, end),
                op if !is_flow_op(op) => {
                    // Implicit-caller statement call: the line opcode is
                    // the command id.
                    let args = self.decode_args(start, end)?;
                    self.host.call(op, &args, None);
                }
                _ => {}
            }
        }
        Some(end)
    }

    /// If/ElseIf/Else chain rooted at the `0x16`/`0x18` in `offset`. Only
    /// `0x16` nests; `0x17`/`0x18` are arms at this level. Returns the
    /// offset just past the chain's EndIf.
    fn exec_if_chain(&mut self, mut offset: usize) -> Option<usize> {
        loop {
            let op = read_u16(self.bytes, offset)?;
            match op {
                0x16 | 0x18 => {
                    // Arm header: [op][len][u16][u16 expr_len][expr].
                    let expr_len = read_u16(self.bytes, offset + 6)? as usize;
                    let cond_start = offset + 8;
                    let cond_end = cond_start.checked_add(expr_len)?;
                    if cond_end > self.bytes.len() {
                        return None;
                    }
                    let taken = self.eval_expr(cond_start, cond_end) != 0.0;
                    let after = next_statement(self.bytes, offset, op)?;
                    if taken {
                        return self.run_arm_body(after);
                    }
                    offset = self.skip_to_next_arm(after)?;
                }
                0x17 => {
                    // Else — its body is the arm that runs when nothing
                    // before it matched.
                    let after = next_statement(self.bytes, offset, op)?;
                    return self.run_arm_body(after);
                }
                0x19 => return next_statement(self.bytes, offset, op),
                _ => return None,
            }
        }
    }

    /// Run a taken arm's body until the chain's EndIf. Nested `0x16` chains
    /// recurse; reaching a sibling `0x17`/`0x18` header means the body is
    /// complete — skip the remaining arms.
    fn run_arm_body(&mut self, mut offset: usize) -> Option<usize> {
        loop {
            if self.returned {
                // Return inside an arm exits the whole block — skip the
                // rest of the chain so the caller's offset stays valid.
                return self.skip_to_chain_end(offset);
            }
            let op = read_u16(self.bytes, offset)?;
            match op {
                0x19 => return next_statement(self.bytes, offset, op),
                0x17 | 0x18 => return self.skip_to_chain_end(offset),
                0x16 => {
                    offset = self.exec_if_chain(offset)?;
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
            let op = read_u16(self.bytes, offset)?;
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
            let op = read_u16(self.bytes, offset)?;
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
            let op = read_u16(self.bytes, offset)?;
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

    /// `Set` payload: `[target escape][u16 expr_len][expr]`. Targets:
    /// `' s<n>'`/`' f<n>'` locals (narrowed per declared type), `' G<n>'`
    /// globals (routed to the host as a global write), `' r<n>'` ref
    /// variables (6-byte escape; the trailing u16 is a secondary index the
    /// corpus always zeroes).
    fn exec_set(&mut self, start: usize, end: usize) {
        let Some(&tag) = self.bytes.get(start) else {
            return;
        };
        // Target escape: `'s'+idx` is 3 bytes, `'r'+idx+secondary` is 6
        // (the leading space of the *expression* escapes is absent here —
        // corpus: `73 07 00 | 05 00 | " 1200"`), then `[u16 expr_len][expr]`.
        let idx = read_u16(self.bytes, start + 1).unwrap_or(0);
        let expr_len_at = match tag {
            b's' | b'f' | b'G' => start + 3,
            b'r' => start + 6,
            _ => return,
        };
        if expr_len_at + 2 > end {
            return;
        }
        let expr_len = read_u16(self.bytes, expr_len_at).unwrap_or(0) as usize;
        let expr_start = expr_len_at + 2;
        let expr_end = expr_start + expr_len;
        if expr_end > end {
            return;
        }
        self.last_ref = None;
        let value = self.eval_expr(expr_start, expr_end);
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
            b'r' => {
                let form = self.last_ref.unwrap_or(value as u32);
                self.state.ref_vars.insert(idx, form);
            }
            _ => {}
        }
    }

    /// Evaluate an RPN expression token stream `[start, end)` to a numeric
    /// result (nonzero = true for conditions).
    fn eval_expr(&mut self, start: usize, end: usize) -> f32 {
        let mut stack: Vec<ObScriptValue> = Vec::with_capacity(8);
        self.last_ref = None;
        let bytes = &self.bytes[start..end];
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == 0x20 && i + 1 < bytes.len() && is_escape_tag(bytes[i + 1]) {
                let tag = bytes[i + 1];
                let consumed = match tag {
                    b'r' if i + 4 <= bytes.len() => {
                        let idx = read_u16(bytes, i + 2).unwrap_or(0);
                        let resolved = self.resolve_ref_var(idx).unwrap_or(0);
                        self.last_ref = Some(resolved);
                        stack.push(ObScriptValue::Ref(resolved));
                        Some(4)
                    }
                    b's' | b'f' if i + 4 <= bytes.len() => {
                        let idx = read_u16(bytes, i + 2).unwrap_or(0);
                        stack.push(ObScriptValue::Num(
                            self.state.locals.get(&idx).copied().unwrap_or(0.0),
                        ));
                        Some(4)
                    }
                    b'G' if i + 4 <= bytes.len() => {
                        let idx = read_u16(bytes, i + 2).unwrap_or(0);
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
                        let cmd = read_u16(bytes, i + 2).unwrap_or(0);
                        let arg_len = read_u16(bytes, i + 4).unwrap_or(0) as usize;
                        let arg_start = i + 6;
                        let Some(arg_end) = arg_start.checked_add(arg_len) else {
                            break;
                        };
                        if arg_end > bytes.len() {
                            break;
                        }
                        // `i` is slice-relative; decode_args indexes the
                        // whole script buffer, so translate to absolute.
                        let Some(args) = self.decode_args(start + arg_start, start + arg_end)
                        else {
                            break;
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
        stack.last().map(|v| v.as_num()).unwrap_or(0.0)
    }

    /// Decode `[u16 count][count typed args]` call arguments.
    fn decode_args(&mut self, start: usize, end: usize) -> Option<Vec<ObScriptValue>> {
        let mut args = Vec::new();
        if start == end {
            return Some(args);
        }
        let count = read_u16(self.bytes, start)?;
        let mut o = start + 2;
        for _ in 0..count {
            if o >= end {
                return None;
            }
            let tag = self.bytes[o];
            match tag {
                b'r' => {
                    let idx = read_u16(self.bytes, o + 1)?;
                    let form = self.resolve_ref_var(idx).unwrap_or(0);
                    args.push(ObScriptValue::Ref(form));
                    o += 3;
                }
                b's' | b'f' => {
                    let idx = read_u16(self.bytes, o + 1)?;
                    args.push(ObScriptValue::Num(
                        self.state.locals.get(&idx).copied().unwrap_or(0.0),
                    ));
                    o += 3;
                }
                b'G' => {
                    let idx = read_u16(self.bytes, o + 1)?;
                    let form = self.resolve_ref_var(idx).unwrap_or(0);
                    let value = self
                        .host
                        .call(CMD_GET_GLOBAL, &[ObScriptValue::Ref(form)], Some(form))
                        .map(|v| v.as_num())
                        .unwrap_or(0.0);
                    args.push(ObScriptValue::Num(value));
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
                    args.push(ObScriptValue::Num(raw as f32));
                    o += 5;
                }
                b'z' => {
                    // String literal, NUL-terminated. Phase-1 hosts receive
                    // no string value; text-aware commands decode the raw
                    // bytes themselves, so the VM just steps over it.
                    let text_start = o + 1;
                    let rel = self.bytes[text_start..end].iter().position(|&c| c == 0)?;
                    o = text_start + rel + 1;
                }
                b'Y' | b'Z' => {
                    // Zero-payload tags ('Y' = the calling reference, 'Z' =
                    // a null reference in every corpus context).
                    args.push(ObScriptValue::Ref(0));
                    o += 1;
                }
                b'X' => {
                    let cmd = read_u16(self.bytes, o + 1)?;
                    let inner_len = read_u16(self.bytes, o + 3)? as usize;
                    let inner_start = o + 5;
                    let inner_end = inner_start.checked_add(inner_len)?;
                    if inner_end > end {
                        return None;
                    }
                    let inner = self.decode_args(inner_start, inner_end)?;
                    let result = self.host.call(cmd, &inner, None);
                    args.push(result.unwrap_or(ObScriptValue::Num(0.0)));
                    o = inner_end;
                }
                _ => {
                    // u16 integer literal.
                    let v = read_u16(self.bytes, o)?;
                    args.push(ObScriptValue::Num(v as f32));
                    o += 2;
                }
            }
        }
        Some(args)
    }

    /// Resolve a 1-based reference index against the script's `SCRO` table,
    /// with script-assigned ref variables shadowing it.
    fn resolve_ref_var(&self, index: u16) -> Option<u32> {
        if index == 0 {
            return None;
        }
        if let Some(form) = self.state.ref_vars.get(&index) {
            return Some(*form);
        }
        self.refs.get(index as usize - 1).copied()
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
        "==" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            flag((lhs - rhs).abs() < f32::EPSILON)
        }
        "!=" => {
            let (rhs, lhs) = (pop(stack), pop(stack));
            flag((lhs - rhs).abs() >= f32::EPSILON)
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
    use byroredux_plugin::esm::records::{ScriptLocalVar, ScriptRecord, ScriptType};

    /// Recording host stub — captures every dispatched call.
    #[derive(Default)]
    struct Recorder {
        calls: Vec<(u16, Vec<ObScriptValue>, Option<u32>)>,
        globals: std::collections::HashMap<u32, f32>,
    }

    impl ObScriptHost for Recorder {
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
        payload.extend(std::iter::repeat(0u8).take(pad)); // 'r' secondary u16
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

    /// Local escape bytes: `' s<n>'`.
    fn s(n: u16) -> Vec<u8> {
        let mut v = vec![b' ', b's'];
        v.extend_from_slice(&n.to_le_bytes());
        v
    }

    /// Ref escape bytes: `' r<n>'`.
    fn r(n: u16) -> Vec<u8> {
        let mut v = vec![b' ', b'r'];
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
        let script = script(
            wrap_body(body),
            vec![local(1, "x", 1)],
            vec![],
        );
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
                ref_vars: Default::default(),
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
                ref_vars: Default::default(),
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
    fn expression_call_and_ref_arguments() {
        // set x to GetStage QuestRef + 1
        // args for the X call: count=1, [r1]
        let x_args = args_blob(&[ra(1)]);
        let expr = [x(GETSTAGE, &x_args), b"1 + ".to_vec()].concat();
        let body = set_stmt(b'f', 1, &expr, 0);
        let script = script(
            wrap_body(body),
            vec![local(1, "x", 0)],
            vec![0x000ABCDE],
        );
        let program = ObScriptProgram::new(&script);
        let mut state = VmState::default();
        let mut host = Recorder::default();
        let outcome = program.run_block(BLOCK_GAME_MODE, &mut state, &mut host);
        assert_eq!(outcome, BlockOutcome::Completed);
        assert_eq!(state.locals.get(&1), Some(&43.0)); // GetStage stub 42 + 1
        assert!(host.calls.iter().any(|(cmd, args, _)| {
            *cmd == GETSTAGE
                && matches!(args.first(), Some(ObScriptValue::Ref(0x000ABCDE)))
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

    #[test]
    fn return_stops_the_block() {
        let body = [
            simple_stmt(0x1e),                                // Return
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
}
