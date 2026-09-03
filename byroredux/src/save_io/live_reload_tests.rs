//! Extracted from `save_io.rs`'s inline `mod tests` (#2407 / TD1-004).
//! Production code there is ~1030 LOC; the test bulk alone pushed the
//! file past 3000. Split by topic, contents unchanged.

use super::*;
use byroredux_core::form_id::FormIdPool;
use byroredux_core::math::Vec3;
use byroredux_core::string::StringPool;
use byroredux_save::decode;

/// FlyCam round-trip: capture reads the camera Transform + look
/// angles into [`PlayerPose`]; apply puts them back after the pose is
/// scrambled — the camera returns to the saved spot and InputState to
/// the saved yaw/pitch.
#[test]
fn player_pose_round_trips_flycam() {
    use crate::components::InputState;
    use crate::systems::PlayerMode;
    use byroredux_core::ecs::{ActiveCamera, Transform};

    let mut world = World::new();
    world.insert_resource(PlayerMode::FlyCam);
    world.insert_resource(PlayerPose::default());
    world.insert_resource(InputState {
        yaw: 1.25,
        pitch: -0.4,
        ..InputState::default()
    });

    let cam = world.spawn();
    world.insert(
        cam,
        Transform::from_translation(Vec3::new(10.0, 20.0, 30.0)),
    );
    world.insert_resource(ActiveCamera(cam));

    capture_player_pose(&world);
    let pose = *world.resource::<PlayerPose>();
    assert_eq!(pose.position, [10.0, 20.0, 30.0]);
    assert!((pose.yaw - 1.25).abs() < 1e-6);
    assert!((pose.pitch + 0.4).abs() < 1e-6);
    assert!(!pose.character_mode);

    // Scramble, then restore.
    {
        let mut tq = world.query_mut::<Transform>().unwrap();
        tq.get_mut(cam).unwrap().translation = Vec3::ZERO;
    }
    {
        let mut i = world.resource_mut::<InputState>();
        i.yaw = 0.0;
        i.pitch = 0.0;
    }
    apply_player_pose(&mut world, &pose);

    let tq = world.query::<Transform>().unwrap();
    assert_eq!(
        tq.get(cam).unwrap().translation,
        Vec3::new(10.0, 20.0, 30.0)
    );
    let i = world.resource::<InputState>();
    assert!((i.yaw - 1.25).abs() < 1e-6);
    assert!((i.pitch + 0.4).abs() < 1e-6);
}

/// Character mode keys the captured position off the player *body*
/// (not the camera), and apply moves that body — the camera follows
/// it the next frame via `camera_follow_system`.
#[test]
fn player_pose_character_tracks_body() {
    use crate::components::InputState;
    use crate::systems::{PlayerEntity, PlayerMode};
    use byroredux_core::ecs::{GlobalTransform, Transform};
    use byroredux_core::math::Quat;

    let mut world = World::new();
    world.insert_resource(PlayerMode::Character);
    world.insert_resource(PlayerPose::default());
    world.insert_resource(InputState::default());
    let body = world.spawn();
    world.insert(
        body,
        Transform::from_translation(Vec3::new(-5.0, 64.0, 12.0)),
    );
    world.insert(
        body,
        GlobalTransform::new(Vec3::new(-5.0, 64.0, 12.0), Quat::IDENTITY, 1.0),
    );
    world.insert_resource(PlayerEntity(Some(body)));

    capture_player_pose(&world);
    let pose = *world.resource::<PlayerPose>();
    assert_eq!(pose.position, [-5.0, 64.0, 12.0]);
    assert!(pose.character_mode);

    // Apply a different saved pose; the body relocates (no Rapier
    // handles in the test → `set_kinematic_translation` is a no-op).
    let restored = PlayerPose {
        position: [100.0, 50.0, -25.0],
        yaw: 0.5,
        pitch: 0.1,
        character_mode: true,
    };
    apply_player_pose(&mut world, &restored);
    let tq = world.query::<Transform>().unwrap();
    assert_eq!(
        tq.get(body).unwrap().translation,
        Vec3::new(100.0, 50.0, -25.0)
    );
}

/// #2018 / SAVE-D6-03 — a pose saved in FlyCam mode (`pose.position` is
/// the CAMERA position) reloaded into a live Character-mode session
/// must still relocate the BODY, not fall through to the camera-only
/// branch (which `camera_follow_system` would silently overwrite one
/// frame later). The body's feet land `eye_height` below the saved
/// camera position, mirroring `snap_character_body_to_camera`'s
/// `cam_pos - eye_height` — so `camera_follow_system` re-derives
/// exactly the saved camera vantage (`body.y + eye_height ==
/// saved_camera.y`) every subsequent frame instead of reverting.
#[test]
fn player_pose_flycam_saved_relocates_body_in_live_character_mode() {
    use crate::components::InputState;
    use crate::systems::{PlayerEntity, PlayerMode};
    use byroredux_core::ecs::{GlobalTransform, Transform};
    use byroredux_core::math::Quat;
    use byroredux_physics::CharacterController;

    let mut world = World::new();
    world.insert_resource(PlayerMode::Character);
    world.insert_resource(InputState::default());
    let body = world.spawn();
    // Body sits far from the saved pose before restore — proves the
    // fallback branch (which left it untouched pre-fix) didn't run.
    world.insert(body, Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)));
    world.insert(
        body,
        GlobalTransform::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, 1.0),
    );
    world.insert(body, CharacterController::HUMAN);
    world.insert_resource(PlayerEntity(Some(body)));

    // A FlyCam-mode save: `position` is the camera's absolute position.
    let saved = PlayerPose {
        position: [10.0, 200.0, 30.0],
        yaw: 0.3,
        pitch: -0.1,
        character_mode: false,
    };
    apply_player_pose(&mut world, &saved);

    let expected_body_y = 200.0 - CharacterController::HUMAN.eye_height;
    let tq = world.query::<Transform>().unwrap();
    let body_pos = tq.get(body).unwrap().translation;
    assert_eq!(
        body_pos,
        Vec3::new(10.0, expected_body_y, 30.0),
        "body must relocate to camera_pos - eye_height, not stay untouched"
    );

    // `camera_follow_system`'s derivation must reproduce the exact
    // saved camera Y from this body placement.
    assert_eq!(
        body_pos.y + CharacterController::HUMAN.eye_height,
        200.0,
        "camera_follow_system's body.y + eye_height must reproduce the saved camera Y"
    );
}

/// A `PlayerPose` rides along in the snapshot as a registered resource
/// and decodes back out by name — the wire the live load reads.
#[test]
fn player_pose_survives_snapshot_round_trip() {
    let reg = build_save_registry();
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.insert_resource(FormIdPool::new());
    world.insert_resource(PlayerPose {
        position: [1.0, 2.0, 3.0],
        yaw: 0.7,
        pitch: -0.2,
        character_mode: true,
    });

    let snap = save_world(&world, &reg).unwrap();
    let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let pose = snapshot_player_pose(&decoded).expect("pose column present");
    assert_eq!(pose.position, [1.0, 2.0, 3.0]);
    assert!(pose.character_mode);
    assert!((pose.yaw - 0.7).abs() < 1e-6);
}

/// The day/night clock is persistent gameplay state, not a render-only
/// parameter. Exercise the same resource overlay used by a live `load`,
/// including the hidden rate retained while the clock is paused.
#[test]
fn game_time_survives_live_resource_restore() {
    use crate::components::GameTimeRes;

    let reg = build_save_registry();
    let mut source = World::new();
    source.insert_resource(StringPool::new());
    source.insert_resource(FormIdPool::new());
    let mut game_time = GameTimeRes::new(23.5, 120.0);
    game_time.advance_hours(49.0);
    game_time.pause();
    source.insert_resource(game_time);

    let snapshot = save_world(&source, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut restored = World::new();
    restored.insert_resource(GameTimeRes::default());
    byroredux_save::restore_resources(&mut restored, &reg, &decoded).unwrap();

    let mut time = restored.resource_mut::<GameTimeRes>();
    assert_eq!(time.day, 3);
    assert!((time.hour - 0.5).abs() < 1e-6);
    assert!(time.is_paused());
    time.resume();
    assert_eq!(time.time_scale, 120.0);
}

/// #1862 / SAVE-07 — `QuestStageState` and `QuestObjectiveState` are
/// live gameplay state (Papyrus `SetStage`/`GetStageDone` and
/// `SetObjectiveDisplayed`/`SetObjectiveCompleted`/`SetObjectiveFailed`),
/// mutated every frame by real recognizer-emitted scripts. Pre-fix
/// neither type carried a `Serialize`/`Deserialize` derive and neither
/// was registered in `build_save_registry`, so both silently reverted
/// to default on every save/load. This pins the round trip through the
/// same snapshot → encode → decode → restore_resources pipeline the
/// live M45.1 overlay load uses.
#[test]
fn quest_stage_and_objective_state_survive_snapshot_round_trip() {
    use byroredux_scripting::quest_stages::{QuestFormId, QuestObjectiveState, QuestStageState};

    let reg = build_save_registry();
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.insert_resource(FormIdPool::new());

    let quest = QuestFormId(0x0002_2f08); // the real DA10 quest FormID
    let mut stages = QuestStageState::default();
    stages.set_stage(quest, 37);
    stages.set_stage(quest, 40);
    world.insert_resource(stages);

    let mut objectives = QuestObjectiveState::default();
    objectives.set_displayed(quest, 10, true);
    objectives.set_completed(quest, 10, true);
    world.insert_resource(objectives);

    let snap = save_world(&world, &reg).unwrap();
    let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    // Full restore_world path (loose/test load).
    let mut restored_world = World::new();
    byroredux_save::restore_world(&mut restored_world, &reg, &decoded).unwrap();
    let restored_stages = restored_world.resource::<QuestStageState>();
    assert_eq!(restored_stages.get_stage(quest), 40);
    assert!(restored_stages.get_stage_done(quest, 37));
    assert!(restored_stages.get_stage_done(quest, 40));
    assert!(
        !restored_stages.get_stage_done(quest, 20),
        "never-visited stage stays false"
    );
    // #3819 — drop before acquiring `QuestObjectiveState` below. The
    // `BYRO_LOCK_ORDER_CHECK=1` thread-local graph tracks acquisitions by
    // TYPE, not by which `World` instance a `ResourceRead` came from — an
    // un-dropped guard here (its lexical scope runs to the end of this
    // test fn) would still record a `QuestStageState → QuestObjectiveState`
    // edge against this thread even though `overlay_world` below is a
    // completely different `World`. Without this drop (and the matching
    // one after `restored_objectives`), that edge plus the reverse one at
    // `overlay_stages` closes a same-thread cycle and panics.
    drop(restored_stages);

    let restored_objectives = restored_world.resource::<QuestObjectiveState>();
    let status = restored_objectives.get(quest, 10);
    assert!(status.displayed);
    assert!(status.completed);
    assert!(!status.failed);
    drop(restored_objectives);

    // Live M45.1 overlay path (restore_resources — resource-only, no
    // entity clear/respawn).
    let mut overlay_world = World::new();
    overlay_world.insert_resource(StringPool::new());
    overlay_world.insert_resource(FormIdPool::new());
    byroredux_save::restore_resources(&mut overlay_world, &reg, &decoded).unwrap();
    let overlay_stages = overlay_world.resource::<QuestStageState>();
    assert_eq!(overlay_stages.get_stage(quest), 40);
    assert!(overlay_stages.get_stage_done(quest, 37));
}

/// QUST alias CNTO entries are permanent grants. The alias resolver's
/// ledger must survive a live resource restore where the reloaded actor's
/// session-local entity ID differs from the one present at save time.
#[test]
fn quest_alias_inventory_grant_ledger_survives_live_reload_with_new_entity_id() {
    use byroredux_plugin::esm::records::{
        AliasFillType, AliasInjectedData, QuestAlias, QustRecord,
    };
    use byroredux_scripting::{
        install_scene_quest_aliases, refresh_scene_actor_bindings, SceneAliasCandidate,
    };

    let reg = build_save_registry();
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let actor = world.spawn();
    world.insert(
        actor,
        SceneAliasCandidate {
            reference_form_id: 0xA1,
            base_form_id: 0xB1,
            linked_refs: Vec::new(),
            location_ref_types: Vec::new(),
        },
    );
    let quest_record = QustRecord {
        form_id: 0x100,
        aliases: vec![QuestAlias {
            alias_id: 1,
            fill_type: Some(AliasFillType::ForcedReference(0xA1)),
            injected: AliasInjectedData {
                inventory: vec![(0xC1, 2), (0xC2, 1)],
                ..Default::default()
            },
            ..Default::default()
        }],
        ..Default::default()
    };
    install_scene_quest_aliases(&mut world, [quest_record.clone()]);
    refresh_scene_actor_bindings(&world);

    let snap = save_world(&world, &reg).unwrap();
    let grants = snap.resources["QuestAliasInjectionState"]["inventory_grants"]
        .as_array()
        .expect("grant ledger serializes as a sequence");
    assert_eq!(grants.len(), 2);

    let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();
    let mut restored = World::new();
    byroredux_scripting::register(&mut restored);
    let _unrelated = restored.spawn();
    let restored_actor = restored.spawn();
    assert_ne!(
        restored_actor, actor,
        "live reload advances allocation and must not preserve entity ids"
    );
    restored.insert(
        restored_actor,
        SceneAliasCandidate {
            reference_form_id: 0xA1,
            base_form_id: 0xB1,
            linked_refs: Vec::new(),
            location_ref_types: Vec::new(),
        },
    );
    let mut inventory = byroredux_core::ecs::components::Inventory::new();
    inventory.push(byroredux_core::ecs::components::ItemStack::new(0xC1, 2));
    inventory.push(byroredux_core::ecs::components::ItemStack::new(0xC2, 1));
    restored.insert(restored_actor, inventory);
    byroredux_save::restore_resources(&mut restored, &reg, &decoded).unwrap();
    install_scene_quest_aliases(&mut restored, [quest_record]);
    refresh_scene_actor_bindings(&restored);
    assert_eq!(
        restored
            .get::<byroredux_core::ecs::components::Inventory>(restored_actor)
            .unwrap()
            .items
            .len(),
        2,
        "restored ledger prevents duplicate CNTO stacks on first refresh"
    );
    let restored_snap = save_world(&restored, &reg).unwrap();
    let restored_grants = restored_snap.resources["QuestAliasInjectionState"]["inventory_grants"]
        .as_array()
        .expect("restored grant ledger remains serializable");
    assert_eq!(restored_grants.len(), 2);
}

/// #3789 — the saved `ReferenceEnableState` must be in the world BEFORE the
/// cell reload spawns anything.
///
/// Since #3278 the ledger has a *spawn-time* consumer:
/// `cell_loader::spawn::placement_is_disabled` consults it per placed REFR,
/// ahead of any mesh, collider or light. `restore_resources` used to run
/// only after the reload, so the reload took its spawn decisions against the
/// live session's ledger — on a fresh `--load N` that is
/// `ReferenceEnableState::default()`, i.e. everything enabled, so every
/// reference the save recorded as disabled came back solid. `apply_deltas`,
/// which follows, is additive-only by contract and can neither spawn nor
/// despawn, so nothing downstream could correct it.
///
/// Pinned by source ordering: `execute_pending_save_loads` needs a live
/// `VulkanContext` and on-disk archives, so the spawn decision itself is not
/// reachable from a unit test. What is checkable — and what actually broke —
/// is that the restore precedes the reload.
#[test]
fn saved_resources_are_restored_before_the_cell_reload() {
    // No production/test split needed: this module lives in its own file,
    // and `save_io.rs`'s inline test module contains none of the tokens
    // matched below.
    let source = include_str!("../save_io.rs");
    let start = source
        .find("pub fn execute_pending_save_loads(")
        .expect("the live-load drain must exist");
    let body = &source[start..];

    let pre_restore = body
        .find("byroredux_save::restore_resources(world, &registry, &snapshot)")
        .expect("the drain must restore saved resources");
    let interior_reload = body
        .find("reload_interior_session(")
        .expect("the drain must reload the saved interior");
    let exterior_reload = body
        .find("reload_exterior_session(")
        .expect("the drain must reload the saved exterior");

    assert!(
        pre_restore < interior_reload && pre_restore < exterior_reload,
        "saved resources must be restored BEFORE either reload branch — the \
         spawn gate reads `ReferenceEnableState` while the cell is being \
         built, and a post-reload restore arrives too late to affect it \
         (#3789)"
    );

    // Both branches are equally exposed: `assemble_exterior_streaming`
    // reaches the same `spawn_placed_instances`. And the post-reload restore
    // must survive, since it is what re-asserts saved values over what the
    // reload itself rebuilt (`CurrentCellContext`, `PlayerPose`).
    let after_reload = body[exterior_reload..]
        .find("byroredux_save::restore_resources(world, &registry, &snapshot)")
        .expect(
            "the post-reload restore must stay — it re-asserts saved values \
             over resources the reload rebuilds",
        );
    assert!(after_reload > 0);
}
