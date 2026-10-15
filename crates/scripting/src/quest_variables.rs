//! Quest script variables (M47.2).
//!
//! Papyrus quest scripts carry state the rest of the game reads by name: a
//! dialogue line flips `kmyQuest.GetInHjerimTovaPointer = true`, and the next
//! line's `GetVMQuestVariable MS11 "::GetInHjerimTovaPointer_var" == 1`
//! condition opens. Two resources hold that state, keyed exactly the way the
//! condition reads it:
//!
//! - [`QuestScriptVariables`] — the values scripts *wrote* this playthrough.
//!   Saved.
//! - [`QuestScriptVariableDefaults`] — the values each quest's `VMAD`
//!   *authors*. Re-derived from the load order every boot and never saved,
//!   so a save load (which replaces the saved resource wholesale) cannot wipe
//!   them.
//!
//! [`quest_variable`] reads the write, else the authored value, else `None`
//! (the caller's 0 — Papyrus's numeric default).
//!
//! **Key.** A fragment writes an *auto property* of the quest script
//! (`kmyQuest.Name = …`; external code cannot reach a plain script variable),
//! whose backing variable is `::Name_var` — the spelling 3,287 of the 3,319
//! vanilla Skyrim `GetVMQuestVariable` conditions (and 730 of FO4's 774) carry
//! in their `CIS2` (`crates/plugin/examples/ctda_variable_census.rs`). The
//! rest name plain script variables, which only the quest's own script writes;
//! they read their authored value.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::ConditionStringId;
use byroredux_plugin::esm::records::script_instance::{PropertyValue, ScriptInstanceData};

use crate::quest_stages::QuestFormId;

type VariableMap = HashMap<QuestFormId, HashMap<ConditionStringId, f32>>;

/// Quest-script variables written by scripts this playthrough.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct QuestScriptVariables {
    written: VariableMap,
}

impl Resource for QuestScriptVariables {}

impl QuestScriptVariables {
    /// The key of an auto property's backing variable, `::Name_var` — the
    /// spelling a `GetVMQuestVariable` condition names it by.
    pub fn property_key(property: &str) -> ConditionStringId {
        // Papyrus identifiers are case-insensitive, the `_var` suffix too.
        let bare = property
            .strip_prefix("::")
            .and_then(|name| {
                let split = name.len().checked_sub(4)?;
                let (stem, suffix) = (name.get(..split)?, name.get(split..)?);
                suffix.eq_ignore_ascii_case("_var").then_some(stem)
            })
            .unwrap_or(property);
        ConditionStringId::from_text(&format!("::{bare}_var"))
    }

    /// The scripted value of `variable` on `quest`, if one was written.
    pub fn get(&self, quest: QuestFormId, variable: ConditionStringId) -> Option<f32> {
        self.written.get(&quest)?.get(&variable).copied()
    }

    /// Record a scripted write.
    pub fn set(&mut self, quest: QuestFormId, variable: ConditionStringId, value: f32) {
        self.written.entry(quest).or_default().insert(variable, value);
    }

    /// Every scripted write, for the debug console and tests.
    pub fn written(&self) -> impl Iterator<Item = (QuestFormId, ConditionStringId, f32)> + '_ {
        self.written
            .iter()
            .flat_map(|(quest, vars)| vars.iter().map(move |(id, value)| (*quest, *id, *value)))
    }
}

/// The numeric property values each quest's `VMAD` authors. `NOT_SAVED_BY_DESIGN`:
/// re-derived from the load order every boot.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct QuestScriptVariableDefaults {
    authored: VariableMap,
}

impl Resource for QuestScriptVariableDefaults {}

impl QuestScriptVariableDefaults {
    /// Seed one quest's authored Int / Float / Bool property values.
    pub fn author(&mut self, quest: QuestFormId, vmad: &ScriptInstanceData) {
        let authored = self.authored.entry(quest).or_default();
        for script in &vmad.scripts {
            for property in &script.properties {
                let value = match property.value {
                    PropertyValue::Int32(value) => value as f32,
                    PropertyValue::Float(value) => value,
                    PropertyValue::Bool(value) => f32::from(u8::from(value)),
                    _ => continue,
                };
                authored
                    .entry(QuestScriptVariables::property_key(&property.name))
                    .or_insert(value);
            }
        }
    }

    pub fn get(&self, quest: QuestFormId, variable: ConditionStringId) -> Option<f32> {
        self.authored.get(&quest)?.get(&variable).copied()
    }
}

/// The value of `variable` on `quest`: the scripted write, else the authored
/// value, else `None`. Each resource guard is released before the next one is
/// taken.
pub fn quest_variable(world: &World, quest: QuestFormId, variable: ConditionStringId) -> Option<f32> {
    let written = world
        .try_resource::<QuestScriptVariables>()
        .and_then(|variables| variables.get(quest, variable));
    written.or_else(|| {
        world
            .try_resource::<QuestScriptVariableDefaults>()
            .and_then(|defaults| defaults.get(quest, variable))
    })
}

/// Install the authored values of every quest that carries a `VMAD` (replacing
/// the previous load order's), and make sure the written-values resource
/// exists without touching any writes already in it.
pub fn install_quest_script_variables<'a>(
    world: &mut World,
    quests: impl IntoIterator<Item = (u32, &'a ScriptInstanceData)>,
) -> usize {
    let mut defaults = QuestScriptVariableDefaults::default();
    let mut count = 0;
    for (quest, vmad) in quests {
        defaults.author(QuestFormId(quest), vmad);
        count += 1;
    }
    world.insert_resource(defaults);
    world.init_resource::<QuestScriptVariables>();
    world.init_resource::<QuestScriptObjects>();
    count
}

/// The non-numeric value a script variable holds (M47.2, the Papyrus VM).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ScriptObject {
    None,
    /// A form by FormID (a reference, a base object, a quest).
    Form(u32),
    Alias { quest: u32, alias: i16 },
    /// A string — a `String` variable, or a script's state name.
    Str(String),
    /// An element of a stored array or struct member.
    Int(i32),
    Float(f32),
    Bool(bool),
    /// An array a script variable holds (CK *Arrays (Papyrus)*): its
    /// elements, written back whole when a run ends.
    Array(Vec<ScriptObject>),
    /// A Fallout 4 struct a script variable holds, or one an array holds:
    /// its members by name.
    Struct(Vec<(String, ScriptObject)>),
}

/// Non-numeric script variables scripts wrote this playthrough
/// (`myActor = Alias.GetActorRef()`, a script's state), keyed like
/// [`QuestScriptVariables`] by the object the script is attached to — a
/// quest, or a reference for a reference script. Saved.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct QuestScriptObjects {
    written: HashMap<QuestFormId, HashMap<ConditionStringId, ScriptObject>>,
}

impl Resource for QuestScriptObjects {}

impl QuestScriptObjects {
    pub fn get(&self, quest: QuestFormId, variable: ConditionStringId) -> Option<ScriptObject> {
        self.written.get(&quest)?.get(&variable).cloned()
    }

    pub fn set(&mut self, quest: QuestFormId, variable: ConditionStringId, value: ScriptObject) {
        self.written.entry(quest).or_default().insert(variable, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::script_instance::{ScriptInstance, ScriptProperty};

    fn vmad(properties: Vec<(&str, PropertyValue)>) -> ScriptInstanceData {
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "MS11QuestScript".into(),
                status: 0,
                properties: properties
                    .into_iter()
                    .map(|(name, value)| ScriptProperty {
                        name: name.into(),
                        status: 1,
                        value,
                    })
                    .collect(),
            }],
        }
    }

    #[test]
    fn property_key_is_the_backing_variable_spelling_conditions_use() {
        let key = QuestScriptVariables::property_key("GetInHjerimTovaPointer");
        assert_eq!(key, ConditionStringId::from_text("::GetInHjerimTovaPointer_var"));
        // Case-insensitive, and an already-backing spelling maps to itself.
        assert_eq!(key, QuestScriptVariables::property_key("::getinhjerimtovapointer_VAR"));
    }

    #[test]
    fn writes_shadow_authored_values_and_unknown_reads_are_none() {
        let quest = QuestFormId(0x0004_CF45);
        let mut world = World::new();
        let authored = vmad(vec![
            ("FriendsKilled", PropertyValue::Int32(2)),
            ("Ratio", PropertyValue::Float(0.5)),
            ("Done", PropertyValue::Bool(true)),
            ("Target", PropertyValue::Object { form_id: 7, alias: -1 }),
        ]);
        install_quest_script_variables(&mut world, [(quest.0, &authored)]);
        let key = QuestScriptVariables::property_key;
        let read = |world: &World, name| quest_variable(world, quest, key(name));
        assert_eq!(read(&world, "FriendsKilled"), Some(2.0));
        assert_eq!(read(&world, "Ratio"), Some(0.5));
        assert_eq!(read(&world, "Done"), Some(1.0));
        assert_eq!(read(&world, "Target"), None, "objects are not numeric");
        assert_eq!(read(&world, "Unauthored"), None);
        assert_eq!(quest_variable(&world, QuestFormId(1), key("FriendsKilled")), None);

        world
            .resource_mut::<QuestScriptVariables>()
            .set(quest, key("FriendsKilled"), 3.0);
        assert_eq!(read(&world, "FriendsKilled"), Some(3.0));
    }

    /// A reinstall (a new load order) replaces the authored values and keeps
    /// the writes; replacing the written resource (a save load) keeps the
    /// authored values.
    #[test]
    fn reinstall_and_restore_each_leave_the_other_half_alone() {
        let quest = QuestFormId(0x10);
        let key = QuestScriptVariables::property_key;
        let mut world = World::new();
        install_quest_script_variables(
            &mut world,
            [(0x10, &vmad(vec![("Count", PropertyValue::Int32(1))]))],
        );
        world.resource_mut::<QuestScriptVariables>().set(quest, key("Written"), 9.0);
        install_quest_script_variables(
            &mut world,
            [(0x10, &vmad(vec![("Count", PropertyValue::Int32(4))]))],
        );
        assert_eq!(quest_variable(&world, quest, key("Count")), Some(4.0));
        assert_eq!(quest_variable(&world, quest, key("Written")), Some(9.0));

        world.insert_resource(QuestScriptVariables::default());
        assert_eq!(quest_variable(&world, quest, key("Count")), Some(4.0));
        assert_eq!(quest_variable(&world, quest, key("Written")), None);
    }
}
