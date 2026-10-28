use super::*;
use byroredux_plugin::esm::records::{NpcRecord, RaceRecord};

const DREMORA_RACE: u32 = 0x0001_3740;
const DREMORA: u32 = 0x0001_6F7D;
const ACTOR_TYPE_DAEDRA: u32 = 0x0001_3797;
const ACTOR_TYPE_NPC: u32 = 0x0001_3794;
const INJURED: u32 = 0x0009_1234;

fn world_with_dremora() -> (World, EntityId) {
    let mut world = World::new();
    crate::register(&mut world);
    let mut index = EsmIndex::default();
    index.races.insert(
        DREMORA_RACE,
        RaceRecord {
            form_id: DREMORA_RACE,
            keywords: vec![ACTOR_TYPE_DAEDRA],
            ..Default::default()
        },
    );
    index.npcs.insert(
        DREMORA,
        NpcRecord {
            form_id: DREMORA,
            race_form_id: DREMORA_RACE,
            keywords: vec![ACTOR_TYPE_NPC],
            ..Default::default()
        },
    );
    install_keyword_table(&mut world, &index);
    let entity = world.spawn();
    world.insert(
        entity,
        crate::scene::SceneAliasCandidate {
            base_form_id: DREMORA,
            ..Default::default()
        },
    );
    (world, entity)
}

/// An actor has its own `KWDA`, its race's, and what scripts add; a script's
/// `RemoveKeyword` overrides the base, and `ResetKeyword` undoes both.
#[test]
fn actors_have_their_race_keywords_and_scripts_override() {
    let (world, dremora) = world_with_dremora();
    assert!(has_keyword(&world, dremora, ACTOR_TYPE_NPC));
    assert!(has_keyword(&world, dremora, ACTOR_TYPE_DAEDRA), "the race's");
    assert!(!has_keyword(&world, dremora, INJURED));
    // No FormID to key a change by: refused rather than lost silently.
    assert!(!change_keyword(&world, dremora, INJURED, KeywordChange::Add));

    let mut world = world;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    world.insert_resource(FormIdPool::new());
    let id = world.resource_mut::<FormIdPool>().intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x0001_6F7E),
    });
    world.insert(dremora, byroredux_core::ecs::components::FormIdComponent(id));
    assert!(change_keyword(&world, dremora, INJURED, KeywordChange::Add));
    assert!(has_keyword(&world, dremora, INJURED));
    assert!(change_keyword(&world, dremora, ACTOR_TYPE_DAEDRA, KeywordChange::Remove));
    assert!(!has_keyword(&world, dremora, ACTOR_TYPE_DAEDRA));
    assert!(change_keyword(&world, dremora, ACTOR_TYPE_DAEDRA, KeywordChange::Reset));
    assert!(has_keyword(&world, dremora, ACTOR_TYPE_DAEDRA), "back to the race's");
}

/// #6515 / #6434 — `KWDA` is a Use-Keywords field and race a Use-Traits field,
/// so a templated shell has its terminals', not its own. `LvlHorse` shape: no
/// `KWDA` of its own, an LVLN template whose member authors it.
#[test]
fn templated_shells_have_their_templates_keywords_and_race() {
    use byroredux_plugin::equip::{TEMPLATE_FLAG_USE_KEYWORDS, TEMPLATE_FLAG_USE_TRAITS};
    use byroredux_plugin::esm::records::container::{LeveledEntry, LeveledList};
    const SHELL: u32 = 0x10;
    const HORSE: u32 = 0x11;
    const LVLN: u32 = 0x12;
    const SHELL_RACE: u32 = 0x20;
    const HORSE_RACE: u32 = 0x21;
    const ANIMAL: u32 = 0x30;
    const HORSE_KW: u32 = 0x31;
    const SHELL_RACE_KW: u32 = 0x32;
    let mut index = EsmIndex::default();
    for (race, keyword) in [(SHELL_RACE, SHELL_RACE_KW), (HORSE_RACE, ANIMAL)] {
        index.races.insert(
            race,
            RaceRecord { form_id: race, keywords: vec![keyword], ..Default::default() },
        );
    }
    index.npcs.insert(
        HORSE,
        NpcRecord {
            form_id: HORSE,
            race_form_id: HORSE_RACE,
            keywords: vec![HORSE_KW],
            ..Default::default()
        },
    );
    index.leveled_npcs.insert(
        LVLN,
        LeveledList {
            form_id: LVLN,
            editor_id: String::new(),
            chance_none: 0,
            flags: 0,
            entries: vec![LeveledEntry { level: 1, form_id: HORSE, count: 1 }],
        },
    );
    let shell = |flags| NpcRecord {
        form_id: SHELL,
        race_form_id: SHELL_RACE,
        template_form_id: LVLN,
        template_flags: flags,
        level: 1,
        ..Default::default()
    };
    let mut index = index;
    let mut table = |flags| {
        index.npcs.insert(SHELL, shell(flags));
        KeywordTable::from_index(&index)
    };

    let both = table(TEMPLATE_FLAG_USE_KEYWORDS | TEMPLATE_FLAG_USE_TRAITS);
    assert!(both.base_has(SHELL, HORSE_KW), "Use Keywords: the template's KWDA");
    assert!(both.base_has(SHELL, ANIMAL), "Use Traits: the template's race keywords");
    assert!(!both.base_has(SHELL, SHELL_RACE_KW), "not the shell's own race");

    let keywords_only = table(TEMPLATE_FLAG_USE_KEYWORDS);
    assert!(keywords_only.base_has(SHELL, HORSE_KW));
    assert!(keywords_only.base_has(SHELL, SHELL_RACE_KW), "race stays the shell's without Use Traits");
    assert!(!keywords_only.base_has(SHELL, ANIMAL));

    let neither = table(0);
    assert!(!neither.base_has(SHELL, HORSE_KW), "no flag, no inheritance");
    assert!(neither.base_has(SHELL, SHELL_RACE_KW));
}
