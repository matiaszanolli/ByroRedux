//! M47.2 perk entry-point composition against the shipped FalloutNV.esm: the
//! vanilla perks the composer's rules were derived from, composed end to end
//! (`parse_esm` → [`install_perk_entry_points`] → [`compose_entry_point`]).
//!
//! **Opt-in / `#[ignore]`d** — needs FalloutNV.esm on disk:
//! ```bash
//! cargo test -p byroredux-scripting --test perk_entry_points_real_data -- --ignored
//! ```
//! Skips gracefully (passing) when the game data isn't present.

use std::path::PathBuf;

use byroredux_core::character::Perks;
use byroredux_core::ecs::components::{ActorValues, EquippedWeapon, InventoryIndex};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;
use byroredux_plugin::esm::test_paths;
use byroredux_scripting::perk_entry_points::{

    compose_damage_threshold, compose_entry_point, compose_entry_point_traced, compose_outgoing_damage, fnv,
    install_perk_entry_points,
};

mod common;

fn fnv_index() -> Option<EsmIndex> {
    let dir = std::env::var(test_paths::FNV_ENV).unwrap_or_else(|_| test_paths::FNV_DEFAULT.into());
    let bytes = std::fs::read(PathBuf::from(dir).join("FalloutNV.esm")).ok()?;
    byroredux_plugin::esm::parse_esm(&bytes).ok()
}

fn actor_with(index: &EsmIndex, owned: &[(u32, u8)]) -> (World, byroredux_core::ecs::storage::EntityId) {
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.register::<Perks>();
    install_perk_entry_points(&mut world, index);
    let actor = world.spawn();
    let mut perks = Perks::default();
    for &(perk, rank) in owned {
        perks.set_rank(perk, rank);
    }
    world.insert(actor, perks);
    (world, actor)
}

#[test]
#[ignore = "needs FalloutNV.esm on disk"]
fn vanilla_fnv_perks_compose_as_their_descriptions_say() {
    let Some(index) = fnv_index() else {
        common::require_or_skip("FalloutNV.esm", &"fnv_index()");
        eprintln!("SKIP: FalloutNV.esm not found");
        return;
    };
    const BLOODY_MESS: u32 = 0x94EBA; // "5% extra damage with all weapons"
    const EDUCATED: u32 = 0x31DD8; // "two more skill points every level"
    const DEMOLITION_EXPERT: u32 = 0x31DAB; // ×1.2 / ×1.4 / ×1.6 explosive damage
    const SWIFT_LEARNER: u32 = 0x31DD3; // +10% / 20% / 30% experience

    let id = |editor_id: &str| {
        index
            .perks
            .values()
            .find(|perk| perk.editor_id == editor_id)
            .unwrap_or_else(|| panic!("{editor_id} is a vanilla FNV perk"))
            .form_id
    };
    assert_eq!(id("BloodyMess"), BLOODY_MESS);
    assert_eq!(id("Educated"), EDUCATED);
    assert_eq!(id("DemolitionExpert"), DEMOLITION_EXPERT);
    assert_eq!(id("SwiftLearner"), SWIFT_LEARNER);
    let weapon_handling = id("WeaponHandling"); // −2 weapon Strength requirement

    let (world, actor) = actor_with(&index, &[(BLOODY_MESS, 1)]);
    let damage = compose_entry_point(&world, actor, fnv::CALCULATE_WEAPON_DAMAGE, 100.0, &[]);
    assert!((damage - 105.0).abs() < 1e-3, "Bloody Mess: {damage}");

    let (world, actor) = actor_with(&index, &[(EDUCATED, 1)]);
    let points = compose_entry_point(&world, actor, fnv::ADJUST_GAINED_SKILL_POINTS, 15.0, &[]);
    assert_eq!(points, 17.0, "Educated");

    // Demolition Expert's multiplier is conditioned on the weapon being an
    // explosive: with no weapon subject supplied it must NOT fire, at any rank.
    for rank in 1..=3u8 {
        let (world, actor) = actor_with(&index, &[(DEMOLITION_EXPERT, rank)]);
        let out = compose_entry_point(&world, actor, fnv::CALCULATE_WEAPON_DAMAGE, 50.0, &[]);
        assert_eq!(out, 50.0, "rank {rank}: a weapon-conditioned entry with no weapon");
    }

    for (rank, factor) in [(1u8, 1.1f32), (2, 1.2), (3, 1.3)] {
        let (world, actor) = actor_with(&index, &[(SWIFT_LEARNER, rank)]);
        let xp = compose_entry_point(&world, actor, fnv::ADJUST_EXPERIENCE_POINTS, 100.0, &[]);
        assert!((xp - 100.0 * factor).abs() < 1e-3, "Swift Learner rank {rank}: {xp}");
    }

    // Weapon Handling: function 1 on the subtract hook 53.
    let (world, actor) = actor_with(&index, &[(weapon_handling, 1)]);
    let trace = compose_entry_point_traced(&world, actor, 53, 7.0, &[None]);
    assert_eq!(trace.value, 5.0, "Weapon Handling: {trace:?}");
}

/// A world for a real load order, with what the conditions read: the game, the form lists, and actors.
fn combat_world(index: &EsmIndex, owned: &[(u32, u8)]) -> (World, EntityId, EntityId) {
    let (mut world, owner) = actor_with(index, owned);
    world.insert_resource(byroredux_scripting::ScriptGame(index.game));
    byroredux_scripting::form_lists::install_form_lists(&mut world, index);
    world.register::<EquippedWeapon>();
    world.register::<ActorValues>();
    world.insert(owner, ActorValues::default());
    let other = world.spawn();
    world.insert(other, ActorValues::default());
    (world, owner, other)
}

fn equip(world: &mut World, actor: EntityId, base_form_id: u32) {
    world.insert(actor, EquippedWeapon { inventory_index: InventoryIndex(0), base_form_id, damage: 1.0, reach: 0.0, speed: 0.0 });
}

/// PERK-SUB-002 on the real records: each perk does what its description says, with the real weapons.
#[test]
#[ignore = "needs FalloutNV.esm on disk"]
fn vanilla_fnv_weapon_conditioned_perks_compose() {
    let Some(index) = fnv_index() else {
        common::require_or_skip("FalloutNV.esm", &"fnv_index()");
        eprintln!("SKIP: FalloutNV.esm not found");
        return;
    };
    let perk = |editor_id: &str| {
        index
            .perks
            .values()
            .find(|perk| perk.editor_id == editor_id)
            .unwrap_or_else(|| panic!("{editor_id} is a vanilla FNV perk"))
            .form_id
    };
    let weapon = |editor_id: &str| {
        index
            .items
            .values()
            .find(|item| item.common.editor_id == editor_id)
            .unwrap_or_else(|| panic!("{editor_id} is a vanilla FNV weapon"))
            .form_id
    };
    let (knife, pistol, grenade) = (weapon("WeapKnifeCombat"), weapon("Weap10mmPistol"), weapon("WeapGrenadeFrag"));
    let shotgun = weapon("WeapShotgunSawedOff");

    // Hook 58, the attacker's perks: "ignore" the target's DT.
    let (mut world, attacker, target) = combat_world(&index, &[(perk("PiercingStrike"), 1)]);
    equip(&mut world, attacker, knife);
    assert_eq!(compose_damage_threshold(&world, attacker, target, 20.0), 5.0, "Piercing Strike, melee");
    equip(&mut world, attacker, pistol);
    assert_eq!(compose_damage_threshold(&world, attacker, target, 20.0), 20.0, "Piercing Strike, a gun");
    let (world, puncher, target) = combat_world(&index, &[(perk("PiercingStrike"), 1)]);
    assert_eq!(compose_damage_threshold(&world, puncher, target, 20.0), 5.0, "Piercing Strike, bare hands");

    let (mut world, attacker, target) = combat_world(&index, &[(perk("ShotgunSurgeon"), 1)]);
    equip(&mut world, attacker, shotgun);
    assert_eq!(compose_damage_threshold(&world, attacker, target, 20.0), 10.0, "Shotgun Surgeon, a shotgun");
    equip(&mut world, attacker, pistol);
    assert_eq!(compose_damage_threshold(&world, attacker, target, 20.0), 20.0, "Shotgun Surgeon, a pistol");

    // Hook 56, the defender's perks: its Target tab tests the attack.
    let (mut world, defender, attacker) = combat_world(&index, &[(perk("Stonewall"), 1)]);
    equip(&mut world, attacker, knife);
    assert_eq!(compose_damage_threshold(&world, attacker, defender, 10.0), 15.0, "Stonewall, melee");
    equip(&mut world, attacker, pistol);
    assert_eq!(compose_damage_threshold(&world, attacker, defender, 10.0), 10.0, "Stonewall, a gun");
    let (mut world, defender, attacker) = combat_world(&index, &[(perk("HitTheDeck"), 1)]);
    equip(&mut world, attacker, grenade);
    assert_eq!(compose_damage_threshold(&world, attacker, defender, 10.0), 35.0, "Hit the Deck, explosive");
    equip(&mut world, attacker, knife);
    assert_eq!(compose_damage_threshold(&world, attacker, defender, 10.0), 10.0, "Hit the Deck, melee");

    // Hook 0, the weapon-damage hook.
    for (rank, factor) in [(1u8, 1.2f32), (2, 1.4), (3, 1.6)] {
        let (mut world, attacker, target) = combat_world(&index, &[(perk("DemolitionExpert"), rank)]);
        equip(&mut world, attacker, grenade);
        let out = compose_outgoing_damage(&world, attacker, target, 50.0);
        assert!((out - 50.0 * factor).abs() < 1e-3, "Demolition Expert rank {rank}: {out}");
        equip(&mut world, attacker, pistol);
        assert_eq!(compose_outgoing_damage(&world, attacker, target, 50.0), 50.0, "a gun, rank {rank}");
    }
}

/// M47.2 — the Damage Threshold hooks: Toughness (+3 / +6 DT, hook 56 on the
/// defender) raises the target's DT; Piercing Strike (hook 58 on the attacker)
/// is weapon-conditioned, so an attacker that is no actor has no weapon subject
/// and it does not fire. `vanilla_fnv_weapon_conditioned_perks_compose` has the
/// weapon cases.
#[test]
#[ignore = "needs FalloutNV.esm on disk"]
fn vanilla_fnv_damage_threshold_perks_compose() {
    let Some(index) = fnv_index() else {
        common::require_or_skip("FalloutNV.esm", &"fnv_index()");
        eprintln!("SKIP: FalloutNV.esm not found");
        return;
    };
    let id = |editor_id: &str| {
        index
            .perks
            .values()
            .find(|perk| perk.editor_id == editor_id)
            .unwrap_or_else(|| panic!("{editor_id} is a vanilla FNV perk"))
            .form_id
    };
    for (rank, dt) in [(1u8, 13.0f32), (2, 16.0)] {
        let (mut world, defender) = actor_with(&index, &[(id("Toughness"), rank)]);
        let attacker = world.spawn();
        let composed = compose_damage_threshold(&world, attacker, defender, 10.0);
        assert_eq!(composed, dt, "Toughness rank {rank}");
    }
    let (mut world, attacker) = actor_with(&index, &[(id("PiercingStrike"), 1)]);
    let defender = world.spawn();
    assert_eq!(compose_damage_threshold(&world, attacker, defender, 10.0), 10.0);
}

#[test]
#[ignore = "needs FalloutNV.esm on disk"]
fn the_fnv_table_is_populated() {
    let Some(index) = fnv_index() else {
        common::require_or_skip("FalloutNV.esm", &"fnv_index()");
        eprintln!("SKIP: FalloutNV.esm not found");
        return;
    };
    let mut world = World::new();
    let perks = install_perk_entry_points(&mut world, &index);
    // 176 PERK records, 143 entry-point entries: most carry value math.
    assert!(perks > 50, "{perks}");
}
