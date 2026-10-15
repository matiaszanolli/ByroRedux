//! M47.2 — terminal (`TERM`) fragments: the `Fragment_Terminal_NN` functions a
//! Fallout 4 / Starfield terminal binds to its menu items, run on the Papyrus VM
//! with `akTerminalRef` bound to the terminal reference.
//!
//! The CK's terminal fragments run when the player picks the menu item whose
//! `ITID` is `NN` (the census proves every Fallout 4 binding's index is one of
//! its terminal's menu item ids, and the function suffix repeats it). The engine
//! has no terminal screen yet, so the host reports a selection
//! ([`report_terminal_selection`]) from whatever drives the menu — the
//! `terminal.select` console command today — and [`terminal_fragment_system`]
//! runs the item's fragment.

use super::*;
use crate::translate::effects::VmStored;

/// A terminal menu item (`ITID`): the event a terminal fragment binds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalItem(pub u32);

/// Every terminal's installed fragments, by terminal base FormID.
pub type TerminalFragments = FragmentTable<TerminalItem>;

/// A menu item picked on a terminal reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSelection {
    /// The terminal reference (`akTerminalRef`).
    pub terminal: u32,
    /// The terminal's base record, whose `VMAD` binds the fragments.
    pub base: u32,
    /// The picked menu item's `ITID`.
    pub item: u32,
}

/// The selections the host reported this frame, drained by
/// [`terminal_fragment_system`].
#[derive(Debug, Default)]
pub struct PendingTerminalSelections(pub Vec<TerminalSelection>);

impl Resource for PendingTerminalSelections {}

/// Report that menu item `item` was picked on the terminal reference `terminal`
/// (base record `base`).
pub fn report_terminal_selection(world: &World, terminal: u32, base: u32, item: u32) {
    if let Some(mut pending) = world.try_resource_mut::<PendingTerminalSelections>() {
        pending.0.push(TerminalSelection { terminal, base, item });
    }
}

/// Run the fragments of this frame's terminal selections.
pub fn terminal_fragment_system(world: &World, _dt: f32) {
    let selections = match world.try_resource_mut::<PendingTerminalSelections>() {
        Some(mut pending) if !pending.0.is_empty() => std::mem::take(&mut pending.0),
        _ => return,
    };
    let fragments = world.resource::<TerminalFragments>().clone();
    let mut advances = Vec::new();
    for selection in selections {
        let Some(set) = fragments.get(selection.base) else {
            log::debug!("terminal {:08X}: no fragments", selection.base);
            continue;
        };
        for (_, script, function) in set.fragments.iter().filter(|(item, _, _)| item.0 == selection.item) {
            log::debug!("terminal {:08X} item {}: {script}.{function}", selection.terminal, selection.item);
            let effect = Effect::Interpret {
                script: script.clone(),
                function: function.clone(),
                args: vec![VmStored::Form(selection.terminal)],
                resume: None,
            };
            advances.extend(run_papyrus_call(
                world,
                QuestFormId(selection.base),
                set.vmad.as_ref(),
                effect,
            ));
        }
    }
    flush_advances(world, advances);
}
