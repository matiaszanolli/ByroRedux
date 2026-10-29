use super::*;
use crate::obscript_cmd::run;

/// #6365 — the live-caller prologue lives in `live_caller` /
/// `open_actor_effects`. 45 handlers had spelled it out (four lines, plus a
/// `Binder` for fifteen), each a place for the "declines when the caller is
/// not loaded" policy to drift.
#[test]
fn handlers_open_through_the_live_caller_helpers() {
    let production = byroredux_core::source_scan::production_text(super::SOURCES);
    let hand_spelled = ["let caller_entity = ", "live(world, caller);"].concat();
    assert!(
        !production.contains(&hand_spelled),
        "a handler re-spells the live-caller prologue — open it with `live_caller` or `open_actor_effects` (#6365)"
    );
}

/// A getter through the command table: the number or reference it answers, or a decline.
fn getter(world: &World, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    run(world, cmd, args, context)
}
use byroredux_core::ecs::components::{ActorValues, FormIdComponent, Inventory, Locked};
use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{AvifRecord, EsmIndex};

const ACTOR: u32 = 0x0001_4D8A;
const OTHER: u32 = 0x0001_4D8B;
const ITEM: u32 = 0x0002_0001;
const LIST: u32 = 0x0002_0002;
const STRENGTH: u32 = 0x3E8;
const HEALTH: u32 = 0x450;

/// A Fallout world with two placed actors, `ACTOR` and `OTHER`, and the
/// Strength / Health AVIFs a code can name.
fn world() -> (World, EntityId, EntityId) {
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::quest_stages::QuestStageState::default());
    world.insert_resource(crate::quest_stages::QuestObjectiveState::default());
    world.register::<ActorValues>();
    world.register::<Inventory>();
    world.register::<Locked>();
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
    let mut index = EsmIndex { game: GameKind::Fallout3NV, ..Default::default() };
    for (form_id, editor_id) in [(STRENGTH, "AVStrength"), (HEALTH, "AVHealth")] {
        index.actor_values.insert(form_id, AvifRecord { form_id, editor_id: editor_id.into(), ..Default::default() });
    }
    crate::install_actor_value_names(&mut world, &index);
    (world, actor, other)
}

fn context(caller: Option<u32>) -> Context {
    Context { caller, known: None, action_ref: None, owner: 0x00AA_0001, elapsed: 0.5 }
}

fn done(action: &Action) -> bool {
    matches!(action, Action::Done { .. })
}

fn count(world: &World, entity: EntityId, item: u32) -> u32 {
    world
        .get::<Inventory>(entity)
        .map_or(0, |inventory| inventory.count_of(item))
}

fn refs(forms: &[u32]) -> Vec<ObScriptValue> {
    forms.iter().map(|&form| ObScriptValue::Ref(form)).collect()
}

#[test]
fn add_item_and_remove_item_move_the_stack_in_the_callers_inventory() {
    let (world, actor, _) = world();
    let mut args = refs(&[ITEM]);
    args.push(ObScriptValue::Num(5.0));
    assert!(done(&run(&world, CMD_ADD_ITEM, &args, &context(Some(ACTOR)))));
    assert_eq!(count(&world, actor, ITEM), 5);
    args[1] = ObScriptValue::Num(2.0);
    assert!(done(&run(&world, CMD_REMOVE_ITEM, &args, &context(Some(ACTOR)))));
    assert_eq!(count(&world, actor, ITEM), 3);
    assert_eq!(
        getter(&world, CMD_GET_ITEM_COUNT, &refs(&[ITEM]), &context(Some(ACTOR))),
        Action::number(3.0),
        "GetItemCount reads the same stack"
    );
}

#[test]
fn a_form_list_stands_for_each_of_its_members() {
    const SECOND: u32 = 0x0002_0003;
    let (mut world, actor, _) = world();
    let mut index = EsmIndex::default();
    index.form_lists.insert(LIST, byroredux_plugin::esm::records::FlstRecord { form_id: LIST, entries: vec![ITEM, SECOND], ..Default::default() });
    crate::install_form_lists(&mut world, &index);
    let args = [ObScriptValue::Ref(LIST), ObScriptValue::Num(2.0)];
    assert!(done(&run(&world, CMD_ADD_ITEM, &args, &context(Some(ACTOR)))));
    assert_eq!((count(&world, actor, ITEM), count(&world, actor, SECOND)), (2, 2));
    assert_eq!(getter(&world, CMD_GET_ITEM_COUNT, &refs(&[LIST]), &context(Some(ACTOR))), Action::number(4.0));
}

#[test]
fn a_command_on_a_reference_that_is_not_loaded_is_declined() {
    let (world, _, _) = world();
    let args = [ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0)];
    assert_eq!(run(&world, CMD_ADD_ITEM, &args, &context(Some(0x00FF_FFFF))), Action::Declined);
    assert_eq!(run(&world, CMD_ADD_ITEM, &args, &context(None)), Action::Declined, "a quest has no inventory");
    assert_eq!(getter(&world, CMD_GET_ITEM_COUNT, &refs(&[ITEM]), &context(Some(0x00FF_FFFF))), Action::Declined);
    // A null item names nothing.
    assert_eq!(run(&world, CMD_ADD_ITEM, &[ObScriptValue::Ref(0), ObScriptValue::Num(1.0)], &context(Some(ACTOR))), Action::Declined);
}

#[test]
fn lock_and_unlock_set_the_lock_and_lock_zero_keeps_the_level() {
    let (world, actor, _) = world();
    assert!(done(&run(&world, CMD_LOCK, &[], &context(Some(ACTOR)))));
    assert_eq!(getter(&world, CMD_GET_LOCKED, &[], &context(Some(ACTOR))), Action::number(1.0));
    assert!(done(&run(&world, CMD_LOCK, &[ObScriptValue::Num(75.0)], &context(Some(ACTOR)))));
    assert_eq!(world.get::<Locked>(actor).map(|lock| lock.lock_level), Some(75));
    assert_eq!(getter(&world, CMD_GET_LOCK_LEVEL, &[], &context(Some(ACTOR))), Action::number(75.0));
    assert!(done(&run(&world, CMD_UNLOCK, &[], &context(Some(ACTOR)))));
    assert_eq!(getter(&world, CMD_GET_LOCKED, &[], &context(Some(ACTOR))), Action::number(0.0));
    assert_eq!(getter(&world, CMD_GET_LOCK_LEVEL, &[], &context(Some(ACTOR))), Action::number(0.0));
}

#[test]
fn actor_value_commands_translate_the_code_and_set_mod_damage_and_restore() {
    let (world, actor, _) = world();
    world.query_mut::<ActorValues>().unwrap().insert(actor, ActorValues::new());
    let ctx = context(Some(ACTOR));
    let av = |code: f32, value: f32| [ObScriptValue::Num(code), ObScriptValue::Num(value)];
    // SetAV Strength 6, ModAV Strength 2, DamageAV Strength 3, RestoreAV Strength 1.
    assert!(done(&run(&world, CMD_SET_AV, &av(5.0, 6.0), &ctx)));
    assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(5.0)], &ctx), Action::number(6.0));
    assert!(done(&run(&world, CMD_MOD_AV, &av(5.0, 2.0), &ctx)));
    assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(5.0)], &ctx), Action::number(8.0));
    assert!(done(&run(&world, CMD_DAMAGE_AV, &av(5.0, 3.0), &ctx)));
    assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(5.0)], &ctx), Action::number(5.0));
    assert!(done(&run(&world, CMD_RESTORE_AV, &av(5.0, 1.0), &ctx)));
    assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(5.0)], &ctx), Action::number(6.0));
    // GetBaseAV is the base alone: the +2 modifier and the damage are not in it.
    assert_eq!(getter(&world, CMD_GET_BASE_AV, &[ObScriptValue::Num(5.0)], &ctx), Action::number(6.0));
    // ForceAV 10 brings the current value to 10 through a permanent modifier.
    assert!(done(&run(&world, CMD_FORCE_AV, &av(5.0, 10.0), &ctx)));
    assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(5.0)], &ctx), Action::number(10.0));
    // ResetHealth heals the damage on Health.
    assert!(done(&run(&world, CMD_SET_AV, &av(16.0, 50.0), &ctx)));
    assert!(done(&run(&world, CMD_DAMAGE_AV, &av(16.0, 20.0), &ctx)));
    assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(16.0)], &ctx), Action::number(30.0));
    assert!(done(&run(&world, CMD_RESET_HEALTH, &[], &ctx)));
    assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(16.0)], &ctx), Action::number(50.0));
}

#[test]
fn an_actor_value_code_with_no_avif_is_declined_not_invented() {
    let (world, _, _) = world();
    let ctx = context(Some(ACTOR));
    // 21 = SpeedMult has no AVIF; 99 is not a code.
    for code in [21.0, 99.0] {
        assert_eq!(run(&world, CMD_SET_AV, &[ObScriptValue::Num(code), ObScriptValue::Num(1.0)], &ctx), Action::Declined);
        assert_eq!(getter(&world, CMD_GET_AV, &[ObScriptValue::Num(code)], &ctx), Action::Declined);
    }
}

#[test]
fn unconscious_and_ghost_set_the_actor_control_state() {
    let (world, actor, _) = world();
    let ctx = context(Some(ACTOR));
    assert!(done(&run(&world, CMD_SET_UNCONSCIOUS, &[ObScriptValue::Num(1.0)], &ctx)));
    assert!(world.get::<crate::ActorControlState>(actor).is_some_and(|state| state.unconscious));
    assert_eq!(getter(&world, CMD_GET_UNCONSCIOUS, &[], &ctx), Action::number(1.0));
    assert!(done(&run(&world, CMD_SET_UNCONSCIOUS, &[ObScriptValue::Num(0.0)], &ctx)));
    assert_eq!(getter(&world, CMD_GET_UNCONSCIOUS, &[], &ctx), Action::number(0.0));
    assert!(done(&run(&world, CMD_SET_GHOST, &[ObScriptValue::Num(1.0)], &ctx)));
    assert!(world.get::<crate::ActorControlState>(actor).is_some_and(|state| state.ghost));
}

#[test]
fn get_is_reference_compares_form_ids() {
    let (world, _, _) = world();
    let ctx = context(Some(ACTOR));
    assert_eq!(getter(&world, CMD_GET_IS_REFERENCE, &refs(&[ACTOR]), &ctx), Action::number(1.0));
    assert_eq!(getter(&world, CMD_GET_IS_REFERENCE, &refs(&[OTHER]), &ctx), Action::number(0.0));
    assert_eq!(getter(&world, CMD_GET_IS_REFERENCE, &refs(&[0]), &ctx), Action::Declined, "a null ref is not asked about");
}

#[test]
fn stop_combat_with_a_target_stops_only_that_fight() {
    let (mut world, actor, other) = world();
    world.insert(actor, crate::AiCombatState { target: other, attack_cooldown_remaining: 0.0 });
    let ctx = context(Some(ACTOR));
    // Fighting OTHER: `StopCombat <some third ref>` leaves it alone.
    assert!(done(&run(&world, CMD_STOP_COMBAT, &refs(&[0x00CC_0001]), &ctx)));
    assert!(world.get::<crate::AiCombatState>(actor).is_some());
    assert_eq!(getter(&world, CMD_IS_IN_COMBAT, &[], &ctx), Action::number(1.0));
    assert_eq!(getter(&world, CMD_GET_COMBAT_TARGET, &[], &ctx), Action::reference(OTHER));
    assert!(done(&run(&world, CMD_STOP_COMBAT, &refs(&[OTHER]), &ctx)));
    assert!(world.get::<crate::AiCombatState>(actor).is_none());
    assert_eq!(getter(&world, CMD_IS_IN_COMBAT, &[], &ctx), Action::number(0.0));
    assert_eq!(getter(&world, CMD_GET_COMBAT_TARGET, &[], &ctx), Action::reference(0));
}

#[test]
fn equip_item_with_the_no_unequip_flag_is_declined() {
    let (world, _, _) = world();
    let args = [ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0)];
    assert_eq!(run(&world, CMD_EQUIP_ITEM, &args, &context(Some(ACTOR))), Action::Declined);
}

fn av_world_with_player() -> (World, EntityId, EntityId) {
    let (mut world, actor, _other) = world();
    let player = world.spawn();
    world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
    (world, actor, player)
}

#[test]
fn add_perk_and_remove_perk_change_the_actors_perks() {
    const PERK: u32 = 0x0003_0001;
    let (mut world, actor, _) = world();
    world.register::<byroredux_core::character::Perks>();
    let ctx = context(Some(ACTOR));
    assert!(done(&run(&world, CMD_ADD_PERK, &refs(&[PERK]), &ctx)));
    assert_eq!(world.get::<byroredux_core::character::Perks>(actor).map(|perks| perks.rank(PERK)), Some(1));
    assert!(done(&run(&world, CMD_REMOVE_PERK, &refs(&[PERK]), &ctx)));
    assert_eq!(world.get::<byroredux_core::character::Perks>(actor).map(|perks| perks.rank(PERK)), Some(0));
}

#[test]
fn destroyed_and_restrained_are_set_and_read_back() {
    let (world, actor, _) = world();
    let ctx = context(Some(ACTOR));
    assert_eq!(getter(&world, CMD_GET_DESTROYED, &[], &ctx), Action::number(0.0));
    assert!(done(&run(&world, CMD_SET_DESTROYED, &[ObScriptValue::Num(1.0)], &ctx)));
    assert_eq!(getter(&world, CMD_GET_DESTROYED, &[], &ctx), Action::number(1.0));
    assert!(done(&run(&world, CMD_SET_DESTROYED, &[ObScriptValue::Num(0.0)], &ctx)));
    assert_eq!(getter(&world, CMD_GET_DESTROYED, &[], &ctx), Action::number(0.0));
    assert!(done(&run(&world, CMD_SET_RESTRAINED, &[ObScriptValue::Num(1.0)], &ctx)));
    assert!(world.get::<crate::ActorControlState>(actor).is_some_and(|state| state.restrained));
    assert_eq!(getter(&world, CMD_GET_RESTRAINED, &[], &ctx), Action::number(1.0));
}

#[test]
fn remove_all_typed_items_takes_only_the_type_and_spares_the_exempt_list() {
    use byroredux_plugin::esm::records::{EsmIndex, ItemKind, ItemRecord};
    const OTHER_ITEM: u32 = ITEM + 1;
    let (mut world, actor, other) = world();
    let mut index = EsmIndex::default();
    for (form_id, kind) in [(ITEM, ItemKind::Misc), (OTHER_ITEM, ItemKind::Key)] {
        index.items.insert(form_id, ItemRecord { form_id, common: Default::default(), kind });
    }
    world.insert_resource(crate::obscript_state::ItemTypeCodes::from_index(&index));
    let ctx = context(Some(ACTOR));
    run(&world, CMD_ADD_ITEM, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(4.0)], &ctx);
    run(&world, CMD_ADD_ITEM, &[ObScriptValue::Ref(OTHER_ITEM), ObScriptValue::Num(2.0)], &ctx);
    // `RemoveAllTypedItems <other> 0 0 31`: Misc goes to the target, the Key stays.
    let args = [ObScriptValue::Ref(OTHER), ObScriptValue::Num(0.0), ObScriptValue::Num(0.0), ObScriptValue::Num(31.0)];
    assert!(done(&run(&world, CMD_REMOVE_ALL_TYPED_ITEMS, &args, &ctx)));
    assert_eq!((count(&world, actor, ITEM), count(&world, other, ITEM)), (0, 4));
    assert_eq!(count(&world, actor, OTHER_ITEM), 2);
    // No type code: declined, as the GECK says the result is undefined.
    assert!(matches!(run(&world, CMD_REMOVE_ALL_TYPED_ITEMS, &refs(&[OTHER]), &ctx), Action::Declined));
}

#[test]
fn remove_all_items_empties_the_inventory_or_moves_it() {
    let (world, actor, other) = world();
    let ctx = context(Some(ACTOR));
    run(&world, CMD_ADD_ITEM, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(4.0)], &ctx);
    // Moved to OTHER.
    assert!(done(&run(&world, CMD_REMOVE_ALL_ITEMS, &refs(&[OTHER]), &ctx)));
    assert_eq!((count(&world, actor, ITEM), count(&world, other, ITEM)), (0, 4));
    // Destroyed.
    assert!(done(&run(&world, CMD_REMOVE_ALL_ITEMS, &[], &context(Some(OTHER)))));
    assert_eq!(count(&world, other, ITEM), 0);
}

#[test]
fn place_at_me_creates_a_reference_and_answers_it() {
    let (mut world, _, _) = world();
    world.insert_resource(crate::CreatedReferences::default());
    let ctx = context(Some(ACTOR));
    let Action::Done { value: Some(ObScriptValue::Ref(created)), .. } = run(&world, CMD_PLACE_AT_ME, &refs(&[ITEM]), &ctx) else {
        panic!("PlaceAtMe answers the reference it made");
    };
    assert_ne!(created, 0);
    let created_refs = world.try_resource::<crate::CreatedReferences>().unwrap();
    assert!(created_refs.iter().any(|record| record.form_id == created && record.base == ITEM && record.at_reference == ACTOR));
    drop(created_refs);
    // An offset is not modelled: declined, not dropped.
    let offset = [ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0), ObScriptValue::Num(64.0)];
    assert_eq!(run(&world, CMD_PLACE_AT_ME, &offset, &ctx), Action::Declined);
}

#[test]
fn mark_for_delete_works_only_on_a_disabled_reference() {
    use crate::obscript_objects::ObScriptInstance;
    let (mut world, actor, _) = world();
    world.insert_resource(crate::ReferenceEnableState::default());
    world.insert_resource(crate::CreatedReferences::default());
    world.register::<ObScriptInstance>();
    world.insert(actor, ObScriptInstance { script: 1 });
    let ctx = context(Some(ACTOR));
    // Enabled: "It will not work on enabled references" — nothing happens.
    assert!(done(&run(&world, CMD_MARK_FOR_DELETE, &[], &ctx)));
    assert!(world.has::<ObScriptInstance>(actor), "an enabled reference keeps its script");
    // Disabled and authored: "the script on such objects is removed and no longer runs".
    world.try_resource_mut::<crate::ReferenceEnableState>().unwrap().set_enabled(ACTOR, false);
    assert!(done(&run(&world, CMD_MARK_FOR_DELETE, &[], &ctx)));
    assert!(!world.has::<ObScriptInstance>(actor));
    // Disabled and created by a script: it is deleted once its cell goes.
    world.try_resource_mut::<crate::CreatedReferences>().unwrap().create_scripted(OTHER, ITEM, ACTOR, false);
    world.try_resource_mut::<crate::ReferenceEnableState>().unwrap().set_enabled(OTHER, false);
    assert!(done(&run(&world, CMD_MARK_FOR_DELETE, &[], &context(Some(OTHER)))));
    let created = world.try_resource::<crate::CreatedReferences>().unwrap();
    let record = created.iter().find(|record| record.form_id == OTHER).unwrap();
    assert_eq!(record.deletion, crate::CreatedDeletion::WhenAble);
}

#[test]
fn player_controls_assign_the_domains_the_flags_name() {
    let (mut world, _, _) = av_world_with_player();
    world.insert_resource(crate::PlayerControlState::default());
    let ctx = context(None);
    // DisablePlayerControls with its defaults: Movement, PipBoy, Fighting, POV.
    assert!(done(&run(&world, CMD_DISABLE_PLAYER_CONTROLS, &[], &ctx)));
    let state = *world.try_resource::<crate::PlayerControlState>().unwrap();
    assert!(!state.movement_enabled && !state.menu_enabled && !state.fighting_enabled && !state.camera_switching_enabled);
    assert!(state.looking_enabled && state.sneaking_enabled, "Looking and Sneaking are not in the default");
    assert!(!state.activation_enabled, "Movement takes world activation with it");
    // `EnablePlayerControls 0 0 1`: only Fighting comes back.
    let flags = [0.0, 0.0, 1.0].map(ObScriptValue::Num);
    assert!(done(&run(&world, CMD_ENABLE_PLAYER_CONTROLS, &flags, &ctx)));
    let state = *world.try_resource::<crate::PlayerControlState>().unwrap();
    assert!(state.fighting_enabled && !state.movement_enabled);
}

#[test]
fn get_in_same_cell_compares_the_two_references_cells() {
    use byroredux_core::ecs::components::CellFormId;
    let (mut world, actor, other) = world();
    world.register::<CellFormId>();
    let ctx = context(Some(ACTOR));
    assert_eq!(getter(&world, CMD_GET_IN_SAME_CELL, &refs(&[OTHER]), &ctx), Action::Declined, "no cell is known yet");
    world.insert(actor, CellFormId(0x5A));
    world.insert(other, CellFormId(0x5A));
    assert_eq!(getter(&world, CMD_GET_IN_SAME_CELL, &refs(&[OTHER]), &ctx), Action::number(1.0));
    world.insert(other, CellFormId(0x5B));
    assert_eq!(getter(&world, CMD_GET_IN_SAME_CELL, &refs(&[OTHER]), &ctx), Action::number(0.0));
}

#[test]
fn get_linked_ref_and_get_parent_ref_answer_the_linked_and_parent_references() {
    let (mut world, _, _) = world();
    let ctx = context(Some(ACTOR));
    let mut links = crate::LinkedRefOverrides::default();
    links.set(ACTOR, 0, OTHER);
    world.insert_resource(links);
    assert_eq!(getter(&world, CMD_GET_LINKED_REF, &[], &ctx), Action::reference(OTHER));
    assert_eq!(getter(&world, CMD_GET_PARENT_REF, &[], &ctx), Action::reference(0), "no enable parent");
    world.insert_resource(crate::AuthoredDisabledReferences {
        initially_disabled: Default::default(),
        enable_parents: [(
            ACTOR,
            byroredux_plugin::esm::cell::EnableParent { form_id: ITEM, inverted: false },
        )]
        .into(),
    });
    assert_eq!(getter(&world, CMD_GET_PARENT_REF, &[], &ctx), Action::reference(ITEM));
}

#[test]
fn set_ownership_names_the_owner_and_no_argument_is_the_player() {
    const FACTION: u32 = 0x0006_0001;
    let (mut world, _, _) = world();
    world.insert_resource(crate::OwnerOverrides::default());
    let ctx = context(Some(ACTOR));
    assert!(done(&run(&world, CMD_SET_OWNERSHIP, &refs(&[FACTION]), &ctx)));
    assert_eq!(world.try_resource::<crate::OwnerOverrides>().unwrap().get(ACTOR), Some(FACTION));
    // "If the parameter is omitted, the reference will be set to be owned by the player."
    assert!(done(&run(&world, CMD_SET_OWNERSHIP, &[], &ctx)));
    assert_eq!(world.try_resource::<crate::OwnerOverrides>().unwrap().get(ACTOR), Some(0));
}

#[test]
fn get_is_current_package_reads_the_package_the_ai_reported() {
    const WALK: u32 = 0x0005_0001;
    let (world, actor, _) = world();
    let ctx = context(Some(ACTOR));
    assert_eq!(getter(&world, CMD_GET_IS_CURRENT_PACKAGE, &refs(&[WALK]), &ctx), Action::number(0.0));
    crate::fragment::report_package_transition(&world, actor, None, Some(WALK));
    assert_eq!(getter(&world, CMD_GET_IS_CURRENT_PACKAGE, &refs(&[WALK]), &ctx), Action::number(1.0));
    assert_eq!(getter(&world, CMD_GET_IS_CURRENT_PACKAGE, &refs(&[WALK + 1]), &ctx), Action::number(0.0));
}

#[test]
fn position_and_scale_are_read_and_written_in_the_z_up_world_axes() {
    use byroredux_core::ecs::components::{GlobalTransform, Transform};
    use byroredux_core::math::Vec3;
    let (mut world, actor, _) = world();
    world.register::<Transform>();
    world.register::<GlobalTransform>();
    world.register::<crate::EditorLocation>();
    // Y-up (1, 3, -2) is Z-up (x 1, y 2, z 3).
    world.insert(actor, Transform { translation: Vec3::new(1.0, 3.0, -2.0), rotation: Default::default(), scale: 1.0 });
    world.insert(actor, GlobalTransform { translation: Vec3::new(1.0, 3.0, -2.0), ..Default::default() });
    world.insert(actor, crate::EditorLocation { translation: Vec3::new(5.0, 6.0, -7.0), rotation: Default::default() });
    let ctx = context(Some(ACTOR));
    let axis = |a: f32| [ObScriptValue::Num(a)];
    assert_eq!(getter(&world, CMD_GET_POS, &axis(0.0), &ctx), Action::number(1.0));
    assert_eq!(getter(&world, CMD_GET_POS, &axis(1.0), &ctx), Action::number(2.0));
    assert_eq!(getter(&world, CMD_GET_POS, &axis(2.0), &ctx), Action::number(3.0));
    assert_eq!(getter(&world, CMD_GET_STARTING_POS, &axis(2.0), &ctx), Action::number(6.0));
    assert_eq!(getter(&world, CMD_GET_POS, &axis(3.0), &ctx), Action::Declined, "not an axis");
    // `SetPos z 10`: only the world Z moves.
    assert!(done(&run(&world, CMD_SET_POS, &[ObScriptValue::Num(2.0), ObScriptValue::Num(10.0)], &ctx)));
    assert_eq!(world.get::<Transform>(actor).unwrap().translation, Vec3::new(1.0, 10.0, -2.0));
    assert_eq!(getter(&world, CMD_GET_SCALE, &[], &ctx), Action::number(1.0));
    assert!(done(&run(&world, CMD_SET_SCALE, &[ObScriptValue::Num(0.5)], &ctx)));
    assert_eq!(getter(&world, CMD_GET_SCALE, &[], &ctx), Action::number(0.5));
}

#[test]
fn play_group_starts_the_objects_sequence_and_is_anim_playing_reads_it_back() {
    use byroredux_core::animation::{AnimationClip, AnimationClipRegistry, AnimationPlayer, CycleType, NamedSequences};
    let (mut world, actor, _) = world();
    let clip = |name: &str| AnimationClip {
        name: name.to_owned(),
        duration: 2.0,
        cycle_type: CycleType::Loop,
        frequency: 1.0,
        phase: 0.0,
        weight: 1.0,
        accum_root_name: None,
        channels: Default::default(),
        float_channels: Vec::new(),
        color_channels: Vec::new(),
        bool_channels: Vec::new(),
        texture_flip_channels: Vec::new(),
        text_keys: Vec::new(),
        root_yaw: Vec::new(),
    };
    let mut registry = AnimationClipRegistry::new();
    let forward = registry.add(clip("Forward"));
    world.insert_resource(registry);
    world.register::<NamedSequences>();
    world.insert(actor, NamedSequences(std::sync::Arc::from(vec![("forward".into(), forward)])));
    let player = world.spawn();
    let mut idle = AnimationPlayer::new(forward);
    idle.root_entity = Some(actor);
    idle.playing = false;
    world.insert(player, idle);
    let ctx = context(Some(ACTOR));
    let group = |index: f32| [ObScriptValue::Num(index), ObScriptValue::Num(0.0)];
    assert_eq!(getter(&world, CMD_IS_ANIM_PLAYING, &[], &ctx), Action::number(0.0));
    // `PlayGroup Forward 0`; `Backward` is a group the object has no sequence of.
    assert!(done(&run(&world, CMD_PLAY_GROUP, &group(3.0), &ctx)));
    assert!(matches!(run(&world, CMD_PLAY_GROUP, &group(4.0), &ctx), Action::Declined));
    assert_eq!(getter(&world, CMD_IS_ANIM_PLAYING, &[], &ctx), Action::number(1.0));
    assert_eq!(getter(&world, CMD_IS_ANIM_PLAYING, &[ObScriptValue::Num(3.0)], &ctx), Action::number(1.0));
    assert_eq!(getter(&world, CMD_IS_ANIM_PLAYING, &[ObScriptValue::Num(4.0)], &ctx), Action::number(0.0));
}

#[test]
fn interior_and_worldspace_getters_follow_the_cell_kind() {
    use crate::cell_kind::{CellKind, ExteriorCell, PlayerCellKind};
    use byroredux_core::ecs::components::{CellFormId, CellRoot};
    const WRLD: u32 = 0x000A_0001;
    let (mut world, actor, other) = world();
    world.register::<ExteriorCell>();
    world.register::<CellRoot>();
    world.register::<CellFormId>();
    world.insert_resource(PlayerCellKind(Some(CellKind::Interior)));
    let interior = world.spawn();
    world.insert(interior, CellFormId(1));
    let exterior = world.spawn();
    world.insert(exterior, CellFormId(2));
    world.insert(exterior, ExteriorCell(WRLD));
    world.insert(actor, CellRoot(interior));
    world.insert(other, CellRoot(exterior));
    let indoors = context(Some(ACTOR));
    let outdoors = context(Some(OTHER));
    assert_eq!(getter(&world, CMD_IS_IN_INTERIOR, &[], &indoors), Action::number(1.0));
    assert_eq!(getter(&world, CMD_IS_IN_INTERIOR, &[], &outdoors), Action::number(0.0));
    let wrld = refs(&[WRLD]);
    assert_eq!(getter(&world, CMD_GET_IN_WORLDSPACE, &wrld, &outdoors), Action::number(1.0));
    assert_eq!(getter(&world, CMD_GET_IN_WORLDSPACE, &refs(&[WRLD + 1]), &outdoors), Action::number(0.0));
    // Always 0 in an interior.
    assert_eq!(getter(&world, CMD_GET_IN_WORLDSPACE, &wrld, &indoors), Action::number(0.0));
}

#[test]
fn set_open_state_gives_a_door_a_state_get_open_state_reads_back() {
    use crate::vm_state::TwoStateActivator;
    let (mut world, _, _) = world();
    world.register::<TwoStateActivator>();
    let ctx = context(Some(ACTOR));
    assert_eq!(getter(&world, CMD_GET_OPEN_STATE, &[], &ctx), Action::number(0.0), "no state, no open/closed");
    assert!(done(&run(&world, CMD_SET_OPEN_STATE, &[ObScriptValue::Num(1.0)], &ctx)));
    assert_eq!(getter(&world, CMD_GET_OPEN_STATE, &[], &ctx), Action::number(1.0));
    assert!(done(&run(&world, CMD_SET_OPEN_STATE, &[ObScriptValue::Num(0.0)], &ctx)));
    assert_eq!(getter(&world, CMD_GET_OPEN_STATE, &[], &ctx), Action::number(3.0));
}

#[test]
fn play_idle_requests_the_idle_on_the_calling_actor() {
    let (mut world, actor, _) = world();
    world.register::<crate::ActorCinematicState>();
    let ctx = context(Some(ACTOR));
    const IDLE: u32 = 0x000C_1991;
    assert!(done(&run(&world, CMD_PLAY_IDLE, &refs(&[IDLE]), &ctx)));
    let state = world.get::<crate::ActorCinematicState>(actor).unwrap();
    assert_eq!((state.requested_idle_form_id, state.idle_request_serial), (Some(IDLE), 1));
    drop(state);
    // No idle named: declined, not guessed.
    assert!(matches!(run(&world, CMD_PLAY_IDLE, &[], &ctx), Action::Declined));
}

#[test]
fn pick_idle_queues_the_request_for_the_idle_runtime() {
    let (mut world, actor, _) = world();
    world.register::<crate::PickIdleRequest>();
    assert!(done(&run(&world, CMD_PICK_IDLE, &[], &context(Some(ACTOR)))));
    assert!(world.has::<crate::PickIdleRequest>(actor));
}

#[test]
fn notes_deaths_and_actor_flags_read_back_what_the_commands_wrote() {
    use crate::obscript_state::{install, KillRecords};
    const NOTE: u32 = 0x0010_0001;
    const BASE: u32 = 0x0007_7777;
    let (mut world, actor, _) = world();
    install(&mut world);
    let ctx = context(Some(ACTOR));
    // Notes: no reference needed.
    let none = context(None);
    assert_eq!(getter(&world, CMD_GET_HAS_NOTE, &refs(&[NOTE]), &none), Action::number(0.0));
    assert!(done(&run(&world, CMD_ADD_NOTE, &refs(&[NOTE]), &none)));
    assert_eq!(getter(&world, CMD_GET_HAS_NOTE, &refs(&[NOTE]), &none), Action::number(1.0));
    assert!(done(&run(&world, CMD_REMOVE_NOTE, &refs(&[NOTE]), &none)));
    assert_eq!(getter(&world, CMD_GET_HAS_NOTE, &refs(&[NOTE]), &none), Action::number(0.0));
    // Deaths: the count is per base, the killer per victim reference.
    world.resource_mut::<KillRecords>().record(ACTOR, BASE, Some(0x14));
    assert_eq!(getter(&world, CMD_GET_DEAD_COUNT, &refs(&[BASE]), &none), Action::number(1.0));
    assert_eq!(getter(&world, CMD_IS_KILLER, &refs(&[0x14]), &ctx), Action::number(1.0));
    assert_eq!(getter(&world, CMD_IS_KILLER, &refs(&[OTHER]), &ctx), Action::number(0.0));
    // The flags.
    assert_eq!(getter(&world, CMD_IS_WEAPON_OUT, &[], &ctx), Action::number(0.0));
    assert!(done(&run(&world, CMD_SET_ALERT, &[ObScriptValue::Num(1.0)], &ctx)));
    assert_eq!(getter(&world, CMD_IS_WEAPON_OUT, &[], &ctx), Action::number(1.0));
    assert!(done(&run(&world, CMD_SET_ALERT, &[ObScriptValue::Num(0.0)], &ctx)));
    assert_eq!(getter(&world, CMD_IS_WEAPON_OUT, &[], &ctx), Action::number(0.0));
    assert!(done(&run(&world, CMD_IGNORE_CRIME, &[ObScriptValue::Num(1.0)], &ctx)));
    assert!(world.has::<crate::obscript_state::IgnoreCrime>(actor));
    assert_eq!(getter(&world, CMD_GET_IGNORE_CRIME, &[], &ctx), Action::number(1.0));
}

#[test]
fn an_assault_alarm_turns_the_victim_on_the_player_and_get_detected_asks_the_sight_query() {
    use byroredux_core::ecs::components::{FactionRanks, GlobalTransform};
    const FACTION: u32 = 0x000F_0001;
    let (mut world, victim, bystander) = world();
    let player = world.spawn();
    world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
    world.register::<crate::AiCombatState>();
    world.register::<FactionRanks>();
    world.register::<GlobalTransform>();
    world.insert(victim, FactionRanks::from_pairs([(FACTION, 0)]));
    world.insert(bystander, FactionRanks::from_pairs([(FACTION, 0)]));
    let at = |x: f32| GlobalTransform { translation: byroredux_core::math::Vec3::new(x, 0.0, 0.0), ..Default::default() };
    world.insert(player, at(0.0));
    world.insert(victim, at(900.0));
    world.insert(bystander, at(100.0));
    let none = context(None);
    // `SendAssaultAlarm player <faction>`: the faction's member nearest the player fights him.
    assert!(done(&run(&world, CMD_SEND_ASSAULT_ALARM, &[ObScriptValue::Ref(0x14), ObScriptValue::Ref(FACTION)], &none)));
    assert_eq!(world.get::<crate::AiCombatState>(bystander).map(|state| state.target), Some(player));
    assert!(world.get::<crate::AiCombatState>(victim).is_none());
    // Nobody named and no faction: nothing to react.
    assert!(matches!(run(&world, CMD_SEND_ASSAULT_ALARM, &[], &none), Action::Declined));
    // `GetDetected`: the installed sight test decides; with none installed it is declined.
    let ctx = context(Some(ACTOR));
    assert_eq!(getter(&world, CMD_GET_DETECTED, &refs(&[OTHER]), &ctx), Action::Declined);
    world.insert_resource(crate::SightQuery(|_, _, _| true));
    assert_eq!(getter(&world, CMD_GET_DETECTED, &refs(&[OTHER]), &ctx), Action::number(1.0));
    world.insert_resource(crate::SightQuery(|_, _, _| false));
    assert_eq!(getter(&world, CMD_GET_DETECTED, &refs(&[OTHER]), &ctx), Action::number(0.0));
}

#[test]
fn force_weather_records_the_override_and_release_clears_it() {
    use crate::obscript_state::{install, ForcedWeather};
    const WTHR: u32 = 0x000E_0001;
    let (mut world, _, _) = world();
    install(&mut world);
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.weathers.insert(WTHR, Default::default());
    crate::install_weather_classifications(&mut world, &index);
    let none = context(None);
    // An unknown weather is declined, not invented.
    assert!(matches!(run(&world, CMD_FORCE_WEATHER, &refs(&[WTHR + 1]), &none), Action::Declined));
    let args = [ObScriptValue::Ref(WTHR), ObScriptValue::Num(1.0)];
    assert!(done(&run(&world, CMD_SET_WEATHER, &args, &none)));
    assert_eq!(*world.resource::<ForcedWeather>(), ForcedWeather { weather: Some(WTHR), locked: true });
    assert!(done(&run(&world, CMD_RELEASE_WEATHER_OVERRIDE, &[], &none)));
    assert_eq!(*world.resource::<ForcedWeather>(), ForcedWeather::default());
    // `GetIsCurrentWeather` reads the sky's weather.
    world.insert_resource(crate::weather_state::CurrentWeather(Some(WTHR)));
    assert_eq!(getter(&world, CMD_GET_IS_CURRENT_WEATHER, &refs(&[WTHR]), &none), Action::number(1.0));
    assert_eq!(getter(&world, CMD_GET_IS_CURRENT_WEATHER, &refs(&[WTHR + 1]), &none), Action::number(0.0));
}

#[test]
fn trigger_hit_shader_blurs_the_screen_and_fades_out_within_a_second() {
    use crate::cinematic::{image_space_modifier_system, CinematicPresentationState, HitShaderKicks, HIT_SHADER_BLUR_PIXELS};
    let (mut world, _, _) = world();
    world.init_resource::<CinematicPresentationState>();
    world.init_resource::<HitShaderKicks>();
    let none = context(None);
    assert!(done(&run(&world, CMD_TRIGGER_HIT_SHADER, &[ObScriptValue::Num(3.0)], &none)));
    image_space_modifier_system(&world, 0.0);
    let blur = |world: &World| world.resource::<CinematicPresentationState>().image_space_modifier_frame.blur_radius_pixels;
    assert!((blur(&world) - 3.0 * HIT_SHADER_BLUR_PIXELS).abs() < 1e-5, "full strength the frame it is kicked");
    image_space_modifier_system(&world, 0.5);
    assert!((blur(&world) - 1.5 * HIT_SHADER_BLUR_PIXELS).abs() < 1e-5, "half way through");
    image_space_modifier_system(&world, 0.6);
    assert_eq!(blur(&world), 0.0, "gone after a second");
    // A strength past 5 loses the effect rather than blurring ever harder.
    run(&world, CMD_TRIGGER_HIT_SHADER, &[ObScriptValue::Num(50.0)], &none);
    image_space_modifier_system(&world, 0.0);
    assert!((blur(&world) - 5.0 * HIT_SHADER_BLUR_PIXELS).abs() < 1e-5);
    assert!(matches!(run(&world, CMD_TRIGGER_HIT_SHADER, &[ObScriptValue::Num(0.0)], &none), Action::Declined));
}

#[test]
fn cast_dispel_shader_look_and_resurrect_commands_reach_their_runtimes() {
    use crate::magic_effects::{ActiveMagicEffects, CastEffect, ValueEffect};
    use byroredux_core::ecs::components::{ActorValues, Dead};
    use byroredux_plugin::esm::records::{EfshFill, EfshRecord, EsmIndex};
    const SPELL: u32 = 0x0005_0001;
    const MGEF: u32 = 0x0005_0002;
    const SHADER: u32 = 0x0005_0003;
    const HEALTH: u32 = 0x450;
    let (mut world, actor, other) = world();
    crate::register(&mut world);
    world.insert(actor, ActorValues::from_pairs([(HEALTH, 100.0)]));
    // The spell: a 60 s Recover fortify of Health, +25.
    let mut spells = std::collections::HashMap::new();
    spells.insert(
        SPELL,
        crate::magic::CanonicalSpell {
            effects: vec![CastEffect {
                mgef: MGEF,
                script: 0,
                duration: 60.0,
                magnitude: 25.0,
                value: Some(ValueEffect { actor_value: HEALTH, amount: 25.0, recover: true }),
            }],
            ..Default::default()
        },
    );
    world.insert_resource(crate::magic::SpellCatalog::from_canonical(spells));
    let ctx = context(Some(ACTOR));
    // `CastImmediateOnSelf` puts the effect on the caster; `HasMagicEffect` sees it; `Dispel` ends it.
    assert!(done(&run(&world, CMD_CAST_IMMEDIATE_ON_SELF, &refs(&[SPELL]), &ctx)));
    crate::magic_effects::magic_effect_system(&world, 0.0);
    assert_eq!(getter(&world, CMD_HAS_MAGIC_EFFECT, &refs(&[MGEF]), &ctx), Action::number(1.0));
    assert_eq!(world.get::<ActorValues>(actor).unwrap().current(HEALTH), 125.0);
    assert!(done(&run(&world, CMD_DISPEL, &refs(&[SPELL]), &ctx)));
    crate::magic_effects::magic_effect_system(&world, 0.0);
    assert_eq!(world.get::<ActorValues>(actor).unwrap().current(HEALTH), 100.0);
    assert!(world.get::<ActiveMagicEffects>(actor).is_none());
    // `Cast <spell> <target>`: the caster casts on another reference.
    assert!(done(&run(&world, CMD_CAST, &refs(&[SPELL, OTHER]), &ctx)));
    assert!(world.get::<ActiveMagicEffects>(other).is_some());
    // An unknown spell is declined, not invented.
    assert!(matches!(run(&world, CMD_CAST, &refs(&[SPELL + 9]), &ctx), Action::Declined));
    // `ScriptEffectElapsedSeconds` answers only while an effect script runs.
    assert_eq!(getter(&world, CMD_SCRIPT_EFFECT_ELAPSED_SECONDS, &[], &none_ctx()), Action::Declined);
    world.resource_mut::<crate::magic_effects::ScriptEffectRun>().elapsed = Some(1.25);
    assert_eq!(getter(&world, CMD_SCRIPT_EFFECT_ELAPSED_SECONDS, &[], &none_ctx()), Action::number(1.25));

    // Shaders: `PlayMagicShaderVisuals` plays a known shader, `StopMagicShaderVisuals` stops it.
    let mut index = EsmIndex::default();
    index.effect_shaders.insert(
        SHADER,
        EfshRecord { form_id: SHADER, fill: Some(EfshFill { color: [1, 2, 3], full_alpha_time: 1.0, ..Default::default() }), ..Default::default() },
    );
    world.insert_resource(crate::magic_shaders::ShaderCatalog::from_index(&index));
    assert!(done(&run(&world, CMD_PLAY_MAGIC_SHADER, &[ObScriptValue::Ref(SHADER), ObScriptValue::Num(5.0)], &ctx)));
    assert!(world.get::<crate::magic_shaders::PlayingShaders>(actor).is_some());
    assert!(done(&run(&world, CMD_STOP_MAGIC_SHADER, &refs(&[SHADER]), &ctx)));
    assert!(world.get::<crate::magic_shaders::PlayingShaders>(actor).is_none());
    assert!(matches!(run(&world, CMD_PLAY_MAGIC_SHADER, &refs(&[SHADER + 1]), &ctx), Action::Declined));

    // `Look` / `StopLook`.
    assert!(done(&run(&world, CMD_LOOK, &refs(&[OTHER]), &ctx)));
    assert_eq!(world.get::<crate::obscript_anim::HeadLook>(actor).map(|look| look.target), Some(other));
    assert!(done(&run(&world, CMD_STOP_LOOK, &[], &ctx)));
    assert!(world.get::<crate::obscript_anim::HeadLook>(actor).is_none());

    // `Resurrect` queues the revival of a dead actor and leaves a living one alone.
    assert!(done(&run(&world, CMD_RESURRECT, &[ObScriptValue::Num(0.0)], &ctx)));
    assert!(!world.has::<crate::obscript_state::ReviveRequest>(actor));
    world.register::<Dead>();
    world.insert(actor, Dead);
    assert!(done(&run(&world, CMD_RESURRECT, &[ObScriptValue::Num(1.0)], &ctx)));
    assert!(world.has::<crate::obscript_state::ReviveRequest>(actor));
}

fn none_ctx() -> Context {
    context(None)
}

#[test]
fn game_settings_sneaking_swimming_and_detection_level_answer_from_their_sources() {
    let (mut world, _, _) = world();
    let ctx = context(Some(ACTOR));
    // `GetGS`: the VM looks the name up and hands the value (or the null argument).
    assert_eq!(getter(&world, CMD_GET_GS, &[ObScriptValue::Num(2.5)], &context(None)), Action::number(2.5));
    assert_eq!(getter(&world, CMD_GET_GS, &[ObScriptValue::Ref(0)], &context(None)), Action::Declined);
    // The engine has no sneak stance.
    assert_eq!(getter(&world, CMD_IS_SNEAKING, &[], &ctx), Action::number(0.0));
    // Swimming and sight come from the queries the engine installs.
    assert_eq!(getter(&world, CMD_IS_SWIMMING, &[], &ctx), Action::Declined);
    world.insert_resource(crate::SwimQuery(|_, _| true));
    assert_eq!(getter(&world, CMD_IS_SWIMMING, &[], &ctx), Action::number(1.0));
    world.insert_resource(crate::SightQuery(|_, _, _| true));
    assert_eq!(getter(&world, CMD_GET_DETECTION_LEVEL, &refs(&[OTHER]), &ctx), Action::number(3.0));
    world.insert_resource(crate::SightQuery(|_, _, _| false));
    assert_eq!(getter(&world, CMD_GET_DETECTION_LEVEL, &refs(&[OTHER]), &ctx), Action::number(0.0));
    // `ShowWarning` has already been shown by the VM when the command arrives.
    assert!(done(&run(&world, CMD_SHOW_WARNING, &[ObScriptValue::Num(0.0)], &context(None))));
}

#[test]
fn get_sitting_answers_three_for_a_seated_actor_and_autosave_needs_no_caller() {
    use byroredux_core::ecs::components::{Seated, SeatedAnimationRestore};
    let (mut world, actor, other) = world();
    world.register::<Seated>();
    let ctx = context(Some(ACTOR));
    assert_eq!(getter(&world, CMD_GET_SITTING, &[], &ctx), Action::number(0.0));
    let restore = SeatedAnimationRestore { clip_handle: 0, local_time: 0.0, prev_time: 0.0, playing: true, speed: 1.0 };
    world.insert(actor, Seated { furniture: other, animation_restore: restore });
    assert_eq!(getter(&world, CMD_GET_SITTING, &[], &ctx), Action::number(3.0));
    // `AutoSave` names no reference.
    assert!(done(&run(&world, CMD_AUTO_SAVE, &[], &context(None))));
    assert!(done(&run(&world, CMD_FORCE_SAVE, &[], &context(None))));
    assert!(done(&run(&world, CMD_SYSTEM_SAVE, &[], &context(None))));
}

#[test]
fn angles_are_the_euler_triple_a_placement_stores_in_degrees() {
    use byroredux_core::ecs::components::{GlobalTransform, Transform};
    let (mut world, actor, _) = world();
    world.register::<Transform>();
    world.register::<GlobalTransform>();
    world.register::<crate::EditorLocation>();
    let at = |rotation| Transform { translation: Default::default(), rotation, scale: 1.0 };
    world.insert(actor, at(Default::default()));
    world.insert(actor, GlobalTransform::default());
    let ctx = context(Some(ACTOR));
    let sync = |world: &World| {
        let rotation = world.get::<Transform>(actor).unwrap().rotation;
        world.query_mut::<GlobalTransform>().unwrap().get_mut(actor).unwrap().rotation = rotation;
    };
    let angle = |world: &World, axis: f32| match getter(world, CMD_GET_ANGLE, &[ObScriptValue::Num(axis)], &ctx) {
        Action::Done { value: Some(ObScriptValue::Num(v)), .. } => v,
        other => panic!("{other:?}"),
    };
    // `SetAngle z 270`, then `SetAngle x 28.8`: each keeps the other component.
    assert!(done(&run(&world, CMD_SET_ANGLE, &[ObScriptValue::Num(2.0), ObScriptValue::Num(270.0)], &ctx)));
    sync(&world);
    assert!((angle(&world, 2.0) - 270.0).abs() < 1e-2, "a heading, in [0, 360)");
    assert!(done(&run(&world, CMD_SET_ANGLE, &[ObScriptValue::Num(0.0), ObScriptValue::Num(28.8)], &ctx)));
    sync(&world);
    assert!((angle(&world, 0.0) - 28.8).abs() < 1e-2 && (angle(&world, 2.0) - 270.0).abs() < 1e-2);
    // `Rotate z 90` over a 0.5 s frame turns the heading 45 degrees.
    assert!(done(&run(&world, CMD_ROTATE, &[ObScriptValue::Num(2.0), ObScriptValue::Num(90.0)], &ctx)));
    sync(&world);
    assert!((angle(&world, 2.0) - 315.0).abs() < 1e-1, "{}", angle(&world, 2.0));
    // A negative heading reads in [0, 360).
    run(&world, CMD_SET_ANGLE, &[ObScriptValue::Num(2.0), ObScriptValue::Num(-90.0)], &ctx);
    sync(&world);
    assert!((angle(&world, 2.0) - 270.0).abs() < 1e-2);
    // The player's pitch and heading are the camera's: not asked of the body.
    world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(actor));
    assert_eq!(getter(&world, CMD_GET_ANGLE, &[ObScriptValue::Num(2.0)], &ctx), Action::Declined);
    assert_eq!(run(&world, CMD_SET_ANGLE, &[ObScriptValue::Num(2.0), ObScriptValue::Num(1.0)], &ctx), Action::Declined);
}

#[test]
fn image_space_modifiers_play_stop_and_report_through_the_cinematic_runtime() {
    use byroredux_plugin::esm::records::ImadRecord;
    const IMAD: u32 = 0x000A_0001;
    let (mut world, _, _) = world();
    let record = ImadRecord { form_id: IMAD, duration_seconds: 2.0, ..Default::default() };
    crate::cinematic::install_image_space_modifiers(&mut world, vec![record]);
    let ctx = context(None);
    let active = |world: &World| getter(world, CMD_IS_IMAGE_SPACE_ACTIVE, &refs(&[IMAD]), &ctx);
    assert_eq!(active(&world), Action::number(0.0));
    assert!(done(&run(&world, CMD_IMOD, &refs(&[IMAD]), &ctx)));
    assert_eq!(active(&world), Action::number(1.0));
    assert!(done(&run(&world, CMD_RIMOD, &refs(&[IMAD]), &ctx)));
    assert_eq!(active(&world), Action::number(0.0));
    // An IMAD the load order lacks is declined, not held.
    assert_eq!(run(&world, CMD_IMOD, &refs(&[IMAD + 1]), &ctx), Action::Declined);
    // The modifier plays out: its duration ends it.
    run(&world, CMD_IMOD, &refs(&[IMAD]), &ctx);
    crate::cinematic::image_space_modifier_system(&world, 3.0);
    assert_eq!(active(&world), Action::number(0.0));
}

#[test]
fn get_health_percentage_is_current_over_the_undamaged_ceiling() {
    use byroredux_core::ecs::components::ActorVitals;
    let (mut world, actor, _) = world();
    world.register::<ActorVitals>();
    world.insert(actor, ActorVitals { health: HEALTH });
    let ctx = context(Some(ACTOR));
    run(&world, CMD_SET_AV, &[ObScriptValue::Num(16.0), ObScriptValue::Num(80.0)], &ctx);
    assert_eq!(getter(&world, CMD_GET_HEALTH_PERCENTAGE, &[], &ctx), Action::number(1.0));
    run(&world, CMD_DAMAGE_AV, &[ObScriptValue::Num(16.0), ObScriptValue::Num(20.0)], &ctx);
    assert_eq!(getter(&world, CMD_GET_HEALTH_PERCENTAGE, &[], &ctx), Action::number(0.75));
}

#[test]
fn unequip_item_releases_the_slot_and_announces_it() {
    use byroredux_core::ecs::components::{EquipmentSlots, InventoryIndex};
    let (mut world, actor, _) = world();
    world.register::<EquipmentSlots>();
    world.register::<crate::EquipmentEventBatch>();
    let ctx = context(Some(ACTOR));
    run(&world, CMD_ADD_ITEM, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0)], &ctx);
    let mut slots = EquipmentSlots::new();
    slots.equip(0b1, InventoryIndex(0));
    world.insert(actor, slots);
    assert!(done(&run(&world, CMD_UNEQUIP_ITEM, &refs(&[ITEM]), &ctx)));
    assert!(!world.get::<EquipmentSlots>(actor).unwrap().is_equipped(InventoryIndex(0)));
    assert_eq!(
        world.get::<crate::EquipmentEventBatch>(actor).map(|batch| batch.0.clone()),
        Some(vec![crate::EquipmentChange { item_form_id: ITEM, equipped: false }])
    );
    // An item the actor does not carry, and the no-equip flag, are declined.
    assert_eq!(run(&world, CMD_UNEQUIP_ITEM, &refs(&[ITEM + 1]), &ctx), Action::Declined);
    assert_eq!(run(&world, CMD_UNEQUIP_ITEM, &[ObScriptValue::Ref(ITEM), ObScriptValue::Num(1.0)], &ctx), Action::Declined);
}

#[test]
fn add_reputation_adds_the_points_the_amount_names_to_the_players_standing() {
    use byroredux_core::character::FactionReputation;
    const REPU: u32 = 0x0009_0001;
    let (mut world, _, player) = av_world_with_player();
    world.register::<FactionReputation>();
    let ctx = context(None);
    let add = |axis: f32, amount: f32| [ObScriptValue::Ref(REPU), ObScriptValue::Num(axis), ObScriptValue::Num(amount)];
    // "AddReputation RepNVGoodsprings 0 5": infamy, the very major bump (12 by default).
    assert!(done(&run(&world, CMD_ADD_REPUTATION, &add(0.0, 5.0), &ctx)));
    assert!(done(&run(&world, CMD_ADD_REPUTATION, &add(1.0, 3.0), &ctx)));
    let standing = world.get::<FactionReputation>(player).unwrap();
    assert_eq!((standing.infamy(REPU), standing.fame(REPU)), (12, 4));
    drop(standing);
    // "it only allows 1-5", and the axis is 0 or 1.
    assert_eq!(run(&world, CMD_ADD_REPUTATION, &add(1.0, 6.0), &ctx), Action::Declined);
    assert_eq!(run(&world, CMD_ADD_REPUTATION, &add(2.0, 3.0), &ctx), Action::Declined);
}

#[test]
fn play_sound_3d_plays_at_the_calling_reference() {
    const SOUND: u32 = 0x0009_0002;
    let (mut world, _, _) = world();
    world.insert_resource(crate::PendingSoundPlays::default());
    assert!(done(&run(&world, CMD_PLAY_SOUND_3D, &[ObScriptValue::Ref(SOUND)], &context(Some(ACTOR)))));
    assert_eq!(world.try_resource::<crate::PendingSoundPlays>().unwrap().0, vec![crate::SoundPlay { sound: SOUND, source: ACTOR }]);
    // It plays "at the location of the calling reference": with none, there is no place.
    assert_eq!(run(&world, CMD_PLAY_SOUND_3D, &[ObScriptValue::Ref(SOUND)], &context(None)), Action::Declined);
}

#[test]
fn start_conversation_with_the_player_asks_for_a_force_greet() {
    const TOPIC: u32 = 0x0009_0003;
    let (world, actor, _) = world();
    let ctx = context(Some(ACTOR));
    assert!(done(&run(&world, CMD_START_CONVERSATION, &refs(&[crate::PLAYER_REFERENCE, TOPIC]), &ctx)));
    assert!(done(&run(&world, CMD_START_CONVERSATION, &refs(&[crate::PLAYER_REFERENCE]), &ctx)));
    let lines = world.try_resource::<crate::obscript_speech::PendingScriptedLines>().unwrap();
    assert_eq!(lines.conversations, vec![(actor, Some(TOPIC)), (actor, None)], "a topic, then the generic greeting");
    drop(lines);
    // A conversation between two NPCs has no runtime.
    assert_eq!(run(&world, CMD_START_CONVERSATION, &refs(&[OTHER, TOPIC]), &ctx), Action::Declined);
}

#[test]
fn show_message_is_a_fallout_command_that_queues_the_message_record() {
    const MESSAGE: u32 = 0x0010_2030;
    let (world, _, _) = world();
    assert!(done(&run(&world, CMD_SHOW_MESSAGE, &[ObScriptValue::Ref(MESSAGE), ObScriptValue::Num(3.0)], &context(None))));
    // A null form names no message; and in Oblivion the id is the text `Message`, not this.
    assert_eq!(run(&world, CMD_SHOW_MESSAGE, &[ObScriptValue::Ref(0)], &context(None)), Action::Declined);
    let mut oblivion = World::new();
    oblivion.insert_resource(crate::ScriptGame(GameKind::Oblivion));
    assert_eq!(run(&oblivion, CMD_SHOW_MESSAGE, &[ObScriptValue::Ref(MESSAGE)], &context(None)), Action::Unhandled);
}

#[test]
fn get_current_time_is_the_hour_of_the_game_clock() {
    let (world, _, _) = world();
    // No clock reported yet: nothing to read, so it is declined, not midnight.
    assert_eq!(getter(&world, CMD_GET_CURRENT_TIME, &[], &context(None)), Action::Declined);
    // Day 1, 06:30 (30.5 game hours since the runtime began): "4:30 am = 4.5".
    crate::script_update_system(&world, 0.0, Some(30.5));
    assert_eq!(getter(&world, CMD_GET_CURRENT_TIME, &[], &context(None)), Action::number(6.5));
}

#[test]
fn a_command_this_module_does_not_run_is_left_for_the_host() {
    let (world, _, _) = world();
    assert_eq!(run(&world, 0x1fff, &[], &context(Some(ACTOR))), Action::Unhandled);
    assert_eq!(getter(&world, 0x1fff, &[], &context(Some(ACTOR))), Action::Unhandled);
}

/// #6478 — these ids are other commands in Oblivion (0x10ab `IsPlayerInJail`, 0x10c0
/// `CloseCurrentOblivionGate`, 0x1078 `PositionWorld`, 0x1079 `PositionCell`, 0x105f `Yield`). A
/// `Games::Any` row answered them with a Fallout command's effect before the Oblivion getter route
/// (which owns `IsPlayerInJail`) was reached, and a faked `Done` for a gate close that never ran.
#[test]
fn fallout_only_opcodes_do_not_shadow_oblivions() {
    let (fallout, _, _) = world();
    let mut oblivion = World::new();
    oblivion.insert_resource(crate::ScriptGame(GameKind::Oblivion));
    for id in [CMD_IGNORE_CRIME, CMD_GET_IGNORE_CRIME, CMD_SET_ENEMY, CMD_SET_ALLY, CMD_SEND_ASSAULT_ALARM] {
        assert_eq!(run(&oblivion, id, &[], &context(Some(ACTOR))), Action::Unhandled, "{id:#06x} must be left to Oblivion's own handling");
        assert!(!matches!(run(&fallout, id, &[], &context(Some(ACTOR))), Action::Unhandled), "{id:#06x} is still a Fallout command");
    }
}
