//! Bounded, principal-isolated persistent key/value storage.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::component::{ExtensionCommand, ExtensionValue};
use crate::event::PublishEventCommand;
use crate::identity::{PrincipalId, StorageKey};

/// Save-safe value in one principal's private storage namespace.
///
/// Collection elements intentionally remain primitive [`ExtensionValue`]s:
/// recursive guest-authored trees would make validation cost and save size
/// dependent on untrusted nesting depth.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum PrincipalStorageValue {
    Bool(bool),
    I64(i64),
    U64(u64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<ExtensionValue>),
    Map(BTreeMap<String, ExtensionValue>),
    Set(BTreeSet<ExtensionValue>),
}

fn collection_or_insert_array<'a>(
    values: &'a mut BTreeMap<StorageKey, PrincipalStorageValue>,
    key: &StorageKey,
) -> Result<&'a mut Vec<ExtensionValue>, PrincipalStorageError> {
    if !values.contains_key(key) {
        values.insert(key.clone(), PrincipalStorageValue::Array(Vec::new()));
    }
    match values.get_mut(key) {
        Some(PrincipalStorageValue::Array(values)) => Ok(values),
        _ => Err(PrincipalStorageError::CollectionTypeMismatch {
            key: key.clone(),
            expected: "array",
        }),
    }
}

fn collection_or_insert_map<'a>(
    values: &'a mut BTreeMap<StorageKey, PrincipalStorageValue>,
    key: &StorageKey,
) -> Result<&'a mut BTreeMap<String, ExtensionValue>, PrincipalStorageError> {
    if !values.contains_key(key) {
        values.insert(key.clone(), PrincipalStorageValue::Map(BTreeMap::new()));
    }
    match values.get_mut(key) {
        Some(PrincipalStorageValue::Map(values)) => Ok(values),
        _ => Err(PrincipalStorageError::CollectionTypeMismatch {
            key: key.clone(),
            expected: "map",
        }),
    }
}

fn collection_or_insert_set<'a>(
    values: &'a mut BTreeMap<StorageKey, PrincipalStorageValue>,
    key: &StorageKey,
) -> Result<&'a mut BTreeSet<ExtensionValue>, PrincipalStorageError> {
    if !values.contains_key(key) {
        values.insert(key.clone(), PrincipalStorageValue::Set(BTreeSet::new()));
    }
    match values.get_mut(key) {
        Some(PrincipalStorageValue::Set(values)) => Ok(values),
        _ => Err(PrincipalStorageError::CollectionTypeMismatch {
            key: key.clone(),
            expected: "set",
        }),
    }
}

fn extension_value_bytes(value: &ExtensionValue) -> Option<usize> {
    Some(match value {
        ExtensionValue::Bool(_) => 1,
        ExtensionValue::I64(_) | ExtensionValue::U64(_) => 8,
        ExtensionValue::String(value) => value.len(),
        ExtensionValue::Bytes(value) => value.len(),
    })
}

fn storage_value_bytes(value: &PrincipalStorageValue) -> Option<usize> {
    match value {
        PrincipalStorageValue::Bool(_) => Some(1),
        PrincipalStorageValue::I64(_) | PrincipalStorageValue::U64(_) => Some(8),
        PrincipalStorageValue::String(value) => Some(value.len()),
        PrincipalStorageValue::Bytes(value) => Some(value.len()),
        PrincipalStorageValue::Array(values) => values.iter().try_fold(0usize, |total, value| {
            total.checked_add(extension_value_bytes(value)?)
        }),
        PrincipalStorageValue::Map(values) => {
            values.iter().try_fold(0usize, |total, (key, value)| {
                total
                    .checked_add(key.len())
                    .and_then(|total| total.checked_add(extension_value_bytes(value)?))
            })
        }
        PrincipalStorageValue::Set(values) => values.iter().try_fold(0usize, |total, value| {
            total.checked_add(extension_value_bytes(value)?)
        }),
    }
}

fn storage_value_kind(value: &PrincipalStorageValue) -> &'static str {
    match value {
        PrincipalStorageValue::Bool(_) => "bool",
        PrincipalStorageValue::I64(_) => "i64",
        PrincipalStorageValue::U64(_) => "u64",
        PrincipalStorageValue::String(_) => "string",
        PrincipalStorageValue::Bytes(_) => "bytes",
        PrincipalStorageValue::Array(_) => "array",
        PrincipalStorageValue::Map(_) => "map",
        PrincipalStorageValue::Set(_) => "set",
    }
}

impl From<ExtensionValue> for PrincipalStorageValue {
    fn from(value: ExtensionValue) -> Self {
        match value {
            ExtensionValue::Bool(value) => Self::Bool(value),
            ExtensionValue::I64(value) => Self::I64(value),
            ExtensionValue::U64(value) => Self::U64(value),
            ExtensionValue::String(value) => Self::String(value),
            ExtensionValue::Bytes(value) => Self::Bytes(value),
        }
    }
}

impl PrincipalStorageValue {
    pub fn as_scalar(&self) -> Option<ExtensionValue> {
        Some(match self {
            Self::Bool(value) => ExtensionValue::Bool(*value),
            Self::I64(value) => ExtensionValue::I64(*value),
            Self::U64(value) => ExtensionValue::U64(*value),
            Self::String(value) => ExtensionValue::String(value.clone()),
            Self::Bytes(value) => ExtensionValue::Bytes(value.clone()),
            Self::Array(_) | Self::Map(_) | Self::Set(_) => return None,
        })
    }
}

/// One deferred operation against the authenticated principal's private map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrincipalStorageCommand {
    Set {
        key: StorageKey,
        value: ExtensionValue,
    },
    Delete {
        key: StorageKey,
    },
    IncrementI64 {
        key: StorageKey,
        delta: i64,
    },
    ArrayPush {
        key: StorageKey,
        value: ExtensionValue,
    },
    ArraySet {
        key: StorageKey,
        index: u32,
        value: ExtensionValue,
    },
    ArrayRemove {
        key: StorageKey,
        index: u32,
    },
    MapSet {
        key: StorageKey,
        entry: String,
        value: ExtensionValue,
    },
    MapDelete {
        key: StorageKey,
        entry: String,
    },
    SetInsert {
        key: StorageKey,
        value: ExtensionValue,
    },
    SetRemove {
        key: StorageKey,
        value: ExtensionValue,
    },
}

/// One mutation emitted by a sandbox callback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostCommand {
    Component(ExtensionCommand),
    PrincipalStorage(PrincipalStorageCommand),
    PublishEvent(PublishEventCommand),
}

/// Hard bounds for one engine-owned principal storage service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrincipalStorageLimits {
    pub max_principals: usize,
    pub max_entries_per_principal: usize,
    pub max_string_bytes: usize,
    pub max_blob_bytes: usize,
    pub max_collection_entries: usize,
    pub max_collection_key_bytes: usize,
    pub max_total_bytes_per_principal: usize,
}

impl Default for PrincipalStorageLimits {
    fn default() -> Self {
        Self {
            max_principals: 4_096,
            max_entries_per_principal: 4_096,
            max_string_bytes: 16 * 1024,
            max_blob_bytes: 64 * 1024,
            max_collection_entries: 4_096,
            max_collection_key_bytes: 1_024,
            max_total_bytes_per_principal: 4 * 1024 * 1024,
        }
    }
}

/// One principal's save-safe storage record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedPrincipalStorage {
    pub principal: PrincipalId,
    pub schema_version: u32,
    pub values: BTreeMap<StorageKey, PrincipalStorageValue>,
}

/// Rejection from schema registration, mutation, or restore.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PrincipalStorageError {
    #[error("{field} must be non-zero")]
    InvalidLimit { field: &'static str },
    #[error("{field} exceeds the u32 wire limit")]
    LimitExceedsWire { field: &'static str },
    #[error("principal storage schema version must be positive")]
    ZeroSchemaVersion,
    #[error("principal storage schema is already registered for {0}")]
    DuplicatePrincipal(PrincipalId),
    #[error("principal storage exceeds the principal limit of {maximum}")]
    PrincipalBudgetExceeded { maximum: usize },
    #[error("principal {0} did not declare persistent storage")]
    UndeclaredPrincipal(PrincipalId),
    #[error("principal {principal} storage is version {actual}, but saved state requires {saved}")]
    SchemaVersionMismatch {
        principal: PrincipalId,
        saved: u32,
        actual: u32,
    },
    #[error("saved state repeats principal storage for {0}")]
    DuplicatePersistedPrincipal(PrincipalId),
    #[error("principal {principal} exceeds the entry limit of {maximum}")]
    EntryBudgetExceeded {
        principal: PrincipalId,
        maximum: usize,
    },
    #[error("value for {key} exceeds its size limit of {maximum} bytes")]
    ValueTooLarge { key: StorageKey, maximum: usize },
    #[error("principal {principal} exceeds its total storage limit of {maximum} bytes")]
    TotalByteBudgetExceeded {
        principal: PrincipalId,
        maximum: usize,
    },
    #[error("storage key {key} is {actual}, expected i64")]
    TypeMismatch {
        key: StorageKey,
        actual: &'static str,
    },
    #[error("storage key {key} is not a {expected} collection")]
    CollectionTypeMismatch {
        key: StorageKey,
        expected: &'static str,
    },
    #[error("collection at {key} exceeds its entry limit of {maximum}")]
    CollectionEntryBudgetExceeded { key: StorageKey, maximum: usize },
    #[error("array index {index} is outside collection {key} with length {length}")]
    CollectionIndexOutOfBounds {
        key: StorageKey,
        index: u32,
        length: usize,
    },
    #[error("map entry key for {key} is invalid or exceeds {maximum} bytes")]
    InvalidCollectionKey { key: StorageKey, maximum: usize },
    #[error("integer overflow incrementing principal storage key {0}")]
    IntegerOverflow(StorageKey),
}

/// Engine-owned storage isolated by authenticated principal.
#[derive(Clone, Debug)]
pub struct PrincipalStorageStore {
    limits: PrincipalStorageLimits,
    schemas: BTreeMap<PrincipalId, u32>,
    values: BTreeMap<PrincipalId, BTreeMap<StorageKey, PrincipalStorageValue>>,
}

impl PrincipalStorageStore {
    pub fn new(limits: PrincipalStorageLimits) -> Result<Self, PrincipalStorageError> {
        for (field, value) in [
            ("max_principals", limits.max_principals),
            (
                "max_entries_per_principal",
                limits.max_entries_per_principal,
            ),
            ("max_string_bytes", limits.max_string_bytes),
            ("max_blob_bytes", limits.max_blob_bytes),
            ("max_collection_entries", limits.max_collection_entries),
            ("max_collection_key_bytes", limits.max_collection_key_bytes),
            (
                "max_total_bytes_per_principal",
                limits.max_total_bytes_per_principal,
            ),
        ] {
            if value == 0 {
                return Err(PrincipalStorageError::InvalidLimit { field });
            }
        }
        if limits.max_collection_entries > u32::MAX as usize {
            return Err(PrincipalStorageError::LimitExceedsWire {
                field: "max_collection_entries",
            });
        }
        Ok(Self {
            limits,
            schemas: BTreeMap::new(),
            values: BTreeMap::new(),
        })
    }

    pub fn register_schema(
        &mut self,
        principal: PrincipalId,
        version: u32,
    ) -> Result<(), PrincipalStorageError> {
        if version == 0 {
            return Err(PrincipalStorageError::ZeroSchemaVersion);
        }
        if self.schemas.contains_key(&principal) {
            return Err(PrincipalStorageError::DuplicatePrincipal(principal));
        }
        if self.schemas.len() >= self.limits.max_principals {
            return Err(PrincipalStorageError::PrincipalBudgetExceeded {
                maximum: self.limits.max_principals,
            });
        }
        self.schemas.insert(principal, version);
        Ok(())
    }

    pub fn schema_version(&self, principal: &PrincipalId) -> Option<u32> {
        self.schemas.get(principal).copied()
    }

    pub fn values(
        &self,
        principal: &PrincipalId,
    ) -> Option<&BTreeMap<StorageKey, PrincipalStorageValue>> {
        self.values.get(principal)
    }

    pub fn apply_batch(
        &mut self,
        principal: &PrincipalId,
        commands: &[PrincipalStorageCommand],
    ) -> Result<(), PrincipalStorageError> {
        if !self.schemas.contains_key(principal) {
            return Err(PrincipalStorageError::UndeclaredPrincipal(
                principal.clone(),
            ));
        }
        let mut staged = self.values.get(principal).cloned().unwrap_or_default();
        for command in commands {
            match command {
                PrincipalStorageCommand::Set { key, value } => {
                    self.validate_scalar(key, value)?;
                    staged.insert(key.clone(), value.clone().into());
                }
                PrincipalStorageCommand::Delete { key } => {
                    staged.remove(key);
                }
                PrincipalStorageCommand::IncrementI64 { key, delta } => {
                    let current = match staged.get(key) {
                        Some(PrincipalStorageValue::I64(value)) => *value,
                        Some(value) => {
                            return Err(PrincipalStorageError::TypeMismatch {
                                key: key.clone(),
                                actual: storage_value_kind(value),
                            });
                        }
                        None => 0,
                    };
                    let next = current
                        .checked_add(*delta)
                        .ok_or_else(|| PrincipalStorageError::IntegerOverflow(key.clone()))?;
                    staged.insert(key.clone(), PrincipalStorageValue::I64(next));
                }
                PrincipalStorageCommand::ArrayPush { key, value } => {
                    self.validate_scalar(key, value)?;
                    let array = collection_or_insert_array(&mut staged, key)?;
                    if array.len() >= self.limits.max_collection_entries {
                        return Err(PrincipalStorageError::CollectionEntryBudgetExceeded {
                            key: key.clone(),
                            maximum: self.limits.max_collection_entries,
                        });
                    }
                    array.push(value.clone());
                }
                PrincipalStorageCommand::ArraySet { key, index, value } => {
                    self.validate_scalar(key, value)?;
                    let Some(PrincipalStorageValue::Array(array)) = staged.get_mut(key) else {
                        return Err(PrincipalStorageError::CollectionTypeMismatch {
                            key: key.clone(),
                            expected: "array",
                        });
                    };
                    let length = array.len();
                    let slot = array.get_mut(*index as usize).ok_or_else(|| {
                        PrincipalStorageError::CollectionIndexOutOfBounds {
                            key: key.clone(),
                            index: *index,
                            length,
                        }
                    })?;
                    *slot = value.clone();
                }
                PrincipalStorageCommand::ArrayRemove { key, index } => {
                    let Some(PrincipalStorageValue::Array(array)) = staged.get_mut(key) else {
                        return Err(PrincipalStorageError::CollectionTypeMismatch {
                            key: key.clone(),
                            expected: "array",
                        });
                    };
                    if *index as usize >= array.len() {
                        return Err(PrincipalStorageError::CollectionIndexOutOfBounds {
                            key: key.clone(),
                            index: *index,
                            length: array.len(),
                        });
                    }
                    array.remove(*index as usize);
                }
                PrincipalStorageCommand::MapSet { key, entry, value } => {
                    self.validate_collection_key(key, entry)?;
                    self.validate_scalar(key, value)?;
                    let map = collection_or_insert_map(&mut staged, key)?;
                    if !map.contains_key(entry) && map.len() >= self.limits.max_collection_entries {
                        return Err(PrincipalStorageError::CollectionEntryBudgetExceeded {
                            key: key.clone(),
                            maximum: self.limits.max_collection_entries,
                        });
                    }
                    map.insert(entry.clone(), value.clone());
                }
                PrincipalStorageCommand::MapDelete { key, entry } => {
                    self.validate_collection_key(key, entry)?;
                    let Some(PrincipalStorageValue::Map(map)) = staged.get_mut(key) else {
                        return Err(PrincipalStorageError::CollectionTypeMismatch {
                            key: key.clone(),
                            expected: "map",
                        });
                    };
                    map.remove(entry);
                }
                PrincipalStorageCommand::SetInsert { key, value } => {
                    self.validate_scalar(key, value)?;
                    let set = collection_or_insert_set(&mut staged, key)?;
                    if !set.contains(value) && set.len() >= self.limits.max_collection_entries {
                        return Err(PrincipalStorageError::CollectionEntryBudgetExceeded {
                            key: key.clone(),
                            maximum: self.limits.max_collection_entries,
                        });
                    }
                    set.insert(value.clone());
                }
                PrincipalStorageCommand::SetRemove { key, value } => {
                    self.validate_scalar(key, value)?;
                    let Some(PrincipalStorageValue::Set(set)) = staged.get_mut(key) else {
                        return Err(PrincipalStorageError::CollectionTypeMismatch {
                            key: key.clone(),
                            expected: "set",
                        });
                    };
                    set.remove(value);
                }
            }
        }
        self.validate_map(principal, &staged)?;
        if staged.is_empty() {
            self.values.remove(principal);
        } else {
            self.values.insert(principal.clone(), staged);
        }
        Ok(())
    }

    pub fn replace_active(
        &mut self,
        records: impl IntoIterator<Item = PersistedPrincipalStorage>,
    ) -> Result<(), PrincipalStorageError> {
        let mut staged = self.clone();
        staged.values.clear();
        let mut principals = BTreeSet::new();
        for record in records {
            if !principals.insert(record.principal.clone()) {
                return Err(PrincipalStorageError::DuplicatePersistedPrincipal(
                    record.principal,
                ));
            }
            let actual = staged.schema_version(&record.principal).ok_or_else(|| {
                PrincipalStorageError::UndeclaredPrincipal(record.principal.clone())
            })?;
            if actual != record.schema_version {
                return Err(PrincipalStorageError::SchemaVersionMismatch {
                    principal: record.principal,
                    saved: record.schema_version,
                    actual,
                });
            }
            for (key, value) in &record.values {
                staged.validate_value(key, value)?;
            }
            staged.validate_map(&record.principal, &record.values)?;
            if !record.values.is_empty() {
                staged.values.insert(record.principal, record.values);
            }
        }
        self.values = staged.values;
        Ok(())
    }

    pub fn persisted(&self) -> Vec<PersistedPrincipalStorage> {
        self.schemas
            .iter()
            .filter_map(|(principal, version)| {
                self.values
                    .get(principal)
                    .map(|values| PersistedPrincipalStorage {
                        principal: principal.clone(),
                        schema_version: *version,
                        values: values.clone(),
                    })
            })
            .collect()
    }

    /// Validate one record's generic bounds without requiring its schema to
    /// be installed. Hosts use this to retain disabled-extension state safely.
    pub fn validate_record_bounds(
        &self,
        record: &PersistedPrincipalStorage,
    ) -> Result<(), PrincipalStorageError> {
        if record.schema_version == 0 {
            return Err(PrincipalStorageError::ZeroSchemaVersion);
        }
        for (key, value) in &record.values {
            self.validate_value(key, value)?;
        }
        self.validate_map(&record.principal, &record.values)
    }

    fn validate_scalar(
        &self,
        key: &StorageKey,
        value: &ExtensionValue,
    ) -> Result<(), PrincipalStorageError> {
        let maximum = match value {
            ExtensionValue::String(value) if value.len() > self.limits.max_string_bytes => {
                Some(self.limits.max_string_bytes)
            }
            ExtensionValue::Bytes(value) if value.len() > self.limits.max_blob_bytes => {
                Some(self.limits.max_blob_bytes)
            }
            _ => None,
        };
        if let Some(maximum) = maximum {
            return Err(PrincipalStorageError::ValueTooLarge {
                key: key.clone(),
                maximum,
            });
        }
        Ok(())
    }

    fn validate_collection_key(
        &self,
        key: &StorageKey,
        entry: &str,
    ) -> Result<(), PrincipalStorageError> {
        if entry.is_empty()
            || entry.len() > self.limits.max_collection_key_bytes
            || entry.chars().any(char::is_control)
        {
            return Err(PrincipalStorageError::InvalidCollectionKey {
                key: key.clone(),
                maximum: self.limits.max_collection_key_bytes,
            });
        }
        Ok(())
    }

    fn validate_value(
        &self,
        key: &StorageKey,
        value: &PrincipalStorageValue,
    ) -> Result<(), PrincipalStorageError> {
        match value {
            PrincipalStorageValue::Bool(value) => {
                self.validate_scalar(key, &ExtensionValue::Bool(*value))
            }
            PrincipalStorageValue::I64(value) => {
                self.validate_scalar(key, &ExtensionValue::I64(*value))
            }
            PrincipalStorageValue::U64(value) => {
                self.validate_scalar(key, &ExtensionValue::U64(*value))
            }
            PrincipalStorageValue::String(value) => {
                self.validate_scalar(key, &ExtensionValue::String(value.clone()))
            }
            PrincipalStorageValue::Bytes(value) => {
                self.validate_scalar(key, &ExtensionValue::Bytes(value.clone()))
            }
            PrincipalStorageValue::Array(values) => {
                self.validate_collection_len(key, values.len())?;
                for value in values {
                    self.validate_scalar(key, value)?;
                }
                Ok(())
            }
            PrincipalStorageValue::Map(values) => {
                self.validate_collection_len(key, values.len())?;
                for (entry, value) in values {
                    self.validate_collection_key(key, entry)?;
                    self.validate_scalar(key, value)?;
                }
                Ok(())
            }
            PrincipalStorageValue::Set(values) => {
                self.validate_collection_len(key, values.len())?;
                for value in values {
                    self.validate_scalar(key, value)?;
                }
                Ok(())
            }
        }
    }

    fn validate_collection_len(
        &self,
        key: &StorageKey,
        actual: usize,
    ) -> Result<(), PrincipalStorageError> {
        if actual > self.limits.max_collection_entries {
            Err(PrincipalStorageError::CollectionEntryBudgetExceeded {
                key: key.clone(),
                maximum: self.limits.max_collection_entries,
            })
        } else {
            Ok(())
        }
    }

    fn validate_map(
        &self,
        principal: &PrincipalId,
        values: &BTreeMap<StorageKey, PrincipalStorageValue>,
    ) -> Result<(), PrincipalStorageError> {
        if values.len() > self.limits.max_entries_per_principal {
            return Err(PrincipalStorageError::EntryBudgetExceeded {
                principal: principal.clone(),
                maximum: self.limits.max_entries_per_principal,
            });
        }
        let total = values.iter().try_fold(0usize, |total, (key, value)| {
            let value_bytes = storage_value_bytes(value).ok_or(
                PrincipalStorageError::TotalByteBudgetExceeded {
                    principal: principal.clone(),
                    maximum: self.limits.max_total_bytes_per_principal,
                },
            )?;
            total
                .checked_add(key.as_str().len())
                .and_then(|total| total.checked_add(value_bytes))
                .ok_or(PrincipalStorageError::TotalByteBudgetExceeded {
                    principal: principal.clone(),
                    maximum: self.limits.max_total_bytes_per_principal,
                })
        })?;
        if total > self.limits.max_total_bytes_per_principal {
            return Err(PrincipalStorageError::TotalByteBudgetExceeded {
                principal: principal.clone(),
                maximum: self.limits.max_total_bytes_per_principal,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal() -> PrincipalId {
        PrincipalId::new("org.example.storage").unwrap()
    }

    fn key(value: &str) -> StorageKey {
        StorageKey::new(value).unwrap()
    }

    #[test]
    fn principals_are_isolated_and_batches_are_atomic() {
        let mut store = PrincipalStorageStore::new(PrincipalStorageLimits::default()).unwrap();
        let a = principal();
        let b = PrincipalId::new("org.example.other").unwrap();
        store.register_schema(a.clone(), 1).unwrap();
        store.register_schema(b.clone(), 1).unwrap();
        store
            .apply_batch(
                &a,
                &[PrincipalStorageCommand::IncrementI64 {
                    key: key("counter"),
                    delta: i64::MAX,
                }],
            )
            .unwrap();
        let failed = store.apply_batch(
            &a,
            &[
                PrincipalStorageCommand::Set {
                    key: key("other"),
                    value: ExtensionValue::Bool(true),
                },
                PrincipalStorageCommand::IncrementI64 {
                    key: key("counter"),
                    delta: 1,
                },
            ],
        );
        assert!(matches!(
            failed,
            Err(PrincipalStorageError::IntegerOverflow(_))
        ));
        assert!(store.values(&a).unwrap().get(&key("other")).is_none());
        assert!(store.values(&b).is_none());
    }

    #[test]
    fn exact_schema_restore_rejects_without_partial_mutation() {
        let mut store = PrincipalStorageStore::new(PrincipalStorageLimits::default()).unwrap();
        let owner = principal();
        store.register_schema(owner.clone(), 2).unwrap();
        store
            .apply_batch(
                &owner,
                &[PrincipalStorageCommand::IncrementI64 {
                    key: key("counter"),
                    delta: 3,
                }],
            )
            .unwrap();
        let error = store.replace_active([PersistedPrincipalStorage {
            principal: owner.clone(),
            schema_version: 1,
            values: BTreeMap::new(),
        }]);
        assert!(matches!(
            error,
            Err(PrincipalStorageError::SchemaVersionMismatch { .. })
        ));
        assert_eq!(
            store.values(&owner).unwrap().get(&key("counter")),
            Some(&PrincipalStorageValue::I64(3))
        );
    }

    #[test]
    fn arrays_maps_and_sets_are_typed_deterministic_and_atomic() {
        let mut store = PrincipalStorageStore::new(PrincipalStorageLimits::default()).unwrap();
        let owner = principal();
        store.register_schema(owner.clone(), 1).unwrap();
        store
            .apply_batch(
                &owner,
                &[
                    PrincipalStorageCommand::ArrayPush {
                        key: key("history"),
                        value: ExtensionValue::I64(10),
                    },
                    PrincipalStorageCommand::ArrayPush {
                        key: key("history"),
                        value: ExtensionValue::I64(20),
                    },
                    PrincipalStorageCommand::MapSet {
                        key: key("aliases"),
                        entry: "Dragonborn".to_owned(),
                        value: ExtensionValue::String("player".to_owned()),
                    },
                    PrincipalStorageCommand::SetInsert {
                        key: key("visited"),
                        value: ExtensionValue::U64(7),
                    },
                    PrincipalStorageCommand::SetInsert {
                        key: key("visited"),
                        value: ExtensionValue::U64(7),
                    },
                ],
            )
            .unwrap();
        let values = store.values(&owner).unwrap();
        assert_eq!(
            values.get(&key("history")),
            Some(&PrincipalStorageValue::Array(vec![
                ExtensionValue::I64(10),
                ExtensionValue::I64(20),
            ]))
        );
        assert_eq!(
            values.get(&key("visited")),
            Some(&PrincipalStorageValue::Set(BTreeSet::from([
                ExtensionValue::U64(7)
            ])))
        );

        let before = store.values(&owner).unwrap().clone();
        let error = store.apply_batch(
            &owner,
            &[
                PrincipalStorageCommand::MapSet {
                    key: key("aliases"),
                    entry: "Greybeard".to_owned(),
                    value: ExtensionValue::Bool(true),
                },
                PrincipalStorageCommand::ArraySet {
                    key: key("history"),
                    index: 99,
                    value: ExtensionValue::I64(0),
                },
            ],
        );
        assert!(matches!(
            error,
            Err(PrincipalStorageError::CollectionIndexOutOfBounds { .. })
        ));
        assert_eq!(store.values(&owner), Some(&before));
    }

    #[test]
    fn collection_bounds_apply_to_mutation_and_restore() {
        if let Some(too_large) = (u32::MAX as usize).checked_add(1) {
            let wire_invalid = PrincipalStorageLimits {
                max_collection_entries: too_large,
                ..PrincipalStorageLimits::default()
            };
            assert!(matches!(
                PrincipalStorageStore::new(wire_invalid),
                Err(PrincipalStorageError::LimitExceedsWire {
                    field: "max_collection_entries"
                })
            ));
        }

        let limits = PrincipalStorageLimits {
            max_collection_entries: 1,
            ..PrincipalStorageLimits::default()
        };
        let mut store = PrincipalStorageStore::new(limits).unwrap();
        let owner = principal();
        store.register_schema(owner.clone(), 1).unwrap();
        let commands = [
            PrincipalStorageCommand::ArrayPush {
                key: key("bounded"),
                value: ExtensionValue::Bool(true),
            },
            PrincipalStorageCommand::ArrayPush {
                key: key("bounded"),
                value: ExtensionValue::Bool(false),
            },
        ];
        assert!(matches!(
            store.apply_batch(&owner, &commands),
            Err(PrincipalStorageError::CollectionEntryBudgetExceeded { .. })
        ));
        assert!(store.values(&owner).is_none());

        let record = PersistedPrincipalStorage {
            principal: owner,
            schema_version: 1,
            values: BTreeMap::from([(
                key("bounded"),
                PrincipalStorageValue::Array(vec![
                    ExtensionValue::Bool(true),
                    ExtensionValue::Bool(false),
                ]),
            )]),
        };
        assert!(matches!(
            store.validate_record_bounds(&record),
            Err(PrincipalStorageError::CollectionEntryBudgetExceeded { .. })
        ));
    }
}
