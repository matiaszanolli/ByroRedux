//! State the ObScript commands of M47.3 keep that nothing else in the engine owned: the Pip-Boy note
//! list, who killed whom and how many of each base have died, the forced weather, the "weapon out" and
//! "ignore crime" flags of an actor.
//!
//! All of it is gameplay state a save must carry, so each type here is saved ([`register_saved`]).
//!
//! * **Notes** — GECK *AddNote*: "adds a note to the player's note inventory"; *RemoveNote*;
//!   *GetHasNote*: "0 if the note is not there and 1 if it is" (and *GetItemCount* "will always
//!   return 0 when using it to check for a note": a note is not an inventory item).
//! * **Deaths** — GECK *GetDeadCount*: "the total number of this creature/NPC type that have been
//!   killed ... Resurrecting a killed actor will not lower the death count"; *IsKiller*: "1 if the
//!   calling Actor was killed by the specified Actor".
//! * **Weather** — GECK *ForceWeather* / *SetWeather*: switch to a weather at once; with the override
//!   flag the weather "will not change again until ReleaseWeatherOverride is called".
//! * **Weapon out** — GECK *SetAlert*: "whether the actor is in an 'alert' state (weapon drawn)";
//!   *IsWeaponOut*: "1 if the calling actor currently has a weapon out".
//! * **Ignore crime** — GECK *IgnoreCrime*: "prevents an NPC from attacking or reporting a player for
//!   crime"; *GetIgnoreCrime* reads it back.

use std::collections::BTreeMap;

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::{Resource, World};

/// The notes the player holds, in the order they were added.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct PipBoyNotes {
    notes: Vec<u32>,
}

impl Resource for PipBoyNotes {}

impl PipBoyNotes {
    /// Add `note`; `false` when the player already had it.
    pub fn add(&mut self, note: u32) -> bool {
        if self.notes.contains(&note) {
            return false;
        }
        self.notes.push(note);
        true
    }

    /// Remove `note`; `false` when the player did not have it.
    pub fn remove(&mut self, note: u32) -> bool {
        let before = self.notes.len();
        self.notes.retain(|&held| held != note);
        self.notes.len() != before
    }

    pub fn has(&self, note: u32) -> bool {
        self.notes.contains(&note)
    }

    pub fn notes(&self) -> &[u32] {
        &self.notes
    }
}

/// Who died and who killed them. `dead_by_base` counts every actor of a base form that has died (a
/// resurrection does not lower it); `killers` maps a dead reference to the reference that killed it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct KillRecords {
    dead_by_base: BTreeMap<u32, u32>,
    killers: BTreeMap<u32, u32>,
}

impl Resource for KillRecords {}

impl KillRecords {
    /// A death: `victim` (reference) of `base`, killed by `killer` (a reference) if anyone.
    pub fn record(&mut self, victim: u32, base: u32, killer: Option<u32>) {
        if base != 0 {
            *self.dead_by_base.entry(base).or_default() += 1;
        }
        match killer {
            Some(killer) if killer != 0 => {
                self.killers.insert(victim, killer);
            }
            // A death with no killer (drowning) clears an earlier killer of the same reference.
            _ => {
                self.killers.remove(&victim);
            }
        }
    }

    pub fn dead_count(&self, base: u32) -> u32 {
        self.dead_by_base.get(&base).copied().unwrap_or(0)
    }

    pub fn killer_of(&self, victim: u32) -> Option<u32> {
        self.killers.get(&victim).copied()
    }
}

/// The weather a script forced, and whether it holds ("override flag"). `None` is the climate's own.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ForcedWeather {
    pub weather: Option<u32>,
    pub locked: bool,
}

impl Resource for ForcedWeather {}

/// An actor with its weapon drawn (`SetAlert 1`). Presence is the flag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct WeaponDrawn;

impl Component for WeaponDrawn {
    type Storage = SparseSetStorage<Self>;
}

/// An actor that neither attacks the player for crime nor reports it (`IgnoreCrime 1`). Presence is
/// the flag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct IgnoreCrime;

impl Component for IgnoreCrime {
    type Storage = SparseSetStorage<Self>;
}

/// When the player last entered each cell, in game hours, for `OnReset`: CS *OnReset*: "run once
/// whenever the scripted object's cell is reset due to being expired (3 days after the player last
/// entered it). Generally used for traps and animating objects in order to reset their state." The
/// expiry is the `iHoursToRespawnCell` setting (72 where the load order authors none: the CS's "3 days").
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct CellVisits {
    entered: BTreeMap<u32, f64>,
}

impl Resource for CellVisits {}

/// The cell-respawn interval when the load order authors none, in game hours.
pub const DEFAULT_HOURS_TO_RESPAWN_CELL: f64 = 72.0;

impl CellVisits {
    /// `ResetInterior`: mark `cell` expired, so the player's next entry finds it reset.
    pub fn expire(&mut self, cell: u32) {
        self.entered.insert(cell, f64::NEG_INFINITY);
    }

    /// The player enters `cell` at game hour `now`. `true` when they had been there before and it has
    /// been `respawn_hours` or more since — the cell has reset.
    pub fn enter(&mut self, cell: u32, now: f64, respawn_hours: f64) -> bool {
        let previous = self.entered.insert(cell, now);
        previous.is_some_and(|before| now - before >= respawn_hours)
    }
}

/// Cells whose scripted references are owed an `OnReset` (the cell reset as the player entered it):
/// consumed by [`mark_cell_reset`]. `NOT_SAVED_BY_DESIGN`: raised and drained within a frame or two.
#[derive(Debug, Default)]
pub struct PendingCellResets(pub Vec<u32>);

impl Resource for PendingCellResets {}

/// An object script's `OnReset` is due. `NOT_SAVED_BY_DESIGN`: consumed the frame it is raised.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OnResetEvent;

impl Component for OnResetEvent {
    type Storage = SparseSetStorage<Self>;
}

/// The player entered `cell` (its authored FormID) at game hour `now`: record the visit and, if the cell
/// had expired, queue its reset.
pub fn note_cell_entered(world: &World, cell: u32, now: f64, respawn_hours: Option<f64>) -> bool {
    let Some(mut visits) = world.try_resource_mut::<CellVisits>() else { return false };
    let expired = visits.enter(cell, now, respawn_hours.unwrap_or(DEFAULT_HOURS_TO_RESPAWN_CELL));
    drop(visits);
    if expired {
        if let Some(mut pending) = world.try_resource_mut::<PendingCellResets>() {
            pending.0.push(cell);
        }
    }
    expired
}

/// Give every scripted reference in a cell that reset its [`OnResetEvent`]; run each frame, before the
/// object scripts. A cell that has not finished spawning keeps its entry until it has references.
pub fn mark_cell_reset(world: &World, _dt: f32) {
    use byroredux_core::ecs::components::{CellFormId, CellRoot};
    let cells = match world.try_resource_mut::<PendingCellResets>() {
        Some(mut pending) if !pending.0.is_empty() => std::mem::take(&mut pending.0),
        _ => return,
    };
    let scripted: Vec<EntityId> = world
        .query::<crate::obscript_objects::ObScriptInstance>()
        .map(|instances| instances.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    let mut unresolved = Vec::new();
    for cell in cells {
        let mut found = false;
        for &entity in &scripted {
            let root = world.get::<CellRoot>(entity).map_or(entity, |root| root.0);
            if world.get::<CellFormId>(root).is_some_and(|id| id.0 == cell) {
                found = true;
                if let Some(mut events) = world.query_mut::<OnResetEvent>() {
                    events.insert(entity, OnResetEvent);
                }
            }
        }
        // The cell is not loaded yet (its references have not been attached): try again next frame.
        if !found {
            unresolved.push(cell);
        }
    }
    if !unresolved.is_empty() {
        if let Some(mut pending) = world.try_resource_mut::<PendingCellResets>() {
            pending.0.extend(unresolved);
        }
    }
}

/// `Resurrect`: the actor is to be brought back this frame. `NOT_SAVED_BY_DESIGN`: consumed the frame
/// it is raised.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReviveRequest;

impl Component for ReviveRequest {
    type Storage = SparseSetStorage<Self>;
}

/// This subsystem's resources and components.
pub fn install(world: &mut World) {
    world.register::<ReviveRequest>();
    world.register::<PlayingScriptedIdle>();
    world.register::<OnResetEvent>();
    world.init_resource::<CellVisits>();
    world.init_resource::<PendingCellResets>();
    world.init_resource::<PipBoyNotes>();
    world.init_resource::<KillRecords>();
    world.init_resource::<ForcedWeather>();
    world.register::<WeaponDrawn>();
    world.register::<IgnoreCrime>();
}

/// This subsystem's saved columns. Called by the binary's `build_save_registry` in its row order
/// (the schema fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<PipBoyNotes>("PipBoyNotes");
    registry.register_resource::<KillRecords>("KillRecords");
    registry.register_resource::<ForcedWeather>("ForcedWeather");
    registry.register_resource::<CellVisits>("CellVisits");
    registry.register_replacing_component::<WeaponDrawn>("WeaponDrawn");
    registry.register_replacing_component::<IgnoreCrime>("IgnoreCrime");
}

/// Whether one actor currently detects another by sight. The engine installs the real test (its
/// detection range and line of sight need the physics world, which this crate does not link) as a
/// plain function; without one `GetDetected` is declined. `NOT_SAVED_BY_DESIGN`.
#[derive(Clone, Copy)]
pub struct SightQuery(pub fn(&World, EntityId, EntityId) -> bool);

impl Resource for SightQuery {}

/// FOSE / GECK *type codes* of the load order's items, by base FormID (`RemoveAllTypedItems`): 24 Armor,
/// 25 Book, 29 Ingredient, 31 Misc, 40 Weapon, 41 Ammo, 46 Key, 47 AlchemyItem (every ingestible),
/// 49 Note, 103 IMOD. (Clothing, 26, is a record type of its own in Oblivion only: in Fallout 3 and New
/// Vegas clothes are `ARMO`, so no item answers 26 there.) `NOT_SAVED_BY_DESIGN`: rebuilt from the records.
#[derive(Debug, Default, Clone)]
pub struct ItemTypeCodes(std::collections::HashMap<u32, u8>);

impl Resource for ItemTypeCodes {}

impl ItemTypeCodes {
    pub fn from_index(index: &byroredux_plugin::esm::records::EsmIndex) -> Self {
        use byroredux_plugin::esm::records::ItemKind;
        Self(
            index
                .items
                .iter()
                .filter_map(|(&form_id, item)| {
                    let code = match &item.kind {
                        ItemKind::Armor { .. } => 24,
                        ItemKind::Book { .. } => 25,
                        ItemKind::Ingredient { .. } => 29,
                        ItemKind::Misc | ItemKind::Junk => 31,
                        ItemKind::Weapon { .. } => 40,
                        ItemKind::Ammo { .. } => 41,
                        ItemKind::Key => 46,
                        ItemKind::Aid { .. } => 47,
                        ItemKind::Note { .. } => 49,
                        ItemKind::Mod { .. } => 103,
                        _ => return None,
                    };
                    Some((form_id, code))
                })
                .collect(),
        )
    }

    pub fn code_of(&self, base: u32) -> Option<u8> {
        self.0.get(&base).copied()
    }
}

/// The load order's numeric game settings by editor ID (lowercased): `GetGS <setting>` — "returns the
/// value of a game setting". `NOT_SAVED_BY_DESIGN`: rebuilt from the records (a `SetGS` is not modelled).
#[derive(Debug, Default, Clone)]
pub struct GameSettings(std::collections::HashMap<String, f32>);

impl Resource for GameSettings {}

impl GameSettings {
    pub fn from_index(index: &byroredux_plugin::esm::records::EsmIndex) -> Self {
        Self(
            index
                .game_settings
                .values()
                .filter_map(|setting| index.game_setting_float(&setting.editor_id).map(|value| (setting.editor_id.to_ascii_lowercase(), value)))
                .collect(),
        )
    }
}

/// A game setting's value by editor ID (case-insensitive); a string setting has none.
pub fn game_setting(world: &World, name: &str) -> Option<f32> {
    world.try_resource::<GameSettings>()?.0.get(&name.to_ascii_lowercase()).copied()
}

/// `ShowWarning "text"`: "display/write a warning ... a message box on the screen" (GECK): shown as a
/// message, written to the log by the caller.
pub fn show_warning(world: &World, text: &str) {
    crate::messages::show_obscript_message(
        world,
        crate::quest_stages::QuestFormId(0),
        crate::obscript_vm::ObScriptMessage { text: text.to_owned(), values: Vec::new(), buttons: Vec::new(), message_box: false },
    );
}

/// Whether an actor is swimming: the walk/swim verdict the character system decides on needs the
/// physics controller and the water contact, so the engine installs it as a plain function. Without
/// one `IsSwimming` is declined. `NOT_SAVED_BY_DESIGN`.
#[derive(Clone, Copy)]
pub struct SwimQuery(pub fn(&World, EntityId) -> bool);

impl Resource for SwimQuery {}

/// The `DNAM` animation type of every weapon, by base FormID (`GetWeaponAnimType`). Fallout 3 / New Vegas
/// only (Oblivion's weapons carry no such byte). `NOT_SAVED_BY_DESIGN`: rebuilt from the records.
#[derive(Debug, Default, Clone)]
pub struct WeaponAnimTypes(std::collections::HashMap<u32, u8>);

impl Resource for WeaponAnimTypes {}

impl WeaponAnimTypes {
    pub fn from_index(index: &byroredux_plugin::esm::records::EsmIndex) -> Self {
        use byroredux_plugin::esm::records::ItemKind;
        if index.game != byroredux_plugin::esm::reader::GameKind::Fallout3NV {
            return Self::default();
        }
        Self(
            index
                .items
                .iter()
                .filter_map(|(&form, item)| match &item.kind {
                    ItemKind::Weapon { anim_type, .. } => Some((form, *anim_type)),
                    _ => None,
                })
                .collect(),
        )
    }

    pub fn get(&self, weapon: u32) -> Option<u8> {
        self.0.get(&weapon).copied()
    }
}

/// An actor playing a scripted one-shot idle (`PlayIdle`); the app's idle runtime adds it when the idle
/// starts and removes it when the actor goes back to what it was doing. `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayingScriptedIdle;

impl Component for PlayingScriptedIdle {
    type Storage = SparseSetStorage<Self>;
}

/// The polygons of every `REGN`, for `IsPlayerInRegion`. `NOT_SAVED_BY_DESIGN`: rebuilt from the records.
#[derive(Debug, Default, Clone)]
pub struct RegionAreas(std::collections::HashMap<u32, (Option<u32>, Vec<Vec<(f32, f32)>>)>);

impl Resource for RegionAreas {}

impl RegionAreas {
    pub fn from_index(index: &byroredux_plugin::esm::records::EsmIndex) -> Self {
        Self(
            index
                .regions
                .values()
                .map(|region| (region.form_id, (region.worldspace_form, region.areas.iter().map(|area| area.points.clone()).collect())))
                .collect(),
        )
    }

    /// Whether the world point `(x, y)` (Z-up) is inside one of `region`'s polygons, in `worldspace` when
    /// the region names one.
    pub fn contains(&self, region: u32, worldspace: u32, x: f32, y: f32) -> bool {
        let Some((owner, polygons)) = self.0.get(&region) else { return false };
        if owner.is_some_and(|owner| owner != worldspace) {
            return false;
        }
        polygons.iter().any(|polygon| point_in_polygon(polygon, x, y))
    }
}

/// Even-odd ray casting.
fn point_in_polygon(polygon: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = polygon.len().wrapping_sub(1);
    for i in 0..polygon.len() {
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The reference FormID scripts name `entity` by: `0x14` for the player (`PlayerRef`), else its
/// placed reference's FormID.
pub fn reference_form(world: &World, entity: EntityId) -> Option<u32> {
    const PLAYER_REF: u32 = 0x14;
    if crate::papyrus_demo::PapyrusPlayerEntity::get(world) == Some(entity) {
        return Some(PLAYER_REF);
    }
    crate::condition::entity_global_form_id(world, entity)
}

/// Record that `victim` died, killed by `killer` when someone did. The engine calls this from every
/// live death producer (the one funnel `raise_death_event` runs through), never from the corpse
/// restore that re-asserts a death on load, so a save/load does not count a death twice.
pub fn note_death(world: &World, victim: EntityId, killer: Option<EntityId>) {
    let Some(reference) = reference_form(world, victim) else { return };
    let base = world.get::<crate::scene::SceneAliasCandidate>(victim).map_or(0, |candidate| candidate.base_form_id);
    let killer = killer.and_then(|killer| reference_form(world, killer));
    if let Some(mut records) = world.try_resource_mut::<KillRecords>() {
        records.record(reference, base, killer);
    }
    // What the player kills is counted: the GECK misc stats "People Killed", "Creatures Killed" and "Total
    // Things Killed", and the challenges that name a kill.
    if killer == Some(crate::PLAYER_REFERENCE) {
        let creature = world.has::<byroredux_core::ecs::components::CreatureActor>(victim);
        crate::obscript_more::mod_misc_stat(world, if creature { "Creatures Killed" } else { "People Killed" }, 1);
        crate::obscript_more::mod_misc_stat(world, "Total Things Killed", 1);
        crate::challenges::note(world, crate::challenges::ChallengeEvent::Kill { base });
    }
}

/// Whether `actor` has its weapon out: flagged by `SetAlert`, or fighting (an actor in combat has
/// drawn).
pub fn is_weapon_out(world: &World, actor: EntityId) -> bool {
    world.has::<WeaponDrawn>(actor) || world.has::<crate::AiCombatState>(actor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_is_in_a_region_when_a_polygon_holds_it() {
        let square = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(point_in_polygon(&square, 5.0, 5.0));
        assert!(!point_in_polygon(&square, 11.0, 5.0));
        assert!(!point_in_polygon(&[], 0.0, 0.0));
        let mut areas = RegionAreas::default();
        areas.0.insert(7, (Some(0xA), vec![square]));
        assert!(areas.contains(7, 0xA, 1.0, 1.0));
        assert!(!areas.contains(7, 0xB, 1.0, 1.0), "another worldspace");
        assert!(!areas.contains(8, 0xA, 1.0, 1.0), "an unknown region");
    }

    #[test]
    fn notes_are_added_once_and_removed() {
        let mut notes = PipBoyNotes::default();
        assert!(notes.add(0x10));
        assert!(!notes.add(0x10), "a second copy is not a second note");
        assert!(notes.has(0x10) && !notes.has(0x11));
        assert!(notes.remove(0x10) && !notes.remove(0x10));
        assert!(notes.notes().is_empty());
    }

    #[test]
    fn deaths_count_per_base_and_remember_the_killer() {
        let mut kills = KillRecords::default();
        kills.record(0x100, 0x7, Some(0x14));
        kills.record(0x101, 0x7, None);
        kills.record(0x102, 0x8, Some(0x14));
        assert_eq!((kills.dead_count(0x7), kills.dead_count(0x8), kills.dead_count(0x9)), (2, 1, 0));
        assert_eq!((kills.killer_of(0x100), kills.killer_of(0x101)), (Some(0x14), None));
    }

    /// What the player kills is counted: People / Creatures / Total Things Killed and the challenges that
    /// name a kill; what anyone else kills is not.
    #[test]
    fn the_players_kills_are_counted_as_misc_stats_and_for_kill_challenges() {
        use byroredux_core::ecs::components::{CreatureActor, FormIdComponent};
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        use byroredux_plugin::esm::records::{challenge_kind, ChalRecord, EsmIndex};
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        let mut pool = FormIdPool::new();
        let mut spawn = |world: &mut World, local: u32| {
            let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("FalloutNV.esm"), local: LocalFormId(local) });
            let entity = world.spawn();
            world.insert(entity, FormIdComponent(id));
            entity
        };
        let person = spawn(&mut world, 0x100);
        let beast = spawn(&mut world, 0x101);
        world.insert(beast, CreatureActor);
        world.insert_resource(pool);
        let mut index = EsmIndex::default();
        index.challenges.insert(0x77, ChalRecord { form_id: 0x77, editor_id: "KillBeast".into(), challenge_type: challenge_kind::KILL_SPECIFIC, threshold: 1, form1: 0x555, ..Default::default() });
        world.insert_resource(crate::challenges::ChallengeDefs::from_index(&index));
        world.insert(beast, crate::scene::SceneAliasCandidate { reference_form_id: 0x101, base_form_id: 0x555, linked_refs: Vec::new(), location_ref_types: Vec::new() });
        let stat = |world: &World, name: &str| crate::obscript_more::misc_stat(world, name);
        note_death(&world, person, Some(player));
        assert_eq!((stat(&world, "People Killed"), stat(&world, "Creatures Killed"), stat(&world, "Total Things Killed")), (1, 0, 1));
        note_death(&world, beast, Some(person));
        assert_eq!(stat(&world, "Total Things Killed"), 1, "an NPC's kill is not the player's");
        assert_eq!(crate::challenges::completed(&world, 0x77), Some(false));
        note_death(&world, beast, Some(player));
        assert_eq!((stat(&world, "Creatures Killed"), stat(&world, "Total Things Killed")), (1, 2));
        assert_eq!(crate::challenges::completed(&world, 0x77), Some(true), "the kill of the named form completed it");
    }
}
