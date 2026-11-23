//! `McmHost`: the static class the engine's `SKI_ConfigBase` calls to write the Mod Configuration Menu
//! host ([`crate::mcm`], LMC-UI-002). No mod script calls it; the engine's base script is its only user,
//! so every argument shape is the one that script passes.

use super::*;
use crate::mcm::{McmHost, McmKind, McmOption};

/// The `McmHost` natives, lowercase, with what each returns.
const MCM_NATIVES: &[(&str, Kind)] = &[
    ("register", Kind::None),
    ("addoption", Kind::Num),
    ("setcursor", Kind::None),
    ("setfillmode", Kind::None),
    ("setoptiontext", Kind::None),
    ("setoptionnumber", Kind::None),
    ("setoptionflags", Kind::None),
    ("stateoption", Kind::Num),
    ("optionstate", Kind::Str),
    ("setdialognumber", Kind::None),
    ("setmenudialogoptions", Kind::None),
    ("setinputdialogtext", Kind::None),
    ("setinfotext", Kind::None),
    ("settitletext", Kind::None),
    ("forcepagereset", Kind::None),
    ("loadcustomcontent", Kind::None),
    ("unloadcustomcontent", Kind::None),
    // Listed so the checker names it; it is declined below.
    ("showmessage", Kind::Bool),
];

impl Vm<'_> {
    /// An `McmHost.<name>(…)` call of the engine's `SKI_ConfigBase`.
    pub(in crate::fragment::vm) fn read_mcm_host(&mut self, frame: &mut Frame, name: &str, args: &[CallArg]) -> Run<Option<Value>> {
        let values = self.eval_args(frame, args)?;
        let int = |i: usize| values.get(i).and_then(Value::number).unwrap_or(0.0) as i32;
        let num = |i: usize| values.get(i).and_then(Value::number).unwrap_or(0.0);
        let text = |i: usize| match values.get(i) {
            Some(Value::Str(text)) => text.clone(),
            _ => String::new(),
        };
        let strings = |i: usize| match values.get(i) {
            Some(Value::Array(items)) => items.borrow().iter().map(|item| match item {
                Value::Str(text) => text.clone(),
                other => other.text(),
            }).collect(),
            _ => Vec::new(),
        };
        // The config script registering is the quest's own script that extends the base.
        let quest = self.context;
        let config_script = || {
            self.world.try_resource::<crate::QuestOwnScripts>().and_then(|scripts| {
                scripts
                    .running_scripts()
                    .find(|(owner, _)| *owner == quest)
                    .and_then(|(_, scripts)| scripts.iter().find(|s| s.extends(crate::mcm::CONFIG_BASE)).map(|s| s.script.clone()))
            })
        };
        let script = if name == "register" { config_script() } else { None };
        // SkyUI's `ShowMessage`: a box with the message and Accept (and Cancel) whose answer the
        // calling statement waits for, as at a `Message.Show()`; accepted is button 0.
        if name == "showmessage" {
            if let Some(button) = self.answer.take() {
                return Ok(Some(Value::Bool(button == 0)));
            }
            let mut buttons = vec![text(2)];
            if values.get(1).is_some_and(Value::truthy) {
                buttons.push(text(3));
            }
            let Some(ticket) = crate::messages::reserve_papyrus_ticket(self.world) else {
                return stop("ShowMessage could not raise its box");
            };
            self.deferred.defer_awaiting_text_box(text(0), buttons, ticket);
            self.deferred.awaits_message = Some(ticket);
            return stop(super::super::interp::AWAITS_MESSAGE);
        }
        let Some(mut host) = self.world.try_resource_mut::<McmHost>() else {
            return stop("McmHost without the MCM host resource");
        };
        Ok(Some(match name {
            "register" => {
                let Some(script) = script else {
                    return stop("McmHost.Register from a quest with no config script");
                };
                host.register(quest, &script, text(0), strings(1));
                Value::None
            }
            "addoption" => {
                let Some(kind) = McmKind::from_code(int(0)) else {
                    return stop("McmHost.AddOption of an unknown option kind");
                };
                Value::Int(host.add_option(McmOption {
                    kind,
                    label: text(1),
                    text: text(2),
                    number: num(3),
                    flags: int(4) as u32,
                    state: text(5),
                }))
            }
            "setcursor" => {
                host.set_cursor(int(0));
                Value::None
            }
            "setfillmode" => {
                host.set_fill_mode(int(0));
                Value::None
            }
            "setoptiontext" => {
                host.set_option_text(int(0), text(1));
                Value::None
            }
            "setoptionnumber" => {
                host.set_option_number(int(0), num(1), text(2));
                Value::None
            }
            "setoptionflags" => {
                host.set_option_flags(int(0), int(1));
                Value::None
            }
            "stateoption" => Value::Int(host.state_option(&text(0))),
            "optionstate" => Value::Str(host.option_state(int(0))),
            "setdialognumber" => {
                host.set_dialog_number(int(0), num(1));
                Value::None
            }
            "setmenudialogoptions" => {
                host.set_menu_dialog_options(strings(0));
                Value::None
            }
            "setinputdialogtext" => {
                host.set_input_dialog_text(text(0));
                Value::None
            }
            "setinfotext" => {
                host.set_info_text(text(0));
                Value::None
            }
            "settitletext" => {
                host.set_title_text(text(0));
                Value::None
            }
            "forcepagereset" => {
                host.force_page_reset();
                Value::None
            }
            "loadcustomcontent" => {
                host.load_custom_content(text(0), num(1), num(2));
                Value::None
            }
            "unloadcustomcontent" => {
                host.unload_custom_content();
                Value::None
            }
            _ => return stop(format!("McmHost.{name}")),
        }))
    }
}

impl Checker<'_> {
    /// Whether the VM runs `McmHost.<name>`. `ShowMessage` is a box the calling statement waits on (a
    /// `Message.Show()` for its value): it waits, and its statement holds at most one.
    pub(in crate::fragment::vm) fn check_mcm_host(&mut self, name: &str, statement: bool) -> Option<Kind> {
        if name == "showmessage" {
            if statement {
                return self.fail(|| "McmHost.ShowMessage as a statement".into());
            }
            self.waits = true;
            self.value_shows += 1;
            if self.value_shows > 1 {
                return self.fail(|| "two Message.Show() answers in one statement".into());
            }
            return Some(Kind::Bool);
        }
        match MCM_NATIVES.iter().find(|(native, _)| *native == name) {
            Some((_, kind)) => Some(*kind),
            None => self.fail(|| format!("McmHost.{name}")),
        }
    }
}
