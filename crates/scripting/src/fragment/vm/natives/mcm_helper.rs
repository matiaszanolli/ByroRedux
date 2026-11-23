//! `McmHelper`: the static class the engine's `MCM_ConfigBase` and `MCM` call to serve MCM Helper
//! configs ([`crate::mcm::helper`], LMC-UI-002 slice 4). No mod script calls it; the engine's two
//! scripts are its only users, so every argument shape is the one they pass. The config quest is the
//! running quest (`Self` of the config script).

use super::*;
use crate::mcm::helper::{self, Accepted, McmHelperStore};

/// The `McmHelper` natives, lowercase, with what each returns.
const NATIVES: &[(&str, Kind)] = &[
    ("versioncode", Kind::Num),
    ("configname", Kind::Str),
    ("configpages", Kind::Array),
    ("pagereset", Kind::None),
    ("highlight", Kind::None),
    ("select", Kind::Str),
    ("default", Kind::Str),
    ("opendialog", Kind::None),
    ("acceptnumber", Kind::Str),
    ("acceptindex", Kind::Str),
    ("accepttext", Kind::Str),
    ("refreshmenu", Kind::None),
    ("setmenuoptions", Kind::None),
    ("getmodsettingint", Kind::Num),
    ("getmodsettingbool", Kind::Bool),
    ("getmodsettingfloat", Kind::Num),
    ("getmodsettingstring", Kind::Str),
    ("setmodsettingint", Kind::None),
    ("setmodsettingbool", Kind::None),
    ("setmodsettingfloat", Kind::None),
    ("setmodsettingstring", Kind::None),
];

impl Vm<'_> {
    /// An `McmHelper.<name>(…)` call of the engine's `MCM_ConfigBase` or `MCM`.
    pub(in crate::fragment::vm) fn read_mcm_helper(&mut self, frame: &mut Frame, name: &str, args: &[CallArg]) -> Run<Option<Value>> {
        let values = self.eval_args(frame, args)?;
        let int = |i: usize| values.get(i).and_then(Value::number).unwrap_or(0.0) as i32;
        let num = |i: usize| values.get(i).and_then(Value::number).unwrap_or(0.0);
        let text = |i: usize| match values.get(i) {
            Some(Value::Str(text)) => text.clone(),
            _ => String::new(),
        };
        let strings = |i: usize| match values.get(i) {
            Some(Value::Array(items)) => items.borrow().iter().map(Value::text).collect(),
            _ => Vec::new(),
        };
        let quest = self.context;
        let world = self.world;
        if world.try_resource::<McmHelperStore>().is_none() {
            return stop("McmHelper without the MCM Helper store");
        }
        // A setting call names its mod, or "" for the config quest's own.
        let mod_name = |given: String| -> Option<String> {
            if given.is_empty() { world.resource::<McmHelperStore>().mod_of(world, quest) } else { Some(given) }
        };
        Ok(Some(match name {
            "versioncode" => Value::Int(helper::VERSION_CODE as i32),
            "configname" => {
                let name = world.resource_mut::<McmHelperStore>().registration(world, quest).map(|(name, _)| name).unwrap_or_default();
                // The config is read: its mod's hotkeys become live (slice 4b).
                helper::note_config_quest(world, quest);
                Value::Str(name)
            }
            "configpages" => {
                let pages = world.resource_mut::<McmHelperStore>().registration(world, quest).map(|(_, pages)| pages).unwrap_or_default();
                Value::Array(std::rc::Rc::new(std::cell::RefCell::new(pages.into_iter().map(Value::Str).collect())))
            }
            "pagereset" => {
                if let Err(error) = helper::page_reset(world, quest, &text(0)) {
                    return stop(format!("McmHelper.PageReset: {error}"));
                }
                Value::None
            }
            "highlight" => {
                helper::highlight(world, quest, int(0));
                Value::None
            }
            "select" => Value::Str(helper::select(world, quest, int(0))),
            "default" => Value::Str(helper::reset(world, quest, int(0))),
            "opendialog" => {
                helper::open_dialog(world, quest, int(0));
                Value::None
            }
            "acceptnumber" => Value::Str(helper::accept(world, quest, int(0), Accepted::Number(num(1)))),
            "acceptindex" => Value::Str(helper::accept(world, quest, int(0), Accepted::Index(int(1)))),
            "accepttext" => Value::Str(helper::accept(world, quest, int(0), Accepted::Text(text(1)))),
            "refreshmenu" => {
                helper::refresh(world, quest);
                Value::None
            }
            "setmenuoptions" => {
                world.resource_mut::<McmHelperStore>().set_menu_options(quest, &text(0), strings(1), strings(2));
                helper::refresh(world, quest);
                Value::None
            }
            "getmodsettingint" => Value::Int(match mod_name(text(0)) {
                Some(mod_name) => world.resource_mut::<McmHelperStore>().setting_int(&mod_name, &text(1)),
                None => -1,
            }),
            "getmodsettingfloat" => Value::Float(match mod_name(text(0)) {
                Some(mod_name) => world.resource_mut::<McmHelperStore>().setting_float(&mod_name, &text(1)),
                None => -1.0,
            }),
            "getmodsettingbool" => Value::Bool(
                mod_name(text(0)).is_some_and(|mod_name| world.resource_mut::<McmHelperStore>().setting_bool(&mod_name, &text(1))),
            ),
            "getmodsettingstring" => Value::Str(
                mod_name(text(0)).map(|mod_name| world.resource_mut::<McmHelperStore>().setting_text(&mod_name, &text(1))).unwrap_or_default(),
            ),
            "setmodsettingint" | "setmodsettingbool" | "setmodsettingfloat" => {
                if let Some(mod_name) = mod_name(text(0)) {
                    let value = match name {
                        "setmodsettingint" => int(2) as f32,
                        "setmodsettingbool" => f32::from(u8::from(values.get(2).is_some_and(Value::truthy))),
                        _ => num(2),
                    };
                    world.resource_mut::<McmHelperStore>().set_setting_number(&mod_name, &text(1), value);
                }
                Value::None
            }
            "setmodsettingstring" => {
                if let Some(mod_name) = mod_name(text(0)) {
                    world.resource_mut::<McmHelperStore>().set_setting_text(&mod_name, &text(1), text(2));
                }
                Value::None
            }
            _ => return stop(format!("McmHelper.{name}")),
        }))
    }
}

impl Checker<'_> {
    /// Whether the VM runs `McmHelper.<name>`.
    pub(in crate::fragment::vm) fn check_mcm_helper(&mut self, name: &str) -> Option<Kind> {
        match NATIVES.iter().find(|(native, _)| *native == name) {
            Some((_, kind)) => Some(*kind),
            None => self.fail(|| format!("McmHelper.{name}")),
        }
    }
}
