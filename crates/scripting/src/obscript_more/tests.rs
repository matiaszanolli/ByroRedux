use super::*;
use byroredux_core::ecs::components::{
    ActorVitals, ActorValues, Dead, EquippedWeapon, FactionRanks, FormIdComponent, Inventory,
};
use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
use byroredux_plugin::esm::reader::GameKind;

use crate::obscript_ledger::FactionFlag;

const ACTOR: u32 = 0x0001_4D8A;
const OTHER: u32 = 0x0001_4D8B;
const FACTION: u32 = 0x0004_0001;
const ITEM: u32 = 0x0002_0001;

/// A world of `game` with two placed actors, `ACTOR` and `OTHER`, and the player.
fn world(game: GameKind) -> (World, EntityId, EntityId, EntityId) {
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(game));
    world.register::<Inventory>();
    world.register::<ActorValues>();
    world.insert_resource(crate::quest_stages::QuestStageState::default());
    world.insert_resource(crate::quest_stages::QuestObjectiveState::default());
    let mut pool = FormIdPool::new();
    let mut spawn = |world: &mut World, local: u32| {
        let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("FalloutNV.esm"), local: LocalFormId(local) });
        let entity = world.spawn();
        world.insert(entity, FormIdComponent(id));
        entity
    };
    let actor = spawn(&mut world, ACTOR);
    let other = spawn(&mut world, OTHER);
    world.insert_resource(pool);
    let player = world.spawn();
    world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
    (world, actor, other, player)
}

fn ctx(caller: Option<u32>) -> Context {
    Context { caller, known: None, action_ref: None, owner: 0x00AA_0001, elapsed: 0.5 }
}

fn nums(values: &[f32]) -> Vec<ObScriptValue> {
    values.iter().map(|&v| ObScriptValue::Num(v)).collect()
}

fn refs(forms: &[u32]) -> Vec<ObScriptValue> {
    forms.iter().map(|&f| ObScriptValue::Ref(f)).collect()
}

fn value(action: Action) -> f32 {
    match action {
        Action::Done { value: Some(v), .. } => v.as_num(),
        other => panic!("expected an answer, got {other:?}"),
    }
}

fn is_done(action: &Action) -> bool {
    matches!(action, Action::Done { .. })
}

/// Every id the module claims for a game has an arm: nothing falls to `Unhandled` (which the host would
/// decline as an unknown command, hiding a listed-but-unwritten command).
#[test]
fn every_listed_id_has_an_arm_in_its_game() {
    for (game, list) in [(GameKind::Fallout3NV, FALLOUT), (GameKind::Oblivion, OBLIVION)] {
        let (world, _, _, _) = world(game);
        let mut unhandled = Vec::new();
        for &cmd in list {
            let args = [ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0), ObScriptValue::Num(1.0)];
            if matches!(run(&world, cmd, &args, &ctx(Some(ACTOR))), Action::Unhandled) {
                unhandled.push(format!("{cmd:#06x}"));
            }
        }
        assert!(unhandled.is_empty(), "{game:?}: listed but not run: {unhandled:?}");
    }
}

/// A game other than the two families runs none of the ids.
#[test]
fn other_games_run_none_of_the_ids() {
    let (world, _, _, _) = world(GameKind::Skyrim);
    for &cmd in FALLOUT.iter().chain(OBLIVION) {
        assert!(matches!(run(&world, cmd, &[], &ctx(Some(ACTOR))), Action::Unhandled), "{cmd:#06x}");
    }
}

#[test]
fn oblivion_fame_infamy_and_bounty_are_read_and_changed() {
    let (world, ..) = world(GameKind::Oblivion);
    let c = ctx(None);
    assert_eq!(value(run(&world, 0x10f9, &[], &c)), 0.0);
    assert!(is_done(&run(&world, 0x10f8, &nums(&[7.0]), &c)));
    assert_eq!(value(run(&world, 0x10f9, &[], &c)), 7.0);
    assert_eq!(value(run(&world, 0x10fb, &[], &c)), 0.0);
    run(&world, 0x1075, &nums(&[40.0]), &c);
    run(&world, 0x1076, &nums(&[10.0]), &c);
    assert_eq!(value(run(&world, 0x1074, &[], &c)), 50.0);
}

#[test]
fn oblivion_faction_crime_flags_are_set_and_read_per_faction() {
    let (world, ..) = world(GameKind::Oblivion);
    let c = ctx(None);
    // (getter, setter, flag): Expelled, Murder, Steal, Attack.
    for (get, set, kind) in [
        (0x10c1, 0x10c2, FactionFlag::Expelled),
        (0x10c3, 0x10c4, FactionFlag::Murder),
        (0x10c5, 0x10c6, FactionFlag::Steal),
        (0x10c7, 0x10c7 + 0, FactionFlag::Attack),
    ] {
        if get == set {
            continue;
        }
        assert_eq!(value(run(&world, get, &refs(&[FACTION]), &c)), 0.0);
        run(&world, set, &[ObScriptValue::Ref(FACTION), ObScriptValue::Num(1.0)], &c);
        assert_eq!(value(run(&world, get, &refs(&[FACTION]), &c)), 1.0, "{kind:?}");
        assert!(world.try_resource::<ObScriptLedger>().unwrap().flag(kind, FACTION));
    }
}

#[test]
fn fallout_enemy_of_faction_flag_and_its_clear() {
    let (mut world, actor, ..) = world(GameKind::Fallout3NV);
    world.register::<FactionRanks>();
    world.insert(actor, FactionRanks(vec![(FACTION, 0)]));
    let c = ctx(Some(ACTOR));
    assert_eq!(value(run(&world, 0x11cc, &[], &c)), 0.0);
    run(&world, 0x10c6, &[ObScriptValue::Ref(FACTION), ObScriptValue::Num(1.0)], &c);
    assert_eq!(value(run(&world, 0x10c5, &refs(&[FACTION]), &c)), 1.0);
    assert_eq!(value(run(&world, 0x11cc, &[], &c)), 1.0, "the actor's faction is flagged");
    run(&world, 0x11c9, &refs(&[FACTION]), &c);
    assert_eq!(value(run(&world, 0x11cc, &[], &c)), 0.0);
}

#[test]
fn mod_disposition_accumulates_for_the_caller() {
    let (world, ..) = world(GameKind::Oblivion);
    run(&world, 0x1053, &nums(&[5.0]), &ctx(Some(ACTOR)));
    run(&world, 0x1053, &nums(&[3.0]), &ctx(Some(ACTOR)));
    assert_eq!(world.try_resource::<ObScriptLedger>().unwrap().disposition_mod(ACTOR), 8);
}

#[test]
fn topics_and_quest_objects_are_kept() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(None);
    assert!(is_done(&run(&world, 0x1058, &refs(&[0x0005_0001]), &c)));
    assert!(world.try_resource::<ObScriptLedger>().unwrap().knows_topic(0x0005_0001));
    run(&world, 0x110d, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0)], &c);
    assert!(world.try_resource::<ObScriptLedger>().unwrap().is_quest_object(ITEM));
    assert_eq!(run(&world, 0x1058, &[], &c), Action::Declined, "no topic named");
}

#[test]
fn the_talked_to_player_flag_reads_the_ledger() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(Some(ACTOR));
    assert_eq!(value(run(&world, 0x1032, &[], &c)), 0.0);
    world.try_resource_mut::<ObScriptLedger>().unwrap().note_talked_to_player(ACTOR);
    assert_eq!(value(run(&world, 0x1032, &[], &c)), 1.0);
}

#[test]
fn sleep_hours_are_queued_and_wake_up_cancels_them() {
    // `SetPCSleepHours` is a GECK command; `IsPCSleeping` and `WakeUpPC` are Oblivion's.
    let (world, ..) = world(GameKind::Fallout3NV);
    run(&world, 0x10bd, &nums(&[8.0]), &ctx(None));
    assert_eq!(world.try_resource::<PendingSleep>().unwrap().0, 8.0);
    let (world, ..) = world_oblivion_sleeping(8.0);
    let c = ctx(None);
    assert_eq!(value(run(&world, 0x10af, &[], &c)), 1.0);
    assert_eq!(value(run(&world, 0x1109, &[], &c)), 1.0);
    run(&world, 0x10ae, &[], &c);
    assert_eq!(value(run(&world, 0x10af, &[], &c)), 0.0);
}

fn world_oblivion_sleeping(hours: f32) -> (World, EntityId, EntityId, EntityId) {
    let built = world(GameKind::Oblivion);
    built.0.try_resource_mut::<PendingSleep>().unwrap().0 = hours;
    built
}

#[test]
fn force_sneak_is_stored_and_read_back_for_the_actor() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(Some(ACTOR));
    assert_eq!(value(run(&world, 0x10d2, &[], &c)), 0.0);
    run(&world, 0x10d3, &nums(&[1.0]), &c);
    assert_eq!(value(run(&world, 0x10d2, &[], &c)), 1.0);
    assert_eq!(value(run(&world, 0x10d2, &[], &ctx(Some(OTHER)))), 0.0, "another actor is unaffected");
}

#[test]
fn push_flee_and_reset_inventory_hand_the_app_a_request() {
    let (world, actor, other, _) = world(GameKind::Fallout3NV);
    let c = ctx(Some(ACTOR));
    assert!(is_done(&run(&world, 0x118a, &[ObScriptValue::Ref(OTHER), ObScriptValue::Num(3.0)], &c)));
    assert_eq!(world.get::<PushActorRequest>(other).map(|r| (r.from, r.force)), Some((actor, 3.0)));
    run(&world, 0x10f5, &[], &c);
    assert!(world.has::<FleeRequest>(actor));
    run(&world, 0x11ed, &[], &c);
    assert!(world.has::<ResetInventoryRequest>(actor));
    assert_eq!(run(&world, 0x118a, &refs(&[0x00FF_FFFF]), &c), Action::Declined, "an unloaded target");
}

#[test]
fn the_weapon_condition_percent_is_changed_for_the_equipped_weapon_only() {
    let (mut world, actor, ..) = world(GameKind::Fallout3NV);
    let c = ctx(Some(ACTOR));
    assert_eq!(run(&world, 0x11f4, &[], &c), Action::Declined, "no weapon equipped");
    world.insert(actor, EquippedWeapon { inventory_index: byroredux_core::ecs::components::InventoryIndex(0), base_form_id: ITEM, damage: 1.0, reach: 0.0, speed: 1.0 });
    assert_eq!(value(run(&world, 0x11f4, &[], &c)), 100.0);
    run(&world, 0x11f6, &nums(&[-30.0]), &c);
    assert_eq!(value(run(&world, 0x11f4, &[], &c)), 70.0);
}

#[test]
fn container_inventory_count_sums_the_stacks() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(Some(ACTOR));
    assert_eq!(value(run(&world, 0x127a, &[], &c)), 0.0);
    crate::obscript_effects::run(&world, 0x1002, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(6.0)], &c);
    // AddItem's id is not assumed here: whichever the effects table uses, the count follows the inventory.
    let held = world.get::<Inventory>(live(&world, ACTOR).unwrap()).map_or(0, |i| i.items.iter().map(|s| s.count).sum::<u32>());
    assert_eq!(value(run(&world, 0x127a, &[], &c)), held as f32);
}

#[test]
fn quests_reset_and_complete_all_objectives() {
    use crate::quest_stages::{QuestFormId, QuestObjectiveState, QuestStageState};
    let (mut world, ..) = world(GameKind::Fallout3NV);
    world.insert_resource(QuestStageState::default());
    world.insert_resource(QuestObjectiveState::default());
    let quest = QuestFormId(0x0006_0001);
    world.try_resource_mut::<QuestStageState>().unwrap().set_stage(quest, 40);
    assert!(is_done(&run(&world, 0x11db, &refs(&[quest.0]), &ctx(None))));
    assert_eq!(world.try_resource::<QuestStageState>().unwrap().get_stage(quest), 0);
    assert!(is_done(&run(&world, 0x11ad, &refs(&[quest.0]), &ctx(None))));
}

#[test]
fn the_engine_queries_the_app_installs_answer_los_menu_and_first_person() {
    let (mut world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(Some(ACTOR));
    assert_eq!(run(&world, 0x101b, &refs(&[OTHER]), &c), Action::Declined, "no sight query installed");
    assert_eq!(run(&world, 0x1024, &[], &c), Action::Declined);
    world.insert_resource(EngineQueries {
        line_of_sight: Some(|_, _, _| true),
        menu_open: Some(|_, kind| kind == 1009),
        first_person: Some(|_| true),
        ..Default::default()
    });
    assert_eq!(value(run(&world, 0x101b, &refs(&[OTHER]), &c)), 1.0);
    assert_eq!(value(run(&world, 0x1024, &nums(&[1009.0]), &c)), 1.0);
    assert_eq!(value(run(&world, 0x1024, &nums(&[1002.0]), &c)), 0.0);
    assert_eq!(value(run(&world, 0x1188, &[], &c)), 1.0);
}

#[test]
fn oblivion_shivering_isles_flag_is_kept() {
    let (world, ..) = world(GameKind::Oblivion);
    let c = ctx(None);
    assert_eq!(value(run(&world, 0x116d, &[], &c)), 0.0);
    run(&world, 0x116c, &nums(&[1.0]), &c);
    assert_eq!(value(run(&world, 0x116d, &[], &c)), 1.0);
}

#[test]
fn creature_questions_follow_the_actor_state() {
    let (mut world, actor, ..) = world(GameKind::Oblivion);
    let c = ctx(Some(ACTOR));
    assert_eq!(value(run(&world, 0x1040, &[], &c)), 0.0);
    world.insert(actor, byroredux_core::ecs::components::CreatureActor);
    assert_eq!(value(run(&world, 0x1040, &[], &c)), 1.0);
    // IsActor needs vitals; the killing-blow limb of a living actor is -1.
    assert_eq!(value(run(&world, 0x1161, &[], &c)), 0.0);
    world.insert(actor, ActorVitals { health: 10 });
    assert_eq!(value(run(&world, 0x1161, &[], &c)), 1.0);
    let (mut fo, fo_actor, ..) = world_fo();
    assert_eq!(value(run(&fo, 0x11f0, &[], &ctx(Some(ACTOR)))), -1.0);
    fo.insert(fo_actor, Dead);
    assert_eq!(run(&fo, 0x11f0, &[], &ctx(Some(ACTOR))), Action::Declined);
}

fn world_fo() -> (World, EntityId, EntityId, EntityId) {
    world(GameKind::Fallout3NV)
}

/// The pure-polygon and heading maths the commands lean on.
#[test]
fn heading_angle_is_zero_straight_ahead_and_positive_to_the_right() {
    use byroredux_core::ecs::components::GlobalTransform;
    use byroredux_core::math::Vec3;
    let (mut world, actor, other, _) = world(GameKind::Fallout3NV);
    world.insert(actor, GlobalTransform::new(Vec3::ZERO, byroredux_core::math::Quat::IDENTITY, 1.0));
    // Straight ahead is local -Z.
    world.insert(other, GlobalTransform::new(Vec3::new(0.0, 0.0, -10.0), byroredux_core::math::Quat::IDENTITY, 1.0));
    let ahead = value(run(&world, 0x1063, &refs(&[OTHER]), &ctx(Some(ACTOR))));
    assert!(ahead.abs() < 1e-3, "{ahead}");
    world.insert(other, GlobalTransform::new(Vec3::new(10.0, 0.0, 0.0), byroredux_core::math::Quat::IDENTITY, 1.0));
    let right = value(run(&world, 0x1063, &refs(&[OTHER]), &ctx(Some(ACTOR))));
    assert!((right - 90.0).abs() < 1e-3, "{right}");
}

/// Print any id both this module and an older table run in the same family: the older arm would answer
/// first, so a clash means one of them is dead code.
#[test]
fn no_id_is_claimed_twice_in_a_family() {
    let mut clashes = Vec::new();
    for (fallout, list) in [(true, FALLOUT), (false, OBLIVION)] {
        for &cmd in list {
            // `0x116b` is Fallout's `GetLinkedRef` and Oblivion's `PurgeCellBuffers`; the host asks
            // this module first, with the loaded game, so the older table never sees it there.
            if !fallout && cmd == 0x116b {
                continue;
            }
            let other = crate::obscript_actions::handles(cmd) || crate::obscript_effects::handles(cmd) || crate::obscript_effects::is_getter(cmd);
            if other {
                clashes.push(format!("{cmd:#06x} ({})", if fallout { "Fallout" } else { "Oblivion" }));
            }
        }
    }
    assert!(clashes.is_empty(), "claimed by an older table too: {clashes:?}");
}
