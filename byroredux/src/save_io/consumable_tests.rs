//! Disk save/load commands followed by the production live-overlay helpers.
//! GPU cell reload is simulated by rebuilding entities in a different order.
use super::*;
use byroredux_core::ecs::components::{
    ActorValues, ActorVitals, EquipmentSlots, FormIdComponent, Inventory, InventoryIndex, ItemStack,
};
use byroredux_core::ecs::resources::{ItemInstance, ItemInstancePool};
use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
use byroredux_core::string::StringPool;
use byroredux_plugin::esm::records::EsmIndex;

fn player(world: &mut World) -> byroredux_core::ecs::EntityId {
    let id = world.resource_mut::<FormIdPool>().intern(FormIdPair {
        plugin: PluginId::from_filename("ConsumableFixture.esm"),
        local: LocalFormId(0x14),
    });
    let entity = world.spawn();
    world.insert(entity, FormIdComponent(id));
    world.insert_resource(crate::systems::PlayerEntity(Some(entity)));
    entity
}

fn world(index: &EsmIndex, directory: PathBuf) -> World {
    let mut world = World::new();
    world.insert_resource(FormIdPool::new());
    world.insert_resource(StringPool::new());
    world.insert_resource(byroredux_core::ecs::resources::HardcoreMode::default());
    world.insert_resource(build_save_registry());
    world.insert_resource(SaveState::new(directory, 4));
    world.insert_resource(PendingSaveLoadSlot::default());
    world.insert_resource(crate::extensions::SessionEventQueue::default());
    world.insert_resource(crate::notifications::PlayerNotifications::default());
    world.insert_resource(crate::cell_loader::CurrentCellContext {
        cell_editor_id: "ConsumableFixtureCell".into(),
        esm_path: "ConsumableFixture.esm".into(),
        masters: vec![],
    });
    crate::inventory::install_catalog(&mut world, index);
    world
}

#[test]
fn ranked_empty_and_absent_perks_replace_live_state_on_disk_load() {
    use byroredux_core::character::Perks;
    let directory = tempfile::tempdir().unwrap();
    let index = EsmIndex::default();
    let mut source = world(&index, directory.path().to_owned());
    let saved_player = player(&mut source);
    let mut owned = Perks::default();
    owned.set_rank(77, 2);
    for slot in 0..3 {
        match slot {
            0 => {
                source.insert(saved_player, owned.clone());
            }
            1 => {
                source.insert(saved_player, Perks::default());
            }
            _ => {
                source.remove::<Perks>(saved_player);
            }
        }
        let output = SaveCommand.execute(&source, &slot.to_string());
        assert!(!command_output_is_failure(&output), "{:?}", output.lines);
    }
    let mut live = world(&index, directory.path().to_owned());
    let unrelated = live.spawn();
    live.insert(unrelated, owned.clone());
    let live_player = player(&mut live);
    let mut outgoing = Perks::default();
    outgoing.set_rank(999, 1);
    live.insert(live_player, outgoing);
    for slot in [0, 1, 0, 2, 0, 2] {
        let output = LoadCommand.execute(&live, &slot.to_string());
        assert!(!command_output_is_failure(&output), "{:?}", output.lines);
        let snapshot = live
            .resource_mut::<PendingSaveLoadSlot>()
            .snapshot
            .take()
            .unwrap();
        let registry = build_save_registry();
        byroredux_save::validate_snapshot_types(&registry, &snapshot).unwrap();
        byroredux_save::restore_resources(&mut live, &registry, &snapshot).unwrap();
        let remap = byroredux_save::build_form_id_remap(&live, &registry, &snapshot);
        assert_eq!(remap.get(&saved_player), Some(&live_player));
        byroredux_save::apply_deltas(
            &mut live,
            &registry,
            &snapshot,
            &remap,
            MUTABLE_DELTA_COLUMNS,
        )
        .unwrap();
        let loaded = live.get::<Perks>(live_player);
        assert_eq!(
            loaded.as_ref().map_or(0, |p| p.rank(77)),
            if slot == 0 { 2 } else { 0 }
        );
        assert_eq!(loaded.as_ref().map_or(0, |p| p.rank(999)), 0);
        assert_eq!(loaded.is_some(), slot != 2);
        assert_eq!(live.get::<Perks>(unrelated).unwrap().rank(77), 2);
    }
}

fn disk_round_trip(index: &EsmIndex, potion: u32, restoration: f32) {
    disk_round_trip_with_perk(index, potion, restoration, None);
}

fn disk_round_trip_with_perk(index: &EsmIndex, potion: u32, restoration: f32, perk: Option<u32>) {
    use byroredux_core::character::Perks;
    let directory = tempfile::tempdir().unwrap();
    let health = index.health_actor_value_key().expect("health mapping");
    let plan = byroredux_plugin::consumables::restoration_plan(index, potion).unwrap();
    let limb_ids: Vec<_> = byroredux_plugin::consumables::BODY_CONDITION_VALUES
        .into_iter()
        .filter_map(|name| index.actor_value_form_id(name))
        .filter(|id| plan.iter().any(|e| e.actor_value == *id))
        .collect();
    let mut source = world(index, directory.path().to_owned());
    let saved_player = player(&mut source);
    if let Some(perk) = perk {
        let mut owned = Perks::default();
        owned.set_rank(perk, 1);
        source.insert(saved_player, owned);
    }
    let mut pool = ItemInstancePool::new();
    let potion_instance = pool.allocate(ItemInstance::default());
    let equipment_instance = pool.allocate(ItemInstance::default());
    source.insert_resource(pool);
    source.insert(
        saved_player,
        Inventory {
            items: vec![
                ItemStack {
                    base_form_id: potion,
                    count: 2,
                    instance: Some(potion_instance),
                },
                ItemStack {
                    base_form_id: 0x1234,
                    count: 1,
                    instance: Some(equipment_instance),
                },
            ],
        },
    );
    let mut equipment = EquipmentSlots::new();
    equipment.equip(1 << 4, InventoryIndex(1));
    source.insert(saved_player, equipment);
    source.insert(saved_player, ActorVitals { health });
    let mut values = ActorValues::from_pairs([(health, 100.0)]);
    seed_restoration_values(&mut values, index);
    for &id in &limb_ids {
        values.apply_damage(id, 80.0);
    }
    values.apply_damage(health, 60.0);
    source.insert(saved_player, values);

    for slot in 0..3 {
        if slot > 0 {
            assert_eq!(
                crate::inventory::apply_action(
                    &mut source,
                    byroredux_debug_ui::InventoryAction::Consume {
                        index: 0,
                        form_id: potion
                    }
                ),
                crate::inventory::MutationResult::Consumed
            );
        }
        let output = SaveCommand.execute(&source, &slot.to_string());
        assert!(!command_output_is_failure(&output), "{:?}", output.lines);
        assert!(output
            .lines
            .iter()
            .any(|line| line.contains(&format!("saved slot {slot}"))));
    }
    assert_eq!(source.resource::<ItemInstancePool>().live_count(), 1);

    let mut live = world(index, directory.path().to_owned());
    {
        live.spawn();
        live.spawn();
        live.spawn();
        let live_player = player(&mut live);
        assert_ne!(live_player, saved_player);
        // Deliberately poison live state; every loaded value must replace it.
        live.insert(live_player, ActorValues::from_pairs([(health, 999.0)]));
        live.insert(live_player, ActorVitals { health });
        live.insert(
            live_player,
            Inventory {
                items: vec![ItemStack::new(potion, 99)],
            },
        );
        live.insert(live_player, EquipmentSlots::new());
        let mut live_pool = ItemInstancePool::new();
        let stale = live_pool.allocate(ItemInstance::default());
        assert_eq!(stale, potion_instance, "force arena-handle collision");
        live.insert_resource(live_pool);
    }

    // Reuse the same world across backward, forward, and repeated loads. Each
    // overlay must replace the state left by the preceding load and use action.
    for slot in [2, 0, 1, 2, 2] {
        let live_player = live.resource::<crate::systems::PlayerEntity>().0.unwrap();
        // Opposite outgoing ownership must not select the wrong effect after
        // load. 0x3131 controls the real FO3 BloodPack's 19-point bonus.
        let mut outgoing = Perks::default();
        if perk.is_none() {
            outgoing.set_rank(0x3131, 1);
        }
        live.insert(live_player, outgoing);
        let output = LoadCommand.execute(&live, &slot.to_string());
        assert!(!command_output_is_failure(&output), "{:?}", output.lines);
        let snapshot = live
            .resource_mut::<PendingSaveLoadSlot>()
            .snapshot
            .take()
            .expect("disk load queued");
        let registry = build_save_registry();
        byroredux_save::validate_snapshot_types(&registry, &snapshot).unwrap();
        byroredux_save::restore_resources(&mut live, &registry, &snapshot).unwrap();
        let remap = byroredux_save::build_form_id_remap(&live, &registry, &snapshot);
        assert_eq!(remap.get(&saved_player), Some(&live_player));
        byroredux_save::apply_deltas(
            &mut live,
            &registry,
            &snapshot,
            &remap,
            MUTABLE_DELTA_COLUMNS,
        )
        .unwrap();
        assert_eq!(
            live.get::<Perks>(live_player)
                .as_ref()
                .map_or(0, |p| p.rank(perk.unwrap_or(0x3131))),
            u8::from(perk.is_some())
        );
        let inventory = live.get::<Inventory>(live_player).unwrap();
        assert_eq!(inventory.items.len(), 2);
        assert_eq!(inventory.items[0].count, 2 - slot);
        assert_eq!(
            inventory.items[0].instance,
            if slot == 2 {
                None
            } else {
                Some(potion_instance)
            }
        );
        assert_eq!(inventory.items[1].instance, Some(equipment_instance));
        drop(inventory);
        assert_eq!(
            live.get::<ActorValues>(live_player)
                .unwrap()
                .current(health),
            (40.0 + slot as f32 * restoration).min(100.0)
        );
        for &id in &limb_ids {
            assert_eq!(
                live.get::<ActorValues>(live_player).unwrap().current(id),
                (20.0 + slot as f32 * restoration).min(100.0)
            );
        }
        assert_eq!(
            live.get::<EquipmentSlots>(live_player).unwrap().at(4),
            Some(InventoryIndex(1))
        );
        assert_eq!(
            live.resource::<ItemInstancePool>().live_count(),
            if slot == 2 { 1 } else { 2 }
        );
        assert_eq!(
            live.resource::<ItemInstancePool>()
                .get(potion_instance)
                .is_some(),
            slot != 2
        );
        assert!(live
            .resource::<ItemInstancePool>()
            .get(equipment_instance)
            .is_some());
        // Loading itself must not re-emit the transient consumption message.
        assert!(crate::notifications::drain(&live).is_empty());
        let result = crate::inventory::apply_action(
            &mut live,
            byroredux_debug_ui::InventoryAction::Consume {
                index: 0,
                form_id: potion,
            },
        );
        assert_eq!(
            result,
            if slot == 2 {
                crate::inventory::MutationResult::Unavailable
            } else {
                crate::inventory::MutationResult::Consumed
            }
        );
        let consumed = slot != 2;
        assert_eq!(
            live.get::<Inventory>(live_player).unwrap().items[0].count,
            2 - slot - u32::from(consumed)
        );
        assert_eq!(
            live.get::<ActorValues>(live_player)
                .unwrap()
                .current(health),
            (40.0 + (slot + u32::from(consumed)) as f32 * restoration).min(100.0)
        );
        assert_eq!(
            crate::notifications::drain(&live).len(),
            usize::from(consumed)
        );
    }
}

fn synthetic_index() -> EsmIndex {
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::{
        AvifRecord, ItemKind, ItemRecord, MagicEffectItem, MgefRecord,
    };
    let mut index = EsmIndex {
        game: GameKind::Skyrim,
        ..Default::default()
    };
    index.actor_values.insert(
        1000,
        AvifRecord {
            form_id: 1000,
            editor_id: "AVHealth".into(),
            ..Default::default()
        },
    );
    index.magic_effects.insert(
        20,
        MgefRecord {
            instant_restoration_av: Some(24),
            ..Default::default()
        },
    );
    index.items.insert(
        10,
        ItemRecord {
            form_id: 10,
            common: Default::default(),
            kind: ItemKind::Aid {
                magic_effects: vec![20],
                addiction_chance: 0.0,
                authored_effects: Some(vec![
                    byroredux_plugin::esm::records::items::ConsumableEffect {
                        effect: MagicEffectItem {
                            effect_form_id: 20,
                            magnitude: 25.0,
                            ..Default::default()
                        },
                        delivery: None,
                        actor_value_cache: None,
                        conditions: Vec::new(),
                        condition_flags: Vec::new(),
                    },
                ]),
                simple_consumption_header: true,
                medicine: false,
                immediate_effects: Some(vec![MagicEffectItem {
                    effect_form_id: 20,
                    magnitude: 25.0,
                    ..Default::default()
                }]),
            },
        },
    );
    index
}

#[test]
fn consumable_health_inventory_and_arena_survive_disk_load_overlay() {
    disk_round_trip(&synthetic_index(), 10, 25.0);
}

fn timed_disk_round_trip(
    index: &EsmIndex,
    potion: u32,
    instant: f32,
    rate: f32,
    duration: f32,
    perk: Option<u32>,
    hardcore: bool,
) {
    use byroredux_core::ecs::components::TimedRestorations;
    let directory = tempfile::tempdir().unwrap();
    let health = index.health_actor_value_key().unwrap();
    let mut source = world(index, directory.path().to_owned());
    source
        .resource_mut::<byroredux_core::ecs::resources::HardcoreMode>()
        .enabled = hardcore;
    let saved_player = player(&mut source);
    source.insert(saved_player, ActorVitals { health });
    let mut values = ActorValues::from_pairs([(health, 1000.0)]);
    seed_restoration_values(&mut values, index);
    values.apply_damage(health, 960.0);
    source.insert(saved_player, values);
    source.insert(
        saved_player,
        Inventory {
            items: vec![ItemStack::new(potion, 2)],
        },
    );
    if let Some(perk) = perk {
        let mut perks = byroredux_core::character::Perks::default();
        perks.set_rank(perk, 1);
        source.insert(saved_player, perks);
    }
    let output = SaveCommand.execute(&source, "0");
    assert!(!command_output_is_failure(&output), "{:?}", output.lines);
    assert_eq!(
        crate::inventory::apply_action(
            &mut source,
            byroredux_debug_ui::InventoryAction::Consume {
                index: 0,
                form_id: potion
            }
        ),
        crate::inventory::MutationResult::Consumed
    );
    assert_eq!(
        source
            .get::<ActorValues>(saved_player)
            .unwrap()
            .current(health),
        40.0 + instant
    );
    crate::systems::restoration::restoration_system(&source, duration * 0.5);
    let midpoint = 40.0 + instant + rate * duration * 0.5;
    assert_eq!(
        source
            .get::<ActorValues>(saved_player)
            .unwrap()
            .current(health),
        midpoint
    );
    let output = SaveCommand.execute(&source, "1");
    assert!(!command_output_is_failure(&output), "{:?}", output.lines);
    let mut live = world(index, directory.path().to_owned());
    let _unrelated = live.spawn();
    let live_player = player(&mut live);
    live.insert(live_player, ActorVitals { health });
    live.insert(live_player, ActorValues::from_pairs([(health, 1000.0)]));
    // Repeat both directions: loading before consumption must remove outgoing
    // timed state; loading halfway through must restore remaining time once.
    for slot in [1, 0, 1, 0] {
        live.resource_mut::<byroredux_core::ecs::resources::HardcoreMode>()
            .enabled = !hardcore;
        let output = LoadCommand.execute(&live, &slot.to_string());
        assert!(!command_output_is_failure(&output), "{:?}", output.lines);
        let snapshot = live
            .resource_mut::<PendingSaveLoadSlot>()
            .snapshot
            .take()
            .unwrap();
        let registry = build_save_registry();
        byroredux_save::validate_snapshot_types(&registry, &snapshot).unwrap();
        byroredux_save::restore_resources(&mut live, &registry, &snapshot).unwrap();
        assert_eq!(
            live.resource::<byroredux_core::ecs::resources::HardcoreMode>()
                .enabled,
            hardcore
        );
        let remap = byroredux_save::build_form_id_remap(&live, &registry, &snapshot);
        assert_eq!(remap.get(&saved_player), Some(&live_player));
        byroredux_save::apply_deltas(
            &mut live,
            &registry,
            &snapshot,
            &remap,
            MUTABLE_DELTA_COLUMNS,
        )
        .unwrap();
        assert_eq!(
            live.get::<Inventory>(live_player).unwrap().items[0].count,
            if slot == 0 { 2 } else { 1 }
        );
        if slot == 1 {
            assert_eq!(
                live.get::<TimedRestorations>(live_player).unwrap().effects[0].remaining,
                f64::from(duration * 0.5)
            );
            assert_eq!(
                live.get::<ActorValues>(live_player)
                    .unwrap()
                    .current(health),
                midpoint
            );
            // Overshoot expiry: only the remaining half may restore health.
            crate::systems::restoration::restoration_system(&live, duration + 1.0);
            assert_eq!(
                live.get::<ActorValues>(live_player)
                    .unwrap()
                    .current(health),
                40.0 + instant + rate * duration
            );
            assert!(live
                .get::<TimedRestorations>(live_player)
                .unwrap()
                .effects
                .is_empty());
            // Leave a new outgoing effect before loading the absent column.
            assert_eq!(
                crate::inventory::apply_action(
                    &mut live,
                    byroredux_debug_ui::InventoryAction::Consume {
                        index: 0,
                        form_id: potion
                    }
                ),
                crate::inventory::MutationResult::Consumed
            );
        } else {
            assert!(live.get::<TimedRestorations>(live_player).is_none());
            crate::systems::restoration::restoration_system(&live, duration);
            assert_eq!(
                live.get::<ActorValues>(live_player)
                    .unwrap()
                    .current(health),
                40.0
            );
        }
    }
}

#[test]
fn timed_consumable_midpoint_and_absence_survive_disk_overlay() {
    let mut index = synthetic_index();
    index
        .magic_effects
        .get_mut(&20)
        .unwrap()
        .timed_restoration_av = Some(24);
    if let byroredux_plugin::esm::records::ItemKind::Aid {
        authored_effects: Some(effects),
        ..
    } = &mut index.items.get_mut(&10).unwrap().kind
    {
        effects[0].effect.duration = 3;
    }
    timed_disk_round_trip(&index, 10, 0.0, 25.0, 3.0, None, false);
}

#[test]
#[ignore = "requires installed Fallout New Vegas master"]
fn real_new_vegas_timed_restoratives_survive_disk_overlay() {
    let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/FalloutNV.esm";
    let index = byroredux_plugin::esm::parse_esm(&std::fs::read(path).unwrap()).unwrap();
    timed_disk_round_trip(&index, 0x1613BD, 0.0, 2.0, 18.0, None, false);
    timed_disk_round_trip(&index, 0x34051, 1.0, 4.0, 5.0, Some(0x3131), false);
    disk_round_trip(&index, 0x34051, 1.0);
}

fn seed_restoration_values(values: &mut ActorValues, index: &EsmIndex) {
    if let Some(medicine) = index.actor_value_form_id("Medicine") {
        values.set_base(medicine, 0.0); // fixture verifies the authored base rate
    }
    for name in byroredux_plugin::consumables::BODY_CONDITION_VALUES {
        if let Some(id) = index.actor_value_form_id(name) {
            values.set_base(id, 100.0);
        }
    }
}

#[test]
#[ignore = "requires installed Fallout 3 and New Vegas masters"]
fn real_stimpaks_survive_disk_overlay_in_normal_and_hardcore_modes() {
    for (directory, master, nv) in [
        ("Fallout 3 goty", "Fallout3.esm", false),
        ("Fallout New Vegas", "FalloutNV.esm", true),
    ] {
        let path = format!("/mnt/data/SteamLibrary/steamapps/common/{directory}/Data/{master}");
        let index = byroredux_plugin::esm::parse_esm(&std::fs::read(path).unwrap()).unwrap();
        disk_round_trip(&index, 0x15169, 30.0);
        disk_round_trip_with_perk(&index, 0x15169, 36.0, Some(0x94ebf));
        if nv {
            timed_disk_round_trip(&index, 0x15169, 0.0, 5.0, 6.0, None, true);
            timed_disk_round_trip(&index, 0x15169, 0.0, 6.0, 6.0, Some(0x94ebf), true);
        }
    }
}

#[test]
#[ignore = "requires installed Skyrim SE master"]
fn real_skyrim_potion_survives_disk_load_overlay() {
    let path = "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim.esm";
    let index =
        byroredux_plugin::esm::parse_esm(&std::fs::read(path).expect("Skyrim master required"))
            .unwrap();
    disk_round_trip(&index, 0x3EADD, 25.0);
}

#[test]
#[ignore = "requires installed Fallout 3 master"]
fn real_fallout3_water_survives_disk_load_overlay() {
    let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout3.esm";
    let index =
        byroredux_plugin::esm::parse_esm(&std::fs::read(path).expect("Fallout 3 master required"))
            .unwrap();
    assert_eq!(index.items[&0x151A3].common.editor_id, "WaterPurified");
    // Conditional Stimpak effects must not be flattened into one restoration.
    assert!(byroredux_plugin::consumables::instant_restorations(&index, 0x15169).is_none());
    disk_round_trip(&index, 0x151A3, 20.0);
    // BloodPack without its perk restores only the unconditional point.
    disk_round_trip(&index, 0x34051, 1.0);
    disk_round_trip_with_perk(&index, 0x34051, 20.0, Some(0x3131));
}
