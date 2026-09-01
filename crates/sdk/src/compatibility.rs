//! Source-API compatibility catalog for extender-era Papyrus calls.
//!
//! This catalog does not pretend that recognizing a legacy call makes its
//! binary ABI available. It identifies the engine semantic service a source
//! adapter must target, or records why the operation is intentionally absent.

use crate::component::ExtensionValue;
use crate::content::{ContentCatalog, PluginKind};
use crate::event::{legacy_skse_mod_event_id, LegacySkseModEventPayload, PublishEventCommand};
use crate::identity::{
    ComponentId, FormRef, IdentityError, ScriptFunctionId, ScriptParameterId, StorageKey,
};
use crate::script_function::{
    PapyrusFunctionAlias, ScriptFunctionDeclaration, ScriptParameterDeclaration,
    ScriptResultDeclaration, ScriptValueType,
};
use crate::service::{
    CONTENT_CATALOG_SERVICE, CONTEXT_SERVICE, EVENT_SERVICE, LEGACY_CONTAINERS_SERVICE,
    PRINCIPAL_STORAGE_SERVICE,
};
use crate::storage::{PrincipalStorageCommand, PrincipalStorageValue};

/// Extender ecosystem that introduced a recognized Papyrus provider/call.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ExtenderFamily {
    Skse,
    F4se,
    Xnvse,
    Obse,
    PapyrusUtil,
    JContainers,
    Shared,
}

/// Exact source recipe for load-order discovery commands shared by OBSE and
/// xNVSE. Numeric indices are callback-local compatibility values; portable
/// authored identity remains [`FormRef`].
pub fn obscript_source_alias(command: &str) -> Option<SourceAlias> {
    let (function, operation, value_kind, constraint) =
        if command.eq_ignore_ascii_case("IsModLoaded") {
            (
                "IsModLoaded",
                "content.find",
                "bool",
                "plugin basename is case-insensitive and includes its extension",
            )
        } else if command.eq_ignore_ascii_case("GetModIndex") {
            (
                "GetModIndex",
                "content.find-index",
                "signed",
                "index is callback-local; missing xNVSE plugins use the legacy 255 sentinel",
            )
        } else if command.eq_ignore_ascii_case("GetNumLoadedMods") {
            (
                "GetNumLoadedMods",
                "content.count",
                "signed",
                "legacy xNVSE name; the catalog order is immutable for the callback",
            )
        } else if command.eq_ignore_ascii_case("GetNumLoadedPlugins") {
            (
                "GetNumLoadedPlugins",
                "content.count",
                "signed",
                "legacy OBSE name; the catalog order is immutable for the callback",
            )
        } else if command.eq_ignore_ascii_case("GetNthModName") {
            (
                "GetNthModName",
                "content.plugin-name",
                "text",
                "index is callback-local; use stable source identity outside the adapter",
            )
        } else {
            return None;
        };
    Some(SourceAlias {
        provider: "xNVSE/OBSE",
        function,
        service: CONTENT_CATALOG_SERVICE,
        operation,
        value_kind,
        constraint,
    })
}

/// Classify extender commands embedded in Oblivion/FO3/FNV ObScript source.
/// Version probes map to semantic feature discovery rather than pretending an
/// external loader is installed.
pub fn classify_obscript_command(command: &str) -> Option<CompatibilityMatch> {
    if obscript_source_alias(command).is_some() {
        return Some(native(
            ExtenderFamily::Shared,
            CONTENT_CATALOG_SERVICE,
            "an exact engine adapter executes the legacy load-order query against the immutable content catalog",
        ));
    }
    if command.eq_ignore_ascii_case("GetNVSEVersion")
        || command.eq_ignore_ascii_case("GetNVSERevision")
        || command.eq_ignore_ascii_case("GetNVSEBeta")
    {
        return Some(mapped(
            ExtenderFamily::Xnvse,
            CONTEXT_SERVICE,
            "replace xNVSE version gates with SDK/service feature discovery",
        ));
    }
    if command
        .get(.."GetNVSE".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("GetNVSE"))
    {
        return Some(unsupported(
            ExtenderFamily::Xnvse,
            "the xNVSE command namespace is recognized, but this command has no engine semantic mapping",
        ));
    }
    if command.eq_ignore_ascii_case("GetOBSEVersion")
        || command.eq_ignore_ascii_case("GetOBSERevision")
    {
        return Some(mapped(
            ExtenderFamily::Obse,
            CONTEXT_SERVICE,
            "replace OBSE version gates with SDK/service feature discovery",
        ));
    }
    if command
        .get(.."GetOBSE".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("GetOBSE"))
    {
        return Some(unsupported(
            ExtenderFamily::Obse,
            "the OBSE command namespace is recognized, but this command has no engine semantic mapping",
        ));
    }
    None
}

/// Current source-compatibility disposition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CompatibilityDisposition {
    /// A source-level adapter is installed and delegates to the named service.
    Native,
    /// An engine semantic service exists, but the legacy call still needs an
    /// adapter or explicit source migration.
    Mapped,
    /// The operation has no safe/equivalent engine contract.
    Unsupported,
}

/// Classification returned for one recognized extender-era call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilityMatch {
    pub family: ExtenderFamily,
    pub disposition: CompatibilityDisposition,
    pub service: Option<&'static str>,
    pub guidance: &'static str,
}

/// Exact semantic target for a legacy source call that can be ported onto an
/// existing engine service without reproducing extender internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceAlias {
    pub provider: &'static str,
    pub function: &'static str,
    pub service: &'static str,
    pub operation: &'static str,
    pub value_kind: &'static str,
    pub constraint: &'static str,
}

/// Largest plugin count representable by the classic OBSE/xNVSE load order.
/// Indices `0..=254` are valid and `255` is reserved as the missing sentinel.
pub const LEGACY_OBSCRIPT_PLUGIN_LIMIT: usize = 255;
pub const LEGACY_OBSCRIPT_MISSING_MOD_INDEX: i32 = 255;

/// Engine routes backing SKSE's content-discovery extensions on `Game`.
pub const PAPYRUS_GAME_GET_MOD_COUNT_ROUTE: &str = "byro.content.catalog.get-mod-count";
pub const PAPYRUS_GAME_GET_MOD_BY_NAME_ROUTE: &str = "byro.content.catalog.get-mod-by-name";
pub const PAPYRUS_GAME_GET_MOD_NAME_ROUTE: &str = "byro.content.catalog.get-mod-name";
pub const PAPYRUS_GAME_GET_MOD_DEPENDENCY_COUNT_ROUTE: &str =
    "byro.content.catalog.get-mod-dependency-count";
pub const PAPYRUS_GAME_IS_PLUGIN_INSTALLED_ROUTE: &str = "byro.content.catalog.is-plugin-installed";
pub const PAPYRUS_GAME_GET_LIGHT_MOD_COUNT_ROUTE: &str = "byro.content.catalog.get-light-mod-count";
pub const PAPYRUS_GAME_GET_LIGHT_MOD_BY_NAME_ROUTE: &str =
    "byro.content.catalog.get-light-mod-by-name";
pub const PAPYRUS_GAME_GET_LIGHT_MOD_NAME_ROUTE: &str = "byro.content.catalog.get-light-mod-name";
pub const PAPYRUS_GAME_GET_LIGHT_MOD_DEPENDENCY_COUNT_ROUTE: &str =
    "byro.content.catalog.get-light-mod-dependency-count";
pub const PAPYRUS_GAME_GET_NTH_LIGHT_MOD_DEPENDENCY_ROUTE: &str =
    "byro.content.catalog.get-nth-light-mod-dependency";
pub const PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.get-int-value";
pub const PAPYRUS_STORAGE_UTIL_PLUCK_INT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.pluck-int-value";
pub const PAPYRUS_STORAGE_UTIL_HAS_INT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.has-int-value";
pub const PAPYRUS_STORAGE_UTIL_SET_INT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.set-int-value";
pub const PAPYRUS_STORAGE_UTIL_UNSET_INT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.unset-int-value";
pub const PAPYRUS_STORAGE_UTIL_ADJUST_INT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.adjust-int-value";
pub const PAPYRUS_STORAGE_UTIL_GET_FLOAT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.get-float-value";
pub const PAPYRUS_STORAGE_UTIL_PLUCK_FLOAT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.pluck-float-value";
pub const PAPYRUS_STORAGE_UTIL_HAS_FLOAT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.has-float-value";
pub const PAPYRUS_STORAGE_UTIL_SET_FLOAT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.set-float-value";
pub const PAPYRUS_STORAGE_UTIL_UNSET_FLOAT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.unset-float-value";
pub const PAPYRUS_STORAGE_UTIL_ADJUST_FLOAT_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.adjust-float-value";
pub const PAPYRUS_STORAGE_UTIL_GET_STRING_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.get-string-value";
pub const PAPYRUS_STORAGE_UTIL_PLUCK_STRING_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.pluck-string-value";
pub const PAPYRUS_STORAGE_UTIL_HAS_STRING_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.has-string-value";
pub const PAPYRUS_STORAGE_UTIL_SET_STRING_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.set-string-value";
pub const PAPYRUS_STORAGE_UTIL_UNSET_STRING_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.unset-string-value";
pub const PAPYRUS_STORAGE_UTIL_GET_FORM_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.get-form-value";
pub const PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.pluck-form-value";
pub const PAPYRUS_STORAGE_UTIL_HAS_FORM_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.has-form-value";
pub const PAPYRUS_STORAGE_UTIL_SET_FORM_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.set-form-value";
pub const PAPYRUS_STORAGE_UTIL_UNSET_FORM_VALUE_ROUTE: &str =
    "byro.storage.compat.storage-util.unset-form-value";
pub const PAPYRUS_STORAGE_UTIL_LIST_ROUTE_PREFIX: &str = "byro.storage.compat.storage-util.list-";
pub const PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX: &str = "byro.legacy-containers.compat.";
pub const PAPYRUS_MOD_EVENT_ROUTE_PREFIX: &str = "byro.events.compat.mod-event.";

pub const PAPYRUS_GAME_LIGHT_MOD_OFFSET: i32 = 0x100;
pub const PAPYRUS_GAME_MISSING_LIGHT_MOD_INDEX: i32 = 0xffff;

/// One exact engine-owned Papyrus alias and its typed call declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnginePapyrusFunctionDeclaration {
    pub route: String,
    pub declaration: ScriptFunctionDeclaration,
}

fn papyrus_game_content_declaration(
    route: &'static str,
    id: &str,
    function: &str,
    parameters: &[(&str, ScriptValueType)],
    result: ScriptValueType,
    description: &str,
) -> EnginePapyrusFunctionDeclaration {
    let declaration = ScriptFunctionDeclaration {
        id: ScriptFunctionId::new(id).expect("built-in Papyrus function ID is valid"),
        component: ComponentId::new("content-catalog")
            .expect("built-in Papyrus component ID is valid"),
        parameters: parameters
            .iter()
            .cloned()
            .map(|(id, value_type)| ScriptParameterDeclaration {
                id: ScriptParameterId::new(id).expect("built-in Papyrus parameter ID is valid"),
                value_type,
                optional: false,
            })
            .collect(),
        result: Some(ScriptResultDeclaration {
            value_type: result,
            optional: false,
        }),
        papyrus: Some(PapyrusFunctionAlias {
            provider: "Game".to_owned(),
            function: function.to_owned(),
        }),
        description: description.to_owned(),
    };
    EnginePapyrusFunctionDeclaration {
        route: route.to_owned(),
        declaration,
    }
}

/// Exact SKSE `Game` functions executable through the content catalog.
pub fn papyrus_game_content_declarations() -> Vec<EnginePapyrusFunctionDeclaration> {
    vec![
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

fn papyrus_storage_util_list_declarations(
    object_and_key: &[(&str, ScriptValueType, bool); 2],
) -> Vec<EnginePapyrusFunctionDeclaration> {
    let mut declarations = Vec::with_capacity(58);
    for (kind, suffix, value_type) in [
        ("int", "Int", ScriptValueType::Integer),
        ("float", "Float", ScriptValueType::Float),
        ("string", "String", ScriptValueType::String),
        ("form", "Form", ScriptValueType::Form),
    ] {
        for (operation, function_operation, result, parameters) in [
            (
                "add",
                "Add",
                ScriptValueType::Integer,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("value", value_type, true),
                    ("allow-duplicate", ScriptValueType::Boolean, true),
                ],
            ),
            (
                "get",
                "Get",
                value_type,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("index", ScriptValueType::Integer, true),
                ],
            ),
            (
                "set",
                "Set",
                value_type,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("index", ScriptValueType::Integer, true),
                    ("value", value_type, true),
                ],
            ),
            (
                "pluck",
                "Pluck",
                value_type,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("index", ScriptValueType::Integer, true),
                    ("missing", value_type, true),
                ],
            ),
            ("shift", "Shift", value_type, object_and_key.to_vec()),
            ("pop", "Pop", value_type, object_and_key.to_vec()),
            (
                "count",
                "Count",
                ScriptValueType::Integer,
                object_and_key.to_vec(),
            ),
            (
                "clear",
                "Clear",
                ScriptValueType::Integer,
                object_and_key.to_vec(),
            ),
            (
                "remove-at",
                "RemoveAt",
                ScriptValueType::Boolean,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("index", ScriptValueType::Integer, true),
                ],
            ),
            (
                "insert",
                "Insert",
                ScriptValueType::Boolean,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("index", ScriptValueType::Integer, true),
                    ("value", value_type, true),
                ],
            ),
            (
                "remove",
                "Remove",
                ScriptValueType::Integer,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("value", value_type, true),
                    ("all-instances", ScriptValueType::Boolean, true),
                ],
            ),
            (
                "count-value",
                "CountValue",
                ScriptValueType::Integer,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("value", value_type, true),
                    ("exclude", ScriptValueType::Boolean, true),
                ],
            ),
            (
                "find",
                "Find",
                ScriptValueType::Integer,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("value", value_type, true),
                ],
            ),
            (
                "has",
                "Has",
                ScriptValueType::Boolean,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("value", value_type, true),
                ],
            ),
        ] {
            let function = format!("{suffix}List{function_operation}");
            let id = format!("storage-util-{kind}-list-{operation}");
            let route = format!("{PAPYRUS_STORAGE_UTIL_LIST_ROUTE_PREFIX}{kind}-{operation}");
            declarations.push(papyrus_storage_util_declaration(
                &route,
                &id,
                &function,
                &parameters,
                result,
            ));
        }
    }
    for (kind, suffix, value_type) in [
        ("int", "Int", ScriptValueType::Integer),
        ("float", "Float", ScriptValueType::Float),
    ] {
        let function = format!("{suffix}ListAdjust");
        let id = format!("storage-util-{kind}-list-adjust");
        let route = format!("{PAPYRUS_STORAGE_UTIL_LIST_ROUTE_PREFIX}{kind}-adjust");
        declarations.push(papyrus_storage_util_declaration(
            &route,
            &id,
            &function,
            &[
                object_and_key[0],
                object_and_key[1],
                ("index", ScriptValueType::Integer, true),
                ("amount", value_type, true),
            ],
            value_type,
        ));
    }
    declarations
}

fn papyrus_storage_util_declaration(
    route: &str,
    id: &str,
    function: &str,
    parameters: &[(&str, ScriptValueType, bool)],
    result: ScriptValueType,
) -> EnginePapyrusFunctionDeclaration {
    EnginePapyrusFunctionDeclaration {
        route: route.to_owned(),
        declaration: ScriptFunctionDeclaration {
            id: ScriptFunctionId::new(id).expect("built-in StorageUtil function ID is valid"),
            component: ComponentId::new("principal-storage")
                .expect("built-in StorageUtil component ID is valid"),
            parameters: parameters
                .iter()
                .cloned()
                .map(|(id, value_type, optional)| ScriptParameterDeclaration {
                    id: ScriptParameterId::new(id)
                        .expect("built-in StorageUtil parameter ID is valid"),
                    value_type,
                    optional,
                })
                .collect(),
            result: Some(ScriptResultDeclaration {
                value_type: result,
                optional: false,
            }),
            papyrus: Some(PapyrusFunctionAlias {
                provider: "StorageUtil".to_owned(),
                function: function.to_owned(),
            }),
            description: "Engine-owned principal-private PapyrusUtil compatibility".to_owned(),
        },
    }
}

fn legacy_container_id(provider: &str, function: &str) -> String {
    let mut id = provider.to_ascii_lowercase();
    id.push('-');
    for character in function.chars() {
        if character.is_ascii_uppercase() {
            id.push('-');
            id.push(character.to_ascii_lowercase());
        } else {
            id.push(character);
        }
    }
    id
}

fn papyrus_legacy_container_declaration(
    provider: &str,
    function: &str,
    parameters: &[(&str, ScriptValueType, bool)],
    result: Option<ScriptValueType>,
) -> EnginePapyrusFunctionDeclaration {
    let id = legacy_container_id(provider, function);
    EnginePapyrusFunctionDeclaration {
        route: format!("{PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX}{id}"),
        declaration: ScriptFunctionDeclaration {
            id: ScriptFunctionId::new(&id).expect("built-in JContainers function ID is valid"),
            component: ComponentId::new("legacy-containers")
                .expect("built-in JContainers component ID is valid"),
            parameters: parameters
                .iter()
                .cloned()
                .map(|(id, value_type, optional)| ScriptParameterDeclaration {
                    id: ScriptParameterId::new(id)
                        .expect("built-in JContainers parameter ID is valid"),
                    value_type,
                    optional,
                })
                .collect(),
            result: result.map(|value_type| ScriptResultDeclaration {
                value_type,
                optional: false,
            }),
            papyrus: Some(PapyrusFunctionAlias {
                provider: provider.to_owned(),
                function: function.to_owned(),
            }),
            description: "Engine-owned principal-private JContainers compatibility".to_owned(),
        },
    }
}

/// Exact in-memory JValue/JArray/JMap core backed by the save-persistent,
/// principal-private engine container registry.
pub fn papyrus_legacy_container_declarations() -> Vec<EnginePapyrusFunctionDeclaration> {
    let integer = ScriptValueType::Integer;
    let float = ScriptValueType::Float;
    let string = ScriptValueType::String;
    let form = ScriptValueType::Form;
    let boolean = ScriptValueType::Boolean;
    let object = [("object", integer, false)];
    let object_key = [("object", integer, false), ("key", string, false)];
    let mut declarations = vec![
        papyrus_legacy_container_declaration("JValue", "isExists", &object, Some(boolean)),
        papyrus_legacy_container_declaration("JValue", "isArray", &object, Some(boolean)),
        papyrus_legacy_container_declaration("JValue", "isMap", &object, Some(boolean)),
        papyrus_legacy_container_declaration("JValue", "empty", &object, Some(boolean)),
        papyrus_legacy_container_declaration("JValue", "count", &object, Some(integer)),
        papyrus_legacy_container_declaration("JValue", "clear", &object, None),
        papyrus_legacy_container_declaration("JValue", "shallowCopy", &object, Some(integer)),
        papyrus_legacy_container_declaration("JValue", "deepCopy", &object, Some(integer)),
        papyrus_legacy_container_declaration(
            "JValue",
            "retain",
            &[("object", integer, false), ("tag", string, true)],
            Some(integer),
        ),
        papyrus_legacy_container_declaration("JValue", "release", &object, Some(integer)),
        papyrus_legacy_container_declaration(
            "JValue",
            "releaseAndRetain",
            &[
                ("previous-object", integer, false),
                ("new-object", integer, false),
                ("tag", string, true),
            ],
            Some(integer),
        ),
        papyrus_legacy_container_declaration(
            "JValue",
            "releaseObjectsWithTag",
            &[("tag", string, false)],
            None,
        ),
        papyrus_legacy_container_declaration("JArray", "object", &[], Some(integer)),
        papyrus_legacy_container_declaration("JArray", "count", &object, Some(integer)),
        papyrus_legacy_container_declaration("JArray", "clear", &object, None),
        papyrus_legacy_container_declaration(
            "JArray",
            "eraseIndex",
            &[("object", integer, false), ("index", integer, false)],
            None,
        ),
        papyrus_legacy_container_declaration("JMap", "object", &[], Some(integer)),
        papyrus_legacy_container_declaration("JMap", "count", &object, Some(integer)),
        papyrus_legacy_container_declaration("JMap", "clear", &object, None),
        papyrus_legacy_container_declaration("JMap", "hasKey", &object_key, Some(boolean)),
        papyrus_legacy_container_declaration("JMap", "removeKey", &object_key, Some(boolean)),
    ];
    for (suffix, value_type, value_name, nullable) in [
        ("Int", integer, "value", false),
        ("Flt", float, "value", false),
        ("Str", string, "value", false),
        ("Form", form, "value", true),
        ("Obj", integer, "container", false),
    ] {
        declarations.push(papyrus_legacy_container_declaration(
            "JArray",
            &format!("add{suffix}"),
            &[
                ("object", integer, false),
                (value_name, value_type, nullable),
                ("add-to-index", integer, true),
            ],
            None,
        ));
        declarations.push(papyrus_legacy_container_declaration(
            "JArray",
            &format!("get{suffix}"),
            &[
                ("object", integer, false),
                ("index", integer, false),
                ("default", value_type, true),
            ],
            Some(value_type),
        ));
        declarations.push(papyrus_legacy_container_declaration(
            "JArray",
            &format!("set{suffix}"),
            &[
                ("object", integer, false),
                ("index", integer, false),
                (value_name, value_type, nullable),
            ],
            None,
        ));
        declarations.push(papyrus_legacy_container_declaration(
            "JMap",
            &format!("get{suffix}"),
            &[
                ("object", integer, false),
                ("key", string, false),
                ("default", value_type, true),
            ],
            Some(value_type),
        ));
        declarations.push(papyrus_legacy_container_declaration(
            "JMap",
            &format!("set{suffix}"),
            &[
                ("object", integer, false),
                ("key", string, false),
                (value_name, value_type, nullable),
            ],
            None,
        ));
    }
    declarations
}

fn papyrus_mod_event_declaration(
    function: &str,
    parameters: &[(&str, ScriptValueType, bool)],
    result: Option<ScriptValueType>,
) -> EnginePapyrusFunctionDeclaration {
    let operation = match function {
        "Create" => "create",
        "Send" => "send",
        "Release" => "release",
        "PushBool" => "push-bool",
        "PushInt" => "push-int",
        "PushFloat" => "push-float",
        "PushString" => "push-string",
        "PushForm" => "push-form",
        _ => unreachable!("ModEvent declarations are a closed built-in set"),
    };
    let id = format!("mod-event-{operation}");
    EnginePapyrusFunctionDeclaration {
        route: format!("{PAPYRUS_MOD_EVENT_ROUTE_PREFIX}{id}"),
        declaration: ScriptFunctionDeclaration {
            id: ScriptFunctionId::new(&id).expect("built-in ModEvent function ID is valid"),
            component: ComponentId::new("legacy-mod-events")
                .expect("built-in ModEvent component ID is valid"),
            parameters: parameters
                .iter()
                .cloned()
                .map(|(id, value_type, optional)| ScriptParameterDeclaration {
                    id: ScriptParameterId::new(id)
                        .expect("built-in ModEvent parameter ID is valid"),
                    value_type,
                    optional,
                })
                .collect(),
            result: result.map(|value_type| ScriptResultDeclaration {
                value_type,
                optional: false,
            }),
            papyrus: Some(PapyrusFunctionAlias {
                provider: "ModEvent".to_owned(),
                function: function.to_owned(),
            }),
            description: "Engine-owned principal-private SKSE ModEvent builder compatibility"
                .to_owned(),
        },
    }
}

/// Exact SKSE `ModEvent` handle-builder calls backed by the shared engine bus.
pub fn papyrus_mod_event_declarations() -> Vec<EnginePapyrusFunctionDeclaration> {
    let integer = ScriptValueType::Integer;
    let handle = ("handle", integer, false);
    vec![
        papyrus_mod_event_declaration(
            "Create",
            &[("event-name", ScriptValueType::String, false)],
            Some(integer),
        ),
        papyrus_mod_event_declaration("Send", &[handle], Some(ScriptValueType::Boolean)),
        papyrus_mod_event_declaration("Release", &[handle], None),
        papyrus_mod_event_declaration(
            "PushBool",
            &[handle, ("value", ScriptValueType::Boolean, false)],
            None,
        ),
        papyrus_mod_event_declaration("PushInt", &[handle, ("value", integer, false)], None),
        papyrus_mod_event_declaration(
            "PushFloat",
            &[handle, ("value", ScriptValueType::Float, false)],
            None,
        ),
        papyrus_mod_event_declaration(
            "PushString",
            &[handle, ("value", ScriptValueType::String, false)],
            None,
        ),
        papyrus_mod_event_declaration(
            "PushForm",
            &[handle, ("value", ScriptValueType::Form, true)],
            None,
        ),
    ]
}

/// Exact global scalar `StorageUtil` calls backed by principal-private engine
/// storage. The object key accepts only `None`; the host rejects every Form.
pub fn papyrus_storage_util_declarations() -> Vec<EnginePapyrusFunctionDeclaration> {
    let object_and_key = [
        ("object", ScriptValueType::Form, true),
        // `optional` is also the SDK's nullable marker. Because Papyrus puts
        // nullable ObjKey before required parameters, all following fields
        // must use that representation too; the scripting/host adapters
        // enforce the exact legacy arity independently.
        ("key", ScriptValueType::String, true),
    ];
    let mut declarations = vec![
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE,
            "storage-util-get-int-value",
            "GetIntValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("missing", ScriptValueType::Integer, true),
            ],
            ScriptValueType::Integer,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_PLUCK_INT_VALUE_ROUTE,
            "storage-util-pluck-int-value",
            "PluckIntValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("missing", ScriptValueType::Integer, true),
            ],
            ScriptValueType::Integer,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_HAS_INT_VALUE_ROUTE,
            "storage-util-has-int-value",
            "HasIntValue",
            &object_and_key,
            ScriptValueType::Boolean,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_SET_INT_VALUE_ROUTE,
            "storage-util-set-int-value",
            "SetIntValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("value", ScriptValueType::Integer, true),
            ],
            ScriptValueType::Integer,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_UNSET_INT_VALUE_ROUTE,
            "storage-util-unset-int-value",
            "UnsetIntValue",
            &object_and_key,
            ScriptValueType::Boolean,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_ADJUST_INT_VALUE_ROUTE,
            "storage-util-adjust-int-value",
            "AdjustIntValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("amount", ScriptValueType::Integer, true),
            ],
            ScriptValueType::Integer,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_GET_FLOAT_VALUE_ROUTE,
            "storage-util-get-float-value",
            "GetFloatValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("missing", ScriptValueType::Float, true),
            ],
            ScriptValueType::Float,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_PLUCK_FLOAT_VALUE_ROUTE,
            "storage-util-pluck-float-value",
            "PluckFloatValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("missing", ScriptValueType::Float, true),
            ],
            ScriptValueType::Float,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_HAS_FLOAT_VALUE_ROUTE,
            "storage-util-has-float-value",
            "HasFloatValue",
            &object_and_key,
            ScriptValueType::Boolean,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_SET_FLOAT_VALUE_ROUTE,
            "storage-util-set-float-value",
            "SetFloatValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("value", ScriptValueType::Float, true),
            ],
            ScriptValueType::Float,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_UNSET_FLOAT_VALUE_ROUTE,
            "storage-util-unset-float-value",
            "UnsetFloatValue",
            &object_and_key,
            ScriptValueType::Boolean,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_ADJUST_FLOAT_VALUE_ROUTE,
            "storage-util-adjust-float-value",
            "AdjustFloatValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("amount", ScriptValueType::Float, true),
            ],
            ScriptValueType::Float,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_GET_STRING_VALUE_ROUTE,
            "storage-util-get-string-value",
            "GetStringValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("missing", ScriptValueType::String, true),
            ],
            ScriptValueType::String,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_PLUCK_STRING_VALUE_ROUTE,
            "storage-util-pluck-string-value",
            "PluckStringValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("missing", ScriptValueType::String, true),
            ],
            ScriptValueType::String,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_HAS_STRING_VALUE_ROUTE,
            "storage-util-has-string-value",
            "HasStringValue",
            &object_and_key,
            ScriptValueType::Boolean,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_SET_STRING_VALUE_ROUTE,
            "storage-util-set-string-value",
            "SetStringValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("value", ScriptValueType::String, true),
            ],
            ScriptValueType::String,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_UNSET_STRING_VALUE_ROUTE,
            "storage-util-unset-string-value",
            "UnsetStringValue",
            &object_and_key,
            ScriptValueType::Boolean,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_GET_FORM_VALUE_ROUTE,
            "storage-util-get-form-value",
            "GetFormValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("missing", ScriptValueType::Form, true),
            ],
            ScriptValueType::Form,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE,
            "storage-util-pluck-form-value",
            "PluckFormValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("missing", ScriptValueType::Form, true),
            ],
            ScriptValueType::Form,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_HAS_FORM_VALUE_ROUTE,
            "storage-util-has-form-value",
            "HasFormValue",
            &object_and_key,
            ScriptValueType::Boolean,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_SET_FORM_VALUE_ROUTE,
            "storage-util-set-form-value",
            "SetFormValue",
            &[
                object_and_key[0],
                object_and_key[1],
                ("value", ScriptValueType::Form, true),
            ],
            ScriptValueType::Form,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_UNSET_FORM_VALUE_ROUTE,
            "storage-util-unset-form-value",
            "UnsetFormValue",
            &object_and_key,
            ScriptValueType::Boolean,
        ),
    ];
    declarations.extend(papyrus_storage_util_list_declarations(&object_and_key));
    declarations
}

pub fn adapt_papyrus_game_get_mod_count(catalog: &ContentCatalog) -> i32 {
    plugin_count(catalog, PluginKind::Regular)
}

/// Execute SKSE's `Game.GetModByName` against the immutable engine catalog.
pub fn adapt_papyrus_game_get_mod_by_name(catalog: &ContentCatalog, plugin: &str) -> i32 {
    let Some((kind, index)) = plugin_index(catalog, plugin) else {
        return LEGACY_OBSCRIPT_MISSING_MOD_INDEX;
    };
    match kind {
        PluginKind::Regular => index,
        PluginKind::Light => PAPYRUS_GAME_LIGHT_MOD_OFFSET.saturating_add(index),
    }
}

pub fn adapt_papyrus_game_get_mod_name(catalog: &ContentCatalog, index: i64) -> String {
    if index < 0 || index > i64::from(i32::MAX) {
        return String::new();
    }
    let index = index as i32;
    if index > LEGACY_OBSCRIPT_MISSING_MOD_INDEX {
        plugin_name(
            catalog,
            PluginKind::Light,
            index - PAPYRUS_GAME_LIGHT_MOD_OFFSET,
        )
    } else {
        plugin_name(catalog, PluginKind::Regular, index)
    }
}

pub fn adapt_papyrus_game_get_mod_dependency_count(catalog: &ContentCatalog, index: i64) -> i32 {
    i32::try_from(
        plugin_at_mod_index(catalog, index).map_or(0, |plugin| plugin.dependencies().len()),
    )
    .expect("content catalog dependency count fits i32")
}

pub fn adapt_papyrus_game_is_plugin_installed(catalog: &ContentCatalog, plugin: &str) -> bool {
    catalog.find(plugin).is_some()
}

pub fn adapt_papyrus_game_get_light_mod_count(catalog: &ContentCatalog) -> i32 {
    plugin_count(catalog, PluginKind::Light)
}

pub fn adapt_papyrus_game_get_light_mod_by_name(catalog: &ContentCatalog, plugin: &str) -> i32 {
    match plugin_index(catalog, plugin) {
        Some((PluginKind::Light, index)) => index,
        _ => PAPYRUS_GAME_MISSING_LIGHT_MOD_INDEX,
    }
}

pub fn adapt_papyrus_game_get_light_mod_name(catalog: &ContentCatalog, index: i64) -> String {
    i32::try_from(index).ok().map_or_else(String::new, |index| {
        plugin_name(catalog, PluginKind::Light, index)
    })
}

pub fn adapt_papyrus_game_get_light_mod_dependency_count(
    catalog: &ContentCatalog,
    index: i64,
) -> i32 {
    let count = i32::try_from(index)
        .ok()
        .and_then(|index| plugin_at(catalog, PluginKind::Light, index))
        .map_or(0, |plugin| plugin.dependencies().len());
    i32::try_from(count).expect("content catalog dependency count fits i32")
}

pub fn adapt_papyrus_game_get_nth_light_mod_dependency(
    catalog: &ContentCatalog,
    mod_index: i64,
    dependency_index: i64,
) -> i32 {
    let Some(plugin) = i32::try_from(mod_index)
        .ok()
        .and_then(|index| plugin_at(catalog, PluginKind::Light, index))
    else {
        return 0;
    };
    let Some(dependency_ordinal) = usize::try_from(dependency_index)
        .ok()
        .and_then(|index| plugin.dependencies().get(index))
        .copied()
    else {
        return 0;
    };
    let Some(dependency) = catalog.plugin(dependency_ordinal) else {
        return 0;
    };
    if dependency.kind() != PluginKind::Regular {
        return 0;
    }
    i32::try_from(
        catalog
            .iter()
            .take(dependency_ordinal as usize)
            .filter(|plugin| plugin.kind() == PluginKind::Regular)
            .count(),
    )
    .expect("content catalog regular index fits i32")
}

fn plugin_count(catalog: &ContentCatalog, kind: PluginKind) -> i32 {
    i32::try_from(
        catalog
            .iter()
            .filter(|plugin| plugin.kind() == kind)
            .count(),
    )
    .expect("content catalog count fits i32")
}

fn plugin_index(catalog: &ContentCatalog, name: &str) -> Option<(PluginKind, i32)> {
    let target = catalog.find(name)?.1;
    let kind = target.kind();
    let index = catalog
        .iter()
        .filter(|plugin| plugin.kind() == kind)
        .position(|plugin| std::ptr::eq(plugin, target))?;
    Some((
        kind,
        i32::try_from(index).expect("content catalog index fits i32"),
    ))
}

fn plugin_name(catalog: &ContentCatalog, kind: PluginKind, index: i32) -> String {
    let Ok(index) = usize::try_from(index) else {
        return String::new();
    };
    catalog
        .iter()
        .filter(|plugin| plugin.kind() == kind)
        .nth(index)
        .map_or_else(String::new, |plugin| plugin.name().to_owned())
}

fn plugin_at_mod_index(
    catalog: &ContentCatalog,
    index: i64,
) -> Option<&crate::content::PluginInfo> {
    let index = i32::try_from(index).ok()?;
    if index > LEGACY_OBSCRIPT_MISSING_MOD_INDEX {
        plugin_at(
            catalog,
            PluginKind::Light,
            index.checked_sub(PAPYRUS_GAME_LIGHT_MOD_OFFSET)?,
        )
    } else {
        plugin_at(catalog, PluginKind::Regular, index)
    }
}

fn plugin_at(
    catalog: &ContentCatalog,
    kind: PluginKind,
    index: i32,
) -> Option<&crate::content::PluginInfo> {
    let index = usize::try_from(index).ok()?;
    catalog
        .iter()
        .filter(|plugin| plugin.kind() == kind)
        .nth(index)
}

/// Typed load-order operation recovered from extender-era ObScript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyObscriptLoadOrderCall {
    IsModLoaded { plugin: String },
    GetModIndex { plugin: String },
    GetNumLoadedMods,
    GetNumLoadedPlugins,
    GetNthModName { index: i32 },
}

/// ObScript-visible scalar produced by a load-order compatibility call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyObscriptLoadOrderResult {
    Bool(bool),
    Integer(i32),
    String(String),
}

/// Failure to represent the active catalog through the classic 8-bit ABI.
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum LegacyObscriptLoadOrderError {
    #[error(
        "active catalog has {actual} plugins, exceeding the classic ObScript limit of {maximum}"
    )]
    PluginBudgetExceeded { actual: usize, maximum: usize },
}

/// Execute an OBSE/xNVSE load-order query against the immutable engine
/// content snapshot without loading an external script extender.
///
/// This deliberately preserves the classic `0xff` missing-index sentinel and
/// empty-string nth-name behavior. Catalog ordinals remain callback-local and
/// must not be persisted as authored identity.
pub fn adapt_legacy_obscript_load_order(
    catalog: &ContentCatalog,
    call: LegacyObscriptLoadOrderCall,
) -> Result<LegacyObscriptLoadOrderResult, LegacyObscriptLoadOrderError> {
    if catalog.len() > LEGACY_OBSCRIPT_PLUGIN_LIMIT {
        return Err(LegacyObscriptLoadOrderError::PluginBudgetExceeded {
            actual: catalog.len(),
            maximum: LEGACY_OBSCRIPT_PLUGIN_LIMIT,
        });
    }

    let result = match call {
        LegacyObscriptLoadOrderCall::IsModLoaded { plugin } => {
            LegacyObscriptLoadOrderResult::Bool(catalog.find(&plugin).is_some())
        }
        LegacyObscriptLoadOrderCall::GetModIndex { plugin } => {
            let index = catalog
                .find(&plugin)
                .map_or(LEGACY_OBSCRIPT_MISSING_MOD_INDEX, |(index, _)| {
                    i32::try_from(index).expect("classic content catalog index fits i32")
                });
            LegacyObscriptLoadOrderResult::Integer(index)
        }
        LegacyObscriptLoadOrderCall::GetNumLoadedMods
        | LegacyObscriptLoadOrderCall::GetNumLoadedPlugins => {
            LegacyObscriptLoadOrderResult::Integer(
                i32::try_from(catalog.len()).expect("classic content catalog length fits i32"),
            )
        }
        LegacyObscriptLoadOrderCall::GetNthModName { index } => {
            let name = u32::try_from(index)
                .ok()
                .and_then(|index| catalog.plugin(index))
                .map_or_else(String::new, |plugin| plugin.name().to_owned());
            LegacyObscriptLoadOrderResult::String(name)
        }
    };
    Ok(result)
}

/// Scalar `StorageUtil` call supported by the engine source adapter.
#[derive(Clone, Debug, PartialEq)]
pub enum StorageUtilScalarCall {
    GetInt { missing: i32 },
    PluckInt { missing: i32 },
    HasInt,
    SetInt { value: i32 },
    UnsetInt,
    AdjustInt { amount: i32 },
    GetFloat { missing: f32 },
    PluckFloat { missing: f32 },
    HasFloat,
    SetFloat { value: f32 },
    UnsetFloat,
    AdjustFloat { amount: f32 },
    GetString { missing: String },
    PluckString { missing: String },
    HasString,
    SetString { value: String },
    UnsetString,
    GetForm { missing: Option<FormRef> },
    PluckForm { missing: Option<FormRef> },
    HasForm,
    SetForm { value: Option<FormRef> },
    UnsetForm,
}

/// Papyrus-visible result produced by a scalar `StorageUtil` adapter call.
#[derive(Clone, Debug, PartialEq)]
pub enum StorageUtilScalarResult {
    Int(i32),
    Float(f32),
    Bool(bool),
    String(String),
    Form(Option<FormRef>),
}

/// Executable result of adapting one global scalar `StorageUtil` call.
#[derive(Clone, Debug, PartialEq)]
pub struct StorageUtilAdaptation {
    /// Type-isolated, case-folded key in the authenticated principal namespace.
    pub key: StorageKey,
    /// Value returned synchronously to Papyrus.
    pub result: StorageUtilScalarResult,
    /// Deferred engine mutation, absent for read-only calls.
    pub command: Option<PrincipalStorageCommand>,
}

/// Failure to preserve the supported `StorageUtil` scalar contract.
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum StorageUtilAdapterError {
    #[error(
        "StorageUtil key cannot be represented by the portable principal-storage grammar: {0}"
    )]
    InvalidKey(#[from] IdentityError),
    #[error("StorageUtil integer value is outside the Papyrus i32 range")]
    IntegerOutOfRange,
    #[error("StorageUtil integer adjustment overflowed the Papyrus i32 range")]
    IntegerOverflow,
    #[error("StorageUtil float value must be finite")]
    NonFiniteFloat,
    #[error("StorageUtil adapter found an incompatible value at its type-isolated key")]
    TypeMismatch,
}

/// Scalar element kind used by the exact `StorageUtil` list adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageUtilListKind {
    Int,
    Float,
    String,
    Form,
}

/// Typed value stored in a principal-private `StorageUtil` list.
#[derive(Clone, Debug, PartialEq)]
pub enum StorageUtilListValue {
    Int(i32),
    Float(f32),
    String(String),
    Form(Option<FormRef>),
}

/// Core global list operation supported by the engine source adapter.
#[derive(Clone, Debug, PartialEq)]
pub enum StorageUtilListCall {
    Add {
        value: StorageUtilListValue,
        allow_duplicate: bool,
    },
    Get {
        index: i32,
    },
    Set {
        index: i32,
        value: StorageUtilListValue,
    },
    Pluck {
        index: i32,
        missing: StorageUtilListValue,
    },
    Shift,
    Pop,
    Count,
    Clear,
    RemoveAt {
        index: i32,
    },
    Insert {
        index: i32,
        value: StorageUtilListValue,
    },
    Remove {
        value: StorageUtilListValue,
        all_instances: bool,
    },
    CountValue {
        value: StorageUtilListValue,
        exclude: bool,
    },
    Adjust {
        index: i32,
        amount: StorageUtilListValue,
    },
    Find {
        value: StorageUtilListValue,
    },
    Has {
        value: StorageUtilListValue,
    },
}

/// Papyrus-visible result of one core `StorageUtil` list operation.
#[derive(Clone, Debug, PartialEq)]
pub enum StorageUtilListResult {
    Value(StorageUtilListValue),
    Int(i32),
    Bool(bool),
}

/// Validated result plus deferred mutations for one list call.
#[derive(Clone, Debug, PartialEq)]
pub struct StorageUtilListAdaptation {
    pub key: StorageKey,
    pub result: StorageUtilListResult,
    pub commands: Vec<PrincipalStorageCommand>,
}

/// Closed operation names carried by built-in list routes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageUtilListOperation {
    Add,
    Get,
    Set,
    Pluck,
    Shift,
    Pop,
    Count,
    Clear,
    RemoveAt,
    Insert,
    Remove,
    CountValue,
    Adjust,
    Find,
    Has,
}

/// Failure to adapt SKSE's fixed-arity `SendModEvent` call.
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum LegacyModEventAdapterError {
    #[error("legacy mod-event name is empty or exceeds the reversible engine bound")]
    InvalidEventName,
    #[error("legacy mod-event payload exceeds the bounded engine event contract")]
    PayloadTooLarge,
}

/// Adapt `Form`/`Alias`/`ActiveMagicEffect.SendModEvent` to the shared,
/// engine-owned event service.
pub fn adapt_legacy_send_mod_event(
    event_name: &str,
    string_arg: String,
    number_arg: f32,
    sender: Option<FormRef>,
) -> Result<PublishEventCommand, LegacyModEventAdapterError> {
    let event =
        legacy_skse_mod_event_id(event_name).ok_or(LegacyModEventAdapterError::InvalidEventName)?;
    let payload = LegacySkseModEventPayload::new(string_arg, number_arg, sender)
        .encode()
        .ok_or(LegacyModEventAdapterError::PayloadTooLarge)?;
    PublishEventCommand::new(event, payload).ok_or(LegacyModEventAdapterError::PayloadTooLarge)
}

/// Exact source alias for extender-added instance calls.
pub fn method_source_alias(function: &str) -> Option<SourceAlias> {
    let (function, operation, value_kind, constraint) =
        if function.eq_ignore_ascii_case("SendModEvent") {
            (
                "SendModEvent",
                "events.publish",
                "legacy-skse-fixed-event",
                "event name <=53 UTF-8 bytes; sender must have a stable FormRef",
            )
        } else if function.eq_ignore_ascii_case("RegisterForModEvent") {
            (
                "RegisterForModEvent",
                "events.queue-legacy-subscribe",
                "legacy-skse-registration",
                "event name <=53 UTF-8 bytes; callback <=128 bytes; refresh after load",
            )
        } else if function.eq_ignore_ascii_case("UnregisterForModEvent") {
            (
                "UnregisterForModEvent",
                "events.queue-legacy-unsubscribe",
                "legacy-skse-registration",
                "event name <=53 UTF-8 bytes",
            )
        } else if function.eq_ignore_ascii_case("UnregisterForAllModEvents") {
            (
                "UnregisterForAllModEvents",
                "events.queue-legacy-unsubscribe-all",
                "legacy-skse-registration",
                "removes only runtime SKSE-compatibility registrations",
            )
        } else {
            return None;
        };
    Some(SourceAlias {
        provider: "Form/Alias/ActiveMagicEffect",
        function,
        service: EVENT_SERVICE,
        operation,
        value_kind,
        constraint,
    })
}

/// Resolve engine-backed PapyrusUtil source aliases.
///
/// `StorageUtil` namespaces values by `(object, key, type)`, while
/// `byro.storage` is private to one extension principal and keys each value
/// once. These aliases therefore cover only global (`ObjKey == None`) values
/// whose names fit the portable storage-key grammar. The
/// executable adapter below case-folds and type-namespaces those keys. Its
/// writes preserve synchronous return and same-callback visibility through the
/// host transaction overlay. Object-scoped values require extension
/// components, file access, and cross-principal sharing remain unsupported
/// until an engine service can honor their full contracts.
pub fn source_alias(provider: &str, function: &str) -> Option<SourceAlias> {
    if let Some(alias) = legacy_container_source_alias(provider, function) {
        return Some(alias);
    }
    if provider.eq_ignore_ascii_case("ModEvent") {
        let (function, operation, value_kind) = if function.eq_ignore_ascii_case("Create") {
            ("Create", "events.legacy-builder-create", "handle")
        } else if function.eq_ignore_ascii_case("Send") {
            ("Send", "events.legacy-builder-send", "bool")
        } else if function.eq_ignore_ascii_case("Release") {
            ("Release", "events.legacy-builder-release", "none")
        } else if function.eq_ignore_ascii_case("PushBool") {
            ("PushBool", "events.legacy-builder-push", "bool")
        } else if function.eq_ignore_ascii_case("PushInt") {
            ("PushInt", "events.legacy-builder-push", "signed")
        } else if function.eq_ignore_ascii_case("PushFloat") {
            ("PushFloat", "events.legacy-builder-push", "float")
        } else if function.eq_ignore_ascii_case("PushString") {
            ("PushString", "events.legacy-builder-push", "text")
        } else if function.eq_ignore_ascii_case("PushForm") {
            ("PushForm", "events.legacy-builder-push", "form")
        } else {
            return None;
        };
        return Some(SourceAlias {
            provider: "ModEvent",
            function,
            service: EVENT_SERVICE,
            operation,
            value_kind,
            constraint: "<=64 live handles; <=128 arguments; encoded payload <=4096 bytes",
        });
    }
    if !provider.eq_ignore_ascii_case("StorageUtil") {
        return None;
    }
    if let Some(alias) = storage_util_list_source_alias(function) {
        return Some(alias);
    }
    let (function, operation, value_kind) = if function.eq_ignore_ascii_case("GetIntValue") {
        ("GetIntValue", "storage.get", "signed")
    } else if function.eq_ignore_ascii_case("PluckIntValue") {
        ("PluckIntValue", "storage.get+queue-delete", "signed")
    } else if function.eq_ignore_ascii_case("HasIntValue") {
        ("HasIntValue", "storage.get", "signed")
    } else if function.eq_ignore_ascii_case("SetIntValue") {
        ("SetIntValue", "storage.queue-set/delete", "signed")
    } else if function.eq_ignore_ascii_case("UnsetIntValue") {
        ("UnsetIntValue", "storage.get+queue-delete", "signed")
    } else if function.eq_ignore_ascii_case("AdjustIntValue") {
        ("AdjustIntValue", "storage.get+queue-set/delete", "signed")
    } else if function.eq_ignore_ascii_case("GetFloatValue") {
        ("GetFloatValue", "storage.get", "float")
    } else if function.eq_ignore_ascii_case("PluckFloatValue") {
        ("PluckFloatValue", "storage.get+queue-delete", "float")
    } else if function.eq_ignore_ascii_case("HasFloatValue") {
        ("HasFloatValue", "storage.get", "float")
    } else if function.eq_ignore_ascii_case("SetFloatValue") {
        ("SetFloatValue", "storage.queue-set/delete", "float")
    } else if function.eq_ignore_ascii_case("UnsetFloatValue") {
        ("UnsetFloatValue", "storage.get+queue-delete", "float")
    } else if function.eq_ignore_ascii_case("AdjustFloatValue") {
        ("AdjustFloatValue", "storage.get+queue-set/delete", "float")
    } else if function.eq_ignore_ascii_case("GetStringValue") {
        ("GetStringValue", "storage.get", "text")
    } else if function.eq_ignore_ascii_case("PluckStringValue") {
        ("PluckStringValue", "storage.get+queue-delete", "text")
    } else if function.eq_ignore_ascii_case("HasStringValue") {
        ("HasStringValue", "storage.get", "text")
    } else if function.eq_ignore_ascii_case("SetStringValue") {
        ("SetStringValue", "storage.queue-set/delete", "text")
    } else if function.eq_ignore_ascii_case("UnsetStringValue") {
        ("UnsetStringValue", "storage.get+queue-delete", "text")
    } else if function.eq_ignore_ascii_case("GetFormValue") {
        ("GetFormValue", "storage.get", "form")
    } else if function.eq_ignore_ascii_case("PluckFormValue") {
        ("PluckFormValue", "storage.get+queue-delete", "form")
    } else if function.eq_ignore_ascii_case("HasFormValue") {
        ("HasFormValue", "storage.get", "form")
    } else if function.eq_ignore_ascii_case("SetFormValue") {
        ("SetFormValue", "storage.queue-set/delete", "form")
    } else if function.eq_ignore_ascii_case("UnsetFormValue") {
        ("UnsetFormValue", "storage.get+queue-delete", "form")
    } else {
        return None;
    };
    Some(SourceAlias {
        provider: "StorageUtil",
        function,
        service: PRINCIPAL_STORAGE_SERVICE,
        operation,
        value_kind,
        constraint: "ObjKey must be None; portable key; principal-private (no cross-mod sharing)",
    })
}

fn storage_util_list_source_alias(function: &str) -> Option<SourceAlias> {
    let aliases = [
        ("IntListAdd", "storage.array-get+queue-push", "signed"),
        ("IntListGet", "storage.array-get", "signed"),
        ("IntListSet", "storage.array-get+queue-set", "signed"),
        ("IntListPluck", "storage.array-get+queue-remove", "signed"),
        ("IntListShift", "storage.array-get+queue-remove", "signed"),
        ("IntListPop", "storage.array-get+queue-remove", "signed"),
        ("IntListCount", "storage.array-get", "signed"),
        ("IntListClear", "storage.array-get+queue-delete", "signed"),
        ("IntListRemoveAt", "storage.array-get+queue-remove", "bool"),
        ("IntListInsert", "storage.array-get+queue-replace", "bool"),
        ("IntListRemove", "storage.array-get+queue-replace", "signed"),
        ("IntListCountValue", "storage.array-get", "signed"),
        ("IntListAdjust", "storage.array-get+queue-set", "signed"),
        ("IntListFind", "storage.array-get", "signed"),
        ("IntListHas", "storage.array-get", "bool"),
        ("FloatListAdd", "storage.array-get+queue-push", "float"),
        ("FloatListGet", "storage.array-get", "float"),
        ("FloatListSet", "storage.array-get+queue-set", "float"),
        ("FloatListPluck", "storage.array-get+queue-remove", "float"),
        ("FloatListShift", "storage.array-get+queue-remove", "float"),
        ("FloatListPop", "storage.array-get+queue-remove", "float"),
        ("FloatListCount", "storage.array-get", "signed"),
        ("FloatListClear", "storage.array-get+queue-delete", "signed"),
        (
            "FloatListRemoveAt",
            "storage.array-get+queue-remove",
            "bool",
        ),
        ("FloatListInsert", "storage.array-get+queue-replace", "bool"),
        (
            "FloatListRemove",
            "storage.array-get+queue-replace",
            "signed",
        ),
        ("FloatListCountValue", "storage.array-get", "signed"),
        ("FloatListAdjust", "storage.array-get+queue-set", "float"),
        ("FloatListFind", "storage.array-get", "signed"),
        ("FloatListHas", "storage.array-get", "bool"),
        ("StringListAdd", "storage.array-get+queue-push", "text"),
        ("StringListGet", "storage.array-get", "text"),
        ("StringListSet", "storage.array-get+queue-set", "text"),
        ("StringListPluck", "storage.array-get+queue-remove", "text"),
        ("StringListShift", "storage.array-get+queue-remove", "text"),
        ("StringListPop", "storage.array-get+queue-remove", "text"),
        ("StringListCount", "storage.array-get", "signed"),
        (
            "StringListClear",
            "storage.array-get+queue-delete",
            "signed",
        ),
        (
            "StringListRemoveAt",
            "storage.array-get+queue-remove",
            "bool",
        ),
        (
            "StringListInsert",
            "storage.array-get+queue-replace",
            "bool",
        ),
        (
            "StringListRemove",
            "storage.array-get+queue-replace",
            "signed",
        ),
        ("StringListCountValue", "storage.array-get", "signed"),
        ("StringListFind", "storage.array-get", "signed"),
        ("StringListHas", "storage.array-get", "bool"),
        ("FormListAdd", "storage.array-get+queue-push", "form"),
        ("FormListGet", "storage.array-get", "form"),
        ("FormListSet", "storage.array-get+queue-set", "form"),
        ("FormListPluck", "storage.array-get+queue-remove", "form"),
        ("FormListShift", "storage.array-get+queue-remove", "form"),
        ("FormListPop", "storage.array-get+queue-remove", "form"),
        ("FormListCount", "storage.array-get", "signed"),
        ("FormListClear", "storage.array-get+queue-delete", "signed"),
        ("FormListRemoveAt", "storage.array-get+queue-remove", "bool"),
        ("FormListInsert", "storage.array-get+queue-replace", "bool"),
        (
            "FormListRemove",
            "storage.array-get+queue-replace",
            "signed",
        ),
        ("FormListCountValue", "storage.array-get", "signed"),
        ("FormListFind", "storage.array-get", "signed"),
        ("FormListHas", "storage.array-get", "bool"),
    ];
    aliases
        .into_iter()
        .find(|(candidate, _, _)| function.eq_ignore_ascii_case(candidate))
        .map(|(function, operation, value_kind)| SourceAlias {
            provider: "StorageUtil",
            function,
            service: PRINCIPAL_STORAGE_SERVICE,
            operation,
            value_kind,
            constraint: "ObjKey must be None; bounded typed list; principal-private",
        })
}

fn legacy_container_source_alias(provider: &str, function: &str) -> Option<SourceAlias> {
    let (provider, function, operation, value_kind) = if provider.eq_ignore_ascii_case("JValue") {
        if function.eq_ignore_ascii_case("isExists") {
            ("JValue", "isExists", "legacy-containers.is-exists", "bool")
        } else if function.eq_ignore_ascii_case("isArray") {
            ("JValue", "isArray", "legacy-containers.is-array", "bool")
        } else if function.eq_ignore_ascii_case("isMap") {
            ("JValue", "isMap", "legacy-containers.is-map", "bool")
        } else if function.eq_ignore_ascii_case("empty") {
            ("JValue", "empty", "legacy-containers.empty", "bool")
        } else if function.eq_ignore_ascii_case("count") {
            ("JValue", "count", "legacy-containers.count", "signed")
        } else if function.eq_ignore_ascii_case("clear") {
            ("JValue", "clear", "legacy-containers.clear", "none")
        } else if function.eq_ignore_ascii_case("shallowCopy") {
            (
                "JValue",
                "shallowCopy",
                "legacy-containers.shallow-copy",
                "handle",
            )
        } else if function.eq_ignore_ascii_case("deepCopy") {
            (
                "JValue",
                "deepCopy",
                "legacy-containers.deep-copy",
                "handle",
            )
        } else if function.eq_ignore_ascii_case("retain") {
            ("JValue", "retain", "legacy-containers.retain", "handle")
        } else if function.eq_ignore_ascii_case("release") {
            ("JValue", "release", "legacy-containers.release", "handle")
        } else if function.eq_ignore_ascii_case("releaseAndRetain") {
            (
                "JValue",
                "releaseAndRetain",
                "legacy-containers.release-and-retain",
                "handle",
            )
        } else if function.eq_ignore_ascii_case("releaseObjectsWithTag") {
            (
                "JValue",
                "releaseObjectsWithTag",
                "legacy-containers.release-objects-with-tag",
                "none",
            )
        } else {
            return None;
        }
    } else if provider.eq_ignore_ascii_case("JArray") {
        if function.eq_ignore_ascii_case("object") {
            (
                "JArray",
                "object",
                "legacy-containers.array-create",
                "handle",
            )
        } else if function.eq_ignore_ascii_case("count") {
            ("JArray", "count", "legacy-containers.count", "signed")
        } else if function.eq_ignore_ascii_case("clear") {
            ("JArray", "clear", "legacy-containers.clear", "none")
        } else if function.eq_ignore_ascii_case("eraseIndex") {
            (
                "JArray",
                "eraseIndex",
                "legacy-containers.array-erase",
                "none",
            )
        } else if function.eq_ignore_ascii_case("addInt") {
            ("JArray", "addInt", "legacy-containers.array-add", "signed")
        } else if function.eq_ignore_ascii_case("addFlt") {
            ("JArray", "addFlt", "legacy-containers.array-add", "float")
        } else if function.eq_ignore_ascii_case("addStr") {
            ("JArray", "addStr", "legacy-containers.array-add", "text")
        } else if function.eq_ignore_ascii_case("addForm") {
            ("JArray", "addForm", "legacy-containers.array-add", "form")
        } else if function.eq_ignore_ascii_case("addObj") {
            ("JArray", "addObj", "legacy-containers.array-add", "handle")
        } else if function.eq_ignore_ascii_case("getInt") {
            ("JArray", "getInt", "legacy-containers.array-get", "signed")
        } else if function.eq_ignore_ascii_case("getFlt") {
            ("JArray", "getFlt", "legacy-containers.array-get", "float")
        } else if function.eq_ignore_ascii_case("getStr") {
            ("JArray", "getStr", "legacy-containers.array-get", "text")
        } else if function.eq_ignore_ascii_case("getForm") {
            ("JArray", "getForm", "legacy-containers.array-get", "form")
        } else if function.eq_ignore_ascii_case("getObj") {
            ("JArray", "getObj", "legacy-containers.array-get", "handle")
        } else if function.eq_ignore_ascii_case("setInt") {
            ("JArray", "setInt", "legacy-containers.array-set", "signed")
        } else if function.eq_ignore_ascii_case("setFlt") {
            ("JArray", "setFlt", "legacy-containers.array-set", "float")
        } else if function.eq_ignore_ascii_case("setStr") {
            ("JArray", "setStr", "legacy-containers.array-set", "text")
        } else if function.eq_ignore_ascii_case("setForm") {
            ("JArray", "setForm", "legacy-containers.array-set", "form")
        } else if function.eq_ignore_ascii_case("setObj") {
            ("JArray", "setObj", "legacy-containers.array-set", "handle")
        } else {
            return None;
        }
    } else if provider.eq_ignore_ascii_case("JMap") {
        if function.eq_ignore_ascii_case("object") {
            ("JMap", "object", "legacy-containers.map-create", "handle")
        } else if function.eq_ignore_ascii_case("count") {
            ("JMap", "count", "legacy-containers.count", "signed")
        } else if function.eq_ignore_ascii_case("clear") {
            ("JMap", "clear", "legacy-containers.clear", "none")
        } else if function.eq_ignore_ascii_case("hasKey") {
            ("JMap", "hasKey", "legacy-containers.map-has-key", "bool")
        } else if function.eq_ignore_ascii_case("removeKey") {
            ("JMap", "removeKey", "legacy-containers.map-remove", "none")
        } else if function.eq_ignore_ascii_case("setInt") {
            ("JMap", "setInt", "legacy-containers.map-set", "signed")
        } else if function.eq_ignore_ascii_case("setFlt") {
            ("JMap", "setFlt", "legacy-containers.map-set", "float")
        } else if function.eq_ignore_ascii_case("setStr") {
            ("JMap", "setStr", "legacy-containers.map-set", "text")
        } else if function.eq_ignore_ascii_case("setForm") {
            ("JMap", "setForm", "legacy-containers.map-set", "form")
        } else if function.eq_ignore_ascii_case("setObj") {
            ("JMap", "setObj", "legacy-containers.map-set", "handle")
        } else if function.eq_ignore_ascii_case("getInt") {
            ("JMap", "getInt", "legacy-containers.map-get", "signed")
        } else if function.eq_ignore_ascii_case("getFlt") {
            ("JMap", "getFlt", "legacy-containers.map-get", "float")
        } else if function.eq_ignore_ascii_case("getStr") {
            ("JMap", "getStr", "legacy-containers.map-get", "text")
        } else if function.eq_ignore_ascii_case("getForm") {
            ("JMap", "getForm", "legacy-containers.map-get", "form")
        } else if function.eq_ignore_ascii_case("getObj") {
            ("JMap", "getObj", "legacy-containers.map-get", "handle")
        } else {
            return None;
        }
    } else {
        return None;
    };
    Some(SourceAlias {
        provider,
        function,
        service: LEGACY_CONTAINERS_SERVICE,
        operation,
        value_kind,
        constraint: "principal-local; <=256 objects; <=4096 aggregate entries; <=4096 UTF-8 bytes per key/string",
    })
}

/// Execute the engine recipe for a supported global scalar `StorageUtil` call.
///
/// The caller supplies the current value from the callback transaction overlay
/// and queues the returned command through `byro.storage`. Scalar type
/// keys are kept separate exactly as in `StorageUtil`, and names are folded to
/// ASCII lowercase because the legacy API treats value names case-insensitively.
pub fn adapt_storage_util_global_scalar(
    key_name: &str,
    call: StorageUtilScalarCall,
    current: Option<&PrincipalStorageValue>,
) -> Result<StorageUtilAdaptation, StorageUtilAdapterError> {
    let prefix = match &call {
        StorageUtilScalarCall::GetInt { .. }
        | StorageUtilScalarCall::PluckInt { .. }
        | StorageUtilScalarCall::HasInt
        | StorageUtilScalarCall::SetInt { .. }
        | StorageUtilScalarCall::UnsetInt
        | StorageUtilScalarCall::AdjustInt { .. } => "storageutil.int:",
        StorageUtilScalarCall::GetFloat { .. }
        | StorageUtilScalarCall::PluckFloat { .. }
        | StorageUtilScalarCall::HasFloat
        | StorageUtilScalarCall::SetFloat { .. }
        | StorageUtilScalarCall::UnsetFloat
        | StorageUtilScalarCall::AdjustFloat { .. } => "storageutil.float:",
        StorageUtilScalarCall::GetString { .. }
        | StorageUtilScalarCall::PluckString { .. }
        | StorageUtilScalarCall::HasString
        | StorageUtilScalarCall::SetString { .. }
        | StorageUtilScalarCall::UnsetString => "storageutil.string:",
        StorageUtilScalarCall::GetForm { .. }
        | StorageUtilScalarCall::PluckForm { .. }
        | StorageUtilScalarCall::HasForm
        | StorageUtilScalarCall::SetForm { .. }
        | StorageUtilScalarCall::UnsetForm => "storageutil.form:",
    };
    let key = StorageKey::new(format!("{prefix}{}", key_name.to_ascii_lowercase()))?;

    let (result, command) = match call {
        StorageUtilScalarCall::GetInt { missing } => {
            let value = match current {
                Some(PrincipalStorageValue::I64(value)) => {
                    i32::try_from(*value).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?
                }
                Some(_) => return Err(StorageUtilAdapterError::TypeMismatch),
                None => missing,
            };
            (StorageUtilScalarResult::Int(value), None)
        }
        StorageUtilScalarCall::PluckInt { missing } => {
            let value = checked_int(current)?.unwrap_or(missing);
            (
                StorageUtilScalarResult::Int(value),
                Some(PrincipalStorageCommand::Delete { key: key.clone() }),
            )
        }
        StorageUtilScalarCall::HasInt => (
            StorageUtilScalarResult::Bool(checked_int(current)?.is_some()),
            None,
        ),
        StorageUtilScalarCall::SetInt { value } => {
            let command = if value == 0 {
                PrincipalStorageCommand::Delete { key: key.clone() }
            } else {
                PrincipalStorageCommand::Set {
                    key: key.clone(),
                    value: ExtensionValue::I64(i64::from(value)),
                }
            };
            (StorageUtilScalarResult::Int(value), Some(command))
        }
        StorageUtilScalarCall::UnsetInt => (
            StorageUtilScalarResult::Bool(checked_int(current)?.is_some()),
            Some(PrincipalStorageCommand::Delete { key: key.clone() }),
        ),
        StorageUtilScalarCall::AdjustInt { amount } => {
            let value = checked_int(current)?
                .unwrap_or(0)
                .checked_add(amount)
                .ok_or(StorageUtilAdapterError::IntegerOverflow)?;
            let command = storage_util_set_int(&key, value);
            (StorageUtilScalarResult::Int(value), Some(command))
        }
        StorageUtilScalarCall::GetFloat { missing } => {
            validate_storage_util_float(missing)?;
            (
                StorageUtilScalarResult::Float(checked_float(current)?.unwrap_or(missing)),
                None,
            )
        }
        StorageUtilScalarCall::PluckFloat { missing } => {
            validate_storage_util_float(missing)?;
            (
                StorageUtilScalarResult::Float(checked_float(current)?.unwrap_or(missing)),
                Some(PrincipalStorageCommand::Delete { key: key.clone() }),
            )
        }
        StorageUtilScalarCall::HasFloat => (
            StorageUtilScalarResult::Bool(checked_float(current)?.is_some()),
            None,
        ),
        StorageUtilScalarCall::SetFloat { value } => {
            validate_storage_util_float(value)?;
            let command = storage_util_set_float(&key, value);
            (StorageUtilScalarResult::Float(value), Some(command))
        }
        StorageUtilScalarCall::UnsetFloat => (
            StorageUtilScalarResult::Bool(checked_float(current)?.is_some()),
            Some(PrincipalStorageCommand::Delete { key: key.clone() }),
        ),
        StorageUtilScalarCall::AdjustFloat { amount } => {
            validate_storage_util_float(amount)?;
            let value = checked_float(current)?.unwrap_or(0.0) + amount;
            validate_storage_util_float(value)?;
            let command = storage_util_set_float(&key, value);
            (StorageUtilScalarResult::Float(value), Some(command))
        }
        StorageUtilScalarCall::GetString { missing } => {
            let value = match current {
                Some(PrincipalStorageValue::String(value)) => value.clone(),
                Some(_) => return Err(StorageUtilAdapterError::TypeMismatch),
                None => missing,
            };
            (StorageUtilScalarResult::String(value), None)
        }
        StorageUtilScalarCall::PluckString { missing } => (
            StorageUtilScalarResult::String(
                checked_string(current)?.map_or(missing, str::to_owned),
            ),
            Some(PrincipalStorageCommand::Delete { key: key.clone() }),
        ),
        StorageUtilScalarCall::HasString => (
            StorageUtilScalarResult::Bool(checked_string(current)?.is_some()),
            None,
        ),
        StorageUtilScalarCall::SetString { value } => {
            let command = if value.is_empty() {
                PrincipalStorageCommand::Delete { key: key.clone() }
            } else {
                PrincipalStorageCommand::Set {
                    key: key.clone(),
                    value: ExtensionValue::String(value.clone()),
                }
            };
            (StorageUtilScalarResult::String(value), Some(command))
        }
        StorageUtilScalarCall::UnsetString => (
            StorageUtilScalarResult::Bool(checked_string(current)?.is_some()),
            Some(PrincipalStorageCommand::Delete { key: key.clone() }),
        ),
        StorageUtilScalarCall::GetForm { missing } => (
            StorageUtilScalarResult::Form(checked_form(current)?.or(missing)),
            None,
        ),
        StorageUtilScalarCall::PluckForm { missing } => (
            StorageUtilScalarResult::Form(checked_form(current)?.or(missing)),
            Some(PrincipalStorageCommand::Delete { key: key.clone() }),
        ),
        StorageUtilScalarCall::HasForm => (
            StorageUtilScalarResult::Bool(checked_form(current)?.is_some()),
            None,
        ),
        StorageUtilScalarCall::SetForm { value } => {
            let command = match value {
                Some(value) => PrincipalStorageCommand::Set {
                    key: key.clone(),
                    value: ExtensionValue::Bytes(encode_storage_util_form(value)),
                },
                None => PrincipalStorageCommand::Delete { key: key.clone() },
            };
            (StorageUtilScalarResult::Form(value), Some(command))
        }
        StorageUtilScalarCall::UnsetForm => (
            StorageUtilScalarResult::Bool(checked_form(current)?.is_some()),
            Some(PrincipalStorageCommand::Delete { key: key.clone() }),
        ),
    };
    Ok(StorageUtilAdaptation {
        key,
        result,
        command,
    })
}

fn storage_util_set_int(key: &StorageKey, value: i32) -> PrincipalStorageCommand {
    if value == 0 {
        PrincipalStorageCommand::Delete { key: key.clone() }
    } else {
        PrincipalStorageCommand::Set {
            key: key.clone(),
            value: ExtensionValue::I64(i64::from(value)),
        }
    }
}

fn validate_storage_util_float(value: f32) -> Result<(), StorageUtilAdapterError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(StorageUtilAdapterError::NonFiniteFloat)
    }
}

fn storage_util_set_float(key: &StorageKey, value: f32) -> PrincipalStorageCommand {
    if value == 0.0 {
        PrincipalStorageCommand::Delete { key: key.clone() }
    } else {
        PrincipalStorageCommand::Set {
            key: key.clone(),
            value: ExtensionValue::Bytes(value.to_bits().to_le_bytes().to_vec()),
        }
    }
}

fn encode_storage_util_form(value: FormRef) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(20);
    encoded.extend_from_slice(&value.source());
    encoded.extend_from_slice(&value.local().to_le_bytes());
    encoded
}

fn checked_int(
    current: Option<&PrincipalStorageValue>,
) -> Result<Option<i32>, StorageUtilAdapterError> {
    match current {
        Some(PrincipalStorageValue::I64(value)) => Ok(Some(
            i32::try_from(*value).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
        )),
        Some(_) => Err(StorageUtilAdapterError::TypeMismatch),
        None => Ok(None),
    }
}

fn checked_string(
    current: Option<&PrincipalStorageValue>,
) -> Result<Option<&str>, StorageUtilAdapterError> {
    match current {
        Some(PrincipalStorageValue::String(value)) => Ok(Some(value)),
        Some(_) => Err(StorageUtilAdapterError::TypeMismatch),
        None => Ok(None),
    }
}

fn checked_float(
    current: Option<&PrincipalStorageValue>,
) -> Result<Option<f32>, StorageUtilAdapterError> {
    let Some(current) = current else {
        return Ok(None);
    };
    let PrincipalStorageValue::Bytes(encoded) = current else {
        return Err(StorageUtilAdapterError::TypeMismatch);
    };
    let encoded: [u8; 4] = encoded
        .as_slice()
        .try_into()
        .map_err(|_| StorageUtilAdapterError::TypeMismatch)?;
    let value = f32::from_bits(u32::from_le_bytes(encoded));
    validate_storage_util_float(value)?;
    Ok(Some(value))
}

fn checked_form(
    current: Option<&PrincipalStorageValue>,
) -> Result<Option<FormRef>, StorageUtilAdapterError> {
    let Some(current) = current else {
        return Ok(None);
    };
    let PrincipalStorageValue::Bytes(encoded) = current else {
        return Err(StorageUtilAdapterError::TypeMismatch);
    };
    if encoded.len() != 20 {
        return Err(StorageUtilAdapterError::TypeMismatch);
    }
    let mut source = [0_u8; 16];
    source.copy_from_slice(&encoded[..16]);
    let local = u32::from_le_bytes(
        encoded[16..]
            .try_into()
            .map_err(|_| StorageUtilAdapterError::TypeMismatch)?,
    );
    Ok(Some(FormRef::new(source, local)))
}

/// Decode a built-in global `StorageUtil` list route.
pub fn parse_storage_util_list_route(
    route: &str,
) -> Option<(StorageUtilListKind, StorageUtilListOperation)> {
    let suffix = route.strip_prefix(PAPYRUS_STORAGE_UTIL_LIST_ROUTE_PREFIX)?;
    let (kind, operation) = suffix.split_once('-')?;
    let kind = match kind {
        "int" => StorageUtilListKind::Int,
        "float" => StorageUtilListKind::Float,
        "string" => StorageUtilListKind::String,
        "form" => StorageUtilListKind::Form,
        _ => return None,
    };
    let operation = match operation {
        "add" => StorageUtilListOperation::Add,
        "get" => StorageUtilListOperation::Get,
        "set" => StorageUtilListOperation::Set,
        "pluck" => StorageUtilListOperation::Pluck,
        "shift" => StorageUtilListOperation::Shift,
        "pop" => StorageUtilListOperation::Pop,
        "count" => StorageUtilListOperation::Count,
        "clear" => StorageUtilListOperation::Clear,
        "remove-at" => StorageUtilListOperation::RemoveAt,
        "insert" => StorageUtilListOperation::Insert,
        "remove" => StorageUtilListOperation::Remove,
        "count-value" => StorageUtilListOperation::CountValue,
        "adjust" => StorageUtilListOperation::Adjust,
        "find" => StorageUtilListOperation::Find,
        "has" => StorageUtilListOperation::Has,
        _ => return None,
    };
    Some((kind, operation))
}

/// Adapt one exact global `StorageUtil` list call to bounded principal storage.
pub fn adapt_storage_util_global_list(
    key_name: &str,
    kind: StorageUtilListKind,
    call: StorageUtilListCall,
    current: Option<&PrincipalStorageValue>,
    max_entries: usize,
) -> Result<StorageUtilListAdaptation, StorageUtilAdapterError> {
    let kind_name = match kind {
        StorageUtilListKind::Int => "int",
        StorageUtilListKind::Float => "float",
        StorageUtilListKind::String => "string",
        StorageUtilListKind::Form => "form",
    };
    let key = StorageKey::new(format!(
        "storageutil.list.{kind_name}:{}",
        key_name.to_ascii_lowercase()
    ))?;
    let values = decode_storage_util_list(kind, current)?;
    let mut commands = Vec::with_capacity(1);
    let result = match call {
        StorageUtilListCall::Add {
            value,
            allow_duplicate,
        } => {
            let encoded = encode_storage_util_list_value(kind, &value)?;
            if values.len() >= max_entries || (!allow_duplicate && values.contains(&value)) {
                StorageUtilListResult::Int(-1)
            } else {
                let index = i32::try_from(values.len())
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?;
                commands.push(PrincipalStorageCommand::ArrayPush {
                    key: key.clone(),
                    value: encoded,
                });
                StorageUtilListResult::Int(index)
            }
        }
        StorageUtilListCall::Get { index } => {
            let value = usize::try_from(index)
                .ok()
                .and_then(|index| values.get(index))
                .cloned()
                .unwrap_or_else(|| default_storage_util_list_value(kind));
            StorageUtilListResult::Value(value)
        }
        StorageUtilListCall::Set { index, value } => {
            let encoded = encode_storage_util_list_value(kind, &value)?;
            let Some((index, previous)) = usize::try_from(index)
                .ok()
                .and_then(|index| values.get(index).cloned().map(|value| (index, value)))
            else {
                return Ok(StorageUtilListAdaptation {
                    key,
                    result: StorageUtilListResult::Value(default_storage_util_list_value(kind)),
                    commands,
                });
            };
            commands.push(PrincipalStorageCommand::ArraySet {
                key: key.clone(),
                index: u32::try_from(index)
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
                value: encoded,
            });
            StorageUtilListResult::Value(previous)
        }
        StorageUtilListCall::Pluck { index, missing } => {
            encode_storage_util_list_value(kind, &missing)?;
            let Some((index, value)) = usize::try_from(index)
                .ok()
                .and_then(|index| values.get(index).cloned().map(|value| (index, value)))
            else {
                return Ok(StorageUtilListAdaptation {
                    key,
                    result: StorageUtilListResult::Value(missing),
                    commands,
                });
            };
            commands.push(PrincipalStorageCommand::ArrayRemove {
                key: key.clone(),
                index: u32::try_from(index)
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
            });
            StorageUtilListResult::Value(value)
        }
        StorageUtilListCall::Shift => {
            let value = values
                .first()
                .cloned()
                .unwrap_or_else(|| default_storage_util_list_value(kind));
            if !values.is_empty() {
                commands.push(PrincipalStorageCommand::ArrayRemove {
                    key: key.clone(),
                    index: 0,
                });
            }
            StorageUtilListResult::Value(value)
        }
        StorageUtilListCall::Pop => {
            let Some((index, value)) = values
                .len()
                .checked_sub(1)
                .map(|index| (index, values[index].clone()))
            else {
                return Ok(StorageUtilListAdaptation {
                    key,
                    result: StorageUtilListResult::Value(default_storage_util_list_value(kind)),
                    commands,
                });
            };
            commands.push(PrincipalStorageCommand::ArrayRemove {
                key: key.clone(),
                index: u32::try_from(index)
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
            });
            StorageUtilListResult::Value(value)
        }
        StorageUtilListCall::Count => StorageUtilListResult::Int(
            i32::try_from(values.len()).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
        ),
        StorageUtilListCall::Clear => {
            let count = i32::try_from(values.len())
                .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?;
            commands.push(PrincipalStorageCommand::Delete { key: key.clone() });
            StorageUtilListResult::Int(count)
        }
        StorageUtilListCall::RemoveAt { index } => {
            let Some(index) = usize::try_from(index)
                .ok()
                .filter(|index| *index < values.len())
            else {
                return Ok(StorageUtilListAdaptation {
                    key,
                    result: StorageUtilListResult::Bool(false),
                    commands,
                });
            };
            commands.push(PrincipalStorageCommand::ArrayRemove {
                key: key.clone(),
                index: u32::try_from(index)
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
            });
            StorageUtilListResult::Bool(true)
        }
        StorageUtilListCall::Insert { index, value } => {
            let Some(index) = usize::try_from(index)
                .ok()
                .filter(|index| *index <= values.len())
            else {
                return Ok(StorageUtilListAdaptation {
                    key,
                    result: StorageUtilListResult::Bool(false),
                    commands,
                });
            };
            encode_storage_util_list_value(kind, &value)?;
            if values.len() >= max_entries {
                StorageUtilListResult::Bool(false)
            } else {
                let mut replacement = values.clone();
                replacement.insert(index, value);
                commands.push(PrincipalStorageCommand::ArrayReplace {
                    key: key.clone(),
                    values: encode_storage_util_list_values(kind, &replacement)?,
                });
                StorageUtilListResult::Bool(true)
            }
        }
        StorageUtilListCall::Remove {
            value,
            all_instances,
        } => {
            encode_storage_util_list_value(kind, &value)?;
            let mut replacement = values.clone();
            let removed = if all_instances {
                let previous_len = replacement.len();
                replacement.retain(|candidate| candidate != &value);
                previous_len - replacement.len()
            } else if let Some(index) = replacement.iter().position(|candidate| candidate == &value)
            {
                replacement.remove(index);
                1
            } else {
                0
            };
            if removed > 0 {
                commands.push(PrincipalStorageCommand::ArrayReplace {
                    key: key.clone(),
                    values: encode_storage_util_list_values(kind, &replacement)?,
                });
            }
            StorageUtilListResult::Int(
                i32::try_from(removed).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
            )
        }
        StorageUtilListCall::CountValue { value, exclude } => {
            encode_storage_util_list_value(kind, &value)?;
            let count = values
                .iter()
                .filter(|candidate| (*candidate == &value) != exclude)
                .count();
            StorageUtilListResult::Int(
                i32::try_from(count).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
            )
        }
        StorageUtilListCall::Adjust { index, amount } => {
            encode_storage_util_list_value(kind, &amount)?;
            let Some((index, current)) = usize::try_from(index)
                .ok()
                .and_then(|index| values.get(index).cloned().map(|value| (index, value)))
            else {
                return Ok(StorageUtilListAdaptation {
                    key,
                    result: StorageUtilListResult::Value(default_storage_util_list_value(kind)),
                    commands,
                });
            };
            let next = match (current, amount) {
                (StorageUtilListValue::Int(current), StorageUtilListValue::Int(amount)) => {
                    StorageUtilListValue::Int(
                        current
                            .checked_add(amount)
                            .ok_or(StorageUtilAdapterError::IntegerOverflow)?,
                    )
                }
                (StorageUtilListValue::Float(current), StorageUtilListValue::Float(amount)) => {
                    let next = current + amount;
                    validate_storage_util_float(next)?;
                    StorageUtilListValue::Float(next)
                }
                _ => return Err(StorageUtilAdapterError::TypeMismatch),
            };
            commands.push(PrincipalStorageCommand::ArraySet {
                key: key.clone(),
                index: u32::try_from(index)
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
                value: encode_storage_util_list_value(kind, &next)?,
            });
            StorageUtilListResult::Value(next)
        }
        StorageUtilListCall::Find { value } => {
            encode_storage_util_list_value(kind, &value)?;
            let index = values
                .iter()
                .position(|candidate| candidate == &value)
                .map_or(Ok(-1), |index| {
                    i32::try_from(index).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)
                })?;
            StorageUtilListResult::Int(index)
        }
        StorageUtilListCall::Has { value } => {
            encode_storage_util_list_value(kind, &value)?;
            StorageUtilListResult::Bool(values.contains(&value))
        }
    };
    Ok(StorageUtilListAdaptation {
        key,
        result,
        commands,
    })
}

fn decode_storage_util_list(
    kind: StorageUtilListKind,
    current: Option<&PrincipalStorageValue>,
) -> Result<Vec<StorageUtilListValue>, StorageUtilAdapterError> {
    let Some(current) = current else {
        return Ok(Vec::new());
    };
    let PrincipalStorageValue::Array(values) = current else {
        return Err(StorageUtilAdapterError::TypeMismatch);
    };
    values
        .iter()
        .map(|value| decode_storage_util_list_value(kind, value))
        .collect()
}

fn encode_storage_util_list_value(
    kind: StorageUtilListKind,
    value: &StorageUtilListValue,
) -> Result<ExtensionValue, StorageUtilAdapterError> {
    match (kind, value) {
        (StorageUtilListKind::Int, StorageUtilListValue::Int(value)) => {
            Ok(ExtensionValue::I64(i64::from(*value)))
        }
        (StorageUtilListKind::Float, StorageUtilListValue::Float(value)) => {
            validate_storage_util_float(*value)?;
            Ok(ExtensionValue::Bytes(
                value.to_bits().to_le_bytes().to_vec(),
            ))
        }
        (StorageUtilListKind::String, StorageUtilListValue::String(value)) => {
            Ok(ExtensionValue::String(value.clone()))
        }
        (StorageUtilListKind::Form, StorageUtilListValue::Form(None)) => {
            Ok(ExtensionValue::Bytes(Vec::new()))
        }
        (StorageUtilListKind::Form, StorageUtilListValue::Form(Some(value))) => {
            Ok(ExtensionValue::Bytes(encode_storage_util_form(*value)))
        }
        _ => Err(StorageUtilAdapterError::TypeMismatch),
    }
}

fn encode_storage_util_list_values(
    kind: StorageUtilListKind,
    values: &[StorageUtilListValue],
) -> Result<Vec<ExtensionValue>, StorageUtilAdapterError> {
    values
        .iter()
        .map(|value| encode_storage_util_list_value(kind, value))
        .collect()
}

fn decode_storage_util_list_value(
    kind: StorageUtilListKind,
    value: &ExtensionValue,
) -> Result<StorageUtilListValue, StorageUtilAdapterError> {
    match (kind, value) {
        (StorageUtilListKind::Int, ExtensionValue::I64(value)) => Ok(StorageUtilListValue::Int(
            i32::try_from(*value).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
        )),
        (StorageUtilListKind::Float, ExtensionValue::Bytes(encoded)) => {
            let encoded: [u8; 4] = encoded
                .as_slice()
                .try_into()
                .map_err(|_| StorageUtilAdapterError::TypeMismatch)?;
            let value = f32::from_bits(u32::from_le_bytes(encoded));
            validate_storage_util_float(value)?;
            Ok(StorageUtilListValue::Float(value))
        }
        (StorageUtilListKind::String, ExtensionValue::String(value)) => {
            Ok(StorageUtilListValue::String(value.clone()))
        }
        (StorageUtilListKind::Form, ExtensionValue::Bytes(encoded)) if encoded.is_empty() => {
            Ok(StorageUtilListValue::Form(None))
        }
        (StorageUtilListKind::Form, ExtensionValue::Bytes(encoded)) if encoded.len() == 20 => {
            let mut source = [0_u8; 16];
            source.copy_from_slice(&encoded[..16]);
            let local = u32::from_le_bytes(
                encoded[16..]
                    .try_into()
                    .map_err(|_| StorageUtilAdapterError::TypeMismatch)?,
            );
            Ok(StorageUtilListValue::Form(Some(FormRef::new(
                source, local,
            ))))
        }
        _ => Err(StorageUtilAdapterError::TypeMismatch),
    }
}

fn default_storage_util_list_value(kind: StorageUtilListKind) -> StorageUtilListValue {
    match kind {
        StorageUtilListKind::Int => StorageUtilListValue::Int(0),
        StorageUtilListKind::Float => StorageUtilListValue::Float(0.0),
        StorageUtilListKind::String => StorageUtilListValue::String(String::new()),
        StorageUtilListKind::Form => StorageUtilListValue::Form(None),
    }
}

/// Classify a static Papyrus call by provider type and function name.
/// Returns `None` for providers that are not known extender APIs.
pub fn classify_static_call(provider: &str, function: &str) -> Option<CompatibilityMatch> {
    if provider.eq_ignore_ascii_case("Game")
        && matches_ignore_ascii_case(
            function,
            &[
                "GetModCount",
                "GetModByName",
                "GetModName",
                "GetModDependencyCount",
                "IsPluginInstalled",
                "GetLightModCount",
                "GetLightModByName",
                "GetLightModName",
                "GetLightModDependencyCount",
                "GetNthLightModDependency",
            ],
        )
    {
        return Some(native(
            ExtenderFamily::Skse,
            CONTENT_CATALOG_SERVICE,
            "executed by the engine content catalog with exact regular/light index semantics",
        ));
    }
    if provider.eq_ignore_ascii_case("SKSE") {
        return Some(if is_extender_version_probe(function) {
            mapped(
                ExtenderFamily::Skse,
                CONTEXT_SERVICE,
                "replace extender-version probes with SDK/service feature discovery",
            )
        } else {
            unsupported(
                ExtenderFamily::Skse,
                "the SKSE provider is recognized, but this function has no engine semantic mapping",
            )
        });
    }
    if provider.eq_ignore_ascii_case("F4SE") {
        return Some(if is_extender_version_probe(function) {
            mapped(
                ExtenderFamily::F4se,
                CONTEXT_SERVICE,
                "replace extender-version probes with SDK/service feature discovery",
            )
        } else {
            unsupported(
                ExtenderFamily::F4se,
                "the F4SE provider is recognized, but this function has no engine semantic mapping",
            )
        });
    }
    if provider.eq_ignore_ascii_case("StorageUtil") {
        return Some(match source_alias(provider, function) {
            Some(_) => native(
                ExtenderFamily::PapyrusUtil,
                PRINCIPAL_STORAGE_SERVICE,
                "an exact engine-owned global-value adapter targets byro.storage; ObjKey must be None",
            ),
            None => unsupported(
                ExtenderFamily::PapyrusUtil,
                "this StorageUtil function has no exact engine alias; object-scoped values require extension components, and pluck/file/list/cross-principal semantics remain unavailable",
            ),
        });
    }
    if provider.eq_ignore_ascii_case("JsonUtil") {
        return Some(unsupported(
            ExtenderFamily::PapyrusUtil,
            "arbitrary host-file JSON access is outside the sandbox; use principal storage or packaged content",
        ));
    }
    if matches_ignore_ascii_case(
        provider,
        &["JValue", "JMap", "JArray", "JFormMap", "JIntMap", "JDB"],
    ) {
        return Some(if source_alias(provider, function).is_some() {
            mapped(
                ExtenderFamily::JContainers,
                LEGACY_CONTAINERS_SERVICE,
                "an exact core typed-container alias targets the persisted principal-local object service",
            )
        } else {
            unsupported(
                ExtenderFamily::JContainers,
                "the JContainers provider is recognized, but this function has no exact engine adapter; file JSON, Lua, path solving, form/int-keyed maps, and global cross-mod databases remain unavailable",
            )
        });
    }
    if provider.eq_ignore_ascii_case("ModEvent") {
        return Some(if source_alias(provider, function).is_some() {
            native(
                ExtenderFamily::Skse,
                EVENT_SERVICE,
                "an exact engine-owned ModEvent handle adapter targets the bounded shared compatibility bus",
            )
        } else {
            unsupported(
                ExtenderFamily::Skse,
                "the ModEvent provider is recognized, but this function has no exact engine mapping",
            )
        });
    }
    if provider.eq_ignore_ascii_case("Input") {
        return Some(
            if matches_ignore_ascii_case(
                function,
                &[
                    "GetMappedKey",
                    "IsKeyPressed",
                    "TapKey",
                    "HoldKey",
                    "ReleaseKey",
                ],
            ) {
                unsupported(
                ExtenderFamily::Shared,
                "physical-key polling/injection is not exposed; subscribe to normalized manifest input actions",
            )
            } else {
                unsupported(
                ExtenderFamily::Shared,
                "the Input provider is recognized, but this function has no engine semantic mapping",
            )
            },
        );
    }
    if provider.eq_ignore_ascii_case("UI") {
        return Some(unsupported(
            ExtenderFamily::Shared,
            "arbitrary Scaleform object access is not exposed; use the future isolated UI contribution service",
        ));
    }
    None
}

/// Classify extender-added instance functions whose declaring type is erased
/// from a `callmethod` instruction. Exact names are used to avoid treating
/// ordinary mod methods as compatibility calls.
pub fn classify_method_call(function: &str) -> Option<CompatibilityMatch> {
    if method_source_alias(function).is_some() {
        return Some(mapped(
            ExtenderFamily::Skse,
            EVENT_SERVICE,
            "an exact source adapter targets the shared engine ModEvent compatibility bus",
        ));
    }
    if matches_ignore_ascii_case(
        function,
        &["RegisterForKey", "UnregisterForKey", "UnregisterForAllKeys"],
    ) {
        return Some(mapped(
            ExtenderFamily::Shared,
            EVENT_SERVICE,
            "replace physical key registration with normalized manifest input-action subscriptions",
        ));
    }
    if matches_ignore_ascii_case(function, &["RegisterForMenu", "UnregisterForMenu"]) {
        return Some(unsupported(
            ExtenderFamily::Shared,
            "menu lifecycle aliases await the isolated UI contribution service",
        ));
    }
    None
}

const fn mapped(
    family: ExtenderFamily,
    service: &'static str,
    guidance: &'static str,
) -> CompatibilityMatch {
    CompatibilityMatch {
        family,
        disposition: CompatibilityDisposition::Mapped,
        service: Some(service),
        guidance,
    }
}

const fn native(
    family: ExtenderFamily,
    service: &'static str,
    guidance: &'static str,
) -> CompatibilityMatch {
    CompatibilityMatch {
        family,
        disposition: CompatibilityDisposition::Native,
        service: Some(service),
        guidance,
    }
}

const fn unsupported(family: ExtenderFamily, guidance: &'static str) -> CompatibilityMatch {
    CompatibilityMatch {
        family,
        disposition: CompatibilityDisposition::Unsupported,
        service: None,
        guidance,
    }
}

fn matches_ignore_ascii_case(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn is_extender_version_probe(function: &str) -> bool {
    matches_ignore_ascii_case(
        function,
        &[
            "GetVersion",
            "GetVersionMinor",
            "GetVersionBeta",
            "GetVersionRelease",
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{PluginInfo, PluginKind};

    fn classic_catalog(names: &[&str]) -> ContentCatalog {
        ContentCatalog::new(
            names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    PluginInfo::new(
                        *name,
                        (index as u128 + 1).to_be_bytes(),
                        PluginKind::Regular,
                    )
                    .unwrap()
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn storage_and_events_map_to_existing_semantic_services() {
        let storage = classify_static_call("storageutil", "GetIntValue").unwrap();
        assert_eq!(storage.disposition, CompatibilityDisposition::Native);
        assert_eq!(storage.service, Some(PRINCIPAL_STORAGE_SERVICE));
        let event = classify_method_call("RegisterForModEvent").unwrap();
        assert_eq!(event.service, Some(EVENT_SERVICE));
    }

    #[test]
    fn legacy_obscript_version_probes_map_to_context_discovery() {
        let nvse = classify_obscript_command("getnvseversion").unwrap();
        assert_eq!(nvse.family, ExtenderFamily::Xnvse);
        assert_eq!(nvse.service, Some(CONTEXT_SERVICE));
        assert_eq!(nvse.disposition, CompatibilityDisposition::Mapped);
        let obse = classify_obscript_command("GetOBSERevision").unwrap();
        assert_eq!(obse.family, ExtenderFamily::Obse);
        assert_eq!(obse.service, Some(CONTEXT_SERVICE));
        assert_eq!(
            classify_obscript_command("GetNVSEUnknown")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Unsupported
        );
        assert!(classify_obscript_command("GetActorValue").is_none());
    }

    #[test]
    fn legacy_load_order_commands_map_to_content_catalog_recipes() {
        let loaded = obscript_source_alias("ismodloaded").unwrap();
        assert_eq!(loaded.service, CONTENT_CATALOG_SERVICE);
        assert_eq!(loaded.operation, "content.find");
        assert_eq!(loaded.value_kind, "bool");

        let index = obscript_source_alias("GetModIndex").unwrap();
        assert_eq!(index.operation, "content.find-index");
        assert!(index.constraint.contains("255 sentinel"));

        assert_eq!(
            obscript_source_alias("GetNumLoadedMods").unwrap().operation,
            "content.count"
        );
        assert_eq!(
            obscript_source_alias("GetNumLoadedPlugins")
                .unwrap()
                .operation,
            "content.count"
        );
        assert_eq!(
            obscript_source_alias("GetNthModName").unwrap().operation,
            "content.plugin-name"
        );
        assert_eq!(
            classify_obscript_command("IsModLoaded").unwrap().service,
            Some(CONTENT_CATALOG_SERVICE)
        );
        assert_eq!(
            classify_obscript_command("IsModLoaded")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Native
        );
        assert!(obscript_source_alias("GetSourceModIndex").is_none());
    }

    #[test]
    fn papyrus_game_content_aliases_preserve_regular_and_light_indices() {
        let catalog = ContentCatalog::new_with_dependencies(
            vec![
                PluginInfo::new("Skyrim.esm", 1_u128.to_be_bytes(), PluginKind::Regular).unwrap(),
                PluginInfo::new("Update.esm", 3_u128.to_be_bytes(), PluginKind::Regular).unwrap(),
                PluginInfo::new("Patch.esl", 2_u128.to_be_bytes(), PluginKind::Light).unwrap(),
            ],
            vec![vec![], vec![0], vec![1]],
        )
        .unwrap();

        assert_eq!(
            adapt_papyrus_game_get_mod_by_name(&catalog, "UPDATE.ESM"),
            1
        );
        assert_eq!(
            adapt_papyrus_game_get_mod_by_name(&catalog, "Patch.esl"),
            0x100
        );
        assert_eq!(
            adapt_papyrus_game_get_mod_by_name(&catalog, "Missing.esp"),
            255
        );
        assert_eq!(adapt_papyrus_game_get_mod_count(&catalog), 2);
        assert_eq!(adapt_papyrus_game_get_mod_name(&catalog, 1), "Update.esm");
        assert_eq!(
            adapt_papyrus_game_get_mod_name(&catalog, 0x100),
            "Patch.esl"
        );
        assert_eq!(adapt_papyrus_game_get_mod_name(&catalog, 255), "");
        assert_eq!(adapt_papyrus_game_get_mod_dependency_count(&catalog, 1), 1);
        assert_eq!(
            adapt_papyrus_game_get_mod_dependency_count(&catalog, 0x100),
            1
        );
        assert_eq!(adapt_papyrus_game_get_mod_dependency_count(&catalog, -1), 0);
        assert!(adapt_papyrus_game_is_plugin_installed(
            &catalog,
            "patch.ESL"
        ));
        assert_eq!(adapt_papyrus_game_get_light_mod_count(&catalog), 1);
        assert_eq!(
            adapt_papyrus_game_get_light_mod_by_name(&catalog, "Patch.esl"),
            0
        );
        assert_eq!(
            adapt_papyrus_game_get_light_mod_by_name(&catalog, "Skyrim.esm"),
            0xffff
        );
        assert_eq!(
            adapt_papyrus_game_get_light_mod_name(&catalog, 0),
            "Patch.esl"
        );
        assert_eq!(adapt_papyrus_game_get_light_mod_name(&catalog, -1), "");
        assert_eq!(
            adapt_papyrus_game_get_light_mod_dependency_count(&catalog, 0),
            1
        );
        assert_eq!(
            adapt_papyrus_game_get_light_mod_dependency_count(&catalog, 1),
            0
        );
        assert_eq!(
            adapt_papyrus_game_get_nth_light_mod_dependency(&catalog, 0, 0),
            1
        );
        assert_eq!(
            adapt_papyrus_game_get_nth_light_mod_dependency(&catalog, 0, 1),
            0
        );
        assert_eq!(
            adapt_papyrus_game_get_nth_light_mod_dependency(&catalog, -1, 0),
            0
        );
        let declarations = papyrus_game_content_declarations();
        assert_eq!(declarations.len(), 10);
        for declaration in declarations {
            declaration.declaration.validate().unwrap();
        }
        assert_eq!(
            classify_static_call("game", "getmodbyname")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Native
        );
        assert!(classify_static_call("Game", "GetPlayer").is_none());
    }

    #[test]
    fn legacy_load_order_adapter_preserves_classic_results() {
        let catalog = classic_catalog(&["FalloutNV.esm", "Companion.esp"]);
        assert_eq!(
            adapt_legacy_obscript_load_order(
                &catalog,
                LegacyObscriptLoadOrderCall::IsModLoaded {
                    plugin: "companion.ESP".to_owned(),
                },
            ),
            Ok(LegacyObscriptLoadOrderResult::Bool(true))
        );
        assert_eq!(
            adapt_legacy_obscript_load_order(
                &catalog,
                LegacyObscriptLoadOrderCall::GetModIndex {
                    plugin: "missing.esp".to_owned(),
                },
            ),
            Ok(LegacyObscriptLoadOrderResult::Integer(255))
        );
        assert_eq!(
            adapt_legacy_obscript_load_order(
                &catalog,
                LegacyObscriptLoadOrderCall::GetNthModName { index: 1 },
            ),
            Ok(LegacyObscriptLoadOrderResult::String(
                "Companion.esp".to_owned()
            ))
        );
        assert_eq!(
            adapt_legacy_obscript_load_order(
                &catalog,
                LegacyObscriptLoadOrderCall::GetNthModName { index: -1 },
            ),
            Ok(LegacyObscriptLoadOrderResult::String(String::new()))
        );
    }

    #[test]
    fn legacy_load_order_adapter_rejects_unrepresentable_catalogs() {
        let names = (0..=LEGACY_OBSCRIPT_PLUGIN_LIMIT)
            .map(|index| format!("Plugin{index}.esp"))
            .collect::<Vec<_>>();
        let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
        let catalog = classic_catalog(&refs);
        assert_eq!(
            adapt_legacy_obscript_load_order(
                &catalog,
                LegacyObscriptLoadOrderCall::GetNumLoadedMods,
            ),
            Err(LegacyObscriptLoadOrderError::PluginBudgetExceeded {
                actual: 256,
                maximum: 255,
            })
        );
    }

    #[test]
    fn storage_aliases_are_exact_global_scalar_operations() {
        let get = source_alias("storageutil", "getintvalue").unwrap();
        assert_eq!(get.service, PRINCIPAL_STORAGE_SERVICE);
        assert_eq!(get.operation, "storage.get");
        assert_eq!(get.value_kind, "signed");
        assert!(get.constraint.contains("ObjKey must be None"));

        let string = source_alias("StorageUtil", "HasStringValue").unwrap();
        assert_eq!(string.operation, "storage.get");
        assert_eq!(string.value_kind, "text");
        let set = source_alias("StorageUtil", "SetStringValue").unwrap();
        assert_eq!(set.operation, "storage.queue-set/delete");
        let unset = source_alias("StorageUtil", "UnsetIntValue").unwrap();
        assert_eq!(unset.operation, "storage.get+queue-delete");
        assert_eq!(
            source_alias("StorageUtil", "AdjustIntValue")
                .unwrap()
                .operation,
            "storage.get+queue-set/delete"
        );
        assert_eq!(
            source_alias("StorageUtil", "GetFloatValue")
                .unwrap()
                .value_kind,
            "float"
        );
        assert_eq!(
            source_alias("StorageUtil", "SetFormValue")
                .unwrap()
                .value_kind,
            "form"
        );
        assert_eq!(
            source_alias("StorageUtil", "PluckStringValue")
                .unwrap()
                .operation,
            "storage.get+queue-delete"
        );
        assert_eq!(
            source_alias("StorageUtil", "FormListAdd")
                .unwrap()
                .operation,
            "storage.array-get+queue-push"
        );
        assert_eq!(
            source_alias("StorageUtil", "FormListSet")
                .unwrap()
                .operation,
            "storage.array-get+queue-set"
        );
        assert_eq!(
            source_alias("StorageUtil", "FormListInsert")
                .unwrap()
                .operation,
            "storage.array-get+queue-replace"
        );
        assert!(source_alias("StorageUtil", "FormListSort").is_none());
        assert_eq!(
            classify_static_call("StorageUtil", "GetFloatValue")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Native
        );
        let declarations = papyrus_storage_util_declarations();
        assert_eq!(declarations.len(), 80);
        assert!(declarations
            .iter()
            .all(|function| function.declaration.validate().is_ok()));
    }

    #[test]
    fn jcontainers_aliases_only_claim_the_executable_core_surface() {
        let create = source_alias("jarray", "OBJECT").unwrap();
        assert_eq!(create.service, LEGACY_CONTAINERS_SERVICE);
        assert_eq!(create.operation, "legacy-containers.array-create");
        let nested = source_alias("JMap", "setObj").unwrap();
        assert_eq!(nested.operation, "legacy-containers.map-set");
        assert_eq!(nested.value_kind, "handle");
        assert_eq!(
            classify_static_call("JArray", "getForm")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Mapped
        );
        assert!(source_alias("JDB", "solveObj").is_none());
        assert_eq!(
            classify_static_call("JDB", "solveObj").unwrap().disposition,
            CompatibilityDisposition::Unsupported
        );
        assert_eq!(
            classify_static_call("JArray", "writeToFile")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Unsupported
        );
        let declarations = papyrus_legacy_container_declarations();
        assert_eq!(declarations.len(), 46);
        assert!(declarations
            .iter()
            .all(|function| function.declaration.validate().is_ok()));
        let release =
            declarations
                .iter()
                .find(|function| {
                    function.declaration.papyrus.as_ref().is_some_and(|alias| {
                        alias.provider == "JValue" && alias.function == "release"
                    })
                })
                .unwrap();
        assert_eq!(
            release.declaration.result,
            Some(ScriptResultDeclaration {
                value_type: ScriptValueType::Integer,
                optional: false,
            })
        );
        let retain = source_alias("JValue", "retain").unwrap();
        assert_eq!(retain.operation, "legacy-containers.retain");
        let release_tagged = source_alias("JValue", "releaseObjectsWithTag").unwrap();
        assert_eq!(
            release_tagged.operation,
            "legacy-containers.release-objects-with-tag"
        );
    }

    #[test]
    fn storage_util_adapter_preserves_scalar_return_and_delete_contracts() {
        let set = adapt_storage_util_global_scalar(
            "MyMod.Score",
            StorageUtilScalarCall::SetInt { value: 12 },
            None,
        )
        .unwrap();
        assert_eq!(set.key.as_str(), "storageutil.int:mymod.score");
        assert_eq!(set.result, StorageUtilScalarResult::Int(12));
        assert_eq!(
            set.command,
            Some(PrincipalStorageCommand::Set {
                key: set.key.clone(),
                value: ExtensionValue::I64(12),
            })
        );

        let zero = adapt_storage_util_global_scalar(
            "MYMOD.SCORE",
            StorageUtilScalarCall::SetInt { value: 0 },
            Some(&PrincipalStorageValue::I64(12)),
        )
        .unwrap();
        assert_eq!(zero.key, set.key);
        assert!(matches!(
            zero.command,
            Some(PrincipalStorageCommand::Delete { .. })
        ));

        let unset = adapt_storage_util_global_scalar(
            "MyMod.Name",
            StorageUtilScalarCall::UnsetString,
            Some(&PrincipalStorageValue::String("Dragonborn".to_owned())),
        )
        .unwrap();
        assert_eq!(unset.result, StorageUtilScalarResult::Bool(true));
        assert!(matches!(
            unset.command,
            Some(PrincipalStorageCommand::Delete { .. })
        ));
    }

    #[test]
    fn storage_util_adapter_type_isolates_keys_and_honors_missing_values() {
        let get_int = adapt_storage_util_global_scalar(
            "SharedKey",
            StorageUtilScalarCall::GetInt { missing: 7 },
            None,
        )
        .unwrap();
        let get_string = adapt_storage_util_global_scalar(
            "sharedkey",
            StorageUtilScalarCall::GetString {
                missing: "fallback".to_owned(),
            },
            None,
        )
        .unwrap();
        assert_ne!(get_int.key, get_string.key);
        assert_eq!(get_int.result, StorageUtilScalarResult::Int(7));
        assert_eq!(
            get_string.result,
            StorageUtilScalarResult::String("fallback".to_owned())
        );
        assert!(get_int.command.is_none());
        assert!(get_string.command.is_none());
    }

    #[test]
    fn storage_util_adapter_round_trips_float_form_and_numeric_adjustments() {
        let float = adapt_storage_util_global_scalar(
            "SharedKey",
            StorageUtilScalarCall::SetFloat { value: 1.25 },
            None,
        )
        .unwrap();
        assert_eq!(float.key.as_str(), "storageutil.float:sharedkey");
        let Some(PrincipalStorageCommand::Set {
            value: ExtensionValue::Bytes(encoded),
            ..
        }) = float.command
        else {
            panic!("float set must encode a bounded byte value");
        };
        let adjusted = adapt_storage_util_global_scalar(
            "sharedkey",
            StorageUtilScalarCall::AdjustFloat { amount: 0.5 },
            Some(&PrincipalStorageValue::Bytes(encoded)),
        )
        .unwrap();
        assert_eq!(adjusted.result, StorageUtilScalarResult::Float(1.75));

        let adjusted_int = adapt_storage_util_global_scalar(
            "new-count",
            StorageUtilScalarCall::AdjustInt { amount: 4 },
            None,
        )
        .unwrap();
        assert_eq!(adjusted_int.result, StorageUtilScalarResult::Int(4));

        let form = FormRef::new([0x2a; 16], 0x1234_5678);
        let set_form = adapt_storage_util_global_scalar(
            "SharedKey",
            StorageUtilScalarCall::SetForm { value: Some(form) },
            None,
        )
        .unwrap();
        assert_eq!(set_form.key.as_str(), "storageutil.form:sharedkey");
        let Some(PrincipalStorageCommand::Set {
            value: ExtensionValue::Bytes(encoded),
            ..
        }) = set_form.command
        else {
            panic!("form set must encode a bounded byte value");
        };
        assert_eq!(encoded.len(), 20);
        assert_eq!(
            adapt_storage_util_global_scalar(
                "sharedkey",
                StorageUtilScalarCall::GetForm { missing: None },
                Some(&PrincipalStorageValue::Bytes(encoded)),
            )
            .unwrap()
            .result,
            StorageUtilScalarResult::Form(Some(form))
        );
        assert_ne!(float.key, set_form.key);
    }

    #[test]
    fn storage_util_adapter_plucks_each_scalar_type_and_deletes_missing_keys() {
        let int = adapt_storage_util_global_scalar(
            "count",
            StorageUtilScalarCall::PluckInt { missing: -1 },
            Some(&PrincipalStorageValue::I64(9)),
        )
        .unwrap();
        assert_eq!(int.result, StorageUtilScalarResult::Int(9));
        assert!(matches!(
            int.command,
            Some(PrincipalStorageCommand::Delete { .. })
        ));

        let float = adapt_storage_util_global_scalar(
            "ratio",
            StorageUtilScalarCall::PluckFloat { missing: -1.0 },
            Some(&PrincipalStorageValue::Bytes(
                2.5_f32.to_bits().to_le_bytes().to_vec(),
            )),
        )
        .unwrap();
        assert_eq!(float.result, StorageUtilScalarResult::Float(2.5));

        let string = adapt_storage_util_global_scalar(
            "status",
            StorageUtilScalarCall::PluckString {
                missing: "missing".to_owned(),
            },
            None,
        )
        .unwrap();
        assert_eq!(
            string.result,
            StorageUtilScalarResult::String("missing".to_owned())
        );
        assert!(matches!(
            string.command,
            Some(PrincipalStorageCommand::Delete { .. })
        ));

        let form = FormRef::new([0x17; 16], 0x42);
        let plucked_form = adapt_storage_util_global_scalar(
            "owner",
            StorageUtilScalarCall::PluckForm { missing: None },
            Some(&PrincipalStorageValue::Bytes(encode_storage_util_form(
                form,
            ))),
        )
        .unwrap();
        assert_eq!(
            plucked_form.result,
            StorageUtilScalarResult::Form(Some(form))
        );
    }

    #[test]
    fn storage_util_list_adapter_is_typed_bounded_and_preserves_legacy_results() {
        let add = adapt_storage_util_global_list(
            "Recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::Add {
                value: StorageUtilListValue::Int(4),
                allow_duplicate: true,
            },
            None,
            4,
        )
        .unwrap();
        assert_eq!(add.key.as_str(), "storageutil.list.int:recent");
        assert_eq!(add.result, StorageUtilListResult::Int(0));
        assert_eq!(
            add.commands,
            [PrincipalStorageCommand::ArrayPush {
                key: add.key.clone(),
                value: ExtensionValue::I64(4),
            }]
        );

        let current = PrincipalStorageValue::Array(vec![ExtensionValue::I64(4)]);
        let duplicate = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::Add {
                value: StorageUtilListValue::Int(4),
                allow_duplicate: false,
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(duplicate.result, StorageUtilListResult::Int(-1));
        assert!(duplicate.commands.is_empty());
        assert_eq!(
            adapt_storage_util_global_list(
                "recent",
                StorageUtilListKind::Int,
                StorageUtilListCall::Find {
                    value: StorageUtilListValue::Int(4),
                },
                Some(&current),
                4,
            )
            .unwrap()
            .result,
            StorageUtilListResult::Int(0)
        );
        assert_eq!(
            adapt_storage_util_global_list(
                "recent",
                StorageUtilListKind::Int,
                StorageUtilListCall::Get { index: -1 },
                Some(&current),
                4,
            )
            .unwrap()
            .result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(0))
        );

        let none_form = PrincipalStorageValue::Array(vec![ExtensionValue::Bytes(Vec::new())]);
        assert_eq!(
            adapt_storage_util_global_list(
                "owners",
                StorageUtilListKind::Form,
                StorageUtilListCall::Get { index: 0 },
                Some(&none_form),
                4,
            )
            .unwrap()
            .result,
            StorageUtilListResult::Value(StorageUtilListValue::Form(None))
        );

        assert_eq!(
            adapt_storage_util_global_list(
                "ratios",
                StorageUtilListKind::Float,
                StorageUtilListCall::Count,
                Some(&PrincipalStorageValue::Array(vec![ExtensionValue::Bytes(
                    vec![0; 3],
                )])),
                4,
            ),
            Err(StorageUtilAdapterError::TypeMismatch)
        );
    }

    #[test]
    fn storage_util_list_mutations_return_previous_values_and_queue_exact_edits() {
        let current =
            PrincipalStorageValue::Array(vec![ExtensionValue::I64(4), ExtensionValue::I64(9)]);
        let set = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::Set {
                index: 1,
                value: StorageUtilListValue::Int(7),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            set.result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(9))
        );
        assert_eq!(
            set.commands,
            [PrincipalStorageCommand::ArraySet {
                key: set.key.clone(),
                index: 1,
                value: ExtensionValue::I64(7),
            }]
        );

        let pluck = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::Pluck {
                index: 0,
                missing: StorageUtilListValue::Int(-1),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            pluck.result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(4))
        );
        assert_eq!(
            pluck.commands,
            [PrincipalStorageCommand::ArrayRemove {
                key: pluck.key.clone(),
                index: 0,
            }]
        );

        let missing = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::Pluck {
                index: -1,
                missing: StorageUtilListValue::Int(-1),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            missing.result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(-1))
        );
        assert!(missing.commands.is_empty());

        for (call, expected, index) in [
            (StorageUtilListCall::Shift, 4, 0),
            (StorageUtilListCall::Pop, 9, 1),
        ] {
            let adapted = adapt_storage_util_global_list(
                "recent",
                StorageUtilListKind::Int,
                call,
                Some(&current),
                4,
            )
            .unwrap();
            assert_eq!(
                adapted.result,
                StorageUtilListResult::Value(StorageUtilListValue::Int(expected))
            );
            assert_eq!(
                adapted.commands,
                [PrincipalStorageCommand::ArrayRemove {
                    key: adapted.key.clone(),
                    index,
                }]
            );
        }

        let removed = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::RemoveAt { index: 1 },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(removed.result, StorageUtilListResult::Bool(true));
        assert_eq!(
            removed.commands,
            [PrincipalStorageCommand::ArrayRemove {
                key: removed.key.clone(),
                index: 1,
            }]
        );

        let not_removed = adapt_storage_util_global_list(
            "recent",
            StorageUtilListKind::Int,
            StorageUtilListCall::RemoveAt { index: 2 },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(not_removed.result, StorageUtilListResult::Bool(false));
        assert!(not_removed.commands.is_empty());
    }

    #[test]
    fn storage_util_list_value_mutations_are_atomic_bounded_and_exact() {
        let current = PrincipalStorageValue::Array(vec![
            ExtensionValue::I64(2),
            ExtensionValue::I64(4),
            ExtensionValue::I64(2),
        ]);
        let insert = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Insert {
                index: 1,
                value: StorageUtilListValue::Int(3),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(insert.result, StorageUtilListResult::Bool(true));
        assert_eq!(
            insert.commands,
            [PrincipalStorageCommand::ArrayReplace {
                key: insert.key.clone(),
                values: vec![
                    ExtensionValue::I64(2),
                    ExtensionValue::I64(3),
                    ExtensionValue::I64(4),
                    ExtensionValue::I64(2),
                ],
            }]
        );

        let full = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Insert {
                index: 3,
                value: StorageUtilListValue::Int(5),
            },
            Some(&current),
            3,
        )
        .unwrap();
        assert_eq!(full.result, StorageUtilListResult::Bool(false));
        assert!(full.commands.is_empty());

        for (all_instances, expected_count, expected_values) in [
            (
                false,
                1,
                vec![ExtensionValue::I64(4), ExtensionValue::I64(2)],
            ),
            (true, 2, vec![ExtensionValue::I64(4)]),
        ] {
            let remove = adapt_storage_util_global_list(
                "numbers",
                StorageUtilListKind::Int,
                StorageUtilListCall::Remove {
                    value: StorageUtilListValue::Int(2),
                    all_instances,
                },
                Some(&current),
                4,
            )
            .unwrap();
            assert_eq!(remove.result, StorageUtilListResult::Int(expected_count));
            assert_eq!(
                remove.commands,
                [PrincipalStorageCommand::ArrayReplace {
                    key: remove.key.clone(),
                    values: expected_values,
                }]
            );
        }

        for (exclude, expected) in [(false, 2), (true, 1)] {
            assert_eq!(
                adapt_storage_util_global_list(
                    "numbers",
                    StorageUtilListKind::Int,
                    StorageUtilListCall::CountValue {
                        value: StorageUtilListValue::Int(2),
                        exclude,
                    },
                    Some(&current),
                    4,
                )
                .unwrap()
                .result,
                StorageUtilListResult::Int(expected)
            );
        }

        let adjust = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Adjust {
                index: 1,
                amount: StorageUtilListValue::Int(3),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            adjust.result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(7))
        );
        assert_eq!(
            adjust.commands,
            [PrincipalStorageCommand::ArraySet {
                key: adjust.key.clone(),
                index: 1,
                value: ExtensionValue::I64(7),
            }]
        );

        let missing = adapt_storage_util_global_list(
            "numbers",
            StorageUtilListKind::Int,
            StorageUtilListCall::Adjust {
                index: -1,
                amount: StorageUtilListValue::Int(3),
            },
            Some(&current),
            4,
        )
        .unwrap();
        assert_eq!(
            missing.result,
            StorageUtilListResult::Value(StorageUtilListValue::Int(0))
        );
        assert!(missing.commands.is_empty());

        let float = adapt_storage_util_global_list(
            "ratios",
            StorageUtilListKind::Float,
            StorageUtilListCall::Adjust {
                index: 0,
                amount: StorageUtilListValue::Float(0.5),
            },
            Some(&PrincipalStorageValue::Array(vec![ExtensionValue::Bytes(
                1.5_f32.to_bits().to_le_bytes().to_vec(),
            )])),
            4,
        )
        .unwrap();
        assert_eq!(
            float.result,
            StorageUtilListResult::Value(StorageUtilListValue::Float(2.0))
        );
    }

    #[test]
    fn storage_util_adapter_rejects_unrepresentable_or_corrupt_values() {
        assert!(matches!(
            adapt_storage_util_global_scalar(
                "contains spaces",
                StorageUtilScalarCall::HasInt,
                None,
            ),
            Err(StorageUtilAdapterError::InvalidKey(_))
        ));
        assert_eq!(
            adapt_storage_util_global_scalar(
                "score",
                StorageUtilScalarCall::GetInt { missing: 0 },
                Some(&PrincipalStorageValue::String("wrong".to_owned())),
            ),
            Err(StorageUtilAdapterError::TypeMismatch)
        );
        assert_eq!(
            adapt_storage_util_global_scalar(
                "score",
                StorageUtilScalarCall::AdjustInt { amount: 1 },
                Some(&PrincipalStorageValue::I64(i64::from(i32::MAX))),
            ),
            Err(StorageUtilAdapterError::IntegerOverflow)
        );
        assert_eq!(
            adapt_storage_util_global_scalar(
                "ratio",
                StorageUtilScalarCall::SetFloat { value: f32::NAN },
                None,
            ),
            Err(StorageUtilAdapterError::NonFiniteFloat)
        );
        assert_eq!(
            adapt_storage_util_global_scalar(
                "ratio",
                StorageUtilScalarCall::GetFloat { missing: f32::NAN },
                None,
            ),
            Err(StorageUtilAdapterError::NonFiniteFloat)
        );
        assert_eq!(
            adapt_storage_util_global_scalar(
                "owner",
                StorageUtilScalarCall::HasForm,
                Some(&PrincipalStorageValue::Bytes(vec![0; 19])),
            ),
            Err(StorageUtilAdapterError::TypeMismatch)
        );
    }

    #[test]
    fn fixed_mod_event_adapter_preserves_name_payload_and_sender() {
        let sender = FormRef::new([0x2a; 16], 0x800);
        let command = adapt_legacy_send_mod_event(
            "SKICP_configManagerReady",
            "page:selected".to_owned(),
            42.5,
            Some(sender),
        )
        .unwrap();
        assert_eq!(
            crate::event::legacy_skse_mod_event_name(&command.event).as_deref(),
            Some("SKICP_configManagerReady")
        );
        let payload = LegacySkseModEventPayload::decode(&command.payload).unwrap();
        assert_eq!(payload.string_arg, "page:selected");
        assert_eq!(payload.number_arg(), 42.5);
        assert_eq!(payload.sender, Some(sender));
        assert_eq!(
            method_source_alias("sendmodevent").unwrap().operation,
            "events.publish"
        );
        assert_eq!(
            method_source_alias("RegisterForModEvent")
                .unwrap()
                .operation,
            "events.queue-legacy-subscribe"
        );
        assert_eq!(
            method_source_alias("UnregisterForAllModEvents")
                .unwrap()
                .operation,
            "events.queue-legacy-unsubscribe-all"
        );
    }

    #[test]
    fn mod_event_catalog_does_not_map_unknown_provider_functions() {
        assert_eq!(
            classify_static_call("ModEvent", "UnknownHandleOperation")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Unsupported
        );
        assert_eq!(
            classify_static_call("ModEvent", "PushString")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Native
        );
        assert_eq!(
            source_alias("ModEvent", "Create").unwrap().operation,
            "events.legacy-builder-create"
        );
        assert_eq!(
            source_alias("ModEvent", "PushForm").unwrap().value_kind,
            "form"
        );
        let declarations = papyrus_mod_event_declarations();
        assert_eq!(declarations.len(), 8);
        for declaration in declarations {
            declaration.declaration.validate().unwrap();
        }
    }

    #[test]
    fn unsafe_host_facilities_are_explicitly_unsupported() {
        let json = classify_static_call("JsonUtil", "Load").unwrap();
        assert_eq!(json.disposition, CompatibilityDisposition::Unsupported);
        assert!(json.guidance.contains("sandbox"));
        let input = classify_static_call("Input", "TapKey").unwrap();
        assert_eq!(input.disposition, CompatibilityDisposition::Unsupported);
        assert!(input.guidance.contains("normalized"));
    }

    #[test]
    fn vanilla_and_unknown_mod_calls_are_not_misclassified() {
        assert!(classify_static_call("Utility", "Wait").is_none());
        assert!(classify_method_call("MyModFunction").is_none());
        assert_eq!(
            classify_static_call("SKSE", "UnknownNative")
                .unwrap()
                .disposition,
            CompatibilityDisposition::Unsupported
        );
    }
}
