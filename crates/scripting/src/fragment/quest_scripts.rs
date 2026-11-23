//! A quest's own Papyrus scripts (the QUST `VMAD` script section) on the VM: their `OnInit`.
//!
//! The CK *OnInit*: "OnInit is called at the following times: For Quests and Aliases: On game startup,
//! and again whenever the quest starts, due to the quest being reset." The first is delivered here,
//! once per save and quest script (the gate is the saved `::init::` key among the quest's script
//! variables, the one reference scripts use), so a quest a mod adds to an existing save gets it on its
//! first load too. The second, a non-Run-Once quest's reset on start (variables and properties back to
//! their defaults, then `OnInit` again), is not modelled yet.
//!
//! Checking a script decompiles it, so the host checks a few per frame: the running quests' first,
//! then the rest of the load order's ([`QuestOwnScripts::unchecked`]).

use super::*;

/// What the engine calls on a quest's own script: `OnInit`, and on a Mod Configuration Menu script (one
/// extending the engine's `SKI_ConfigBase`, [`crate::mcm`]) the game-load event and the panel's entry
/// point. A script is checked over each it handles, so a config script's option handlers are checked
/// through `_McmEvent` before any of them runs.
pub const DELIVERED_QUEST_EVENTS: [&str; 3] = ["OnInit", "OnGameReload", "_McmEvent"];

/// One script the VM runs on a quest.
#[derive(Debug, Clone)]
pub struct QuestScript {
    /// The script's name, as the VM's script store keys it.
    pub script: String,
    /// The script's properties: its own `VMAD` entry.
    pub vmad: Arc<ScriptInstanceData>,
    /// The [`DELIVERED_QUEST_EVENTS`] it handles.
    pub events: Vec<String>,
    /// The scripts it extends, lowercase, most derived first (itself excluded).
    pub extends: Vec<String>,
}

impl QuestScript {
    pub(super) fn handles(&self, event: &str) -> bool {
        self.events.iter().any(|handled| handled.eq_ignore_ascii_case(event))
    }

    /// Whether it extends `script` (lowercase), directly or not.
    pub fn extends(&self, script: &str) -> bool {
        self.extends.iter().any(|parent| parent == script)
    }
}

/// The quests whose own scripts the host has checked, with the ones the VM runs (possibly none).
/// Derived from the QUST records and the script archives, so never saved.
#[derive(Debug, Default, Clone)]
pub struct QuestOwnScripts {
    checked: HashMap<QuestFormId, Arc<Vec<QuestScript>>>,
}

impl Resource for QuestOwnScripts {}

impl QuestOwnScripts {
    /// The quests of `instances` not checked yet.
    pub fn unchecked<'a>(&'a self, instances: &'a QuestScriptInstances) -> impl Iterator<Item = QuestFormId> + 'a {
        instances.0.keys().copied().filter(|quest| !self.checked.contains_key(quest))
    }

    /// Record the scripts of `quest` the VM runs (possibly none).
    pub fn set_checked(&mut self, quest: QuestFormId, scripts: Vec<QuestScript>) {
        self.checked.insert(quest, Arc::new(scripts));
    }

    /// Every checked quest with a script the VM runs, and those scripts.
    pub fn running_scripts(&self) -> impl Iterator<Item = (QuestFormId, &Arc<Vec<QuestScript>>)> + '_ {
        self.checked.iter().filter(|(_, scripts)| !scripts.is_empty()).map(|(quest, scripts)| (*quest, scripts))
    }
}
