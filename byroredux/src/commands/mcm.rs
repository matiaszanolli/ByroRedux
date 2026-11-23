//! LMC-UI-002 — `mcm.*`: the Mod Configuration Menu driven from the console, the same actions the
//! native page takes (`docs/engine/mcm-host.md` §3.3). Gates and corpus scenarios use these.

use super::shared::*;

use byroredux_scripting::mcm::{self, McmAnswer, McmDialog, McmHost};

/// One `mcm.*` command: its name, usage and action.
pub(crate) struct McmCommand {
    name: &'static str,
    description: &'static str,
    run: fn(&World, &str) -> Result<Vec<String>, String>,
}

impl ConsoleCommand for McmCommand {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        match (self.run)(world, args.trim()) {
            Ok(lines) => CommandOutput::lines(lines),
            Err(error) => CommandOutput::error(format!("{}: {error}", self.name)),
        }
    }
}

/// The `mcm.*` commands.
pub(crate) const MCM_COMMANDS: [McmCommand; 10] = [
    McmCommand { name: "mcm.list", description: "List the registered Mod Configuration Menus", run: list },
    McmCommand { name: "mcm.open", description: "Open a config menu: mcm.open <mod index> [page index]", run: open },
    McmCommand { name: "mcm.page", description: "Show a page of the open config menu: mcm.page <page index | front>", run: page },
    McmCommand { name: "mcm.show", description: "Print the open config page: its cells, ids, values and dialog", run: show },
    McmCommand { name: "mcm.select", description: "Use an option (select, or open its dialog): mcm.select <id>", run: select },
    McmCommand {
        name: "mcm.accept",
        description: "Answer the open dialog: mcm.accept <id> <slider value | menu index | colour RRGGBB | key code | text>",
        run: accept,
    },
    McmCommand { name: "mcm.cancel", description: "Close the open dialog unanswered", run: cancel },
    McmCommand { name: "mcm.default", description: "Reset an option to its default (OnOptionDefault): mcm.default <id>", run: reset },
    McmCommand { name: "mcm.highlight", description: "Rest the cursor on an option (OnOptionHighlight): mcm.highlight <id>", run: highlight },
    McmCommand { name: "mcm.close", description: "Close the open config menu", run: close },
];

fn option_id(args: &str) -> Result<i32, String> {
    let raw_option = args.split_whitespace().next().ok_or("an option id is required")?;
    parse_console_u32(raw_option).map(|id| id as i32).ok_or_else(|| format!("bad option id `{raw_option}`"))
}

fn list(world: &World, _: &str) -> Result<Vec<String>, String> {
    let host = world.try_resource::<McmHost>().ok_or("no MCM host")?;
    let mut lines = vec![format!("{} config menu(s)", host.mods.len())];
    for (index, entry) in host.mods.iter().enumerate() {
        lines.push(format!(
            "  {index}: {} ({} on {:08X}), pages: {}",
            entry.name,
            entry.script,
            entry.quest.0,
            if entry.pages.is_empty() { "none".to_owned() } else { entry.pages.join(" | ") }
        ));
    }
    Ok(lines)
}

fn open(world: &World, args: &str) -> Result<Vec<String>, String> {
    let mut tokens = args.split_whitespace();
    let index = tokens.next().and_then(|t| t.parse::<usize>().ok()).ok_or("usage: mcm.open <mod index> [page index]")?;
    mcm::open(world, index)?;
    if let Some(page) = tokens.next() {
        mcm::select_page(world, Some(page.parse::<usize>().map_err(|_| format!("bad page `{page}`"))?))?;
    }
    show(world, "")
}

fn page(world: &World, args: &str) -> Result<Vec<String>, String> {
    let page = match args {
        "front" | "" => None,
        index => Some(index.parse::<usize>().map_err(|_| format!("bad page `{index}`"))?),
    };
    mcm::select_page(world, page)?;
    show(world, "")
}

fn show(world: &World, _: &str) -> Result<Vec<String>, String> {
    let host = world.try_resource::<McmHost>().ok_or("no MCM host")?;
    let page = host.open.as_ref().ok_or("no config menu is open")?;
    let entry = &host.mods[page.mod_index];
    let page_name = page.page.and_then(|i| entry.pages.get(i).cloned()).unwrap_or_else(|| "front".to_owned());
    let mut lines = vec![format!("{} / {page_name}: \"{}\"", entry.name, page.title)];
    for (index, cell) in page.cells.iter().enumerate() {
        let Some(option) = cell else { continue };
        lines.push(format!(
            "  [{index:3}] id={:#x} {:?} \"{}\" = \"{}\"{}{}",
            page.option_id(index),
            option.kind,
            option.label,
            option.value_text(),
            if option.flags == 0 { String::new() } else { format!(" flags={}", option.flags) },
            if option.state.is_empty() { String::new() } else { format!(" state={}", option.state) },
        ));
    }
    if !page.info.is_empty() {
        lines.push(format!("  info: \"{}\"", page.info));
    }
    if let Some((source, x, y)) = &page.custom {
        lines.push(format!("  custom content: {source} at ({x}, {y})"));
    }
    if let Some((id, dialog)) = &page.dialog {
        lines.push(format!("  dialog for {id:#x}: {}", describe(dialog)));
    }
    Ok(lines)
}

fn describe(dialog: &McmDialog) -> String {
    match dialog {
        McmDialog::Slider { start, default, min, max, interval } => {
            format!("slider start={start} default={default} range={min}..{max} interval={interval}")
        }
        McmDialog::Menu { options, start, default } => format!("menu start={start} default={default} options={options:?}"),
        McmDialog::Color { start, default } => format!("colour start=#{start:06X} default=#{default:06X}"),
        McmDialog::Input { start_text } => format!("input \"{start_text}\""),
        McmDialog::KeyMap => "key map".to_owned(),
    }
}

fn select(world: &World, args: &str) -> Result<Vec<String>, String> {
    mcm::activate(world, option_id(args)?)?;
    show(world, "")
}

fn accept(world: &World, args: &str) -> Result<Vec<String>, String> {
    let id = option_id(args)?;
    let value = args.split_once(char::is_whitespace).map(|(_, rest)| rest.trim()).ok_or("a value is required")?;
    let dialog = {
        let host = world.try_resource::<McmHost>().ok_or("no MCM host")?;
        host.open.as_ref().and_then(|page| page.dialog.clone()).filter(|(open, _)| *open == id).map(|(_, dialog)| dialog)
    };
    let answer = match dialog.ok_or_else(|| format!("option {id:#x} has no open dialog (mcm.select it first)"))? {
        McmDialog::Slider { .. } => McmAnswer::Slider(value.parse().map_err(|_| format!("bad slider value `{value}`"))?),
        McmDialog::Menu { .. } => McmAnswer::Menu(value.parse().map_err(|_| format!("bad menu index `{value}`"))?),
        McmDialog::Color { .. } => McmAnswer::Color(
            u32::from_str_radix(value.trim_start_matches('#'), 16).map_err(|_| format!("bad colour `{value}`"))?,
        ),
        McmDialog::KeyMap => McmAnswer::KeyMap(value.parse().map_err(|_| format!("bad key code `{value}`"))?, String::new()),
        McmDialog::Input { .. } => McmAnswer::Input(value.to_owned()),
    };
    mcm::accept(world, id, answer)?;
    show(world, "")
}

fn cancel(world: &World, _: &str) -> Result<Vec<String>, String> {
    mcm::cancel_dialog(world);
    show(world, "")
}

fn reset(world: &World, args: &str) -> Result<Vec<String>, String> {
    mcm::reset_option(world, option_id(args)?)?;
    show(world, "")
}

fn highlight(world: &World, args: &str) -> Result<Vec<String>, String> {
    mcm::highlight(world, option_id(args)?)?;
    show(world, "")
}

fn close(world: &World, _: &str) -> Result<Vec<String>, String> {
    mcm::close(world)?;
    Ok(vec!["closed".to_owned()])
}
