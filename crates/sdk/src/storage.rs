//! Bounded, principal-isolated persistent key/value storage.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::component::{ExtensionCommand, ExtensionValue, ExtensionValueType};
use crate::identity::{PrincipalId, StorageKey};

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
}

/// One mutation emitted by a sandbox callback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostCommand {
    Component(ExtensionCommand),
    PrincipalStorage(PrincipalStorageCommand),
}

/// Hard bounds for one engine-owned principal storage service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrincipalStorageLimits {
    pub max_principals: usize,
    pub max_entries_per_principal: usize,
    pub max_string_bytes: usize,
    pub max_blob_bytes: usize,
    pub max_total_bytes_per_principal: usize,
}

impl Default for PrincipalStorageLimits {
    fn default() -> Self {
        Self {
            max_principals: 4_096,
            max_entries_per_principal: 4_096,
            max_string_bytes: 16 * 1024,
            max_blob_bytes: 64 * 1024,
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
    pub values: BTreeMap<StorageKey, ExtensionValue>,
}

/// Rejection from schema registration, mutation, or restore.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PrincipalStorageError {
    #[error("{field} must be non-zero")]
    InvalidLimit { field: &'static str },
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
    #[error("storage key {key} is {actual:?}, expected I64")]
    TypeMismatch {
        key: StorageKey,
        actual: ExtensionValueType,
    },
    #[error("integer overflow incrementing principal storage key {0}")]
    IntegerOverflow(StorageKey),
}

/// Engine-owned storage isolated by authenticated principal.
#[derive(Clone, Debug)]
pub struct PrincipalStorageStore {
    limits: PrincipalStorageLimits,
    schemas: BTreeMap<PrincipalId, u32>,
    values: BTreeMap<PrincipalId, BTreeMap<StorageKey, ExtensionValue>>,
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
            (
                "max_total_bytes_per_principal",
                limits.max_total_bytes_per_principal,
            ),
        ] {
            if value == 0 {
                return Err(PrincipalStorageError::InvalidLimit { field });
            }
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

    pub fn values(&self, principal: &PrincipalId) -> Option<&BTreeMap<StorageKey, ExtensionValue>> {
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
                    self.validate_value(key, value)?;
                    staged.insert(key.clone(), value.clone());
                }
                PrincipalStorageCommand::Delete { key } => {
                    staged.remove(key);
                }
                PrincipalStorageCommand::IncrementI64 { key, delta } => {
                    let current = match staged.get(key) {
                        Some(ExtensionValue::I64(value)) => *value,
                        Some(value) => {
                            return Err(PrincipalStorageError::TypeMismatch {
                                key: key.clone(),
                                actual: value.value_type(),
                            });
                        }
                        None => 0,
                    };
                    let next = current
                        .checked_add(*delta)
                        .ok_or_else(|| PrincipalStorageError::IntegerOverflow(key.clone()))?;
                    staged.insert(key.clone(), ExtensionValue::I64(next));
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

    fn validate_value(
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

    fn validate_map(
        &self,
        principal: &PrincipalId,
        values: &BTreeMap<StorageKey, ExtensionValue>,
    ) -> Result<(), PrincipalStorageError> {
        if values.len() > self.limits.max_entries_per_principal {
            return Err(PrincipalStorageError::EntryBudgetExceeded {
                principal: principal.clone(),
                maximum: self.limits.max_entries_per_principal,
            });
        }
        let total = values.iter().try_fold(0usize, |total, (key, value)| {
            let value_bytes = match value {
                ExtensionValue::Bool(_) => 1,
                ExtensionValue::I64(_) | ExtensionValue::U64(_) => 8,
                ExtensionValue::String(value) => value.len(),
                ExtensionValue::Bytes(value) => value.len(),
            };
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
            Some(&ExtensionValue::I64(3))
        );
    }
}
