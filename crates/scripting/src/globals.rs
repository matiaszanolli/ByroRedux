//! GLOB runtime values as a World resource (#1668).
//!
//! Bethesda CTDA conditions can compare a function result against a GLOB
//! record's current value ("Use Global" — `ConditionValue::Global`). The
//! parsed values live in `EsmIndex.globals`, but `EsmIndex` is not a
//! `Resource`. This module mirrors `EsmIndex.globals` into a lean,
//! evaluator-facing resource: a `FormID → f32` map keyed in the **global
//! load-order space** (the same space CTDA comparands are remapped into at
//! parse time, see `remap_condition_form_ids`), so the lookup is
//! space-consistent and free of the multi-plugin false-positive risk.
//!
//! Globals are script-mutable at runtime; the map is therefore owned (not
//! borrowed from the index) so `SetGlobalValue`-style mutations have a home.

use byroredux_core::ecs::resource::Resource;
use byroredux_plugin::esm::records::GlobalRecord;
use std::collections::HashMap;

/// Runtime GLOB values, keyed by global-load-order FormID.
#[derive(Debug, Clone, Default)]
pub struct Globals(pub HashMap<u32, f32>);

impl Globals {
    /// An empty global table.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Snapshot a parsed `EsmIndex.globals` map into runtime values.
    /// Keys are preserved verbatim — `EsmIndex` already remaps record
    /// FormIDs into global load-order space at parse time.
    pub fn from_records(records: &HashMap<u32, GlobalRecord>) -> Self {
        Self(
            records
                .iter()
                .map(|(&form_id, rec)| (form_id, rec.value.as_f32()))
                .collect(),
        )
    }

    /// Current value of a GLOB, or `None` when the FormID is unknown.
    /// A missing GLOB resolves to `0.0` at the comparand site (Bethesda's
    /// "missing GLOB defaults to 0").
    pub fn get(&self, form_id: u32) -> Option<f32> {
        self.0.get(&form_id).copied()
    }

    /// Set (or insert) a GLOB's runtime value — the `SetGlobalValue`
    /// mutation surface a future script runtime writes through.
    pub fn set(&mut self, form_id: u32, value: f32) {
        self.0.insert(form_id, value);
    }

    /// Number of globals tracked.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` when no globals are tracked.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Resource for Globals {}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::SettingValue;

    fn rec(form_id: u32, value: SettingValue) -> GlobalRecord {
        GlobalRecord {
            form_id,
            editor_id: String::new(),
            value,
        }
    }

    #[test]
    fn from_records_widens_each_value_kind() {
        let mut src = HashMap::new();
        src.insert(0x0100_0001, rec(0x0100_0001, SettingValue::Int(7)));
        src.insert(0x0100_0002, rec(0x0100_0002, SettingValue::Float(2.5)));
        src.insert(0x0100_0003, rec(0x0100_0003, SettingValue::Short(-3)));
        src.insert(0x0100_0004, rec(0x0100_0004, SettingValue::String("x".into())));

        let globals = Globals::from_records(&src);
        assert_eq!(globals.get(0x0100_0001), Some(7.0));
        assert_eq!(globals.get(0x0100_0002), Some(2.5));
        assert_eq!(globals.get(0x0100_0003), Some(-3.0));
        assert_eq!(globals.get(0x0100_0004), Some(0.0), "string → 0.0");
        assert_eq!(globals.get(0x0DEAD), None, "unknown FormID → None");
    }

    #[test]
    fn set_overwrites_runtime_value() {
        let mut globals = Globals::new();
        assert!(globals.is_empty());
        globals.set(0x42, 1.0);
        globals.set(0x42, 9.0);
        assert_eq!(globals.get(0x42), Some(9.0));
        assert_eq!(globals.len(), 1);
    }
}
