use super::*;
use crate::keywords::{has_keyword, install_keyword_table};
use byroredux_plugin::esm::records::{KywdRecord, NpcRecord, RaceRecord};

const HUMAN: u32 = 0x0001_3746;
const TEMPLATE: u32 = 0x0016_6729;
const SETTLER: u32 = 0x0002_0001;
const CONFIDENT: u32 = 0x0002_2E49;
const SCARED: u32 = 0x0005_F763;
const FEMALE: u32 = 0x0008_4486;
const HANDS_UP: u32 = 0x000B_1CF5;

fn entry(keys: &[u32], graph: &str) -> SubgraphData {
    SubgraphData {
        actor_keywords: keys.to_vec(),
        behavior_graph: graph.to_owned(),
        animation_paths: Vec::new(),
        target_keywords: Vec::new(),
        role: SubgraphRole::Mt,
        first_person: false,
    }
}

fn keyword(index: &mut EsmIndex, form_id: u32, editor_id: &str) {
    index.keywords.insert(form_id, KywdRecord { form_id, editor_id: editor_id.to_owned(), attraction_rule: 0 });
}

/// A settler who authors `AnimArchetypeConfident`, on a race that takes its
/// subgraph data from a template race.
fn world_with_settler(acbs_flags: u32) -> (World, EntityId) {
    let mut world = World::new();
    crate::register(&mut world);
    let mut index = EsmIndex::default();
    keyword(&mut index, CONFIDENT, "AnimArchetypeConfident");
    keyword(&mut index, SCARED, "AnimArchetypeScared");
    keyword(&mut index, FEMALE, "Female");
    keyword(&mut index, HANDS_UP, "AnimFlavorHandsUp");
    index.races.insert(
        TEMPLATE,
        RaceRecord {
            form_id: TEMPLATE,
            subgraph_data: vec![
                entry(&[], "neutral"),
                entry(&[CONFIDENT], "confident"),
                entry(&[SCARED], "scared"),
                entry(&[SCARED, FEMALE], "scared-female"),
                entry(&[HANDS_UP], "hands-up"),
                SubgraphData { first_person: true, ..entry(&[CONFIDENT, HANDS_UP], "first-person") },
                SubgraphData { role: SubgraphRole::Weapon, ..entry(&[CONFIDENT], "weapon") },
            ],
            ..Default::default()
        },
    );
    index.races.insert(
        HUMAN,
        RaceRecord { form_id: HUMAN, subgraph_template: Some(TEMPLATE), ..Default::default() },
    );
    index.npcs.insert(
        SETTLER,
        NpcRecord {
            form_id: SETTLER,
            race_form_id: HUMAN,
            keywords: vec![CONFIDENT],
            acbs_flags,
            ..Default::default()
        },
    );
    install_keyword_table(&mut world, &index);
    install_subgraph_table(&mut world, &index);
    world.insert_resource(crate::AnimArchetypeOverrides::default());
    let entity = world.spawn();
    world.insert(entity, crate::scene::SceneAliasCandidate { base_form_id: SETTLER, ..Default::default() });
    (world, entity)
}

fn pick(world: &World, entity: EntityId) -> Option<String> {
    select_subgraph(world, entity, SubgraphRole::Mt, &[]).map(|e| e.behavior_graph)
}

/// The authored archetype picks the most specific entry of the role, third
/// person; the template race's entries apply to the race that names it.
#[test]
fn the_authored_archetype_selects_its_entry() {
    let (world, settler) = world_with_settler(0);
    assert_eq!(pick(&world, settler).as_deref(), Some("confident"));
    // Role and perspective are filters: the weapon and first-person entries
    // never win a third-person MT pick.
    let weapon = select_subgraph(&world, settler, SubgraphRole::Weapon, &[]).unwrap();
    assert_eq!(weapon.behavior_graph, "weapon");
}

/// Give the actor the FormID the override is keyed by; returns that key.
fn give_form_id(world: &mut World, entity: EntityId) -> u32 {
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    world.insert_resource(FormIdPool::new());
    let id = world.resource_mut::<FormIdPool>().intern(FormIdPair {
        plugin: PluginId::from_filename("Fallout4.esm"),
        local: LocalFormId(0x0002_0777),
    });
    world.insert(entity, byroredux_core::ecs::components::FormIdComponent(id));
    crate::condition::entity_global_form_id(world, entity).unwrap()
}

/// `ChangeAnimArchetype(Scared)` replaces the authored archetype: HasKeyword
/// sees only the new one and the selected entry follows; with none the actor is
/// Neutral (no archetype keyword, the keyword-less entry).
#[test]
fn change_anim_archetype_replaces_the_keyword_and_the_selection() {
    let (mut world, settler) = world_with_settler(0);
    let key = give_form_id(&mut world, settler);
    assert!(has_keyword(&world, settler, CONFIDENT), "authored");

    world.resource_mut::<crate::AnimArchetypeOverrides>().set(key, SCARED);
    assert!(has_keyword(&world, settler, SCARED));
    assert!(!has_keyword(&world, settler, CONFIDENT), "the authored one is shadowed");
    assert_eq!(pick(&world, settler).as_deref(), Some("scared"));

    world.resource_mut::<crate::AnimArchetypeOverrides>().set(key, 0);
    assert!(!has_keyword(&world, settler, SCARED));
    assert!(!has_keyword(&world, settler, CONFIDENT));
    assert_eq!(pick(&world, settler).as_deref(), Some("neutral"));
}

/// The special `Female` keyword is the gender flag XOR "Swap Gender Anims"
/// (ACBS bits 0 and 19), and the entry matching the most keywords wins.
#[test]
fn the_female_keyword_comes_from_the_gender_flags() {
    let scared_pick = |flags: u32| {
        let (mut world, settler) = world_with_settler(flags);
        let key = give_form_id(&mut world, settler);
        world.resource_mut::<crate::AnimArchetypeOverrides>().set(key, SCARED);
        pick(&world, settler)
    };
    assert_eq!(scared_pick(0).as_deref(), Some("scared"), "male");
    assert_eq!(scared_pick(1).as_deref(), Some("scared-female"), "female: the entry needing both keywords");
    assert_eq!(scared_pick(1 | (1 << 19)).as_deref(), Some("scared"), "Swap Gender Anims flips it back");
}

/// A flavor keyword is not an archetype: it is neither shadowed nor swapped.
#[test]
fn an_override_leaves_other_keywords_alone() {
    let (mut world, settler) = world_with_settler(0);
    let key = give_form_id(&mut world, settler);
    world.resource_mut::<crate::AnimArchetypeOverrides>().set(key, SCARED);
    assert_eq!(archetype_override_has(&world, key, HANDS_UP), None);
    assert_eq!(archetype_override_has(&world, key ^ 1, SCARED), None, "other actors are untouched");
}

#[test]
fn a_template_cycle_cannot_hang_the_table() {
    let mut index = EsmIndex::default();
    index.races.insert(1, RaceRecord { form_id: 1, subgraph_template: Some(2), ..Default::default() });
    index.races.insert(2, RaceRecord { form_id: 2, subgraph_template: Some(1), subgraph_data: vec![entry(&[], "x")], ..Default::default() });
    assert_eq!(effective_entries(&index, 1).len(), 1);
}

/// Vanilla Fallout4.esm: every NPC_ that authors an archetype keyword selects
/// a third-person MT entry whose actor keywords it all has, and — where its
/// race has an entry for that archetype — the entry names it.
#[test]
#[ignore = "needs Fallout4.esm on disk (~1 GB resident)"]
fn vanilla_fo4_archetype_npcs_select_their_entries() {
    let path = byroredux_plugin::esm::test_paths::fo4_esm();
    if !path.is_file() {
        if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
            panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but Fallout4.esm was not found at {path:?}");
        }
        return;
    }
    let index = byroredux_plugin::esm::parse_esm(&std::fs::read(&path).unwrap()).unwrap();
    let mut world = World::new();
    crate::register(&mut world);
    install_keyword_table(&mut world, &index);
    assert_eq!(install_subgraph_table(&mut world, &index), 10, "the ten AnimArchetype keywords");
    world.insert_resource(crate::AnimArchetypeOverrides::default());
    let archetypes: Vec<u32> = world.resource::<SubgraphTable>().archetypes.iter().copied().collect();

    let (mut checked, mut named) = (0, 0);
    for npc in index.npcs.values() {
        let Some(&own) = npc.keywords.iter().find(|k| archetypes.contains(k)) else { continue };
        let entity = world.spawn();
        world.insert(entity, crate::scene::SceneAliasCandidate { base_form_id: npc.form_id, ..Default::default() });
        let Some(entry) = select_subgraph(&world, entity, SubgraphRole::Mt, &[]) else { continue };
        checked += 1;
        assert!(!entry.first_person);
        assert!(entry.actor_keywords.iter().all(|&k| k == world.resource::<SubgraphTable>().female_keyword.unwrap_or(0)
            || has_keyword(&world, entity, k)));
        named += usize::from(entry.actor_keywords.contains(&own));
    }
    println!("{checked} archetype NPCs selected an MT entry; {named} name their own archetype");
    assert!(checked > 100, "the corpus has 726 archetype NPCs");
    assert!(named > checked / 2, "most selections are the archetype's own entry");
}
