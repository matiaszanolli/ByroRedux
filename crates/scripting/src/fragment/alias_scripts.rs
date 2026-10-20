//! M47.2 — Papyrus alias scripts on the VM: the scripts a quest attaches to
//! its aliases (the QUST `VMAD` alias section).
//!
//! The CK *ReferenceAlias Script*: "ReferenceAliases receive events from the
//! ObjectReference they are pointing at", and *OnAliasInit*: "Event received
//! when this alias has been filled (or not filled) as a result of the quest
//! starting up. Other aliases in the quest will have also been filled by this
//! point, and the quest is about to start its startup stage"; *OnAliasShutdown*:
//! "Event received when the quest that owns this alias has shut down".
//!
//! An alias script is an instance like a reference script, so its variables and
//! state live in the same saved stores under an owner id. The owner of an alias
//! script is a synthetic id in [`ALIAS_OWNER_BASE`]'s range, allocated by
//! [`AliasScriptOwners`] per (quest, alias) and saved with the stores it keys, so
//! two aliases carrying one script keep separate variables.
//!
//! The delivery system (`alias_script_event_system`, in `reference.rs` beside
//! the reference scripts it shares the engine's event markers with) starts a
//! running quest's alias scripts (`OnInit`, then `OnAliasInit`), relays the
//! events of the reference an alias is filled with, and shuts them down
//! (`OnAliasShutdown`) when the quest stops.

use super::*;
use byroredux_plugin::esm::records::script_instance::ScriptInstance;

/// The first owner id of an alias script instance. Its range is no plugin's:
/// the load-order slot allocator never hands a regular plugin the `0xFC` byte
/// (#5988), and the engine's own forms (`0xFF` created references) sit above it.
pub const ALIAS_OWNER_BASE: u32 = 0xFC00_0000;

/// The events an alias script receives that the engine raises: its own
/// lifecycle, and the events of the reference the alias is filled with.
pub const DELIVERED_ALIAS_EVENTS: [&str; 16] = [
    "OnInit",
    "OnAliasInit",
    "OnAliasShutdown",
    "OnCellAttach",
    "OnLoad",
    "OnCellLoad",
    "OnActivate",
    "OnTriggerEnter",
    "OnHit",
    "OnEnterBleedout",
    "OnCombatStateChanged",
    "OnLocationChange",
    "OnDying",
    "OnDeath",
    "OnCellDetach",
    "OnUnload",
];

pub use super::vm::check_alias_script;

/// One script the VM runs on a quest alias.
#[derive(Debug, Clone)]
pub struct AliasScript {
    /// The alias it is attached to.
    pub alias: i16,
    /// The script's name, as the VM's script store keys it.
    pub script: String,
    /// The script's properties: its own `VMAD` entry.
    pub vmad: Arc<ScriptInstanceData>,
    /// The [`DELIVERED_ALIAS_EVENTS`] it handles.
    pub events: Vec<String>,
}

impl AliasScript {
    pub(super) fn handles(&self, event: &str) -> bool {
        self.events.iter().any(|handled| handled.eq_ignore_ascii_case(event))
    }
}

/// The alias scripts of the load order's quests, by quest: the attached scripts
/// as authored, and — once the host has run [`check_alias_script`] on a started
/// quest's — the ones the VM runs. Derived from the QUST records and the script
/// archives, so never saved.
#[derive(Debug, Default, Clone)]
pub struct QuestAliasScripts {
    raw: HashMap<QuestFormId, Arc<Vec<(i16, ScriptInstance)>>>,
    checked: HashMap<QuestFormId, Arc<Vec<AliasScript>>>,
}

impl Resource for QuestAliasScripts {}

impl QuestAliasScripts {
    /// The scripts quest `quest` attaches to aliases, as authored.
    pub fn raw(&self, quest: QuestFormId) -> Option<&Arc<Vec<(i16, ScriptInstance)>>> {
        self.raw.get(&quest)
    }

    /// Every quest with attached alias scripts the host has not checked yet.
    pub fn unchecked(&self) -> impl Iterator<Item = (QuestFormId, &Arc<Vec<(i16, ScriptInstance)>>)> + '_ {
        self.raw
            .iter()
            .filter(|(quest, _)| !self.checked.contains_key(quest))
            .map(|(quest, scripts)| (*quest, scripts))
    }

    /// Record the scripts of `quest` the VM runs (possibly none).
    pub fn set_checked(&mut self, quest: QuestFormId, scripts: Vec<AliasScript>) {
        self.checked.insert(quest, Arc::new(scripts));
    }

    /// The scripts of `quest` the VM runs.
    pub fn scripts(&self, quest: QuestFormId) -> Option<&Arc<Vec<AliasScript>>> {
        self.checked.get(&quest)
    }

    /// Every quest whose alias scripts have been checked and include one the
    /// VM runs.
    pub fn checked_quests(&self) -> impl Iterator<Item = QuestFormId> + '_ {
        self.checked.iter().filter(|(_, scripts)| !scripts.is_empty()).map(|(quest, _)| *quest)
    }

    /// Whether any quest has attached alias scripts at all.
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }
}

/// Install the load order's authored alias scripts into [`QuestAliasScripts`];
/// returns how many alias script instances that is.
///
/// Idempotent: a second call keeps the first's table and its checked scripts.
pub fn install_quest_alias_scripts(world: &mut World, index: &byroredux_plugin::esm::records::EsmIndex) -> usize {
    if let Some(installed) = world.try_resource::<QuestAliasScripts>() {
        if !installed.is_empty() {
            return installed.raw.values().map(|scripts| scripts.len()).sum();
        }
    }
    let mut raw: HashMap<QuestFormId, Arc<Vec<(i16, ScriptInstance)>>> = HashMap::new();
    let mut count = 0usize;
    for (&form_id, quest) in index.quests.iter() {
        let scripts: Vec<(i16, ScriptInstance)> = quest
            .alias_scripts
            .iter()
            .flat_map(|alias| alias.scripts.scripts.iter().map(|script| (alias.alias_id, script.clone())))
            .collect();
        if scripts.is_empty() {
            continue;
        }
        count += scripts.len();
        raw.insert(QuestFormId(form_id), Arc::new(scripts));
    }
    world.insert_resource(QuestAliasScripts {
        raw,
        checked: HashMap::new(),
    });
    // The stores an alias script's variables and state live in.
    world.init_resource::<crate::QuestScriptVariables>();
    world.init_resource::<crate::QuestScriptObjects>();
    count
}

/// The owner ids of the alias script instances, per (quest, alias), and which
/// quests' alias scripts have started. Saved with the script stores the ids key.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "save", serde(from = "AliasOwnersSaved", into = "AliasOwnersSaved"))]
pub struct AliasScriptOwners {
    by_key: HashMap<(u32, i16), u32>,
    /// The inverse of `by_key`, shared by handle with the VM's snapshots.
    by_owner: Arc<HashMap<u32, (u32, i16)>>,
    next: u32,
    started: HashSet<u32>,
}

impl Resource for AliasScriptOwners {}

/// [`AliasScriptOwners`] as saved: the allocations, the counter and the
/// started quests; the inverse map is rebuilt.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct AliasOwnersSaved {
    pub owners: Vec<(u32, i16, u32)>,
    pub next: u32,
    pub started: Vec<u32>,
}

impl From<AliasOwnersSaved> for AliasScriptOwners {
    fn from(saved: AliasOwnersSaved) -> Self {
        let by_key: HashMap<(u32, i16), u32> =
            saved.owners.iter().map(|&(quest, alias, owner)| ((quest, alias), owner)).collect();
        let by_owner = saved.owners.iter().map(|&(quest, alias, owner)| (owner, (quest, alias))).collect();
        Self {
            by_key,
            by_owner: Arc::new(by_owner),
            next: saved.next,
            started: saved.started.into_iter().collect(),
        }
    }
}

impl From<AliasScriptOwners> for AliasOwnersSaved {
    fn from(owners: AliasScriptOwners) -> Self {
        let mut allocated: Vec<(u32, i16, u32)> =
            owners.by_key.iter().map(|(&(quest, alias), &owner)| (quest, alias, owner)).collect();
        allocated.sort_unstable();
        let mut started: Vec<u32> = owners.started.into_iter().collect();
        started.sort_unstable();
        Self {
            owners: allocated,
            next: owners.next,
            started,
        }
    }
}

impl AliasScriptOwners {
    /// The owner id of `alias` of `quest`, allocating it on first use.
    pub fn ensure(&mut self, quest: QuestFormId, alias: i16) -> Option<u32> {
        if let Some(&owner) = self.by_key.get(&(quest.0, alias)) {
            return Some(owner);
        }
        if self.next > 0x00FF_FFFF {
            return None;
        }
        let owner = ALIAS_OWNER_BASE | self.next;
        self.next += 1;
        self.by_key.insert((quest.0, alias), owner);
        Arc::make_mut(&mut self.by_owner).insert(owner, (quest.0, alias));
        Some(owner)
    }

    /// The owner id of `alias` of `quest`, if one was allocated.
    pub fn owner_of(&self, quest: QuestFormId, alias: i16) -> Option<u32> {
        self.by_key.get(&(quest.0, alias)).copied()
    }

    /// The (quest, alias) an owner id stands for.
    pub fn alias_of(&self, owner: u32) -> Option<(u32, i16)> {
        self.by_owner.get(&owner).copied()
    }

    /// The owner → (quest, alias) table, by handle: what a Papyrus run snapshots
    /// to know what `Self` is.
    pub fn table(&self) -> Arc<HashMap<u32, (u32, i16)>> {
        Arc::clone(&self.by_owner)
    }

    pub fn is_started(&self, quest: QuestFormId) -> bool {
        self.started.contains(&quest.0)
    }

    pub fn mark_started(&mut self, quest: QuestFormId) {
        self.started.insert(quest.0);
    }

    pub fn mark_stopped(&mut self, quest: QuestFormId) {
        self.started.remove(&quest.0);
    }

    /// The quests whose alias scripts have started.
    pub fn started_quests(&self) -> Vec<QuestFormId> {
        let mut quests: Vec<QuestFormId> = self.started.iter().map(|&quest| QuestFormId(quest)).collect();
        quests.sort_unstable_by_key(|quest| quest.0);
        quests
    }
}

/// Whether `owner` is an alias script owner id.
pub fn is_alias_owner(owner: u32) -> bool {
    owner >> 24 == ALIAS_OWNER_BASE >> 24
}

/// The standing of an alias-script owner: `Some(true)` while its quest runs,
/// `Some(false)` once it stopped, `None` when `owner` is no alias owner.
pub(super) fn alias_owner_standing(world: &World, owner: u32, stages: &QuestStageState) -> Option<bool> {
    if !is_alias_owner(owner) {
        return None;
    }
    let (quest, _) = world.try_resource::<AliasScriptOwners>()?.alias_of(owner)?;
    Some(stages.is_running(QuestFormId(quest)))
}

/// The properties of `owner`'s alias instance of `script`, when `owner` is an
/// alias owner.
pub(super) fn alias_owner_vmad(world: &World, owner: u32, script: &str) -> Option<ScriptInstanceData> {
    if !is_alias_owner(owner) {
        return None;
    }
    let (quest, alias) = world.try_resource::<AliasScriptOwners>()?.alias_of(owner)?;
    let scripts = world.try_resource::<QuestAliasScripts>()?.scripts(QuestFormId(quest))?.clone();
    scripts
        .iter()
        .find(|attached| attached.alias == alias && attached.script.eq_ignore_ascii_case(script))
        .map(|attached| (*attached.vmad).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_are_allocated_once_per_alias_and_round_trip_their_inverse() {
        let mut owners = AliasScriptOwners::default();
        let first = owners.ensure(QuestFormId(0x10), 3).unwrap();
        let second = owners.ensure(QuestFormId(0x10), 4).unwrap();
        assert_eq!(first, ALIAS_OWNER_BASE);
        assert_eq!(second, ALIAS_OWNER_BASE + 1);
        assert_eq!(owners.ensure(QuestFormId(0x10), 3), Some(first), "stable");
        assert_eq!(owners.alias_of(second), Some((0x10, 4)));
        assert!(is_alias_owner(first) && !is_alias_owner(0x0001_0000));
        owners.mark_started(QuestFormId(0x10));
        assert!(owners.is_started(QuestFormId(0x10)));
        owners.mark_stopped(QuestFormId(0x10));
        assert!(!owners.is_started(QuestFormId(0x10)));
    }

    #[test]
    fn the_saved_shape_rebuilds_the_inverse() {
        let mut owners = AliasScriptOwners::default();
        owners.ensure(QuestFormId(7), 1);
        owners.ensure(QuestFormId(8), 2);
        owners.mark_started(QuestFormId(8));
        let restored = AliasScriptOwners::from(AliasOwnersSaved::from(owners.clone()));
        assert_eq!(restored, owners);
        assert_eq!(restored.alias_of(ALIAS_OWNER_BASE + 1), Some((8, 2)));
        assert_eq!(restored.table().len(), 2);
    }
}
