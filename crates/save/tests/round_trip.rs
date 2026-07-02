//! End-to-end save → encode → decode → restore round-trip on a realistic
//! synthetic World, plus the validation pass.
//!
//! Proves the data-model half of M45: a snapshot taken from one World and
//! restored into a fresh, empty World reproduces entity ids, the string
//! pool, hierarchy edges, inventory/equipment, stable form ids, and
//! `next_entity` exactly.

use byroredux_core::ecs::components::{
    Children, EquipmentSlots, FormIdComponent, Inventory, InventoryIndex, ItemStack, Name, Parent,
    Transform,
};
use byroredux_core::ecs::world::World;
use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
use byroredux_core::string::StringPool;
use byroredux_save::validate::{validate_world, ValidationKind};
use byroredux_save::{decode, encode, restore_world, save_world, SaveError, SaveRegistry};
use glam::{Quat, Vec3};

/// The curated game-state registry the test (and, later, the binary) uses.
fn registry() -> SaveRegistry {
    let mut r = SaveRegistry::new();
    r.register_component::<Transform>("Transform")
        .register_component::<Name>("Name")
        .register_component::<Parent>("Parent")
        .register_component::<Children>("Children")
        .register_component::<Inventory>("Inventory")
        .register_component::<EquipmentSlots>("EquipmentSlots")
        .register_form_id_component("FormIdComponent");
    r
}

/// Build a World with a sparse entity-id layout (a despawn leaves a gap),
/// a hierarchy, named entities, and an actor with inventory + equipment +
/// a stable form id.
fn build_source_world() -> (World, FormIdPair) {
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.insert_resource(FormIdPool::new());

    // Spawn a handful, then despawn one so ids 0..N are sparse — exercises
    // the "preserve original ids, gaps and all" guarantee.
    let root = world.spawn(); // 0
    let child_a = world.spawn(); // 1
    let doomed = world.spawn(); // 2
    let child_b = world.spawn(); // 3
    let actor = world.spawn(); // 4
    world.despawn(doomed); // id 2 now a permanent gap

    // Transforms (PackedStorage + change tracking).
    world.insert(root, Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)));
    world.insert(
        actor,
        Transform::new(Vec3::new(10.0, 0.0, -5.0), Quat::from_rotation_y(0.5), 2.0),
    );

    // Names (StringPool-backed FixedString).
    let (root_name, actor_name) = {
        let mut pool = world.resource_mut::<StringPool>();
        (pool.intern("Scene Root"), pool.intern("Doc Mitchell"))
    };
    world.insert(root, Name(root_name));
    world.insert(actor, Name(actor_name));

    // Hierarchy: root → [child_a, child_b]; actor is unparented.
    world.insert(child_a, Parent(root));
    world.insert(child_b, Parent(root));
    world.insert(root, Children(vec![child_a, child_b]));

    // Actor inventory + equipment.
    let pair = FormIdPair {
        plugin: PluginId::from_filename("FalloutNV.esm"),
        local: LocalFormId(0x0014),
    };
    let mut inv = Inventory::new();
    let idx = inv.push(ItemStack::new(0xDEAD, 1));
    inv.push(ItemStack::new(0xBEEF, 5));
    world.insert(actor, inv);
    let mut equip = EquipmentSlots::new();
    equip.equip(0b1, idx); // bit 0 → inventory slot 0
    world.insert(actor, equip);

    // Stable form id on the actor.
    let fid = {
        let mut pool = world.resource_mut::<FormIdPool>();
        pool.intern(pair)
    };
    world.insert(actor, FormIdComponent(fid));

    (world, pair)
}

#[test]
fn full_world_round_trips_through_container() {
    let (src, pair) = build_source_world();
    let reg = registry();

    // Save → encode → decode → restore into a brand-new, empty World.
    let snapshot = save_world(&src, &reg).expect("save");
    let bytes = encode(&snapshot, reg.schema_fingerprint()).expect("encode");
    let decoded = decode(&bytes, reg.schema_fingerprint()).expect("decode");

    let mut dst = World::new();
    // The destination needs a FormIdPool present for FormIdComponent
    // re-interning (the live engine always has one; a fresh load must too).
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).expect("restore");

    // next_entity preserved (high-water mark = 5 even with the id-2 gap).
    assert_eq!(dst.next_entity_id(), src.next_entity_id());
    assert_eq!(dst.next_entity_id(), 5);

    // Transforms preserved at their original ids.
    {
        let q = dst.query::<Transform>().expect("transform storage");
        let map: std::collections::HashMap<u32, Transform> =
            q.iter().map(|(e, t)| (e, *t)).collect();
        assert_eq!(map.len(), 2);
        assert_eq!(map[&0].translation, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(map[&4].translation, Vec3::new(10.0, 0.0, -5.0));
        assert_eq!(map[&4].scale, 2.0);
    }

    // Names resolve to the same strings through the restored StringPool.
    {
        let pool = dst.resource::<StringPool>();
        let q = dst.query::<Name>().expect("name storage");
        let names: std::collections::HashMap<u32, String> = q
            .iter()
            .map(|(e, n)| (e, pool.resolve(n.0).unwrap().to_string()))
            .collect();
        assert_eq!(names[&0], "scene root"); // pool lowercases canonically
        assert_eq!(names[&4], "doc mitchell");
    }

    // Hierarchy edges preserved.
    {
        let qp = dst.query::<Parent>().expect("parent storage");
        let parents: std::collections::HashMap<u32, u32> =
            qp.iter().map(|(e, p)| (e, p.0)).collect();
        assert_eq!(parents[&1], 0);
        assert_eq!(parents[&3], 0);

        let qc = dst.query::<Children>().expect("children storage");
        let children: Vec<u32> = qc.iter().next().unwrap().1 .0.clone();
        assert_eq!(children, vec![1, 3]);
    }

    // Inventory + equipment preserved.
    {
        let qi = dst.query::<Inventory>().expect("inventory storage");
        let (e, inv) = qi.iter().next().unwrap();
        assert_eq!(e, 4);
        assert_eq!(inv.items.len(), 2);
        assert_eq!(inv.items[0].base_form_id, 0xDEAD);
        assert_eq!(inv.items[1].count, 5);

        let qe = dst.query::<EquipmentSlots>().expect("equip storage");
        let (_, equip) = qe.iter().next().unwrap();
        assert_eq!(equip.occupants[0], Some(InventoryIndex(0)));
    }

    // FormIdComponent resolves back to the SAME stable pair through the
    // destination pool (handle value is session-local and may differ).
    {
        let pool = dst.resource::<FormIdPool>();
        let qf = dst.query::<FormIdComponent>().expect("formid storage");
        let (e, comp) = qf.iter().next().unwrap();
        assert_eq!(e, 4);
        assert_eq!(pool.resolve(comp.0).copied(), Some(pair));
    }
}

#[test]
fn empty_columns_are_omitted_from_the_snapshot() {
    // A world with only a StringPool and no entities should produce a
    // snapshot with no component columns at all.
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    let reg = registry();
    let snap = save_world(&world, &reg).unwrap();
    assert_eq!(snap.row_count(), 0);
    assert!(snap.components.is_empty());
}

#[test]
fn validation_catches_equipment_out_of_bounds() {
    let mut world = World::new();
    let a = world.spawn();
    let mut inv = Inventory::new();
    inv.push(ItemStack::new(1, 1)); // single slot, index 0 only
    world.insert(a, inv);
    let mut equip = EquipmentSlots::new();
    equip.equip(0b1, InventoryIndex(7)); // points past the inventory
    world.insert(a, equip);

    let errors = validate_world(&world);
    assert!(errors
        .iter()
        .any(|e| e.kind == ValidationKind::Equipment && e.entity == a));
}

#[test]
fn validation_catches_dangling_parent() {
    let mut world = World::new();
    let child = world.spawn(); // id 0, next_entity = 1
    world.insert(child, Parent(99)); // 99 never spawned

    let errors = validate_world(&world);
    assert!(errors
        .iter()
        .any(|e| e.kind == ValidationKind::DanglingEntity && e.entity == child));
}

#[test]
fn validation_passes_on_a_consistent_world() {
    let (world, _pair) = build_source_world();
    assert_eq!(
        validate_world(&world),
        vec![],
        "the hand-built world must be referentially consistent"
    );
}

/// M45.1 — the FormId-keyed delta apply: a saved session and a freshly
/// reloaded cell carry the *same* form ids on *different* entity ids;
/// saved Transform/Inventory deltas must land on the matching live
/// entity, not the (stale) saved id.
#[test]
fn delta_apply_reroutes_by_form_id_after_cell_reload() {
    use byroredux_save::{apply_deltas, build_form_id_remap};

    // Two distinct form ids for two cell objects.
    let pair_a = FormIdPair {
        plugin: PluginId::from_filename("FalloutNV.esm"),
        local: LocalFormId(0x0A),
    };
    let pair_b = FormIdPair {
        plugin: PluginId::from_filename("FalloutNV.esm"),
        local: LocalFormId(0x0B),
    };

    // ── "Saved session": objects at ids 5 and 6, A moved + given loot. ──
    let mut saved_world = World::new();
    saved_world.insert_resource(StringPool::new());
    saved_world.insert_resource(FormIdPool::new());
    for _ in 0..5 {
        saved_world.spawn();
    }
    let s_a = saved_world.spawn(); // 5
    let s_b = saved_world.spawn(); // 6
    saved_world.insert(s_a, Transform::from_translation(Vec3::new(100.0, 0.0, 0.0)));
    let mut inv = Inventory::new();
    inv.push(ItemStack::new(0xCAFE, 3));
    saved_world.insert(s_a, inv);
    saved_world.insert(s_b, Transform::from_translation(Vec3::new(0.0, 50.0, 0.0)));
    for (e, pair) in [(s_a, pair_a), (s_b, pair_b)] {
        let fid = saved_world.resource_mut::<FormIdPool>().intern(pair);
        saved_world.insert(e, FormIdComponent(fid));
    }

    let reg = registry();
    let snapshot = save_world(&saved_world, &reg).unwrap();

    // ── "Reloaded cell": SAME form ids, DIFFERENT ids (reverse order, no
    //    gaps), authored Transforms not yet reflecting the saved deltas. ──
    let mut live = World::new();
    live.insert_resource(FormIdPool::new());
    let l_b = live.spawn(); // 0  (note: B spawns first here)
    let l_a = live.spawn(); // 1
    live.insert(l_a, Transform::from_translation(Vec3::new(1.0, 1.0, 1.0)));
    live.insert(l_b, Transform::from_translation(Vec3::new(2.0, 2.0, 2.0)));
    for (e, pair) in [(l_a, pair_a), (l_b, pair_b)] {
        let fid = live.resource_mut::<FormIdPool>().intern(pair);
        live.insert(e, FormIdComponent(fid));
    }

    // Build the remap and apply only the mutable delta columns.
    let remap = build_form_id_remap(&live, &reg, &snapshot);
    assert_eq!(remap.get(&s_a), Some(&l_a), "saved A → live A");
    assert_eq!(remap.get(&s_b), Some(&l_b), "saved B → live B");

    let applied = apply_deltas(
        &mut live,
        &reg,
        &snapshot,
        &remap,
        &["Transform", "Inventory"],
    )
    .unwrap();
    assert_eq!(applied, 3, "2 transforms + 1 inventory");

    // The saved deltas landed on the correct live entities.
    let qt = live.query::<Transform>().unwrap();
    let tf: std::collections::HashMap<u32, Vec3> =
        qt.iter().map(|(e, t)| (e, t.translation)).collect();
    assert_eq!(tf[&l_a], Vec3::new(100.0, 0.0, 0.0), "A's saved move applied to live A");
    assert_eq!(tf[&l_b], Vec3::new(0.0, 50.0, 0.0), "B's saved move applied to live B");

    let qi = live.query::<Inventory>().unwrap();
    let (e, inv) = qi.iter().next().unwrap();
    assert_eq!(e, l_a, "loot applied to live A, not the stale saved id");
    assert_eq!(inv.items[0].base_form_id, 0xCAFE);
}

/// #1846 / SAVE-03 — the player character body must carry a
/// `FormIdComponent` built from the reserved `PLAYER_FORM_ID_PAIR` so it
/// is a normal remappable entity for the live-load delta-apply path,
/// exactly like any NPC's `FormIdComponent`. Before the fix the player
/// body had no form id at all: `build_form_id_remap` had no pair to
/// match it against, so any saved delta targeting the player (inventory,
/// equipment) was silently dropped by `apply_deltas`'s `filter_map` on
/// every live load — the single worst data-loss class for a save
/// system, arriving invisibly. This test would fail pre-fix (the
/// `remap.get(&saved_player)` lookup would be `None` and 0 rows would
/// apply).
#[test]
fn player_body_inventory_survives_live_load() {
    use byroredux_core::form_id::PLAYER_FORM_ID_PAIR;
    use byroredux_save::{apply_deltas, build_form_id_remap};

    // ── "Saved session": the player, given a sword mid-session. ──
    let mut saved_world = World::new();
    saved_world.insert_resource(StringPool::new());
    saved_world.insert_resource(FormIdPool::new());
    let saved_player = saved_world.spawn();
    let mut inv = Inventory::new();
    inv.push(ItemStack::new(0xBEEF, 1));
    saved_world.insert(saved_player, inv);
    let fid = saved_world
        .resource_mut::<FormIdPool>()
        .intern(PLAYER_FORM_ID_PAIR);
    saved_world.insert(saved_player, FormIdComponent(fid));

    let reg = registry();
    let snapshot = save_world(&saved_world, &reg).unwrap();

    // ── "Reloaded cell": the player respawns at a DIFFERENT entity id,
    //    with no inventory yet — same as any post-reload player spawn.
    //    `scene::setup_scene` attaches the same PLAYER_FORM_ID_PAIR. ──
    let mut live = World::new();
    live.insert_resource(FormIdPool::new());
    let _other_entity = live.spawn(); // shifts the player off id 0
    let live_player = live.spawn();
    let fid = live
        .resource_mut::<FormIdPool>()
        .intern(PLAYER_FORM_ID_PAIR);
    live.insert(live_player, FormIdComponent(fid));

    let remap = build_form_id_remap(&live, &reg, &snapshot);
    assert_eq!(
        remap.get(&saved_player),
        Some(&live_player),
        "the player's stable form id must resolve saved → live across the reload"
    );

    let applied = apply_deltas(&mut live, &reg, &snapshot, &remap, &["Inventory"]).unwrap();
    assert_eq!(applied, 1, "the saved player Inventory delta must apply");

    let qi = live.query::<Inventory>().unwrap();
    let (e, inv) = qi.iter().next().unwrap();
    assert_eq!(e, live_player, "inventory landed on the live player entity");
    assert_eq!(inv.items[0].base_form_id, 0xBEEF, "the saved item survived the live load");
}

/// #1696 — `apply_deltas` remaps each row's entity *key* (saved id → live id)
/// but moves the component *value* verbatim. `AnimationPlayer.root_entity` is
/// an `Option<EntityId>` holding a *saved-session* id; overlaying it would
/// clobber the *fresh* `root_entity` the reloaded cell already set with a
/// stale one. This test proves both halves: overlaying the column corrupts the
/// live value (the bug), and the binary's fix — excluding it from the overlay
/// set — preserves the cell-owned value.
#[test]
fn anim_player_root_entity_not_clobbered_by_delta_apply() {
    use byroredux_core::animation::AnimationPlayer;
    use byroredux_save::{apply_deltas, build_form_id_remap};

    fn registry_with_anim() -> SaveRegistry {
        let mut r = SaveRegistry::new();
        r.register_component::<AnimationPlayer>("AnimationPlayer")
            .register_form_id_component("FormIdComponent");
        r
    }

    let pair = FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x0A),
    };

    // ── "Saved session": animated object at id 6, scoped to subtree root 4. ──
    let mut saved_world = World::new();
    saved_world.insert_resource(StringPool::new());
    saved_world.insert_resource(FormIdPool::new());
    for _ in 0..6 {
        saved_world.spawn();
    }
    let s_obj = saved_world.spawn(); // 6
    saved_world.insert(s_obj, AnimationPlayer::new(3).with_root(4)); // stale root id 4
    let fid = saved_world.resource_mut::<FormIdPool>().intern(pair);
    saved_world.insert(s_obj, FormIdComponent(fid));

    let reg = registry_with_anim();
    let snapshot = save_world(&saved_world, &reg).unwrap();

    // ── "Reloaded cell": SAME form id, DIFFERENT ids; the cell loader has
    //    already attached a player scoped to the FRESH subtree root (1). ──
    let build_live = || {
        let mut live = World::new();
        live.insert_resource(FormIdPool::new());
        let l_root = live.spawn(); // 0
        let l_obj = live.spawn(); // 1
        live.insert(l_obj, AnimationPlayer::new(3).with_root(l_root));
        let fid = live.resource_mut::<FormIdPool>().intern(pair);
        live.insert(l_obj, FormIdComponent(fid));
        (live, l_obj, l_root)
    };

    // The bug: including "AnimationPlayer" in the overlay set clobbers the
    // fresh root_entity (0) with the stale saved one (4).
    {
        let (mut live, l_obj, _) = build_live();
        let remap = build_form_id_remap(&live, &reg, &snapshot);
        apply_deltas(&mut live, &reg, &snapshot, &remap, &["AnimationPlayer"]).unwrap();
        let q = live.query::<AnimationPlayer>().unwrap();
        assert_eq!(
            q.get(l_obj).unwrap().root_entity,
            Some(4),
            "overlaying AnimationPlayer leaks the stale saved root_entity (the #1696 hazard)"
        );
    }

    // The fix: the binary omits "AnimationPlayer" from the overlay set, so the
    // cell-owned fresh root_entity survives the live load untouched.
    {
        let (mut live, l_obj, l_root) = build_live();
        let remap = build_form_id_remap(&live, &reg, &snapshot);
        let applied = apply_deltas(&mut live, &reg, &snapshot, &remap, &["Transform"]).unwrap();
        assert_eq!(applied, 0, "no animation column overlaid");
        let q = live.query::<AnimationPlayer>().unwrap();
        assert_eq!(
            q.get(l_obj).unwrap().root_entity,
            Some(l_root),
            "excluding AnimationPlayer preserves the cell-owned fresh root_entity"
        );
    }
}

/// SAVE-D2-02 — restoring a `FormIdComponent`-bearing save into a world that
/// has **no** `FormIdPool` installed must fail with a typed
/// `SaveError::MissingResource`, never panic.
///
/// The `FormIdComponent` load closure re-interns each saved `FormIdPair`
/// through the destination's pool; if no pool is present it returns the typed
/// error rather than `resource_mut`'s "Resource not found" panic, mirroring
/// the defensive save side. The live engine always installs a pool (boot +
/// cell reload), so this guards against a future restore-ordering bug or a
/// refactor back to `resource_mut`. The source world here carries a real
/// `FormIdComponent` (on the actor), so the column is non-empty and its load
/// closure actually runs the pool lookup.
#[test]
fn form_id_restore_without_pool_errors_cleanly() {
    let (src, _pair) = build_source_world();
    let reg = registry();

    let snapshot = save_world(&src, &reg).expect("save");
    let bytes = encode(&snapshot, reg.schema_fingerprint()).expect("encode");
    let decoded = decode(&bytes, reg.schema_fingerprint()).expect("decode");

    // Deliberately DO NOT install a FormIdPool on the destination. (Every
    // other column loads first; FormIdComponent is registered last, so the
    // failure is on its closure, not a setup gap.) `SaveError` doesn't derive
    // `PartialEq`, so match the variant + payload directly.
    let mut dst = World::new();
    match restore_world(&mut dst, &reg, &decoded) {
        Err(SaveError::MissingResource("FormIdPool")) => {}
        other => panic!(
            "expected Err(MissingResource(\"FormIdPool\")) restoring a \
             FormIdComponent column without a pool, got {other:?}"
        ),
    }
}
