use super::*;
use byroredux_core::ecs::components::{Children, MeshHandle, Parent};

/// #4571 — pickup_loot's marker must reach the placement's MESH
/// entities, not just the root: the root never carries a MeshHandle
/// (spawn_placement_root), and both render skips read `PickedUp` on the
/// mesh/skinned entity itself, so a root-only marker never hid anything.
/// The two test asserts the render skips' own docs claimed ("will hide
/// the placement's meshes") were testing the mesh entity directly and
/// could not see the placement-root shape.
#[test]
fn pickup_stamps_the_subtree_meshes_not_just_the_root() {
    let mut world = World::new();
    world.register::<PickedUp>();
    world.register::<MeshHandle>();
    world.register::<Parent>();
    world.register::<Children>();
    // placement root -> mesh child (the cell-loader shape).
    let root = world.spawn();
    let mesh = world.spawn();
    world.insert(mesh, Parent(root));
    crate::helpers::add_child(&mut world, root, mesh);
    world.insert(mesh, MeshHandle(7));

    // Same shape as `pickup_loot`: walk first, then take the marker write
    // (#4983).
    let meshes = crate::npc_spawn::loot_appearance::mesh_entities_under(&world, root);
    if let Some(mut markers) = world.query_mut::<PickedUp>() {
        markers.insert(root, PickedUp);
        for entity in meshes {
            markers.insert(entity, PickedUp);
        }
    }

    let q = world.query::<PickedUp>().unwrap();
    assert!(q.get(root).is_some(), "the root keeps its marker");
    assert!(
        q.get(mesh).is_some(),
        "the mesh descendant must carry the marker too — the render skips \
         read it there (#4571)"
    );
}

/// #4707 — a refused `Consume` leaves exactly one player-visible
/// "Can't use … now" line (and nothing else) behind.
fn assert_one_refusal(world: &World) {
    let lines = crate::notifications::drain(world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("Can't use "), "{lines:?}");
}

fn restoration(
    actor_value: u32,
    magnitude: f32,
) -> byroredux_plugin::consumables::ConditionalRestoration {
    byroredux_plugin::consumables::ConditionalRestoration {
        actor_value,
        magnitude,
        duration: 0,
        conditions: Vec::new(),
        medicine: None,
    }
}

fn restorative_fixture() -> (World, byroredux_core::ecs::EntityId) {
    use byroredux_core::ecs::components::{ActorValues, ActorVitals, Dead};
    let (mut world, player) = fixture();
    world.register::<ActorValues>();
    world.register::<ActorVitals>();
    world.register::<Dead>();
    let mut values = ActorValues::from_pairs([(1000, 100.0)]);
    values.apply_damage(1000, 60.0);
    world.insert(player, values);
    world.insert(player, ActorVitals { health: 1000 });
    world.get_mut::<Inventory>(player).unwrap().items[1].count = 2;
    world
        .resource_mut::<InventoryCatalog>()
        .restorations
        .insert(0x5678, vec![restoration(1000, 25.0)]);
    world.insert_resource(crate::notifications::PlayerNotifications::default());
    (world, player)
}

#[test]
fn timed_consumption_ticks_partial_frames_and_stops_on_death() {
    use byroredux_core::ecs::components::{ActorValues, Dead, TimedRestorations};
    let (mut world, player) = restorative_fixture();
    world
        .resource_mut::<InventoryCatalog>()
        .restorations
        .get_mut(&0x5678)
        .unwrap()[0]
        .duration = 3;
    let action = byroredux_debug_ui::InventoryAction::Consume {
        index: 1,
        form_id: 0x5678,
    };
    assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(1000),
        40.0
    );
    assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 1);
    crate::systems::restoration::restoration_system(&world, 0.5);
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(1000),
        52.5
    );
    crate::systems::restoration::restoration_system(&world, 0.0);
    assert_eq!(
        world.get::<TimedRestorations>(player).unwrap().effects[0].remaining,
        2.5
    );
    world.insert(player, Dead);
    crate::systems::restoration::restoration_system(&world, 1.0);
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(1000),
        52.5
    );
    assert!(world
        .get::<TimedRestorations>(player)
        .unwrap()
        .effects
        .is_empty());
    world.remove::<Dead>(player);
    assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
    // Health may reach zero before the structural death marker is stamped.
    world
        .get_mut::<ActorValues>(player)
        .unwrap()
        .apply_damage(1000, 52.5);
    crate::systems::restoration::restoration_system(&world, 1.0);
    assert_eq!(world.get::<ActorValues>(player).unwrap().current(1000), 0.0);
    assert!(world
        .get::<TimedRestorations>(player)
        .unwrap()
        .effects
        .is_empty());
}

#[test]
fn medicine_scaling_uses_live_composed_skill_and_preflights_missing_or_invalid_values() {
    use byroredux_core::ecs::components::ActorValues;
    use byroredux_plugin::consumables::MedicineScaling;
    let (mut world, player) = restorative_fixture();
    let scale = Some(MedicineScaling {
        actor_value: 2000,
        base: 1.0,
        multiplier: 2.0,
    });
    world
        .resource_mut::<InventoryCatalog>()
        .restorations
        .get_mut(&0x5678)
        .unwrap()[0]
        .medicine = scale;
    let action = byroredux_debug_ui::InventoryAction::Consume {
        index: 1,
        form_id: 0x5678,
    };
    for bad in [None, Some(f32::NAN), Some(f32::INFINITY)] {
        if let Some(value) = bad {
            world
                .get_mut::<ActorValues>(player)
                .unwrap()
                .set_base(2000, value);
        }
        assert_eq!(
            apply_action(&mut world, action),
            MutationResult::Unavailable
        );
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            40.0
        );
        assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 2);
    }
    let values = world.get_mut::<ActorValues>(player).unwrap();
    values.set_base(2000, 50.0);
    values.set_base(3000, 100.0);
    values.apply_damage(3000, 80.0);
    let mut limb = restoration(3000, 25.0);
    limb.medicine = scale;
    world
        .resource_mut::<InventoryCatalog>()
        .restorations
        .get_mut(&0x5678)
        .unwrap()
        .push(limb);
    assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(1000),
        90.0
    );
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(3000),
        70.0
    );
    world
        .get_mut::<ActorValues>(player)
        .unwrap()
        .mod_permanent(2000, 50.0);
    assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(1000),
        100.0
    );
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(3000),
        100.0
    );
    assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 0);
}

#[test]
#[ignore = "requires installed Fallout 3 and New Vegas masters"]
fn real_stimpaks_restore_scaled_health_and_limbs_but_hardcore_only_health() {
    use byroredux_core::character::Perks;
    use byroredux_core::ecs::components::{ActorValues, ActorVitals, Dead, TimedRestorations};
    use byroredux_core::ecs::resources::HardcoreMode;
    for (path, master, nv) in [
        (byroredux_plugin::esm::test_paths::fo3_esm(), "Fallout3.esm", false),
        (byroredux_plugin::esm::test_paths::fnv_esm(), "FalloutNV.esm", true),
    ] {
        let index = byroredux_plugin::esm::parse_esm(&std::fs::read(path).unwrap()).unwrap();
        let health = index.health_actor_value_key().unwrap();
        let medicine = index.actor_value_form_id("Medicine").unwrap();
        let derived =
            byroredux_plugin::esm::records::derive_npc_actor_values(&index.npcs[&7], &index);
        let base = ActorValues::from_pairs(derived);
        assert!(
            base.get(medicine).is_some(),
            "player spawn must seed Medicine"
        );
        for name in byroredux_plugin::consumables::BODY_CONDITION_VALUES {
            assert_eq!(
                base.current(index.actor_value_form_id(name).unwrap()),
                100.0,
                "{master} {name}"
            );
        }
        for hardcore in [false, true]
            .into_iter()
            .filter(|&hardcore| nv || !hardcore)
        {
            for perk in [false, true] {
                let (mut world, player) = fixture();
                world.register::<Dead>();
                install_catalog(&mut world, &index);
                world.insert_resource(HardcoreMode { enabled: hardcore });
                let mut values = base.clone();
                values.set_base(health, 1000.0);
                values.apply_damage(health, 960.0);
                values.set_base(medicine, 50.0);
                for name in byroredux_plugin::consumables::BODY_CONDITION_VALUES {
                    values.apply_damage(index.actor_value_form_id(name).unwrap(), 80.0);
                }
                world.insert(player, values);
                world.insert(player, ActorVitals { health });
                world.insert(
                    player,
                    Inventory {
                        items: vec![ItemStack::new(0x15169, 1)],
                    },
                );
                let mut perks = Perks::default();
                if perk {
                    perks.set_rank(0x94ebf, 1);
                }
                world.insert(player, perks);
                assert!(snapshot(&world).unwrap().items[0].consumable);
                assert_eq!(
                    apply_action(
                        &mut world,
                        byroredux_debug_ui::InventoryAction::Consume {
                            index: 0,
                            form_id: 0x15169
                        }
                    ),
                    MutationResult::Consumed
                );
                let amount = if perk { 72.0 } else { 60.0 };
                assert_eq!(
                    world.get::<ActorValues>(player).unwrap().current(health),
                    if hardcore { 40.0 } else { 40.0 + amount }
                );
                if hardcore {
                    assert_eq!(
                        world
                            .get::<TimedRestorations>(player)
                            .unwrap()
                            .effects
                            .len(),
                        1
                    );
                    // The dose snapshots Medicine rather than changing as
                    // a skill buff expires partway through the duration.
                    world
                        .get_mut::<ActorValues>(player)
                        .unwrap()
                        .set_base(medicine, 0.0);
                    crate::systems::restoration::restoration_system(&world, 3.0);
                    assert_eq!(
                        world.get::<ActorValues>(player).unwrap().current(health),
                        40.0 + amount * 0.5
                    );
                    crate::systems::restoration::restoration_system(&world, 100.0);
                    assert_eq!(
                        world.get::<ActorValues>(player).unwrap().current(health),
                        40.0 + amount
                    );
                    assert!(world
                        .get::<TimedRestorations>(player)
                        .unwrap()
                        .effects
                        .is_empty());
                }
                for name in byroredux_plugin::consumables::BODY_CONDITION_VALUES {
                    let id = index.actor_value_form_id(name).unwrap();
                    assert_eq!(
                        world.get::<ActorValues>(player).unwrap().current(id),
                        if hardcore { 20.0 } else { 20.0 + amount },
                        "{master} {name}"
                    );
                }
                assert_eq!(world.get::<Inventory>(player).unwrap().items[0].count, 0);
            }
        }
    }
}

#[test]
fn conditional_consumption_observes_perks_at_use_time() {
    use byroredux_core::character::Perks;
    use byroredux_core::ecs::components::ActorValues;
    use byroredux_plugin::esm::reader::SubRecord;
    use byroredux_plugin::esm::records::{parse_alch_for_game, AvifRecord, MgefRecord};
    let (mut world, player) = restorative_fixture();
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
    let mut subs = vec![SubRecord {
        sub_type: *b"ENIT",
        data: vec![0; 20],
    }];
    for (magnitude, comparand) in [(10f32, 0f32), (30.0, 1.0)] {
        let mut condition = vec![0; 32];
        condition[4..8].copy_from_slice(&comparand.to_le_bytes());
        condition[8..10].copy_from_slice(&448u16.to_le_bytes());
        condition[12..16].copy_from_slice(&77u32.to_le_bytes());
        condition[20..24].copy_from_slice(&1u32.to_le_bytes()); // target is consumer
        subs.extend([
            SubRecord {
                sub_type: *b"EFID",
                data: 20u32.to_le_bytes().to_vec(),
            },
            SubRecord {
                sub_type: *b"EFIT",
                data: [magnitude.to_le_bytes(), [0; 4], [0; 4]].concat(),
            },
            SubRecord {
                sub_type: *b"CTDA",
                data: condition,
            },
        ]);
    }
    index.items.insert(
        0x5678,
        parse_alch_for_game(0x5678, &subs, index.game, &None),
    );
    install_catalog(&mut world, &index);
    let action = byroredux_debug_ui::InventoryAction::Consume {
        index: 1,
        form_id: 0x5678,
    };
    assert!(
        snapshot(&world)
            .unwrap()
            .items
            .iter()
            .find(|i| i.index == 1)
            .unwrap()
            .consumable
    );
    assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(1000),
        50.0
    );
    let mut perks = Perks::default();
    perks.set_rank(77, 1);
    world.insert(player, perks);
    // Same catalog; runtime ownership selects the other branch.
    assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(1000),
        80.0
    );
    assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 0);
    assert_eq!(crate::notifications::drain(&world).len(), 2);

    // All-false conditions leave both inventory and health unchanged.
    world.get_mut::<Inventory>(player).unwrap().items[1].count = 1;
    world
        .resource_mut::<InventoryCatalog>()
        .restorations
        .get_mut(&0x5678)
        .unwrap()
        .truncate(1);
    assert_eq!(
        apply_action(&mut world, action),
        MutationResult::Unavailable
    );
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(1000),
        80.0
    );
    assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 1);
    assert_one_refusal(&world);
}

#[test]
fn consuming_restores_damage_decrements_once_and_keeps_equipment_indices() {
    use byroredux_core::ecs::components::ActorValues;
    let (mut world, player) = restorative_fixture();
    world
        .get_mut::<EquipmentSlots>(player)
        .unwrap()
        .equip_weapon(InventoryIndex(2));
    assert!(
        snapshot(&world)
            .unwrap()
            .items
            .iter()
            .find(|i| i.index == 1)
            .unwrap()
            .consumable
    );
    let action = byroredux_debug_ui::InventoryAction::Consume {
        index: 1,
        form_id: 0x5678,
    };
    for health in [65.0, 90.0] {
        assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            health
        );
    }
    assert_eq!(
        apply_action(&mut world, action),
        MutationResult::Unavailable
    );
    assert_eq!(world.get::<Inventory>(player).unwrap().items.len(), 3);
    assert_eq!(
        world.get::<EquipmentSlots>(player).unwrap().weapon,
        Some(InventoryIndex(2))
    );
    // Two "Used" lines, then the refusal of the emptied row (#4707).
    let lines = crate::notifications::drain(&world);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[2].starts_with("Can't use "), "{lines:?}");
}

#[test]
fn unavailable_consumption_never_changes_health_or_count() {
    use byroredux_core::ecs::components::{ActorValues, Dead};
    for variant in 0..5 {
        let (mut world, player) = restorative_fixture();
        let mut form_id = 0x5678;
        match variant {
            0 => form_id = 0x1234, // stale UI identity
            1 => {
                world.insert(player, Dead);
            }
            2 => {
                world
                    .resource_mut::<InventoryCatalog>()
                    .restorations
                    .clear();
            }
            3 => {
                world
                    .resource_mut::<InventoryCatalog>()
                    .restorations
                    .get_mut(&form_id)
                    .unwrap()
                    .push(restoration(9999, 10.0));
            }
            _ => {
                world
                    .get_mut::<EquipmentSlots>(player)
                    .unwrap()
                    .equip_weapon(InventoryIndex(1));
            }
        }
        assert_eq!(
            apply_action(
                &mut world,
                byroredux_debug_ui::InventoryAction::Consume { index: 1, form_id }
            ),
            MutationResult::Unavailable
        );
        assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 2);
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            40.0
        );
        assert_one_refusal(&world);
    }
}

#[test]
fn consuming_invalid_instance_is_atomic() {
    use byroredux_core::ecs::components::ActorValues;
    use byroredux_core::ecs::resources::{ItemInstance, ItemInstancePool};
    for missing_pool in [false, true] {
        for count in [1, 2] {
            let (mut world, player) = restorative_fixture();
            let mut pool = ItemInstancePool::new();
            let id = pool.allocate(ItemInstance::default());
            pool.release(id).unwrap();
            if !missing_pool {
                world.insert_resource(pool);
            }
            {
                let inventory = world.get_mut::<Inventory>(player).unwrap();
                inventory.items[1].count = count;
                inventory.items[1].instance = Some(id);
            }
            assert_eq!(
                apply_action(
                    &mut world,
                    byroredux_debug_ui::InventoryAction::Consume {
                        index: 1,
                        form_id: 0x5678,
                    },
                ),
                MutationResult::Unavailable,
                "missing_pool={missing_pool}, count={count}"
            );
            let inventory = world.get::<Inventory>(player).unwrap();
            assert_eq!(inventory.items[1].count, count);
            assert_eq!(inventory.items[1].instance, Some(id));
            drop(inventory);
            assert_eq!(
                world.get::<ActorValues>(player).unwrap().current(1000),
                40.0
            );
            assert_one_refusal(&world);
            if !missing_pool {
                let mut pool = world.resource_mut::<ItemInstancePool>();
                assert_eq!(pool.live_count(), 0);
                assert_eq!(pool.allocate(ItemInstance::default()), id);
                assert_ne!(pool.allocate(ItemInstance::default()), id);
            }
        }
    }
}

#[test]
fn consuming_final_instance_releases_pool_and_never_overheals() {
    use byroredux_core::ecs::components::ActorValues;
    use byroredux_core::ecs::resources::{ItemInstance, ItemInstancePool};
    let (mut world, player) = restorative_fixture();
    let mut pool = ItemInstancePool::new();
    let id = pool.allocate(ItemInstance::default());
    world.insert_resource(pool);
    {
        let inventory = world.get_mut::<Inventory>(player).unwrap();
        inventory.items[1].count = 1;
        inventory.items[1].instance = Some(id);
    }
    world
        .resource_mut::<InventoryCatalog>()
        .restorations
        .insert(0x5678, vec![restoration(1000, 9999.0)]);
    assert_eq!(
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::Consume {
                index: 1,
                form_id: 0x5678
            }
        ),
        MutationResult::Consumed
    );
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(1000),
        100.0
    );
    assert_eq!(world.resource::<ItemInstancePool>().live_count(), 0);
    assert!(world.get::<Inventory>(player).unwrap().items[1]
        .instance
        .is_none());
}

#[test]
#[ignore = "requires installed Skyrim SE master"]
fn real_skyrim_healing_potion_runs_through_native_action() {
    use byroredux_core::ecs::components::ActorValues;
    let path = byroredux_plugin::esm::test_paths::skyrim_se_esm();
    let index =
        byroredux_plugin::esm::parse_esm(&std::fs::read(path).expect("Skyrim master required"))
            .unwrap();
    let (mut world, player) = restorative_fixture();
    install_catalog(&mut world, &index);
    world.get_mut::<Inventory>(player).unwrap().items[1] = ItemStack::new(0x3EADD, 1);
    assert_eq!(
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::Consume {
                index: 1,
                form_id: 0x3EADD
            }
        ),
        MutationResult::Consumed
    );
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(1000),
        65.0
    );
    assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 0);
}

#[test]
#[ignore = "requires installed Fallout 3 master"]
fn real_fallout3_bloodpack_selects_live_perk_bonus() {
    use byroredux_core::character::Perks;
    use byroredux_core::ecs::components::{ActorValues, ActorVitals};
    let path = byroredux_plugin::esm::test_paths::fo3_esm();
    let index = byroredux_plugin::esm::parse_esm(
        &std::fs::read(path).expect("Fallout 3 master required"),
    )
    .unwrap();
    assert_eq!(index.items[&0x34051].common.editor_id, "BloodPack");
    let health = index.health_actor_value_key().unwrap();
    let (mut world, player) = restorative_fixture();
    install_catalog(&mut world, &index);
    world.get_mut::<Inventory>(player).unwrap().items[1] = ItemStack::new(0x34051, 2);
    let mut values = ActorValues::from_pairs([(health, 100.0)]);
    values.apply_damage(health, 60.0);
    world.insert(player, values);
    world.insert(player, ActorVitals { health });
    let action = byroredux_debug_ui::InventoryAction::Consume {
        index: 1,
        form_id: 0x34051,
    };
    assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(health),
        41.0
    );
    let mut perks = Perks::default();
    perks.set_rank(0x3131, 1);
    world.insert(player, perks);
    assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
    assert_eq!(
        world.get::<ActorValues>(player).unwrap().current(health),
        61.0
    );
    assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 0);
    assert_eq!(crate::notifications::drain(&world).len(), 2);
}

fn fixture() -> (World, byroredux_core::ecs::EntityId) {
    let mut world = World::new();
    world.register::<Inventory>();
    world.register::<EquipmentSlots>();
    world.register::<EquippedWeapon>();
    world.register::<byroredux_scripting::EquipmentEventBatch>();
    let player = world.spawn();
    let mut inventory = Inventory::new();
    inventory.push(ItemStack::new(0x1234, 1));
    inventory.push(ItemStack::new(0x5678, 12));
    inventory.push(ItemStack::new(0x9ABC, 1));
    world.insert(player, inventory);
    world.insert(player, EquipmentSlots::new());
    world.insert_resource(PlayerEntity(Some(player)));
    world.insert_resource(InventoryCatalog {
        restorations: Default::default(),
        script_effects: Default::default(),
        containers: Default::default(),
        player_base_form_id: PLAYER_NPC_FORM_ID,
        entries: FxHashMap::from_iter([
            (
                0x1234,
                InventoryItemDefinition {
                    name: "Iron Armor".to_owned(),
                    category: "Armor",
                    value: 125,
                    weight: 30.0,
                    details: "Armor rating 25.0".to_owned(),
                    equip_target: Some(EquipTarget::BipedSlots(1 << 12)),
                    weapon_damage: None,
                    weapon_reach: 0.0,
                    weapon_speed: 0.0,
                },
            ),
            (
                0x5678,
                InventoryItemDefinition {
                    name: "Iron Arrow".to_owned(),
                    category: "Ammo",
                    value: 1,
                    weight: 0.0,
                    details: "Damage 8.0".to_owned(),
                    equip_target: None,
                    weapon_damage: None,
                    weapon_reach: 0.0,
                    weapon_speed: 0.0,
                },
            ),
            (
                0x9ABC,
                InventoryItemDefinition {
                    name: "Iron Sword".to_owned(),
                    category: "Weapon",
                    value: 100,
                    weight: 8.0,
                    details: "Damage 12".to_owned(),
                    equip_target: Some(EquipTarget::Weapon),
                    weapon_damage: Some(12.0),
                    weapon_reach: 90.0,
                    weapon_speed: 1.1,
                },
            ),
        ]),
    });
    (world, player)
}

/// M47.2 — encumbrance weighs the fixture's 38 units against Carry
/// Weight, writes InventoryWeight, and slows a Fallout player once over.
#[test]
fn encumbrance_weighs_the_player_inventory_against_carry_weight() {
    use crate::encumbrance::{encumbrance_system, PlayerEncumbrance};
    use byroredux_core::ecs::components::ActorValues;
    use byroredux_plugin::esm::records::AvifRecord;
    let (mut world, player) = fixture();
    world.register::<ActorValues>();
    world.insert_resource(PlayerEncumbrance::default());
    let mut index = EsmIndex {
        game: byroredux_plugin::esm::reader::GameKind::Fallout3NV,
        character_rules: byroredux_core::character::CharacterRulesProfile::FALLOUT_NEW_VEGAS,
        ..Default::default()
    };
    for (form_id, editor_id) in [(0x2D1, "CarryWeight"), (0x2D2, "InventoryWeight")] {
        index.actor_values.insert(
            form_id,
            AvifRecord {
                form_id,
                editor_id: editor_id.into(),
                ..Default::default()
            },
        );
    }
    crate::encumbrance::install_encumbrance_rules(&mut world, &index);
    world.insert(player, ActorValues::from_pairs([(0x2D1, 40.0)]));

    encumbrance_system(&world, 0.0);
    let encumbrance = *world.resource::<PlayerEncumbrance>();
    assert_eq!((encumbrance.weight, encumbrance.capacity), (38.0, 40.0));
    assert_eq!(encumbrance.movement(), (1.0, true));
    assert_eq!(world.get::<ActorValues>(player).unwrap().current(0x2D2), 38.0);

    world.get_mut::<ActorValues>(player).unwrap().set_base(0x2D1, 30.0);
    encumbrance_system(&world, 0.0);
    assert_eq!(world.resource::<PlayerEncumbrance>().movement(), (0.4, true));

    // A Get Max Carry Weight (22) perk adding 25 lifts the capacity to 55.
    use byroredux_core::character::Perks;
    use byroredux_plugin::esm::records::{
        PerkEntry, PerkEntryBody, PerkFunctionData, PerkRecord,
    };
    const STRONG_BACK: u32 = 0x0003_1DBA;
    index.perks.insert(
        STRONG_BACK,
        PerkRecord {
            form_id: STRONG_BACK,
            entries: vec![PerkEntry {
                rank: 0,
                priority: 0,
                body: PerkEntryBody::EntryPoint {
                    entry_point_index: 22,
                    function: 2,
                    tab_count: 1,
                    param_type: 1,
                    function_data: PerkFunctionData::Float(25.0),
                    formatter: Vec::new(),
                    extra_flags: Vec::new(),
                },
                condition_tabs: Vec::new(),
            }],
            ..Default::default()
        },
    );
    byroredux_scripting::perk_entry_points::install_perk_entry_points(&mut world, &index);
    world.register::<Perks>();
    let mut perks = Perks::default();
    perks.set_rank(STRONG_BACK, 1);
    world.insert(player, perks);
    encumbrance_system(&world, 0.0);
    let encumbrance = *world.resource::<PlayerEncumbrance>();
    assert_eq!((encumbrance.capacity, encumbrance.over()), (55.0, false));
}

fn activated_container(
    world: &mut World,
    player: byroredux_core::ecs::EntityId,
) -> byroredux_core::ecs::EntityId {
    world
        .resource_mut::<InventoryCatalog>()
        .containers
        .insert(0xCAFE);
    let container = world.spawn();
    world.insert(
        container,
        byroredux_scripting::SceneAliasCandidate {
            reference_form_id: 0xDEAD,
            base_form_id: 0xCAFE,
            linked_refs: Vec::new(),
            location_ref_types: Vec::new(),
        },
    );
    world.insert(
        container,
        Inventory {
            items: vec![ItemStack::new(0x5678, 7)],
        },
    );
    world.insert(
        container,
        byroredux_scripting::ActivateEvent { activator: player },
    );
    container
}

#[test]
fn container_loot_preserves_stacks_instances_equipment_and_activation() {
    let (mut world, player) = fixture();
    let container = activated_container(&mut world, player);
    world
        .get_mut::<EquipmentSlots>(player)
        .unwrap()
        .equip_weapon(InventoryIndex(2));
    let instance =
        byroredux_core::ecs::components::ItemInstanceId(std::num::NonZeroU32::new(17).unwrap());
    let stacks = vec![
        ItemStack::new(0x5678, u32::MAX),
        ItemStack {
            base_form_id: 0x9ABC,
            count: 1,
            instance: Some(instance),
        },
        ItemStack {
            base_form_id: 0x1111,
            count: 0,
            instance: Some(byroredux_core::ecs::components::ItemInstanceId(
                std::num::NonZeroU32::new(18).unwrap(),
            )),
        },
    ];
    world.get_mut::<Inventory>(container).unwrap().items = stacks.clone();
    let original = world.get::<Inventory>(player).unwrap().items.clone();
    container_loot_system(&world, 0.0);
    container_loot_system(&world, 0.0);
    let inventory = world.get::<Inventory>(player).unwrap();
    // #5871 — the plain 0x5678 stack folds into the player's existing
    // row for it (saturating); the rows keep their positions, and the
    // instanced stacks keep their identities as appended rows.
    let mut expected = original.clone();
    let merged = expected.iter().position(|row| row.base_form_id == 0x5678).unwrap();
    expected[merged].count = u32::MAX;
    assert_eq!(&inventory.items[..original.len()], expected.as_slice());
    assert_eq!(&inventory.items[original.len()..], &stacks[1..]);
    drop(inventory);
    assert!(world.get::<Inventory>(container).unwrap().is_empty());
    assert_eq!(
        world.get::<EquipmentSlots>(player).unwrap().weapon,
        Some(InventoryIndex(2))
    );
    assert!(
        world
            .get::<byroredux_scripting::ActivateEvent>(container)
            .is_some(),
        "script consumers must still observe the activation"
    );
}

/// #4712 — a scripted `ContainerRef.Activate(PlayerRef)` from a quest
/// fragment or package leaf is queued, then turned into an
/// `ActivateEvent` by `fragment_activation_flush_system`. Run in the
/// schedule's order (flush, then loot — pinned by
/// `activation_flush_is_scheduled_before_every_activate_event_consumer`),
/// the queued activation must actually loot the container.
#[test]
fn scripted_fragment_activate_loots_the_container() {
    let (mut world, player) = fixture();
    world.register::<byroredux_scripting::ActivateEvent>();
    let container = activated_container(&mut world, player);
    world.remove::<byroredux_scripting::ActivateEvent>(container);
    let mut queue = byroredux_scripting::PendingFragmentActivations::default();
    queue.push(container, player);
    world.insert_resource(queue);

    byroredux_scripting::fragment_activation_flush_system(&world, 0.0);
    container_loot_system(&world, 0.0);

    assert!(world.get::<Inventory>(container).unwrap().is_empty());
    // #5871 — folded into the player's existing 0x5678 row (12 + 7).
    let inventory = world.get::<Inventory>(player).unwrap();
    assert_eq!(
        inventory.items.iter().find(|row| row.base_form_id == 0x5678),
        Some(&ItemStack::new(0x5678, 19))
    );
    assert_eq!(inventory.items.iter().filter(|row| row.base_form_id == 0x5678).count(), 1);
}

#[test]
fn container_loot_rejects_locked_nonplayer_and_noncontainer_targets() {
    let (mut world, player) = fixture();
    let locked = activated_container(&mut world, player);
    world.insert(
        locked,
        byroredux_core::ecs::components::Locked {
            lock_level: 25,
            key_form_id: None,
        },
    );
    let npc = world.spawn();
    let npc_activation = activated_container(&mut world, npc);
    let actor_inventory = activated_container(&mut world, player);
    world
        .get_mut::<byroredux_scripting::SceneAliasCandidate>(actor_inventory)
        .unwrap()
        .base_form_id = 0xBEEF;
    container_loot_system(&world, 0.0);
    assert_eq!(world.get::<Inventory>(player).unwrap().len(), 3);
    for target in [locked, npc_activation, actor_inventory] {
        assert_eq!(
            world.get::<Inventory>(target).unwrap().items,
            vec![ItemStack::new(0x5678, 7)]
        );
    }
}

/// #4699 — the theft rule's three inputs: player-owned property is the
/// Player base (0x7) in `XOWN`, never stolen; CELL ownership reaches
/// only containers/items with no `XOWN` of their own; and the faction
/// exemption honours the `XRNK` rank bar.
#[test]
fn theft_rule_reads_the_player_base_cell_ownership_and_the_rank_bar() {
    use byroredux_core::ecs::components::{FactionRanks, Owned};
    use byroredux_plugin::esm::cell::CellOwnership;
    let (mut world, player) = fixture();
    world.insert_resource(crate::notifications::PlayerNotifications::default());
    world.register::<Owned>();

    let own_chest = activated_container(&mut world, player);
    world.insert(
        own_chest,
        Owned {
            owner_form_id: PLAYER_NPC_FORM_ID,
            faction_rank: None,
            global_var_form_id: None,
        },
    );
    let outcome = transfer_loot(&world, player, own_chest, LootSelection::All).unwrap();
    assert!(!outcome.stolen, "the player's own chest");
    assert_eq!(
        crate::notifications::drain(&world),
        vec!["Took 7 items".to_owned()]
    );

    // A shop interior: CELL XOWN names the merchant faction, rank 2+.
    let first = world.next_entity_id();
    let shop_chest = activated_container(&mut world, player);
    let refr_owned = activated_container(&mut world, player);
    world.insert(
        refr_owned,
        Owned {
            owner_form_id: 0xABBA,
            faction_rank: None,
            global_var_form_id: None,
        },
    );
    let shopkeeper = world.spawn();
    world.insert(
        shopkeeper,
        byroredux_scripting::SceneAliasCandidate {
            reference_form_id: 0x5EED,
            base_form_id: 0xBEEF,
            ..Default::default()
        },
    );
    let merchants = CellOwnership {
        owner_form_id: 0xF00D,
        faction_rank: Some(2),
        global_var_form_id: None,
    };
    let last = world.next_entity_id();
    stamp_cell_ownership(&mut world, Some(&merchants), first, last);
    assert_eq!(
        world
            .get::<Owned>(shop_chest)
            .map(|owned| owned.owner_form_id),
        Some(0xF00D),
        "a container with no XOWN inherits its cell's"
    );
    assert_eq!(
        world
            .get::<Owned>(refr_owned)
            .map(|owned| owned.owner_form_id),
        Some(0xABBA),
        "a REFR's own XOWN overrides its cell's"
    );
    assert!(!world.has::<Owned>(shopkeeper), "actors are not property");

    // Below the rank bar it is theft; at the bar it is not.
    world.insert(player, FactionRanks::from_pairs([(0xF00D, 1)]));
    assert!(theft_owner(&world, player, shop_chest).is_some());
    world.insert(player, FactionRanks::from_pairs([(0xF00D, 2)]));
    assert!(theft_owner(&world, player, shop_chest).is_none());
    world.remove::<FactionRanks>(player);
    // #5866 — a script's ownership change reaches the theft rule: cleared
    // or handed to the player it is no theft; handed to someone else it is.
    world.init_resource::<byroredux_scripting::OwnerOverrides>();
    {
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        world.register::<byroredux_core::ecs::components::FormIdComponent>();
        world.init_resource::<FormIdPool>();
        let id = world.resource_mut::<FormIdPool>().intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(0xDEAD),
        });
        world.insert(shop_chest, byroredux_core::ecs::components::FormIdComponent(id));
    }
    assert!(theft_owner(&world, player, shop_chest).is_some());
    world.resource_mut::<byroredux_scripting::OwnerOverrides>().set(0xDEAD, 0);
    assert!(theft_owner(&world, player, shop_chest).is_none(), "SetActorOwner(None) clears it");
    world.resource_mut::<byroredux_scripting::OwnerOverrides>().set(0xDEAD, PLAYER_NPC_FORM_ID);
    assert!(theft_owner(&world, player, shop_chest).is_none());
    world.resource_mut::<byroredux_scripting::OwnerOverrides>().set(0xDEAD, 0xABBA);
    assert!(theft_owner(&world, player, shop_chest).is_some());
    let outcome = transfer_loot(&world, player, shop_chest, LootSelection::All).unwrap();
    assert!(outcome.stolen);
    assert_eq!(
        crate::notifications::drain(&world),
        vec!["Stolen 7 items".to_owned()]
    );
}

/// A theft of the merchants' chest (7 arrows, value 1 each) in a world
/// with the Oblivion crime model: the faction flag is set whoever looks,
/// the bounty (`7 × 0.5`, rounded up) only when an NPC can see the thief.
fn crime_world(witness_at: Option<byroredux_core::math::Vec3>) -> (World, byroredux_core::ecs::EntityId, byroredux_core::ecs::EntityId) {
    use byroredux_core::ecs::components::{GlobalTransform, Owned};
    use byroredux_core::math::Vec3;
    use byroredux_plugin::esm::records::{ActorAiData, Aggression, Confidence, FactionRecord};
    let (mut world, player) = fixture();
    world.insert_resource(crate::notifications::PlayerNotifications::default());
    world.register::<Owned>();
    world.register::<GlobalTransform>();
    world.register::<byroredux_core::ecs::components::Dead>();
    world.register::<crate::systems::faction_hostility::CombatDisposition>();
    world.insert_resource(byroredux_core::character::CrimeModel::OBLIVION);
    world.insert_resource(byroredux_scripting::obscript_ledger::ObScriptLedger::default());
    world.insert_resource(crate::systems::faction_hostility::DetectionConfig { max_distance: 1000.0, exterior_mult: 1.0 });
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.factions.insert(
        0xF00D,
        FactionRecord {
            form_id: 0xF00D,
            editor_id: String::new(),
            full_name: String::new(),
            flags: 0,
            relations: Vec::new(),
            ranks: Vec::new(),
            reputation: None,
            vendor: None,
            crime: None,
        },
    );
    byroredux_scripting::install_owner_kinds(&mut world, &index);
    world.insert(player, GlobalTransform::new(Vec3::ZERO, Default::default(), 1.0));
    if let Some(at) = witness_at {
        let witness = world.spawn();
        world.insert(witness, GlobalTransform::new(at, Default::default(), 1.0));
        world.insert(
            witness,
            crate::systems::faction_hostility::CombatDisposition(ActorAiData {
                aggression: Aggression::Unaggressive,
                confidence: Confidence::Average,
                responsibility: None,
                attack_radius: None,
            }),
        );
    }
    let chest = activated_container(&mut world, player);
    world.insert(chest, Owned { owner_form_id: 0xF00D, faction_rank: None, global_var_form_id: None });
    (world, player, chest)
}

#[test]
fn a_witnessed_theft_adds_the_bounty_and_marks_the_faction() {
    use byroredux_scripting::obscript_ledger::{FactionFlag, ObScriptLedger};
    let (world, player, chest) = crime_world(Some(byroredux_core::math::Vec3::new(0.0, 0.0, 500.0)));
    transfer_loot(&world, player, chest, LootSelection::All).unwrap();
    let ledger = world.resource::<ObScriptLedger>();
    assert_eq!(ledger.crime_gold, 4, "7 arrows × 1 gold × 0.5 = 3.5, rounded up");
    assert_eq!(ledger.stolen_goods(), vec![(0x5678, 7)], "the seven arrows are on record as stolen");
    assert!(ledger.flag(FactionFlag::Steal, 0xF00D));
    drop(ledger);
    assert!(crate::notifications::drain(&world).contains(&"You were seen stealing. Your bounty is now 4 gold".to_owned()));
}

#[test]
fn an_unwitnessed_theft_marks_the_faction_but_costs_nothing() {
    use byroredux_scripting::obscript_ledger::{FactionFlag, ObScriptLedger};
    // The only NPC is past the detection range.
    let (world, player, chest) = crime_world(Some(byroredux_core::math::Vec3::new(0.0, 0.0, 5000.0)));
    transfer_loot(&world, player, chest, LootSelection::All).unwrap();
    let ledger = world.resource::<ObScriptLedger>();
    assert_eq!(ledger.crime_gold, 0);
    assert!(ledger.flag(FactionFlag::Steal, 0xF00D), "no witness is needed for the flag");
    assert_eq!(ledger.stolen_goods(), vec![(0x5678, 7)], "unseen goods are still stolen goods");
}

#[test]
fn a_dead_or_creature_onlooker_reports_nothing() {
    use byroredux_scripting::obscript_ledger::ObScriptLedger;
    let near = byroredux_core::math::Vec3::new(0.0, 0.0, 500.0);
    for kind in ["dead", "creature"] {
        let (mut world, player, chest) = crime_world(Some(near));
        let witness = world
            .query::<crate::systems::faction_hostility::CombatDisposition>()
            .unwrap()
            .iter()
            .map(|(entity, _)| entity)
            .next()
            .unwrap();
        if kind == "dead" {
            world.insert(witness, byroredux_core::ecs::components::Dead);
        } else {
            world.register::<byroredux_core::ecs::components::CreatureActor>();
            world.insert(witness, byroredux_core::ecs::components::CreatureActor);
        }
        transfer_loot(&world, player, chest, LootSelection::All).unwrap();
        assert_eq!(world.resource::<ObScriptLedger>().crime_gold, 0, "{kind}");
    }
}

#[test]
fn a_game_without_a_crime_model_charges_nothing() {
    use byroredux_scripting::obscript_ledger::{FactionFlag, ObScriptLedger};
    let (mut world, player, chest) = crime_world(Some(byroredux_core::math::Vec3::new(0.0, 0.0, 500.0)));
    world.remove_resource::<byroredux_core::character::CrimeModel>();
    let outcome = transfer_loot(&world, player, chest, LootSelection::All).unwrap();
    assert!(outcome.stolen, "still classified as theft");
    let ledger = world.resource::<ObScriptLedger>();
    assert_eq!(ledger.crime_gold, 0);
    assert!(!ledger.flag(FactionFlag::Steal, 0xF00D));
}

#[test]
fn corpse_loot_clears_source_equipment_and_emits_each_unequip_once() {
    let (mut world, player) = fixture();
    let corpse = activated_container(&mut world, player);
    world.remove::<byroredux_scripting::SceneAliasCandidate>(corpse);
    world.insert(corpse, byroredux_core::ecs::components::Dead);
    world.get_mut::<Inventory>(corpse).unwrap().items =
        vec![ItemStack::new(0x1234, 1), ItemStack::new(0x9ABC, 1)];
    let mut slots = EquipmentSlots::new();
    slots.equip(0b110, InventoryIndex(0));
    slots.equip_weapon(InventoryIndex(1));
    world.insert(corpse, slots);
    world.insert(
        corpse,
        EquippedWeapon {
            inventory_index: InventoryIndex(1),
            base_form_id: 0x9ABC,
            damage: 12.0,
            reach: 1.0,
            speed: 1.0,
        },
    );
    container_loot_system(&world, 0.0);
    container_loot_system(&world, 0.0);
    assert!(world.get::<Inventory>(corpse).unwrap().is_empty());
    // #5871 — the corpse's plain 0x1234 / 0x9ABC fold into the player's
    // existing rows for them instead of appending two more.
    assert_eq!(world.get::<Inventory>(player).unwrap().len(), 3);
    assert_eq!(
        world
            .get::<EquipmentSlots>(corpse)
            .unwrap()
            .equipped_indices()
            .count(),
        0
    );
    assert!(world.get::<EquippedWeapon>(corpse).is_none());
    assert_eq!(
        world
            .get::<byroredux_scripting::EquipmentEventBatch>(corpse)
            .unwrap()
            .0,
        vec![
            byroredux_scripting::EquipmentChange {
                item_form_id: 0x1234,
                equipped: false
            },
            byroredux_scripting::EquipmentChange {
                item_form_id: 0x9ABC,
                equipped: false
            },
        ]
    );
    world.insert(player, byroredux_core::ecs::components::Dead);
    assert!(
        !is_loot_source(&world, player),
        "the player must not target their own inventory"
    );
}

#[test]
fn container_loot_keeps_source_when_player_inventory_is_missing() {
    let (mut world, player) = fixture();
    let container = activated_container(&mut world, player);
    world.remove::<Inventory>(player);
    container_loot_system(&world, 0.0);
    assert_eq!(
        world.get::<Inventory>(container).unwrap().items,
        vec![ItemStack::new(0x5678, 7)]
    );
}

/// #3488 — the removal direction across a live load. The save overlay is
/// additive-only, so a snapshot taken while unarmed carries no
/// `EquippedWeapon` row at all; the live component on the surviving
/// player body is never touched by `apply_deltas`. This reconstructs the
/// post-overlay state — restored `EquipmentSlots.weapon == None`, live
/// `EquippedWeapon` still standing — and pins that the reconciler clears
/// it.
#[test]
fn load_reconciler_clears_a_weapon_the_save_did_not_have() {
    let (mut world, player) = fixture();

    // Live session: the player equips the Iron Sword (index 2).
    world
        .get_mut::<EquipmentSlots>(player)
        .unwrap()
        .equip_weapon(InventoryIndex(2));
    reconcile_player_equipped_weapon(&mut world, player);
    assert_eq!(
        world.get::<EquippedWeapon>(player).map(|w| w.base_form_id),
        Some(0x9ABC),
        "precondition: the live session is holding the sword"
    );

    // The overlay lands the saved (unarmed) `EquipmentSlots` — and
    // cannot remove the live `EquippedWeapon`, which is the whole bug.
    world.insert(player, EquipmentSlots::new());
    assert!(
        world.get::<EquippedWeapon>(player).is_some(),
        "apply_deltas is additive-only: the stale weapon is still here"
    );

    reconcile_player_equipped_weapon(&mut world, player);
    assert!(
        world.get::<EquippedWeapon>(player).is_none(),
        "the load reconciler must clear a weapon the save did not have — \
         otherwise melee keeps dealing the live session's weapon damage \
         instead of the saved unarmed damage (#3488)"
    );
}

/// The other direction, so the reconciler cannot pass by removing
/// unconditionally: a save taken while armed must re-derive the weapon
/// onto a body that is currently unarmed.
#[test]
fn load_reconciler_restores_a_weapon_the_save_did_have() {
    let (mut world, player) = fixture();
    world
        .get_mut::<EquipmentSlots>(player)
        .unwrap()
        .equip_weapon(InventoryIndex(2));

    reconcile_player_equipped_weapon(&mut world, player);
    let weapon = world
        .get::<EquippedWeapon>(player)
        .expect("armed slots must re-derive the component");
    assert_eq!(weapon.base_form_id, 0x9ABC);
    assert_eq!(weapon.inventory_index, InventoryIndex(2));
    assert_eq!(weapon.damage, 12.0);
}

#[test]
fn snapshot_resolves_metadata_and_equipped_state() {
    let (mut world, player) = fixture();
    world
        .get_mut::<EquipmentSlots>(player)
        .unwrap()
        .equip(1 << 12, InventoryIndex(0));

    let snapshot = snapshot(&world).unwrap();
    assert_eq!(snapshot.items.len(), 3);
    let armor = snapshot
        .items
        .iter()
        .find(|item| item.form_id == 0x1234)
        .unwrap();
    assert_eq!(armor.name, "Iron Armor");
    assert!(armor.equipped);
    assert!(armor.equippable);
    assert_eq!(snapshot.total_weight, 38.0);
    let metadata = world
        .resource::<InventoryCatalog>()
        .sdk_metadata(0x1234)
        .unwrap();
    assert_eq!(metadata.name(), "Iron Armor");
    assert_eq!(metadata.category(), ItemCategory::Armor);
    assert_eq!((metadata.value(), metadata.weight()), (125, 30.0));
}

#[test]
fn scroll_catalog_exposes_metadata_without_claiming_cast_support() {
    let mut index = EsmIndex::default();
    let item = byroredux_plugin::esm::records::parse_scrl(0x1234, &[], &None);
    index.items.insert(0x1234, item);
    let mut world = World::new();
    install_catalog(&mut world, &index);
    let catalog = world.resource::<InventoryCatalog>();
    let definition = &catalog.entries[&0x1234];
    assert_eq!(definition.category, "Scroll");
    assert!(definition.equip_target.is_none());
    assert!(definition.details.contains("casting unavailable"));
    assert_eq!(
        catalog.sdk_metadata(0x1234).unwrap().category(),
        ItemCategory::Scroll
    );
}

#[test]
fn carried_light_catalog_does_not_claim_equipping_support() {
    let mut index = EsmIndex::default();
    let mut data = vec![0; 48];
    data[12] = 2;
    let item = byroredux_plugin::esm::records::parse_carryable_light(
        0x1234,
        &[byroredux_plugin::esm::reader::SubRecord {
            sub_type: *b"DATA",
            data,
        }],
        GameKind::Skyrim,
        &None,
    )
    .unwrap();
    index.items.insert(0x1234, item);
    let mut world = World::new();
    install_catalog(&mut world, &index);
    let catalog = world.resource::<InventoryCatalog>();
    assert!(catalog.entries[&0x1234].equip_target.is_none());
    assert!(catalog.entries[&0x1234]
        .details
        .contains("equipping unavailable"));
    assert_eq!(
        catalog.sdk_metadata(0x1234).unwrap().category(),
        ItemCategory::Light
    );
}

#[test]
fn apparatus_catalog_exposes_metadata_without_claiming_crafting() {
    let mut index = EsmIndex::default();
    let item =
        byroredux_plugin::esm::records::parse_appa(0x1234, &[], GameKind::Oblivion, &None)
            .unwrap();
    index.items.insert(0x1234, item);
    let mut world = World::new();
    install_catalog(&mut world, &index);
    let catalog = world.resource::<InventoryCatalog>();
    assert!(catalog.entries[&0x1234].equip_target.is_none());
    assert!(catalog.entries[&0x1234]
        .details
        .contains("crafting unavailable"));
    assert_eq!(
        catalog.sdk_metadata(0x1234).unwrap().category(),
        ItemCategory::Apparatus
    );
}

#[test]
fn toggle_equip_mutates_only_supported_items() {
    let (mut world, player) = fixture();
    assert_eq!(
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 }
        ),
        MutationResult::Equipped
    );
    assert_eq!(
        world.get::<EquipmentSlots>(player).unwrap().at(12),
        Some(InventoryIndex(0))
    );
    assert_eq!(
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 }
        ),
        MutationResult::Unequipped
    );
    assert_eq!(world.get::<EquipmentSlots>(player).unwrap().at(12), None);
    assert!(world.get::<EquippedWeapon>(player).is_none());
    assert_eq!(
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 1 }
        ),
        MutationResult::Unavailable
    );
    assert_eq!(
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 2 }
        ),
        MutationResult::Equipped
    );
    let weapon = world.get::<EquippedWeapon>(player).unwrap();
    assert_eq!(weapon.inventory_index, InventoryIndex(2));
    assert_eq!(weapon.base_form_id, 0x9ABC);
    assert_eq!(weapon.damage, 12.0);
    assert_eq!(weapon.reach, 90.0);
    assert_eq!(weapon.speed, 1.1);
    drop(weapon);
    assert_eq!(
        world.get::<EquipmentSlots>(player).unwrap().weapon,
        Some(InventoryIndex(2))
    );
    assert_eq!(
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 2 }
        ),
        MutationResult::Unequipped
    );
    assert!(world.get::<EquippedWeapon>(player).is_none());
    assert_eq!(
        world
            .get::<byroredux_scripting::EquipmentEventBatch>(player)
            .unwrap()
            .0,
        vec![
            byroredux_scripting::EquipmentChange {
                item_form_id: 0x1234,
                equipped: true,
            },
            byroredux_scripting::EquipmentChange {
                item_form_id: 0x1234,
                equipped: false,
            },
            byroredux_scripting::EquipmentChange {
                item_form_id: 0x9ABC,
                equipped: true,
            },
            byroredux_scripting::EquipmentChange {
                item_form_id: 0x9ABC,
                equipped: false,
            },
        ]
    );
}

/// #4701 / #4707 — a `Dead` player's equip toggle is refused without
/// touching `EquipmentSlots`, and every refused menu action tells the
/// player so instead of only logging.
#[test]
fn refused_menu_actions_notify_and_a_dead_player_cannot_toggle_equipment() {
    use byroredux_core::ecs::components::Dead;
    let (mut world, player) = fixture();
    world.insert_resource(crate::notifications::PlayerNotifications::default());
    world.insert(player, Dead);

    let result = apply_action(
        &mut world,
        byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 },
    );
    assert_eq!(result, MutationResult::Unavailable);
    assert!(!world
        .get::<EquipmentSlots>(player)
        .unwrap()
        .is_equipped(InventoryIndex(0)));
    let result = apply_action(
        &mut world,
        byroredux_debug_ui::InventoryAction::Consume {
            index: 1,
            form_id: 0x5678,
        },
    );
    assert_eq!(result, MutationResult::Unavailable);
    assert_eq!(
        crate::notifications::drain(&world),
        vec![
            "Can't equip Iron Armor now".to_owned(),
            "Can't use Iron Arrow now".to_owned(),
        ]
    );

    world.remove::<Dead>(player);
    let result = apply_action(
        &mut world,
        byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 },
    );
    assert_eq!(result, MutationResult::Equipped);
    assert!(
        crate::notifications::drain(&world).is_empty(),
        "a successful toggle says nothing new"
    );
}

#[test]
fn fallout_categories_are_preserved_for_native_inventory() {
    assert_eq!(describe_kind(&ItemKind::Mod { was_junk: false }).0, "Mods");
    assert_eq!(describe_kind(&ItemKind::Junk).0, "Junk");
    assert_eq!(describe_kind(&ItemKind::Misc).0, "Misc");
}

// ── weapon slot vs. authored biped masks (#3112) ────────────────────

/// The headline regression: a Skyrim/FO4 ARMO whose `BOD2` sets bit 31
/// (body-part 61 / `FX01`) is a real authorable record. Pre-fix the
/// weapon lived at `occupants[31]`, so equipping such an armor wrote
/// over it and `reconcile_equipped_weapon` — reading that same bit and
/// finding an armor with `weapon_damage: None` — removed the player's
/// `EquippedWeapon`, silently dropping melee damage to `UNARMED_DAMAGE`.
#[test]
fn equipping_a_bit31_armor_does_not_unequip_the_weapon() {
    let (mut world, player) = fixture();
    // Re-author the armor with the exact mask that used to collide.
    world
        .try_resource_mut::<InventoryCatalog>()
        .unwrap()
        .entries
        .get_mut(&0x1234)
        .unwrap()
        .equip_target = Some(EquipTarget::BipedSlots(1 << 31));

    // Wield the sword first.
    assert_eq!(
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 2 }
        ),
        MutationResult::Equipped
    );
    assert!(world.get::<EquippedWeapon>(player).is_some());

    // Now equip the bit-31 armor — the weapon must survive.
    assert_eq!(
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 }
        ),
        MutationResult::Equipped
    );
    let weapon = world
        .get::<EquippedWeapon>(player)
        .expect("a bit-31 armor must not unequip the wielded weapon (#3112)");
    assert_eq!(weapon.inventory_index, InventoryIndex(2));
    assert_eq!(weapon.damage, 12.0);
    drop(weapon);

    // And the armor really did land in biped bit 31 — the two occupy
    // genuinely disjoint storage, not one displacing the other.
    let slots = world.get::<EquipmentSlots>(player).unwrap();
    assert_eq!(slots.at(31), Some(InventoryIndex(0)));
    assert_eq!(slots.weapon, Some(InventoryIndex(2)));
}

/// The reverse direction from the same finding: wielding a weapon must
/// not evict an armor that occupies bit 31.
#[test]
fn equipping_a_weapon_does_not_displace_a_bit31_armor() {
    let (mut world, player) = fixture();
    world
        .try_resource_mut::<InventoryCatalog>()
        .unwrap()
        .entries
        .get_mut(&0x1234)
        .unwrap()
        .equip_target = Some(EquipTarget::BipedSlots(1 << 31));

    apply_action(
        &mut world,
        byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 },
    );
    apply_action(
        &mut world,
        byroredux_debug_ui::InventoryAction::ToggleEquip { index: 2 },
    );

    let slots = world.get::<EquipmentSlots>(player).unwrap();
    assert_eq!(
        slots.at(31),
        Some(InventoryIndex(0)),
        "wielding a weapon must not displace an armor from biped bit 31"
    );
    assert_eq!(slots.weapon, Some(InventoryIndex(2)));
}

/// `describe_kind` is the only producer of `EquipTarget`, so the
/// collision is closed at the source: no `ItemKind::Armor` — whatever
/// its authored mask — can ever yield `EquipTarget::Weapon`.
#[test]
fn no_armor_describe_kind_output_can_target_the_weapon_slot() {
    for biped_flags in [1u32 << 31, u32::MAX, 0x8000_1000, 1, 0] {
        let (_, _, target) = describe_kind(&ItemKind::Armor {
            female_model_path: String::new(),
            biped_flags,
            dt: 0.0,
            dr: 0,
            health: 0,
            slot_mask: 0,
            armor_rating_x100: 0,
            armor_type: None,
            armatures: Vec::new(),
        });
        assert_ne!(
            target,
            Some(EquipTarget::Weapon),
            "armor with biped_flags {biped_flags:#010X} must never target the weapon slot",
        );
    }
}

/// An armor toggle must leave `EquippedWeapon` untouched entirely —
/// not merely "re-derive to the same value". Pins the `weapon_changed`
/// gate in `apply_action`.
#[test]
fn armor_toggles_never_touch_the_equipped_weapon() {
    let (mut world, player) = fixture();
    apply_action(
        &mut world,
        byroredux_debug_ui::InventoryAction::ToggleEquip { index: 2 },
    );
    let before = *world.get::<EquippedWeapon>(player).unwrap();

    // Equip then unequip the armor.
    apply_action(
        &mut world,
        byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 },
    );
    apply_action(
        &mut world,
        byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 },
    );

    let after = *world.get::<EquippedWeapon>(player).unwrap();
    assert_eq!(before.inventory_index, after.inventory_index);
    assert_eq!(before.base_form_id, after.base_form_id);
    assert_eq!(before.damage, after.damage);
}

#[test]
fn player_template_uses_authored_inventory_and_equipment_slots() {
    use byroredux_plugin::esm::records::common::CommonItemFields;
    use byroredux_plugin::esm::records::{ItemRecord, NpcInventoryEntry, NpcRecord};

    let armor_form = 0x0001_2345;
    let ammo_form = 0x0001_2346;
    let synthetic_player_form = 0x00AB_CDEF;
    let mut index = EsmIndex::default();
    index.npcs.insert(
        synthetic_player_form,
        NpcRecord {
            inventory: vec![
                NpcInventoryEntry {
                    item_form_id: armor_form,
                    count: 1,
                },
                NpcInventoryEntry {
                    item_form_id: ammo_form,
                    count: 20,
                },
            ],
            level: 1,
            ..Default::default()
        },
    );
    index.items.insert(
        armor_form,
        ItemRecord {
            form_id: armor_form,
            common: CommonItemFields::default(),
            kind: ItemKind::Armor {
                female_model_path: String::new(),
                biped_flags: 1 << 12,
                dt: 0.0,
                dr: 0,
                health: 0,
                slot_mask: 0,
                armor_rating_x100: 0,
                armor_type: None,
                armatures: Vec::new(),
            },
        },
    );
    index.items.insert(
        ammo_form,
        ItemRecord {
            form_id: ammo_form,
            common: CommonItemFields::default(),
            kind: ItemKind::Ammo {
                damage: 8.0,
                dt_mult: 1.0,
                spread: 0.0,
                projectile_form: 0,
                casing_form: 0,
                health: 0,
                clip_rounds: 0,
            },
        },
    );

    let template = build_player_template_for(&index, synthetic_player_form);
    assert_eq!(template.inventory.items.len(), 2);
    assert_eq!(
        template
            .inventory
            .items
            .iter()
            .find(|stack| stack.base_form_id == ammo_form)
            .unwrap()
            .count,
        20
    );
    assert_eq!(template.equipment.at(12), Some(InventoryIndex(0)));
}

#[test]
fn player_npc_form_id_is_canonical_across_supported_games() {
    for game in [
        GameKind::Oblivion,
        GameKind::Fallout3NV,
        GameKind::Skyrim,
        GameKind::Fallout4,
        GameKind::Fallout76,
        GameKind::Starfield,
    ] {
        assert_eq!(player_npc_form_id(game), 0x0000_0007, "{game:?}");
    }
}

/// #4458 — the player's CHARAL seed derives from the base Player
/// `NPC_` record through the same population path every NPC takes.
/// Pre-fix, `consume_item` / `apply_player_drowning_damage` gated on
/// the player carrying `ActorValues` + `ActorVitals` while no
/// production path stamped either, so every consumable use silently
/// returned `Unavailable`; the tests that covered those paths
/// hand-inserted the components. The seed must be a POPULATED
/// derivation (the scene.rs #3158 note's zero-SPECIAL warning is why
/// an empty stub was never acceptable), and it must degrade to `None`
/// — the pre-fix state — when the index has nothing to derive from.
#[test]
fn player_character_template_derives_from_the_player_npc_record() {
    use byroredux_core::character::CharacterRulesProfile;
    use byroredux_core::ecs::components::ActorVitals;
    use byroredux_plugin::esm::records::{AvifRecord, ClassRecord, NpcRecord};

    let mut index = EsmIndex {
        character_rules: CharacterRulesProfile::FALLOUT_NEW_VEGAS,
        game: GameKind::Fallout3NV,
        ..EsmIndex::default()
    };
    for (fid, name) in [
        (0x100u32, "AVStrength"),
        (0x101, "AVPerception"),
        (0x102, "AVEndurance"),
        (0x103, "AVCharisma"),
        (0x104, "AVIntelligence"),
        (0x105, "AVAgility"),
        (0x106, "AVLuck"),
        (0x2C9, "AVHealth"),
        (0x2D1, "AVActionPoints"),
    ] {
        index.actor_values.insert(
            fid,
            AvifRecord {
                form_id: fid,
                editor_id: name.to_owned(),
                ..Default::default()
            },
        );
    }
    index.classes.insert(
        0x2000,
        ClassRecord {
            form_id: 0x2000,
            base_attributes: [5, 6, 5, 4, 7, 6, 5],
            ..Default::default()
        },
    );
    index.npcs.insert(
        PLAYER_NPC_FORM_ID,
        NpcRecord {
            class_form_id: 0x2000,
            level: 1,
            ..Default::default()
        },
    );

    let template = build_player_character_template(&index);
    let values = template.values.expect("a populated derivation");
    let health = index.health_actor_value_key().expect("Health AVIF");
    assert_eq!(
        values.current(health),
        200.0,
        "FNV player Health 100 + 20·END(5) + 5·(L 1 − 1) = 200, the \
         player_only() ruleset row (#5342) — not the NPC curve (which \
         gives 20 here)"
    );
    assert_eq!(values.current(index.actor_value_form_id("Luck").unwrap()), 5.0);
    // #4674 — AP is PlayerOnly and was never seeded pre-fix (read 0.0);
    // the player formula 65 + 3·AGI(6) now applies at stamping.
    assert_eq!(
        values.current(index.actor_value_form_id("AVActionPoints").unwrap()),
        83.0,
        "FNV player AP = 65 + 3·AGI(6), evaluated from the PlayerOnly row"
    );
    assert_eq!(template.vitals, Some(ActorVitals { health }));

    // Degradation: no Player NPC_ in the index → no seed, never an
    // empty zero-SPECIAL component.
    let empty = build_player_character_template(&EsmIndex::default());
    assert!(empty.values.is_none());
    assert_eq!(empty.vitals, None);

    // #4699 — factions do not hinge on the derivation: a Player record
    // whose values derive to nothing (no AVIFs) still seeds them.
    let mut bare = EsmIndex::default();
    bare.npcs.insert(
        PLAYER_NPC_FORM_ID,
        NpcRecord {
            factions: vec![byroredux_plugin::esm::records::FactionMembership {
                faction_form_id: 0x1B2A4,
                rank: 0,
            }],
            ..Default::default()
        },
    );
    let seeded = build_player_character_template(&bare);
    assert!(seeded.values.is_none());
    assert_eq!(
        seeded.factions.map(|ranks| ranks.rank(0x1B2A4)),
        Some(Some(0))
    );
}

/// #5058 — the post-load reset gives the player the authored record
/// answer: a stale alias-injected membership (which the pre-load
/// session's refresh recorded as authored after the wholesale ledger
/// reset) is stripped, and a drifted rank on an authored faction
/// returns to the record value, so the first post-load alias refresh
/// re-injects only what the loaded save still binds.
#[test]
fn load_reset_strips_stale_alias_injected_player_factions() {
    use std::sync::Arc;

    use byroredux_core::ecs::components::FactionRanks;
    use byroredux_plugin::esm::records::NpcRecord;

    let mut index = EsmIndex::default();
    index.npcs.insert(
        PLAYER_NPC_FORM_ID,
        NpcRecord {
            factions: vec![byroredux_plugin::esm::records::FactionMembership {
                faction_form_id: 0x1B2A4,
                rank: 1,
            }],
            ..Default::default()
        },
    );
    let mut world = World::new();
    world.insert_resource(crate::cell_loader::LoadedCellIndex(Arc::new(index)));
    let player = world.spawn();
    // The pre-load session's live ranks: the authored faction drifted
    // to rank 5, and the alias-injected membership the save does not
    // carry sits beside it.
    world.insert(
        player,
        FactionRanks::from_pairs([(0x1B2A4, 5), (0x0000_DEAD, 0)]),
    );

    reset_player_factions_to_record(&mut world, player);

    {
        let ranks = world
            .get::<FactionRanks>(player)
            .expect("an authored membership re-seeds the component");
        assert_eq!(ranks.rank(0x1B2A4), Some(1), "record rank wins over session drift");
        assert_eq!(
            ranks.rank(0x0000_DEAD),
            None,
            "a stale alias-injected membership must not survive the load"
        );
    }

    // Degradation: a record seeding no factions removes the component
    // outright rather than leaving session residue behind.
    let mut bare = EsmIndex::default();
    bare.npcs.insert(PLAYER_NPC_FORM_ID, NpcRecord::default());
    world.insert_resource(crate::cell_loader::LoadedCellIndex(Arc::new(bare)));
    reset_player_factions_to_record(&mut world, player);
    assert!(world.get::<FactionRanks>(player).is_none());
}

/// #4458 — the production path end to end: `install_catalog` builds
/// the seed resource, `attach_to_player` stamps both components onto
/// the body. This is the leg every pre-fix test faked with
/// hand-inserted `ActorValues`/`ActorVitals`.
#[test]
fn attach_to_player_stamps_the_character_seed() {
    use byroredux_core::character::CharacterRulesProfile;
    use byroredux_core::ecs::components::{ActorValues, ActorVitals};
    use byroredux_plugin::esm::records::{
        AvifRecord, ClassRecord, FactionMembership, NpcRecord,
    };

    let mut index = EsmIndex {
        character_rules: CharacterRulesProfile::FALLOUT_NEW_VEGAS,
        game: GameKind::Fallout3NV,
        ..EsmIndex::default()
    };
    index.actor_values.insert(
        0x2C9,
        AvifRecord {
            form_id: 0x2C9,
            editor_id: "AVHealth".to_owned(),
            ..Default::default()
        },
    );
    index.classes.insert(
        0x2000,
        ClassRecord {
            form_id: 0x2000,
            base_attributes: [5, 5, 5, 5, 5, 5, 5],
            ..Default::default()
        },
    );
    index.npcs.insert(
        PLAYER_NPC_FORM_ID,
        NpcRecord {
            class_form_id: 0x2000,
            race_form_id: 0x0007,
            level: 1,
            factions: vec![FactionMembership {
                faction_form_id: 0x1B2A4,
                rank: 0,
            }],
            // #5920 — the Player record's PRKR perks.
            perks: vec![(0x000F_11A9, 1), (0x000C_F788, 1)],
            ..Default::default()
        },
    );

    // #4415 — a racial spell reaches the player's spell list.
    index.races.insert(
        0x0007,
        byroredux_plugin::esm::records::RaceRecord {
            form_id: 0x0007,
            spells: vec![0x0070],
            ..Default::default()
        },
    );
    index.spells.insert(
        0x0070,
        byroredux_plugin::esm::records::SpelRecord {
            form_id: 0x0070,
            ..Default::default()
        },
    );

    let mut world = World::new();
    install_catalog(&mut world, &index);
    let player = world.spawn();
    attach_to_player(&mut world, player);
    assert_eq!(
        world
            .get::<byroredux_scripting::SpellList>(player)
            .map(|l| l.0.clone()),
        Some(vec![0x0070])
    );
    // #4699 — the Player record's own factions ride the same attach.
    assert_eq!(
        world
            .get::<byroredux_core::ecs::components::FactionRanks>(player)
            .map(|ranks| ranks.rank(0x1B2A4)),
        Some(Some(0))
    );
    // #5920 — the Player record's perks reach the player through the
    // production attach (the unit tests used to hand-insert `Perks`, which
    // hid that nothing stamped it), so `HasPerk` and the perk entry
    // points can see them.
    let perks = world
        .get::<byroredux_core::character::Perks>(player)
        .expect("attach_to_player must stamp the Player record's perks");
    assert_eq!(
        perks.entries,
        vec![
            byroredux_core::character::PerkRank { perk_form_id: 0x000F_11A9, rank: 1 },
            byroredux_core::character::PerkRank { perk_form_id: 0x000C_F788, rank: 1 },
        ]
    );
    drop(perks);
    // #5413 — the racial SPLO subset rides the attach too, in the same
    // pair `stamp_spell_list` stamps on NPC roots. Pre-fix the player
    // (the only actor the #4415 lowering targets) never carried
    // `RaceSpells`, so both scripted entry points below were silent
    // no-ops on it — the exact writers-vs-stamps gap of #4458.
    assert_eq!(
        world
            .get::<byroredux_scripting::RaceSpells>(player)
            .map(|spells| spells.0.clone()),
        Some(vec![0x0070]),
        "attach_to_player must stamp the racial subset beside SpellList"
    );
    // End to end through the scripted pair, against a player built by
    // the production path (not one with hand-inserted components).
    assert!(
        byroredux_scripting::magic::remove_race_spells(&world, player),
        "RemoveRaceSpells must clear the attached racial set"
    );
    assert_eq!(
        world
            .get::<byroredux_scripting::SpellList>(player)
            .map(|list| list.0.clone()),
        Some(Vec::new()),
        "RemoveRaceSpells empties the player's list"
    );
    assert!(
        byroredux_scripting::magic::add_race_spells(&world, player),
        "AddRaceSpells must re-apply the attached racial set"
    );
    assert_eq!(
        world
            .get::<byroredux_scripting::SpellList>(player)
            .map(|list| list.0.clone()),
        Some(vec![0x0070]),
        "the round trip restores the player's racial spell"
    );

    let values = world
        .get::<ActorValues>(player)
        .expect("the player body must carry its derived actor values");
    // #5238 — the sourced NPC curve with no authored Base Health:
    // 0 + 5·(END 5−1) + 5·(L 1−1) = 20 (was the player curve's 200).
    assert_eq!(
        values.current(index.health_actor_value_key().unwrap()),
        20.0,
        "0 + 5·(END 5−1) + 5·(L 1−1)"
    );
    // #4984 — release before the ActorVitals read: production takes
    // `ActorVitals → ActorValues` (`commit_actor_value_deaths`).
    drop(values);
    assert_eq!(
        world.get::<ActorVitals>(player).map(|v| *v),
        Some(ActorVitals {
            health: index.health_actor_value_key().unwrap()
        })
    );
    // #4678 — level + Background ride the same attach, off the same
    // Player record the derivation consumed: GetLevel reads 1 (not 0),
    // GetXPForNextLevel computes for L1 (not L0), and the race/class
    // provenance is the record the values actually came from.
    assert_eq!(
        world.get::<byroredux_core::character::CharacterLevel>(player).map(|l| l.level),
        Some(1)
    );
    assert_eq!(
        world.get::<byroredux_core::character::Background>(player).map(|b| *b),
        Some(byroredux_core::character::Background {
            race_form_id: 0x0007,
            class_form_id: 0x2000,
        })
    );
}

#[test]
#[ignore = "needs FNV game data on disk; parses the whole master (~850 MB resident)"]
fn installed_fnv_player_base_builds_a_nonempty_template() {
    let esm = byroredux_plugin::esm::test_paths::fnv_esm();
    let Ok(bytes) = std::fs::read(&esm) else {
        byroredux_plugin::esm::test_paths::require_or_skip(&format!(
            "real-data #3099 regression: {}",
            esm.display()
        ));
        return;
    };
    let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse FalloutNV.esm");
    let player_form_id = player_npc_form_id(index.game);
    let player = index
        .npcs
        .get(&player_form_id)
        .expect("canonical Player NPC_ 00000007");

    assert_eq!(player.editor_id, "Player");
    assert!(
        !index.npcs.contains_key(&0x0000_0014),
        "00000014 is the placed player reference, not an NPC_ base",
    );
    assert!(
        !build_player_template(&index).inventory.items.is_empty(),
        "the authored Player inventory must reach the startup template",
    );
}

#[test]
fn vitals_snapshot_composes_layers_and_skips_absent_values() {
    let mut world = World::new();
    world.register::<byroredux_core::ecs::components::ActorValues>();
    let player = world.spawn();
    world.insert_resource(PlayerEntity(Some(player)));
    world.insert_resource(PlayerVitals {
        bars: vec![("Health", 1000), ("Magicka", 1001), ("Fatigue", 1002)],
        by_editor_id: FxHashMap::from_iter([
            ("Health", 1000),
            ("Magicka", 1001),
            ("Fatigue", 1002),
        ]),
        radiation: None,
    });
    let mut values = byroredux_core::ecs::components::ActorValues::new();
    values.set_base(1000, 100.0);
    values.apply_damage(1000, 35.0);
    values.set_base(1001, 50.0);
    values.mod_temporary(1001, 20.0);
    // 1002 (Fatigue) carries no entry at all — the absent-AV contract.
    world.insert(player, values);

    let bars = vitals_snapshot(&world).expect("a populated player must compose bars");
    assert_eq!(
        bars,
        vec![
            byroredux_debug_ui::VitalBarView {
                label: "Health",
                current: 65.0,
                max: 100.0
            },
            byroredux_debug_ui::VitalBarView {
                label: "Magicka",
                current: 70.0,
                max: 70.0
            },
        ],
        "current subtracts the damage layer; absent values drop their bar"
    );
}

/// #5508 — a routed player `SetBase` lives in `set_override`; the
/// vitals `max` must include it or the panel reports `base − damage`
/// (negative once damage passes the formula base).
#[test]
fn vitals_snapshot_max_includes_the_set_override_layer() {
    let mut world = World::new();
    world.register::<byroredux_core::ecs::components::ActorValues>();
    let player = world.spawn();
    world.insert_resource(PlayerEntity(Some(player)));
    world.insert_resource(PlayerVitals {
        bars: vec![("Health", 1000)],
        by_editor_id: FxHashMap::from_iter([("Health", 1000)]),
        radiation: None,
    });
    let mut values = byroredux_core::ecs::components::ActorValues::new();
    values.set_base(1000, 105.0);
    values.set_override(1000, 500.0);
    values.apply_damage(1000, 200.0);
    world.insert(player, values);

    let bars = vitals_snapshot(&world).expect("a populated player must compose bars");
    assert_eq!(
        bars,
        vec![byroredux_debug_ui::VitalBarView {
            label: "Health",
            current: 405.0,
            max: 605.0
        }],
    );
}

/// The FO3 / FNV rad meter accumulates: it reads the current rads
/// against the last sickness stage, shows an empty gauge before any rads
/// land (no `RadiationRads` entry yet), and pins at the full scale.
#[test]
fn vitals_snapshot_appends_the_rad_gauge_after_the_pools() {
    let mut world = World::new();
    world.register::<byroredux_core::ecs::components::ActorValues>();
    let player = world.spawn();
    world.insert_resource(PlayerEntity(Some(player)));
    world.insert_resource(PlayerVitals {
        bars: vec![("HP", 1000), ("AP", 1001)],
        by_editor_id: FxHashMap::from_iter([("Health", 1000), ("ActionPoints", 1001), ("RadiationRads", 0x5E4)]),
        radiation: Some(("RAD", 0x5E4, 1000.0)),
    });
    let mut values = byroredux_core::ecs::components::ActorValues::new();
    values.set_base(1000, 200.0);
    values.set_base(1001, 70.0);
    world.insert(player, values);
    let rad = |world: &World| vitals_snapshot(world).unwrap().pop().unwrap();
    assert_eq!(rad(&world), byroredux_debug_ui::VitalBarView { label: "RAD", current: 0.0, max: 1000.0 });

    world.get_mut::<byroredux_core::ecs::components::ActorValues>(player).unwrap().mod_temporary(0x5E4, 250.0);
    assert_eq!(rad(&world).current, 250.0);
    world.get_mut::<byroredux_core::ecs::components::ActorValues>(player).unwrap().mod_temporary(0x5E4, 5000.0);
    assert_eq!(rad(&world).current, 1000.0, "pinned at the fatal stage");
    assert_eq!(vitals_snapshot(&world).unwrap().len(), 3, "HP, AP, RAD");
}

/// #6428 — the HUD gauge and the affliction model read the rad pool from
/// the same layer, so one write moves both.
#[test]
fn the_rad_gauge_and_the_affliction_pool_read_one_write() {
    use byroredux_core::character::AfflictionTable;
    let mut world = World::new();
    world.register::<byroredux_core::ecs::components::ActorValues>();
    let player = world.spawn();
    world.insert_resource(PlayerEntity(Some(player)));
    world.insert_resource(PlayerVitals {
        bars: vec![("HP", 1000)],
        by_editor_id: FxHashMap::from_iter([("Health", 1000), ("RadiationRads", 0x5E4)]),
        radiation: Some(("RAD", 0x5E4, 1000.0)),
    });
    let mut values = byroredux_core::ecs::components::ActorValues::new();
    values.set_base(1000, 200.0);
    values.mod_temporary(0x5E4, 250.0);
    world.insert(player, values);
    let table = AfflictionTable { pool_avif: 0x5E4, bands: vec![] };
    let pool = table.pool_value(&world.get::<byroredux_core::ecs::components::ActorValues>(player).unwrap());
    let gauge = vitals_snapshot(&world).unwrap().pop().unwrap().current;
    assert_eq!((pool, gauge), (250.0, 250.0));
}

#[test]
fn vitals_snapshot_without_a_player_or_bars_is_none() {
    let mut world = World::new();
    assert!(vitals_snapshot(&world).is_none(), "no player resource");

    world.insert_resource(PlayerEntity(None));
    world.insert_resource(PlayerVitals::default());
    let player = world.spawn();
    world.insert_resource(PlayerEntity(Some(player)));
    assert!(
        vitals_snapshot(&world).is_none(),
        "no resolvable bars → the HUD draws nothing"
    );
}

#[test]
fn install_catalog_resolves_per_game_vital_keys() {
    use byroredux_core::character::CharacterRulesProfile;
    use byroredux_plugin::esm::records::AvifRecord;
    let avif = |form_id: u32, editor_id: &str| AvifRecord {
        form_id,
        editor_id: editor_id.to_owned(),
        ..Default::default()
    };
    // #4679 — the roster comes from the CHARACTER RULES PROFILE now,
    // not the GameKind match, so each fixture names its profile.
    let build = |game: GameKind, profile: CharacterRulesProfile| {
        let mut index = EsmIndex {
            game,
            character_rules: profile,
            ..Default::default()
        };
        index.actor_values.insert(0x3E8, avif(0x3E8, "AVHealth"));
        index.actor_values.insert(0x3E9, avif(0x3E9, "AVMagicka"));
        index.actor_values.insert(0x3EA, avif(0x3EA, "Fatigue"));
        index.actor_values.insert(0x3EB, avif(0x3EB, "ActionPoints"));
        index
    };

    let mut world = World::new();
    install_catalog(
        &mut world,
        &build(GameKind::Oblivion, CharacterRulesProfile::OBLIVION),
    );
    let labels: Vec<_> = world
        .try_resource::<PlayerVitals>()
        .unwrap()
        .bars
        .iter()
        .map(|&(label, _)| label)
        .collect();
    assert_eq!(labels, vec!["Health", "Magicka", "Fatigue"]);

    let mut world = World::new();
    install_catalog(
        &mut world,
        &build(GameKind::Fallout3NV, CharacterRulesProfile::FALLOUT_NEW_VEGAS),
    );
    let labels: Vec<_> = world
        .try_resource::<PlayerVitals>()
        .unwrap()
        .bars
        .iter()
        .map(|&(label, _)| label)
        .collect();
    assert_eq!(
        labels,
        vec!["HP", "AP"],
        "FO3/FNV vocabulary, and the Magicka/Fatigue AVIFs this game does not use drop out"
    );
}

/// #6426 — the RAD gauge comes from the profile row plus the master's
/// authored sickness stages: FNV installs it (scale = the highest stage
/// threshold), Skyrim never does, and an FNV load order without the
/// `RadiationRads` AVIF or without stages degrades to no gauge.
#[test]
fn install_catalog_installs_the_rad_gauge_only_where_the_profile_says() {
    use byroredux_core::character::CharacterRulesProfile;
    use byroredux_plugin::esm::records::{AvifRecord, RadiationStageRecord};
    let master = |game: GameKind, profile: CharacterRulesProfile, stages: bool| {
        let mut index = EsmIndex {
            game,
            character_rules: profile,
            ..Default::default()
        };
        index.actor_values.insert(
            0x5E4,
            AvifRecord {
                form_id: 0x5E4,
                editor_id: "RadiationRads".to_owned(),
                ..Default::default()
            },
        );
        if stages {
            for (id, threshold) in [(1u32, 200u32), (2, 1000)] {
                index.radiation_stages.insert(
                    id,
                    RadiationStageRecord { threshold, ..Default::default() },
                );
            }
        }
        index
    };
    let radiation = |index: &EsmIndex| {
        let mut world = World::new();
        install_catalog(&mut world, index);
        let vitals = world.try_resource::<PlayerVitals>().unwrap();
        vitals.radiation
    };

    let fnv = CharacterRulesProfile::FALLOUT_NEW_VEGAS;
    assert_eq!(
        radiation(&master(GameKind::Fallout3NV, fnv, true)),
        Some(("RAD", 0x5E4, 1000.0))
    );
    // No stages authored -> no scale -> no gauge (the degraded rule).
    assert_eq!(radiation(&master(GameKind::Fallout3NV, fnv, false)), None);
    // Skyrim carries no gauge row, whatever the master holds.
    assert_eq!(
        radiation(&master(GameKind::Skyrim, CharacterRulesProfile::SKYRIM, true)),
        None
    );
}

/// #4674 (CHAR-2026-09-21-D4-01) — the real-master leg. Reads the
/// actual Player `NPC_` 0x7 records: FO4's PRPS authors Health 40 /
/// ActionPoints 0 and DNAM pushes calc_health 150 / calc_ap 100 after
/// them, so the pre-fix stamp (NPC answer verbatim) left the player at
/// Health 150 / AP 100 — 1.76×/1.43× off the capture's player formulas
/// (85 / 70). FNV's ActionPoints was never seeded at all (read 0.0;
/// the capture gives 80 at AGI 5). Run with
/// `cargo test -p byroredux --bin byroredux
///  real_master_player_seed -- --ignored`.
#[test]
#[ignore = "requires installed Fallout 4 and New Vegas masters"]
fn real_master_player_seed_evaluates_the_player_only_rows() {
    let fo4 = byroredux_plugin::esm::test_paths::fo4_esm();
    let index = byroredux_plugin::esm::parse_esm(&std::fs::read(fo4).unwrap()).unwrap();
    let template = build_player_character_template(&index);
    let values = template.values.expect("FO4 player seed");
    let health = index.actor_value_form_id("Health").expect("Health AVIF");
    let ap = index.actor_value_form_id("ActionPoints").expect("AP AVIF");
    assert_eq!(
        values.current(health),
        85.0,
        "FO4 player Health must come from the capture's player formula, \
         not the NPC-baked 150 the PRPS/DNAM push order produced"
    );
    assert_eq!(
        values.current(ap),
        70.0,
        "FO4 player AP must come from the capture's player formula, \
         not the NPC-baked 100"
    );

    let fnv = byroredux_plugin::esm::test_paths::fnv_esm();
    let index = byroredux_plugin::esm::parse_esm(&std::fs::read(fnv).unwrap()).unwrap();
    let template = build_player_character_template(&index);
    let values = template.values.expect("FNV player seed");
    let ap = index.actor_value_form_id("ActionPoints").expect("AP AVIF");
    assert_eq!(
        values.current(ap),
        80.0,
        "FNV player AP = 65 + 3·AGI(5) — pre-#4674 it was never seeded"
    );
}

/// #4675 — the HUD's bar keys resolve through `PlayerVitals` (real
/// FNV AVIF ids `AVHealth 0x450` / `AVActionPoints 0x44C` here, not
/// the fallout.rs test-fixture 0x2C9/0x2D0 the drivers used to
/// hardcode), and `fraction` reads the PLAYER only: an NPC inserted
/// first — whose values the old first-hit storage scan would have
/// reported — must not move the bar.
#[test]
fn hud_bars_resolve_through_player_vitals_and_read_the_player_only() {
    use crate::hud::fraction;
    use byroredux_core::character::CharacterRulesProfile;
    use byroredux_core::ecs::components::ActorValues;
    use byroredux_plugin::esm::records::AvifRecord;

    let mut index = EsmIndex {
        character_rules: CharacterRulesProfile::FALLOUT_NEW_VEGAS,
        game: GameKind::Fallout3NV,
        ..EsmIndex::default()
    };
    for (fid, name) in [(0x450u32, "AVHealth"), (0x44C, "AVActionPoints")] {
        index.actor_values.insert(
            fid,
            AvifRecord {
                form_id: fid,
                editor_id: name.to_owned(),
                ..Default::default()
            },
        );
    }
    let vitals = build_player_vitals(&index);
    assert_eq!(vitals.resolved("Health"), Some(0x450));
    assert_eq!(vitals.resolved("ActionPoints"), Some(0x44C));
    assert_eq!(vitals.resolved("Magicka"), None, "FNV authors no Magicka");

    let mut world = World::new();
    world.register::<ActorValues>();
    // An NPC carrying Health, inserted FIRST — the pre-#4675 scan
    // would have taken this dense-slot-first entry.
    let npc = world.spawn();
    let mut npc_values = ActorValues::new();
    npc_values.set_base(0x450, 10.0);
    world.insert(npc, npc_values);
    // The player, at half health.
    let player = world.spawn();
    let mut player_values = ActorValues::new();
    player_values.set_base(0x450, 100.0);
    player_values.apply_damage(0x450, 50.0);
    player_values.set_base(0x44C, 80.0);
    world.insert(player, player_values);
    world.insert_resource(PlayerEntity(Some(player)));
    world.insert_resource(vitals);

    let health = index.actor_value_form_id("Health").unwrap();
    assert_eq!(
        fraction(&world, Some(health), None),
        0.5,
        "the bar must read the player's 50/100, not the first-scanned \
         NPC's 10/10"
    );
    // Unpinned absent key → full bar; pins still win.
    assert_eq!(fraction(&world, Some(0x999), None), 1.0);
    assert_eq!(fraction(&world, Some(health), Some(0.3)), 0.3);
}

/// #4818 — a taken item's collision entities are standalone (never
/// descendants), so the `PickedUp` subtree marker cannot reach them.
/// `pickup_loot` must sweep the bodies out of the `PhysicsWorld`, or
/// the front bottle of a shelf row stays an invisible solid that
/// blocks the aim ray to the item behind it and can be shoved around.
#[test]
fn pickup_removes_the_taken_item_collision_bodies() {
    use byroredux_core::ecs::components::{
        CollisionShape, FormIdComponent, GlobalTransform, PhysicsSourceForm, RigidBodyData,
    };
    use byroredux_core::ecs::{EntityId, Transform};
    use byroredux_core::math::{Quat, Vec3};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_core::ecs::resources::ItemInstancePool;
    use byroredux_physics::{PhysicsWorld, RapierHandles};
    use byroredux_scripting::SceneAliasCandidate;

    let mut world = World::new();
    world.register::<Transform>();
    world.register::<GlobalTransform>();
    world.register::<CollisionShape>();
    world.register::<RigidBodyData>();
    world.register::<RapierHandles>();
    world.register::<PickedUp>();
    world.register::<Inventory>();
    world.register::<FormIdComponent>();
    world.register::<PhysicsSourceForm>();
    world.register::<SceneAliasCandidate>();
    world.insert_resource(PhysicsWorld::new());
    world.insert_resource(FormIdPool::new());
    world.insert_resource(ItemInstancePool::new());
    world.insert_resource(
        crate::cell_loader::reference_state::PersistentReferenceStates::default(),
    );

    let id = world
        .resource_mut::<FormIdPool>()
        .intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV"),
            local: LocalFormId(0x1234),
        });

    // The player and the placement.
    let player = world.spawn();
    world.insert(player, Inventory::new());
    let target = world.spawn();
    world.insert(target, FormIdComponent(id));
    world.insert(
        target,
        SceneAliasCandidate {
            reference_form_id: 0xC0DE_1234,
            base_form_id: 0x5678,
            ..Default::default()
        },
    );

    // The placement's collision proxy: standalone, tagged with the
    // placement's stable form id, and registered into Rapier exactly
    // as `physics_sync_system` would.
    let collider: EntityId = world.spawn();
    let position = Vec3::new(1.0, 0.5, 2.0);
    world.insert(collider, Transform::new(position, Quat::IDENTITY, 1.0));
    world.insert(
        collider,
        GlobalTransform {
            translation: position,
            ..GlobalTransform::IDENTITY
        },
    );
    world.insert(
        collider,
        CollisionShape::Cuboid {
            half_extents: Vec3::new(4.0, 4.0, 4.0),
        },
    );
    world.insert(collider, RigidBodyData::static_body());
    world.insert(collider, PhysicsSourceForm(id));

    // A second item behind the first, in the aim line, with its own
    // placement and collider.
    let behind = world.spawn();
    let behind_id = world.resource_mut::<FormIdPool>().intern(FormIdPair {
        plugin: PluginId::from_filename("FalloutNV"),
        local: LocalFormId(0x2345),
    });
    world.insert(behind, FormIdComponent(behind_id));
    world.insert(
        behind,
        SceneAliasCandidate {
            reference_form_id: 0xC0DE_2345,
            base_form_id: 0x5678,
            ..Default::default()
        },
    );

    assert_eq!(
        byroredux_physics::register_newcomers_and_refresh_queries(&world),
        1,
        "the fixture's collider must register"
    );
    assert_eq!(world.resource::<PhysicsWorld>().body_count(), 1);

    // The catalog must know the base for the item to be a pickup
    // target.
    let mut entries = rustc_hash::FxHashMap::default();
    entries.insert(
        0x5678u32,
        InventoryItemDefinition {
            name: "Bottle".to_string(),
            category: "Misc",
            value: 1,
            weight: 0.1,
            details: String::new(),
            equip_target: None,
            weapon_damage: None,
            weapon_reach: 0.0,
            weapon_speed: 0.0,
        },
    );
    world.insert_resource(InventoryCatalog {
        restorations: rustc_hash::FxHashMap::default(),
        script_effects: rustc_hash::FxHashMap::default(),
        entries,
        containers: rustc_hash::FxHashSet::default(),
        player_base_form_id: 0x14,
    });

    assert!(pickup_loot(&world, player, target), "the front item must be pickable");
    assert_eq!(
        world.resource::<PhysicsWorld>().body_count(),
        0,
        "the taken item's body must leave the solver"
    );
    // The behind item was never picked up (no `PersistentReferenceStates`
    // restock here — just the collider bookkeeping): its collider never
    // existed, and the taken item's body is the only one that left.
    assert!(!pickup_loot(&world, player, target), "already picked up");
    assert_eq!(
        world.resource::<PhysicsWorld>().body_count(),
        0,
        "the taken item's body stays gone"
    );
}

#[test]
fn add_item_merges_same_base_and_appends_new_rows() {
    let (world, player) = fixture();
    let first = add_item(&world, 0xABCD, 2).expect("player inventory present");
    let merged = add_item(&world, 0xABCD, 1).expect("player inventory present");
    assert_eq!(
        first, merged,
        "a same-base add must merge, not grow a second row"
    );
    let appended = add_item(&world, 0xDBBA, 1).expect("player inventory present");
    assert_ne!(appended, first, "a new base lands in its own row");
    let inventory = world.get::<Inventory>(player).unwrap();
    assert_eq!(inventory.items.len(), 5);
    let merged_row = &inventory.items[first.0 as usize];
    assert_eq!((merged_row.base_form_id, merged_row.count), (0xABCD, 3));
}

#[test]
fn queued_equip_drains_through_the_canonical_action() {
    let (mut world, player) = fixture();
    world.insert_resource(PendingInventoryActions::default());

    // The fixture's row 0 is the Iron Armor (equip target biped 1<<12).
    let message =
        queue_equip_by_form_id(&world, 0x1234).expect("the row exists");
    assert!(message.contains("row 0"), "{message}");
    assert!(
        queue_equip_by_form_id(&world, 0xDEAD).is_err(),
        "a form with no row must be refused at the queue, not silently dropped"
    );
    assert_eq!(world.resource::<PendingInventoryActions>().0.len(), 1);

    drain_pending_inventory_actions(&mut world);
    assert!(
        world.resource::<PendingInventoryActions>().0.is_empty(),
        "the drain empties the queue"
    );
    let slots = world.get::<EquipmentSlots>(player).unwrap();
    assert_eq!(slots.occupants[12].map(|index| index.0), Some(0));
    let batch = world
        .get::<byroredux_scripting::EquipmentEventBatch>(player)
        .expect("toggle_equip emitted the canonical batch");
    assert!(
        batch.0.iter().any(|change| change.item_form_id == 0x1234 && change.equipped),
        "the canonical batch is what the mid-life gear detection reads"
    );
}

/// One ALCH (form 10) of scripted effects (script 0x500 each), header and per-effect shape varied.
fn scripted_potion(simple_header: bool, effects: Vec<byroredux_plugin::esm::records::items::ConsumableEffect>) -> EsmIndex {
    let mut index = EsmIndex { game: byroredux_plugin::esm::reader::GameKind::Fallout3NV, ..Default::default() };
    index.items.insert(
        10,
        byroredux_plugin::esm::records::ItemRecord {
            form_id: 10,
            common: Default::default(),
            kind: ItemKind::Aid {
                magic_effects: Vec::new(),
                addiction_chance: 0.0,
                immediate_effects: None,
                authored_effects: Some(effects),
                simple_consumption_header: simple_header,
                medicine: false,
                food: false,
            },
        },
    );
    index
}

fn effect(script: u32) -> byroredux_plugin::esm::records::items::ConsumableEffect {
    byroredux_plugin::esm::records::items::ConsumableEffect {
        effect: byroredux_plugin::esm::records::MagicEffectItem {
            effect_form_id: 20,
            magnitude: 5.0,
            script_form_id: script,
            ..Default::default()
        },
        delivery: Some(0),
        actor_value_cache: None,
        conditions: Vec::new(),
        condition_flags: Vec::new(),
    }
}

/// #6229 — the caster path keeps the restoration plan's all-or-nothing rule: a poison/addiction
/// header, an effect-local CTDA, a non-self delivery, or one unmodelled sibling refuses the item.
#[test]
fn caster_effects_refuse_what_the_runtime_cannot_play_whole() {
    let ok = scripted_potion(true, vec![effect(0x500), effect(0x501)]);
    assert_eq!(caster_effects(&ok, 10, false).map(|effects| effects.len()), Some(2));

    let poison = scripted_potion(false, vec![effect(0x500)]);
    assert!(caster_effects(&poison, 10, false).is_none(), "a non-simple header is refused");

    let mut gated = effect(0x500);
    gated.conditions.push(Default::default());
    gated.condition_flags.push(0);
    assert!(caster_effects(&scripted_potion(true, vec![gated]), 10, false).is_none(), "a CTDA is refused");

    let mut touch = effect(0x500);
    touch.delivery = Some(1);
    assert!(caster_effects(&scripted_potion(true, vec![touch]), 10, false).is_none(), "touch is refused");

    let partial = scripted_potion(true, vec![effect(0x500), effect(0)]);
    assert!(caster_effects(&partial, 10, false).is_none(), "an unmodelled sibling refuses the item");
    // Planned: the plan applies the non-scripted effect, the caster runs only the script.
    assert_eq!(caster_effects(&partial, 10, true).map(|effects| effects.len()), Some(1));
}

/// #6229 — an installed catalog lists a refused item as neither consumable nor described as usable.
#[test]
fn a_refused_caster_item_is_unavailable_in_the_catalog() {
    let mut world = World::new();
    install_catalog(&mut world, &scripted_potion(false, vec![effect(0x500)]));
    let catalog = world.resource::<InventoryCatalog>();
    assert!(!catalog.script_effects.contains_key(&10) && !catalog.restorations.contains_key(&10));
    assert_eq!(catalog.entries[&10].details, "Consumable (effects unavailable)");
}
