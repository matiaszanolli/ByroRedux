//! The *Activate* hook (PERK-HOOK-007, PERK-HOOK-012): what the player's perks add to activating a reference,
//! and what picking it runs.
//!
//! Sources (the local GECK and CK wiki dumps, xEdit, UESP):
//!
//! * GECK *Entry Point*: *Activate* is "Run when the player activates something" and "Presents a choice list
//!   to the player that can trigger a script".
//! * CK *Perk Entry Point*, *Activate*: "Adds a new choice to show in addition to the normal activation
//!   prompt". *Run Immediately*: "If checked, there is no selection menu. This is run in addition to the
//!   default activation". *Replace Default*: "This is run in place of the default activation". *Apply Spell*:
//!   "The contact spell to apply to the target". The fragment's `akTargetRef` is "the object reference that
//!   was activated" and `akActor` "the actor that did the activation (perk owner)".
//! * CK *Perk Fragments*: "NPCs won't run activate entry point perk entries", so only the player's activations
//!   compose them. That rule is the caller's: [`compose_activate_choices`] composes for whoever it is given.
//! * The condition tabs: tab 0 is the perk owner and tab 1 the activated reference. That is xEdit FNV's
//!   `epcTarget` on hook 27, and UESP *Skyrim Mod:Mod File Format/PERK*'s "Perk Owner (0) and Target (1)" for
//!   Skyrim's hook 14.
//!
//! What a choice runs is the game's own. An FO3 / FNV entry runs its embedded script as a *reference* script
//! whose implicit reference is the activated reference: vanilla `RoboticsExpert`'s `SetUnconscious 1` shuts
//! down the robot itself, and `Cannibal`'s `StartCannibal Player` passes the player as an argument. A Skyrim
//! entry runs the fragment its `EPF3` *Fragment Index* binds (PERK-DEC-011), with `akTargetRef` the activated
//! reference, and casts its *Apply Spell* on that reference. Fallout 4's leg waits on its *Run Immediately*
//! timing ([`super::PerkCalculation::Activate`]).

use std::sync::Arc;

use byroredux_core::character::Perks;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{PerkEntryDetails, PerkFunctionData, ScriptRecord};

use super::{compose_order_key, entry_applies_at, tabs_pass, PerkBodyKind, PerkCalculation, PerkEntryTable};

/// What an *Add Activate Choice* entry (function 9) carries.
#[derive(Debug, Clone, PartialEq)]
pub struct PerkActivateChoice {
    /// The entry's `PRKE` priority, which orders the choices like a composition ([`compose_order_key`]).
    pub priority: u8,
    /// `EPF2`, the choice's button label. Empty on a *Run Immediately* entry, which has no button.
    pub label: String,
    pub run_immediately: bool,
    pub replace_default: bool,
    /// The fragment the entry binds: Skyrim's `EPF3` *Fragment Index* or Fallout 4's `EPFB` entry id, whichever
    /// the decode filled for the game.
    pub fragment: Option<u32>,
    /// *Apply Spell* (`EPFT` 4's spell on Skyrim and Fallout 4), cast on the activated reference.
    pub spell: Option<u32>,
    /// The FO3 / FNV embedded script.
    pub script: Option<Arc<ScriptRecord>>,
}

impl PerkActivateChoice {
    pub(super) fn from_entry(priority: u8, data: &PerkFunctionData, details: &PerkEntryDetails) -> Self {
        Self {
            priority,
            label: details.button_label.clone(),
            run_immediately: details.run_immediately,
            replace_default: details.replace_default,
            fragment: details.fragment_index.or(details.entry_id).map(u32::from),
            spell: match data {
                PerkFunctionData::FormId(spell) if *spell != 0 => Some(*spell),
                _ => None,
            },
            script: details.script.clone().map(Arc::new),
        }
    }
}

/// One activate entry of the activator's perks whose conditions pass for this activation.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivateChoiceOffer {
    pub perk: u32,
    pub choice: Arc<PerkActivateChoice>,
}

/// The activate entries `activator`'s perks offer for activating `target` (PERK-HOOK-007): each *Add Activate
/// Choice* entry on the dialect's *Activate* hook, at the rank held, whose tabs pass with `target` as the
/// Target subject (tab 1). Ordered as a composition orders its entries ([`compose_order_key`]). Empty when the
/// world has no perk table or the dialect has no *Activate* row.
pub fn compose_activate_choices(world: &World, activator: EntityId, target: EntityId) -> Vec<ActivateChoiceOffer> {
    // Snapshot the candidates, then let every guard go: the condition evaluator takes its own locks.
    let mut due = {
        let Some(table) = world.try_resource::<PerkEntryTable>() else {
            return Vec::new();
        };
        let dialect = table.dialect();
        let Some(hook) = PerkCalculation::Activate.hook(dialect) else {
            return Vec::new();
        };
        let Some(perks) = world.get::<Perks>(activator) else {
            return Vec::new();
        };
        let mut due = Vec::new();
        for (perk_order, held) in perks.entries.iter().enumerate() {
            let Some(entries) = table.bodies(held.perk_form_id) else {
                continue;
            };
            for (entry_order, entry) in entries.iter().enumerate() {
                let PerkBodyKind::ActivateChoice { entry_point, choice } = &entry.kind else {
                    continue;
                };
                if *entry_point == hook && entry_applies_at(entry.rank, held.rank) {
                    let key = compose_order_key(dialect, choice.priority, perk_order, entry_order);
                    due.push((key, held.perk_form_id, choice.clone(), entry.tabs.clone()));
                }
            }
        }
        due
    };
    due.sort_by_key(|(key, ..)| *key);
    due.into_iter()
        .filter(|(_, _, _, tabs)| tabs_pass(world, activator, tabs, &[Some(target)]))
        .map(|(_, perk, choice, _)| ActivateChoiceOffer { perk, choice })
        .collect()
}

/// What running one activate entry did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActivateChoiceRun {
    /// The FO3 / FNV embedded script ran.
    pub script: bool,
    /// A bound fragment was reported for [`crate::perk_fragment_system`] to run.
    pub fragment: bool,
    /// *Apply Spell* landed on the activated reference.
    pub spell: bool,
}

impl ActivateChoiceRun {
    /// Whether anything ran.
    pub fn ran(self) -> bool {
        self.script || self.fragment || self.spell
    }
}

/// Run what an activate entry carries, for `activator` activating `target` (PERK-HOOK-012): the embedded
/// script with `target` as its implicit reference, the fragment its index binds with `target` as
/// `akTargetRef`, and *Apply Spell* cast by `activator` on `target`.
pub fn run_activate_choice(
    world: &World,
    offer: &ActivateChoiceOffer,
    activator: EntityId,
    target: EntityId,
) -> ActivateChoiceRun {
    let mut run = ActivateChoiceRun::default();
    let choice = &offer.choice;
    let target_form = crate::condition::entity_global_form_id(world, target);
    if let Some(script) = &choice.script {
        match target_form {
            Some(form) => {
                let outcome = crate::obscript_results::run_result_script(world, form, Some(target), script);
                log::info!(
                    "perk {:08X} activate entry {:?} ran its script on {form:08X}: {outcome:?}",
                    offer.perk,
                    choice.label
                );
                run.script = true;
            }
            None => log::warn!(
                "perk {:08X} activate entry {:?}: the activated entity {target} has no reference FormID, so its \
                 script cannot run",
                offer.perk,
                choice.label
            ),
        }
    }
    if let Some(fragment) = choice.fragment {
        let bound = world
            .try_resource::<crate::PerkFragments>()
            .and_then(|fragments| fragments.get(offer.perk).map(|set| set.fragments.iter().any(|(on, ..)| on.0 == fragment)))
            .unwrap_or(false);
        let owner = crate::condition::entity_global_form_id(world, activator);
        if let (true, Some(owner)) = (bound, owner) {
            crate::report_perk_entry(
                world,
                crate::PerkEntryRun { perk: offer.perk, entry: fragment, owner, target: target_form },
            );
            run.fragment = true;
        }
    }
    if let Some(spell) = choice.spell {
        run.spell = crate::magic_effects::cast_spell(world, Some(activator), target, spell);
    }
    run
}
