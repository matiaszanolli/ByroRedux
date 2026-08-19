//! Extracted from `save_io.rs`'s inline `mod tests` (#2407 / TD1-004).
//! Production code there is ~1030 LOC; the test bulk alone pushed the
//! file past 3000. Split by topic, contents unchanged.

use super::*;
use byroredux_core::ecs::components::{LightFlicker, LightSource, Transform};
use byroredux_core::form_id::FormIdPool;
use byroredux_core::math::Vec3;
use byroredux_core::string::StringPool;
use byroredux_save::{decode, restore_world};
use byroredux_scripting::ScriptTimer;

/// Tripwire for the [`MUTABLE_DELTA_COLUMNS`] invariant (SAVE-D1-02 /
/// SAVE-D6-01): the live overlay applies component *values* verbatim
/// onto a reloaded cell without re-installing the saved `StringPool`
/// or remapping value-embedded entity ids, so a delta column may carry
/// only session-stable fields — **no `FixedString`, no `EntityId`, no
/// session-local registry handle**.
///
/// Rust has no field reflection, so this can't auto-scan the structs.
/// Instead it pins the exact set against an audited expectation: adding
/// a column makes this fail, forcing the maintainer to confirm the new
/// type is delta-safe (per the doc comment) and update `AUDITED` here.
/// `Name` (FixedString) and `AnimationPlayer`/`AnimationStack`
/// (EntityId + registry handle) are the registered-but-excluded types
/// this guard exists to keep out.
#[test]
fn delta_columns_carry_only_session_stable_fields() {
    // Each entry was hand-verified free of FixedString / EntityId /
    // session-handle fields (Transform: glam f32s; Inventory: u32 +
    // ItemInstancePool index; EquipmentSlots: Option<u32> array;
    // LightSource/LightFlicker: f32/u32 + [f32;3]; ScriptTimer: u32+f32).
    const AUDITED: &[&str] = &[
        "Transform",
        "Inventory",
        "EquipmentSlots",
        "LightSource",
        "LightFlicker",
        "ScriptTimer",
        // #2291 — bool-only activator state plus ScriptVariables' stable
        // ConditionStringId(u64) → f32 map. No session-local identity.
        "TwoStateActivator",
        "ScriptVariables",
        // #1834 — ActorValues: HashMap<u32 AVIF-FormID, [f32; 4] layers>.
        // Keys are global-space FormIDs (stable across reload); values are
        // plain f32s. No FixedString / EntityId / session handle → delta-safe.
        "ActorValues",
        // EquippedWeapon: u32 inventory index + u32 base FormID + f32
        // damage. Dead: empty marker. Neither carries session identity.
        "EquippedWeapon",
        "Dead",
        // #2014 — WanderState/PatrolState: home/target Vec3 + WanderPhase
        // enum (Walking, or Paused{remaining: f32}) + pick_count u32.
        // TravelState: destination Vec3. GuardState: anchor Vec3.
        // Traveled/Escorted: empty unit-struct terminal markers. None
        // carry FixedString / EntityId / session handle → delta-safe.
        "WanderState",
        "TravelState",
        "Traveled",
        "GuardState",
        "PatrolState",
        "Escorted",
        // #2292 — ActorControlState: a single bool (`restrained`). No
        // FixedString / EntityId / session handle → delta-safe.
        "ActorControlState",
        // #2379 — RigidBodyData: MotionType enum (no payload) + 5 plain
        // f32s. No FixedString / EntityId / session handle → delta-safe.
        "RigidBodyData",
        // #2382 — RumbleOnActivate: f32/bool property fields +
        // RumbleState enum (Busy carries one f32). No FixedString /
        // EntityId / session handle → delta-safe.
        "RumbleOnActivate",
    ];
    assert_eq!(
        MUTABLE_DELTA_COLUMNS, AUDITED,
        "MUTABLE_DELTA_COLUMNS changed: a delta column must carry no \
         FixedString / EntityId / session-handle field (see the type's \
         doc comment). If the new type is delta-safe, add it to AUDITED.",
    );
}

/// The binary's curated registry must round-trip its full type set —
/// including the cross-crate `ScriptTimer`, a stable form id, and
/// (SAVE-D2-04 / #2021) `LightSource`/`LightFlicker` — through
/// encode → decode → restore into a fresh World.
///
/// SIBLING sweep (#2021): the other registered types with an
/// audit-confirmed flat shape (no `FixedString`/`EntityId`) —
/// `Inventory`/`EquipmentSlots` (round-tripped by
/// `crates/save/tests/round_trip.rs`'s `build_source_world`),
/// `ActorValues` (`actor_values_survive_save_load_round_trip`), and
/// `ScriptTimer` (this test) — already have dedicated round-trip
/// coverage. `FollowState`/`EscortState`/`Seated` are excluded from
/// this sweep: per the `MUTABLE_DELTA_COLUMNS` doc comment above they
/// carry `EntityId` fields, so they're not flat and a gap there would
/// be a different (higher-risk) finding, not this one.
#[test]
fn binary_registry_round_trips_including_scripttimer() {
    let reg = build_save_registry();

    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());
    let e = src.spawn();
    let other = src.spawn();
    src.insert(e, Transform::from_translation(Vec3::new(4.0, 5.0, 6.0)));
    src.insert(
        e,
        ScriptTimer {
            id: 42,
            remaining: 3.5,
        },
    );
    src.insert(
        other,
        ScriptTimer {
            id: 7,
            remaining: 0.25,
        },
    );
    let mut saved_light = LightSource::from_legacy_world_units(
        512.0,
        [0.9, 0.6, 0.2],
        0x0000_0008, // LIGHT_FLAG_FLICKER
        2.0,
        byroredux_core::ecs::LightKind::Point,
        [0.0; 3],
        0.0,
        0,
    );
    saved_light.dimmer = 0.75;
    saved_light.intensity = 1.25;
    src.insert(e, saved_light);
    src.insert(
        e,
        LightFlicker {
            animation_flags: byroredux_core::ecs::LIGHT_FLAG_FLICKER,
            period_secs: 0.5,
            intensity_amplitude: 0.25,
            movement_amplitude: 1.5,
            base_translation: [10.0, 20.0, 30.0],
            phase_offset_secs: 0.125,
        },
    );

    let snap = save_world(&src, &reg).unwrap();
    let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    assert_eq!(dst.next_entity_id(), 2);
    let q = dst.query::<ScriptTimer>().unwrap();
    let timers: std::collections::HashMap<u32, (u32, f32)> =
        q.iter().map(|(en, t)| (en, (t.id, t.remaining))).collect();
    assert_eq!(timers[&0], (42, 3.5));
    assert_eq!(timers[&1], (7, 0.25));

    let qt = dst.query::<Transform>().unwrap();
    assert_eq!(
        qt.iter().next().unwrap().1.translation,
        Vec3::new(4.0, 5.0, 6.0)
    );

    let light = dst.query::<LightSource>().unwrap().get(0).copied().unwrap();
    assert_eq!(light.emitter.range.to_bethesda_units(), 512.0);
    assert_eq!(light.emitter.radiant_intensity.get(), [0.9, 0.6, 0.2]);
    assert_eq!(light.flags, 0x0000_0008);
    assert_eq!(light.dimmer, 0.75);
    assert_eq!(light.intensity, 1.25);
    assert_eq!(light.emitter.falloff_exponent, 2.0);

    let flicker = dst
        .query::<LightFlicker>()
        .unwrap()
        .get(0)
        .copied()
        .unwrap();
    assert_eq!(flicker.period_secs, 0.5);
    assert_eq!(flicker.intensity_amplitude, 0.25);
    assert_eq!(flicker.movement_amplitude, 1.5);
    assert_eq!(flicker.base_translation, [10.0, 20.0, 30.0]);
    assert_eq!(flicker.phase_offset_secs, 0.125);
}

/// #1834 — an actor's layered `ActorValues` (class-auto-calc base plus any
/// `setav`/`modav` console edit) must survive a save → load round-trip.
/// Pre-fix the component was neither registered nor serialised, so a
/// reload dropped every permanent/temporary/damage layer and re-derived
/// only the spawn base.
#[test]
fn actor_values_survive_save_load_round_trip() {
    use byroredux_core::ecs::components::ActorValues;
    const AV_HEALTH: u32 = 0x0000_02C9;
    let reg = build_save_registry();

    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());
    let e = src.spawn();
    let mut av = ActorValues::new();
    av.set_base(AV_HEALTH, 100.0); // class auto-calc base
    av.mod_permanent(AV_HEALTH, 25.0); // e.g. a `modav` edit
    av.apply_damage(AV_HEALTH, 40.0);
    src.insert(e, av);

    let snap = save_world(&src, &reg).unwrap();
    let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let q = dst.query::<ActorValues>().unwrap();
    let (_, restored) = q.iter().next().expect("ActorValues must round-trip");
    // All four layers survive (pre-#1834 the whole component was dropped).
    assert_eq!(restored.current(AV_HEALTH), 85.0, "100 + 25 − 40");
    let layer = restored.get(AV_HEALTH).expect("entry present after reload");
    assert_eq!(layer.base, 100.0);
    assert_eq!(layer.permanent_mod, 25.0);
    assert_eq!(layer.damage, 40.0);
}

/// #2014 / SAVE-D1-NEW-01 — the seven M42 AI-procedure runtime-state
/// components must survive a save → load round-trip. Pre-fix, none were
/// registered, so a reload silently dropped them — the sharpest edge
/// being a terminal one-shot marker like `Traveled`: an NPC that had
/// already finished its Travel behavior would come back unfinished and
/// silently redo it. Covers one delta-safe state type (`WanderState`),
/// one terminal marker (`Traveled`), and one `EntityId`-carrying type
/// (`Seated`) — the three distinct save shapes this fix introduces.
#[test]
fn ai_procedure_state_and_terminal_markers_survive_save_load_round_trip() {
    use byroredux_core::ecs::components::{Seated, Traveled, WanderPhase, WanderState};
    let reg = build_save_registry();

    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let wanderer = src.spawn();
    src.insert(
        wanderer,
        WanderState {
            home: Vec3::new(1.0, 2.0, 3.0),
            target: Vec3::new(4.0, 5.0, 6.0),
            phase: WanderPhase::Paused { remaining: 2.5 },
            pick_count: 7,
        },
    );

    let arrived = src.spawn();
    src.insert(arrived, Traveled);

    let furniture = src.spawn();
    let sitter = src.spawn();
    src.insert(sitter, Seated { furniture });

    let snap = save_world(&src, &reg).unwrap();
    let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let wq = dst.query::<WanderState>().unwrap();
    let (_, restored_wander) = wq.iter().next().expect("WanderState must round-trip");
    assert_eq!(restored_wander.home, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(restored_wander.target, Vec3::new(4.0, 5.0, 6.0));
    assert_eq!(
        restored_wander.phase,
        WanderPhase::Paused { remaining: 2.5 }
    );
    assert_eq!(restored_wander.pick_count, 7);

    let tq = dst.query::<Traveled>().unwrap();
    assert_eq!(
        tq.iter().count(),
        1,
        "Traveled must round-trip — losing it makes an already-arrived NPC redo its Travel"
    );

    let sq = dst.query::<Seated>().unwrap();
    let (_, restored_seated) = sq.iter().next().expect("Seated must round-trip");
    // restore_world preserves entity ids verbatim, so the furniture
    // reference survives even though it wasn't a MUTABLE_DELTA_COLUMN.
    assert_eq!(restored_seated.furniture, furniture);
}

/// #2291 — recognized default2StateActivator instances carry live state
/// in two components. Dropping either one reverts the object's behavior
/// or makes GetVMScriptVariable observe stale values after reload.
#[test]
fn two_state_activator_and_script_variables_survive_save_load_round_trip() {
    use byroredux_scripting::{ScriptVariables, TwoStateActivator};

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let activator = src.spawn();
    src.insert(
        activator,
        TwoStateActivator {
            is_open: true,
            is_animating: false,
            do_once: true,
            activated_once: true,
        },
    );
    let mut variables = ScriptVariables::default();
    variables.set_by_name("::isOpen_var", 1.0);
    variables.set_by_name("::isAnimating_var", 0.0);
    src.insert(activator, variables);

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored = dst
        .get::<TwoStateActivator>(activator)
        .expect("TwoStateActivator must round-trip");
    assert_eq!(
        *restored,
        TwoStateActivator {
            is_open: true,
            is_animating: false,
            do_once: true,
            activated_once: true,
        }
    );

    let restored_variables = dst
        .get::<ScriptVariables>(activator)
        .expect("ScriptVariables must round-trip");
    assert_eq!(restored_variables.get_by_name("::isOpen_var"), Some(1.0));
    assert_eq!(
        restored_variables.get_by_name("::isAnimating_var"),
        Some(0.0)
    );
}

/// Regression: #2292 (SAVE-D1-09) — `Game.DisablePlayerControls`/
/// `Actor.SetRestrained` lock state must survive a save/load round
/// trip. Pre-fix, neither `PlayerControlState` (a `Resource`) nor
/// `ActorControlState` (a per-actor component) was in
/// `build_save_registry`, so a save taken mid-scripted-sequence
/// silently reset both to their all-enabled / unrestrained defaults
/// on reload.
#[test]
fn player_and_actor_control_state_survive_save_load_round_trip() {
    use byroredux_scripting::{ActorControlState, PlayerControlState};

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());
    src.insert_resource(PlayerControlState {
        movement_enabled: false,
        fighting_enabled: false,
        camera_switching_enabled: true,
        looking_enabled: true,
        sneaking_enabled: false,
        menu_enabled: true,
        activation_enabled: false,
        journal_tabs_enabled: true,
        disabled_pov_type: 1,
        ai_driven: true,
        hud_cart_mode: true,
    });

    let npc = src.spawn();
    src.insert(npc, ActorControlState { restrained: true });

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored_controls = dst
        .try_resource::<PlayerControlState>()
        .expect("PlayerControlState must round-trip");
    assert!(!restored_controls.movement_enabled);
    assert!(!restored_controls.fighting_enabled);
    assert!(restored_controls.camera_switching_enabled);
    assert!(!restored_controls.sneaking_enabled);
    assert!(!restored_controls.activation_enabled);
    assert_eq!(restored_controls.disabled_pov_type, 1);
    assert!(restored_controls.ai_driven);
    assert!(restored_controls.hud_cart_mode);
    drop(restored_controls);

    let restored_actor = dst
        .get::<ActorControlState>(npc)
        .expect("ActorControlState must round-trip");
    assert!(restored_actor.restrained);
}

/// Regression: #2379 (SAVE-D1-14) — a scripted `.SetMotionType()` call
/// must survive a save/load round trip. Pre-fix, `RigidBodyData` was
/// absent from `build_save_registry`, so a scripted motion-type change
/// (e.g. making a normally-static prop dynamic for a scripted sequence)
/// silently reverted to the ESM-derived default on reload.
#[test]
fn rigid_body_data_survives_save_load_round_trip() {
    use byroredux_core::ecs::components::{MotionType, RigidBodyData};

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let prop = src.spawn();
    src.insert(
        prop,
        RigidBodyData {
            motion_type: MotionType::Dynamic,
            mass: 12.5,
            friction: 0.4,
            restitution: 0.2,
            linear_damping: 0.1,
            angular_damping: 0.05,
            collidable: true,
        },
    );

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored = dst
        .get::<RigidBodyData>(prop)
        .expect("RigidBodyData must round-trip");
    assert_eq!(restored.motion_type, MotionType::Dynamic);
    assert_eq!(restored.mass, 12.5);
    assert_eq!(restored.friction, 0.4);
    assert_eq!(restored.restitution, 0.2);
    assert_eq!(restored.linear_damping, 0.1);
    assert_eq!(restored.angular_damping, 0.05);
}

/// Regression: #2382 (SAVE-D1-17) — a `defaultRumbleOnActivate` lever
/// mid-wait (or already one-shot-fired) must survive a save/load round
/// trip instead of silently resetting to `Active`.
#[test]
fn rumble_on_activate_survives_save_load_round_trip() {
    use byroredux_scripting::papyrus_demo::{RumbleOnActivate, RumbleState};

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let lever = src.spawn();
    src.insert(
        lever,
        RumbleOnActivate {
            camera_intensity: 0.5,
            duration: 1.5,
            repeatable: false,
            shake_left: 0.75,
            shake_right: 0.75,
            state: RumbleState::Busy {
                wait_remaining_secs: 0.9,
            },
        },
    );

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored = dst
        .get::<RumbleOnActivate>(lever)
        .expect("RumbleOnActivate must round-trip");
    assert_eq!(restored.camera_intensity, 0.5);
    assert_eq!(restored.duration, 1.5);
    assert!(!restored.repeatable);
    assert_eq!(restored.shake_left, 0.75);
    assert_eq!(restored.shake_right, 0.75);
    assert_eq!(
        restored.state,
        RumbleState::Busy {
            wait_remaining_secs: 0.9
        }
    );
}

/// Regression: #2378 (SAVE-D1-13) — a live `mat.set`-edited `Material`
/// must survive a save/load round trip. Pre-fix, `Material` was absent
/// from `build_save_registry`, so a debug-console material edit
/// silently reverted to whatever the NIF importer produced on reload.
#[test]
fn material_survives_save_load_round_trip() {
    use byroredux_core::ecs::components::material::Material;

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let mesh = src.spawn();
    src.insert(
        mesh,
        Material {
            metalness: 0.9,
            roughness: 0.15,
            alpha: 0.5,
            material_kind: 100, // MATERIAL_KIND_GLASS
            diffuse_color: [0.2, 0.4, 0.6],
            texture_path: Some("textures/edited_by_mat_set.dds".to_owned()),
            ..Material::default()
        },
    );

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored = dst.get::<Material>(mesh).expect("Material must round-trip");
    assert_eq!(restored.metalness, 0.9);
    assert_eq!(restored.roughness, 0.15);
    assert_eq!(restored.alpha, 0.5);
    assert_eq!(restored.material_kind, 100);
    assert_eq!(restored.diffuse_color, [0.2, 0.4, 0.6]);
    assert_eq!(
        restored.texture_path.as_deref(),
        Some("textures/edited_by_mat_set.dds")
    );
}

/// Regression: #2381 (SAVE-D1-16) — a fragment suspended mid-
/// `Utility.Wait` must survive a save/load round trip and still fire
/// its queued tail afterward. Pre-fix, `FragmentExecutionQueue` was
/// absent from `build_save_registry`, so a save taken mid-wait
/// silently dropped the pending `SetHudCartMode` the wait was gating.
#[test]
fn fragment_execution_queue_survives_save_load_round_trip_and_resumes() {
    use byroredux_scripting::papyrus_demo::PlayerEntity;
    use byroredux_scripting::quest_stages::{QuestStageAdvancedBatch, QuestStageState};
    use byroredux_scripting::translate::effects::Effect;
    use byroredux_scripting::{
        fragment_continuation_system, quest_fragment_dispatch_system, FragmentExecutionQueue,
        PlayerControlState, QuestFormId, QuestStageFragments,
    };

    const Q: QuestFormId = QuestFormId(0x0001_2345);

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());
    byroredux_scripting::register(&mut src);
    let player = src.spawn();
    src.insert_resource(PlayerEntity(player));
    src.insert_resource(QuestStageState::default());

    {
        let mut frags = src.resource_mut::<QuestStageFragments>();
        frags.insert(
            Q,
            10,
            vec![
                Effect::Wait { seconds: 5.0 },
                Effect::SetHudCartMode { cart_mode: true },
            ],
        );
    }
    src.resource_mut::<QuestStageState>().set_stage(Q, 10);
    {
        let mut q = src.query_mut::<QuestStageAdvancedBatch>().unwrap();
        q.insert(
            player,
            QuestStageAdvancedBatch(vec![
                byroredux_scripting::quest_stages::QuestStageAdvanced {
                    quest: Q,
                    previous_stage: 0,
                    new_stage: 10,
                },
            ]),
        );
    }
    quest_fragment_dispatch_system(&src);
    assert_eq!(
        src.resource::<FragmentExecutionQueue>().len(),
        1,
        "Utility.Wait must suspend into FragmentExecutionQueue"
    );
    assert!(!src.resource::<PlayerControlState>().hud_cart_mode);

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    assert_eq!(
        dst.resource::<FragmentExecutionQueue>().len(),
        1,
        "FragmentExecutionQueue must round-trip"
    );

    // Resuming after the restored wait still fires the queued tail —
    // proof the round-trip preserved real, resumable effect data, not
    // just an opaque blob.
    fragment_continuation_system(&dst, 5.0);
    assert!(
        dst.resource::<PlayerControlState>().hud_cart_mode,
        "the queued SetHudCartMode effect must still fire after restore"
    );
}

/// Regression: #2380 (SAVE-D1-15) — the MQ101 cinematic fragment-effect
/// state must survive a save/load round trip. Pre-fix,
/// `ActorCinematicState`/`HorseTetherState`/`CinematicPresentationState`
/// were absent from `build_save_registry`, so a save taken mid-cinematic
/// (riding a tethered cart, mid-`PlayIdle`, non-default sitting rotation)
/// silently reverted to default state on reload.
#[test]
fn cinematic_trio_survives_save_load_round_trip() {
    use byroredux_scripting::{ActorCinematicState, CinematicPresentationState, HorseTetherState};

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let vehicle = src.spawn();
    let actor = src.spawn();
    src.insert(
        actor,
        ActorCinematicState {
            vehicle: Some(vehicle),
            vehicle_local_translation: Some(Vec3::new(1.0, 2.0, 3.0)),
            requested_idle_form_id: Some(0xDEAD_BEEF),
            idle_request_serial: 7,
            ..Default::default()
        },
    );

    let horse = src.spawn();
    let cart = src.spawn();
    src.insert(
        cart,
        HorseTetherState {
            horse,
            horse_local_translation: Vec3::new(4.0, 5.0, 6.0),
            horse_local_rotation: byroredux_core::math::Quat::IDENTITY,
        },
    );

    let mut presentation = CinematicPresentationState::default();
    presentation.sitting_rotation_degrees = 42.0;
    src.insert_resource(presentation);

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored_actor = dst
        .get::<ActorCinematicState>(actor)
        .expect("ActorCinematicState must round-trip");
    // restore_world preserves entity ids verbatim, so the EntityId
    // reference resolves to the same numeric id it had at save time.
    assert_eq!(restored_actor.vehicle, Some(vehicle));
    assert_eq!(
        restored_actor.vehicle_local_translation,
        Some(Vec3::new(1.0, 2.0, 3.0))
    );
    assert_eq!(restored_actor.requested_idle_form_id, Some(0xDEAD_BEEF));
    assert_eq!(restored_actor.idle_request_serial, 7);

    let restored_tether = dst
        .get::<HorseTetherState>(cart)
        .expect("HorseTetherState must round-trip");
    assert_eq!(restored_tether.horse, horse);
    assert_eq!(
        restored_tether.horse_local_translation,
        Vec3::new(4.0, 5.0, 6.0)
    );

    let restored_presentation = dst
        .try_resource::<CinematicPresentationState>()
        .expect("CinematicPresentationState must round-trip");
    assert_eq!(restored_presentation.sitting_rotation_degrees, 42.0);
}

/// #1835 — every gameplay-state component `spawn_npc_entity` stamps on an
/// NPC placement root must be a deliberate save decision: registered in
/// [`build_save_registry`] (persisted + restored) XOR listed as
/// re-derived-from-static-ESM-at-respawn (write-once, no runtime mutator,
/// so not saving it is a correct no-op). A new spawn-stamp that is neither
/// — or one wrongly in both — trips this test. This is the structural
/// guard the `ActorValues` (#1834) gap lacked, so the pattern can't
/// silently repeat a third time.
///
/// Manually maintained — Rust has no reflection over `world.insert` sites,
/// same tripwire philosophy as `delta_columns_carry_only_session_stable_fields`.
/// When a runtime mutator lands for a re-derived type (leveling XP,
/// `AddPerk`, a faction-rank command), register it AND drop it from the
/// allowlist in the SAME commit (per #1835).
#[test]
fn npc_spawn_stamped_components_are_saved_or_intentionally_rederived() {
    // Persistent gameplay-state components stamped on the placement root by
    // `spawn_npc_entity` + its `stamp_*` helpers (`npc_spawn.rs`). Pure
    // placement scaffolding (Parent/Children), GPU handles, and transient
    // markers are out of scope — this guards actor state, the #1834 class.
    const NPC_SPAWN_STAMPED: &[&str] = &[
        "Transform",
        "Name",
        "Inventory",
        "EquipmentSlots",
        "ActorValues",
        "FactionRanks",
        "CharacterLevel",
        "Background",
        "Perks",
        "AmbientPackageRuntime",
    ];
    // Re-derived from static ESM `NPC_` data. Most entries are write-once;
    // AmbientPackageRuntime is the deliberate exception: its first
    // post-load tick recomputes the winner from PKID plus restored
    // clock/CTDA state, so persisting its cached winner is unnecessary.
    //
    // #2947 — CharacterLevel and Perks specifically hold only *while no
    // leveling runtime exists*: `npc_spawn.rs` always stamps
    // `CharacterLevel { xp: 0, .. }` and `Perks` verbatim from `PRKR`, so
    // there is nothing accumulated to lose today. CharacterLevel.xp is
    // CHARAL-defined runtime progress toward the next level — not
    // re-derivable from a static ESM record by construction — so the exempt
    // premise breaks the moment XP starts accumulating. That is not left to
    // this allow-list to notice on its own:
    // `crates/save/src/validate.rs::validate_progression_state` aborts any
    // save where a `CharacterLevel.xp != 0` slips through with these two
    // still unregistered, so the exemption fails loudly rather than
    // silently discarding progress.
    const REDERIVED_NOT_SAVED: &[&str] = &[
        "FactionRanks",
        "CharacterLevel",
        "Background",
        "Perks",
        // M42.9 — rebuilt from NPC_.PKID plus the restored clock/CTDA
        // state on the first ambient-package tick after a cell reload.
        "AmbientPackageRuntime",
    ];

    let registered: std::collections::HashSet<&str> =
        build_save_registry().component_names().collect();

    for name in NPC_SPAWN_STAMPED {
        let saved = registered.contains(name);
        let rederived = REDERIVED_NOT_SAVED.contains(name);
        assert!(
            saved ^ rederived,
            "NPC-spawn-stamped {name:?}: must be EITHER registered in \
             build_save_registry (saved={saved}) OR in REDERIVED_NOT_SAVED \
             (rederived={rederived}) — never both/neither. If it gained a \
             runtime mutator without deterministic reload re-derivation, \
             register it (#1834); otherwise document that re-derivation \
             here (#1835).",
        );
    }
}
