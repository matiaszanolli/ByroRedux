//! Layer 2 — engine-side Papyrus function declaration builders.

use super::*;

/// One exact engine-owned Papyrus alias and its typed call declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnginePapyrusFunctionDeclaration {
    pub route: String,
    pub declaration: ScriptFunctionDeclaration,
}

/// One built-in engine Papyrus declaration, described once and built by [`Self::build`]: the single
/// home for the `ScriptFunctionDeclaration` every compatibility surface (Game, Input, UI, ModEvent,
/// JContainers, StorageUtil) registers (#6961).
pub(super) struct BuiltinDeclaration<'a> {
    pub(super) route: String,
    pub(super) id: &'a str,
    /// The SDK component the function runs in.
    pub(super) component: &'a str,
    /// The Papyrus script the function is an alias on, and its name there.
    pub(super) provider: &'a str,
    pub(super) function: &'a str,
    pub(super) parameters: &'a [(&'a str, ScriptValueType, bool)],
    pub(super) result: Option<ScriptValueType>,
    pub(super) description: &'a str,
    /// The surface an invalid built-in ID names when it panics (`Papyrus`, `Input`, ...).
    pub(super) label: &'a str,
}

impl BuiltinDeclaration<'_> {
    pub(super) fn build(&self) -> EnginePapyrusFunctionDeclaration {
        let label = self.label;
        EnginePapyrusFunctionDeclaration {
            route: self.route.clone(),
            declaration: ScriptFunctionDeclaration {
                id: ScriptFunctionId::new(self.id)
                    .unwrap_or_else(|_| panic!("built-in {label} function ID is valid")),
                component: ComponentId::new(self.component)
                    .unwrap_or_else(|_| panic!("built-in {label} component ID is valid")),
                parameters: self
                    .parameters
                    .iter()
                    .map(|&(id, value_type, optional)| ScriptParameterDeclaration {
                        id: ScriptParameterId::new(id)
                            .unwrap_or_else(|_| panic!("built-in {label} parameter ID is valid")),
                        value_type,
                        optional,
                    })
                    .collect(),
                result: self.result.map(|value_type| ScriptResultDeclaration {
                    value_type,
                    optional: false,
                }),
                papyrus: Some(PapyrusFunctionAlias {
                    provider: self.provider.to_owned(),
                    function: self.function.to_owned(),
                }),
                description: self.description.to_owned(),
            },
        }
    }
}

/// `(id, type)` parameter pairs as required `(id, type, optional)` triples.
pub(super) fn required_parameters<'a>(
    parameters: &[(&'a str, ScriptValueType)],
) -> Vec<(&'a str, ScriptValueType, bool)> {
    parameters.iter().map(|&(id, value_type)| (id, value_type, false)).collect()
}

fn papyrus_game_content_declaration(
    route: &'static str,
    id: &str,
    function: &str,
    parameters: &[(&str, ScriptValueType)],
    result: ScriptValueType,
    description: &str,
) -> EnginePapyrusFunctionDeclaration {
    BuiltinDeclaration {
        route: route.to_owned(),
        id,
        component: "content-catalog",
        provider: "Game",
        function,
        parameters: &required_parameters(parameters),
        result: Some(result),
        description,
        label: "Papyrus",
    }
    .build()
}

fn papyrus_game_entity_declaration(
    route: &'static str,
    id: &str,
    function: &str,
    description: &str,
) -> EnginePapyrusFunctionDeclaration {
    let mut declaration = papyrus_game_content_declaration(
        route,
        id,
        function,
        &[],
        ScriptValueType::Entity,
        description,
    );
    declaration
        .declaration
        .result
        .as_mut()
        .expect("entity compatibility declaration has a result")
        .optional = true;
    declaration.declaration.component =
        ComponentId::new("world").expect("built-in player component ID is valid");
    declaration
}

/// Exact SKSE `Game` functions executable through the content catalog.
pub fn papyrus_game_content_declarations() -> Vec<EnginePapyrusFunctionDeclaration> {
    vec![
        papyrus_game_entity_declaration(
            PAPYRUS_GAME_GET_PLAYER_ROUTE,
            "get-player",
            "GetPlayer",
            "Return the current engine player as an opaque entity handle, or None when no player body exists",
        ),
        papyrus_game_content_declaration(
            PAPYRUS_GAME_GET_MOD_COUNT_ROUTE,
            "get-mod-count",
            "GetModCount",
            &[],
            ScriptValueType::Integer,
            "Return the number of active regular plugins",
        ),
        papyrus_game_content_declaration(
            PAPYRUS_GAME_GET_MOD_BY_NAME_ROUTE,
            "get-mod-by-name",
            "GetModByName",
            &[("plugin", ScriptValueType::String)],
            ScriptValueType::Integer,
            "Return a regular index, 0x100 plus a light index, or 0xff when absent",
        ),
        papyrus_game_content_declaration(
            PAPYRUS_GAME_GET_FORM_FROM_FILE_ROUTE,
            "get-form-from-file",
            "GetFormFromFile",
            &[
                ("form-id", ScriptValueType::Integer),
                ("plugin", ScriptValueType::String),
            ],
            ScriptValueType::Form,
            "Qualify a plugin-local form ID into a portable FormRef, or None when absent/invalid",
        ),
        papyrus_game_content_declaration(
            PAPYRUS_GAME_GET_MOD_NAME_ROUTE,
            "get-mod-name",
            "GetModName",
            &[("index", ScriptValueType::Integer)],
            ScriptValueType::String,
            "Return the regular or offset-light plugin name at an SKSE mod index",
        ),
        papyrus_game_content_declaration(
            PAPYRUS_GAME_GET_MOD_DEPENDENCY_COUNT_ROUTE,
            "get-mod-dependency-count",
            "GetModDependencyCount",
            &[("index", ScriptValueType::Integer)],
            ScriptValueType::Integer,
            "Return the master count for a regular or offset-light plugin index",
        ),
        papyrus_game_content_declaration(
            PAPYRUS_GAME_IS_PLUGIN_INSTALLED_ROUTE,
            "is-plugin-installed",
            "IsPluginInstalled",
            &[("plugin", ScriptValueType::String)],
            ScriptValueType::Boolean,
            "Return whether a regular or light plugin is active",
        ),
        papyrus_game_content_declaration(
            PAPYRUS_GAME_GET_LIGHT_MOD_COUNT_ROUTE,
            "get-light-mod-count",
            "GetLightModCount",
            &[],
            ScriptValueType::Integer,
            "Return the number of active light plugins",
        ),
        papyrus_game_content_declaration(
            PAPYRUS_GAME_GET_LIGHT_MOD_BY_NAME_ROUTE,
            "get-light-mod-by-name",
            "GetLightModByName",
            &[("plugin", ScriptValueType::String)],
            ScriptValueType::Integer,
            "Return a light-plugin index or 0xffff when absent or regular",
        ),
        papyrus_game_content_declaration(
            PAPYRUS_GAME_GET_LIGHT_MOD_NAME_ROUTE,
            "get-light-mod-name",
            "GetLightModName",
            &[("index", ScriptValueType::Integer)],
            ScriptValueType::String,
            "Return the light-plugin name at an index or an empty string",
        ),
        papyrus_game_content_declaration(
            PAPYRUS_GAME_GET_LIGHT_MOD_DEPENDENCY_COUNT_ROUTE,
            "get-light-mod-dependency-count",
            "GetLightModDependencyCount",
            &[("index", ScriptValueType::Integer)],
            ScriptValueType::Integer,
            "Return the master count for a light-plugin index",
        ),
        papyrus_game_content_declaration(
            PAPYRUS_GAME_GET_NTH_LIGHT_MOD_DEPENDENCY_ROUTE,
            "get-nth-light-mod-dependency",
            "GetNthLightModDependency",
            &[
                ("mod-index", ScriptValueType::Integer),
                ("dependency-index", ScriptValueType::Integer),
            ],
            ScriptValueType::Integer,
            "Return the regular mod index of a light plugin's nth master",
        ),
    ]
}
