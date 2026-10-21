use super::*;
use byroredux_plugin::esm::reader::GameKind;
use crate::papyrus_demo::PapyrusPlayerEntity;
use crate::quest_stages::{QuestFormId, QuestStageState};
use crate::scene::{install_scene_quest_aliases, SceneActorBindings};
use byroredux_plugin::esm::records::{
    AliasFlags, GameSetting, QuestAlias, QustRecord, SettingValue,
};

const HEALTH: u32 = 0x3E8;
const QUEST: u32 = 0x0004_CF45;

/// A world with the player, rules of 20 s / 0.3, and one actor at 100
/// health.
fn world() -> (World, EntityId, EntityId) {
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PapyrusPlayerEntity(player));
    world.insert_resource(BleedoutRules {
        death_time_secs: 20.0,
        health_regain: 0.3,
    });
    let actor = world.spawn();
    let mut values = ActorValues::new();
    values.set_base(HEALTH, 100.0);
    world.insert(actor, values);
    world.insert(actor, ActorVitals { health: HEALTH });
    (world, player, actor)
}

/// The component mirrors the record's parse-boundary decode (#5921):
/// either flag stamps it, neither leaves the actor without one.
#[test]
fn protection_follows_the_records_decoded_flags() {
    let npc = |essential, protected| NpcRecord {
        essential,
        protected,
        ..Default::default()
    };
    assert_eq!(
        ActorProtection::from_record(&npc(true, false)),
        Some(ActorProtection {
            essential: true,
            protected: false
        })
    );
    assert_eq!(
        ActorProtection::from_record(&npc(false, true)),
        Some(ActorProtection {
            essential: false,
            protected: true
        })
    );
    assert_eq!(ActorProtection::from_record(&npc(false, false)), None);
}

/// An essential actor bleeds out, whoever strikes, and stays down on further
/// blows; the player is never spared.
#[test]
fn an_essential_actor_bleeds_out_instead_of_dying() {
    let (mut world, player, actor) = world();
    assert!(!spare_from_death(&world, actor, Some(player)), "not essential");
    world.insert(
        actor,
        ActorProtection {
            essential: true,
            protected: false,
        },
    );
    assert!(spare_from_death(&world, actor, Some(player)));
    assert_eq!(world.get::<Bleedout>(actor).map(|state| state.remaining_secs), Some(Some(20.0)));
    assert!(crate::is_unconscious(&world, actor), "bleedout is unconscious");
    let mut unconscious = Vec::new();
    crate::collect_unconscious(&world, &mut unconscious);
    assert_eq!(unconscious, vec![actor]);
    world.get_mut::<Bleedout>(actor).unwrap().remaining_secs = Some(5.0);
    assert!(spare_from_death(&world, actor, None));
    assert_eq!(
        world.get::<Bleedout>(actor).map(|state| state.remaining_secs),
        Some(Some(5.0)),
        "a blow while down does not restart the bleedout"
    );

    world.insert(
        player,
        ActorProtection {
            essential: true,
            protected: false,
        },
    );
    assert!(!spare_from_death(&world, player, None), "the player always dies");
}

/// A protected actor is essential to everything but the player, who can
/// finish it even while it bleeds out.
#[test]
fn only_the_player_kills_a_protected_actor() {
    let (mut world, player, actor) = world();
    let other = world.spawn();
    world.insert(
        actor,
        ActorProtection {
            essential: false,
            protected: true,
        },
    );
    assert!(spare_from_death(&world, actor, Some(other)));
    assert!(spare_from_death(&world, actor, None), "drowning spares it too");
    assert!(!spare_from_death(&world, actor, Some(player)));
    assert!(!is_bleeding_out(&world, actor), "the killing blow ends the bleedout");
}

/// The CK *Quest Alias Tab*: an alias's Essential flag protects its actor
/// while the quest runs.
#[test]
fn an_essential_alias_protects_its_actor_while_the_quest_runs() {
    let (mut world, _player, actor) = world();
    install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: QUEST,
            aliases: vec![QuestAlias {
                alias_id: 2,
                flags: AliasFlags(ALIAS_FLAG_ESSENTIAL),
                ..Default::default()
            }],
            ..Default::default()
        }],
    );
    world.insert_resource(QuestStageState::default());
    world
        .resource_mut::<SceneActorBindings>()
        .bind(QuestFormId(QUEST), 2, actor);
    assert!(!is_essential(&world, actor), "the quest is not running");
    world
        .resource_mut::<QuestStageState>()
        .start_quest(QuestFormId(QUEST), None);
    assert!(is_essential(&world, actor));
    assert!(!is_protected(&world, actor));
    assert!(spare_from_death(&world, actor, None));
}

/// The bleedout runs `fEssentialDeathTime` and the actor gets back up with
/// `fEssentialHealthPercentReGain` of its health.
#[test]
fn the_actor_recovers_when_the_bleedout_runs_out() {
    let (mut world, _player, actor) = world();
    world.insert(
        actor,
        ActorProtection {
            essential: true,
            protected: false,
        },
    );
    world
        .get_mut::<ActorValues>(actor)
        .unwrap()
        .apply_damage(HEALTH, 130.0);
    assert!(spare_from_death(&world, actor, None));
    bleedout_system(&world, 19.0);
    assert!(is_bleeding_out(&world, actor));
    bleedout_system(&world, 1.0);
    assert!(!is_bleeding_out(&world, actor));
    assert!(!crate::is_unconscious(&world, actor));
    assert_eq!(world.get::<ActorValues>(actor).unwrap().current(HEALTH), 30.0);
}

/// Authored settings win over the engine defaults; Starfield has no rules.
#[test]
fn rules_read_the_load_order_settings_else_the_engine_defaults() {
    let mut world = World::new();
    let mut index = EsmIndex {
        game: GameKind::Fallout3NV,
        character_rules: CharacterRulesProfile::FALLOUT_NEW_VEGAS,
        ..Default::default()
    };
    install_bleedout_rules(&mut world, &index);
    assert_eq!(
        *world.resource::<BleedoutRules>(),
        BleedoutRules {
            death_time_secs: 10.0,
            health_regain: 0.3
        }
    );
    index.game = GameKind::Skyrim;
    index.character_rules = CharacterRulesProfile::SKYRIM;
    index.game_settings.insert(
        0x10,
        GameSetting {
            form_id: 0x10,
            editor_id: "fEssentialDeathTime".into(),
            value: SettingValue::Float(20.0),
        },
    );
    install_bleedout_rules(&mut world, &index);
    assert_eq!(world.resource::<BleedoutRules>().death_time_secs, 20.0);
    index.game = GameKind::Starfield;
    index.character_rules = CharacterRulesProfile::STARFIELD;
    install_bleedout_rules(&mut world, &index);
    assert!(world.try_resource::<BleedoutRules>().is_none());
}

/// CK *SetEssential / SetProtected - ActorBase*: a script override of the
/// actor's base wins over its stamp, and setting one flag clears the other.
#[test]
fn script_overrides_of_the_actor_base_win() {
    const BASE: u32 = 0x0001_3478;
    let (mut world, _player, actor) = world();
    world.insert(
        actor,
        crate::scene::SceneAliasCandidate {
            reference_form_id: 0x0001_9DC7,
            base_form_id: BASE,
            linked_refs: Vec::new(),
            location_ref_types: Vec::new(),
        },
    );
    world.insert(
        actor,
        ActorProtection {
            essential: true,
            protected: false,
        },
    );
    world
        .resource_mut::<ActorBaseProtectionOverrides>()
        .set(BASE, false, false);
    assert!(!is_essential(&world, actor), "SetEssential(false)");
    world
        .resource_mut::<ActorBaseProtectionOverrides>()
        .set(BASE, true, true);
    assert!(is_protected(&world, actor));
    world
        .resource_mut::<ActorBaseProtectionOverrides>()
        .set(BASE, false, true);
    assert!(is_essential(&world, actor));
    assert!(!is_protected(&world, actor), "SetEssential clears protected");
}

/// #5858 — with no `BleedoutRules` (Starfield, FO76) a spared actor has no
/// recovery time and stays down, rather than a non-finite `f32` a save
/// cannot hold.
#[test]
fn without_rules_a_spared_actor_has_no_recovery_time_and_stays_down() {
    let (mut world, player, actor) = world();
    world.remove_resource::<BleedoutRules>();
    world.insert(
        actor,
        ActorProtection {
            essential: true,
            protected: false,
        },
    );
    assert!(spare_from_death(&world, actor, Some(player)));
    assert_eq!(world.get::<Bleedout>(actor).map(|state| state.remaining_secs), Some(None));
    bleedout_system(&world, 1_000.0);
    assert!(is_bleeding_out(&world, actor), "no rules: never gets back up");
    world.insert_resource(BleedoutRules {
        death_time_secs: 20.0,
        health_regain: 0.3,
    });
    bleedout_system(&world, 1_000.0);
    assert!(is_bleeding_out(&world, actor), "a timer-less bleedout never counts down");
}
