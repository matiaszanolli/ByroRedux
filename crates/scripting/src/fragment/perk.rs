//! M47.2 — perk (`PERK`) fragments: the `Fragment_Entry_NN` functions a Fallout 4 /
//! Starfield perk binds to its entries, run on the Papyrus VM with `akTargetRef`
//! (what the actor activated; empty for a value entry) and `akActor` (the perk's
//! owner).
//!
//! CK *Perk Fragments*, run time: "Activate entries: Run Immediately: Fragment is
//! run every frame conditions are met. Not Run Immediately: Fragment is run when
//! the player picks the appropriate option in the popup menu. Value entries: Run
//! whenever conditions are met." The fragment index is the entry's `EPFB` "Perk
//! Entry ID" (the `perk_fragment_census` example checks every Fallout 4 binding
//! names one). The engine has neither the activation popup nor a per-frame entry
//! evaluation that could run a fragment, so the host reports an entry run
//! ([`report_perk_entry`]) — the `perk.run` console command today — and
//! [`perk_fragment_system`] runs the entry's fragment.

use super::*;
use crate::translate::effects::VmStored;

/// A perk entry's `EPFB` id: the event a perk fragment binds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerkEntryId(pub u32);

/// Every perk's installed fragments, by PERK FormID.
pub type PerkFragments = FragmentTable<PerkEntryId>;

/// A perk entry run on an owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerkEntryRun {
    pub perk: u32,
    pub entry: u32,
    /// The perk's owner (a reference FormID).
    pub owner: u32,
    /// What the owner activated; `None` for a value entry.
    pub target: Option<u32>,
}

/// The entry runs the host reported this frame, drained by
/// [`perk_fragment_system`].
#[derive(Debug, Default)]
pub struct PendingPerkEntries(pub Vec<PerkEntryRun>);

impl Resource for PendingPerkEntries {}

/// Report that `entry` of `perk` ran for `owner`.
pub fn report_perk_entry(world: &World, run: PerkEntryRun) {
    if let Some(mut pending) = world.try_resource_mut::<PendingPerkEntries>() {
        pending.0.push(run);
    }
}

/// Run the fragments of this frame's perk entry runs.
pub fn perk_fragment_system(world: &World, _dt: f32) {
    let runs = match world.try_resource_mut::<PendingPerkEntries>() {
        Some(mut pending) if !pending.0.is_empty() => std::mem::take(&mut pending.0),
        _ => return,
    };
    let fragments = world.resource::<PerkFragments>().clone();
    let mut advances = Vec::new();
    for run in runs {
        let Some(set) = fragments.get(run.perk) else {
            continue;
        };
        for (_, script, function) in set.fragments.iter().filter(|(entry, _, _)| entry.0 == run.entry) {
            log::debug!("perk {:08X} entry {}: {script}.{function}", run.perk, run.entry);
            let effect = Effect::Interpret {
                script: script.clone(),
                function: function.clone(),
                args: vec![run.target.map_or(VmStored::None, VmStored::Form), VmStored::Form(run.owner)],
                resume: None,
            };
            advances.extend(run_papyrus_call(
                world,
                QuestFormId(run.perk),
                set.vmad.as_ref(),
                effect,
            ));
        }
    }
    flush_advances(world, advances);
}
