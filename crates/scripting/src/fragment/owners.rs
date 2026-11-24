//! M47.2 — who a script instance belongs to: the lookups the delivery systems
//! (updates, distance events, custom / remote events, hit events) share.
//!
//! An instance's owner id is a quest's FormID, a placed reference's FormID, or
//! an alias script's owner id ([`super::AliasScriptOwners`]). A quest delivers
//! while it runs and drops its registrations when it stops; an alias script
//! follows its quest; a reference's registration waits while the reference is
//! unloaded.

use super::*;

/// The scripts quests carry and the VM's installed scripts, snapshotted once for
/// a delivery pass.
pub(super) struct ScriptOwners {
    quests: Arc<HashMap<QuestFormId, ScriptInstanceData>>,
    fragments: QuestStageFragments,
}

impl ScriptOwners {
    pub(super) fn new(world: &World) -> Self {
        Self {
            quests: world
                .try_resource::<crate::QuestScriptInstances>()
                .map(|instances| Arc::clone(&instances.0))
                .unwrap_or_default(),
            fragments: world.resource::<QuestStageFragments>().clone(),
        }
    }

    /// The installed scripts, by lowercased name.
    pub(super) fn scripts(&self) -> Arc<HashMap<String, Arc<Script>>> {
        self.fragments.vm_scripts()
    }

    /// Whether `owner` is a quest.
    pub(super) fn is_quest(&self, owner: u32) -> bool {
        let quest = QuestFormId(owner);
        self.quests.contains_key(&quest) || self.fragments.vmad(quest).is_some()
    }

    /// What `owner`'s instance is attached to.
    pub(super) fn kind(&self, owner: u32) -> super::vm::SelfKind {
        super::vm::SelfKind::of_owner(owner, self.is_quest(owner))
    }

    /// The standing of `owner` — `Some(true)` while its quest (or its alias's
    /// quest) runs, `Some(false)` once that stopped, `None` for a reference —
    /// read under `stages`.
    pub(super) fn standing(&self, world: &World, stages: &QuestStageState, owner: u32) -> Option<bool> {
        let quest = QuestFormId(owner);
        (self.is_quest(owner) || stages.is_started(quest))
            .then(|| stages.is_running(quest))
            .or_else(|| super::alias_owner_standing(world, owner, stages))
    }

    /// The properties of `owner`'s instance of `script`: a quest's, an alias
    /// script's, or an attached reference script's.
    pub(super) fn vmad(&self, world: &World, owner: u32, script: &str) -> Option<ScriptInstanceData> {
        let quest = QuestFormId(owner);
        if let Some(vmad) = self.quests.get(&quest).or_else(|| self.fragments.vmad(quest)) {
            return Some(vmad.clone());
        }
        if super::is_alias_owner(owner) {
            return super::alias_owner_vmad(world, owner, script);
        }
        ReferenceScriptIndex::lookup(world, owner).and_then(|entity| {
            let attached = world.get::<PapyrusReferenceScripts>(entity)?;
            attached
                .0
                .iter()
                .find(|attached| attached.script.eq_ignore_ascii_case(script))
                .map(|attached| (*attached.vmad).clone())
        })
    }

    /// The scripts attached to `owner`: a quest's, an alias's (its owner id), or a loaded reference's.
    pub(super) fn attached_scripts(&self, world: &World, owner: u32) -> Vec<String> {
        let quest = QuestFormId(owner);
        if let Some(vmad) = self.quests.get(&quest).or_else(|| self.fragments.vmad(quest)) {
            return vmad.scripts.iter().map(|script| script.name.clone()).collect();
        }
        if super::is_alias_owner(owner) {
            let Some((quest, alias)) = world.try_resource::<AliasScriptOwners>().and_then(|owners| owners.alias_of(owner)) else {
                return Vec::new();
            };
            return world
                .try_resource::<QuestAliasScripts>()
                .and_then(|scripts| scripts.scripts(QuestFormId(quest)).cloned())
                .map(|scripts| scripts.iter().filter(|attached| attached.alias == alias).map(|attached| attached.script.clone()).collect())
                .unwrap_or_default();
        }
        ReferenceScriptIndex::lookup(world, owner)
            .and_then(|entity| world.get::<PapyrusReferenceScripts>(entity).map(|attached| attached.0.iter().map(|a| a.script.clone()).collect()))
            .unwrap_or_default()
    }

    /// Whether `handler` of `owner`'s instance of `script` runs whole on the VM.
    pub(super) fn handler_runs(&self, owner: u32, script: &str, handler: &str) -> bool {
        super::vm::handler_runs(&self.scripts(), script, handler, self.kind(owner))
    }
}
