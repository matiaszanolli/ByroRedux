//! The Papyrus VM (M47.2): runs a fragment the effect lowering declines.
//!
//! The lowering in [`crate::translate::effects`] turns a fragment into a flat
//! effect list ahead of time, which is exact but cannot express a value it
//! only learns at run time: an object local assigned from a call
//! (`ObjectReference x = Alias.GetRef()`), a quest-script function that
//! returns a value, a loop. This module interprets such a fragment over its
//! decompiled AST instead. It owns only control flow and values; every native
//! still goes through the effect runtime:
//!
//! * a native *statement* (`x.Enable()`) is re-written with its evaluated
//!   receiver and arguments bound to synthetic names — a form as a property
//!   of a synthetic VMAD, a quest or the player through
//!   [`NativeBindings`], numbers as literals — and classified by the same
//!   primitive table the lowering uses, then applied by `apply_effects`;
//! * a native *read* (`x.IsDisabled()`, `Q.GetStage()`) is classified the
//!   same way as a [`crate::translate::effects::ValueExpr`] or as guards and
//!   evaluated by the fragment runtime;
//! * the object reads no effect models — alias references, `GetPlayer`,
//!   `GetLinkedRef`, base objects, `GetOwningQuest` — are read here.
//!
//! A call into a quest's script (`kmyQuest.Startup()`, or one returning a
//! value) runs that function here too, against the quest's own VMAD.
//!
//! Script state follows the CK's *States (Papyrus)*: a call runs the version
//! in the instance's current state down the `extends` chain, then the empty
//! state's; an instance starts in its `Auto State`. The compiler's generated
//! `GotoState` / `GetState` keep the state in `::State`, stored per script
//! among its owner's script variables. Script variables — numbers in
//! [`crate::QuestScriptVariables`], objects and strings in
//! [`crate::QuestScriptObjects`] — are keyed by the object the script is
//! attached to; an unwritten one reads what the script declares.
//!
//! `Utility.Wait` suspends the run anywhere in a statement call chain; the
//! frames ride an `Effect::Interpret` resume on the continuation queue. The
//! per-frame statement budget ([`super::budget`], #5763) suspends a run the
//! same way, between two statements, with a zero delay; see there for what
//! cannot be split.
//!
//! [`check_function`] decides ahead of time, against typed placeholders,
//! whether every statement and call of a fragment (and of the script
//! functions it reaches, in every state they have) is one this VM runs, so
//! populate installs `Effect::Interpret` only for fragments that run whole.
//! A run that still meets something it cannot do — a native that declines
//! its actual values — stops there and logs it.
//!
//! A call or member access through a cast or a typed variable reaches the
//! script the VM runs on another reference, as that reference's.
//!
//! Structs and array methods run (`Value::Struct`, 20cbfd7b7). What is
//! genuinely not modelled: a struct or array held across `Utility.Wait` —
//! the saved continuation stores scalars only (scripting.md § continuations).
//!
//! The module is split by responsibility and, for the natives, by Papyrus
//! API family (#5800): `value` (values and their marshalling),
//! `script_model` (what a call runs, declared members, call shapes),
//! `interp` (runs, frames, statements, expressions), `registrations`,
//! `checker` (the ahead-of-time check) and `natives/` (one file per API
//! family, holding both the run-time read and the check of its calls).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_papyrus::ast::{
    AssignOp, BinaryOp, CallArg, Expr, FunctionFlags, Identifier, Param, Script, ScriptItem, StateItem,
    Stmt, Type, UnaryOp,
};
use byroredux_plugin::esm::records::condition::ConditionStringId;
use byroredux_papyrus::span::{Span, Spanned};
use byroredux_plugin::esm::records::script_instance::{
    PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
};

use super::effects::{apply_effects, evaluate_guard, evaluate_value, DeferredFragmentEffects};
use crate::papyrus_provider::MAX_PROVIDER_WAIT_SECONDS;
use crate::quest_stages::{QuestFormId, QuestObjectiveState, QuestStageAdvanced, QuestStageState};
use crate::translate::compose::vmad_property_name;
use crate::translate::effects::{
    classify_native_effect, classify_native_guard, classify_native_value, NativeBindings, VmFrame,
    VmResume, VmStep, VmStored,
};

/// The player's reference, `PlayerRef`.
const PLAYER_REF: u32 = 0x14;
/// Fallout 4's `Caps001` (`Game.GetCaps`), form `0x0F` of `Fallout4.esm`.
const FO4_CAPS: u32 = 0x0F;
/// Skyrim's `Gold001` (`Actor.GetGoldAmount`), form `0x0F` of `Skyrim.esm`.
const SKYRIM_GOLD: u32 = 0x0F;
/// Starfield's `Credits` (`Game.GetCredits`), form `0x0F` of `Starfield.esm`.
const STARFIELD_CREDITS: u32 = 0x0F;
/// Statements one run may execute before it is stopped (a runaway loop).
const MAX_STEPS: usize = 100_000;
/// Script-function nesting.
const MAX_DEPTH: usize = 16;
/// Elements one array may hold — matches the static lowering's ceiling
/// (`papyrus_provider/lower_call.rs` declines `new T[n]` past this), so a
/// script the lowering accepts cannot be stopped here for size.
const MAX_ARRAY_ELEMENTS: usize = byroredux_sdk::script_function::MAX_SCRIPT_ARRAY_ELEMENTS;
/// Bytes in one string result — `s = s + s` doubles per statement and would
/// reach terabytes long before `MAX_STEPS` bites.
const MAX_STRING_BYTES: usize = byroredux_sdk::script_function::MAX_SCRIPT_STRING_BYTES;
/// Hops a "follow the linked-ref chain" native walks (#5844). The CK
/// documents `iMaxExpectedLinkedRefs` with default 100 — the documented
/// value is the ceiling for every chain walk (loop guard, caller-supplied
/// limits, and `GetNthLinkedRef`'s hop count), so the three spellings that
/// used to disagree (256 / 100 / 64) are one concept.
const MAX_LINK_CHAIN_HOPS: usize = 100;

mod checker;
mod interp;
mod natives;
mod registrations;
mod script_model;
mod value;

use checker::*;
use interp::*;
use natives::*;
use registrations::*;
use script_model::*;
use value::*;

pub use checker::{check_alias_script, check_function, check_function_reason, check_reference_script};
pub(super) use checker::{handler_runs, SelfKind};
pub(super) use interp::run_fragment;
pub use value::Value;

/// Where the check resolves scripts: the library populate supplies.
pub use crate::translate::effects::ScriptLibrary as CheckLibrary;

#[cfg(test)]
mod tests;
