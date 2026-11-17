//! `ui.*` — the M48 menu registry from the console (GUI-OPS-001 / -005, #6653).
//!
//! `ui.menus` reports every open menu by its game-facing name, bottom to top, with its surface,
//! flags and the input owner, plus the movies' device count and the pause state. `ui.open` /
//! `ui.close` queue a request the frame applies before it republishes the registry, so the next
//! `ui.menus` shows the change. `hud.status` / `hud.debug` stay as they are.

use super::shared::*;
use crate::menu_stack::{MenuRequest, MenuResult};
use byroredux_core::ecs::resources::MenuRegistry;

/// `ui.menus` — the registry report.
pub(crate) struct UiMenusCommand;

impl ConsoleCommand for UiMenusCommand {
    fn name(&self) -> &str {
        "ui.menus"
    }
    fn description(&self) -> &str {
        "List the open menus (name, surface, flags), the input owner, the pause state and the movie device count"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        match world.try_resource::<MenuRegistry>() {
            Some(registry) => CommandOutput::lines(registry.report()),
            None => CommandOutput::error("ui.menus: no menu registry"),
        }
    }
}

/// `ui.open <menu>` — open a menu by its game-facing name on its Phase 0 surface.
pub(crate) struct UiOpenCommand;

impl ConsoleCommand for UiOpenCommand {
    fn name(&self) -> &str {
        "ui.open"
    }
    fn description(&self) -> &str {
        "Open a menu by its game-facing name (ui.open MessageBoxMenu); applied next frame"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let name = args.trim();
        if name.is_empty() {
            return CommandOutput::error("usage: ui.open <menu>");
        }
        let known = world.try_resource::<MenuRegistry>().is_some_and(|registry| registry.table().by_name(name).is_some());
        if !known {
            return CommandOutput::error(format!("ui.open: {name} is not a menu of this game"));
        }
        crate::menu_stack::request(world, MenuRequest::Open(name.to_owned()));
        CommandOutput::line(format!("ui.open: {name} queued"))
    }
}

/// `ui.close <menu> [accept|cancel]` — close an open menu with the stated result.
pub(crate) struct UiCloseCommand;

impl ConsoleCommand for UiCloseCommand {
    fn name(&self) -> &str {
        "ui.close"
    }
    fn description(&self) -> &str {
        "Close an open menu with a result (ui.close MessageBoxMenu cancel; default accept); applied next frame"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        // A game-facing name may hold spaces (`HUD Menu`): the result, if any, is the last word.
        let args = args.trim();
        let (name, result) = match args.rsplit_once(' ') {
            Some((name, "accept")) => (name.trim(), MenuResult::Accept),
            Some((name, "cancel")) => (name.trim(), MenuResult::Cancel),
            _ => (args, MenuResult::Accept),
        };
        if name.is_empty() {
            return CommandOutput::error("usage: ui.close <menu> [accept|cancel]");
        }
        let open = world.try_resource::<MenuRegistry>().is_some_and(|registry| registry.is_open(name));
        if !open {
            return CommandOutput::error(format!("ui.close: {name} is not open"));
        }
        crate::menu_stack::request(world, MenuRequest::Close(name.to_owned(), result));
        CommandOutput::line(format!("ui.close: {name} queued"))
    }
}
