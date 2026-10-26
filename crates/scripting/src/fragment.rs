//! Quest-stage *fragment* dispatch — the runtime half of the b2 lever
//! ([`docs/engine/m47-2-recognizer-scaling.md`]).
//!
//! A Papyrus quest carries one compiled `Fragment_N` function per stage;
//! the runtime runs stage N's fragment when the quest is `SetStage`-d to
//! N. The [`crate::translate::effects`] lowerer turns each fragment body
//! into a `Vec<Effect>`; this module stores those per `(quest, stage)`
//! and applies them when a [`QuestStageAdvanced`] marker says the stage
//! was set.
//!
//! ## What ships here
//!
//! - **Engine + contract + dispatch:** the [`QuestStageFragments`]
//!   resource, [`apply_effects`] against the canonical [`QuestStageState`]
//!   / [`QuestObjectiveState`], and [`quest_fragment_dispatch_system`]
//!   which consumes `QuestStageAdvanced` and cascades chained `SetStage`s
//!   (bounded).
//! - **Population (shipped, #1739 / `8a70b81a`):** [`QuestStageFragments`]
//!   is filled from real game data via the QUST `VMAD` fragment-section
//!   decoder
//!   (`byroredux_plugin::esm::records::script_instance::parse_quest_fragments`)
//!   feeding [`populate_quest_fragments_from_pex`], wired live from the
//!   cell loader. Validated end-to-end on real Skyrim data.
//!
//! ## Quest-ref resolution
//!
//! A fragment's effect targets `Self` / `Self.GetOwningQuest()` (→ the
//! advancing quest, known at dispatch) or a `Quest Property` (→ a *different*
//! quest bound by the QUST's own VMAD). The former always resolves; the
//! latter needs the quest's VMAD scripts-section registered via
//! [`QuestStageFragments::insert_vmad`] (the same bytes the fragment-binding
//! decoder reads, decoded a second time for its property table) — a
//! `Property`-targeted effect with no VMAD on hand, or naming a property the
//! VMAD doesn't carry, is skipped (logged), never guessed.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use byroredux_core::ecs::components::{
    EquipmentSlots, GlobalTransform, Inventory, InventoryIndex, ItemStack, Locked, Transform,
};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::script_instance::{
    PropertyValue, SceneFragmentEvent, ScriptInstanceData,
};

use crate::quest_stages::{
    QuestFormId, QuestObjectiveState, QuestStageAdvanced, QuestStageAdvancedBatch, QuestStageState,
    FRAGMENT_QUEST_EVENT_SUBSCRIBER,
};
use byroredux_papyrus::ast::{Script, ScriptItem, StateItem, Stmt, Type};
use byroredux_papyrus::span::Spanned;

use crate::translate::compose::{ObjectRef, QuestRef};
use crate::translate::effects::{ActorRef, Effect};

pub mod budget;
mod alias_scripts;
mod effects;
mod distance;
mod dispatch;
mod anim_events;
mod events;
mod hits;
mod owners;
mod package;
mod perk;
mod populate;
mod reference;
mod state;
mod terminal;
mod systems;
mod updates;
pub mod vm;

pub use budget::{VmFrameBudget, VmFrameUse, DEFAULT_FRAME_BUDGET, RUN_START_COST};
pub use alias_scripts::*;
pub use effects::*;
pub use distance::*;
pub(crate) use dispatch::{flush_advances, run_papyrus_call};
pub use anim_events::*;
pub use hits::*;
pub use events::{EventRegistration, EventRegistrations, EventRequest, REMOTE_EVENTS};
pub use package::*;
pub use perk::*;
pub use terminal::*;
pub use populate::*;
pub use reference::*;
pub use state::*;
pub use systems::*;
pub use updates::*;

/// Concatenated text of every production file in this module, for the
/// source-shape tests that used to `include_str!("../fragment.rs")` when this
/// was one file (#3854).
///
/// Those scans assert things about *where code sits* — that `apply_effect`'s
/// nested-lock contract is the doc comment of `apply_effect` itself (#3493),
/// that every type it acquires is named in that block (#3949), and that each
/// `.pex` entry point wraps its whole sequence in the panic net. Pointing each
/// one at a single post-split file would silently narrow it to whichever
/// fragment of the module that file happens to hold, and the assertion would
/// still pass — vacuously. Scanning the concatenation keeps them whole no
/// matter which file an item ends up in.
///
/// `tests.rs` is deliberately absent: these scans compose their needles at
/// runtime precisely so a test's own source cannot satisfy them.
#[cfg(test)]
pub(crate) const SOURCES: &str = concat!(
    include_str!("fragment/alias_scripts.rs"),
    include_str!("fragment/anim_events.rs"),
    include_str!("fragment/budget.rs"),
    include_str!("fragment/state.rs"),
    include_str!("fragment/effects.rs"),
    include_str!("fragment/effects/ai_combat.rs"),
    include_str!("fragment/effects/actor_lists.rs"),
    include_str!("fragment/effects/actor_values.rs"),
    include_str!("fragment/effects/location_alias.rs"),
    include_str!("fragment/effects/messages.rs"),
    include_str!("fragment/effects/player_stats.rs"),
    include_str!("fragment/effects/quest_vars.rs"),
    include_str!("fragment/effects/reference_state.rs"),
    include_str!("fragment/effects/deferred.rs"),
    include_str!("fragment/effects/evaluate.rs"),
    include_str!("fragment/effects/guards.rs"),
    include_str!("fragment/effects/inventory.rs"),
    include_str!("fragment/effects/lock.rs"),
    include_str!("fragment/effects/placement.rs"),
    include_str!("fragment/effects/player_control.rs"),
    include_str!("fragment/effects/quest_scoped.rs"),
    include_str!("fragment/effects/resolve.rs"),
    include_str!("fragment/effects/scene.rs"),
    include_str!("fragment/effects/vehicle_cinematic.rs"),
    include_str!("fragment/distance.rs"),
    include_str!("fragment/dispatch.rs"),
    include_str!("fragment/events.rs"),
    include_str!("fragment/hits.rs"),
    include_str!("fragment/owners.rs"),
    include_str!("fragment/package.rs"),
    include_str!("fragment/perk.rs"),
    include_str!("fragment/populate.rs"),
    include_str!("fragment/reference.rs"),
    include_str!("fragment/systems.rs"),
    include_str!("fragment/terminal.rs"),
    include_str!("fragment/updates.rs"),
    include_str!("fragment/vm.rs"),
    // #5800 — vm.rs split into vm/*.rs and vm/natives/*.rs.
    include_str!("fragment/vm/checker.rs"),
    include_str!("fragment/vm/interp.rs"),
    include_str!("fragment/vm/registrations.rs"),
    include_str!("fragment/vm/script_model.rs"),
    include_str!("fragment/vm/value.rs"),
    include_str!("fragment/vm/natives/mod.rs"),
    include_str!("fragment/vm/natives/actor.rs"),
    include_str!("fragment/vm/natives/alias.rs"),
    include_str!("fragment/vm/natives/animation.rs"),
    include_str!("fragment/vm/natives/collections.rs"),
    include_str!("fragment/vm/natives/combat.rs"),
    include_str!("fragment/vm/natives/destruction.rs"),
    include_str!("fragment/vm/natives/favor.rs"),
    include_str!("fragment/vm/natives/form_list.rs"),
    include_str!("fragment/vm/natives/forms.rs"),
    include_str!("fragment/vm/natives/game_utility.rs"),
    include_str!("fragment/vm/natives/linked_refs.rs"),
    include_str!("fragment/vm/natives/location.rs"),
    include_str!("fragment/vm/natives/ownership_crime.rs"),
    include_str!("fragment/vm/natives/quest.rs"),
    include_str!("fragment/vm/natives/reference.rs"),
    include_str!("fragment/vm/natives/scanning.rs"),
);

#[cfg(test)]
mod tests;

/// Pins that [`SOURCES`] actually covers the module (#3854).
#[cfg(test)]
mod sources_completeness {
    /// Every production file in `fragment/` must appear in the [`SOURCES`]
    /// concat, or the source-shape scans that read it silently stop covering
    /// whatever moved into the missing file — and still pass.
    ///
    /// The directory is walked at test time rather than compared against a
    /// hardcoded list, because a hardcoded list cannot see the file nobody
    /// wrote down: adding `fragment/foo.rs` and forgetting the `include_str!`
    /// is exactly the mistake this exists to catch, and a list would have to
    /// be updated by the same person making it.
    #[test]
    fn every_production_file_is_in_the_sources_concat() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fragment");
        let this_file = include_str!("fragment.rs");
        let (_, concat) = this_file
            .split_once("pub(crate) const SOURCES: &str = concat!(")
            .expect("the SOURCES concat! must exist");
        let concat = concat
            .split_once(");")
            .expect("the SOURCES concat! must be closed")
            .0;

        let mut checked = 0usize;
        for entry in std::fs::read_dir(&dir).expect("fragment/ must exist") {
            let path = entry.expect("readable dir entry").path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            // `tests.rs` is deliberately excluded from SOURCES: the scans
            // compose their needles at runtime so a test's own source cannot
            // satisfy them, and including it would undo that.
            if !name.ends_with(".rs") || name == "tests.rs" {
                continue;
            }
            assert!(
                concat.contains(&format!("include_str!(\"fragment/{name}\")")),
                "fragment/{name} is missing from the SOURCES concat!. Add it — \
                 every source-shape scan reads SOURCES, so an absent file is \
                 uncovered code that still reports green."
            );
            checked += 1;
        }
        assert!(
            checked >= 4,
            "expected at least the four post-#3854 submodules, walked {checked}"
        );

        // #5468 — `effects.rs` split into `effects/*.rs`; the same rule
        // applies one level down, or a scan would silently narrow to the
        // parent file's remainder.
        let effects_dir = dir.join("effects");
        let mut effect_files = 0usize;
        for entry in std::fs::read_dir(&effects_dir).expect("fragment/effects/ must exist") {
            let path = entry.expect("readable dir entry").path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with(".rs") {
                continue;
            }
            assert!(
                concat.contains(&format!("include_str!(\"fragment/effects/{name}\")")),
                "fragment/effects/{name} is missing from the SOURCES concat!."
            );
            effect_files += 1;
        }
        assert!(
            effect_files >= 10,
            "expected the post-#5468 effects/ submodules, walked {effect_files}"
        );

        // #5800 — `vm.rs` split into `vm/*.rs` and `vm/natives/*.rs`: the
        // same rule two levels down. `vm/tests.rs` is excluded like
        // `fragment/tests.rs`, for the same reason.
        let mut vm_files = 0usize;
        for sub in ["vm", "vm/natives"] {
            for entry in std::fs::read_dir(dir.join(sub)).expect("fragment/vm/ must exist") {
                let path = entry.expect("readable dir entry").path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !name.ends_with(".rs") || name == "tests.rs" {
                    continue;
                }
                assert!(
                    concat.contains(&format!("include_str!(\"fragment/{sub}/{name}\")")),
                    "fragment/{sub}/{name} is missing from the SOURCES concat!."
                );
                vm_files += 1;
            }
        }
        assert!(
            vm_files >= 20,
            "expected the post-#5800 vm/ and vm/natives/ submodules, walked {vm_files}"
        );
    }
}
