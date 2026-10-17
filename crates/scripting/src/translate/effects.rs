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
//!
//! ## Layout (#5801)
//!
//! This file is the hub: the imports every layer shares and a re-export of
//! each layer, so every `translate::effects::*` path still resolves.
//!
//! - `model` — the effect IR: [`Effect`], [`FragmentGuard`], [`ValueExpr`],
//!   the [`ActorRef`] receivers and the argument types (all saved).
//! - `guards` — guard + value classification (`classify_condition`,
//!   `classify_value`).
//! - `scope` — the lowering `Scope` (the fragment's local environment).
//! - `lower` — statement lowering and the `lower_*_fragment` entry points.
//! - `inline` — quest-script inlining through a [`ScriptLibrary`].
//! - `vm_bridge` — the Papyrus-VM-facing suspension state and
//!   `classify_native_*` entry points.
//! - `prims` — `EFFECT_PRIMITIVES` (the first-match ordering contract, one
//!   table) and the per-family `prim_*` classifiers.
//! - `receivers` — receiver / argument resolvers shared by all of the above.

use std::collections::{HashMap, HashSet};

use byroredux_core::ecs::components::MotionType;
use byroredux_papyrus::ast::{BinaryOp, Expr, Stmt, UnaryOp};
use byroredux_papyrus::span::Spanned;

use crate::cinematic::CinematicAnimationEvent;
use crate::papyrus_provider::{lower_provider_call, MAX_PROVIDER_WAIT_SECONDS, PapyrusProviderCatalog};
use crate::player_control::PlayerControlSelection;
use crate::translate::compose::{
    as_num, int_arg, is_game_get_player, method_call, method_call_any, quest_via, ObjectRef,
    QuestRef,
};

mod guards;
mod inline;
mod lower;
mod model;
mod prims;
mod receivers;
mod scope;
mod vm_bridge;

pub use inline::*;
pub use lower::*;
pub use model::*;
pub use vm_bridge::*;

use guards::*;
use prims::*;
use receivers::*;
use scope::*;

/// Concatenated text of every production file of this module, for the
/// source-shape scans that used to `include_str!("effects.rs")` when this
/// was one file (#5801, the #3854 pattern). A scan pointed at one post-split
/// file would silently narrow to whatever that file happens to hold and
/// still pass; scanning the concatenation keeps it whole wherever an item
/// lands. `effects/tests.rs` is deliberately absent so a test's own source
/// cannot satisfy a scan. [`sources_completeness`] pins the list.
#[cfg(test)]
const SOURCES: &str = concat!(
    include_str!("effects.rs"),
    include_str!("effects/guards.rs"),
    include_str!("effects/inline.rs"),
    include_str!("effects/lower.rs"),
    include_str!("effects/model.rs"),
    include_str!("effects/prims/mod.rs"),
    include_str!("effects/prims/actor_state.rs"),
    include_str!("effects/prims/ai_combat.rs"),
    include_str!("effects/prims/inventory.rs"),
    include_str!("effects/prims/lock.rs"),
    include_str!("effects/prims/placement.rs"),
    include_str!("effects/prims/player_control.rs"),
    include_str!("effects/prims/quest_scoped.rs"),
    include_str!("effects/prims/scene.rs"),
    include_str!("effects/prims/vehicle_cinematic.rs"),
    include_str!("effects/prims/world.rs"),
    include_str!("effects/receivers.rs"),
    include_str!("effects/scope.rs"),
    include_str!("effects/vm_bridge.rs"),
);

#[cfg(test)]
mod tests;

/// Pins that [`SOURCES`] actually covers the module (#5801).
#[cfg(test)]
mod sources_completeness {
    /// Every production file under `translate/effects/` (recursively — the
    /// `prims/` families included) must appear in the [`super::SOURCES`]
    /// concat, or the scans that read it silently stop covering whatever
    /// moved into the missing file — and still pass. The directory is walked
    /// at test time because a hardcoded list cannot see the file nobody
    /// wrote down.
    #[test]
    fn every_production_file_is_in_the_sources_concat() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/translate");
        let this_file = include_str!("effects.rs");
        let (_, concat) = this_file
            .split_once("const SOURCES: &str = concat!(")
            .expect("the SOURCES concat! must exist");
        let concat = concat
            .split_once(");")
            .expect("the SOURCES concat! must be closed")
            .0;
        assert!(
            concat.contains("include_str!(\"effects.rs\")"),
            "the hub itself must stay in the SOURCES concat!"
        );

        let mut checked = 0usize;
        let mut pending = vec![src.join("effects")];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).expect("translate/effects/ must exist") {
                let path = entry.expect("readable dir entry").path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let rel = path
                    .strip_prefix(&src)
                    .expect("walked under translate/")
                    .to_string_lossy()
                    .replace('\\', "/");
                if !rel.ends_with(".rs") || rel == "effects/tests.rs" {
                    continue;
                }
                assert!(
                    concat.contains(&format!("include_str!(\"{rel}\")")),
                    "translate/{rel} is missing from the SOURCES concat!. Add it — \
                     every source-shape scan reads SOURCES, so an absent file is \
                     uncovered code that still reports green."
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 18,
            "expected the post-#5801 effects/ submodules, walked {checked}"
        );
    }
}
