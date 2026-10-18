//! Actor values by their Papyrus names (M47.2).
//!
//! Skyrim's Papyrus names an actor value by string (`SetAV("Variable06", 3)`)
//! where Fallout 4 passes the `ActorValue` form. Skyrim's `AVIF` editor IDs
//! are the Papyrus name with an `AV` prefix (`AVVariable06`, `AVHealth`), so
//! the table maps both spellings, case-insensitively, to the AVIF FormID. A
//! name whose AVIF uses a legacy editor ID (Illusion is `AVMysticism`) does
//! not resolve, and the effect skips. `NOT_SAVED_BY_DESIGN`: rebuilt from the
//! load order every boot.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

#[derive(Debug, Default, Clone)]
pub struct ActorValueNames(HashMap<String, u32>);

impl Resource for ActorValueNames {}

impl ActorValueNames {
    pub fn from_index(index: &EsmIndex) -> Self {
        let mut names = HashMap::new();
        for avif in index.actor_values.values() {
            let editor_id = avif.editor_id.to_ascii_lowercase();
            if let Some(name) = editor_id.strip_prefix("av") {
                names.entry(name.to_string()).or_insert(avif.form_id);
            }
            names.insert(editor_id, avif.form_id);
        }
        Self(names)
    }

    /// The AVIF FormID `name` spells, if any.
    pub fn resolve(&self, name: &str) -> Option<u32> {
        self.0.get(&name.to_ascii_lowercase()).copied()
    }
}

/// The game the load order belongs to, for the Papyrus constants whose numbers
/// differ per game (`SetMotionType`'s). `NOT_SAVED_BY_DESIGN`: set from the load
/// order at install.
#[derive(Debug, Clone, Copy)]
pub struct ScriptGame(pub byroredux_plugin::esm::reader::GameKind);

impl Resource for ScriptGame {}

impl ScriptGame {
    /// Whether the loaded game is Fallout 3 / New Vegas. False when no load
    /// order has been installed: an unknown game is never assumed to be one.
    pub fn is_fallout3_nv(world: &World) -> bool {
        world.try_resource::<ScriptGame>().is_some_and(|game| {
            matches!(game.0, byroredux_plugin::esm::reader::GameKind::Fallout3NV)
        })
    }
}

/// Install the load order's [`ActorValueNames`]. Returns how many names map.
pub fn install_actor_value_names(world: &mut World, index: &EsmIndex) -> usize {
    let names = ActorValueNames::from_index(index);
    let count = names.0.len();
    world.insert_resource(names);
    world.insert_resource(ScriptGame(index.game));
    world.init_resource::<crate::cell_kind::PlayerCellKind>();
    world.insert_resource(crate::idle_names::IdleNames::from_index(index));
    world.insert_resource(crate::obscript_state::ItemTypeCodes::from_index(index));
    world.insert_resource(crate::obscript_state::GameSettings::from_index(index));
    world.insert_resource(crate::obscript_state::WeaponAnimTypes::from_index(index));
    world.insert_resource(crate::challenges::ChallengeDefs::from_index(index));
    world.insert_resource(crate::obscript_more::MusicTracks::from_index(index));
    world.insert_resource(crate::obscript_more::AshPiles::from_index(index));
    world.insert_resource(crate::obscript_more::CellZones::from_index(index));
    world.insert_resource(crate::obscript_more::ChildRaces::from_index(index));
    world.insert_resource(crate::obscript_state::RegionAreas::from_index(index));
    world.insert_resource(crate::magic_shaders::ShaderCatalog::from_index(index));
    world.insert_resource(crate::skills::SkillScaling::from_index(index));
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::AvifRecord;

    #[test]
    fn papyrus_names_and_editor_ids_both_resolve() {
        let mut index = EsmIndex::default();
        for (form_id, editor_id) in [(0x5F0, "AVVariable06"), (0x3E8, "AVHealth")] {
            index.actor_values.insert(
                form_id,
                AvifRecord {
                    form_id,
                    editor_id: editor_id.into(),
                    ..Default::default()
                },
            );
        }
        let names = ActorValueNames::from_index(&index);
        assert_eq!(names.resolve("Variable06"), Some(0x5F0));
        assert_eq!(names.resolve("health"), Some(0x3E8));
        assert_eq!(names.resolve("AVHealth"), Some(0x3E8));
        assert_eq!(names.resolve("Illusion"), None);
    }
}
