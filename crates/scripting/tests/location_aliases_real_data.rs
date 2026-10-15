//! M47.2 location runtime against the shipped Skyrim.esm: every quest's
//! location aliases filled through the live alias refresh (`parse_esm` →
//! [`install_location_table`] + [`install_scene_quest_aliases`] →
//! [`refresh_scene_actor_bindings`]), with no references loaded.
//!
//! **Opt-in / `#[ignore]`d** — needs Skyrim.esm on disk:
//! ```bash
//! cargo test -p byroredux-scripting --test location_aliases_real_data -- --ignored
//! ```
//! Skips gracefully (passing) when the game data isn't present.

use std::collections::BTreeMap;
use std::path::PathBuf;

use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{AliasFillType, EsmIndex};
use byroredux_plugin::esm::test_paths;
use byroredux_scripting::{

    install_location_table, install_scene_quest_aliases, refresh_scene_actor_bindings,
    LocationAliasBindings, QuestFormId,
};

mod common;

fn skyrim_index() -> Option<EsmIndex> {
    let dir = std::env::var(test_paths::SKYRIM_SE_ENV)
        .unwrap_or_else(|_| test_paths::SKYRIM_SE_DEFAULT.into());
    let bytes = std::fs::read(PathBuf::from(dir).join("Skyrim.esm")).ok()?;
    byroredux_plugin::esm::parse_esm(&bytes).ok()
}

/// Every Specific Location alias binds; every External alias whose source
/// binds follows it; and the Find Matching fills that bind (those whose
/// conditions read no reference alias — none is bound in a reference-less
/// world) hold a location that passes their conditions.
#[test]
#[ignore = "needs Skyrim SE game data on disk"]

fn skyrim_location_aliases_fill_against_the_real_hierarchy() {
    use byroredux_scripting::condition::{evaluate, ConditionContext};
    let Some(index) = skyrim_index() else {
        common::require_or_skip("Skyrim's Data", &"skyrim_index()");
        eprintln!("[Skyrim location aliases] skipping: game data unavailable");
        return;
    };
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    install_location_table(&mut world, &index);
    install_scene_quest_aliases(&mut world, index.quests.values().cloned());
    refresh_scene_actor_bindings(&world);
    let bindings = world.resource::<LocationAliasBindings>().clone();
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for quest in index.quests.values() {
        for alias in quest.aliases.iter().filter(|alias| alias.is_location) {
            let kind = match &alias.fill_type {
                Some(AliasFillType::ForcedLocation(_)) => "forced",
                Some(AliasFillType::ExternalAlias { .. }) => "external",
                Some(AliasFillType::LocationAliasReference { .. }) => "reference alias",
                Some(AliasFillType::FromEvent { .. }) => "event",
                None if !alias.match_conditions.is_empty() => "find matching",
                _ => "script-filled",
            };
            let bound = bindings.resolve(QuestFormId(quest.form_id), alias.alias_id);
            let entry = counts.entry(kind).or_default();
            entry.0 += 1;
            entry.1 += usize::from(bound.is_some());
            if let (Some(location), Some(AliasFillType::ForcedLocation(forced))) =
                (bound, &alias.fill_type)
            {
                assert_eq!(location, *forced);
            }
            if let (Some(location), "find matching") = (bound, kind) {
                assert!(evaluate(
                    &alias.match_conditions,
                    &world,
                    &ConditionContext::for_subject(byroredux_core::ecs::storage::EntityId::MAX)
                        .with_quest(QuestFormId(quest.form_id))
                        .with_location_subject(location),
                ));
            }
        }
    }
    for (kind, (total, bound)) in &counts {
        eprintln!("{kind:16} {bound:4}/{total}");
    }
    assert_eq!(counts["forced"], (162, 162));
    // `CWCampaignObj.CampaignHold`'s source (CWCampaign alias 27) fills from a
    // script-raised story event (`SCPT`), which nothing raises here.
    assert_eq!(counts["external"], (4, 3));
    assert!(counts["find matching"].1 >= 28, "{:?}", counts["find matching"]);
    // Script-filled aliases have no fill mechanism of their own.
    assert_eq!(counts["script-filled"].1, 0);
}


/// M47.2 — CK *Cell Reset*: a location clears once its `Boss` references
/// are dead. Every Skyrim boss-bearing location clears when (and only when)
/// its last boss dies; a boss is resolved through the alias-candidate
/// reference FormID, as the live death path does.
#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn skyrim_locations_clear_when_their_bosses_die() {
    use byroredux_scripting::{location_cleared, note_actor_death, LocationTable};
    let Some(index) = skyrim_index() else {
        common::require_or_skip("Skyrim's Data", &"skyrim_index()");
        eprintln!("[Skyrim location clearing] skipping: game data unavailable");
        return;
    };
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    install_location_table(&mut world, &index);
    let boss_locations: Vec<(u32, Vec<u32>)> = {
        let table = world.resource::<LocationTable>();
        table
            .locations()
            .iter()
            .map(|&location| (location, table.bosses(location).to_vec()))
            .filter(|(_, bosses)| !bosses.is_empty())
            .collect()
    };
    assert_eq!(boss_locations.len(), 374);
    let spawn = |world: &mut World, reference: u32| {
        let entity = world.spawn();
        world.insert(
            entity,
            byroredux_scripting::SceneAliasCandidate {
                reference_form_id: reference,
                base_form_id: 0,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        entity
    };
    let mut multi_boss = 0;
    for (location, bosses) in &boss_locations {
        let (last, rest) = bosses.split_last().unwrap();
        for &boss in rest {
            let entity = spawn(&mut world, boss);
            note_actor_death(&world, entity);
        }
        if !rest.is_empty() && !rest.contains(last) {
            multi_boss += 1;
            assert!(!location_cleared(&world, *location), "{location:08X}: a boss lives");
        }
        let entity = spawn(&mut world, *last);
        note_actor_death(&world, entity);
        assert!(location_cleared(&world, *location), "{location:08X}");
    }
    eprintln!("{} boss locations, {multi_boss} with several bosses", boss_locations.len());
}
