//! The ObScript command table (#6139).
//!
//! The M47.3 command surface grew as a chain of `match cmd` functions
//! (`obscript_effects::run`, `obscript_more` and its four links,
//! `obscript_actions::run`), each ending `_ => next::run(..)`. Which function owns
//! a command is only discoverable by reading the chain, and an arm added early
//! silently shadows a later one. This table replaces the chain one link at a
//! time: a command's row names its id, the games it exists in, whether it acts on
//! a calling reference, and the handler that runs it. The host asks the table
//! before it asks any chain link, so a command moves by deleting its arm and
//! adding its row, with the rest of the chain untouched.
//!
//! Rows live in per-family files, each exporting a `COMMANDS` slice that is added
//! to [`FAMILIES`]; [`duplicate_rows`] (and the test over it) makes a repeated id
//! in one game a build-time failure instead of a shadowed arm.

use std::collections::HashMap;
use std::sync::OnceLock;

use byroredux_core::ecs::World;

use crate::obscript_actions::{Action, Context};
use crate::obscript_vm::ObScriptValue;

/// Runs one command.
pub(crate) type Handler = fn(&World, &[ObScriptValue], &Context) -> Action;

/// The game families a command exists in. The opcode numbers of the Oblivion and
/// Fallout 3 / New Vegas tables overlap (`0x116b` is `PurgeCellBuffers` in one and
/// `GetLinkedRef` in the other), so a row is keyed by id *and* family.
// Unconstructed until the first family file adds rows (#6139 stage 1); drop the allow then.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Games {
    Oblivion,
    Fallout,
    Both,
}

impl Games {
    fn includes(self, fallout: bool) -> bool {
        match self {
            Self::Oblivion => !fallout,
            Self::Fallout => fallout,
            Self::Both => true,
        }
    }
}

/// One row of the table.
#[derive(Clone, Copy)]
pub(crate) struct Command {
    pub id: u16,
    pub games: Games,
    /// The command acts on a calling reference (an explicit or defaulted `ref.Cmd`):
    /// with none, it is declined before its handler runs. Faction, achievement and
    /// message commands name no reference and set this `false`.
    pub needs_caller: bool,
    pub handler: Handler,
}

/// Every family's rows. A family file adds its `COMMANDS` slice here when its
/// commands move out of a chain link.
const FAMILIES: &[&[Command]] = &[];

/// `(id, family)` pairs claimed by more than one row of `families`.
#[cfg_attr(not(test), allow(dead_code))] // read by the table's tests; see `Games`
pub(crate) fn duplicate_rows(families: &[&[Command]]) -> Vec<(u16, bool)> {
    let mut seen = std::collections::HashSet::new();
    let mut duplicates = Vec::new();
    for row in families.iter().flat_map(|rows| rows.iter()) {
        for fallout in [false, true] {
            if row.games.includes(fallout) && !seen.insert((row.id, fallout)) {
                duplicates.push((row.id, fallout));
            }
        }
    }
    duplicates
}

fn index(families: &'static [&'static [Command]]) -> HashMap<(u16, bool), &'static Command> {
    let mut table = HashMap::new();
    for row in families.iter().flat_map(|rows| rows.iter()) {
        for fallout in [false, true] {
            if row.games.includes(fallout) {
                table.entry((row.id, fallout)).or_insert(row);
            }
        }
    }
    table
}

fn table() -> &'static HashMap<(u16, bool), &'static Command> {
    static TABLE: OnceLock<HashMap<(u16, bool), &'static Command>> = OnceLock::new();
    TABLE.get_or_init(|| index(FAMILIES))
}

/// The row for `cmd` in the Fallout (`fallout`) or Oblivion family, if the table owns it.
pub(crate) fn find(cmd: u16, fallout: bool) -> Option<&'static Command> {
    table().get(&(cmd, fallout)).copied()
}

/// Run `row` for a call whose arguments are `args`.
fn run_row(row: &Command, world: &World, args: &[ObScriptValue], context: &Context) -> Action {
    if row.needs_caller && context.caller.filter(|&form| form != 0).is_none() {
        return Action::Declined;
    }
    (row.handler)(world, args, context)
}

/// Run `cmd` if the table owns it; `Unhandled` otherwise, so the host keeps looking.
pub(crate) fn run(world: &World, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let fallout = crate::ScriptGame::is_fallout3_nv(world);
    match find(cmd, fallout) {
        Some(row) => run_row(row, world, args, context),
        None => Action::Unhandled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(_: &World, _: &[ObScriptValue], _: &Context) -> Action {
        Action::Done { value: None, effect: "ran".to_owned() }
    }

    const fn row(id: u16, games: Games, needs_caller: bool) -> Command {
        Command { id, games, needs_caller, handler: done }
    }

    /// The shipped table claims no id twice in one game: a repeated row would be a
    /// shadowed handler.
    #[test]
    fn the_table_claims_no_id_twice_in_a_family() {
        assert_eq!(duplicate_rows(FAMILIES), Vec::new());
    }

    /// The same id may be two commands in two games (`0x116b`), and a `Both` row
    /// claims it in each, so it clashes with either.
    #[test]
    fn duplicates_are_per_family() {
        static OBLIVION_AND_FALLOUT: [Command; 2] = [row(0x116b, Games::Oblivion, false), row(0x116b, Games::Fallout, false)];
        assert!(duplicate_rows(&[&OBLIVION_AND_FALLOUT]).is_empty());
        static CLASH: [Command; 2] = [row(0x116b, Games::Both, false), row(0x116b, Games::Fallout, false)];
        assert_eq!(duplicate_rows(&[&CLASH]), vec![(0x116b, true)]);
        static ACROSS_FILES: [Command; 1] = [row(0x1000, Games::Both, false)];
        assert_eq!(duplicate_rows(&[&ACROSS_FILES, &ACROSS_FILES]), vec![(0x1000, false), (0x1000, true)]);
    }

    #[test]
    fn a_row_answers_only_in_its_games() {
        static ROWS: [Command; 2] = [row(0x2001, Games::Fallout, false), row(0x2002, Games::Both, false)];
        static FAMILIES_UNDER_TEST: [&[Command]; 1] = [&ROWS];
        let table = index(&FAMILIES_UNDER_TEST);
        assert!(table.contains_key(&(0x2001, true)) && !table.contains_key(&(0x2001, false)));
        assert!(table.contains_key(&(0x2002, true)) && table.contains_key(&(0x2002, false)));
    }

    /// A row that acts on a calling reference is declined without one, before its
    /// handler runs; a row that names no reference runs regardless.
    #[test]
    fn a_caller_bound_row_is_declined_without_a_caller() {
        let world = World::new();
        let needs = row(0x2003, Games::Both, true);
        let free = row(0x2004, Games::Both, false);
        let none = Context::default();
        let zero = Context { caller: Some(0), ..Context::default() };
        let named = Context { caller: Some(0x14), ..Context::default() };
        assert_eq!(run_row(&needs, &world, &[], &none), Action::Declined);
        assert_eq!(run_row(&needs, &world, &[], &zero), Action::Declined);
        assert!(matches!(run_row(&needs, &world, &[], &named), Action::Done { .. }));
        assert!(matches!(run_row(&free, &world, &[], &none), Action::Done { .. }));
    }

    /// With no rows yet, the table owns nothing and the host's chain answers every command.
    #[test]
    fn an_unowned_command_is_unhandled_so_the_chain_keeps_looking() {
        let world = World::new();
        assert!(FAMILIES.is_empty());
        assert_eq!(run(&world, 0x1002, &[], &Context::default()), Action::Unhandled);
    }
}
