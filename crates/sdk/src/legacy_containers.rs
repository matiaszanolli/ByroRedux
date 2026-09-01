//! Bounded engine-owned replacement for JContainers-style object handles.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::identity::{FormRef, PrincipalId};

/// Maximum live container objects owned by one extension principal.
pub const MAX_LEGACY_CONTAINER_OBJECTS: usize = 256;

/// Maximum aggregate entries across one principal's live containers.
pub const MAX_LEGACY_CONTAINER_ENTRIES: usize = 4 * 1024;

/// Maximum UTF-8 bytes in one string value or map key.
pub const MAX_LEGACY_CONTAINER_STRING_BYTES: usize = 4 * 1024;

/// One JContainers-compatible typed value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum LegacyContainerValue {
    Int(i32),
    FloatBits(u32),
    String(String),
    Form(Option<FormRef>),
    Object(i32),
}

impl LegacyContainerValue {
    pub fn float(value: f32) -> Self {
        Self::FloatBits(value.to_bits())
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::FloatBits(bits) => Some(f32::from_bits(*bits)),
            _ => None,
        }
    }

    fn is_bounded(&self) -> bool {
        !matches!(self, Self::String(value) if value.len() > MAX_LEGACY_CONTAINER_STRING_BYTES)
    }
}

/// Concrete object kind retained behind a Papyrus-compatible integer handle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "entries", rename_all = "kebab-case")]
pub enum LegacyContainer {
    Array(Vec<LegacyContainerValue>),
    Map(BTreeMap<String, LegacyContainerValue>),
}

/// Validation failure for a restored compatibility registry.
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum LegacyContainerError {
    #[error("legacy container object count exceeds {MAX_LEGACY_CONTAINER_OBJECTS}")]
    TooManyObjects,
    #[error("legacy container entry count exceeds {MAX_LEGACY_CONTAINER_ENTRIES}")]
    TooManyEntries,
    #[error("legacy container handle must be positive")]
    InvalidHandle,
    #[error(
        "legacy container string or map key exceeds {MAX_LEGACY_CONTAINER_STRING_BYTES} bytes"
    )]
    StringTooLong,
}

/// Principal-local object table replacing JContainers' native-plugin heap.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyContainerRegistry {
    next_handle: i32,
    containers: BTreeMap<i32, LegacyContainer>,
}

impl Default for LegacyContainerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LegacyContainerRegistry {
    pub fn new() -> Self {
        Self {
            next_handle: 1,
            containers: BTreeMap::new(),
        }
    }

    /// Create an array, returning JContainers' invalid-object sentinel on
    /// exhaustion.
    pub fn create_array(&mut self) -> i32 {
        self.create(LegacyContainer::Array(Vec::new()))
    }

    /// Create a string-keyed map, returning `0` on exhaustion.
    pub fn create_map(&mut self) -> i32 {
        self.create(LegacyContainer::Map(BTreeMap::new()))
    }

    pub fn contains(&self, handle: i32) -> bool {
        self.containers.contains_key(&handle)
    }

    pub fn count(&self, handle: i32) -> i32 {
        self.containers
            .get(&handle)
            .map(|container| match container {
                LegacyContainer::Array(values) => values.len(),
                LegacyContainer::Map(values) => values.len(),
            })
            .and_then(|count| i32::try_from(count).ok())
            .unwrap_or(0)
    }

    pub fn clear(&mut self, handle: i32) -> bool {
        match self.containers.get_mut(&handle) {
            Some(LegacyContainer::Array(values)) => values.clear(),
            Some(LegacyContainer::Map(values)) => values.clear(),
            None => return false,
        }
        true
    }

    pub fn release(&mut self, handle: i32) -> bool {
        self.containers.remove(&handle).is_some()
    }

    /// Insert before `index`, or append when it is absent. Rejected mutations
    /// return `false` without changing the registry.
    pub fn array_add(
        &mut self,
        handle: i32,
        value: LegacyContainerValue,
        index: Option<u32>,
    ) -> bool {
        if !self.can_insert(&value) {
            return false;
        }
        let Some(LegacyContainer::Array(values)) = self.containers.get_mut(&handle) else {
            return false;
        };
        let index = match index {
            Some(index) => {
                let Ok(index) = usize::try_from(index) else {
                    return false;
                };
                if index > values.len() {
                    return false;
                }
                index
            }
            None => values.len(),
        };
        values.insert(index, value);
        true
    }

    pub fn array_get(&self, handle: i32, index: i32) -> Option<&LegacyContainerValue> {
        let index = usize::try_from(index).ok()?;
        match self.containers.get(&handle)? {
            LegacyContainer::Array(values) => values.get(index),
            LegacyContainer::Map(_) => None,
        }
    }

    pub fn array_set(&mut self, handle: i32, index: i32, value: LegacyContainerValue) -> bool {
        if !value.is_bounded() || !self.valid_object_value(&value) {
            return false;
        }
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        let Some(LegacyContainer::Array(values)) = self.containers.get_mut(&handle) else {
            return false;
        };
        let Some(slot) = values.get_mut(index) else {
            return false;
        };
        *slot = value;
        true
    }

    pub fn array_erase(&mut self, handle: i32, index: i32) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        let Some(LegacyContainer::Array(values)) = self.containers.get_mut(&handle) else {
            return false;
        };
        if index >= values.len() {
            return false;
        }
        values.remove(index);
        true
    }

    pub fn map_get(&self, handle: i32, key: &str) -> Option<&LegacyContainerValue> {
        match self.containers.get(&handle)? {
            LegacyContainer::Map(values) => values.get(key),
            LegacyContainer::Array(_) => None,
        }
    }

    pub fn map_has_key(&self, handle: i32, key: &str) -> bool {
        self.map_get(handle, key).is_some()
    }

    pub fn map_set(&mut self, handle: i32, key: String, value: LegacyContainerValue) -> bool {
        if key.len() > MAX_LEGACY_CONTAINER_STRING_BYTES
            || !value.is_bounded()
            || !self.valid_object_value(&value)
        {
            return false;
        }
        let is_new = match self.containers.get(&handle) {
            Some(LegacyContainer::Map(values)) => !values.contains_key(&key),
            _ => return false,
        };
        if is_new && self.entry_count() >= MAX_LEGACY_CONTAINER_ENTRIES {
            return false;
        }
        let Some(LegacyContainer::Map(values)) = self.containers.get_mut(&handle) else {
            return false;
        };
        values.insert(key, value);
        true
    }

    pub fn map_remove(&mut self, handle: i32, key: &str) -> bool {
        match self.containers.get_mut(&handle) {
            Some(LegacyContainer::Map(values)) => values.remove(key).is_some(),
            _ => false,
        }
    }

    pub fn object_count(&self) -> usize {
        self.containers.len()
    }

    pub fn entry_count(&self) -> usize {
        self.containers
            .values()
            .map(|container| match container {
                LegacyContainer::Array(values) => values.len(),
                LegacyContainer::Map(values) => values.len(),
            })
            .sum()
    }

    pub fn validate(&self) -> Result<(), LegacyContainerError> {
        if self.containers.len() > MAX_LEGACY_CONTAINER_OBJECTS {
            return Err(LegacyContainerError::TooManyObjects);
        }
        if self.entry_count() > MAX_LEGACY_CONTAINER_ENTRIES {
            return Err(LegacyContainerError::TooManyEntries);
        }
        if self.next_handle <= 0 || self.containers.keys().any(|handle| *handle <= 0) {
            return Err(LegacyContainerError::InvalidHandle);
        }
        for container in self.containers.values() {
            match container {
                LegacyContainer::Array(values) => {
                    if values.iter().any(|value| !value.is_bounded()) {
                        return Err(LegacyContainerError::StringTooLong);
                    }
                }
                LegacyContainer::Map(values) => {
                    if values.iter().any(|(key, value)| {
                        key.len() > MAX_LEGACY_CONTAINER_STRING_BYTES || !value.is_bounded()
                    }) {
                        return Err(LegacyContainerError::StringTooLong);
                    }
                }
            }
        }
        Ok(())
    }

    fn create(&mut self, container: LegacyContainer) -> i32 {
        if self.containers.len() >= MAX_LEGACY_CONTAINER_OBJECTS {
            return 0;
        }
        for _ in 0..=MAX_LEGACY_CONTAINER_OBJECTS {
            let handle = self.next_handle.max(1);
            self.next_handle = handle.checked_add(1).unwrap_or(1);
            if let std::collections::btree_map::Entry::Vacant(entry) = self.containers.entry(handle)
            {
                entry.insert(container);
                return handle;
            }
        }
        0
    }

    fn can_insert(&self, value: &LegacyContainerValue) -> bool {
        self.entry_count() < MAX_LEGACY_CONTAINER_ENTRIES
            && value.is_bounded()
            && self.valid_object_value(value)
    }

    fn valid_object_value(&self, value: &LegacyContainerValue) -> bool {
        !matches!(value, LegacyContainerValue::Object(handle) if *handle != 0 && !self.contains(*handle))
    }
}

/// Save record for one authenticated principal's compatibility objects.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedLegacyContainers {
    pub principal: PrincipalId,
    pub registry: LegacyContainerRegistry,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_arrays_and_maps_share_nested_object_handles() {
        let mut registry = LegacyContainerRegistry::new();
        let array = registry.create_array();
        let map = registry.create_map();
        assert!(registry.array_add(array, LegacyContainerValue::Int(7), None));
        assert!(registry.array_add(
            array,
            LegacyContainerValue::String("Dragonborn".to_owned()),
            None,
        ));
        assert!(registry.map_set(map, "items".to_owned(), LegacyContainerValue::Object(array),));
        assert_eq!(registry.count(array), 2);
        assert_eq!(registry.count(map), 1);
        assert_eq!(
            registry.map_get(map, "items"),
            Some(&LegacyContainerValue::Object(array))
        );
        assert!(registry.array_set(array, 0, LegacyContainerValue::float(3.5)));
        assert_eq!(registry.array_get(array, 0).unwrap().as_float(), Some(3.5));
    }

    #[test]
    fn invalid_handles_types_indices_and_values_do_not_mutate() {
        let mut registry = LegacyContainerRegistry::new();
        let array = registry.create_array();
        let map = registry.create_map();
        assert!(!registry.array_add(map, LegacyContainerValue::Int(1), None));
        assert!(!registry.map_set(array, "key".to_owned(), LegacyContainerValue::Int(1)));
        assert!(!registry.array_add(array, LegacyContainerValue::Object(999), None));
        assert!(!registry.array_add(
            array,
            LegacyContainerValue::String("x".repeat(MAX_LEGACY_CONTAINER_STRING_BYTES + 1)),
            None,
        ));
        assert!(!registry.array_set(array, 0, LegacyContainerValue::Int(2)));
        assert_eq!(registry.entry_count(), 0);
    }

    #[test]
    fn registry_enforces_object_and_aggregate_entry_bounds() {
        let mut registry = LegacyContainerRegistry::new();
        let first = registry.create_array();
        for _ in 1..MAX_LEGACY_CONTAINER_OBJECTS {
            assert_ne!(registry.create_map(), 0);
        }
        assert_eq!(registry.create_array(), 0);
        for index in 0..MAX_LEGACY_CONTAINER_ENTRIES {
            assert!(registry.array_add(
                first,
                LegacyContainerValue::Int(i32::try_from(index).unwrap()),
                None,
            ));
        }
        assert!(!registry.array_add(first, LegacyContainerValue::Int(0), None));
        registry.validate().unwrap();
    }

    #[test]
    fn serde_round_trip_preserves_handles_and_bits() {
        let mut registry = LegacyContainerRegistry::new();
        let array = registry.create_array();
        registry.array_add(array, LegacyContainerValue::float(-0.0), None);
        let bytes = serde_json::to_vec(&registry).unwrap();
        let restored: LegacyContainerRegistry = serde_json::from_slice(&bytes).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored, registry);
        assert_eq!(restored.array_get(array, 0).unwrap().as_float(), Some(-0.0));
    }
}
