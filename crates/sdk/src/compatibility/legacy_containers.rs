//! The JContainers-style legacy container surface.

use super::*;

pub const PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX: &str = "byro.legacy-containers.compat.";

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

pub(crate) fn legacy_container_source_alias(provider: &str, function: &str) -> Option<SourceAlias> {
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
