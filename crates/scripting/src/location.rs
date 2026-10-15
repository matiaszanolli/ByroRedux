//! Locations (M47.2) — the `LCTN` runtime that location conditions and
//! location aliases read.
//!
//! Every Creation-era reference stands in a cell, and a cell may be tagged
//! with a location (`CELL.XLCN`). Locations nest (`LCTN.PNAM`): Dragonsreach
//! is in Whiterun, Whiterun is in Whiterun Hold, Whiterun Hold is in Tamriel.
//! The CK condition functions read that hierarchy:
//!
//! - **Current location** — the location of the cell the reference stands
//!   in now. `GetInCurrentLoc L` is true when it "is (or has a parent
//!   location of)" `L`.
//! - **Editor location** — where the reference was placed. A persistent
//!   reference lives in the worldspace's persistent cell, so its location
//!   comes from the `LCTN`'s persistent / unique reference tables
//!   (`ACPR`/`LCPR`/`RCPR`, `ACUN`/`LCUN`); any other reference's is its
//!   placed cell's (`GetIsEditorLoc`: "the reference's Editor Location is a
//!   cell which is (or has a parent location of)" `L`).
//!
//! Sources: the CK wiki's `GetInCurrentLoc`, `GetIsEditorLoc`,
//! `LocationHasKeyword` and *Quest Alias Tab* pages; the record layouts are
//! xEdit's `wbDefinitionsTES5` `LCTN` / `CELL`.
//!
//! Cell-loaded entities find their cell through `CellRoot` → `CellFormId`.
//! The player is not cell-owned (it survives every transition), so the
//! engine publishes the cell it occupies as [`PlayerLocation`].

use std::collections::HashMap;

use byroredux_core::ecs::components::{CellFormId, CellRoot};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

use crate::quest_stages::QuestFormId;

/// The load order's locations, flattened for lookups. `NOT_SAVED_BY_DESIGN`:
/// re-derived from the load order every boot.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LocationTable {
    parents: HashMap<u32, u32>,
    keywords: HashMap<u32, Vec<u32>>,
    cell_locations: HashMap<u32, u32>,
    editor_locations: HashMap<u32, u32>,
    /// Per location, its `(location ref type, reference)` static refs.
    ref_types: HashMap<u32, Vec<(u32, u32)>>,
    /// Per location, its `Boss`-tagged references (the `LCRT` editor ID
    /// `Boss`), and per such reference the locations it is a boss of.
    bosses: HashMap<u32, Vec<u32>>,
    boss_of: HashMap<u32, Vec<u32>>,
    /// Every location, by FormID — the pool a "find matching location"
    /// alias fill draws from.
    locations: Vec<u32>,
}

impl Resource for LocationTable {}

impl LocationTable {
    pub fn from_index(index: &EsmIndex) -> Self {
        let mut table = Self::default();
        for location in index.locations.values() {
            table.locations.push(location.form_id);
            if let Some(parent) = location.parent {
                table.parents.insert(location.form_id, parent);
            }
            if !location.keywords.is_empty() {
                table
                    .keywords
                    .insert(location.form_id, location.keywords.clone());
            }
            for &(reference, editor_location) in &location.editor_location_refs {
                table.editor_locations.insert(reference, editor_location);
            }
            if !location.ref_type_refs.is_empty() {
                table
                    .ref_types
                    .insert(location.form_id, location.ref_type_refs.clone());
            }
        }
        table.locations.sort_unstable();
        let boss_types: Vec<u32> = index
            .location_ref_types
            .values()
            .filter(|ref_type| ref_type.editor_id.eq_ignore_ascii_case("Boss"))
            .map(|ref_type| ref_type.form_id)
            .collect();
        for location in index.locations.values() {
            for &(kind, reference) in &location.ref_type_refs {
                if boss_types.contains(&kind) {
                    table.bosses.entry(location.form_id).or_default().push(reference);
                    table.boss_of.entry(reference).or_default().push(location.form_id);
                }
            }
        }
        let exterior = index
            .cells
            .exterior_cells
            .values()
            .flat_map(|grid| grid.values());
        for cell in index.cells.cells.values().chain(exterior) {
            if let Some(location) = cell.location_form {
                table.cell_locations.insert(cell.form_id, location);
            }
        }
        table
    }

    pub fn is_empty(&self) -> bool {
        self.locations.is_empty()
    }

    /// Every location, ascending by FormID.
    pub fn locations(&self) -> &[u32] {
        &self.locations
    }

    pub fn parent(&self, location: u32) -> Option<u32> {
        self.parents.get(&location).copied()
    }

    /// `location` itself, then each parent up the hierarchy. Bounded by the
    /// table size, so a cyclic `PNAM` chain (a mod can author one) ends.
    pub fn lineage(&self, location: u32) -> impl Iterator<Item = u32> + '_ {
        std::iter::successors(Some(location), |&current| self.parent(current))
            .take(self.locations.len().max(1) + 1)
    }

    /// Whether `location` is `ancestor` or has it as a parent location.
    pub fn is_within(&self, location: u32, ancestor: u32) -> bool {
        self.lineage(location).any(|current| current == ancestor)
    }

    /// Whether `location` itself carries `keyword`.
    pub fn has_keyword(&self, location: u32, keyword: u32) -> bool {
        self.keywords
            .get(&location)
            .is_some_and(|keywords| keywords.contains(&keyword))
    }

    /// The nearest of `location` and its parents that carries `keyword` —
    /// a "Reference Alias Location" fill's keyword-selected parent.
    pub fn with_keyword(&self, location: u32, keyword: u32) -> Option<u32> {
        self.lineage(location)
            .find(|&current| self.has_keyword(current, keyword))
    }

    /// The references `location` tags with location ref type `ref_type`.
    pub fn refs_of_type(&self, location: u32, ref_type: u32) -> impl Iterator<Item = u32> + '_ {
        self.ref_types
            .get(&location)
            .into_iter()
            .flatten()
            .filter(move |(kind, _)| *kind == ref_type)
            .map(|(_, reference)| *reference)
    }

    /// The locations `reference` is a `Boss` of.
    pub fn boss_locations(&self, reference: u32) -> &[u32] {
        self.boss_of.get(&reference).map_or(&[], Vec::as_slice)
    }

    /// The `Boss` references of `location`.
    pub fn bosses(&self, location: u32) -> &[u32] {
        self.bosses.get(&location).map_or(&[], Vec::as_slice)
    }

    /// The location a cell is tagged with (`XLCN`).
    pub fn cell_location(&self, cell: u32) -> Option<u32> {
        self.cell_locations.get(&cell).copied()
    }

    /// The editor location an `LCTN` lists a persistent reference under.
    pub fn listed_editor_location(&self, reference: u32) -> Option<u32> {
        self.editor_locations.get(&reference).copied()
    }
}

/// The location of the cell the player occupies, published by the engine as
/// the player moves (`None` in an untagged cell). Not saved: re-derived from
/// the loaded cell.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PlayerLocation(pub Option<u32>);

impl Resource for PlayerLocation {}

/// The location of the cell `entity` belongs to, through its `CellRoot`.
fn cell_location_of(world: &World, entity: EntityId) -> Option<u32> {
    let root = world.get::<CellRoot>(entity).map_or(entity, |root| root.0);
    let cell = world.get::<CellFormId>(root)?.0;
    world.try_resource::<LocationTable>()?.cell_location(cell)
}

/// CK *IsLoaded - Location*: "if this location is loaded or not" — whether a
/// resident cell is one of the location's own (a loaded child location does not
/// load its parent). `None` when `location` is not a known `LCTN`.
pub fn location_loaded(world: &World, location: u32) -> Option<bool> {
    let table = world.try_resource::<LocationTable>()?;
    if !table.locations().contains(&location) {
        return None;
    }
    let loaded = world
        .query::<CellFormId>()
        .is_some_and(|cells| cells.iter().any(|(_, cell)| table.cell_location(cell.0) == Some(location)));
    Some(loaded)
}

fn is_player(world: &World, entity: EntityId) -> bool {
    crate::papyrus_demo::PapyrusPlayerEntity::get(world).is_some_and(|player| player == entity)
}

/// The current location of `entity`: the player's published location, else
/// its cell's.
pub fn current_location(world: &World, entity: EntityId) -> Option<u32> {
    if is_player(world, entity) {
        return world.try_resource::<PlayerLocation>()?.0;
    }
    cell_location_of(world, entity)
}

/// The editor location of `entity`: the location whose persistent tables list
/// it, else the location of the cell it was placed in. A cell-loaded
/// reference never leaves the cell root it was spawned under, so that cell
/// is its placed cell. The player has no editor location.
pub fn editor_location(world: &World, entity: EntityId) -> Option<u32> {
    if is_player(world, entity) {
        return None;
    }
    // A remote alias stub (a forced reference outside the loaded cells)
    // carries no `FormIdComponent`, only its candidate's reference FormID.
    let reference = crate::condition::entity_global_form_id(world, entity).or_else(|| {
        world
            .get::<crate::scene::SceneAliasCandidate>(entity)
            .map(|candidate| candidate.reference_form_id)
    });
    let listed = reference.and_then(|reference| {
        world
            .try_resource::<LocationTable>()?
            .listed_editor_location(reference)
    });
    listed.or_else(|| cell_location_of(world, entity))
}

/// The locations bound to quest location aliases (`ALLS`): per quest, alias
/// id → `LCTN` FormID. A location alias is assigned when its quest starts and
/// holds until the quest stops. Saved — a "Find Matching Location" fill picks
/// at random, so a reload must keep the pick.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct LocationAliasBindings {
    bound: HashMap<QuestFormId, HashMap<i32, u32>>,
}

impl Resource for LocationAliasBindings {}

impl LocationAliasBindings {
    pub fn resolve(&self, quest: QuestFormId, alias_id: i32) -> Option<u32> {
        self.bound.get(&quest)?.get(&alias_id).copied()
    }

    pub fn bind(&mut self, quest: QuestFormId, alias_id: i32, location: u32) {
        self.bound
            .entry(quest)
            .or_default()
            .insert(alias_id, location);
    }

    /// Replace every binding with the fill pass's result.
    pub fn replace(&mut self, bindings: HashMap<(QuestFormId, i32), u32>) {
        self.bound.clear();
        for ((quest, alias_id), location) in bindings {
            self.bind(quest, alias_id, location);
        }
    }

    /// Every binding, for the fill pass and the debug console.
    pub fn iter(&self) -> impl Iterator<Item = ((QuestFormId, i32), u32)> + '_ {
        self.bound.iter().flat_map(|(quest, aliases)| {
            aliases
                .iter()
                .map(move |(alias, location)| ((*quest, *alias), *location))
        })
    }
}

/// Cleared locations (M47.2). The CK *Cell Reset* page: "Locations are
/// cleared when all of their bosses (creatures tagged with the 'Boss'
/// LocRefType) are dead [or] a script calls `SetCleared()` on the location."
/// The dead bosses are kept by reference FormID, so a boss killed in one
/// visit still counts after its cell unloads. Saved.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct LocationClearState {
    cleared: std::collections::HashSet<u32>,
    dead_bosses: std::collections::HashSet<u32>,
}

impl Resource for LocationClearState {}

impl LocationClearState {
    pub fn is_cleared(&self, location: u32) -> bool {
        self.cleared.contains(&location)
    }

    /// `Location.SetCleared(abCleared)`.
    pub fn set_cleared(&mut self, location: u32, cleared: bool) {
        if cleared {
            self.cleared.insert(location);
        } else {
            self.cleared.remove(&location);
        }
    }
}

/// Whether `location` is cleared.
pub fn location_cleared(world: &World, location: u32) -> bool {
    world
        .try_resource::<LocationClearState>()
        .is_some_and(|state| state.is_cleared(location))
}

/// Record a death (M47.2): when `entity` is a `Boss` of some location, the
/// location clears once every one of its bosses is dead. Called from every
/// death producer; idempotent.
pub fn note_actor_death(world: &World, entity: EntityId) {
    let reference = crate::condition::entity_global_form_id(world, entity).or_else(|| {
        world
            .get::<crate::scene::SceneAliasCandidate>(entity)
            .map(|candidate| candidate.reference_form_id)
    });
    let Some(reference) = reference else {
        return;
    };
    let boss_of: Vec<(u32, Vec<u32>)> = {
        let Some(table) = world.try_resource::<LocationTable>() else {
            return;
        };
        table
            .boss_locations(reference)
            .iter()
            .map(|&location| (location, table.bosses(location).to_vec()))
            .collect()
    };
    if boss_of.is_empty() {
        return;
    }
    let Some(mut state) = world.try_resource_mut::<LocationClearState>() else {
        return;
    };
    state.dead_bosses.insert(reference);
    for (location, bosses) in boss_of {
        if !state.cleared.contains(&location)
            && bosses.iter().all(|boss| state.dead_bosses.contains(boss))
        {
            state.cleared.insert(location);
            log::info!("Location {location:08X} cleared: every boss is dead");
        }
    }
}

/// Install the load order's [`LocationTable`] (replacing the previous one)
/// and an empty [`PlayerLocation`] / [`LocationAliasBindings`] if none
/// exists yet. Returns the number of
/// locations.
pub fn install_location_table(world: &mut World, index: &EsmIndex) -> usize {
    let table = LocationTable::from_index(index);
    let count = table.locations.len();
    world.insert_resource(table);
    if world.try_resource::<PlayerLocation>().is_none() {
        world.insert_resource(PlayerLocation::default());
    }
    if world.try_resource::<LocationAliasBindings>().is_none() {
        world.insert_resource(LocationAliasBindings::default());
    }
    if world.try_resource::<LocationClearState>().is_none() {
        world.insert_resource(LocationClearState::default());
    }
    count
}

#[cfg(test)]
pub(crate) mod tests;
