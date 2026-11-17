//! `ui.*` — the M48 menu registry from the console (GUI-OPS-001 / -005, #6653).
//!
//! `ui.menus` reports every open menu by its game-facing name, bottom to top, with its surface,
//! flags and the input owner, plus the movies' device count and the pause state. `ui.open` /
//! `ui.close` queue a request the frame applies before it republishes the registry, so the next
//! `ui.menus` shows the change. `ui.host` reports an open movie's host calls by behaviour class
//! (GUI-OPS-002, #6665). `hud.status` / `hud.debug` stay as they are.

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

/// `ui.host [menu]` — an open movie's host calls since it opened, by behaviour class (PRD §3.2),
/// and the distinct H3 / H4 method names (GUI-OPS-002).
pub(crate) struct UiHostCommand;

impl ConsoleCommand for UiHostCommand {
    fn name(&self) -> &str {
        "ui.host"
    }
    fn description(&self) -> &str {
        "Report an open movie's host calls by behaviour class (H1 served … H4 unknown) and its unserved / unknown methods (ui.host [menu]; all open movies without one)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(reports) = world.try_resource::<crate::menu_stack::ScaleformHostReports>() else {
            return CommandOutput::error("ui.host: no host-call reports");
        };
        let wanted = args.trim();
        let mut lines = Vec::new();
        for (menu, report) in reports.0.iter().filter(|(menu, _)| wanted.is_empty() || menu.eq_ignore_ascii_case(wanted)) {
            lines.extend(host_report_lines(menu, report));
        }
        if lines.is_empty() {
            return if wanted.is_empty() {
                CommandOutput::line("ui.host: no Scaleform movie is open")
            } else {
                CommandOutput::error(format!("ui.host: no open movie named {wanted}"))
            };
        }
        CommandOutput::lines(lines)
    }
}

/// The `ui.host` lines for one movie.
pub(crate) fn host_report_lines(menu: &str, report: &byroredux_ui::HostCallReport) -> Vec<String> {
    use byroredux_ui::HostClass;
    let counts: Vec<String> = HostClass::ALL
        .iter()
        .map(|class| format!("{} {} {}", class.code(), class.label(), report.count(*class)))
        .collect();
    let mut lines = vec![format!("ui.host: {menu}: {}", counts.join(" · "))];
    let names = |set: &std::collections::BTreeSet<String>| set.iter().map(String::as_str).collect::<Vec<_>>().join(", ");
    if !report.unserved.is_empty() {
        lines.push(format!("  unserved (H3): {}", names(&report.unserved)));
    }
    if !report.unknown.is_empty() {
        lines.push(format!("  unknown (H4): {}", names(&report.unknown)));
    }
    if report.capped {
        lines.push(format!("  (name list capped at {} per class)", byroredux_ui::MAX_DISTINCT_HOST_METHOD_NAMES));
    }
    lines
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

#[cfg(test)]
mod host_report_tests {
    use super::*;
    use byroredux_ui::{HostCallReport, HostClass, ScaleformHostCall, ScaleformHostDispatch, ScaleformProfile};

    fn call(method: &str, class: HostClass) -> ScaleformHostCall {
        ScaleformHostCall {
            sequence: 0,
            profile: ScaleformProfile::SkyrimAvm1,
            transport_method: method.to_owned(),
            method: method.to_owned(),
            host_object: None,
            request_id: None,
            arguments: Vec::new(),
            dispatch: ScaleformHostDispatch::Queued,
            class,
        }
    }

    /// GUI-OPS-002 — `ui.host` reports a movie's calls per class and its distinct H3 / H4
    /// names; each H3 / H4 name is "new" once (the drain's one-diagnostic moment).
    #[test]
    fn ui_host_reports_counts_per_class_and_the_unserved_names() {
        let mut report = HostCallReport::default();
        assert!(report.record(&call("EquipItem", HostClass::Unserved)));
        assert!(!report.record(&call("EquipItem", HostClass::Unserved)), "reported once");
        assert!(report.record(&call("Mystery", HostClass::Unknown)));
        assert!(!report.record(&call("ShouldShowMod", HostClass::Placeholder)));
        let mut world = World::new();
        world.insert_resource(crate::menu_stack::ScaleformHostReports([("InventoryMenu".to_owned(), report)].into_iter().collect()));
        let text = UiHostCommand.execute(&world, "inventorymenu").lines.join("\n");
        assert!(text.contains("InventoryMenu: H1 served 0 · H2 placeholder 1 · H3 unserved 2 · H4 unknown 1"), "{text}");
        assert!(text.contains("unserved (H3): EquipItem") && text.contains("unknown (H4): Mystery"), "{text}");
        assert!(UiHostCommand.execute(&world, "MapMenu").lines.join("").contains("no open movie named MapMenu"));
    }
}
