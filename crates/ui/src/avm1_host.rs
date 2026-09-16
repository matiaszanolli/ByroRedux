//! AVM1 (`ActionScript 2`) host-call scanner — the Skyrim-side counterpart of
//! [`crate::avm2_host`]'s `BGSCodeObj` walker (#3103).
//!
//! `SKYRIM_SKYUI_METHODS` was sourced from a point-in-time read of SkyUI's
//! decompiled sources and never measured against a real installed archive the
//! way #2966 regenerated the Fallout 4 catalog from a 311-movie sweep. That
//! sweep works because AVM2 bytecode has a scannable shape; AVM1 is a
//! different format with no scanner in this codebase, which is what this
//! module adds.
//!
//! # What a call site actually looks like
//!
//! Derived by dumping real action streams out of Skyrim SE's
//! `Skyrim - Interface.bsa`, not from a format document. `GameDelegate.call`
//! compiles to an ordinary AVM1 method call, and AVM1 pushes a method call's
//! operands in the order `args…, arg_count, receiver, method_name`:
//!
//! ```text
//! Push["CloseMenu", 2, "gfx"] | GetVariable
//!   | Push["io"]           | GetMember
//!   | Push["GameDelegate"] | GetMember
//!   | Push["call"]         | CallMethod
//! ```
//!
//! Reading the last `Push` right-to-left: `"gfx"` starts the
//! `gfx.io.GameDelegate` member chain, `2` is the argument count, and
//! `"CloseMenu"` is `call`'s first argument — the host method name. Vanilla
//! movies always spell the receiver out; a movie using `import
//! gfx.io.GameDelegate` would resolve the short name through `GetVariable`
//! instead, which is why the receiver is matched on its **last** path
//! component rather than on the whole chain.
//!
//! # Two things that make a naive walk read almost nothing
//!
//! Both were found the same way, by measuring:
//!
//! 1. **Function bodies are nested byte slices.** `DefineFunction` /
//!    `DefineFunction2` carry their body as `actions: &[u8]`, not inline in
//!    the enclosing stream, and essentially every host call lives inside a
//!    method. Walking only the top level of each tag finds **1** call site in
//!    the whole corpus; recursing finds the real population.
//!    (`installed_skyrim_host_calls_are_all_cataloged` reports the corpus
//!    size — 53 movies of `Skyrim - Interface.bsa`, 35 of them calling the
//!    host. An earlier revision of this doc said 46; the sweep is the
//!    authority and there is no second number to keep in step.)
//! 2. **String literals are constant-pool indices.** `ActionConstantPool`
//!    sets the pool for the block, and a nested body inherits the pool in
//!    force where it was defined, so the pool has to descend with the walk.
//!
//! # Why it is conservative, and how you can tell
//!
//! The walk models the operand stack for the handful of actions the call
//! shape uses and **clears** it on anything else, rather than guessing at an
//! unmodelled action's arity — a desynced stack is how a scanner invents
//! method names that no movie contains. Clearing can only lose a call site,
//! never invent one, and [`Avm1HostCallInventory::unresolved`] counts the
//! sites that were recognised as `GameDelegate.call` but whose name did not
//! survive: it reads **0** on the vanilla corpus, which is what makes the
//! resulting inventory a measurement rather than a sample.

use std::collections::BTreeSet;

use swf::avm1::read::Reader as Avm1Reader;
use swf::avm1::types::{Action, Value};
use swf::{decompress_swf, parse_swf, Tag};

/// Receiver whose `call` method reaches the game. Matched on the last
/// component of the member chain — see the module doc.
const HOST_DELEGATE: &str = "GameDelegate";

/// The method on [`HOST_DELEGATE`] that forwards to the host.
const HOST_DELEGATE_CALL: &str = "call";

/// What one movie's scan found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Avm1HostCallInventory {
    /// Distinct host method names passed to `GameDelegate.call`.
    pub methods: BTreeSet<String>,
    /// `GameDelegate.call` sites whose first argument did not resolve to a
    /// literal — a dynamic name, or a stack the walk had to clear.
    ///
    /// Reported rather than silently dropped because it is the difference
    /// between "this movie calls nothing else" and "this walk could not
    /// see what else it calls". A sweep asserting the catalog is complete
    /// is only sound while this is 0.
    pub unresolved: usize,
}

impl Avm1HostCallInventory {
    fn merge(&mut self, other: Self) {
        self.methods.extend(other.methods);
        self.unresolved += other.unresolved;
    }
}

/// Scan a compressed or uncompressed SWF for `GameDelegate.call` sites.
pub fn referenced_host_methods(swf_data: &[u8]) -> Result<Avm1HostCallInventory, String> {
    let decompressed =
        decompress_swf(swf_data).map_err(|error| format!("decompressing SWF: {error}"))?;
    let movie = parse_swf(&decompressed).map_err(|error| format!("parsing SWF: {error}"))?;
    let version = movie.header.version();
    let mut found = Avm1HostCallInventory::default();
    scan_tags(&movie.tags, version, &mut found);
    Ok(found)
}

/// Collect every action block a movie carries: frame scripts, class
/// initialisers, and the same two inside every sprite's own timeline.
fn scan_tags(tags: &[Tag<'_>], version: u8, found: &mut Avm1HostCallInventory) {
    for tag in tags {
        match tag {
            Tag::DoAction(code) => found.merge(scan_block(code, version, &[])),
            Tag::DoInitAction { action_data, .. } => {
                found.merge(scan_block(action_data, version, &[]))
            }
            Tag::DefineSprite(sprite) => scan_tags(&sprite.tags, version, found),
            _ => {}
        }
    }
}

/// One value the walk can still reason about. Anything else is
/// [`StackValue::Opaque`] — present, so arities stay aligned, but unreadable.
#[derive(Clone, Debug)]
enum StackValue {
    /// A string *literal* from the code or the constant pool.
    Str(String),
    /// The result of a variable or member read, named by the last component
    /// of its path.
    ///
    /// Kept apart from [`Self::Str`] because the two are the same bytes and
    /// opposite facts. `GameDelegate.call(someVar, args)` puts a variable
    /// read where a literal name would sit, and folding the two together
    /// reports a host method called `someVar` — a name no movie contains and
    /// no catalog can match, arriving through exactly the door a measurement
    /// is supposed to keep shut. Only a literal can name a method; only an
    /// object read can be the receiver.
    Object(String),
    Num(f64),
    Opaque,
}

impl StackValue {
    /// The value as a string *literal*, or `None` for anything else.
    fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(value) => Some(value),
            _ => None,
        }
    }

    /// The name of the object this value refers to, or `None` if it is not
    /// the result of a variable or member read.
    fn as_object(&self) -> Option<&str> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    fn as_count(&self) -> Option<usize> {
        match self {
            // AVM1 pushes small integers as `Int`, but a compiler is free to
            // emit the same count as a `Double` — both appear in the vanilla
            // corpus on `InitArray`, and rejecting one of them desyncs the
            // stack on the very shape this scanner exists to read.
            Self::Num(value) if *value >= 0.0 && value.fract() == 0.0 => Some(*value as usize),
            _ => None,
        }
    }
}

/// The object a variable or member read resolves to, named by the component
/// just read. An `import`ed short name and a spelled-out `gfx.io.GameDelegate`
/// therefore land on the same name, which is why the receiver is matched on
/// the last component rather than the whole chain.
fn as_object(value: Option<StackValue>) -> StackValue {
    match value {
        Some(StackValue::Str(name)) | Some(StackValue::Object(name)) => StackValue::Object(name),
        _ => StackValue::Opaque,
    }
}

/// Walk one action block, recursing into the function bodies it defines.
///
/// `pool` is the constant pool in force on entry — a nested body inherits the
/// enclosing block's, and may replace it with its own `ActionConstantPool`.
fn scan_block(code: &[u8], version: u8, pool: &[String]) -> Avm1HostCallInventory {
    let mut found = Avm1HostCallInventory::default();
    let mut pool = pool.to_vec();
    let mut stack: Vec<StackValue> = Vec::new();
    // AVM1's register file. Compilers hoist a repeated operand — very often
    // the receiver, sometimes the method name — into a register and push it
    // back by index, so a walk that cannot read registers reports those sites
    // as unresolved. Each body gets its own file, which is what the runtime
    // does for a `DefineFunction2` with its own register count.
    let mut registers: Vec<StackValue> = vec![StackValue::Opaque; 256];
    let mut reader = Avm1Reader::new(code, version);

    loop {
        // A malformed or truncated body ends this block rather than the
        // scan: one unreadable function must not cost a movie every call
        // site outside it.
        let Ok(action) = reader.read_action() else {
            break;
        };
        match action {
            Action::End => break,
            Action::ConstantPool(constants) => {
                pool = constants
                    .strings
                    .iter()
                    .map(|entry| entry.to_string_lossy(swf::UTF_8))
                    .collect();
            }
            Action::Push(push) => {
                for value in &push.values {
                    stack.push(match value {
                        Value::Str(text) => StackValue::Str(text.to_string_lossy(swf::UTF_8)),
                        Value::ConstantPool(index) => pool
                            .get(*index as usize)
                            .map(|entry| StackValue::Str(entry.clone()))
                            .unwrap_or(StackValue::Opaque),
                        Value::Int(value) => StackValue::Num(f64::from(*value)),
                        Value::Float(value) => StackValue::Num(f64::from(*value)),
                        Value::Double(value) => StackValue::Num(*value),
                        Value::Register(index) => registers
                            .get(*index as usize)
                            .cloned()
                            .unwrap_or(StackValue::Opaque),
                        _ => StackValue::Opaque,
                    });
                }
            }
            // `a.b` — the member name is what identifies the receiver, so the
            // result keeps it. That is also what lets an `import`ed short
            // name and a spelled-out `gfx.io.GameDelegate` match the same way.
            Action::GetMember => {
                let member = stack.pop();
                stack.pop();
                stack.push(as_object(member));
            }
            Action::GetVariable => {
                let name = stack.pop();
                stack.push(as_object(name));
            }
            // Stores the top *without* popping it. Clearing here instead
            // would lose the operands already staged for the call below it,
            // which is the single largest source of unresolved sites on the
            // real corpus.
            Action::StoreRegister(store) => {
                if let Some(slot) = registers.get_mut(store.register as usize) {
                    *slot = stack.last().cloned().unwrap_or(StackValue::Opaque);
                }
            }
            Action::PushDuplicate => {
                stack.push(stack.last().cloned().unwrap_or(StackValue::Opaque));
            }
            Action::StackSwap => {
                let len = stack.len();
                if len >= 2 {
                    stack.swap(len - 1, len - 2);
                } else {
                    stack.clear();
                }
            }
            Action::Pop => {
                stack.pop();
            }
            Action::InitArray => {
                let Some(count) = stack.pop().and_then(|value| value.as_count()) else {
                    stack.clear();
                    continue;
                };
                if count > stack.len() {
                    stack.clear();
                    continue;
                }
                stack.truncate(stack.len() - count);
                stack.push(StackValue::Opaque);
            }
            Action::CallMethod => {
                let method = stack.pop();
                let receiver = stack.pop();
                let count = stack.pop();
                let is_host_call = method.as_ref().and_then(StackValue::as_str)
                    == Some(HOST_DELEGATE_CALL)
                    && receiver.as_ref().and_then(StackValue::as_object) == Some(HOST_DELEGATE);
                let Some(count) = count.as_ref().and_then(StackValue::as_count) else {
                    if is_host_call {
                        found.unresolved += 1;
                    }
                    stack.clear();
                    continue;
                };
                if count > stack.len() {
                    if is_host_call {
                        found.unresolved += 1;
                    }
                    stack.clear();
                    continue;
                }
                let args = stack.split_off(stack.len() - count);
                if is_host_call {
                    // `call(methodName, argsArray)` — the host method name is
                    // the first argument, which AVM1 pushes last, so it sits
                    // nearest the top of the operand run.
                    match args.last().and_then(StackValue::as_str) {
                        Some(name) => {
                            found.methods.insert(name.to_string());
                        }
                        None => found.unresolved += 1,
                    }
                }
                stack.push(StackValue::Opaque);
            }
            Action::DefineFunction(function) => {
                found.merge(scan_block(function.actions, version, &pool));
                stack.clear();
            }
            Action::DefineFunction2(function) => {
                found.merge(scan_block(function.actions, version, &pool));
                stack.clear();
            }
            // Deliberately not modelled. Guessing an unmodelled action's
            // arity desyncs the stack, and a desynced stack is how a scanner
            // reports method names no movie contains; clearing can only lose
            // a site, and `unresolved` says when that happened on one that
            // mattered.
            _ => stack.clear(),
        }
    }

    found
}

#[cfg(test)]
mod tests;
