//! PapyrusUtil's StorageUtil surface: routes, declarations, source
//! aliases, adapters and codecs.
//!
//! This is the whole four-layer stack for one service and it owns the
//! file's entire type vocabulary (`StorageUtilScalar*`, `StorageUtilList*`,
//! `StorageUtilPrefix*`), used nowhere else in the crate. At ~2050 lines it
//! was over half of the old 3759-line `compatibility.rs` (#3851).

use super::*;

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

pub const PAPYRUS_STORAGE_UTIL_FORM_FILTER_BY_TYPE_ROUTE: &str =
    "byro.storage.compat.storage-util.list-form-filter-by-type";

pub const PAPYRUS_STORAGE_UTIL_FORM_FILTER_BY_TYPES_ROUTE: &str =
    "byro.storage.compat.storage-util.list-form-filter-by-types";

pub const PAPYRUS_STORAGE_UTIL_PREFIX_ROUTE_PREFIX: &str =
    "byro.storage.compat.storage-util.prefix-";

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
    #[error("StorageUtil prefix cannot be empty")]
    EmptyPrefix,
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
    Random {
        selector: u64,
    },
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
    Sort,
    Resize {
        to_length: i32,
        filler: StorageUtilListValue,
    },
    Copy {
        values: Vec<StorageUtilListValue>,
    },
    Slice {
        values: Vec<StorageUtilListValue>,
        start_index: i32,
    },
    ToArray,
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
    None,
    Value(StorageUtilListValue),
    Array(Vec<StorageUtilListValue>),
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
    Random,
    Count,
    Clear,
    RemoveAt,
    Insert,
    Remove,
    CountValue,
    Adjust,
    Sort,
    Resize,
    Copy,
    Slice,
    FilterByType,
    FilterByTypes,
    ToArray,
    Find,
    Has,
}

/// Type namespace selected by a global `StorageUtil` prefix operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageUtilPrefixKind {
    IntValue,
    FloatValue,
    StringValue,
    FormValue,
    IntList,
    FloatList,
    StringList,
    FormList,
    All,
}

/// Read-only count or atomic clear over one principal-private prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageUtilPrefixOperation {
    Count,
    Clear,
}

/// Bounded result and deferred mutations for a global prefix call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageUtilPrefixAdaptation {
    pub result: i32,
    pub commands: Vec<PrincipalStorageCommand>,
}

pub(crate) fn storage_util_prefix_source_alias(function: &str) -> Option<SourceAlias> {
    let aliases = [
        "CountIntValuePrefix",
        "CountFloatValuePrefix",
        "CountStringValuePrefix",
        "CountFormValuePrefix",
        "CountIntListPrefix",
        "CountFloatListPrefix",
        "CountStringListPrefix",
        "CountFormListPrefix",
        "CountAllPrefix",
        "ClearIntValuePrefix",
        "ClearFloatValuePrefix",
        "ClearStringValuePrefix",
        "ClearFormValuePrefix",
        "ClearIntListPrefix",
        "ClearFloatListPrefix",
        "ClearStringListPrefix",
        "ClearFormListPrefix",
        "ClearAllPrefix",
    ];
    aliases
        .into_iter()
        .find(|candidate| function.eq_ignore_ascii_case(candidate))
        .map(|function| SourceAlias {
            provider: "StorageUtil",
            function,
            service: PRINCIPAL_STORAGE_SERVICE,
            operation: if function.starts_with("Count") {
                "storage.prefix-count"
            } else {
                "storage.prefix-clear"
            },
            value_kind: "signed",
            constraint: "non-empty case-folded prefix; principal-private global values only",
        })
}

pub(crate) fn storage_util_list_source_alias(function: &str) -> Option<SourceAlias> {
    let aliases = [
        ("IntListAdd", "storage.array-get+queue-push", "signed"),
        ("IntListGet", "storage.array-get", "signed"),
        ("IntListSet", "storage.array-get+queue-set", "signed"),
        ("IntListPluck", "storage.array-get+queue-remove", "signed"),
        ("IntListShift", "storage.array-get+queue-remove", "signed"),
        ("IntListPop", "storage.array-get+queue-remove", "signed"),
        ("IntListRandom", "storage.array-get", "signed"),
        ("IntListCopy", "storage.array-get+queue-replace", "bool"),
        ("IntListSlice", "storage.array-get+array-fill", "none"),
        ("IntListToArray", "storage.array-get", "signed-array"),
        ("IntListCount", "storage.array-get", "signed"),
        ("IntListClear", "storage.array-get+queue-delete", "signed"),
        ("IntListRemoveAt", "storage.array-get+queue-remove", "bool"),
        ("IntListInsert", "storage.array-get+queue-replace", "bool"),
        ("IntListRemove", "storage.array-get+queue-replace", "signed"),
        ("IntListCountValue", "storage.array-get", "signed"),
        ("IntListAdjust", "storage.array-get+queue-set", "signed"),
        ("IntListSort", "storage.array-get+queue-replace", "none"),
        ("IntListResize", "storage.array-get+queue-replace", "signed"),
        ("IntListFind", "storage.array-get", "signed"),
        ("IntListHas", "storage.array-get", "bool"),
        ("FloatListAdd", "storage.array-get+queue-push", "float"),
        ("FloatListGet", "storage.array-get", "float"),
        ("FloatListSet", "storage.array-get+queue-set", "float"),
        ("FloatListPluck", "storage.array-get+queue-remove", "float"),
        ("FloatListShift", "storage.array-get+queue-remove", "float"),
        ("FloatListPop", "storage.array-get+queue-remove", "float"),
        ("FloatListRandom", "storage.array-get", "float"),
        ("FloatListCopy", "storage.array-get+queue-replace", "bool"),
        ("FloatListSlice", "storage.array-get+array-fill", "none"),
        ("FloatListToArray", "storage.array-get", "float-array"),
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
        ("FloatListSort", "storage.array-get+queue-replace", "none"),
        (
            "FloatListResize",
            "storage.array-get+queue-replace",
            "signed",
        ),
        ("FloatListFind", "storage.array-get", "signed"),
        ("FloatListHas", "storage.array-get", "bool"),
        ("StringListAdd", "storage.array-get+queue-push", "text"),
        ("StringListGet", "storage.array-get", "text"),
        ("StringListSet", "storage.array-get+queue-set", "text"),
        ("StringListPluck", "storage.array-get+queue-remove", "text"),
        ("StringListShift", "storage.array-get+queue-remove", "text"),
        ("StringListPop", "storage.array-get+queue-remove", "text"),
        ("StringListRandom", "storage.array-get", "text"),
        ("StringListCopy", "storage.array-get+queue-replace", "bool"),
        ("StringListSlice", "storage.array-get+array-fill", "none"),
        ("StringListToArray", "storage.array-get", "text-array"),
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
        ("StringListSort", "storage.array-get+queue-replace", "none"),
        (
            "StringListResize",
            "storage.array-get+queue-replace",
            "signed",
        ),
        ("StringListFind", "storage.array-get", "signed"),
        ("StringListHas", "storage.array-get", "bool"),
        ("FormListAdd", "storage.array-get+queue-push", "form"),
        ("FormListGet", "storage.array-get", "form"),
        ("FormListSet", "storage.array-get+queue-set", "form"),
        ("FormListPluck", "storage.array-get+queue-remove", "form"),
        ("FormListShift", "storage.array-get+queue-remove", "form"),
        ("FormListPop", "storage.array-get+queue-remove", "form"),
        ("FormListRandom", "storage.array-get", "form"),
        ("FormListCopy", "storage.array-get+queue-replace", "bool"),
        ("FormListSlice", "storage.array-get+array-fill", "none"),
        ("FormListToArray", "storage.array-get", "form-array"),
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
        ("FormListSort", "storage.array-get+queue-replace", "none"),
        (
            "FormListResize",
            "storage.array-get+queue-replace",
            "signed",
        ),
        ("FormListFind", "storage.array-get", "signed"),
        ("FormListHas", "storage.array-get", "bool"),
        (
            "FormListFilterByType",
            "storage.array-get+form-type-filter",
            "form-array",
        ),
        (
            "FormListFilterByTypes",
            "storage.array-get+form-type-filter",
            "form-array",
        ),
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

pub(crate) fn encode_storage_util_form(value: FormRef) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(20);
    encoded.extend_from_slice(&value.source());
    encoded.extend_from_slice(&value.local().to_le_bytes());
    encoded
}

/// Resolve the stable Creation Engine `FormType` value for one cataloged form.
///
/// The catalog stores parser-independent record signatures, so this mapping
/// is deliberately centralized at the SDK boundary. Unknown or game-specific
/// signatures return `None` and are omitted from typed compatibility filters
/// instead of being guessed from a transient ECS object.
pub fn storage_util_form_type_id(catalog: &ContentCatalog, form: FormRef) -> Option<i32> {
    let record_type = catalog.record(form)?.record_type();
    Some(match &record_type {
        b"TES4" => 1,
        b"GMST" => 3,
        b"KYWD" => 4,
        b"LCRT" => 5,
        b"AACT" => 6,
        b"TXST" => 7,
        b"GLOB" => 9,
        b"CLAS" => 10,
        b"FACT" => 11,
        b"HDPT" => 12,
        b"EYES" => 13,
        b"RACE" => 14,
        b"SOUN" => 15,
        b"ASPC" => 16,
        b"SKIL" => 17,
        b"MGEF" => 18,
        b"SCPT" => 19,
        b"LTEX" => 20,
        b"ENCH" => 21,
        b"SPEL" => 22,
        b"SCRL" => 23,
        b"ACTI" => 24,
        b"TACT" => 25,
        b"ARMO" => 26,
        b"BOOK" => 27,
        b"CONT" => 28,
        b"DOOR" => 29,
        b"INGR" => 30,
        b"LIGH" => 31,
        b"MISC" => 32,
        b"APPA" => 33,
        b"STAT" => 34,
        b"SCOL" => 35,
        b"MSTT" => 36,
        b"GRAS" => 37,
        b"TREE" => 38,
        b"FLOR" => 39,
        b"FURN" => 40,
        b"WEAP" => 41,
        b"AMMO" => 42,
        b"NPC_" | b"CREA" => 43,
        b"LVLN" => 44,
        b"KEYM" => 45,
        b"ALCH" => 46,
        b"IDLM" => 47,
        b"NOTE" => 48,
        b"COBJ" => 49,
        b"PROJ" => 50,
        b"HAZD" => 51,
        b"SLGM" => 52,
        b"LVLI" => 53,
        b"WTHR" => 54,
        b"CLMT" => 55,
        b"SPGD" => 56,
        b"RFCT" => 57,
        b"REGN" => 58,
        b"NAVI" => 59,
        b"CELL" => 60,
        b"REFR" => 61,
        b"ACHR" => 62,
        b"PMIS" => 63,
        b"PARW" => 64,
        b"PGRE" => 65,
        b"PBEA" => 66,
        b"PFLA" => 67,
        b"PCON" => 68,
        b"PBAR" => 69,
        b"PHZD" => 70,
        b"WRLD" => 71,
        b"LAND" => 72,
        b"NAVM" => 73,
        b"TLOD" => 74,
        b"DIAL" => 75,
        b"INFO" => 76,
        b"QUST" => 77,
        b"IDLE" => 78,
        b"PACK" => 79,
        b"CSTY" => 80,
        b"LSCR" => 81,
        b"LVSP" => 82,
        b"ANIO" => 83,
        b"WATR" => 84,
        b"EFSH" => 85,
        b"TOFT" => 86,
        b"EXPL" => 87,
        b"DEBR" => 88,
        b"IMGS" => 89,
        b"IMAD" => 90,
        b"FLST" => 91,
        b"PERK" => 92,
        b"BPTD" => 93,
        b"ADDN" => 94,
        b"AVIF" => 95,
        b"VTYP" => 98,
        b"MATT" => 99,
        b"IPCT" => 100,
        b"IPDS" => 101,
        b"ARMA" => 102,
        b"ECZN" => 103,
        b"LCTN" => 104,
        b"MESG" => 105,
        b"LGTM" => 108,
        b"MUSC" => 109,
        b"FSTP" => 110,
        b"FSTS" => 111,
        _ => return None,
    })
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
        "random" => StorageUtilListOperation::Random,
        "count" => StorageUtilListOperation::Count,
        "clear" => StorageUtilListOperation::Clear,
        "remove-at" => StorageUtilListOperation::RemoveAt,
        "insert" => StorageUtilListOperation::Insert,
        "remove" => StorageUtilListOperation::Remove,
        "count-value" => StorageUtilListOperation::CountValue,
        "adjust" => StorageUtilListOperation::Adjust,
        "sort" => StorageUtilListOperation::Sort,
        "resize" => StorageUtilListOperation::Resize,
        "copy" => StorageUtilListOperation::Copy,
        "slice" => StorageUtilListOperation::Slice,
        "filter-by-type" => StorageUtilListOperation::FilterByType,
        "filter-by-types" => StorageUtilListOperation::FilterByTypes,
        "to-array" => StorageUtilListOperation::ToArray,
        "find" => StorageUtilListOperation::Find,
        "has" => StorageUtilListOperation::Has,
        _ => return None,
    };
    Some((kind, operation))
}

/// Decode a built-in global `StorageUtil` prefix route.
pub fn parse_storage_util_prefix_route(
    route: &str,
) -> Option<(StorageUtilPrefixKind, StorageUtilPrefixOperation)> {
    let suffix = route.strip_prefix(PAPYRUS_STORAGE_UTIL_PREFIX_ROUTE_PREFIX)?;
    let (operation, kind) = suffix.split_once('-')?;
    let operation = match operation {
        "count" => StorageUtilPrefixOperation::Count,
        "clear" => StorageUtilPrefixOperation::Clear,
        _ => return None,
    };
    let kind = match kind {
        "int-value" => StorageUtilPrefixKind::IntValue,
        "float-value" => StorageUtilPrefixKind::FloatValue,
        "string-value" => StorageUtilPrefixKind::StringValue,
        "form-value" => StorageUtilPrefixKind::FormValue,
        "int-list" => StorageUtilPrefixKind::IntList,
        "float-list" => StorageUtilPrefixKind::FloatList,
        "string-list" => StorageUtilPrefixKind::StringList,
        "form-list" => StorageUtilPrefixKind::FormList,
        "all" => StorageUtilPrefixKind::All,
        _ => return None,
    };
    Some((kind, operation))
}

/// Count or clear case-folded global keys inside one authenticated principal.
pub fn adapt_storage_util_global_prefix(
    prefix: &str,
    kind: StorageUtilPrefixKind,
    operation: StorageUtilPrefixOperation,
    values: Option<&BTreeMap<StorageKey, PrincipalStorageValue>>,
) -> Result<StorageUtilPrefixAdaptation, StorageUtilAdapterError> {
    if prefix.is_empty() {
        return Err(StorageUtilAdapterError::EmptyPrefix);
    }
    let prefix = prefix.to_ascii_lowercase();
    let namespaces: &[&str] = match kind {
        StorageUtilPrefixKind::IntValue => &["storageutil.int:"],
        StorageUtilPrefixKind::FloatValue => &["storageutil.float:"],
        StorageUtilPrefixKind::StringValue => &["storageutil.string:"],
        StorageUtilPrefixKind::FormValue => &["storageutil.form:"],
        StorageUtilPrefixKind::IntList => &["storageutil.list.int:"],
        StorageUtilPrefixKind::FloatList => &["storageutil.list.float:"],
        StorageUtilPrefixKind::StringList => &["storageutil.list.string:"],
        StorageUtilPrefixKind::FormList => &["storageutil.list.form:"],
        StorageUtilPrefixKind::All => &[
            "storageutil.int:",
            "storageutil.float:",
            "storageutil.string:",
            "storageutil.form:",
            "storageutil.list.int:",
            "storageutil.list.float:",
            "storageutil.list.string:",
            "storageutil.list.form:",
        ],
    };
    let keys = values
        .into_iter()
        .flat_map(BTreeMap::keys)
        .filter(|key| {
            namespaces.iter().any(|namespace| {
                key.as_str()
                    .strip_prefix(namespace)
                    .is_some_and(|name| name.starts_with(&prefix))
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    let result =
        i32::try_from(keys.len()).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?;
    let commands = if operation == StorageUtilPrefixOperation::Clear {
        keys.into_iter()
            .map(|key| PrincipalStorageCommand::Delete { key })
            .collect()
    } else {
        Vec::new()
    };
    Ok(StorageUtilPrefixAdaptation { result, commands })
}

/// Adapt one exact global `StorageUtil` list call to bounded principal storage.
pub fn adapt_storage_util_global_list(
    key_name: &str,
    kind: StorageUtilListKind,
    call: StorageUtilListCall,
    current: Option<&PrincipalStorageValue>,
    max_entries: usize,
) -> Result<StorageUtilListAdaptation, StorageUtilAdapterError> {
    const PAPYRUS_UTIL_LIST_RESIZE_LIMIT: usize = 500;

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
        StorageUtilListCall::Random { selector } => {
            let value = if values.is_empty() {
                default_storage_util_list_value(kind)
            } else {
                let index = (selector % values.len() as u64) as usize;
                values[index].clone()
            };
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
        StorageUtilListCall::Sort => {
            let mut replacement = values.clone();
            match kind {
                StorageUtilListKind::Int => replacement.sort_by(|left, right| {
                    let (StorageUtilListValue::Int(left), StorageUtilListValue::Int(right)) =
                        (left, right)
                    else {
                        unreachable!("decoded StorageUtil Int list is homogeneous")
                    };
                    left.cmp(right)
                }),
                StorageUtilListKind::Float => replacement.sort_by(|left, right| {
                    let (StorageUtilListValue::Float(left), StorageUtilListValue::Float(right)) =
                        (left, right)
                    else {
                        unreachable!("decoded StorageUtil Float list is homogeneous")
                    };
                    left.total_cmp(right)
                }),
                StorageUtilListKind::String => replacement.sort_by(|left, right| {
                    let (StorageUtilListValue::String(left), StorageUtilListValue::String(right)) =
                        (left, right)
                    else {
                        unreachable!("decoded StorageUtil String list is homogeneous")
                    };
                    left.cmp(right)
                }),
                StorageUtilListKind::Form => replacement.sort_by(|left, right| {
                    let (StorageUtilListValue::Form(left), StorageUtilListValue::Form(right)) =
                        (left, right)
                    else {
                        unreachable!("decoded StorageUtil Form list is homogeneous")
                    };
                    match (left, right) {
                        (None, None) => std::cmp::Ordering::Equal,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (Some(left), Some(right)) => left
                            .source()
                            .cmp(&right.source())
                            .then_with(|| left.local().cmp(&right.local())),
                    }
                }),
            }
            if current.is_some() {
                commands.push(PrincipalStorageCommand::ArrayReplace {
                    key: key.clone(),
                    values: encode_storage_util_list_values(kind, &replacement)?,
                });
            }
            StorageUtilListResult::None
        }
        StorageUtilListCall::Resize { to_length, filler } => {
            encode_storage_util_list_value(kind, &filler)?;
            let Some(target) = usize::try_from(to_length).ok().filter(|target| {
                *target <= PAPYRUS_UTIL_LIST_RESIZE_LIMIT && *target <= max_entries
            }) else {
                return Ok(StorageUtilListAdaptation {
                    key,
                    result: StorageUtilListResult::Int(0),
                    commands,
                });
            };
            let delta = i64::try_from(target)
                .and_then(|target| i64::try_from(values.len()).map(|length| target - length))
                .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?;
            let delta =
                i32::try_from(delta).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?;
            if target != values.len() {
                if target == 0 {
                    if current.is_some() {
                        commands.push(PrincipalStorageCommand::Delete { key: key.clone() });
                    }
                } else {
                    let mut replacement = values.clone();
                    replacement.resize(target, filler);
                    commands.push(PrincipalStorageCommand::ArrayReplace {
                        key: key.clone(),
                        values: encode_storage_util_list_values(kind, &replacement)?,
                    });
                }
            }
            StorageUtilListResult::Int(delta)
        }
        StorageUtilListCall::Copy {
            values: replacement,
        } => {
            if replacement.len() > max_entries {
                StorageUtilListResult::Bool(false)
            } else {
                commands.push(PrincipalStorageCommand::ArrayReplace {
                    key: key.clone(),
                    values: encode_storage_util_list_values(kind, &replacement)?,
                });
                StorageUtilListResult::Bool(true)
            }
        }
        StorageUtilListCall::Slice {
            values: mut replacement,
            start_index,
        } => {
            encode_storage_util_list_values(kind, &replacement)?;
            if let Ok(start) = usize::try_from(start_index) {
                for (offset, target) in replacement.iter_mut().enumerate() {
                    let Some(source) = values.get(start.saturating_add(offset)) else {
                        break;
                    };
                    *target = source.clone();
                }
            }
            StorageUtilListResult::Array(replacement)
        }
        StorageUtilListCall::ToArray => StorageUtilListResult::Array(values),
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

/// Filter a principal-private Form list by the portable Creation Engine form
/// type IDs used by `FormType.psc`. Unknown record metadata is omitted rather
/// than guessed, for both matching and inverse filters.
pub fn adapt_storage_util_global_form_filter(
    key_name: &str,
    form_type_ids: &[i32],
    return_matching: bool,
    current: Option<&PrincipalStorageValue>,
    catalog: &ContentCatalog,
) -> Result<StorageUtilListAdaptation, StorageUtilAdapterError> {
    if form_type_ids.len() > MAX_SCRIPT_ARRAY_ELEMENTS {
        return Err(StorageUtilAdapterError::IntegerOutOfRange);
    }
    let key = StorageKey::new(format!(
        "storageutil.list.form:{}",
        key_name.to_ascii_lowercase()
    ))?;
    let values = decode_storage_util_list(StorageUtilListKind::Form, current)?;
    let requested = form_type_ids.iter().copied().collect::<BTreeSet<_>>();
    let filtered = values
        .into_iter()
        .filter_map(|value| {
            let StorageUtilListValue::Form(Some(form)) = value else {
                return None;
            };
            let form_type = storage_util_form_type_id(catalog, form)?;
            (requested.contains(&form_type) == return_matching)
                .then_some(StorageUtilListValue::Form(Some(form)))
        })
        .collect();
    Ok(StorageUtilListAdaptation {
        key,
        result: StorageUtilListResult::Array(filtered),
        commands: Vec::new(),
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
