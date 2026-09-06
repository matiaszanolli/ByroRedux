//! Layer 4 — runtime adapters for the input and UI surfaces.

use super::*;

/// Engine routes backing the read-only subset of SKSE's `Input` provider.
pub const PAPYRUS_INPUT_GET_MAPPED_KEY_ROUTE: &str = "byro.input.compat.get-mapped-key";

pub const PAPYRUS_INPUT_GET_MAPPED_CONTROL_ROUTE: &str = "byro.input.compat.get-mapped-control";

/// Engine route backing the read-only subset of SKSE's `UI` provider.
pub const PAPYRUS_UI_IS_MENU_OPEN_ROUTE: &str = "byro.ui.compat.is-menu-open";

fn papyrus_input_declaration(
    route: &'static str,
    id: &str,
    function: &str,
    parameters: &[(&str, ScriptValueType, bool)],
    result: ScriptValueType,
    description: &str,
) -> EnginePapyrusFunctionDeclaration {
    EnginePapyrusFunctionDeclaration {
        route: route.to_owned(),
        declaration: ScriptFunctionDeclaration {
            id: ScriptFunctionId::new(id).expect("built-in Input function ID is valid"),
            component: ComponentId::new("input").expect("built-in Input component ID is valid"),
            parameters: parameters
                .iter()
                .cloned()
                .map(|(id, value_type, optional)| ScriptParameterDeclaration {
                    id: ScriptParameterId::new(id).expect("built-in Input parameter ID is valid"),
                    value_type,
                    optional,
                })
                .collect(),
            result: Some(ScriptResultDeclaration {
                value_type: result,
                optional: false,
            }),
            papyrus: Some(PapyrusFunctionAlias {
                provider: "Input".to_owned(),
                function: function.to_owned(),
            }),
            description: description.to_owned(),
        },
    }
}

fn papyrus_ui_declaration(
    route: &'static str,
    id: &str,
    function: &str,
    parameters: &[(&str, ScriptValueType)],
    result: ScriptValueType,
    description: &str,
) -> EnginePapyrusFunctionDeclaration {
    EnginePapyrusFunctionDeclaration {
        route: route.to_owned(),
        declaration: ScriptFunctionDeclaration {
            id: ScriptFunctionId::new(id).expect("built-in UI function ID is valid"),
            component: ComponentId::new("ui").expect("built-in UI component ID is valid"),
            parameters: parameters
                .iter()
                .cloned()
                .map(|(id, value_type)| ScriptParameterDeclaration {
                    id: ScriptParameterId::new(id).expect("built-in UI parameter ID is valid"),
                    value_type,
                    optional: false,
                })
                .collect(),
            result: Some(ScriptResultDeclaration {
                value_type: result,
                optional: false,
            }),
            papyrus: Some(PapyrusFunctionAlias {
                provider: "UI".to_owned(),
                function: function.to_owned(),
            }),
            description: description.to_owned(),
        },
    }
}

/// Exact read-only `Input` functions executable through the engine's current
/// action-binding table. Physical polling and key injection remain separate
/// policy surfaces because this table does not expose a host key snapshot.
pub fn papyrus_input_declarations() -> Vec<EnginePapyrusFunctionDeclaration> {
    vec![
        papyrus_input_declaration(
            PAPYRUS_INPUT_GET_MAPPED_KEY_ROUTE,
            "get-mapped-key",
            "GetMappedKey",
            &[
                ("control", ScriptValueType::String, false),
                ("device-type", ScriptValueType::Integer, true),
            ],
            ScriptValueType::Integer,
            "Return the current DirectInput-style key code for a known control, or 0xff when unbound",
        ),
        papyrus_input_declaration(
            PAPYRUS_INPUT_GET_MAPPED_CONTROL_ROUTE,
            "get-mapped-control",
            "GetMappedControl",
            &[("keycode", ScriptValueType::Integer, false)],
            ScriptValueType::String,
            "Return the current control name for a known keyboard key code, or an empty string",
        ),
    ]
}

/// Exact read-only `UI` functions executable through the active menu snapshot.
pub fn papyrus_ui_declarations() -> Vec<EnginePapyrusFunctionDeclaration> {
    vec![papyrus_ui_declaration(
        PAPYRUS_UI_IS_MENU_OPEN_ROUTE,
        "is-menu-open",
        "IsMenuOpen",
        &[("menu-name", ScriptValueType::String)],
        ScriptValueType::Boolean,
        "Return whether the named engine-owned menu is currently visible",
    )]
}

/// One engine-owned binding projected into SKSE's Input key-code contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PapyrusInputBinding {
    pub control: String,
    pub device_type: i32,
    pub keycode: i32,
}

pub const PAPYRUS_INPUT_AUTO_DEVICE: i32 = 0xff;

pub const PAPYRUS_INPUT_UNBOUND_KEY: i32 = 0xff;

/// Snapshot of the one active menu exposed by the current UI manager.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PapyrusUiMenuSnapshot {
    pub active_menu: Option<String>,
    pub visible: bool,
}

/// Resolve `UI.IsMenuOpen` against the current active-menu snapshot.
pub fn adapt_papyrus_ui_is_menu_open(snapshot: &PapyrusUiMenuSnapshot, menu_name: &str) -> bool {
    snapshot.visible
        && !menu_name.is_empty()
        && snapshot
            .active_menu
            .as_deref()
            .is_some_and(|active| active == menu_name)
}

/// Resolve a control name using the current engine binding snapshot.
pub fn adapt_papyrus_input_get_mapped_key(
    bindings: &[PapyrusInputBinding],
    control: &str,
    device_type: i64,
) -> i32 {
    let Ok(device_type) = i32::try_from(device_type) else {
        return PAPYRUS_INPUT_UNBOUND_KEY;
    };
    if !matches!(device_type, 0..=2 | PAPYRUS_INPUT_AUTO_DEVICE) {
        return PAPYRUS_INPUT_UNBOUND_KEY;
    }
    bindings
        .iter()
        .find(|binding| {
            binding.control.eq_ignore_ascii_case(control)
                && (device_type == PAPYRUS_INPUT_AUTO_DEVICE || binding.device_type == device_type)
        })
        .map_or(PAPYRUS_INPUT_UNBOUND_KEY, |binding| binding.keycode)
}

/// Resolve a keyboard key code to its current control name.
pub fn adapt_papyrus_input_get_mapped_control(
    bindings: &[PapyrusInputBinding],
    keycode: i64,
) -> String {
    let Ok(keycode) = i32::try_from(keycode) else {
        return String::new();
    };
    bindings
        .iter()
        .find(|binding| binding.keycode == keycode)
        .map_or_else(String::new, |binding| binding.control.clone())
}
