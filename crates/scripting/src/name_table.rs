//! A table keyed by editor ID, folded to lowercase on both sides (#6152).
//!
//! Scripts name records by editor ID in any case (`PlayIdle LooseDogGrowl`, `SoundPlay.<name>`,
//! `SetAV("Variable06")`, `PlayMusic`, `&GlobalName;`). Every resource that answers such a name keeps
//! its records in one of these, so the case folding of the stored key and of the looked-up name
//! cannot drift apart, and a record with no editor ID is never stored under the empty name.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct EditorIdTable<V>(HashMap<String, V>);

impl<V> Default for EditorIdTable<V> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<V> EditorIdTable<V> {
    /// Build from `(editor ID, value)` pairs; a later duplicate name replaces an earlier one.
    pub fn from_records<'a>(records: impl IntoIterator<Item = (&'a str, V)>) -> Self {
        let mut table = Self::default();
        for (editor_id, value) in records {
            table.insert(editor_id, value);
        }
        table
    }

    /// Store `value` under `editor_id` (any case), replacing what was there. An empty editor ID is not stored.
    pub fn insert(&mut self, editor_id: &str, value: V) {
        if !editor_id.is_empty() {
            self.0.insert(editor_id.to_ascii_lowercase(), value);
        }
    }

    /// Store `value` under `editor_id` unless the name is taken (an alias spelling yields to a record's own).
    pub fn insert_if_absent(&mut self, editor_id: &str, value: V) {
        if !editor_id.is_empty() {
            self.0.entry(editor_id.to_ascii_lowercase()).or_insert(value);
        }
    }

    /// The value `name` (any case) stands for.
    pub fn get(&self, name: &str) -> Option<&V> {
        self.0.get(&name.to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_fold_case_on_both_sides_and_the_empty_name_is_never_stored() {
        let mut table = EditorIdTable::from_records([("LooseDogGrowl", 1u32), ("", 2)]);
        assert_eq!(table.get("loosedoggrowl"), Some(&1));
        assert_eq!(table.get("LOOSEDOGGROWL"), Some(&1));
        assert_eq!((table.get(""), table.len()), (None, 1));
        table.insert_if_absent("LooseDogGrowl", 3);
        assert_eq!(table.get("LooseDogGrowl"), Some(&1), "an alias yields to the record's own name");
        table.insert("LooseDogGrowl", 4);
        assert_eq!(table.get("LooseDogGrowl"), Some(&4));
    }
}
