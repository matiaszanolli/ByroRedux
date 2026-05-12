//! Component registry — type-erased accessors for debug inspection.
//!
//! Each inspectable component registers a [`ComponentDescriptor`] that provides
//! closures for getting/setting component data as JSON. This bridges the gap
//! between the compile-time-typed ECS and the string-based debug protocol.

use std::collections::BTreeMap;

/// Type-erased accessors for a single component type.
///
/// The closures capture the concrete `Component` type and call through to
/// typed ECS queries internally. The debug server never needs to know the
/// concrete type — it works entirely through JSON values.
pub struct ComponentDescriptor {
    /// Human-readable type name (e.g. "Transform").
    pub name: &'static str,
    /// Field names for tab-completion and display.
    pub field_names: Vec<&'static str>,
    /// Serialize the entire component on an entity to JSON.
    /// Returns None if the entity doesn't have this component.
    pub get_json: Box<dyn Fn(&dyn std::any::Any, u32) -> Option<serde_json::Value> + Send + Sync>,
    /// Deserialize a JSON value and overwrite the entire component on an entity.
    pub set_json:
        Box<dyn Fn(&dyn std::any::Any, u32, serde_json::Value) -> Result<(), String> + Send + Sync>,
    /// List all entity IDs that have this component.
    pub list_entities: Box<dyn Fn(&dyn std::any::Any) -> Vec<u32> + Send + Sync>,
    /// Serialize a single field of the component to JSON.
    pub get_field:
        Box<dyn Fn(&dyn std::any::Any, u32, &str) -> Option<serde_json::Value> + Send + Sync>,
    /// Deserialize and set a single field of the component.
    pub set_field: Box<
        dyn Fn(&dyn std::any::Any, u32, &str, serde_json::Value) -> Result<(), String>
            + Send
            + Sync,
    >,
}

/// Registry of all inspectable components, keyed by name.
///
/// Stored as a World resource. The debug server looks up descriptors by
/// the component name strings that appear in debug expressions.
pub struct ComponentRegistry {
    descriptors: BTreeMap<String, ComponentDescriptor>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            descriptors: BTreeMap::new(),
        }
    }

    /// Register a component descriptor under its name.
    pub fn insert(&mut self, descriptor: ComponentDescriptor) {
        self.descriptors
            .insert(descriptor.name.to_string(), descriptor);
    }

    /// Look up a descriptor by component name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&ComponentDescriptor> {
        // Try exact match first, then case-insensitive
        if let Some(d) = self.descriptors.get(name) {
            return Some(d);
        }
        let lower = name.to_ascii_lowercase();
        self.descriptors
            .values()
            .find(|d| d.name.to_ascii_lowercase() == lower)
    }

    /// All registered component names, sorted.
    pub fn names(&self) -> Vec<&str> {
        self.descriptors.values().map(|d| d.name).collect()
    }

    /// Iterate every registered descriptor in name order. Used by the
    /// `Inspect` request to dump every component on an entity without
    /// allocating a Vec of names up front.
    pub fn iter(&self) -> impl Iterator<Item = &ComponentDescriptor> {
        self.descriptors.values()
    }

    /// Number of registered components.
    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}
