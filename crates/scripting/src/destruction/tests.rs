use super::*;

fn stage(percent: u8, index: u8, model: u8, flags: u8) -> DestructionStage {
    DestructionStage { health_percent: percent, index, model_damage_stage: model, flags, self_damage_per_second: 0 }
}

/// The CK tutorial's four stages of decay (75, 50, 25, 0) with the last one
/// destroying and disabling, on an object of 100 health.
fn pillar() -> DestructionData {
    DestructionData {
        health: 100,
        stages: vec![
            stage(75, 0, 1, 0),
            stage(50, 1, 2, 0),
            stage(25, 2, 3, 0),
            stage(0, 3, 4, DestructionStage::DESTROY | DestructionStage::DISABLE),
        ],
    }
}

#[test]
fn undamaged_is_minus_one_and_damage_walks_the_stages() {
    let data = pillar();
    assert_eq!(current_stage(&data, 0.0), -1);
    let mut state = ReferenceDestruction::default();
    let outcome = apply_damage(&data, &mut state, 10.0);
    assert_eq!((outcome.old_stage, outcome.new_stage), (-1, -1), "90% is above every stage");
    let outcome = apply_damage(&data, &mut state, 20.0);
    assert_eq!((outcome.old_stage, outcome.new_stage), (-1, 1), "70% reached the 75% stage");
    assert!(!state.destroyed);
    // One big hit skips stages: 70% -> 20%.
    let outcome = apply_damage(&data, &mut state, 50.0);
    assert_eq!((outcome.old_stage, outcome.new_stage), (1, 3), "both events' stage numbers are passed");
    // Down to 0%: the destroying, disabling stage.
    let outcome = apply_damage(&data, &mut state, 500.0);
    assert_eq!(outcome.new_stage, 4);
    assert!(outcome.destroyed && outcome.disable && state.destroyed);
    assert_eq!(state.damage, 100.0, "damage stops at the object's health");
}

#[test]
fn cap_damage_stops_a_hit_at_the_capped_stage() {
    // CK: 20 health and a stage capped at 80% - the first hit that would pass it
    // only brings the object down to 80%.
    let data = DestructionData {
        health: 20,
        stages: vec![stage(80, 0, 1, DestructionStage::CAP_DAMAGE), stage(0, 1, 2, DestructionStage::DESTROY)],
    };
    let mut state = ReferenceDestruction::default();
    let outcome = apply_damage(&data, &mut state, 15.0);
    assert!((state.damage - 4.0).abs() < 1e-4, "capped at 80% health: {}", state.damage);
    assert_eq!(outcome.new_stage, 1);
    assert!(!state.destroyed);
    let outcome = apply_damage(&data, &mut state, 16.0);
    assert_eq!(outcome.new_stage, 2, "the second hit goes through to 0%");
    assert!(state.destroyed);
}

#[test]
fn a_stage_that_ignores_external_damage_takes_none() {
    let data = DestructionData {
        health: 100,
        stages: vec![stage(50, 0, 1, DestructionStage::IGNORE_EXTERNAL_DAMAGE)],
    };
    let mut state = ReferenceDestruction::default();
    apply_damage(&data, &mut state, 60.0);
    let outcome = apply_damage(&data, &mut state, 30.0);
    assert_eq!((outcome.new_stage, state.damage), (1, 60.0));
}

#[test]
fn authored_self_damage_advances_loaded_references_and_queues_stage_changes() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    let mut world = World::new();
    crate::register(&mut world);
    world.register::<FormIdComponent>();
    let plugin = PluginId::from_filename("Skyrim.esm");
    let mut pool = FormIdPool::default();
    let reference = 0x101;
    let unloaded = 0x102;
    let id = pool.intern(FormIdPair { plugin, local: LocalFormId(reference) });
    pool.intern(FormIdPair { plugin, local: LocalFormId(unloaded) });
    world.insert_resource(pool);
    let entity = world.spawn();
    world.insert(entity, FormIdComponent(id));
    let base = 0x103;
    world.insert(entity, crate::scene::SceneAliasCandidate { base_form_id: base, ..Default::default() });
    let flags = DestructionStage::CAP_DAMAGE | DestructionStage::IGNORE_EXTERNAL_DAMAGE;
    let data = DestructionData {
        health: 100,
        stages: vec![
            DestructionStage { self_damage_per_second: 10, ..stage(99, 0, 1, flags) },
            stage(94, 1, 2, DestructionStage::CAP_DAMAGE | DestructionStage::DISABLE),
        ],
    };
    world.insert_resource(DestructionTable([(base, data)].into()));
    assert_eq!(damage_object(&world, entity, reference, 5.0).unwrap().new_stage, 1);
    assert_eq!(damage_object(&world, entity, reference, 5.0).unwrap().new_stage, 1);
    world.resource_mut::<PendingStageChanges>().0.clear();
    world.resource_mut::<DestructionStates>().0.insert(unloaded, ReferenceDestruction { damage: 1.0, destroyed: false });

    self_damage_system(&world, 0.25);
    assert_eq!(destruction_stage(&world, entity, reference), 1);
    assert!(world.resource::<PendingStageChanges>().0.is_empty());

    self_damage_system(&world, 0.5);
    assert_eq!(destruction_stage(&world, entity, reference), 2);
    assert_eq!(world.resource::<PendingStageChanges>().0, [entity]);
    assert!(!crate::fragment::reference_enabled(&world, reference));
    let states = world.resource::<DestructionStates>();
    assert_eq!(states.0[&reference].damage, 6.0);
    assert_eq!(states.0[&unloaded].damage, 1.0);
}

#[test]
fn zero_health_objects_have_no_percentage_to_cross() {
    let data = DestructionData { health: 0, stages: vec![stage(0, 0, 1, DestructionStage::DESTROY)] };
    let mut state = ReferenceDestruction::default();
    assert_eq!(apply_damage(&data, &mut state, 0.0).new_stage, -1);
    let outcome = apply_damage(&data, &mut state, 1.0);
    assert!(outcome.destroyed);
}
