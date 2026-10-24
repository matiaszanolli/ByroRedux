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

/// Runs one command: the world, whether the loaded game is Fallout 3 / New Vegas, the command id (a
/// handler shared by several ids tells them apart by it), its arguments and the call's context.
pub(crate) type Handler = fn(&World, bool, u16, &[ObScriptValue], &Context) -> Action;

/// The game family a call runs under. The opcode numbers of the Oblivion and Fallout 3 / New Vegas
/// tables overlap (`0x116b` is `PurgeCellBuffers` in one and `GetLinkedRef` in the other), so a row is
/// keyed by id *and* family. `Other` is a world with neither loaded (a test's bare world, a Papyrus game).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Game {
    Oblivion,
    Fallout,
    Other,
}

impl Game {
    /// The family of the game loaded in `world`.
    pub(crate) fn of(world: &World) -> Self {
        use byroredux_plugin::esm::reader::GameKind;
        match world.try_resource::<crate::ScriptGame>().map(|game| game.0) {
            Some(GameKind::Fallout3NV) => Self::Fallout,
            Some(GameKind::Oblivion) => Self::Oblivion,
            _ => Self::Other,
        }
    }

    /// `fallout` as `command_is_modeled` is asked it: only the two ObScript families exist there.
    pub(crate) fn from_fallout(fallout: bool) -> Self {
        if fallout { Self::Fallout } else { Self::Oblivion }
    }
}

/// The games a row runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Games {
    Oblivion,
    Fallout,
    /// Oblivion and Fallout 3 / New Vegas, but not a world with neither loaded.
    Both,
    /// Every game, a world with none loaded included.
    Any,
}

impl Games {
    fn includes(self, game: Game) -> bool {
        match self {
            Self::Oblivion => game == Game::Oblivion,
            Self::Fallout => game == Game::Fallout,
            Self::Both => game != Game::Other,
            Self::Any => true,
        }
    }
}

const ALL_GAMES: [Game; 3] = [Game::Oblivion, Game::Fallout, Game::Other];

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
const FAMILIES: &[&[Command]] = &[
    crate::obscript_effects::EFFECT_COMMANDS,
    crate::obscript_actions::ACTION_COMMANDS,
    crate::obscript_more::HEAD_COMMANDS,
    crate::obscript_more::ACTIONS_COMMANDS,
    crate::obscript_more::EXTRAS_COMMANDS,
    crate::obscript_more::STATE_COMMANDS,
    crate::obscript_more::TAIL_COMMANDS,
];

/// `(id, game)` pairs claimed by more than one row of `families`.
#[cfg_attr(not(test), allow(dead_code))] // read by the table's tests
pub(crate) fn duplicate_rows(families: &[&[Command]]) -> Vec<(u16, Game)> {
    let mut seen = std::collections::HashSet::new();
    let mut duplicates = Vec::new();
    for row in families.iter().flat_map(|rows| rows.iter()) {
        for game in ALL_GAMES {
            if row.games.includes(game) && !seen.insert((row.id, game)) {
                duplicates.push((row.id, game));
            }
        }
    }
    duplicates
}

type Table = HashMap<(u16, Game), &'static Command>;

fn index(families: &'static [&'static [Command]]) -> Table {
    let mut table = HashMap::new();
    for row in families.iter().flat_map(|rows| rows.iter()) {
        for game in ALL_GAMES {
            if row.games.includes(game) {
                table.entry((row.id, game)).or_insert(row);
            }
        }
    }
    table
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| index(FAMILIES))
}

/// The row for `cmd` under `game`, if the table owns it.
pub(crate) fn find(cmd: u16, game: Game) -> Option<&'static Command> {
    table().get(&(cmd, game)).copied()
}

/// Run `row` for a call whose arguments are `args`.
fn run_row(row: &Command, world: &World, game: Game, args: &[ObScriptValue], context: &Context) -> Action {
    if row.needs_caller && context.caller.filter(|&form| form != 0).is_none() {
        return Action::Declined;
    }
    (row.handler)(world, game == Game::Fallout, row.id, args, context)
}

/// Run `cmd` if the table owns it; `Unhandled` otherwise, so the host keeps looking.
pub(crate) fn run(world: &World, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let game = Game::of(world);
    match find(cmd, game) {
        Some(row) => run_row(row, world, game, args, context),
        None => Action::Unhandled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(_: &World, _: bool, _: u16, _: &[ObScriptValue], _: &Context) -> Action {
        Action::Done { value: None, effect: "ran".to_owned() }
    }

    /// Echoes what the handler is handed, so a test can see `fallout` and `cmd` arrive.
    fn echo(_: &World, fallout: bool, cmd: u16, _: &[ObScriptValue], _: &Context) -> Action {
        Action::Done { value: None, effect: format!("{cmd:#06x} fallout={fallout}") }
    }

    const fn row(id: u16, games: Games, needs_caller: bool) -> Command {
        Command { id, games, needs_caller, handler: done }
    }

    /// The shipped table claims no id twice in one game: a repeated row would be a
    /// shadowed handler.
    #[test]
    fn the_table_claims_no_id_twice_in_a_game() {
        assert_eq!(duplicate_rows(FAMILIES), Vec::new());
    }

    /// The same id may be two commands in two games (`0x116b`), and a `Both` row
    /// claims it in each, so it clashes with either.
    #[test]
    fn duplicates_are_per_game() {
        static OBLIVION_AND_FALLOUT: [Command; 2] = [row(0x116b, Games::Oblivion, false), row(0x116b, Games::Fallout, false)];
        assert!(duplicate_rows(&[&OBLIVION_AND_FALLOUT]).is_empty());
        static CLASH: [Command; 2] = [row(0x116b, Games::Both, false), row(0x116b, Games::Fallout, false)];
        assert_eq!(duplicate_rows(&[&CLASH]), vec![(0x116b, Game::Fallout)]);
        static ACROSS_FILES: [Command; 1] = [row(0x1000, Games::Both, false)];
        assert_eq!(
            duplicate_rows(&[&ACROSS_FILES, &ACROSS_FILES]),
            vec![(0x1000, Game::Oblivion), (0x1000, Game::Fallout)]
        );
    }

    #[test]
    fn a_row_answers_only_in_its_games() {
        static ROWS: [Command; 3] = [row(0x2001, Games::Fallout, false), row(0x2002, Games::Both, false), row(0x2003, Games::Any, false)];
        static FAMILIES_UNDER_TEST: [&[Command]; 1] = [&ROWS];
        let table = index(&FAMILIES_UNDER_TEST);
        assert!(table.contains_key(&(0x2001, Game::Fallout)) && !table.contains_key(&(0x2001, Game::Oblivion)));
        assert!(!table.contains_key(&(0x2001, Game::Other)));
        assert!(table.contains_key(&(0x2002, Game::Fallout)) && table.contains_key(&(0x2002, Game::Oblivion)));
        assert!(!table.contains_key(&(0x2002, Game::Other)), "`Both` is not a world with neither game loaded");
        assert!(table.contains_key(&(0x2003, Game::Other)));
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
        assert_eq!(run_row(&needs, &world, Game::Fallout, &[], &none), Action::Declined);
        assert_eq!(run_row(&needs, &world, Game::Fallout, &[], &zero), Action::Declined);
        assert!(matches!(run_row(&needs, &world, Game::Fallout, &[], &named), Action::Done { .. }));
        assert!(matches!(run_row(&free, &world, Game::Fallout, &[], &none), Action::Done { .. }));
    }

    /// The handler is handed the command id (a handler shared by several ids needs it) and whether
    /// the game is Fallout.
    #[test]
    fn a_handler_is_told_its_command_and_game() {
        let world = World::new();
        let shared = Command { id: 0x2005, games: Games::Both, needs_caller: false, handler: echo };
        let ctx = Context::default();
        assert_eq!(
            run_row(&shared, &world, Game::Fallout, &[], &ctx),
            Action::Done { value: None, effect: "0x2005 fallout=true".to_owned() }
        );
        assert_eq!(
            run_row(&shared, &world, Game::Oblivion, &[], &ctx),
            Action::Done { value: None, effect: "0x2005 fallout=false".to_owned() }
        );
    }
}
