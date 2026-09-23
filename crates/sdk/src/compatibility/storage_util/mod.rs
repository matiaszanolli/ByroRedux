//! PapyrusUtil's StorageUtil surface: routes, declarations, source
//! aliases, adapters and codecs.
//!
//! This is the whole four-layer stack for one service and it owns the
//! file's entire type vocabulary (`StorageUtilScalar*`, `StorageUtilList*`,
//! `StorageUtilPrefix*`), used nowhere else in the crate. At ~2050 lines it
//! was over half of the old 3759-line `compatibility.rs` (#3851); #4768 split
//! the declarations and the list/prefix surface into `declarations.rs` and
//! `list.rs`, leaving routes, types, source aliases, the scalar adapter and
//! the form codec here.

use super::*;

mod declarations;
mod list;

pub use declarations::*;
pub use list::*;

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

/// Creation Engine record signature → legacy `FormType` id, sorted by
/// signature (#3859).
///
/// Data, not behaviour: this was 106 `b"XXXX" => <i32>` match arms, where a
/// wrong or missing signature is invisible in the wall and nothing could
/// assert a property *about* the mapping. As a table, `form_type_table_is_
/// sorted_and_unique` can — and does — check the two invariants the lookup
/// depends on.
///
/// Sorted by signature rather than by id because [`storage_util_form_type_id`]
/// binary-searches it. `NPC_` and `CREA` both map to 43, the one many-to-one
/// relation in the set, and are two explicit rows rather than an alias arm.
/// Ids 96, 97, 106 and 107 are deliberately absent — the engine skips them.
///
/// This is the only copy of the mapping in the workspace (`grep -rn 'b"KYWD"'`
/// finds this site alone). If a canonical `RecordType` table ever lands in
/// `crates/plugin`, this becomes a duplication finding rather than a table.
static FORM_TYPE_IDS: &[(&[u8; 4], i32)] = &[
    (b"AACT", 6),
    (b"ACHR", 62),
    (b"ACTI", 24),
    (b"ADDN", 94),
    (b"ALCH", 46),
    (b"AMMO", 42),
    (b"ANIO", 83),
    (b"APPA", 33),
    (b"ARMA", 102),
    (b"ARMO", 26),
    (b"ASPC", 16),
    (b"AVIF", 95),
    (b"BOOK", 27),
    (b"BPTD", 93),
    (b"CELL", 60),
    (b"CLAS", 10),
    (b"CLMT", 55),
    (b"COBJ", 49),
    (b"CONT", 28),
    (b"CREA", 43),
    (b"CSTY", 80),
    (b"DEBR", 88),
    (b"DIAL", 75),
    (b"DOOR", 29),
    (b"ECZN", 103),
    (b"EFSH", 85),
    (b"ENCH", 21),
    (b"EXPL", 87),
    (b"EYES", 13),
    (b"FACT", 11),
    (b"FLOR", 39),
    (b"FLST", 91),
    (b"FSTP", 110),
    (b"FSTS", 111),
    (b"FURN", 40),
    (b"GLOB", 9),
    (b"GMST", 3),
    (b"GRAS", 37),
    (b"HAZD", 51),
    (b"HDPT", 12),
    (b"IDLE", 78),
    (b"IDLM", 47),
    (b"IMAD", 90),
    (b"IMGS", 89),
    (b"INFO", 76),
    (b"INGR", 30),
    (b"IPCT", 100),
    (b"IPDS", 101),
    (b"KEYM", 45),
    (b"KYWD", 4),
    (b"LAND", 72),
    (b"LCRT", 5),
    (b"LCTN", 104),
    (b"LGTM", 108),
    (b"LIGH", 31),
    (b"LSCR", 81),
    (b"LTEX", 20),
    (b"LVLI", 53),
    (b"LVLN", 44),
    (b"LVSP", 82),
    (b"MATT", 99),
    (b"MESG", 105),
    (b"MGEF", 18),
    (b"MISC", 32),
    (b"MSTT", 36),
    (b"MUSC", 109),
    (b"NAVI", 59),
    (b"NAVM", 73),
    (b"NOTE", 48),
    (b"NPC_", 43),
    (b"PACK", 79),
    (b"PARW", 64),
    (b"PBAR", 69),
    (b"PBEA", 66),
    (b"PCON", 68),
    (b"PERK", 92),
    (b"PFLA", 67),
    (b"PGRE", 65),
    (b"PHZD", 70),
    (b"PMIS", 63),
    (b"PROJ", 50),
    (b"QUST", 77),
    (b"RACE", 14),
    (b"REFR", 61),
    (b"REGN", 58),
    (b"RFCT", 57),
    (b"SCOL", 35),
    (b"SCPT", 19),
    (b"SCRL", 23),
    (b"SKIL", 17),
    (b"SLGM", 52),
    (b"SOUN", 15),
    (b"SPEL", 22),
    (b"SPGD", 56),
    (b"STAT", 34),
    (b"TACT", 25),
    (b"TES4", 1),
    (b"TLOD", 74),
    (b"TOFT", 86),
    (b"TREE", 38),
    (b"TXST", 7),
    (b"VTYP", 98),
    (b"WATR", 84),
    (b"WEAP", 41),
    (b"WRLD", 71),
    (b"WTHR", 54),
];

/// Resolve the stable Creation Engine `FormType` value for one cataloged form.
///
/// The catalog stores parser-independent record signatures, so this mapping
/// is deliberately centralized at the SDK boundary. Unknown or game-specific
/// signatures return `None` and are omitted from typed compatibility filters
/// instead of being guessed from a transient ECS object.
///
/// #3859 — the mapping itself is [`FORM_TYPE_IDS`]; this is the lookup.
pub fn storage_util_form_type_id(catalog: &ContentCatalog, form: FormRef) -> Option<i32> {
    let record_type = catalog.record(form)?.record_type();
    FORM_TYPE_IDS
        .binary_search_by_key(&&record_type, |(sig, _)| sig)
        .ok()
        .map(|i| FORM_TYPE_IDS[i].1)
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

/// #3859 — the two invariants [`storage_util_form_type_id`]'s lookup depends
/// on, plus a behavioural anchor.
///
/// As 106 match arms none of these were checkable: a match cannot be asked
/// whether it is sorted, whether it repeats a signature, or how many entries
/// it has. That is the whole reason the table is worth more than the arms —
/// the runtime cost was always identical (the compiler builds a jump table
/// either way).
#[cfg(test)]
mod form_type_table_tests {
    use super::FORM_TYPE_IDS;

    /// `binary_search_by_key` returns garbage — silently, and only for some
    /// inputs — on an unsorted slice, and a duplicate signature means one row
    /// is unreachable. Both are invisible at the call site.
    #[test]
    fn form_type_table_is_sorted_and_unique() {
        for pair in FORM_TYPE_IDS.windows(2) {
            let (a, b) = (pair[0].0, pair[1].0);
            assert!(
                a < b,
                "FORM_TYPE_IDS must be sorted by signature and contain no duplicates — \
                 `{}` is not strictly before `{}`. `storage_util_form_type_id` \
                 binary-searches this table, so an out-of-order row makes some lookups \
                 miss a signature that is present, with no error anywhere.",
                String::from_utf8_lossy(a),
                String::from_utf8_lossy(b),
            );
        }
    }

    /// Anti-vacuity plus the one many-to-one relation, spelled out so a future
    /// edit that collapses it (or drops half of it) is loud.
    #[test]
    fn the_table_keeps_its_size_and_its_one_alias() {
        assert_eq!(
            FORM_TYPE_IDS.len(),
            106,
            "the signature set changed size — update this count deliberately, so a \
             row silently lost in a merge cannot pass as an intentional edit"
        );
        let id = |sig: &[u8; 4]| {
            FORM_TYPE_IDS
                .iter()
                .find(|(s, _)| *s == sig)
                .map(|(_, id)| *id)
        };
        assert_eq!(id(b"NPC_"), Some(43));
        assert_eq!(
            id(b"CREA"),
            Some(43),
            "NPC_ and CREA share FormType 43 — the set's only many-to-one relation, \
             carried as two rows rather than an alias arm"
        );
        // Endpoints, so a truncated table cannot pass the sortedness check alone.
        assert_eq!(id(b"TES4"), Some(1));
        assert_eq!(id(b"FSTS"), Some(111));
        // The four ids the engine skips must stay skipped.
        for skipped in [96, 97, 106, 107] {
            assert!(
                !FORM_TYPE_IDS.iter().any(|(_, id)| *id == skipped),
                "FormType {skipped} is deliberately unassigned in the engine's numbering"
            );
        }
    }
}
