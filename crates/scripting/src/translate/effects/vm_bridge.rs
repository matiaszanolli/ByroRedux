//! The Papyrus-VM-facing bridge (#5801, split from `effects.rs`): the
//! suspension state a VM run saves across `Utility.Wait` ([`VmResume`] and
//! its frames / steps / stored locals) and the `classify_native_*` entry
//! points the VM ([`crate::vm`]) hands one native call or read to.

use super::*;

/// M47.2 — where a Papyrus VM run suspended by `Utility.Wait` resumes: per
/// suspended frame, the path of block positions down to the wait or the call
/// it waits in (each block's statement index, and for an `If` / `While` the
/// branch the run was in), and the frame's locals.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct VmResume {
    /// The suspended frames, outermost (the fragment) first; each caller's
    /// path ends at the call statement its callee suspended in.
    pub frames: Vec<VmFrame>,
    /// Statements the logical run has already executed when the frame budget
    /// (not a `Utility.Wait`) suspended it, so the per-run statement cap
    /// still bounds the whole run; 0 after a real wait, which starts the
    /// count afresh as it always did.
    pub carried_steps: u32,
}

/// One suspended function activation of a [`VmResume`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct VmFrame {
    /// The script attached to `owner` (its variables and properties).
    pub script: String,
    pub function: String,
    /// The object the script is attached to (`Self`): a quest or a
    /// reference.
    pub owner: u32,
    /// The script that defines the running body — `script` or one it
    /// extends — and the state block it is in (`None`: the empty state).
    /// The body is kept across the wait even when the state changes.
    pub defined_in: String,
    pub state: Option<String>,
    pub path: Vec<VmStep>,
    pub locals: Vec<(String, VmStored)>,
}

/// One position on a [`VmResume`] path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum VmStep {
    /// The statement at this index of the current block.
    Statement(u32),
    /// Inside an `If` (0 its body, `k` its `k`-th `ElseIf`, `u32::MAX` its
    /// `Else`) or a `While` (0, its body).
    Branch(u32),
}

/// A local's value across a suspension.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum VmStored {
    None,
    Bool(bool),
    Int(i32),
    Float(f32),
    Str(String),
    Form(u32),
    Alias { quest: u32, alias: i16 },
    /// An array an event passes (`Var[] akArgs`): its elements, each scalar.
    Array(Vec<VmStored>),
}

/// M47.2 — how the Papyrus VM ([`crate::vm`]) names the values it binds for
/// one native call or read it hands to this module's primitives. Every value
/// is a synthetic identifier: a quest or a player through these sets, any
/// other form as a property of the synthetic VMAD the VM applies the result
/// against; numbers, Bools and strings stay literals.
#[derive(Debug, Default, Clone)]
pub struct NativeBindings {
    pub players: HashSet<String>,
    pub quests: HashSet<String>,
}

impl NativeBindings {
    fn scope(&self) -> Scope<'static> {
        let mut scope = Scope {
            vmad_is_quest: true,
            ..Scope::default()
        };
        for name in &self.players {
            scope.player_locals.insert(name.to_ascii_lowercase());
        }
        for name in &self.quests {
            scope
                .quest_locals
                .insert(name.to_ascii_lowercase(), QuestRef::Property(name.clone()));
        }
        scope
    }
}

/// M47.2 — a native statement call with VM-bound values as an [`Effect`].
pub fn classify_native_effect(e: &Expr, bindings: &NativeBindings) -> Option<Effect> {
    let scope = bindings.scope();
    classify_effect(e, &scope)
}

/// M47.2 — a native numeric read with VM-bound values as a [`ValueExpr`].
pub fn classify_native_value(e: &Expr, bindings: &NativeBindings) -> Option<ValueExpr> {
    classify_value(e, &bindings.scope())
}

/// M47.2 — a native Bool read with VM-bound values as guards that hold
/// exactly when it is true.
pub fn classify_native_guard(e: &Expr, bindings: &NativeBindings) -> Option<Vec<FragmentGuard>> {
    let mut guards = Vec::new();
    classify_condition(e, &bindings.scope(), false, &mut guards)?;
    Some(guards)
}
