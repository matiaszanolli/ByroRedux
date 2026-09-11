//! PapyrusUtil / JContainers legacy script-extender shims.
//!
//! The largest single region of the old `extensions.rs` (#3843) and the
//! only one that needs the `PAPYRUS_STORAGE_UTIL_*_ROUTE` constant wall —
//! moving it here takes those imports out of every other region's view.
//!
//! These are compatibility surfaces, not engine features: they exist so
//! mods written against the SKSE-family extenders keep working. Their
//! declaration-side twin is `crates/sdk/src/compatibility/`.

use super::*;

impl ExtensionHost {
    pub(super) fn invoke_storage_util(
        &mut self,
        principal: Option<&PrincipalId>,
        qualified_name: &str,
        arguments: &[ScriptValue],
    ) -> Result<ScriptValue, ExtensionHostError> {
        let unavailable = |reason: String| ExtensionHostError::ScriptFunctionUnavailable {
            function: qualified_name.to_owned(),
            reason,
        };
        if let Some((kind, operation)) = parse_storage_util_list_route(qualified_name) {
            return self.invoke_storage_util_list(
                principal,
                qualified_name,
                kind,
                operation,
                arguments,
            );
        }
        if let Some((kind, operation)) = parse_storage_util_prefix_route(qualified_name) {
            return self.invoke_storage_util_prefix(
                principal,
                qualified_name,
                kind,
                operation,
                arguments,
            );
        }
        let principal = principal.ok_or_else(|| {
            unavailable("StorageUtil call has no authenticated legacy-script principal".to_owned())
        })?;
        let integer = |value: i64| {
            i32::try_from(value).map_err(|_| {
                unavailable(
                    "StorageUtil integer argument is outside the Papyrus i32 range".to_owned(),
                )
            })
        };
        let (key_name, call) = match (qualified_name, arguments) {
            (
                PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::GetInt { missing: 0 }),
            (
                PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(missing)],
            ) => (
                key,
                StorageUtilScalarCall::GetInt {
                    missing: integer(*missing)?,
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_PLUCK_INT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::PluckInt { missing: 0 }),
            (
                PAPYRUS_STORAGE_UTIL_PLUCK_INT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(missing)],
            ) => (
                key,
                StorageUtilScalarCall::PluckInt {
                    missing: integer(*missing)?,
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_HAS_INT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::HasInt),
            (
                PAPYRUS_STORAGE_UTIL_SET_INT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(value)],
            ) => (
                key,
                StorageUtilScalarCall::SetInt {
                    value: integer(*value)?,
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_UNSET_INT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::UnsetInt),
            (
                PAPYRUS_STORAGE_UTIL_ADJUST_INT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(amount)],
            ) => (
                key,
                StorageUtilScalarCall::AdjustInt {
                    amount: integer(*amount)?,
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_GET_FLOAT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::GetFloat { missing: 0.0 }),
            (
                PAPYRUS_STORAGE_UTIL_GET_FLOAT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Float(missing)],
            ) => (key, StorageUtilScalarCall::GetFloat { missing: *missing }),
            (
                PAPYRUS_STORAGE_UTIL_PLUCK_FLOAT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::PluckFloat { missing: 0.0 }),
            (
                PAPYRUS_STORAGE_UTIL_PLUCK_FLOAT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Float(missing)],
            ) => (key, StorageUtilScalarCall::PluckFloat { missing: *missing }),
            (
                PAPYRUS_STORAGE_UTIL_HAS_FLOAT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::HasFloat),
            (
                PAPYRUS_STORAGE_UTIL_SET_FLOAT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Float(value)],
            ) => (key, StorageUtilScalarCall::SetFloat { value: *value }),
            (
                PAPYRUS_STORAGE_UTIL_UNSET_FLOAT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::UnsetFloat),
            (
                PAPYRUS_STORAGE_UTIL_ADJUST_FLOAT_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Float(amount)],
            ) => (key, StorageUtilScalarCall::AdjustFloat { amount: *amount }),
            (
                PAPYRUS_STORAGE_UTIL_GET_STRING_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (
                key,
                StorageUtilScalarCall::GetString {
                    missing: String::new(),
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_PLUCK_STRING_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (
                key,
                StorageUtilScalarCall::PluckString {
                    missing: String::new(),
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_PLUCK_STRING_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::String(missing)],
            ) => (
                key,
                StorageUtilScalarCall::PluckString {
                    missing: missing.clone(),
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_GET_STRING_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::String(missing)],
            ) => (
                key,
                StorageUtilScalarCall::GetString {
                    missing: missing.clone(),
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_HAS_STRING_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::HasString),
            (
                PAPYRUS_STORAGE_UTIL_SET_STRING_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::String(value)],
            ) => (
                key,
                StorageUtilScalarCall::SetString {
                    value: value.clone(),
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_UNSET_STRING_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::UnsetString),
            (
                PAPYRUS_STORAGE_UTIL_GET_FORM_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::GetForm { missing: None }),
            (
                PAPYRUS_STORAGE_UTIL_GET_FORM_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::None],
            ) => (key, StorageUtilScalarCall::GetForm { missing: None }),
            (
                PAPYRUS_STORAGE_UTIL_GET_FORM_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Form(missing)],
            ) => (
                key,
                StorageUtilScalarCall::GetForm {
                    missing: Some(*missing),
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::PluckForm { missing: None }),
            (
                PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::None],
            ) => (key, StorageUtilScalarCall::PluckForm { missing: None }),
            (
                PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Form(missing)],
            ) => (
                key,
                StorageUtilScalarCall::PluckForm {
                    missing: Some(*missing),
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_HAS_FORM_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::HasForm),
            (
                PAPYRUS_STORAGE_UTIL_SET_FORM_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::None],
            ) => (key, StorageUtilScalarCall::SetForm { value: None }),
            (
                PAPYRUS_STORAGE_UTIL_SET_FORM_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Form(value)],
            ) => (
                key,
                StorageUtilScalarCall::SetForm {
                    value: Some(*value),
                },
            ),
            (
                PAPYRUS_STORAGE_UTIL_UNSET_FORM_VALUE_ROUTE,
                [ScriptValue::None, ScriptValue::String(key)],
            ) => (key, StorageUtilScalarCall::UnsetForm),
            _ => {
                return Err(unavailable(
                    "StorageUtil supports only global ObjKey=None scalar calls with exact typed arguments"
                        .to_owned(),
                ));
            }
        };
        let probe = adapt_storage_util_global_scalar(key_name, call.clone(), None)
            .map_err(|error| unavailable(error.to_string()))?;
        let current = self
            .principal_storage
            .values(principal)
            .and_then(|values| values.get(&probe.key))
            .cloned();
        let adaptation = adapt_storage_util_global_scalar(key_name, call, current.as_ref())
            .map_err(|error| unavailable(error.to_string()))?;
        if let Some(command) = adaptation.command {
            self.principal_storage.apply_batch(principal, &[command])?;
        }
        Ok(match adaptation.result {
            StorageUtilScalarResult::Int(value) => ScriptValue::Integer(i64::from(value)),
            StorageUtilScalarResult::Float(value) => ScriptValue::Float(value),
            StorageUtilScalarResult::Bool(value) => ScriptValue::Boolean(value),
            StorageUtilScalarResult::String(value) => ScriptValue::String(value),
            StorageUtilScalarResult::Form(Some(value)) => ScriptValue::Form(value),
            StorageUtilScalarResult::Form(None) => ScriptValue::None,
        })
    }

    fn invoke_storage_util_prefix(
        &mut self,
        principal: Option<&PrincipalId>,
        qualified_name: &str,
        kind: StorageUtilPrefixKind,
        operation: StorageUtilPrefixOperation,
        arguments: &[ScriptValue],
    ) -> Result<ScriptValue, ExtensionHostError> {
        let unavailable = |reason: String| ExtensionHostError::ScriptFunctionUnavailable {
            function: qualified_name.to_owned(),
            reason,
        };
        let principal = principal.ok_or_else(|| {
            unavailable("StorageUtil call has no authenticated legacy-script principal".to_owned())
        })?;
        let [ScriptValue::String(prefix)] = arguments else {
            return Err(unavailable(
                "StorageUtil prefix call requires one exact String argument".to_owned(),
            ));
        };
        let adaptation = adapt_storage_util_global_prefix(
            prefix,
            kind,
            operation,
            self.principal_storage.values(principal),
        )
        .map_err(|error| unavailable(error.to_string()))?;
        if !adaptation.commands.is_empty() {
            self.principal_storage
                .apply_batch(principal, &adaptation.commands)?;
        }
        Ok(ScriptValue::Integer(i64::from(adaptation.result)))
    }

    fn invoke_storage_util_form_filter(
        &mut self,
        principal: Option<&PrincipalId>,
        qualified_name: &str,
        operation: StorageUtilListOperation,
        arguments: &[ScriptValue],
    ) -> Result<ScriptValue, ExtensionHostError> {
        let unavailable = |reason: String| ExtensionHostError::ScriptFunctionUnavailable {
            function: qualified_name.to_owned(),
            reason,
        };
        let principal = principal.ok_or_else(|| {
            unavailable("StorageUtil call has no authenticated legacy-script principal".to_owned())
        })?;
        let integer = |value: i64| {
            i32::try_from(value).map_err(|_| {
                unavailable("StorageUtil form type ID is outside the Papyrus i32 range".to_owned())
            })
        };
        let (key_name, form_type_ids, return_matching) = match (operation, arguments) {
            (
                StorageUtilListOperation::FilterByType,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(form_type)],
            ) => (key, vec![integer(*form_type)?], true),
            (
                StorageUtilListOperation::FilterByType,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(form_type), ScriptValue::Boolean(return_matching)],
            ) => (key, vec![integer(*form_type)?], *return_matching),
            (
                StorageUtilListOperation::FilterByTypes,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::IntegerArray(form_types)],
            ) => (
                key,
                form_types
                    .iter()
                    .copied()
                    .map(integer)
                    .collect::<Result<Vec<_>, _>>()?,
                true,
            ),
            (
                StorageUtilListOperation::FilterByTypes,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::IntegerArray(form_types), ScriptValue::Boolean(return_matching)],
            ) => (
                key,
                form_types
                    .iter()
                    .copied()
                    .map(integer)
                    .collect::<Result<Vec<_>, _>>()?,
                *return_matching,
            ),
            _ => {
                return Err(unavailable(
                    "StorageUtil form filter requires ObjKey=None and exact typed arguments"
                        .to_owned(),
                ));
            }
        };
        let probe = adapt_storage_util_global_form_filter(
            key_name,
            &form_type_ids,
            return_matching,
            None,
            &self.content_catalog,
        )
        .map_err(|error| unavailable(error.to_string()))?;
        let current = self
            .principal_storage
            .values(principal)
            .and_then(|values| values.get(&probe.key))
            .cloned();
        let adaptation = adapt_storage_util_global_form_filter(
            key_name,
            &form_type_ids,
            return_matching,
            current.as_ref(),
            &self.content_catalog,
        )
        .map_err(|error| unavailable(error.to_string()))?;
        if !adaptation.commands.is_empty() {
            self.principal_storage
                .apply_batch(principal, &adaptation.commands)?;
        }
        let StorageUtilListResult::Array(values) = adaptation.result else {
            unreachable!("form filter always returns a typed form array")
        };
        Ok(ScriptValue::FormArray(
            values
                .into_iter()
                .map(|value| {
                    let StorageUtilListValue::Form(value) = value else {
                        unreachable!("form filter result is homogeneous")
                    };
                    value
                })
                .collect(),
        ))
    }

    fn invoke_storage_util_list(
        &mut self,
        principal: Option<&PrincipalId>,
        qualified_name: &str,
        kind: StorageUtilListKind,
        operation: StorageUtilListOperation,
        arguments: &[ScriptValue],
    ) -> Result<ScriptValue, ExtensionHostError> {
        if matches!(
            operation,
            StorageUtilListOperation::FilterByType | StorageUtilListOperation::FilterByTypes
        ) {
            return self.invoke_storage_util_form_filter(
                principal,
                qualified_name,
                operation,
                arguments,
            );
        }
        let unavailable = |reason: String| ExtensionHostError::ScriptFunctionUnavailable {
            function: qualified_name.to_owned(),
            reason,
        };
        let principal = principal.ok_or_else(|| {
            unavailable("StorageUtil call has no authenticated legacy-script principal".to_owned())
        })?;
        let integer = |value: i64| {
            i32::try_from(value).map_err(|_| {
                unavailable(
                    "StorageUtil integer argument is outside the Papyrus i32 range".to_owned(),
                )
            })
        };
        let list_value = |value: &ScriptValue| match (kind, value) {
            (StorageUtilListKind::Int, ScriptValue::Integer(value)) => {
                Ok(StorageUtilListValue::Int(integer(*value)?))
            }
            (StorageUtilListKind::Float, ScriptValue::Float(value)) => {
                Ok(StorageUtilListValue::Float(*value))
            }
            (StorageUtilListKind::String, ScriptValue::String(value)) => {
                Ok(StorageUtilListValue::String(value.clone()))
            }
            (StorageUtilListKind::Form, ScriptValue::None) => Ok(StorageUtilListValue::Form(None)),
            (StorageUtilListKind::Form, ScriptValue::Form(value)) => {
                Ok(StorageUtilListValue::Form(Some(*value)))
            }
            _ => Err(unavailable(
                "StorageUtil list call has an invalid typed value".to_owned(),
            )),
        };
        let list_values = |value: &ScriptValue| match (kind, value) {
            (StorageUtilListKind::Int, ScriptValue::IntegerArray(values)) => values
                .iter()
                .map(|value| Ok(StorageUtilListValue::Int(integer(*value)?)))
                .collect::<Result<Vec<_>, ExtensionHostError>>(),
            (StorageUtilListKind::Float, ScriptValue::FloatArray(values)) => Ok(values
                .iter()
                .copied()
                .map(StorageUtilListValue::Float)
                .collect()),
            (StorageUtilListKind::String, ScriptValue::StringArray(values)) => Ok(values
                .iter()
                .cloned()
                .map(StorageUtilListValue::String)
                .collect()),
            (StorageUtilListKind::Form, ScriptValue::FormArray(values)) => Ok(values
                .iter()
                .copied()
                .map(StorageUtilListValue::Form)
                .collect()),
            _ => Err(unavailable(
                "StorageUtil list call has an invalid typed array".to_owned(),
            )),
        };
        let default_value = || match kind {
            StorageUtilListKind::Int => StorageUtilListValue::Int(0),
            StorageUtilListKind::Float => StorageUtilListValue::Float(0.0),
            StorageUtilListKind::String => StorageUtilListValue::String(String::new()),
            StorageUtilListKind::Form => StorageUtilListValue::Form(None),
        };
        let (key_name, call) = match (operation, arguments) {
            (
                StorageUtilListOperation::Add,
                [ScriptValue::None, ScriptValue::String(key), value],
            ) => (
                key,
                StorageUtilListCall::Add {
                    value: list_value(value)?,
                    allow_duplicate: true,
                },
            ),
            (
                StorageUtilListOperation::Add,
                [ScriptValue::None, ScriptValue::String(key), value, ScriptValue::Boolean(allow_duplicate)],
            ) => (
                key,
                StorageUtilListCall::Add {
                    value: list_value(value)?,
                    allow_duplicate: *allow_duplicate,
                },
            ),
            (
                StorageUtilListOperation::Get,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(index)],
            ) => (
                key,
                StorageUtilListCall::Get {
                    index: integer(*index)?,
                },
            ),
            (
                StorageUtilListOperation::Set,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(index), value],
            ) => (
                key,
                StorageUtilListCall::Set {
                    index: integer(*index)?,
                    value: list_value(value)?,
                },
            ),
            (
                StorageUtilListOperation::Pluck,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(index)],
            ) => (
                key,
                StorageUtilListCall::Pluck {
                    index: integer(*index)?,
                    missing: default_value(),
                },
            ),
            (
                StorageUtilListOperation::Pluck,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(index), missing],
            ) => (
                key,
                StorageUtilListCall::Pluck {
                    index: integer(*index)?,
                    missing: list_value(missing)?,
                },
            ),
            (StorageUtilListOperation::Shift, [ScriptValue::None, ScriptValue::String(key)]) => {
                (key, StorageUtilListCall::Shift)
            }
            (StorageUtilListOperation::Pop, [ScriptValue::None, ScriptValue::String(key)]) => {
                (key, StorageUtilListCall::Pop)
            }
            (StorageUtilListOperation::Random, [ScriptValue::None, ScriptValue::String(key)]) => (
                key,
                StorageUtilListCall::Random {
                    selector: self.next_legacy_random_selector(),
                },
            ),
            (StorageUtilListOperation::Count, [ScriptValue::None, ScriptValue::String(key)]) => {
                (key, StorageUtilListCall::Count)
            }
            (StorageUtilListOperation::Clear, [ScriptValue::None, ScriptValue::String(key)]) => {
                (key, StorageUtilListCall::Clear)
            }
            (
                StorageUtilListOperation::RemoveAt,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(index)],
            ) => (
                key,
                StorageUtilListCall::RemoveAt {
                    index: integer(*index)?,
                },
            ),
            (
                StorageUtilListOperation::Insert,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(index), value],
            ) => (
                key,
                StorageUtilListCall::Insert {
                    index: integer(*index)?,
                    value: list_value(value)?,
                },
            ),
            (
                StorageUtilListOperation::Remove,
                [ScriptValue::None, ScriptValue::String(key), value],
            ) => (
                key,
                StorageUtilListCall::Remove {
                    value: list_value(value)?,
                    all_instances: false,
                },
            ),
            (
                StorageUtilListOperation::Remove,
                [ScriptValue::None, ScriptValue::String(key), value, ScriptValue::Boolean(all_instances)],
            ) => (
                key,
                StorageUtilListCall::Remove {
                    value: list_value(value)?,
                    all_instances: *all_instances,
                },
            ),
            (
                StorageUtilListOperation::CountValue,
                [ScriptValue::None, ScriptValue::String(key), value],
            ) => (
                key,
                StorageUtilListCall::CountValue {
                    value: list_value(value)?,
                    exclude: false,
                },
            ),
            (
                StorageUtilListOperation::CountValue,
                [ScriptValue::None, ScriptValue::String(key), value, ScriptValue::Boolean(exclude)],
            ) => (
                key,
                StorageUtilListCall::CountValue {
                    value: list_value(value)?,
                    exclude: *exclude,
                },
            ),
            (
                StorageUtilListOperation::Adjust,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(index), amount],
            ) => (
                key,
                StorageUtilListCall::Adjust {
                    index: integer(*index)?,
                    amount: list_value(amount)?,
                },
            ),
            (StorageUtilListOperation::Sort, [ScriptValue::None, ScriptValue::String(key)]) => {
                (key, StorageUtilListCall::Sort)
            }
            (
                StorageUtilListOperation::Copy,
                [ScriptValue::None, ScriptValue::String(key), values],
            ) => (
                key,
                StorageUtilListCall::Copy {
                    values: list_values(values)?,
                },
            ),
            (
                StorageUtilListOperation::Slice,
                [ScriptValue::None, ScriptValue::String(key), values],
            ) => (
                key,
                StorageUtilListCall::Slice {
                    values: list_values(values)?,
                    start_index: 0,
                },
            ),
            (
                StorageUtilListOperation::Slice,
                [ScriptValue::None, ScriptValue::String(key), values, ScriptValue::Integer(start_index)],
            ) => (
                key,
                StorageUtilListCall::Slice {
                    values: list_values(values)?,
                    start_index: integer(*start_index)?,
                },
            ),
            (StorageUtilListOperation::ToArray, [ScriptValue::None, ScriptValue::String(key)]) => {
                (key, StorageUtilListCall::ToArray)
            }
            (
                StorageUtilListOperation::Resize,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(to_length)],
            ) => (
                key,
                StorageUtilListCall::Resize {
                    to_length: integer(*to_length)?,
                    filler: default_value(),
                },
            ),
            (
                StorageUtilListOperation::Resize,
                [ScriptValue::None, ScriptValue::String(key), ScriptValue::Integer(to_length), filler],
            ) => (
                key,
                StorageUtilListCall::Resize {
                    to_length: integer(*to_length)?,
                    filler: list_value(filler)?,
                },
            ),
            (
                StorageUtilListOperation::Find,
                [ScriptValue::None, ScriptValue::String(key), value],
            ) => (
                key,
                StorageUtilListCall::Find {
                    value: list_value(value)?,
                },
            ),
            (
                StorageUtilListOperation::Has,
                [ScriptValue::None, ScriptValue::String(key), value],
            ) => (
                key,
                StorageUtilListCall::Has {
                    value: list_value(value)?,
                },
            ),
            _ => {
                return Err(unavailable(
                    "StorageUtil list call requires ObjKey=None and exact typed arguments"
                        .to_owned(),
                ));
            }
        };
        let max_entries = self.principal_storage.limits().max_collection_entries;
        let probe = adapt_storage_util_global_list(key_name, kind, call.clone(), None, max_entries)
            .map_err(|error| unavailable(error.to_string()))?;
        let current = self
            .principal_storage
            .values(principal)
            .and_then(|values| values.get(&probe.key))
            .cloned();
        let adaptation =
            adapt_storage_util_global_list(key_name, kind, call, current.as_ref(), max_entries)
                .map_err(|error| unavailable(error.to_string()))?;
        if !adaptation.commands.is_empty() {
            self.principal_storage
                .apply_batch(principal, &adaptation.commands)?;
        }
        Ok(match adaptation.result {
            StorageUtilListResult::None => ScriptValue::None,
            StorageUtilListResult::Int(value) => ScriptValue::Integer(i64::from(value)),
            StorageUtilListResult::Bool(value) => ScriptValue::Boolean(value),
            StorageUtilListResult::Value(StorageUtilListValue::Int(value)) => {
                ScriptValue::Integer(i64::from(value))
            }
            StorageUtilListResult::Value(StorageUtilListValue::Float(value)) => {
                ScriptValue::Float(value)
            }
            StorageUtilListResult::Value(StorageUtilListValue::String(value)) => {
                ScriptValue::String(value)
            }
            StorageUtilListResult::Value(StorageUtilListValue::Form(Some(value))) => {
                ScriptValue::Form(value)
            }
            StorageUtilListResult::Value(StorageUtilListValue::Form(None)) => ScriptValue::None,
            StorageUtilListResult::Array(values) => match kind {
                StorageUtilListKind::Int => ScriptValue::IntegerArray(
                    values
                        .into_iter()
                        .map(|value| {
                            let StorageUtilListValue::Int(value) = value else {
                                unreachable!("decoded StorageUtil Int array is homogeneous")
                            };
                            i64::from(value)
                        })
                        .collect(),
                ),
                StorageUtilListKind::Float => ScriptValue::FloatArray(
                    values
                        .into_iter()
                        .map(|value| {
                            let StorageUtilListValue::Float(value) = value else {
                                unreachable!("decoded StorageUtil Float array is homogeneous")
                            };
                            value
                        })
                        .collect(),
                ),
                StorageUtilListKind::String => ScriptValue::StringArray(
                    values
                        .into_iter()
                        .map(|value| {
                            let StorageUtilListValue::String(value) = value else {
                                unreachable!("decoded StorageUtil String array is homogeneous")
                            };
                            value
                        })
                        .collect(),
                ),
                StorageUtilListKind::Form => ScriptValue::FormArray(
                    values
                        .into_iter()
                        .map(|value| {
                            let StorageUtilListValue::Form(value) = value else {
                                unreachable!("decoded StorageUtil Form array is homogeneous")
                            };
                            value
                        })
                        .collect(),
                ),
            },
        })
    }

    pub(super) fn invoke_legacy_container(
        &mut self,
        principal: Option<&PrincipalId>,
        qualified_name: &str,
        arguments: &[ScriptValue],
    ) -> Result<ScriptValue, ExtensionHostError> {
        let unavailable = |reason: String| ExtensionHostError::ScriptFunctionUnavailable {
            function: qualified_name.to_owned(),
            reason,
        };
        let principal = principal.ok_or_else(|| {
            unavailable("JContainers call has no authenticated legacy-script principal".to_owned())
        })?;
        let operation = qualified_name
            .strip_prefix(PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX)
            .ok_or_else(|| unavailable("invalid JContainers engine route".to_owned()))?;
        let integer = |value: i64| {
            i32::try_from(value).map_err(|_| {
                unavailable("JContainers integer is outside the Papyrus i32 range".to_owned())
            })
        };
        let registry = self.legacy_containers.get_mut(principal).ok_or_else(|| {
            unavailable("JContainers principal has no registered private registry".to_owned())
        })?;

        let value_from_script = |kind: &str, value: &ScriptValue| {
            Ok(match (kind, value) {
                ("int", ScriptValue::Integer(value)) => LegacyContainerValue::Int(integer(*value)?),
                ("flt", ScriptValue::Float(value)) => {
                    LegacyContainerValue::FloatBits(value.to_bits())
                }
                ("str", ScriptValue::String(value)) => LegacyContainerValue::String(value.clone()),
                ("form", ScriptValue::Form(value)) => LegacyContainerValue::Form(Some(*value)),
                ("form", ScriptValue::None) => LegacyContainerValue::Form(None),
                ("obj", ScriptValue::Integer(value)) => {
                    LegacyContainerValue::Object(integer(*value)?)
                }
                _ => {
                    return Err(unavailable(
                        "JContainers value has the wrong exact type".to_owned(),
                    ))
                }
            })
        };
        let default_value = |kind: &str, value: Option<&ScriptValue>| {
            Ok(match (kind, value) {
                ("int", None) => ScriptValue::Integer(0),
                ("flt", None) => ScriptValue::Float(0.0),
                ("str", None) => ScriptValue::String(String::new()),
                ("form", None) | ("form", Some(ScriptValue::None)) => ScriptValue::None,
                ("obj", None) => ScriptValue::Integer(0),
                ("int", Some(ScriptValue::Integer(value))) => {
                    ScriptValue::Integer(i64::from(integer(*value)?))
                }
                ("flt", Some(ScriptValue::Float(value))) => ScriptValue::Float(*value),
                ("str", Some(ScriptValue::String(value))) => ScriptValue::String(value.clone()),
                ("form", Some(ScriptValue::Form(value))) => ScriptValue::Form(*value),
                ("obj", Some(ScriptValue::Integer(value))) => {
                    ScriptValue::Integer(i64::from(integer(*value)?))
                }
                _ => {
                    return Err(unavailable(
                        "JContainers default has the wrong exact type".to_owned(),
                    ))
                }
            })
        };
        let value_to_script = |kind: &str,
                               value: Option<&LegacyContainerValue>,
                               default: ScriptValue| match (
            kind, value,
        ) {
            ("int", Some(LegacyContainerValue::Int(value))) => {
                ScriptValue::Integer(i64::from(*value))
            }
            ("int", Some(LegacyContainerValue::FloatBits(value))) => {
                ScriptValue::Integer(i64::from(f32::from_bits(*value) as i32))
            }
            ("flt", Some(LegacyContainerValue::FloatBits(value))) => {
                ScriptValue::Float(f32::from_bits(*value))
            }
            ("flt", Some(LegacyContainerValue::Int(value))) => ScriptValue::Float(*value as f32),
            ("str", Some(LegacyContainerValue::String(value))) => {
                ScriptValue::String(value.clone())
            }
            ("form", Some(LegacyContainerValue::Form(Some(value)))) => ScriptValue::Form(*value),
            ("form", Some(LegacyContainerValue::Form(None))) => ScriptValue::None,
            ("obj", Some(LegacyContainerValue::Object(value))) => {
                ScriptValue::Integer(i64::from(*value))
            }
            _ => default,
        };
        let read_index = |handle: i32, index: i32, registry: &LegacyContainerRegistry| {
            if index >= 0 {
                index
            } else {
                registry.count(handle).saturating_add(index)
            }
        };
        let write_index = |handle: i32, index: i32, registry: &LegacyContainerRegistry| {
            if index == -1 {
                None
            } else {
                let index = if index >= 0 {
                    index
                } else {
                    registry
                        .count(handle)
                        .saturating_add(index)
                        .saturating_add(1)
                };
                u32::try_from(index).ok()
            }
        };

        let result = match (operation, arguments) {
            ("jarray-object", []) => ScriptValue::Integer(i64::from(registry.create_array())),
            ("jmap-object", []) => ScriptValue::Integer(i64::from(registry.create_map())),
            ("jvalue-is-exists", [ScriptValue::Integer(handle)]) => {
                ScriptValue::Boolean(registry.contains(integer(*handle)?))
            }
            ("jvalue-is-array", [ScriptValue::Integer(handle)]) => {
                ScriptValue::Boolean(registry.is_array(integer(*handle)?))
            }
            ("jvalue-is-map", [ScriptValue::Integer(handle)]) => {
                ScriptValue::Boolean(registry.is_map(integer(*handle)?))
            }
            ("jvalue-empty", [ScriptValue::Integer(handle)]) => {
                ScriptValue::Boolean(registry.is_empty(integer(*handle)?))
            }
            ("jvalue-count" | "jarray-count" | "jmap-count", [ScriptValue::Integer(handle)]) => {
                ScriptValue::Integer(i64::from(registry.count(integer(*handle)?)))
            }
            ("jvalue-clear" | "jarray-clear" | "jmap-clear", [ScriptValue::Integer(handle)]) => {
                registry.clear(integer(*handle)?);
                ScriptValue::None
            }
            ("jvalue-shallow-copy", [ScriptValue::Integer(handle)]) => {
                ScriptValue::Integer(i64::from(registry.shallow_copy(integer(*handle)?)))
            }
            ("jvalue-deep-copy", [ScriptValue::Integer(handle)]) => {
                ScriptValue::Integer(i64::from(registry.deep_copy(integer(*handle)?)))
            }
            ("jvalue-retain", [ScriptValue::Integer(handle)]) => {
                ScriptValue::Integer(i64::from(registry.retain(integer(*handle)?, None)))
            }
            ("jvalue-retain", [ScriptValue::Integer(handle), ScriptValue::String(tag)]) => {
                ScriptValue::Integer(i64::from(registry.retain(integer(*handle)?, Some(tag))))
            }
            ("jvalue-release", [ScriptValue::Integer(handle)]) => {
                registry.release(integer(*handle)?);
                ScriptValue::Integer(0)
            }
            (
                "jvalue-release-and-retain",
                [ScriptValue::Integer(previous), ScriptValue::Integer(new)],
            ) => ScriptValue::Integer(i64::from(registry.release_and_retain(
                integer(*previous)?,
                integer(*new)?,
                None,
            ))),
            (
                "jvalue-release-and-retain",
                [ScriptValue::Integer(previous), ScriptValue::Integer(new), ScriptValue::String(tag)],
            ) => ScriptValue::Integer(i64::from(registry.release_and_retain(
                integer(*previous)?,
                integer(*new)?,
                Some(tag),
            ))),
            ("jvalue-release-objects-with-tag", [ScriptValue::String(tag)]) => {
                registry.release_objects_with_tag(tag);
                ScriptValue::None
            }
            ("jarray-erase-index", [ScriptValue::Integer(handle), ScriptValue::Integer(index)]) => {
                let handle = integer(*handle)?;
                registry.array_erase(handle, read_index(handle, integer(*index)?, registry));
                ScriptValue::None
            }
            ("jmap-has-key", [ScriptValue::Integer(handle), ScriptValue::String(key)]) => {
                ScriptValue::Boolean(
                    !key.is_empty() && registry.map_has_key(integer(*handle)?, key),
                )
            }
            ("jmap-remove-key", [ScriptValue::Integer(handle), ScriptValue::String(key)]) => {
                ScriptValue::Boolean(!key.is_empty() && registry.map_remove(integer(*handle)?, key))
            }
            (operation, [ScriptValue::Integer(handle), value])
                if operation.starts_with("jarray-add-") =>
            {
                let handle = integer(*handle)?;
                let kind = operation.trim_start_matches("jarray-add-");
                registry.array_add(handle, value_from_script(kind, value)?, None);
                ScriptValue::None
            }
            (operation, [ScriptValue::Integer(handle), value, ScriptValue::Integer(index)])
                if operation.starts_with("jarray-add-") =>
            {
                let handle = integer(*handle)?;
                let kind = operation.trim_start_matches("jarray-add-");
                if let Some(index) = write_index(handle, integer(*index)?, registry) {
                    registry.array_add(handle, value_from_script(kind, value)?, Some(index));
                } else if *index == -1 {
                    registry.array_add(handle, value_from_script(kind, value)?, None);
                }
                ScriptValue::None
            }
            (operation, [ScriptValue::Integer(handle), ScriptValue::Integer(index), rest @ ..])
                if operation.starts_with("jarray-get-") && rest.len() <= 1 =>
            {
                let handle = integer(*handle)?;
                let kind = operation.trim_start_matches("jarray-get-");
                let default = default_value(kind, rest.first())?;
                let index = read_index(handle, integer(*index)?, registry);
                value_to_script(kind, registry.array_get(handle, index), default)
            }
            (operation, [ScriptValue::Integer(handle), ScriptValue::Integer(index), value])
                if operation.starts_with("jarray-set-") =>
            {
                let handle = integer(*handle)?;
                let kind = operation.trim_start_matches("jarray-set-");
                let index = read_index(handle, integer(*index)?, registry);
                registry.array_set(handle, index, value_from_script(kind, value)?);
                ScriptValue::None
            }
            (operation, [ScriptValue::Integer(handle), ScriptValue::String(key), rest @ ..])
                if operation.starts_with("jmap-get-") && rest.len() <= 1 =>
            {
                let handle = integer(*handle)?;
                let kind = operation.trim_start_matches("jmap-get-");
                let default = default_value(kind, rest.first())?;
                let value = (!key.is_empty())
                    .then(|| registry.map_get(handle, key))
                    .flatten();
                value_to_script(kind, value, default)
            }
            (operation, [ScriptValue::Integer(handle), ScriptValue::String(key), value])
                if operation.starts_with("jmap-set-") =>
            {
                let kind = operation.trim_start_matches("jmap-set-");
                if !key.is_empty() {
                    registry.map_set(
                        integer(*handle)?,
                        key.clone(),
                        value_from_script(kind, value)?,
                    );
                }
                ScriptValue::None
            }
            _ => {
                return Err(unavailable(
                    "JContainers alias received invalid exact typed arguments".to_owned(),
                ));
            }
        };
        Ok(result)
    }
}
