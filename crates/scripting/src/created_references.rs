//! Created Object "At" quest aliases (M47.2).
//!
//! The CK *Quest Alias Tab*: "Create Reference to Object: When the quest
//! starts, a reference will be created using the selected base object. The
//! new reference can either be placed at the same location as another
//! Reference Alias (usually a marker), or in the inventory of another
//! Reference Alias". The "In" half grants an item (`quest_alias.rs`); this
//! module is the "At" half — 441 Skyrim and 1,175 FO4 aliases, 1,389 of them
//! `NPC_` spawns (`crates/plugin/examples/alias_created_object_census.rs`).
//!
//! The alias refresh records one [`CreatedReference`] per created alias of a
//! running quest once its target alias is bound, with a fresh FormID in the
//! created-object plugin slot `0xFF` (UESP *Skyrim Mod:Save File Format*:
//! "Created (ie, plugin index of 0xFF)"). The engine resolves where it stands
//! — the target reference's placement — and spawns it whenever that cell is
//! loaded; the refresh then binds the alias to the spawned entity like any
//! other reference. Records are saved, so the reference keeps its identity
//! and place across reloads. A stopped quest's records are dropped: its
//! created references no longer respawn.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;

use crate::quest_stages::QuestFormId;

/// The created-object plugin slot's first FormID.
pub const CREATED_FORM_ID_BASE: u32 = 0xFF00_0000;

/// The first FormID a script-created reference takes (`PlaceAtMe`): the upper
/// half of the created slot, so script creation and the alias refresh never
/// draw from the same counter.
pub const SCRIPTED_FORM_ID_BASE: u32 = 0xFF80_0000;

/// The next script-created FormID's low 23 bits, shared by handle so a
/// Papyrus run draws an ID while it holds the quest-state guards without
/// taking the [`CreatedReferences`] lock. Saved as the plain count.
#[derive(Debug, Clone, Default)]
pub struct ScriptedIds(std::sync::Arc<std::sync::atomic::AtomicU32>);

impl ScriptedIds {
    /// Draw the next FormID; `None` once the half-slot is exhausted.
    pub fn next(&self) -> Option<u32> {
        use std::sync::atomic::Ordering;
        let index = self.0.fetch_add(1, Ordering::Relaxed);
        if index > 0x007F_FFFF {
            self.0.store(0x0080_0000, Ordering::Relaxed);
            return None;
        }
        Some(SCRIPTED_FORM_ID_BASE | index)
    }

    fn count(&self) -> u32 {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl PartialEq for ScriptedIds {
    fn eq(&self, other: &Self) -> bool {
        self.count() == other.count()
    }
}

#[cfg(feature = "save")]
impl serde::Serialize for ScriptedIds {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.count().serialize(serializer)
    }
}

#[cfg(feature = "save")]
impl<'de> serde::Deserialize<'de> for ScriptedIds {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let count = u32::deserialize(deserializer)?;
        Ok(Self(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(count))))
    }
}

/// The cell a created reference stands in.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum CreatedCell {
    /// An interior cell, by editor ID.
    Interior { editor_id: String },
    /// An exterior grid cell.
    Exterior { worldspace: String, grid: (i32, i32) },
}

/// Where a created reference stands: its cell and its pose in Bethesda Z-up
/// world units (the placement convention `PlacedRef` uses).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct CreatedPlacement {
    pub cell: CreatedCell,
    pub position_zup: [f32; 3],
    pub rotation_zup: [f32; 3],
}

/// A created reference's `DeleteWhenAble` state (M47.2). The CK
/// *DeleteWhenAble* waits "for this reference to lose its parent cell, or
/// for its parent cell to become detached, then deletes it", and *Delete*
/// says "Only references created in game can actually be deleted". A deleted
/// record stays as a tombstone, so the alias refresh does not create the
/// reference again while its quest runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum CreatedDeletion {
    #[default]
    Kept,
    /// Delete once its cell is no longer loaded.
    WhenAble,
    Deleted,
}

/// One created reference.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct CreatedReference {
    /// Its FormID, in the `0xFF` created-object slot.
    pub form_id: u32,
    /// The base object (`ALCO`).
    pub base: u32,
    /// `ALCL`'s encounter level, as authored.
    pub level: u32,
    /// The reference it is created at — the target alias's reference.
    pub at_reference: u32,
    /// Resolved by the engine from `at_reference`; `None` until then.
    pub placement: Option<CreatedPlacement>,
    pub deletion: CreatedDeletion,
}

/// Every created reference of a running quest, by quest and alias. Saved.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct CreatedReferences {
    by_alias: HashMap<QuestFormId, HashMap<i32, CreatedReference>>,
    /// The next created FormID's low 24 bits.
    next: u32,
    /// References scripts created with `PlaceAtMe`, which no quest alias owns.
    scripted: Vec<CreatedReference>,
    scripted_ids: ScriptedIds,
}

impl Resource for CreatedReferences {}

impl CreatedReferences {
    pub fn get(&self, quest: QuestFormId, alias_id: i32) -> Option<&CreatedReference> {
        self.by_alias.get(&quest)?.get(&alias_id)
    }

    /// Record a new created reference for `quest`'s alias, returning its
    /// FormID. `None` once the 24-bit created slot is exhausted.
    pub fn create(
        &mut self,
        quest: QuestFormId,
        alias_id: i32,
        base: u32,
        level: u32,
        at_reference: u32,
    ) -> Option<u32> {
        if self.next > 0x00FF_FFFF {
            return None;
        }
        let form_id = CREATED_FORM_ID_BASE | self.next;
        self.next += 1;
        self.by_alias.entry(quest).or_default().insert(
            alias_id,
            CreatedReference {
                form_id,
                base,
                level,
                at_reference,
                placement: None,
                deletion: CreatedDeletion::Kept,
            },
        );
        Some(form_id)
    }

    /// The handle a Papyrus run draws script-created FormIDs through.
    pub fn scripted_ids(&self) -> ScriptedIds {
        self.scripted_ids.clone()
    }

    /// Record a reference a script created with `PlaceAtMe` at
    /// `at_reference`, under the FormID `scripted_ids` handed out.
    pub fn create_scripted(&mut self, form_id: u32, base: u32, at_reference: u32, delete_when_able: bool) {
        self.scripted.push(CreatedReference {
            form_id,
            base,
            level: 0,
            at_reference,
            placement: None,
            deletion: if delete_when_able {
                CreatedDeletion::WhenAble
            } else {
                CreatedDeletion::Kept
            },
        });
    }

    /// Drop the records of quests `running` rejects.
    pub fn retain_quests(&mut self, mut running: impl FnMut(QuestFormId) -> bool) {
        self.by_alias.retain(|quest, _| running(*quest));
    }

    /// Every record, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = &CreatedReference> + '_ {
        self.by_alias
            .values()
            .flat_map(|aliases| aliases.values())
            .chain(self.scripted.iter())
    }

    /// `DeleteWhenAble` on `form_id`; `false` when it is no created
    /// reference (an authored one cannot be deleted).
    pub fn delete_when_able(&mut self, form_id: u32) -> bool {
        match self.record_mut(form_id) {
            Some(record) => {
                if record.deletion == CreatedDeletion::Kept {
                    record.deletion = CreatedDeletion::WhenAble;
                }
                true
            }
            None => false,
        }
    }

    /// Delete `form_id` (its cell is gone): it never spawns again.
    pub fn mark_deleted(&mut self, form_id: u32) {
        if let Some(record) = self.record_mut(form_id) {
            record.deletion = CreatedDeletion::Deleted;
        }
    }

    fn record_mut(&mut self, form_id: u32) -> Option<&mut CreatedReference> {
        self.by_alias
            .values_mut()
            .flat_map(|aliases| aliases.values_mut())
            .chain(self.scripted.iter_mut())
            .find(|record| record.form_id == form_id)
    }

    /// Record where `form_id` stands.
    pub fn place(&mut self, form_id: u32, placement: CreatedPlacement) {
        if let Some(record) = self.record_mut(form_id) {
            record.placement = Some(placement);
        }
    }
}

/// XCT-LOC-001 (PRD cross-cell-actor-travel) — the relocation ledger: where
/// each persistent actor that has left its home cell for another space now
/// is, as its cell and pose. The cell loader spawns a ledgered actor there and
/// never in its home cell (XCT-LOC-002). Keyed by the actor reference's
/// load-order FormID. Saved (XCT-LOC-003), as the original saves an actor's
/// moved cell and position (UESP *Save File Format/Location Data*).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ActorRelocations {
    placements: std::collections::BTreeMap<u32, CreatedPlacement>,
}

impl Resource for ActorRelocations {}

impl ActorRelocations {
    /// Record that `actor` is now at `placement`.
    pub fn relocate(&mut self, actor: u32, placement: CreatedPlacement) {
        self.placements.insert(actor, placement);
    }

    /// Where `actor` is, if it has moved.
    pub fn placement(&self, actor: u32) -> Option<&CreatedPlacement> {
        self.placements.get(&actor)
    }

    pub fn contains(&self, actor: u32) -> bool {
        self.placements.contains_key(&actor)
    }

    /// Every moved actor and its placement, in FormID order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &CreatedPlacement)> {
        self.placements.iter().map(|(&actor, placement)| (actor, placement))
    }

    pub fn is_empty(&self) -> bool {
        self.placements.is_empty()
    }

    /// A digest of which actor is in which cell (not their poses): what the
    /// spawn step's gate needs to see change.
    pub fn digest(&self) -> u64 {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut hasher = std::hash::BuildHasherDefault::<std::collections::hash_map::DefaultHasher>::default().build_hasher();
        for (actor, placement) in &self.placements {
            actor.hash(&mut hasher);
            placement.cell.hash(&mut hasher);
        }
        hasher.finish()
    }
}

/// A request to move an actor to another space: a Travel hop through a load
/// door (XCT-ROUTE-002) or a scripted move. The engine records it in
/// [`ActorRelocations`], takes the actor out of the cell it leaves and spawns it
/// in the one it enters once that cell is loaded. `NOT_SAVED_BY_DESIGN`:
/// drained the frame it is raised.
#[derive(Debug, Default)]
pub struct PendingRelocations(pub Vec<(u32, CreatedPlacement)>);

impl Resource for PendingRelocations {}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub: created-object references and the relocation ledger
/// (saved), and the relocation requests.
pub fn install(world: &mut World) {
    world.init_resource::<CreatedReferences>();
    world.init_resource::<ActorRelocations>();
    world.init_resource::<PendingRelocations>();
}

/// #5802 — this subsystem's saved columns. Called by the binary's
/// `build_save_registry` at its place in the row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<CreatedReferences>("CreatedReferences");
    registry.register_resource::<ActorRelocations>("ActorRelocations");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_form_ids_live_in_the_0xff_slot_and_records_follow_their_quest() {
        let mut created = CreatedReferences::default();
        let first = created.create(QuestFormId(1), 3, 0xA, 0, 0x55).unwrap();
        let second = created.create(QuestFormId(2), 4, 0xB, 1, 0x56).unwrap();
        assert_eq!(first, 0xFF00_0000);
        assert_eq!(second, 0xFF00_0001);
        assert_eq!(created.get(QuestFormId(1), 3).unwrap().at_reference, 0x55);

        let placement = CreatedPlacement {
            cell: CreatedCell::Interior {
                editor_id: "MQ101".into(),
            },
            position_zup: [1.0, 2.0, 3.0],
            rotation_zup: [0.0; 3],
        };
        created.place(second, placement.clone());
        assert_eq!(created.get(QuestFormId(2), 4).unwrap().placement, Some(placement));

        created.retain_quests(|quest| quest == QuestFormId(2));
        assert!(created.get(QuestFormId(1), 3).is_none());
        // FormIDs are never reused.
        assert_eq!(created.create(QuestFormId(1), 3, 0xA, 0, 0x55), Some(0xFF00_0002));

        // A script-created reference draws from the upper half of the slot
        // through the shared handle, belongs to no quest, and can be deleted.
        let ids = created.scripted_ids();
        let (first, second) = (ids.next().unwrap(), ids.next().unwrap());
        assert_eq!((first, second), (0xFF80_0000, 0xFF80_0001));
        created.create_scripted(first, 0xC, 0x77, true);
        created.retain_quests(|_| false);
        assert!(created.iter().any(|record| record.form_id == first));
        assert_eq!(
            created.iter().find(|record| record.form_id == first).unwrap().deletion,
            CreatedDeletion::WhenAble
        );
        created.mark_deleted(first);
        assert_eq!(
            created.iter().find(|record| record.form_id == first).unwrap().deletion,
            CreatedDeletion::Deleted
        );
        assert_eq!(created.scripted_ids().next(), Some(0xFF80_0002), "the handle is shared");
    }
}
