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
    #[error("legacy container retention metadata is invalid")]
    InvalidRetention,
    #[error("legacy container contains a missing object handle")]
    InvalidObjectReference,
}

/// Explicit Papyrus ownership retained for one compatibility object.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyContainerRetention {
    count: u32,
    tag: Option<String>,
}

/// Principal-local object table replacing JContainers' native-plugin heap.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyContainerRegistry {
    next_handle: i32,
    containers: BTreeMap<i32, LegacyContainer>,
    retentions: BTreeMap<i32, LegacyContainerRetention>,
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
            retentions: BTreeMap::new(),
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

    pub fn is_array(&self, handle: i32) -> bool {
        matches!(
            self.containers.get(&handle),
            Some(LegacyContainer::Array(_))
        )
    }

    pub fn is_map(&self, handle: i32) -> bool {
        matches!(self.containers.get(&handle), Some(LegacyContainer::Map(_)))
    }

    pub fn is_empty(&self, handle: i32) -> bool {
        self.count(handle) == 0
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
        let released = match self.containers.get_mut(&handle) {
            Some(LegacyContainer::Array(values)) => std::mem::take(values),
            Some(LegacyContainer::Map(values)) => std::mem::take(values).into_values().collect(),
            None => return false,
        };
        self.collect_released_values(released);
        true
    }

    /// Add one explicit owner and optionally associate its recovery tag.
    /// Invalid handles and bounded-state exhaustion return JContainers' zero
    /// sentinel without changing the registry.
    pub fn retain(&mut self, handle: i32, tag: Option<&str>) -> i32 {
        if !self.contains(handle)
            || tag.is_some_and(|tag| tag.len() > MAX_LEGACY_CONTAINER_STRING_BYTES)
        {
            return 0;
        }
        let retention = self
            .retentions
            .entry(handle)
            .or_insert(LegacyContainerRetention {
                count: 0,
                tag: None,
            });
        let Some(count) = retention.count.checked_add(1) else {
            return 0;
        };
        retention.count = count;
        if let Some(tag) = tag.filter(|tag| !tag.is_empty()) {
            retention.tag = Some(tag.to_owned());
        }
        handle
    }

    /// Release one explicit owner. An unowned object is collected immediately;
    /// this deterministic boundary replaces JContainers' wall-clock grace
    /// period while preserving ownership relationships.
    pub fn release(&mut self, handle: i32) -> bool {
        if !self.contains(handle) {
            return false;
        }
        let remove_retention = if let Some(retention) = self.retentions.get_mut(&handle) {
            retention.count -= 1;
            retention.count == 0
        } else {
            false
        };
        if remove_retention {
            self.retentions.remove(&handle);
        }
        self.collect_if_unowned(handle);
        true
    }

    /// Release the previous object and retain the replacement. Equal handles
    /// are left untouched, matching JValue's property-setter helper.
    pub fn release_and_retain(
        &mut self,
        previous_handle: i32,
        new_handle: i32,
        tag: Option<&str>,
    ) -> i32 {
        if previous_handle == new_handle {
            return if self.contains(new_handle) {
                new_handle
            } else {
                0
            };
        }
        if previous_handle != 0 {
            self.release(previous_handle);
        }
        self.retain(new_handle, tag)
    }

    /// Complement every explicit retain on objects carrying `tag`.
    pub fn release_objects_with_tag(&mut self, tag: &str) -> usize {
        let handles = self
            .retentions
            .iter()
            .filter_map(|(handle, retention)| {
                (retention.tag.as_deref() == Some(tag)).then_some(*handle)
            })
            .collect::<Vec<_>>();
        for handle in &handles {
            self.retentions.remove(handle);
            self.collect_if_unowned(*handle);
        }
        handles.len()
    }

    pub fn retain_count(&self, handle: i32) -> u32 {
        self.retentions
            .get(&handle)
            .map(|retention| retention.count)
            .unwrap_or(0)
    }

    pub fn retention_tag(&self, handle: i32) -> Option<&str> {
        self.retentions
            .get(&handle)
            .and_then(|retention| retention.tag.as_deref())
    }

    /// Copy one container while retaining references to the original children.
    pub fn shallow_copy(&mut self, handle: i32) -> i32 {
        let Some(container) = self.containers.get(&handle).cloned() else {
            return 0;
        };
        if self
            .entry_count()
            .checked_add(Self::container_len(&container))
            .is_none_or(|count| count > MAX_LEGACY_CONTAINER_ENTRIES)
        {
            return 0;
        }
        self.create(container)
    }

    /// Copy a complete reachable object graph, preserving shared children and
    /// cycles while assigning fresh handles to every copied object.
    pub fn deep_copy(&mut self, handle: i32) -> i32 {
        if !self.contains(handle) {
            return 0;
        }
        let mut reachable = Vec::new();
        let mut pending = vec![handle];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(candidate) = pending.pop() {
            if !visited.insert(candidate) {
                continue;
            }
            let Some(container) = self.containers.get(&candidate) else {
                return 0;
            };
            reachable.push(candidate);
            pending.extend(
                Self::container_values(container).filter_map(|value| match value {
                    LegacyContainerValue::Object(child) if *child != 0 => Some(*child),
                    _ => None,
                }),
            );
        }
        let copied_entries = reachable
            .iter()
            .filter_map(|source| self.containers.get(source))
            .map(Self::container_len)
            .sum::<usize>();
        if self
            .object_count()
            .checked_add(reachable.len())
            .is_none_or(|count| count > MAX_LEGACY_CONTAINER_OBJECTS)
            || self
                .entry_count()
                .checked_add(copied_entries)
                .is_none_or(|count| count > MAX_LEGACY_CONTAINER_ENTRIES)
        {
            return 0;
        }

        let mut staged = self.clone();
        let mut remap = BTreeMap::new();
        for source in &reachable {
            let empty = match staged.containers.get(source) {
                Some(LegacyContainer::Array(_)) => LegacyContainer::Array(Vec::new()),
                Some(LegacyContainer::Map(_)) => LegacyContainer::Map(BTreeMap::new()),
                None => return 0,
            };
            let copy = staged.create(empty);
            if copy == 0 {
                return 0;
            }
            remap.insert(*source, copy);
        }
        for source in &reachable {
            let Some(original) = self.containers.get(source) else {
                return 0;
            };
            let copied = match original {
                LegacyContainer::Array(values) => LegacyContainer::Array(
                    values
                        .iter()
                        .map(|value| Self::remap_object_value(value, &remap))
                        .collect(),
                ),
                LegacyContainer::Map(values) => LegacyContainer::Map(
                    values
                        .iter()
                        .map(|(key, value)| (key.clone(), Self::remap_object_value(value, &remap)))
                        .collect(),
                ),
            };
            staged.containers.insert(remap[source], copied);
        }
        let root = remap[&handle];
        *self = staged;
        root
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
        let released = std::mem::replace(slot, value);
        self.collect_released_values([released]);
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
        let released = values.remove(index);
        self.collect_released_values([released]);
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
        let released = values.insert(key, value);
        if let Some(released) = released {
            self.collect_released_values([released]);
        }
        true
    }

    pub fn map_remove(&mut self, handle: i32, key: &str) -> bool {
        let released = match self.containers.get_mut(&handle) {
            Some(LegacyContainer::Map(values)) => values.remove(key),
            _ => return false,
        };
        if let Some(released) = released {
            self.collect_released_values([released]);
            true
        } else {
            false
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
        if self.retentions.iter().any(|(handle, retention)| {
            !self.containers.contains_key(handle)
                || retention.count == 0
                || retention.tag.as_ref().is_some_and(|tag| {
                    tag.is_empty() || tag.len() > MAX_LEGACY_CONTAINER_STRING_BYTES
                })
        }) {
            return Err(LegacyContainerError::InvalidRetention);
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
        if self.containers.values().any(|container| {
            Self::container_values(container).any(|value| !self.valid_object_value(value))
        }) {
            return Err(LegacyContainerError::InvalidObjectReference);
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

    fn container_len(container: &LegacyContainer) -> usize {
        match container {
            LegacyContainer::Array(values) => values.len(),
            LegacyContainer::Map(values) => values.len(),
        }
    }

    fn remap_object_value(
        value: &LegacyContainerValue,
        remap: &BTreeMap<i32, i32>,
    ) -> LegacyContainerValue {
        match value {
            LegacyContainerValue::Object(handle) if *handle != 0 => {
                LegacyContainerValue::Object(remap[handle])
            }
            _ => value.clone(),
        }
    }

    fn container_values(
        container: &LegacyContainer,
    ) -> Box<dyn Iterator<Item = &LegacyContainerValue> + '_> {
        match container {
            LegacyContainer::Array(values) => Box::new(values.iter()),
            LegacyContainer::Map(values) => Box::new(values.values()),
        }
    }

    fn inbound_owner_count(&self, handle: i32) -> usize {
        self.containers
            .values()
            .flat_map(Self::container_values)
            .filter(
                |value| matches!(value, LegacyContainerValue::Object(value) if *value == handle),
            )
            .count()
    }

    fn collect_released_values<I>(&mut self, values: I)
    where
        I: IntoIterator<Item = LegacyContainerValue>,
    {
        for value in values {
            if let LegacyContainerValue::Object(handle) = value {
                self.collect_if_unowned(handle);
            }
        }
    }

    fn collect_if_unowned(&mut self, handle: i32) {
        let mut candidates = vec![handle];
        while let Some(candidate) = candidates.pop() {
            if !self.contains(candidate)
                || self.retain_count(candidate) != 0
                || self.inbound_owner_count(candidate) != 0
            {
                continue;
            }
            let Some(container) = self.containers.remove(&candidate) else {
                continue;
            };
            self.retentions.remove(&candidate);
            candidates.extend(
                Self::container_values(&container).filter_map(|value| match value {
                    LegacyContainerValue::Object(child) if *child != 0 => Some(*child),
                    _ => None,
                }),
            );
        }
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
        assert_eq!(registry.retain(array, Some("org.example.fixture")), array);
        let bytes = serde_json::to_vec(&registry).unwrap();
        let restored: LegacyContainerRegistry = serde_json::from_slice(&bytes).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored, registry);
        assert_eq!(restored.array_get(array, 0).unwrap().as_float(), Some(-0.0));
        assert_eq!(restored.retain_count(array), 1);
        assert_eq!(restored.retention_tag(array), Some("org.example.fixture"));
    }

    #[test]
    fn explicit_retains_and_tags_control_object_lifetime() {
        let mut registry = LegacyContainerRegistry::new();
        let first = registry.create_array();
        let second = registry.create_map();
        assert_eq!(registry.retain(first, Some("org.example.fixture")), first);
        assert_eq!(registry.retain(first, None), first);
        assert_eq!(registry.retain_count(first), 2);
        assert_eq!(registry.retention_tag(first), Some("org.example.fixture"));
        assert_eq!(
            registry.release_and_retain(first, first, Some("ignored")),
            first
        );
        assert_eq!(registry.retain_count(first), 2);
        assert_eq!(
            registry.release_and_retain(first, second, Some("org.example.fixture")),
            second
        );
        assert_eq!(registry.retain_count(first), 1);
        assert_eq!(registry.retain_count(second), 1);
        assert_eq!(registry.release_objects_with_tag("org.example.fixture"), 2);
        assert!(!registry.contains(first));
        assert!(!registry.contains(second));
    }

    #[test]
    fn nested_ownership_collects_only_after_the_last_owner_leaves() {
        let mut registry = LegacyContainerRegistry::new();
        let parent = registry.create_map();
        let child = registry.create_array();
        assert!(registry.map_set(
            parent,
            "child".to_owned(),
            LegacyContainerValue::Object(child),
        ));
        assert!(registry.release(child));
        assert!(registry.contains(child));
        assert!(registry.map_remove(parent, "child"));
        assert!(!registry.contains(child));
        assert!(registry.contains(parent));

        let replacement = registry.create_array();
        assert!(registry.map_set(
            parent,
            "replacement".to_owned(),
            LegacyContainerValue::Object(replacement),
        ));
        assert!(registry.release(parent));
        assert!(!registry.contains(parent));
        assert!(!registry.contains(replacement));
    }

    #[test]
    fn shallow_and_deep_copy_preserve_kind_sharing_and_cycles() {
        let mut registry = LegacyContainerRegistry::new();
        let child = registry.create_array();
        assert!(registry.array_add(child, LegacyContainerValue::Int(7), None));
        let root = registry.create_map();
        assert!(registry.map_set(
            root,
            "first".to_owned(),
            LegacyContainerValue::Object(child),
        ));
        assert!(registry.map_set(
            root,
            "second".to_owned(),
            LegacyContainerValue::Object(child),
        ));
        assert!(registry.map_set(root, "self".to_owned(), LegacyContainerValue::Object(root),));

        let shallow = registry.shallow_copy(root);
        assert!(registry.is_map(shallow));
        assert_eq!(
            registry.map_get(shallow, "first"),
            Some(&LegacyContainerValue::Object(child))
        );

        let deep = registry.deep_copy(root);
        assert!(registry.is_map(deep));
        assert!(!registry.is_array(deep));
        assert!(!registry.is_empty(deep));
        let Some(LegacyContainerValue::Object(first_copy)) = registry.map_get(deep, "first") else {
            panic!("deep copy lost its first child")
        };
        let first_copy = *first_copy;
        assert_ne!(first_copy, child);
        assert_eq!(
            registry.map_get(deep, "second"),
            Some(&LegacyContainerValue::Object(first_copy))
        );
        assert_eq!(
            registry.map_get(deep, "self"),
            Some(&LegacyContainerValue::Object(deep))
        );
        assert_eq!(
            registry.array_get(first_copy, 0),
            Some(&LegacyContainerValue::Int(7))
        );
        assert!(registry.is_empty(0));
    }
}
