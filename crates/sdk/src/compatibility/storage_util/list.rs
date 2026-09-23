//! StorageUtil list and prefix surface: route parsing, the per-verb list
//! operations behind `adapt_storage_util_global_list`, the form filters and
//! the list value codecs (split from `storage_util.rs`, #4768).

use super::*;

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

/// PapyrusUtil caps a single `Resize` at 500 entries regardless of the
/// engine's own list ceiling. Hoisted out of `adapt_storage_util_global_list`
/// under #4218 so the extracted `resize` verb can still see it.
const PAPYRUS_UTIL_LIST_RESIZE_LIMIT: usize = 500;

/// One list verb's working set: the decoded list plus the context every
/// verb needs to answer with.
///
/// #4218 — `adapt_storage_util_global_list` was a 384-line, 21-arm match
/// whose arms each carry genuinely distinct logic, so this is a "one
/// function per verb" case rather than a lookup table. The arms are now
/// methods here, behind a thin dispatcher.
///
/// `values` is owned rather than borrowed because `ToArray` returns the
/// decoded list itself; a borrow would force a clone on the one verb whose
/// whole job is handing the list back. Each verb consumes `self`, which is
/// sound because the match arms are mutually exclusive.
///
/// `stored` is deliberately not named `current`: it is the *raw stored
/// value* — whether the key exists at all, which `Sort` and `Resize` check
/// before emitting a write — whereas `Adjust` has a local `current` meaning
/// the list element being adjusted. Conflating those two names corrupted
/// the first attempt at this extraction.
struct ListOp<'a> {
    kind: StorageUtilListKind,
    key: &'a StorageKey,
    values: Vec<StorageUtilListValue>,
    stored: Option<&'a PrincipalStorageValue>,
    max_entries: usize,
}

impl ListOp<'_> {
    fn add(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
        value: StorageUtilListValue,
        allow_duplicate: bool,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            let encoded = encode_storage_util_list_value(self.kind, &value)?;
            if self.values.len() >= self.max_entries
                || (!allow_duplicate && self.values.contains(&value))
            {
                StorageUtilListResult::Int(-1)
            } else {
                let index = i32::try_from(self.values.len())
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?;
                commands.push(PrincipalStorageCommand::ArrayPush {
                    key: self.key.clone(),
                    value: encoded,
                });
                StorageUtilListResult::Int(index)
            }
        })
    }

    fn get(self, index: i32) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            let value = usize::try_from(index)
                .ok()
                .and_then(|index| self.values.get(index))
                .cloned()
                .unwrap_or_else(|| default_storage_util_list_value(self.kind));
            StorageUtilListResult::Value(value)
        })
    }

    fn set(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
        index: i32,
        value: StorageUtilListValue,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            let encoded = encode_storage_util_list_value(self.kind, &value)?;
            let Some((index, previous)) = usize::try_from(index)
                .ok()
                .and_then(|index| self.values.get(index).cloned().map(|value| (index, value)))
            else {
                return Ok(StorageUtilListResult::Value(
                    default_storage_util_list_value(self.kind),
                ));
            };
            commands.push(PrincipalStorageCommand::ArraySet {
                key: self.key.clone(),
                index: u32::try_from(index)
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
                value: encoded,
            });
            StorageUtilListResult::Value(previous)
        })
    }

    fn pluck(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
        index: i32,
        missing: StorageUtilListValue,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            encode_storage_util_list_value(self.kind, &missing)?;
            let Some((index, value)) = usize::try_from(index)
                .ok()
                .and_then(|index| self.values.get(index).cloned().map(|value| (index, value)))
            else {
                return Ok(StorageUtilListResult::Value(missing));
            };
            commands.push(PrincipalStorageCommand::ArrayRemove {
                key: self.key.clone(),
                index: u32::try_from(index)
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
            });
            StorageUtilListResult::Value(value)
        })
    }

    fn shift(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            let value = self
                .values
                .first()
                .cloned()
                .unwrap_or_else(|| default_storage_util_list_value(self.kind));
            if !self.values.is_empty() {
                commands.push(PrincipalStorageCommand::ArrayRemove {
                    key: self.key.clone(),
                    index: 0,
                });
            }
            StorageUtilListResult::Value(value)
        })
    }

    fn pop(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            let Some((index, value)) = self
                .values
                .len()
                .checked_sub(1)
                .map(|index| (index, self.values[index].clone()))
            else {
                return Ok(StorageUtilListResult::Value(
                    default_storage_util_list_value(self.kind),
                ));
            };
            commands.push(PrincipalStorageCommand::ArrayRemove {
                key: self.key.clone(),
                index: u32::try_from(index)
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
            });
            StorageUtilListResult::Value(value)
        })
    }

    fn random(self, selector: u64) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            let value = if self.values.is_empty() {
                default_storage_util_list_value(self.kind)
            } else {
                let index = (selector % self.values.len() as u64) as usize;
                self.values[index].clone()
            };
            StorageUtilListResult::Value(value)
        })
    }

    fn count(self) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            StorageUtilListResult::Int(
                i32::try_from(self.values.len())
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
            )
        })
    }

    fn clear(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            let count = i32::try_from(self.values.len())
                .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?;
            commands.push(PrincipalStorageCommand::Delete {
                key: self.key.clone(),
            });
            StorageUtilListResult::Int(count)
        })
    }

    fn remove_at(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
        index: i32,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            let Some(index) = usize::try_from(index)
                .ok()
                .filter(|index| *index < self.values.len())
            else {
                return Ok(StorageUtilListResult::Bool(false));
            };
            commands.push(PrincipalStorageCommand::ArrayRemove {
                key: self.key.clone(),
                index: u32::try_from(index)
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
            });
            StorageUtilListResult::Bool(true)
        })
    }

    fn insert(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
        index: i32,
        value: StorageUtilListValue,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            let Some(index) = usize::try_from(index)
                .ok()
                .filter(|index| *index <= self.values.len())
            else {
                return Ok(StorageUtilListResult::Bool(false));
            };
            encode_storage_util_list_value(self.kind, &value)?;
            if self.values.len() >= self.max_entries {
                StorageUtilListResult::Bool(false)
            } else {
                let mut replacement = self.values.clone();
                replacement.insert(index, value);
                commands.push(PrincipalStorageCommand::ArrayReplace {
                    key: self.key.clone(),
                    values: encode_storage_util_list_values(self.kind, &replacement)?,
                });
                StorageUtilListResult::Bool(true)
            }
        })
    }

    fn remove(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
        value: StorageUtilListValue,
        all_instances: bool,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            encode_storage_util_list_value(self.kind, &value)?;
            let mut replacement = self.values.clone();
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
                    key: self.key.clone(),
                    values: encode_storage_util_list_values(self.kind, &replacement)?,
                });
            }
            StorageUtilListResult::Int(
                i32::try_from(removed).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
            )
        })
    }

    fn count_value(
        self,
        value: StorageUtilListValue,
        exclude: bool,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            encode_storage_util_list_value(self.kind, &value)?;
            let count = self
                .values
                .iter()
                .filter(|candidate| (*candidate == &value) != exclude)
                .count();
            StorageUtilListResult::Int(
                i32::try_from(count).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
            )
        })
    }

    fn adjust(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
        index: i32,
        amount: StorageUtilListValue,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            encode_storage_util_list_value(self.kind, &amount)?;
            let Some((index, current)) = usize::try_from(index)
                .ok()
                .and_then(|index| self.values.get(index).cloned().map(|value| (index, value)))
            else {
                return Ok(StorageUtilListResult::Value(
                    default_storage_util_list_value(self.kind),
                ));
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
                key: self.key.clone(),
                index: u32::try_from(index)
                    .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?,
                value: encode_storage_util_list_value(self.kind, &next)?,
            });
            StorageUtilListResult::Value(next)
        })
    }

    fn sort(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            let mut replacement = self.values.clone();
            match self.kind {
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
            if self.stored.is_some() {
                commands.push(PrincipalStorageCommand::ArrayReplace {
                    key: self.key.clone(),
                    values: encode_storage_util_list_values(self.kind, &replacement)?,
                });
            }
            StorageUtilListResult::None
        })
    }

    fn resize(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
        to_length: i32,
        filler: StorageUtilListValue,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            encode_storage_util_list_value(self.kind, &filler)?;
            let Some(target) = usize::try_from(to_length).ok().filter(|target| {
                *target <= PAPYRUS_UTIL_LIST_RESIZE_LIMIT && *target <= self.max_entries
            }) else {
                return Ok(StorageUtilListResult::Int(0));
            };
            let delta = i64::try_from(target)
                .and_then(|target| i64::try_from(self.values.len()).map(|length| target - length))
                .map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?;
            let delta =
                i32::try_from(delta).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)?;
            if target != self.values.len() {
                if target == 0 {
                    if self.stored.is_some() {
                        commands.push(PrincipalStorageCommand::Delete {
                            key: self.key.clone(),
                        });
                    }
                } else {
                    let mut replacement = self.values.clone();
                    replacement.resize(target, filler);
                    commands.push(PrincipalStorageCommand::ArrayReplace {
                        key: self.key.clone(),
                        values: encode_storage_util_list_values(self.kind, &replacement)?,
                    });
                }
            }
            StorageUtilListResult::Int(delta)
        })
    }

    fn copy(
        self,
        commands: &mut Vec<PrincipalStorageCommand>,
        replacement: Vec<StorageUtilListValue>,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            if replacement.len() > self.max_entries {
                StorageUtilListResult::Bool(false)
            } else {
                commands.push(PrincipalStorageCommand::ArrayReplace {
                    key: self.key.clone(),
                    values: encode_storage_util_list_values(self.kind, &replacement)?,
                });
                StorageUtilListResult::Bool(true)
            }
        })
    }

    fn slice(
        self,
        mut replacement: Vec<StorageUtilListValue>,
        start_index: i32,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            encode_storage_util_list_values(self.kind, &replacement)?;
            if let Ok(start) = usize::try_from(start_index) {
                for (offset, target) in replacement.iter_mut().enumerate() {
                    let Some(source) = self.values.get(start.saturating_add(offset)) else {
                        break;
                    };
                    *target = source.clone();
                }
            }
            StorageUtilListResult::Array(replacement)
        })
    }

    /// Consumes `self`: the decoded list is handed back as-is, which is
    /// the one verb where borrowing would force a clone.
    fn into_array(self) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok(StorageUtilListResult::Array(self.values))
    }

    fn find(
        self,
        value: StorageUtilListValue,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            encode_storage_util_list_value(self.kind, &value)?;
            let index = self
                .values
                .iter()
                .position(|candidate| candidate == &value)
                .map_or(Ok(-1), |index| {
                    i32::try_from(index).map_err(|_| StorageUtilAdapterError::IntegerOutOfRange)
                })?;
            StorageUtilListResult::Int(index)
        })
    }

    fn has(
        self,
        value: StorageUtilListValue,
    ) -> Result<StorageUtilListResult, StorageUtilAdapterError> {
        Ok({
            encode_storage_util_list_value(self.kind, &value)?;
            StorageUtilListResult::Bool(self.values.contains(&value))
        })
    }
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
    let op = ListOp {
        kind,
        key: &key,
        values,
        stored: current,
        max_entries,
    };
    let result = match call {
        StorageUtilListCall::Add {
            value,
            allow_duplicate,
        } => op.add(&mut commands, value, allow_duplicate)?,
        StorageUtilListCall::Get { index } => op.get(index)?,
        StorageUtilListCall::Set { index, value } => op.set(&mut commands, index, value)?,
        StorageUtilListCall::Pluck { index, missing } => op.pluck(&mut commands, index, missing)?,
        StorageUtilListCall::Shift => op.shift(&mut commands)?,
        StorageUtilListCall::Pop => op.pop(&mut commands)?,
        StorageUtilListCall::Random { selector } => op.random(selector)?,
        StorageUtilListCall::Count => op.count()?,
        StorageUtilListCall::Clear => op.clear(&mut commands)?,
        StorageUtilListCall::RemoveAt { index } => op.remove_at(&mut commands, index)?,
        StorageUtilListCall::Insert { index, value } => op.insert(&mut commands, index, value)?,
        StorageUtilListCall::Remove {
            value,
            all_instances,
        } => op.remove(&mut commands, value, all_instances)?,
        StorageUtilListCall::CountValue { value, exclude } => op.count_value(value, exclude)?,
        StorageUtilListCall::Adjust { index, amount } => op.adjust(&mut commands, index, amount)?,
        StorageUtilListCall::Sort => op.sort(&mut commands)?,
        StorageUtilListCall::Resize { to_length, filler } => {
            op.resize(&mut commands, to_length, filler)?
        }
        StorageUtilListCall::Copy {
            values: replacement,
        } => op.copy(&mut commands, replacement)?,
        StorageUtilListCall::Slice {
            values: replacement,
            start_index,
        } => op.slice(replacement, start_index)?,
        StorageUtilListCall::ToArray => op.into_array()?,
        StorageUtilListCall::Find { value } => op.find(value)?,
        StorageUtilListCall::Has { value } => op.has(value)?,
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
