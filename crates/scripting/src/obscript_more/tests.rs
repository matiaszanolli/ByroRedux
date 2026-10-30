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

/// The Fallout 3 / New Vegas command ids this module ran when its commands moved into the command table
/// (#6139): an independent census, which `the_table_holds_exactly_the_listed_ids` checks the rows against.
const FALLOUT: &[u16] = &[
    0x101b, 0x1024, 0x1031, 0x1032, 0x103e, 0x1042, 0x1046, 0x1055, 0x1058, 0x1062, 0x1063, 0x1068, 0x106c, 0x106e,
    0x1070, 0x107f, 0x1083, 0x108d, 0x108f, 0x10a0, 0x10a2, 0x10a8, 0x10b1, 0x10bb, 0x10bd, 0x10c5, 0x10c6, 0x10d2,
    0x10d3, 0x10db, 0x10df, 0x10f5, 0x110d, 0x1111, 0x1116, 0x111c, 0x1129, 0x1137, 0x1138, 0x1151, 0x1155, 0x1159,
    0x115a, 0x115b, 0x115c, 0x115d, 0x1162, 0x1166, 0x116c, 0x1171, 0x1174, 0x1188, 0x1189, 0x118a, 0x118b, 0x118e,
    0x118f, 0x1196, 0x11ad, 0x11b6, 0x11b7, 0x11ba, 0x11c2, 0x11c4, 0x11c8, 0x11c9, 0x11cc, 0x11d0, 0x11d5, 0x11d6,
    0x11db, 0x11e2, 0x11e4, 0x11e6, 0x11e7, 0x11eb, 0x11ec, 0x11ef, 0x11f0, 0x11f4, 0x11f6, 0x1200, 0x1203, 0x120b,
    0x1212, 0x1213, 0x1224, 0x1225, 0x1226, 0x122e, 0x1230, 0x1248, 0x1270, 0x127a, 0x11d1, 0x11bc,
    0x11ed, 0x1238, 0x1250, 0x1260, 0x10aa, 0x119c, 0x11ff, 0x124e, 0x1254, 0x1256, 0x1252, 0x125c, 0x125e, 0x1266,
    0x126c, 0x1211, 0x1263, 0x1269, 0x122f, 0x1152,
    0x1135, 0x120c, 0x126f, 0x11fb, 0x11b5, 0x1114, 0x11a7, 0x1201, 0x124b, 0x124d, 0x1163, 0x1186, 0x121f, 0x1221, 0x121e, 0x1218, 0x121a, 0x1245, 0x11ee, 0x10da, 0x10e7, 0x126d, 0x11cd, 0x11ce, 0x106b, 0x1187, 0x116d, 0x1120, 0x1133, 0x1148, 0x1184, 0x1214, 0x11f5, 0x11e5, 0x11df, 0x1228, 0x11fc, 0x11d8, 0x125b, 0x123b, 0x11e1, 0x1268, 0x120a, 0x1119, 0x111b, 0x1127, 0x11ea, 0x11be, 0x11bf, 0x11dd, 0x1227, 0x1170, 0x10bc, 0x10ae, 0x10af, 0x110c, 0x1122, 0x1082,
];

/// Oblivion command ids this module runs.
const OBLIVION: &[u16] = &[
    0x101b, 0x1024, 0x1031, 0x1032, 0x103e, 0x1040, 0x1045, 0x1053, 0x1055, 0x1058, 0x1063, 0x106e, 0x1070, 0x1074,
    0x1075, 0x1076, 0x1077, 0x1082, 0x108f, 0x10a8, 0x10ae, 0x10af, 0x10b1, 0x10b5, 0x10bb, 0x10be, 0x10c1, 0x10c2,
    0x10c3, 0x10c4, 0x10c5, 0x10c6, 0x10c7, 0x10d1, 0x10d8, 0x10df, 0x10f8, 0x10f9, 0x10fb, 0x1109, 0x110d, 0x1116,
    0x111c, 0x1122, 0x1132, 0x1134, 0x1137, 0x1145, 0x1147, 0x114e, 0x1151, 0x1156, 0x1159, 0x115a, 0x115b, 0x115c,
    0x115d, 0x1161, 0x1162, 0x1164, 0x1165, 0x116b, 0x116c, 0x116d, 0x116e, 0x116f, 0x1170, 0x10aa, 0x112a, 0x112b, 0x1129, 0x10db, 0x114d, 0x105b, 0x1167, 0x110c,
    0x1030, 0x1057, 0x1062, 0x106b, 0x107b, 0x107c, 0x107d, 0x107e, 0x1083, 0x108d, 0x10a9, 0x10b0, 0x10b7, 0x10c8, 0x10d2, 0x10d3, 0x10d9, 0x10da, 0x10e0, 0x10ea, 0x10eb, 0x10f0, 0x10fc, 0x10fd, 0x1111, 0x1135, 0x1138, 0x114c, 0x1154, 0x1155, 0x1168, 0x1141, 0x106c, 0x10ab,
];


/// Run `cmd` as the host does: through the command table, under the loaded game.
fn run(world: &World, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    crate::obscript_cmd::run(world, cmd, args, ctx)
}

/// The rows of this module's five families hold exactly the ids of the census above, per game: the move
/// into the table lost no command and invented none (#6139).
#[test]
fn the_table_holds_exactly_the_listed_ids() {
    use crate::obscript_cmd::Games;
    let rows = [HEAD_COMMANDS, STATE_COMMANDS, ACTIONS_COMMANDS, EXTRAS_COMMANDS, TAIL_COMMANDS];
    for (fallout, list) in [(true, FALLOUT), (false, OBLIVION)] {
        let mut from_rows: Vec<u16> = rows
            .iter()
            .flat_map(|rows| rows.iter())
            .filter(|row| match row.games {
                Games::Both | Games::Any => true,
                Games::Fallout => fallout,
                Games::Oblivion => !fallout,
            })
            .map(|row| row.id)
            .collect();
        from_rows.sort_unstable();
        let mut listed: Vec<u16> = list.to_vec();
        listed.sort_unstable();
        listed.dedup();
        assert_eq!(from_rows, listed, "{}", if fallout { "Fallout" } else { "Oblivion" });
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
        (0x10c7, 0x10c7, FactionFlag::Attack),
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
fn state_nothing_reads_back_declines_instead_of_reporting_done() {
    // AddTopic / ModDisposition / SetQuestObject / SetItemValue have no consumer in the engine; the
    // decline-on-unmodeled rule keeps them from reporting a stored no-op as handled.
    let (fnv_world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(Some(ACTOR));
    assert_eq!(run(&fnv_world, 0x1058, &refs(&[0x0005_0001]), &c), Action::Declined, "AddTopic");
    let (oblivion_world, ..) = world(GameKind::Oblivion);
    assert_eq!(run(&oblivion_world, 0x1053, &nums(&[5.0]), &c), Action::Declined, "ModDisposition");
    assert_eq!(
        run(&fnv_world, 0x110d, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0)], &c),
        Action::Declined,
        "SetQuestObject"
    );
    assert_eq!(run(&fnv_world, 0x1155, &nums(&[100.0]), &c), Action::Declined, "SetItemValue");
}

#[test]
fn crime_gold_saturates_instead_of_overflowing() {
    let (world, ..) = world(GameKind::Oblivion);
    let c = ctx(Some(ACTOR));
    for _ in 0..2 {
        run(&world, 0x1076, &nums(&[i32::MAX as f32]), &c);
    }
    assert_eq!(world.try_resource::<ObScriptLedger>().unwrap().crime_gold, i32::MAX);
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
fn ignore_friendly_hits_is_read_back_by_its_getter() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(Some(ACTOR));
    assert_eq!(value(run(&world, 0x1152, &[], &c)), 0.0);
    run(&world, 0x1151, &nums(&[1.0]), &c);
    assert_eq!(value(run(&world, 0x1152, &[], &c)), 1.0);
    assert_eq!(value(run(&world, 0x1152, &[], &ctx(Some(OTHER)))), 0.0, "per actor");
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
    crate::obscript_cmd::run(&world, 0x1002, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(6.0)], &c);
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

// ── the last slice (extras) ─────────────────────────────────────────────────────────────────────

fn with_challenge(world: &mut World, form: u32, kind: u32, threshold: u32, flags: u32, stat: u16) {
    use byroredux_plugin::esm::records::{ChalRecord, EsmIndex};
    let mut index = EsmIndex::default();
    index.challenges.insert(form, ChalRecord { form_id: form, editor_id: "NVChallengeTest".into(), challenge_type: kind, threshold, flags, value1: stat, ..Default::default() });
    world.insert_resource(crate::challenges::ChallengeDefs::from_index(&index));
}

#[test]
fn the_challenge_commands_unlock_read_and_remove_the_recurring_flag() {
    const CHAL: u32 = 0x0016_5777;
    let (mut world, ..) = world(GameKind::Fallout3NV);
    with_challenge(&mut world, CHAL, byroredux_plugin::esm::records::challenge_kind::MISC_STAT, 1, byroredux_plugin::esm::records::CHALLENGE_FLAG_START_DISABLED, 37);
    let c = ctx(None);
    assert_eq!(value(run(&world, 0x1266, &refs(&[CHAL]), &c)), 0.0);
    assert!(is_done(&run(&world, 0x125e, &refs(&[CHAL]), &c)));
    mod_misc_stat(&world, "Caravan Games Won", 1);
    crate::challenges::challenge_system(&world, 0.0);
    assert_eq!(value(run(&world, 0x1266, &refs(&[CHAL]), &c)), 1.0);
    assert!(is_done(&run(&world, 0x126c, &refs(&[CHAL]), &c)));
    assert_eq!(run(&world, 0x125e, &refs(&[0xdead]), &c), Action::Declined, "an unknown challenge");
    assert_eq!(run(&world, 0x1266, &[], &c), Action::Declined, "no challenge named");
}

#[test]
fn the_casino_commands_answer_zero_winnings_and_queue_the_screens() {
    const CASINO: u32 = 0x0014_4c3e;
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(None);
    assert_eq!(value(run(&world, 0x125c, &refs(&[CASINO]), &c)), 0.0);
    for (cmd, menu) in [(0x1254, "blackjack"), (0x1256, "roulette"), (0x1252, "slot machine")] {
        run(&world, cmd, &refs(&[CASINO]), &c);
        assert_eq!(world.try_resource::<PendingMenus>().unwrap().0.last().map(|m| (m.menu, m.form)), Some((menu, CASINO)));
    }
}

#[test]
fn the_day_of_week_follows_the_days_run_from_each_games_start() {
    use crate::globals::{GameCalendar, GameClockGlobals};
    let (world, ..) = world(GameKind::Oblivion);
    let c = ctx(None);
    assert_eq!(value(run(&world, 0x10aa, &[], &c)), 1.0, "Oblivion starts on Morndas");
    world.try_resource_mut::<GameCalendar>().unwrap().days_elapsed = 6;
    assert_eq!(value(run(&world, 0x10aa, &[], &c)), 0.0, "six days on is Sundas");
    // Fallout: the date the globals author — 2281-10-18, a Tuesday (2) — plus the days run.
    let (mut world, ..) = world_fo();
    world.insert_resource(GameClockGlobals { start_date: Some((2281.0, 9.0, 18.0)), ..Default::default() });
    assert_eq!(value(run(&world, 0x10aa, &[], &c)), 2.0);
    world.try_resource_mut::<GameCalendar>().unwrap().days_elapsed = 3;
    assert_eq!(value(run(&world, 0x10aa, &[], &c)), 5.0);
}

#[test]
fn the_owner_last_target_is_the_last_actor_the_caller_struck() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(Some(ACTOR));
    let last = |world: &World| match run(world, 0x124e, &[], &c) {
        Action::Done { value: Some(ObScriptValue::Ref(form)), .. } => form,
        other => panic!("{other:?}"),
    };
    assert_eq!(last(&world), 0);
    world.try_resource_mut::<ObScriptLedger>().unwrap().note_hit(ACTOR, OTHER);
    assert_eq!(last(&world), OTHER);
}

#[test]
fn use_weapon_hands_the_actor_a_request_for_its_target() {
    let (world, actor, ..) = world(GameKind::Fallout3NV);
    let args = [ObScriptValue::Ref(ITEM), ObScriptValue::Ref(ACTOR), ObScriptValue::Ref(OTHER), ObScriptValue::Num(3.0)];
    assert!(is_done(&run(&world, 0x119c, &args, &ctx(Some(ACTOR)))));
    assert_eq!(world.get::<UseWeaponRequest>(actor).map(|r| (r.weapon, r.target, r.shots)), Some((ITEM, OTHER, 3)));
    assert_eq!(run(&world, 0x119c, &[ObScriptValue::Ref(ITEM)], &ctx(Some(ACTOR))), Action::Declined, "no target");
}

#[test]
fn play_music_queues_the_named_track_and_declines_an_unknown_one() {
    use byroredux_plugin::esm::records::{EsmIndex, MuscRecord};
    let (mut world, ..) = world(GameKind::Fallout3NV);
    let mut index = EsmIndex::default();
    index.music_types.insert(0x171b4a, MuscRecord { form_id: 0x171b4a, editor_id: "musDocMitchellHouse".into(), file: "loc\\a.mp3".into(), decibels: 6.0 });
    world.insert_resource(MusicTracks::from_index(&index));
    assert_eq!(world.try_resource::<MusicTracks>().unwrap().by_editor_id("MUSDOCMITCHELLHOUSE").map(|t| t.form_id), Some(0x171b4a));
    let c = ctx(None);
    assert!(is_done(&run(&world, 0x11ff, &refs(&[0x171b4a]), &c)));
    assert_eq!(world.try_resource::<PendingMusic>().unwrap().0.iter().map(|t| (t.file.as_str(), t.looping)).collect::<Vec<_>>(), [("loc\\a.mp3", true)]);
    assert_eq!(run(&world, 0x11ff, &refs(&[0x999]), &c), Action::Declined);
    assert_eq!(run(&world, 0x11ff, &[ObScriptValue::Ref(0)], &c), Action::Declined, "the editor ID named nothing");
}

#[test]
fn swap_texture_queues_the_swap_with_its_two_strings() {
    let (world, actor, ..) = world(GameKind::Fallout3NV);
    world.try_resource_mut::<TextArguments>().unwrap().0 = vec!["##GS01:0".into(), "interface\\endgame\\a.dds".into()];
    assert!(is_done(&run(&world, 0x1269, &refs(&[ACTOR]), &ctx(None))));
    assert_eq!(
        world.try_resource::<PendingTextureSwaps>().unwrap().0,
        vec![TextureSwap { target: actor, slot: "##GS01:0".into(), texture: "interface\\endgame\\a.dds".into() }]
    );
    assert_eq!(run(&world, 0x1269, &refs(&[ACTOR]), &ctx(None)), Action::Declined, "the strings were consumed");
}

#[test]
fn essential_death_reload_raises_the_reload_with_its_message() {
    let (world, ..) = world(GameKind::Oblivion);
    world.try_resource_mut::<TextArguments>().unwrap().0 = vec!["Martin has been slain.".into()];
    assert!(is_done(&run(&world, 0x114d, &[], &ctx(None))));
    assert_eq!(world.try_resource::<ReloadRequest>().unwrap().0.as_deref(), Some("Martin has been slain."));
}

#[test]
fn ash_piles_are_placed_at_the_caller_by_type() {
    let (mut world, ..) = world(GameKind::Fallout3NV);
    world.insert_resource(AshPiles { normal: 0x1b, goo: 0x22 });
    let c = ctx(Some(ACTOR));
    let made = |action: Action| match action {
        Action::Done { value: Some(ObScriptValue::Ref(form)), .. } => form,
        other => panic!("{other:?}"),
    };
    let ash = made(run(&world, 0x1211, &[], &c));
    let goo = made(run(&world, 0x1211, &nums(&[2.0]), &c));
    let created = world.try_resource::<crate::CreatedReferences>().unwrap();
    let base_of = |form: u32| created.iter().find(|r| r.form_id == form).map(|r| (r.base, r.at_reference));
    assert_eq!((base_of(ash), base_of(goo)), (Some((0x1b, ACTOR)), Some((0x22, ACTOR))));
}

#[test]
fn place_at_reticle_needs_the_apps_reticle_and_places_where_it_points() {
    use crate::created_references::{CreatedCell, CreatedPlacement};
    let (mut world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(Some(0x14));
    assert_eq!(run(&world, 0x1263, &refs(&[ITEM]), &c), Action::Declined, "no reticle installed");
    world.insert_resource(EngineQueries {
        reticle_point: Some(|_, distance| Some([1.0, 2.0, distance])),
        placement_at: Some(|_, point| Some(CreatedPlacement { cell: CreatedCell::Interior { editor_id: "Test".into() }, position_zup: point, rotation_zup: [0.0; 3] })),
        ..Default::default()
    });
    let args = [ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0), ObScriptValue::Num(0.0), ObScriptValue::Num(20000.0)];
    let form = match run(&world, 0x1263, &args, &c) {
        Action::Done { value: Some(ObScriptValue::Ref(form)), .. } => form,
        other => panic!("{other:?}"),
    };
    let created = world.try_resource::<crate::CreatedReferences>().unwrap();
    let placement = created.iter().find(|r| r.form_id == form).and_then(|r| r.placement.clone()).expect("placed");
    assert_eq!(placement.position_zup, [1.0, 2.0, 20000.0]);
}

#[test]
fn trap_update_hurts_an_actor_on_first_contact_or_every_call_when_continuous() {
    const TRAP: u32 = 0x00AA_0099;
    let (mut world, actor, ..) = world(GameKind::Oblivion);
    world.insert_resource(EngineQueries {
        // ACTOR touches the trap at 30 units/s relative speed, OTHER at 5.
        trap_contacts: Some(|world, _| {
            let entity = |form| live(world, form).unwrap();
            vec![TrapContact { actor: entity(ACTOR), relative_speed: 30.0 }, TrapContact { actor: entity(OTHER), relative_speed: 5.0 }]
        }),
        ..Default::default()
    });
    let trap_entity = world.spawn();
    let id = {
        let mut pool = world.resource_mut::<FormIdPool>();
        pool.intern(FormIdPair { plugin: PluginId::from_filename("FalloutNV.esm"), local: LocalFormId(TRAP) })
    };
    world.insert(trap_entity, FormIdComponent(id));
    let c = ctx(Some(TRAP));
    let hits = |world: &World| world.try_resource::<crate::PendingTrapHits>().unwrap().0.iter().map(|h| (h.target, h.damage)).collect::<Vec<_>>();
    // damage 3, push 0, death push 0, min velocity 20, continuous 0, levelled 0
    let first_contact = nums(&[3.0, 0.0, 0.0, 20.0, 0.0, 0.0]);
    assert!(is_done(&run(&world, 0x110c, &first_contact, &c)));
    assert_eq!(hits(&world), vec![(actor, 3.0)], "only the actor moving fast enough relative to the trap is hurt");
    run(&world, 0x110c, &first_contact, &c);
    assert_eq!(hits(&world).len(), 1, "a first-contact trap hurts once");
    let continuous = nums(&[3.0, 0.0, 0.0, 20.0, 1.0, 0.0]);
    run(&world, 0x110c, &continuous, &c);
    run(&world, 0x110c, &continuous, &c);
    assert_eq!(hits(&world).len(), 3, "a continuous trap hurts on every call");
}

// ── the long tail ───────────────────────────────────────────────────────────────────────────────

#[test]
fn the_platform_and_math_commands_answer_from_what_they_name() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(None);
    assert_eq!(value(run(&world, 0x1135, &[], &c)), 0.0, "IsXbox: this is a PC");
    assert_eq!(value(run(&world, 0x120c, &[], &c)), 1.0, "IsWin32");
    assert_eq!(value(run(&world, 0x121f, &nums(&[9.0]), &c)), 3.0, "Sqrt");
    assert_eq!(value(run(&world, 0x121f, &nums(&[-4.0]), &c)), 0.0, "Sqrt of a negative is not a NaN");
    assert_eq!(value(run(&world, 0x1221, &nums(&[-7.5]), &c)), 7.5, "Abs");
    assert!((value(run(&world, 0x121e, &nums(&[std::f32::consts::FRAC_PI_4]), &c)) - 1.0).abs() < 1e-5, "Tan");
}

#[test]
fn is_dlc_installed_reads_the_load_order() {
    use byroredux_core::form_id::PluginId;
    let (mut world, ..) = world(GameKind::Fallout3NV);
    assert_eq!(run(&world, 0x126f, &nums(&[0.0]), &ctx(None)), Action::Declined, "no load order installed");
    world.insert_resource(crate::LoadOrderIdentity::new(vec![(byroredux_plugin::esm::reader::GlobalSlot::Regular(0), PluginId::from_filename("FalloutNV.esm")), (byroredux_plugin::esm::reader::GlobalSlot::Regular(1), PluginId::from_filename("HonestHearts.esm"))]));
    assert_eq!(value(run(&world, 0x126f, &nums(&[0.0]), &ctx(None))), 0.0, "Dead Money is not loaded");
    assert_eq!(value(run(&world, 0x126f, &nums(&[1.0]), &ctx(None))), 1.0, "Honest Hearts is");
    assert_eq!(run(&world, 0x126f, &nums(&[9.0]), &ctx(None)), Action::Declined, "no such DLC");
}

#[test]
fn the_character_creation_commands_queue_their_screens_with_their_points() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(None);
    for (cmd, menu, points) in [(0x1218u16, "special points", 40u32), (0x121a, "tag skills", 3), (0x1245, "vigor tester", 25), (0x11ee, "special book", 40)] {
        assert!(is_done(&run(&world, cmd, &nums(&[points as f32]), &c)));
        let last = world.try_resource::<PendingMenus>().unwrap().0.last().cloned().unwrap();
        assert_eq!((last.menu, last.form), (menu, points), "{cmd:#06x}");
    }
    for (cmd, menu) in [(0x1268u16, "traits"), (0x11e1, "name entry"), (0x10da, "race")] {
        run(&world, cmd, &[], &c);
        assert_eq!(world.try_resource::<PendingMenus>().unwrap().0.last().map(|m| m.menu), Some(menu));
    }
}

#[test]
fn level_and_xp_commands_change_the_players_progress() {
    use byroredux_core::character::CharacterLevel;
    let (mut world, _, _, player) = world(GameKind::Fallout3NV);
    world.register::<CharacterLevel>();
    world.insert(player, CharacterLevel { level: 12, xp: 340 });
    let c = ctx(None);
    run(&world, 0x1133, &nums(&[1.0, 0.0]), &c);
    assert_eq!(world.get::<CharacterLevel>(player).map(|l| l.level), Some(1), "SetLevel 1 on the player (no caller: the player)");
    assert!(is_done(&run(&world, 0x126d, &[], &c)));
    assert_eq!(world.get::<CharacterLevel>(player).map(|l| l.xp), Some(0), "ResetXP");
    // On an NPC: relative to the player's level, clamped to [min, max].
    world.insert(player, CharacterLevel { level: 10, xp: 0 });
    run(&world, 0x1133, &nums(&[3.0, 1.0, 1.0, 12.0]), &ctx(Some(ACTOR)));
    let actor = live(&world, ACTOR).unwrap();
    assert_eq!(world.get::<CharacterLevel>(actor).map(|l| l.level), Some(12), "10 + 3, capped at 12");
}

#[test]
fn tag_skills_are_kept_and_asked_back() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(None);
    assert_eq!(value(run(&world, 0x11ce, &nums(&[38.0]), &c)), 0.0);
    assert!(is_done(&run(&world, 0x11cd, &nums(&[38.0, 0.0]), &c)));
    assert_eq!(value(run(&world, 0x11ce, &nums(&[38.0]), &c)), 1.0);
    run(&world, 0x11cd, &nums(&[41.0, 0.0]), &c);
    assert_eq!(value(run(&world, 0x11ce, &nums(&[38.0]), &c)), 0.0, "replaced in its slot");
    assert_eq!(run(&world, 0x11cd, &nums(&[38.0, 5.0]), &c), Action::Declined, "only slots 0-2");
}

#[test]
fn faction_commands_change_the_reputation_and_the_actors_factions() {
    use byroredux_core::character::FactionReputation;
    let (mut world, actor, _, player) = world(GameKind::Fallout3NV);
    world.register::<FactionReputation>();
    world.register::<FactionRanks>();
    world.insert(actor, FactionRanks(vec![(FACTION, 1), (FACTION + 1, 0)]));
    assert!(is_done(&run(&world, 0x1214, &[], &ctx(Some(ACTOR)))));
    assert!(world.get::<FactionRanks>(actor).unwrap().0.is_empty(), "RemoveFromAllFactions");
    // AddReputationExact <repu> <fame 1> <amount>
    run(&world, 0x123b, &[ObScriptValue::Ref(0x77), ObScriptValue::Num(1.0), ObScriptValue::Num(10.0)], &ctx(None));
    run(&world, 0x123b, &[ObScriptValue::Ref(0x77), ObScriptValue::Num(0.0), ObScriptValue::Num(4.0)], &ctx(None));
    let standing = world.get::<FactionReputation>(player).unwrap();
    assert_eq!((standing.fame(0x77), standing.infamy(0x77)), (10, 4));
}

#[test]
fn oblivion_stat_commands_read_and_set_the_players_standing() {
    let (world, ..) = world(GameKind::Oblivion);
    let c = ctx(None);
    run(&world, 0x10fc, &nums(&[52.0]), &c);
    run(&world, 0x10fd, &nums(&[9.0]), &c);
    assert_eq!((value(run(&world, 0x10f9, &[], &c)), value(run(&world, 0x10fb, &[], &c))), (52.0, 9.0));
    // GetPCMiscStat 7 is "Places Discovered".
    mod_misc_stat(&world, "Places Discovered", 4);
    assert_eq!(value(run(&world, 0x1138, &nums(&[7.0]), &c)), 4.0);
    assert_eq!(run(&world, 0x1138, &nums(&[99.0]), &c), Action::Declined, "out of the page's list");
    // IsPCAMurderer only after a murder flag.
    assert_eq!(value(run(&world, 0x10b0, &[], &c)), 0.0);
    run(&world, 0x10c4, &[ObScriptValue::Ref(FACTION), ObScriptValue::Num(1.0)], &c);
    assert_eq!(value(run(&world, 0x10b0, &[], &c)), 1.0);
    // SetPCFactionAttack / GetPCFactionAttack pair (0x10c8 / 0x10c7).
    run(&world, 0x10c8, &[ObScriptValue::Ref(FACTION), ObScriptValue::Num(1.0)], &c);
    assert_eq!(value(run(&world, 0x10c7, &refs(&[FACTION]), &c)), 1.0);
}

/// CS *IsPlayerInJail*: 0 until a sentence is being served, 1 while it is, 0 again once it ends.
#[test]
fn is_player_in_jail_follows_the_sentence() {
    let (world, ..) = world(GameKind::Oblivion);
    let c = ctx(None);
    assert_eq!(value(run(&world, 0x10ab, &[], &c)), 0.0);
    world.resource_mut::<crate::obscript_ledger::ObScriptLedger>().jail =
        Some(crate::obscript_ledger::JailSentence { exit_marker: 0, days: 3.0 });
    assert_eq!(value(run(&world, 0x10ab, &[], &c)), 1.0);
    world.resource_mut::<crate::obscript_ledger::ObScriptLedger>().jail = None;
    assert_eq!(value(run(&world, 0x10ab, &[], &c)), 0.0);
}

/// CS *GetWeaponAnimType* on Oblivion: the equipped weapon's `DATA` type grouped into 0 hand-to-hand
/// (no weapon, or a staff), 1 one-handed, 2 two-handed, 3 bow.
#[test]
fn oblivion_weapon_anim_type_groups_the_data_type() {
    use byroredux_plugin::esm::records::{EsmIndex, ItemKind, ItemRecord};
    let (mut world, actor, ..) = world(GameKind::Oblivion);
    let mut index = EsmIndex::default();
    index.game = GameKind::Oblivion;
    // Blade1H, Blade2H, Blunt1H, Blunt2H, Staff, Bow.
    for (form, weapon_type) in (0x100u32..).zip(0u8..6) {
        let mut kind = ItemKind::weapon_stub(10, 0);
        if let ItemKind::Weapon { oblivion_type, .. } = &mut kind {
            *oblivion_type = weapon_type;
        }
        index.items.insert(form, ItemRecord { form_id: form, common: Default::default(), kind });
    }
    world.insert_resource(crate::obscript_state::WeaponAnimTypes::from_index(&index));
    let c = ctx(Some(ACTOR));
    assert_eq!(value(run(&world, 0x106c, &[], &c)), 0.0, "no weapon: hand-to-hand");
    let equip = |world: &mut World, base_form_id| {
        world.insert(actor, EquippedWeapon { inventory_index: byroredux_core::ecs::components::InventoryIndex(0), base_form_id, damage: 1.0, reach: 0.0, speed: 1.0 });
    };
    for (form, expected) in [(0x100, 1.0), (0x101, 2.0), (0x102, 1.0), (0x103, 2.0), (0x104, 0.0), (0x105, 3.0)] {
        equip(&mut world, form);
        assert_eq!(value(run(&world, 0x106c, &[], &c)), expected, "weapon {form:#x}");
    }
}

/// Fallout compiles `GetPCMiscStat "Times Addicted"` to stat code 10 (CGTutorial's addiction
/// check): the command answers that stat's count, never the code itself, and `ModPCMiscStat` adds to
/// the named stat.
#[test]
fn fallout_misc_stat_commands_read_the_stat_their_code_names() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(None);
    assert_eq!(value(run(&world, 0x1138, &nums(&[10.0]), &c)), 0.0, "no addiction yet");
    run(&world, 0x1137, &nums(&[16.0, 2.0]), &c);
    assert_eq!(value(run(&world, 0x1138, &nums(&[16.0]), &c)), 2.0, "Bobbleheads Found");
    assert_eq!(misc_stat(&world, "Bobbleheads Found"), 2);
    assert_eq!(run(&world, 0x1138, &nums(&[99.0]), &c), Action::Declined, "past wbMiscStatEnum");
}

#[test]
fn mod_faction_reaction_builds_on_what_scripts_set() {
    let (mut world, ..) = world(GameKind::Oblivion);
    world.insert_resource(crate::LoadOrderIdentity::new(Vec::new()));
    // Without the faction identity the engine cannot hold a reaction: declined, nothing recorded as done.
    assert_eq!(run(&world, 0x10a9, &[ObScriptValue::Ref(FACTION), ObScriptValue::Ref(FACTION + 1), ObScriptValue::Num(-20.0)], &ctx(None)), Action::Declined);
}

#[test]
fn get_gold_counts_gold001_in_the_players_inventory() {
    use byroredux_core::ecs::components::ItemStack;
    let (mut world, _, _, player) = world(GameKind::Oblivion);
    let mut inventory = Inventory::new();
    inventory.push(ItemStack::new(0x0F, 120));
    inventory.push(ItemStack::new(0x1234, 3));
    world.insert(player, inventory);
    assert_eq!(value(run(&world, 0x1030, &[], &ctx(None))), 120.0);
}

#[test]
fn a_knocked_out_actor_reports_state_one_and_one_pushed_away_reports_zero() {
    let (world, actor, ..) = world(GameKind::Fallout3NV);
    let c = ctx(Some(ACTOR));
    assert_eq!(value(run(&world, 0x106b, &[], &c)), 0.0);
    crate::update_actor_control(&world, actor, |state| state.set_unconscious(true));
    assert_eq!(value(run(&world, 0x106b, &[], &c)), 1.0);
    assert_eq!(value(run(&world, 0x1187, &[], &c)), -1.0, "no hit location is recorded");
}

#[test]
fn the_cell_commands_keep_what_they_are_told_and_reset_interior_expires_the_visit() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(None);
    run(&world, 0x1119, &refs(&[0xAA]), &c);
    assert_eq!(world.try_resource::<ObScriptLedger>().unwrap().form(0x1119, 0xAA), Some(crate::PLAYER_REFERENCE), "no owner argument: the player");
    assert!(is_done(&run(&world, 0x1127, &refs(&[0xAA]), &c)));
    let mut visits = world.try_resource_mut::<crate::obscript_state::CellVisits>().unwrap();
    assert!(visits.enter(0xAA, 1.0, 72.0), "the next entry finds the expired cell reset");
}

#[test]
fn video_and_exit_requests_are_raised_for_the_app() {
    let (world, ..) = world(GameKind::Fallout3NV);
    world.try_resource_mut::<TextArguments>().unwrap().0 = vec!["credits.bik".into()];
    run(&world, 0x1114, &[], &ctx(None));
    assert_eq!(world.try_resource::<PendingVideos>().unwrap().0, vec!["credits.bik".to_owned()]);
    run(&world, 0x11b5, &[], &ctx(None));
    assert!(world.try_resource::<ExitGameRequest>().unwrap().0);
}

#[test]
fn drop_removes_the_item_and_leaves_a_reference_at_the_actors_feet() {
    let (mut world, actor, ..) = world(GameKind::Oblivion);
    world.insert_resource(crate::quest_stages::QuestStageState::default());
    let c = ctx(Some(ACTOR));
    crate::obscript_cmd::run(&world, 0x1002, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(5.0)], &c);
    assert!(is_done(&run(&world, 0x1057, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(2.0)], &c)));
    let held = world.get::<Inventory>(actor).map_or(0, |i| i.count_of(ITEM));
    assert_eq!(held, 3, "two dropped of five");
    let created = world.try_resource::<crate::CreatedReferences>().unwrap();
    assert!(created.iter().any(|r| r.base == ITEM && r.at_reference == ACTOR));
}

/// `PayFine` / `PayFineThief` / `GoToJail` (0x107b / 0x107c / 0x107e) through the host: the player pays or
/// goes to jail, and a refusal is a decline that changes nothing.
#[test]
fn the_fine_and_jail_commands_run_through_the_host() {
    use byroredux_core::character::CrimeModel;
    use byroredux_core::ecs::components::{GlobalTransform, ItemStack};
    use crate::jail::{Jail, JailTable};
    use crate::PlayerMoveRequest;

    let (mut world, ..) = world(GameKind::Oblivion);
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(&world).unwrap();
    world.register::<GlobalTransform>();
    world.insert(player, GlobalTransform::new(Default::default(), Default::default(), 1.0));
    world.insert(player, Inventory { items: vec![ItemStack::new(crate::guards::CURRENCY_FORM, 100)] });
    world.insert_resource(CrimeModel::OBLIVION);
    world.insert_resource(PlayerMoveRequest::default());
    world.insert_resource(PendingSleep::default());
    world.insert_resource(crate::cell_kind::PlayerCellKind(Some(crate::cell_kind::CellKind::Exterior { worldspace: 7 })));
    world.insert_resource(JailTable::for_test(vec![Jail { cell_marker: 0xC1, exit_marker: 0xE1, exit_worldspace: 7, exit_position: [0.0; 3], evidence_chest: None }]));
    let c = ctx(None);
    run(&world, 0x1075, &nums(&[40.0]), &c);

    // Cannot pay 400: declined, nothing taken.
    run(&world, 0x1075, &nums(&[400.0]), &c);
    let refused = run(&world, 0x107c, &[], &c);
    assert!(matches!(refused, Action::Declined), "{refused:?}");
    assert_eq!(value(run(&world, 0x1074, &[], &c)), 400.0);
    run(&world, 0x1075, &nums(&[40.0]), &c);

    assert!(is_done(&run(&world, 0x107c, &[], &c)), "PayFineThief");
    assert_eq!(value(run(&world, 0x1074, &[], &c)), 0.0);
    assert_eq!(world.get::<Inventory>(player).unwrap().items[0].count, 60);

    run(&world, 0x1075, &nums(&[200.0]), &c);
    assert!(is_done(&run(&world, 0x107e, &[], &c)), "GoToJail");
    assert_eq!(world.resource::<PlayerMoveRequest>().0, Some(0xC1));
    assert_eq!(world.get::<Inventory>(player).unwrap().items[0].count, 60, "no gold for a sentence");
    assert!(matches!(run(&world, 0x107e, &[], &c), Action::Declined), "already serving");
}

/// The same opcodes are other commands on Fallout: the Oblivion arms must not take them.
#[test]
fn the_fine_and_jail_ids_belong_to_oblivion_alone() {
    let (world, ..) = world(GameKind::Fallout3NV);
    let c = ctx(None);
    for id in [0x107b, 0x107c, 0x107e] {
        assert!(
            !matches!(run(&world, id, &[], &c), Action::Done { .. }),
            "{id:#06x} must not run Oblivion's fine on Fallout"
        );
    }
}

/// `GetCrimeKnown <type> <criminal> <victim>` answers from the crime record: the caller knows of that
/// crime by that criminal against that victim, or does not.
#[test]
fn get_crime_known_reads_what_the_caller_knows() {
    use crate::crime_knowledge::{CrimeEntry, CrimeKnowledge};
    use byroredux_core::character::CrimeKind;
    let (mut world, ..) = world(GameKind::Oblivion);
    world.insert_resource(CrimeKnowledge::default());
    let c = ctx(Some(ACTOR));
    let ask = |world: &World, kind: f32| {
        run(world, 0x1077, &[ObScriptValue::Num(kind), ObScriptValue::Ref(0x14), ObScriptValue::Ref(OTHER)], &c)
    };
    assert_eq!(value(ask(&world, 4.0)), 0.0);
    world.resource_mut::<CrimeKnowledge>().record(CrimeEntry { knower: ACTOR, criminal: 0x14, kind: CrimeKind::Murder, victim: OTHER });
    assert_eq!(value(ask(&world, 4.0)), 1.0, "murder, as recorded");
    assert_eq!(value(ask(&world, 0.0)), 0.0, "not theft");
    assert!(matches!(ask(&world, 9.0), Action::Declined), "a crime type the engine does not carry");
    let stranger = ctx(Some(OTHER));
    assert_eq!(
        value(run(&world, 0x1077, &[ObScriptValue::Num(4.0), ObScriptValue::Ref(0x14), ObScriptValue::Ref(OTHER)], &stranger)),
        0.0,
        "someone else does not know it"
    );
}

/// #6287 — `is_incapacitated` (an activation path's question) and `collect_unconscious` (a per-frame
/// scan's) are one set seen two ways. Every engine- and script-side way to take an actor out of play,
/// and the states that must not, give the same answer from both.
#[test]
fn incapacity_scalar_and_bulk_forms_agree() {
    use crate::bleedout::Bleedout;
    use crate::player_control::{update_actor_control, AiSuspended};

    type Apply = fn(&mut World, EntityId);
    let cases: [(&str, Apply, bool); 8] = [
        ("conscious and free", |_, _| {}, false),
        ("unconscious", |w, a| assert!(update_actor_control(w, a, |s| s.unconscious = true)), true),
        ("bleeding out", |w, a| w.insert(a, Bleedout { remaining_secs: Some(20.0) }), true),
        ("knocked down or fleeing (AiSuspended)", |w, a| w.insert(a, AiSuspended), true),
        ("SetActorsAI 0", |w, _| w.resource_mut::<crate::obscript_ledger::ObScriptLedger>().set_value(0x118b, ACTOR, 0.0), true),
        ("SetActorsAI 1", |w, _| w.resource_mut::<crate::obscript_ledger::ObScriptLedger>().set_value(0x118b, ACTOR, 1.0), false),
        ("a ghost, which can still talk", |w, a| assert!(update_actor_control(w, a, |s| s.ghost = true)), false),
        ("restrained only", |w, a| assert!(update_actor_control(w, a, |s| s.restrained = true)), false),
    ];
    for (name, apply, incapacitated) in cases {
        let (mut world, actor, other, _) = world(GameKind::Fallout3NV);
        world.register::<crate::player_control::ActorControlState>();
        world.register::<crate::bleedout::Bleedout>();
        world.register::<AiSuspended>();
        world.init_resource::<crate::obscript_ledger::ObScriptLedger>();
        apply(&mut world, actor);
        let mut bulk = Vec::new();
        crate::collect_unconscious(&world, &mut bulk);
        assert_eq!(crate::is_incapacitated(&world, actor), incapacitated, "{name}: scalar");
        assert_eq!(bulk.contains(&actor), incapacitated, "{name}: bulk");
        assert!(!crate::is_incapacitated(&world, other) && !bulk.contains(&other), "{name}: the bystander is unaffected");
    }
}

/// #6221 — state the engine does not track declines (so the `unknown_commands` census counts it) instead of
/// answering a constant: no birthsign, no swing in progress, no tracked player action other than jumping.
#[test]
fn untracked_state_declines_instead_of_answering_a_constant() {
    let (oblivion, ..) = world(GameKind::Oblivion);
    let c = ctx(Some(ACTOR));
    assert_eq!(run(&oblivion, 0x10e0, &refs(&[0x0005_0001]), &c), Action::Declined, "GetIsPlayerBirthsign");
    let (mut fo, fo_actor, ..) = world_fo();
    // An armed actor: the old arm answered ATTACK (2) for every armed combatant.
    fo.insert(fo_actor, EquippedWeapon { inventory_index: byroredux_core::ecs::components::InventoryIndex(0), base_form_id: ITEM, damage: 1.0, reach: 0.0, speed: 1.0 });
    assert_eq!(run(&fo, 0x10db, &[], &c), Action::Declined, "GetAnimAction");
    for index in [1.0, 2.0, 3.0, 4.0, 5.0, 7.0, 8.0, 9.0, 10.0] {
        assert_eq!(run(&fo, 0x1170, &nums(&[index]), &c), Action::Declined, "IsPlayerActionActive {index}");
    }
}
