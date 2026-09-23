//! StorageUtil Papyrus declarations: the list, prefix and scalar function
//! signatures the compatibility layer registers (split from
//! `storage_util.rs`, #4768).

use super::*;

fn papyrus_storage_util_list_declarations(
    object_and_key: &[(&str, ScriptValueType, bool); 2],
) -> Vec<EnginePapyrusFunctionDeclaration> {
    let mut declarations = Vec::with_capacity(82);
    for (kind, suffix, value_type, array_type) in [
        (
            "int",
            "Int",
            ScriptValueType::Integer,
            ScriptValueType::IntegerArray,
        ),
        (
            "float",
            "Float",
            ScriptValueType::Float,
            ScriptValueType::FloatArray,
        ),
        (
            "string",
            "String",
            ScriptValueType::String,
            ScriptValueType::StringArray,
        ),
        (
            "form",
            "Form",
            ScriptValueType::Form,
            ScriptValueType::FormArray,
        ),
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
            ("random", "Random", value_type, object_and_key.to_vec()),
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
                "resize",
                "Resize",
                ScriptValueType::Integer,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("to-length", ScriptValueType::Integer, true),
                    ("filler", value_type, true),
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
        let function = format!("{suffix}ListSort");
        let id = format!("storage-util-{kind}-list-sort");
        let route = format!("{PAPYRUS_STORAGE_UTIL_LIST_ROUTE_PREFIX}{kind}-sort");
        declarations.push(papyrus_storage_util_void_declaration(
            &route,
            &id,
            &function,
            object_and_key,
        ));
        for (operation, function_operation, result, parameters) in [
            (
                "copy",
                "Copy",
                ScriptValueType::Boolean,
                vec![
                    object_and_key[0],
                    object_and_key[1],
                    ("copy", array_type, true),
                ],
            ),
            ("to-array", "ToArray", array_type, object_and_key.to_vec()),
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
        let function = format!("{suffix}ListSlice");
        let id = format!("storage-util-{kind}-list-slice");
        let route = format!("{PAPYRUS_STORAGE_UTIL_LIST_ROUTE_PREFIX}{kind}-slice");
        declarations.push(papyrus_storage_util_void_declaration(
            &route,
            &id,
            &function,
            &[
                object_and_key[0],
                object_and_key[1],
                ("slice", array_type, true),
                ("start-index", ScriptValueType::Integer, true),
            ],
        ));
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

fn papyrus_storage_util_prefix_declarations() -> Vec<EnginePapyrusFunctionDeclaration> {
    let mut declarations = Vec::with_capacity(18);
    for (kind, suffix) in [
        ("int-value", "IntValue"),
        ("float-value", "FloatValue"),
        ("string-value", "StringValue"),
        ("form-value", "FormValue"),
        ("int-list", "IntList"),
        ("float-list", "FloatList"),
        ("string-list", "StringList"),
        ("form-list", "FormList"),
        ("all", "All"),
    ] {
        for (operation, function_operation) in [("count", "Count"), ("clear", "Clear")] {
            let function = format!("{function_operation}{suffix}Prefix");
            let id = format!("storage-util-{operation}-{kind}-prefix");
            let route = format!("{PAPYRUS_STORAGE_UTIL_PREFIX_ROUTE_PREFIX}{operation}-{kind}");
            declarations.push(papyrus_storage_util_declaration(
                &route,
                &id,
                &function,
                &[("prefix", ScriptValueType::String, false)],
                ScriptValueType::Integer,
            ));
        }
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
    papyrus_storage_util_declaration_with_result(route, id, function, parameters, Some(result))
}

fn papyrus_storage_util_void_declaration(
    route: &str,
    id: &str,
    function: &str,
    parameters: &[(&str, ScriptValueType, bool)],
) -> EnginePapyrusFunctionDeclaration {
    papyrus_storage_util_declaration_with_result(route, id, function, parameters, None)
}

fn papyrus_storage_util_declaration_with_result(
    route: &str,
    id: &str,
    function: &str,
    parameters: &[(&str, ScriptValueType, bool)],
    result: Option<ScriptValueType>,
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
            result: result.map(|value_type| ScriptResultDeclaration {
                value_type,
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

/// `(route, function ID, Papyrus function, the one parameter that follows
/// object+key, result type)` for every exact-key scalar `StorageUtil` verb.
///
/// #4218 — this was 22 near-identical `papyrus_storage_util_declaration`
/// calls spanning 210 lines of `papyrus_storage_util_declarations`. The rows
/// are spelled out rather than derived from a type × verb cross product,
/// because the surface is not one: `Adjust` exists for Int and Float only.
/// Generating it would have invented `AdjustStringValue` and
/// `AdjustFormValue` routes PapyrusUtil has never had — the count assertion
/// in `compatibility::tests` is what would have caught that, and only
/// because it pins an exact number.
type StorageUtilScalarDeclaration = (
    &'static str,
    &'static str,
    &'static str,
    Option<(&'static str, ScriptValueType)>,
    ScriptValueType,
);

const STORAGE_UTIL_SCALAR_DECLARATIONS: &[StorageUtilScalarDeclaration] = &[
    (
        PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE,
        "storage-util-get-int-value",
        "GetIntValue",
        Some(("missing", ScriptValueType::Integer)),
        ScriptValueType::Integer,
    ),
    (
        PAPYRUS_STORAGE_UTIL_PLUCK_INT_VALUE_ROUTE,
        "storage-util-pluck-int-value",
        "PluckIntValue",
        Some(("missing", ScriptValueType::Integer)),
        ScriptValueType::Integer,
    ),
    (
        PAPYRUS_STORAGE_UTIL_HAS_INT_VALUE_ROUTE,
        "storage-util-has-int-value",
        "HasIntValue",
        None,
        ScriptValueType::Boolean,
    ),
    (
        PAPYRUS_STORAGE_UTIL_SET_INT_VALUE_ROUTE,
        "storage-util-set-int-value",
        "SetIntValue",
        Some(("value", ScriptValueType::Integer)),
        ScriptValueType::Integer,
    ),
    (
        PAPYRUS_STORAGE_UTIL_UNSET_INT_VALUE_ROUTE,
        "storage-util-unset-int-value",
        "UnsetIntValue",
        None,
        ScriptValueType::Boolean,
    ),
    (
        PAPYRUS_STORAGE_UTIL_ADJUST_INT_VALUE_ROUTE,
        "storage-util-adjust-int-value",
        "AdjustIntValue",
        Some(("amount", ScriptValueType::Integer)),
        ScriptValueType::Integer,
    ),
    (
        PAPYRUS_STORAGE_UTIL_GET_FLOAT_VALUE_ROUTE,
        "storage-util-get-float-value",
        "GetFloatValue",
        Some(("missing", ScriptValueType::Float)),
        ScriptValueType::Float,
    ),
    (
        PAPYRUS_STORAGE_UTIL_PLUCK_FLOAT_VALUE_ROUTE,
        "storage-util-pluck-float-value",
        "PluckFloatValue",
        Some(("missing", ScriptValueType::Float)),
        ScriptValueType::Float,
    ),
    (
        PAPYRUS_STORAGE_UTIL_HAS_FLOAT_VALUE_ROUTE,
        "storage-util-has-float-value",
        "HasFloatValue",
        None,
        ScriptValueType::Boolean,
    ),
    (
        PAPYRUS_STORAGE_UTIL_SET_FLOAT_VALUE_ROUTE,
        "storage-util-set-float-value",
        "SetFloatValue",
        Some(("value", ScriptValueType::Float)),
        ScriptValueType::Float,
    ),
    (
        PAPYRUS_STORAGE_UTIL_UNSET_FLOAT_VALUE_ROUTE,
        "storage-util-unset-float-value",
        "UnsetFloatValue",
        None,
        ScriptValueType::Boolean,
    ),
    (
        PAPYRUS_STORAGE_UTIL_ADJUST_FLOAT_VALUE_ROUTE,
        "storage-util-adjust-float-value",
        "AdjustFloatValue",
        Some(("amount", ScriptValueType::Float)),
        ScriptValueType::Float,
    ),
    (
        PAPYRUS_STORAGE_UTIL_GET_STRING_VALUE_ROUTE,
        "storage-util-get-string-value",
        "GetStringValue",
        Some(("missing", ScriptValueType::String)),
        ScriptValueType::String,
    ),
    (
        PAPYRUS_STORAGE_UTIL_PLUCK_STRING_VALUE_ROUTE,
        "storage-util-pluck-string-value",
        "PluckStringValue",
        Some(("missing", ScriptValueType::String)),
        ScriptValueType::String,
    ),
    (
        PAPYRUS_STORAGE_UTIL_HAS_STRING_VALUE_ROUTE,
        "storage-util-has-string-value",
        "HasStringValue",
        None,
        ScriptValueType::Boolean,
    ),
    (
        PAPYRUS_STORAGE_UTIL_SET_STRING_VALUE_ROUTE,
        "storage-util-set-string-value",
        "SetStringValue",
        Some(("value", ScriptValueType::String)),
        ScriptValueType::String,
    ),
    (
        PAPYRUS_STORAGE_UTIL_UNSET_STRING_VALUE_ROUTE,
        "storage-util-unset-string-value",
        "UnsetStringValue",
        None,
        ScriptValueType::Boolean,
    ),
    (
        PAPYRUS_STORAGE_UTIL_GET_FORM_VALUE_ROUTE,
        "storage-util-get-form-value",
        "GetFormValue",
        Some(("missing", ScriptValueType::Form)),
        ScriptValueType::Form,
    ),
    (
        PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE,
        "storage-util-pluck-form-value",
        "PluckFormValue",
        Some(("missing", ScriptValueType::Form)),
        ScriptValueType::Form,
    ),
    (
        PAPYRUS_STORAGE_UTIL_HAS_FORM_VALUE_ROUTE,
        "storage-util-has-form-value",
        "HasFormValue",
        None,
        ScriptValueType::Boolean,
    ),
    (
        PAPYRUS_STORAGE_UTIL_SET_FORM_VALUE_ROUTE,
        "storage-util-set-form-value",
        "SetFormValue",
        Some(("value", ScriptValueType::Form)),
        ScriptValueType::Form,
    ),
    (
        PAPYRUS_STORAGE_UTIL_UNSET_FORM_VALUE_ROUTE,
        "storage-util-unset-form-value",
        "UnsetFormValue",
        None,
        ScriptValueType::Boolean,
    ),
];

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
    let mut declarations: Vec<EnginePapyrusFunctionDeclaration> = STORAGE_UTIL_SCALAR_DECLARATIONS
        .iter()
        .map(|(route, id, function, extra, result)| {
            let mut parameters = vec![object_and_key[0], object_and_key[1]];
            if let Some((name, value_type)) = *extra {
                parameters.push((name, value_type, true));
            }
            papyrus_storage_util_declaration(route, id, function, &parameters, *result)
        })
        .collect();
    declarations.extend(papyrus_storage_util_list_declarations(&object_and_key));
    declarations.extend([
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_FORM_FILTER_BY_TYPE_ROUTE,
            "storage-util-form-list-filter-by-type",
            "FormListFilterByType",
            &[
                object_and_key[0],
                object_and_key[1],
                ("form-type", ScriptValueType::Integer, true),
                ("return-matching", ScriptValueType::Boolean, true),
            ],
            ScriptValueType::FormArray,
        ),
        papyrus_storage_util_declaration(
            PAPYRUS_STORAGE_UTIL_FORM_FILTER_BY_TYPES_ROUTE,
            "storage-util-form-list-filter-by-types",
            "FormListFilterByTypes",
            &[
                object_and_key[0],
                object_and_key[1],
                ("form-types", ScriptValueType::IntegerArray, true),
                ("return-matching", ScriptValueType::Boolean, true),
            ],
            ScriptValueType::FormArray,
        ),
    ]);
    declarations.extend(papyrus_storage_util_prefix_declarations());
    declarations
}
