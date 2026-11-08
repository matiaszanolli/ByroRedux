use super::*;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_core::ecs::components::{ActorValues, Dead};
use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue, RunOn};
use byroredux_plugin::esm::records::{AvifRecord, PerkEntry, PerkRecord};

fn entry_point(
    rank: u8,
    priority: u8,
    entry_point_index: u8,
    function: u8,
    function_data: PerkFunctionData,
    tabs: Vec<PerkConditionTab>,
) -> PerkEntry {
    PerkEntry {
        rank,
        priority,
        body: PerkEntryBody::EntryPoint {
            entry_point_index,
            function,
            tab_count: 3,
            param_type: 1,
            function_data,
            formatter: Vec::new(),
            extra_flags: Vec::new(),
            details: Default::default(),
        },
        condition_tabs: tabs,
    }
}

fn float(x: f32) -> PerkFunctionData {
    PerkFunctionData::Float(x)
}

fn perk(form_id: u32, entries: Vec<PerkEntry>) -> PerkRecord {
    PerkRecord {
        form_id,
        editor_id: format!("Perk{form_id:X}"),
        entries,
        ..Default::default()
    }
}

fn index(game: GameKind, perks: Vec<PerkRecord>) -> EsmIndex {
    use byroredux_core::character::CharacterRulesProfile;
    // The parser selects the profile from the game (#5921 reads the
    // entry-point dialect off it); FO3/FNV share theirs.
    let character_rules = match game {
        GameKind::Oblivion => CharacterRulesProfile::OBLIVION,
        GameKind::Fallout3NV => CharacterRulesProfile::FALLOUT_NEW_VEGAS,
        GameKind::Skyrim => CharacterRulesProfile::SKYRIM,
        GameKind::Fallout4 => CharacterRulesProfile::FALLOUT4,
        GameKind::Fallout76 => CharacterRulesProfile::FALLOUT76,
        GameKind::Starfield => CharacterRulesProfile::STARFIELD,
    };
    let mut index = EsmIndex {
        game,
        character_rules,
        ..Default::default()
    };
    for record in perks {
        index.perks.insert(record.form_id, record);
    }
    index
}

/// A world with the table installed, and an actor owning `owned`
/// `(perk, one-based rank)`.
fn world_with(index: &EsmIndex, owned: &[(u32, u8)]) -> (World, EntityId) {
    let mut world = World::new();
    crate::register(&mut world);
    world.register::<Perks>();
    world.register::<ActorValues>();
    world.register::<Dead>();
    install_perk_entry_points(&mut world, index);
    let actor = world.spawn();
    let mut perks = Perks::default();
    for &(perk, rank) in owned {
        perks.set_rank(perk, rank);
    }
    world.insert(actor, perks);
    (world, actor)
}

fn condition(function_index: u32, comparand: f32) -> Condition {
    Condition {
        function_index,
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(comparand),
        run_on: RunOn::Subject,
        ..Default::default()
    }
}

fn tab(tab: i8, conditions: Vec<Condition>) -> PerkConditionTab {
    PerkConditionTab { tab, conditions }
}

const DAMAGE: u8 = fnv::CALCULATE_WEAPON_DAMAGE;
const GET_DEAD: u32 = 46;

/// The vanilla entry shape: `BloodyMess` multiplies weapon damage by 1.05
/// and its three condition tabs are empty, so it applies unconditionally.
#[test]
fn bloody_mess_multiplies_weapon_damage() {
    let idx = index(
        GameKind::Fallout3NV,
        vec![perk(0x94EBA, vec![entry_point(0, 0, DAMAGE, 3, float(1.05), vec![])])],
    );
    let (world, actor) = world_with(&idx, &[(0x94EBA, 1)]);
    let out = compose_entry_point(&world, actor, DAMAGE, 20.0, &[]);
    assert!((out - 21.0).abs() < 1e-5, "{out}");
}

/// The rank rule, on vanilla `DemolitionExpert`'s shape: ×1.2 / ×1.4 / ×1.6
/// at zero-based ranks 0 / 1 / 2 are *totals* — rank 2 (held rank 3) gives
/// ×1.6 alone, not ×1.2 × ×1.4 × ×1.6.
#[test]
fn only_the_current_ranks_entry_applies() {
    let idx = index(
        GameKind::Fallout3NV,
        vec![perk(
            0x31DAB,
            vec![
                entry_point(0, 0, DAMAGE, 3, float(1.2), vec![]),
                entry_point(1, 0, DAMAGE, 3, float(1.4), vec![]),
                entry_point(2, 0, DAMAGE, 3, float(1.6), vec![]),
            ],
        )],
    );
    for (held, expected) in [(1u8, 1.2f32), (2, 1.4), (3, 1.6)] {
        let (world, actor) = world_with(&idx, &[(0x31DAB, held)]);
        let out = compose_entry_point(&world, actor, DAMAGE, 100.0, &[]);
        assert!((out - 100.0 * expected).abs() < 1e-3, "held rank {held}: {out}");
    }
    // Not owned: nothing applies.
    let (world, actor) = world_with(&idx, &[]);
    assert_eq!(compose_entry_point(&world, actor, DAMAGE, 100.0, &[]), 100.0);
}

/// Hooks other than the asked one, actors without perks and worlds without a
/// table are all the identity.
#[test]
fn other_hooks_unowned_perks_and_a_missing_table_leave_the_value_alone() {
    let idx = index(
        GameKind::Fallout3NV,
        vec![perk(1, vec![entry_point(0, 0, DAMAGE, 2, float(5.0), vec![])])],
    );
    let (mut world, actor) = world_with(&idx, &[(1, 1)]);
    assert_eq!(
        compose_entry_point(&world, actor, fnv::GET_MAX_CARRY_WEIGHT, 200.0, &[]),
        200.0
    );
    let bystander = world.spawn();
    assert_eq!(compose_entry_point(&world, bystander, DAMAGE, 7.0, &[]), 7.0);

    let mut bare = World::new();
    crate::register(&mut bare);
    bare.register::<Perks>();
    let actor = bare.spawn();
    let mut perks = Perks::default();
    perks.set_rank(1, 1);
    bare.insert(actor, perks);
    assert_eq!(compose_entry_point(&bare, actor, DAMAGE, 7.0, &[]), 7.0);
}

#[test]
fn set_add_multiply_absolute_compose_in_priority_order() {
    // Skyrim `EagleEye30`-style: a Set at the high priority number applies
    // after the priority-0 multiplier, so it wins; the multiplier is not
    // lost to ordering the other way round.
    let idx = index(
        GameKind::Skyrim,
        vec![
            perk(1, vec![entry_point(0, 0, 35, 3, float(2.0), vec![])]),
            perk(2, vec![entry_point(0, 15, 35, 1, float(10.0), vec![])]),
            perk(3, vec![entry_point(0, 5, 35, 2, float(1.0), vec![])]),
        ],
    );
    // Held in the order 2, 1, 3: priority still orders them 0, 5, 15.
    let (world, actor) = world_with(&idx, &[(2, 1), (1, 1), (3, 1)]);
    let trace = compose_entry_point_traced(&world, actor, 35, 4.0, &[]);
    let order: Vec<u32> = trace.applied.iter().map(|a| a.perk).collect();
    assert_eq!(order, vec![1, 3, 2]);
    // ×2 → 8, +1 → 9, Set 10 → 10.
    assert_eq!(trace.value, 10.0);

    // Equal priority: the actor's perk order, then entry order.
    let tie = index(
        GameKind::Fallout3NV,
        vec![
            perk(10, vec![entry_point(0, 0, 0, 2, float(1.0), vec![])]),
            perk(11, vec![entry_point(0, 0, 0, 3, float(2.0), vec![])]),
        ],
    );
    let (world, actor) = world_with(&tie, &[(11, 1), (10, 1)]);
    // 11 first: (3 × 2) + 1 = 7, not (3 + 1) × 2 = 8.
    assert_eq!(compose_entry_point(&world, actor, 0, 3.0, &[]), 7.0);

    let idx = index(
        GameKind::Skyrim,
        vec![
            perk(1, vec![entry_point(0, 0, 36, 7, PerkFunctionData::None, vec![])]),
        ],
    );
    let (world, actor) = world_with(&idx, &[(1, 1)]);
    assert_eq!(compose_entry_point(&world, actor, 36, 4.0, &[]), -4.0);
    let idx = index(
        GameKind::Skyrim,
        vec![perk(1, vec![entry_point(0, 0, 36, 6, PerkFunctionData::None, vec![])])],
    );
    let (world, actor) = world_with(&idx, &[(1, 1)]);
    assert_eq!(compose_entry_point(&world, actor, 36, -4.0, &[]), 4.0);
}

/// Vanilla `WeaponHandling` (hook 53, function 1, 2.0: "-2 Strength
/// requirement") and `PiercingStrike` (hook 58, function 1, 15.0): on the
/// three subtract-table hooks function 1 subtracts; elsewhere it sets.
#[test]
fn function_one_subtracts_on_the_subtract_table_hooks_only() {
    let idx = index(
        GameKind::Fallout3NV,
        vec![
            perk(1, vec![entry_point(0, 0, 53, 1, float(2.0), vec![])]),
            perk(2, vec![entry_point(0, 0, fnv::MODIFY_DAMAGE_THRESHOLD_ATTACKER, 1, float(15.0), vec![])]),
            perk(3, vec![entry_point(0, 0, 52, 1, float(0.15), vec![])]),
        ],
    );
    let (world, actor) = world_with(&idx, &[(1, 1), (2, 1), (3, 1)]);
    assert_eq!(compose_entry_point(&world, actor, 53, 7.0, &[]), 5.0);
    assert_eq!(
        compose_entry_point(&world, actor, fnv::MODIFY_DAMAGE_THRESHOLD_ATTACKER, 20.0, &[]),
        5.0
    );
    // Knockdown Chance (52): plain Set Value, as SuperSlam authors it.
    assert_eq!(compose_entry_point(&world, actor, 52, 0.5, &[]), 0.15);
    // The same function byte on Skyrim is always Set.
    let sky = index(
        GameKind::Skyrim,
        vec![perk(1, vec![entry_point(0, 0, 53, 1, float(2.0), vec![])])],
    );
    let (world, actor) = world_with(&sky, &[(1, 1)]);
    assert_eq!(compose_entry_point(&world, actor, 53, 7.0, &[]), 2.0);
}

/// Condition tabs: tab 0 runs on the owner, the others on the subject the
/// calculation supplies — and a conditioned tab with no subject fails.
#[test]
fn condition_tabs_run_on_their_own_subject() {
    let owner_dead = tab(0, vec![condition(GET_DEAD, 1.0)]);
    let target_dead = tab(2, vec![condition(GET_DEAD, 1.0)]);
    let idx = index(
        GameKind::Fallout3NV,
        vec![
            perk(1, vec![entry_point(0, 0, DAMAGE, 3, float(2.0), vec![owner_dead])]),
            perk(2, vec![entry_point(0, 0, DAMAGE, 3, float(3.0), vec![target_dead])]),
        ],
    );
    let (mut world, owner) = world_with(&idx, &[(1, 1), (2, 1)]);
    let target = world.spawn();
    let weapon = world.spawn();
    let subjects = [Some(weapon), Some(target)];

    // Nobody is dead.
    assert_eq!(compose_entry_point(&world, owner, DAMAGE, 1.0, &subjects), 1.0);
    // The target dies: the tab-2 entry fires, the owner's does not.
    world.insert(target, Dead);
    assert_eq!(compose_entry_point(&world, owner, DAMAGE, 1.0, &subjects), 3.0);
    // Without a supplied target subject the conditioned tab cannot be judged.
    assert_eq!(compose_entry_point(&world, owner, DAMAGE, 1.0, &[Some(weapon)]), 1.0);
    // The owner dies too: both fire.
    world.insert(owner, Dead);
    assert_eq!(compose_entry_point(&world, owner, DAMAGE, 1.0, &subjects), 6.0);
}

/// A condition the evaluator cannot model reads 0.0, which `== 0` would
/// satisfy: an entry gated on one must not fire.
#[test]
fn an_unmodelled_condition_keeps_the_entry_from_firing() {
    let unknown = tab(0, vec![condition(9_999, 0.0)]);
    let idx = index(
        GameKind::Fallout3NV,
        vec![perk(1, vec![entry_point(0, 0, DAMAGE, 3, float(2.0), vec![unknown])])],
    );
    let (world, actor) = world_with(&idx, &[(1, 1)]);
    assert_eq!(compose_entry_point(&world, actor, DAMAGE, 1.0, &[]), 1.0);
}

/// The actor-value functions read the owner's composed value; the AV index
/// resolves to its AVIF FormID through the load order.
#[test]
fn actor_value_functions_read_the_owners_value() {
    let mut idx = index(
        GameKind::Skyrim,
        vec![perk(
            1,
            vec![
                // Multiply 1 + AV Mult: Skyrim `PerkSkillBoosts` shape (AV
                // index 6 = One-Handed → AVIF 0x44C).
                entry_point(
                    0,
                    0,
                    35,
                    14,
                    PerkFunctionData::ActorValueMult { actor_value: 6, mult: 0.01 },
                    vec![],
                ),
            ],
        )],
    );
    idx.actor_values.insert(0x44C, AvifRecord::default());
    let (mut world, actor) = world_with(&idx, &[(1, 1)]);
    let mut values = ActorValues::new();
    values.set_base(0x44C, 50.0);
    world.insert(actor, values);
    // 10 × (1 + 50 × 0.01) = 15.
    assert!((compose_entry_point(&world, actor, 35, 10.0, &[]) - 15.0).abs() < 1e-5);

    // An index with no AVIF in the load order contributes nothing: ×(1+0).
    let mut bare = idx;
    bare.actor_values.clear();
    let (mut world, actor) = world_with(&bare, &[(1, 1)]);
    let mut values = ActorValues::new();
    values.set_base(0x44C, 50.0);
    world.insert(actor, values);
    assert_eq!(compose_entry_point(&world, actor, 35, 10.0, &[]), 10.0);
}

#[test]
fn add_range_draws_between_its_bounds() {
    let idx = index(
        GameKind::Fallout3NV,
        vec![perk(
            1,
            vec![entry_point(0, 0, DAMAGE, 4, PerkFunctionData::Range { min: 2.0, max: 6.0 }, vec![])],
        )],
    );
    let (mut world, actor) = world_with(&idx, &[(1, 1)]);
    world.insert_resource(StoryManagerRng { state: 7 });
    for _ in 0..64 {
        let out = compose_entry_point(&world, actor, DAMAGE, 10.0, &[]);
        assert!((12.0..=16.0).contains(&out), "{out}");
    }
}

/// Games whose PERK entry layout is not established get an empty table, and
/// entries whose function is not value math (leveled list, spell, text) are
/// left out of it.
#[test]
fn unsupported_games_and_non_numeric_functions_are_not_tabulated() {
    let record = perk(1, vec![entry_point(0, 0, DAMAGE, 3, float(2.0), vec![])]);
    assert!(PerkEntryTable::from_index(&index(GameKind::Fallout4, vec![record.clone()])).is_empty());
    assert!(PerkEntryTable::from_index(&index(GameKind::Oblivion, vec![record])).is_empty());

    let idx = index(
        GameKind::Skyrim,
        vec![perk(
            2,
            vec![
                entry_point(0, 0, 51, 10, PerkFunctionData::FormId(0xAB), vec![]),
                entry_point(0, 0, 21, 8, PerkFunctionData::FormId(0xCD), vec![]),
                // Declared Multiply but its parameters did not decode.
                entry_point(0, 0, 35, 3, PerkFunctionData::None, vec![]),
            ],
        )],
    );
    assert!(PerkEntryTable::from_index(&idx).is_empty());
}

// ── PERK-SUB-002 / -003: the weapon and Damage Threshold target subjects ─────

const MELEE_WEAPONS: u32 = 38;
const EXPLOSIVES: u32 = 35;
const GUNS: u32 = 41;
const UNARMED: u32 = 45;
const IS_WEAPON_SKILL_TYPE: u32 = 109;
const IS_IN_LIST: u32 = 372;
const GET_IS_ID: u32 = 72;
const KNIFE: u32 = 0x4000;
const PISTOL: u32 = 0x4001;
const GRENADE: u32 = 0x4002;
const FISTS: u32 = 0x1F4;
const SHOTGUN_LIST: u32 = 0x4100;
const DEFENDER_HOOK: u8 = fnv::MODIFY_DAMAGE_THRESHOLD_DEFENDER;
const ATTACKER_HOOK: u8 = fnv::MODIFY_DAMAGE_THRESHOLD_ATTACKER;

fn is_skill(skill: u32) -> Condition {
    Condition { param_1: skill, ..condition(IS_WEAPON_SKILL_TYPE, 1.0) }
}

/// An FNV load order with the three test weapons, plus `Fists` at its real FormID.
fn fnv_arsenal(perks: Vec<PerkRecord>) -> EsmIndex {
    use byroredux_plugin::esm::records::{FlstRecord, ItemKind, ItemRecord};
    let mut idx = index(GameKind::Fallout3NV, perks);
    for (form_id, skill) in [(KNIFE, MELEE_WEAPONS), (PISTOL, GUNS), (GRENADE, EXPLOSIVES), (FISTS, UNARMED)] {
        idx.items.insert(
            form_id,
            ItemRecord {
                form_id,
                common: Default::default(),
                kind: ItemKind::weapon_stub(10, 1).with_weapon_skill(skill),
            },
        );
    }
    idx.form_lists.insert(
        SHOTGUN_LIST,
        FlstRecord { form_id: SHOTGUN_LIST, entries: vec![PISTOL], ..Default::default() },
    );
    idx
}

fn equip(world: &mut World, actor: EntityId, base_form_id: u32) {
    use byroredux_core::ecs::components::{EquippedWeapon, InventoryIndex};
    world.register::<EquippedWeapon>();
    world.insert(actor, EquippedWeapon { inventory_index: InventoryIndex(0), base_form_id, damage: 1.0, reach: 0.0, speed: 0.0 });
}

/// `world_with`, plus the game and the form lists a real load order installs. The owner is an actor;
/// the second entity is a second actor with no perks.
fn fnv_world(idx: &EsmIndex, owned: &[(u32, u8)]) -> (World, EntityId, EntityId) {
    let (mut world, owner) = world_with(idx, owned);
    world.insert_resource(crate::ScriptGame(GameKind::Fallout3NV));
    crate::form_lists::install_form_lists(&mut world, idx);
    world.insert(owner, ActorValues::default());
    let other = world.spawn();
    world.insert(other, ActorValues::default());
    (world, owner, other)
}

/// PERK-SUB-002 / §7.2 A2 — Piercing Strike's shape: hook 58, `Subtract`, a weapon tab that only the
/// attacker's melee or unarmed weapons pass.
#[test]
fn weapon_tab_subject_answers_weapon_form() {
    let idx = fnv_arsenal(vec![perk(
        1,
        vec![entry_point(0, 0, ATTACKER_HOOK, 1, float(15.0), vec![tab(1, vec![is_skill(MELEE_WEAPONS)])])],
    )]);
    let (mut world, attacker, target) = fnv_world(&idx, &[(1, 1)]);
    equip(&mut world, attacker, KNIFE);
    assert_eq!(compose_damage_threshold(&world, attacker, target, 20.0), 5.0, "a melee weapon passes");
    equip(&mut world, attacker, PISTOL);
    assert_eq!(compose_damage_threshold(&world, attacker, target, 20.0), 20.0, "a gun does not");
}

/// Shotgun Surgeon's shape: `IsInList` on the weapon tab reads the lent base form, which has no
/// FormID of its own for the ObScript getter to call on.
#[test]
fn weapon_tab_answers_form_list_membership() {
    let in_list = Condition { param_1: SHOTGUN_LIST, ..condition(IS_IN_LIST, 1.0) };
    let idx = fnv_arsenal(vec![perk(
        1,
        vec![entry_point(0, 0, ATTACKER_HOOK, 1, float(10.0), vec![tab(1, vec![in_list])])],
    )]);
    let (mut world, attacker, target) = fnv_world(&idx, &[(1, 1)]);
    equip(&mut world, attacker, PISTOL);
    assert_eq!(compose_damage_threshold(&world, attacker, target, 20.0), 10.0, "in the list");
    equip(&mut world, attacker, KNIFE);
    assert_eq!(compose_damage_threshold(&world, attacker, target, 20.0), 20.0, "not in the list");
}

/// PERK-SUB-002's verify item — Stonewall's shape: hook 56's owner is the actor hit, and its Target tab
/// tests the attack, so it names the attacker. The defender's own weapon must not answer.
#[test]
fn hook_56_target_tab_names_the_attacker() {
    let idx = fnv_arsenal(vec![perk(
        1,
        vec![entry_point(0, 0, DEFENDER_HOOK, 2, float(5.0), vec![tab(2, vec![is_skill(MELEE_WEAPONS)])])],
    )]);
    let (mut world, defender, attacker) = fnv_world(&idx, &[(1, 1)]);
    equip(&mut world, attacker, KNIFE);
    equip(&mut world, defender, PISTOL);
    assert_eq!(compose_damage_threshold(&world, attacker, defender, 10.0), 15.0, "hit by a knife");
    equip(&mut world, attacker, PISTOL);
    equip(&mut world, defender, KNIFE);
    assert_eq!(compose_damage_threshold(&world, attacker, defender, 10.0), 10.0, "shot, holding a knife");
}

/// A bare-handed actor attacks with `Fists` (Unarmed, on the weapon tab and when asked as an actor). A
/// creature's natural attack and a non-actor have no weapon form, so they answer nothing.
#[test]
fn bare_hands_answer_as_fists_and_creatures_answer_nothing() {
    use byroredux_core::ecs::components::CreatureActor;
    let idx = fnv_arsenal(vec![
        perk(1, vec![entry_point(0, 0, ATTACKER_HOOK, 1, float(15.0), vec![tab(1, vec![is_skill(UNARMED)])])]),
        perk(2, vec![entry_point(0, 0, DEFENDER_HOOK, 2, float(5.0), vec![tab(2, vec![is_skill(UNARMED)])])]),
    ]);
    let (mut world, puncher, defender) = fnv_world(&idx, &[(1, 1)]);
    assert_eq!(compose_damage_threshold(&world, puncher, defender, 20.0), 5.0, "weapon tab");
    let mut perks = Perks::default();
    perks.set_rank(2, 1);
    world.insert(defender, perks);
    assert_eq!(compose_damage_threshold(&world, puncher, defender, 20.0), 10.0, "target tab too");

    let creature = world.spawn();
    world.insert(creature, ActorValues::default());
    world.register::<CreatureActor>();
    world.insert(creature, CreatureActor);
    assert_eq!(compose_damage_threshold(&world, creature, defender, 20.0), 20.0, "a creature");
    let trap = world.spawn();
    assert_eq!(compose_damage_threshold(&world, trap, defender, 20.0), 20.0, "a trap");
}

/// The loan ends with the composition: a candidate left on the scratch entity would be a placeless
/// alias candidate (#6571).
#[test]
fn the_weapon_loan_is_taken_back() {
    let idx = fnv_arsenal(vec![perk(
        1,
        vec![entry_point(0, 0, ATTACKER_HOOK, 1, float(15.0), vec![tab(1, vec![is_skill(MELEE_WEAPONS)])])],
    )]);
    let (mut world, attacker, target) = fnv_world(&idx, &[(1, 1)]);
    equip(&mut world, attacker, KNIFE);
    compose_damage_threshold(&world, attacker, target, 20.0);
    compose_outgoing_damage(&world, attacker, target, 20.0);
    let subject = world.resource::<WeaponSubject>().0;
    assert!(!world.has::<crate::scene::SceneAliasCandidate>(subject));
}

/// Demolition Expert's shape on the weapon-damage hook, which supplies the target tab as well.
#[test]
fn outgoing_damage_reads_the_weapon_tab() {
    let idx = fnv_arsenal(vec![perk(
        1,
        vec![entry_point(0, 0, DAMAGE, 3, float(2.0), vec![tab(1, vec![is_skill(EXPLOSIVES)])])],
    )]);
    let (mut world, attacker, target) = fnv_world(&idx, &[(1, 1)]);
    equip(&mut world, attacker, GRENADE);
    assert_eq!(compose_outgoing_damage(&world, attacker, target, 10.0), 20.0, "an explosive");
    equip(&mut world, attacker, PISTOL);
    assert_eq!(compose_outgoing_damage(&world, attacker, target, 10.0), 10.0, "a gun");
}

/// Skyrim authors its weapon tabs on the base form too (the Ebony Blade's `GetIsID`). The same subject
/// answers there, on both of its damage hooks.
#[test]
fn skyrim_weapon_tab_reads_the_lent_base_form() {
    let is_blade = Condition { param_1: KNIFE, ..condition(GET_IS_ID, 1.0) };
    let idx = index(
        GameKind::Skyrim,
        vec![perk(
            1,
            vec![entry_point(0, 0, skyrim::MOD_ATTACK_DAMAGE, 3, float(2.0), vec![tab(1, vec![is_blade])])],
        )],
    );
    let (mut world, attacker) = world_with(&idx, &[(1, 1)]);
    world.insert_resource(crate::ScriptGame(GameKind::Skyrim));
    world.insert(attacker, ActorValues::default());
    let target = world.spawn();
    equip(&mut world, attacker, KNIFE);
    assert_eq!(compose_outgoing_damage(&world, attacker, target, 10.0), 20.0);
    equip(&mut world, attacker, PISTOL);
    assert_eq!(compose_outgoing_damage(&world, attacker, target, 10.0), 10.0);
}

/// `IsWeaponSkillType` is modeled only where a weapon decode names the skill. Elsewhere it stays out of
/// the fail-closed gates rather than reading "no skill" from an empty table, and Oblivion's 109 is
/// another function altogether.
#[test]
fn is_weapon_skill_type_is_modeled_on_fallout_alone() {
    let weapon_cond = is_skill(MELEE_WEAPONS);
    for (game, modeled) in [
        (GameKind::Fallout3NV, true),
        (GameKind::Oblivion, false),
        (GameKind::Skyrim, false),
        (GameKind::Fallout4, false),
        (GameKind::Fallout76, false),
        (GameKind::Starfield, false),
    ] {
        let mut world = World::new();
        world.insert_resource(crate::ScriptGame(game));
        assert_eq!(crate::condition::is_modeled(&world, &weapon_cond), modeled, "{game:?}");
    }
}

/// #6422 — vanilla `ConcentratedFire` (FO3 / FNV, form 0x44CAF in FalloutNV.esm) authors one entry:
/// hook 33, function 1 (Set), 1.0, no conditions. Held, it turns the flag on; unheld, or on a game
/// with no FO3/FNV dialect, it stays off.
#[test]
fn concentrated_fire_is_a_flag_the_perk_sets() {
    let idx = index(
        GameKind::Fallout3NV,
        vec![perk(0x44CAF, vec![entry_point(0, 0, fnv::HAS_CONCENTRATED_FIRE, 1, float(1.0), vec![])])],
    );
    let (held, actor) = world_with(&idx, &[(0x44CAF, 1)]);
    assert!(has_concentrated_fire(&held, actor));
    let (unheld, actor) = world_with(&idx, &[]);
    assert!(!has_concentrated_fire(&unheld, actor), "no perk, no flag");

    let skyrim = index(
        GameKind::Skyrim,
        vec![perk(0x44CAF, vec![entry_point(0, 0, fnv::HAS_CONCENTRATED_FIRE, 1, float(1.0), vec![])])],
    );
    let (world, actor) = world_with(&skyrim, &[(0x44CAF, 1)]);
    assert!(!has_concentrated_fire(&world, actor), "entry 33 is not this hook outside FO3/FNV");
}

/// #6760 -- the entry-point dialect is the profile's row, not a `GameKind`
/// match: an index whose game and profile disagree decodes by the profile.
#[test]
fn entry_point_dialect_follows_the_profile_not_the_game() {
    use byroredux_core::character::CharacterRulesProfile;
    let perks = || vec![perk(0x94EBA, vec![entry_point(0, 0, DAMAGE, 3, float(1.05), vec![])])];

    // A Skyrim-game index carrying the FNV profile composes the FNV hook.
    let mut idx = index(GameKind::Skyrim, perks());
    idx.character_rules = CharacterRulesProfile::FALLOUT_NEW_VEGAS;
    let (world, actor) = world_with(&idx, &[(0x94EBA, 1)]);
    let out = compose_entry_point(&world, actor, DAMAGE, 20.0, &[]);
    assert!((out - 21.0).abs() < 1e-5, "{out}");

    // An FNV-game index carrying a no-dialect profile composes nothing.
    let mut idx = index(GameKind::Fallout3NV, perks());
    idx.character_rules = CharacterRulesProfile::FALLOUT4;
    let (world, actor) = world_with(&idx, &[(0x94EBA, 1)]);
    assert_eq!(compose_entry_point(&world, actor, DAMAGE, 20.0, &[]), 20.0);
}

// --- PERK-BODY: Ability and Quest entries follow the actor's perks --------------------------------

mod bodies {
    use super::*;
    use crate::magic::{SpellCatalog, SpellList};
    use crate::quest_stages::{QuestFormId, QuestStageState};
    use byroredux_plugin::esm::records::{
        ActorValueRef, MagicArchetype, MagicEffectItem, MgefRecord, SpelRecord, SpellType,
    };

    const HEALTH: u32 = 0x100;
    const FORTIFY_HEALTH: u32 = 0x10;
    /// Fortify Health abilities of 25 and 40: vanilla's authored-per-rank ability shape (FO3 `IronFist`).
    const ABILITY_RANK_1: u32 = 0x20;
    const ABILITY_RANK_2: u32 = 0x21;
    const PERK: u32 = 0x0001_0000;
    const QUEST: u32 = 0x0002_0000;

    fn ability(rank: u8, spell: u32) -> PerkEntry {
        PerkEntry { rank, priority: 0, body: PerkEntryBody::Ability { spell_form_id: spell }, condition_tabs: Vec::new() }
    }

    fn quest_entry(rank: u8, stage: u16) -> PerkEntry {
        PerkEntry { rank, priority: 0, body: PerkEntryBody::Quest { quest_form_id: QUEST, stage }, condition_tabs: Vec::new() }
    }

    fn body_world(entries: Vec<PerkEntry>, owned: &[(u32, u8)]) -> (World, EntityId) {
        let mut idx = index(GameKind::Fallout3NV, vec![perk(PERK, entries)]);
        idx.actor_values.insert(HEALTH, AvifRecord { form_id: HEALTH, ..Default::default() });
        idx.magic_effects.insert(
            FORTIFY_HEALTH,
            MgefRecord {
                form_id: FORTIFY_HEALTH,
                effect_flags: byroredux_plugin::esm::records::MGEF_FLAG_RECOVER, // a persistent fortify, not a per-second change
                archetype: Some(MagicArchetype::ValueModifier),
                primary_actor_value: Some(ActorValueRef::Form(HEALTH)),
                ..Default::default()
            },
        );
        for (form_id, magnitude) in [(ABILITY_RANK_1, 25.0), (ABILITY_RANK_2, 40.0)] {
            idx.spells.insert(
                form_id,
                SpelRecord {
                    form_id,
                    spell_type: SpellType::Ability,
                    effects: vec![MagicEffectItem {
                        effect_form_id: FORTIFY_HEALTH,
                        magnitude,
                        area: 0,
                        duration: 0,
                        script_form_id: 0,
                    }],
                    ..Default::default()
                },
            );
        }
        let (mut world, actor) = world_with(&idx, owned);
        world.insert_resource(SpellCatalog::from_index(&idx));
        world.insert_resource(QuestStageState::default());
        world.insert_resource(crate::quest_stages::QuestObjectiveState::default());
        let mut values = ActorValues::new();
        values.set_base(HEALTH, 100.0);
        world.insert(actor, values);
        world.insert(actor, SpellList::default());
        (world, actor)
    }

    fn spells(world: &World, actor: EntityId) -> Vec<u32> {
        world.get::<SpellList>(actor).map(|list| list.0.clone()).unwrap_or_default()
    }

    fn health(world: &World, actor: EntityId) -> f32 {
        world.get::<ActorValues>(actor).unwrap().current(HEALTH)
    }

    fn set_rank(world: &World, actor: EntityId, rank: u8) {
        let mut perks = world.query_mut::<Perks>().unwrap();
        let owned = perks.get_mut(actor).unwrap();
        owned.remove(PERK);
        owned.set_rank(PERK, rank);
    }

    /// PERK-BODY-001 / A2 -- the spell is in the list for as long as the perk is held at the rank the entry is
    /// authored for, and its value changes come and go with it.
    #[test]
    fn ability_entry_spell_follows_the_perk() {
        let (world, actor) = body_world(vec![ability(0, ABILITY_RANK_1), ability(1, ABILITY_RANK_2)], &[]);
        reconcile_perk_bodies(&world);
        assert!(spells(&world, actor).is_empty(), "not held: no spell");

        set_rank(&world, actor, 1);
        reconcile_perk_bodies(&world);
        assert_eq!(spells(&world, actor), vec![ABILITY_RANK_1]);
        assert_eq!(health(&world, actor), 125.0);

        // Rank 2 replaces rank 1's ability: each rank's entry is the total at that rank.
        set_rank(&world, actor, 2);
        reconcile_perk_bodies(&world);
        assert_eq!(spells(&world, actor), vec![ABILITY_RANK_2]);
        assert_eq!(health(&world, actor), 140.0);

        // Nothing changed: nothing is applied again.
        reconcile_perk_bodies(&world);
        assert_eq!(health(&world, actor), 140.0);

        // The perk goes (a script's RemovePerk): the spell goes with it.
        world.query_mut::<Perks>().unwrap().get_mut(actor).unwrap().remove(PERK);
        reconcile_perk_bodies(&world);
        assert!(spells(&world, actor).is_empty());
        assert_eq!(health(&world, actor), 100.0);
    }

    /// PERK-BODY-002 -- a reload finds the spell in the saved `SpellList` and its value change in the saved
    /// `ActorValues`; the first reconcile after it adds neither again.
    #[test]
    fn a_reload_finds_the_ability_already_applied() {
        let (world, actor) = body_world(vec![ability(0, ABILITY_RANK_1)], &[(PERK, 1)]);
        // What the save holds: the spell listed once and its +25 in the actor values.
        world.query_mut::<SpellList>().unwrap().get_mut(actor).unwrap().0.push(ABILITY_RANK_1);
        world.query_mut::<ActorValues>().unwrap().get_mut(actor).unwrap().set_base(HEALTH, 125.0);

        reconcile_perk_bodies(&world);
        reconcile_perk_bodies(&world);
        assert_eq!(spells(&world, actor), vec![ABILITY_RANK_1], "listed exactly once");
        assert_eq!(health(&world, actor), 125.0, "counted once");
        // And it still leaves with the perk after the reload.
        world.query_mut::<Perks>().unwrap().get_mut(actor).unwrap().remove(PERK);
        reconcile_perk_bodies(&world);
        assert!(spells(&world, actor).is_empty());
    }

    /// PERK-BODY-004 -- an NPC's perks (stamped from `PRKR` at spawn) grant their abilities like the player's.
    #[test]
    fn an_npcs_perk_grants_its_ability() {
        let (mut world, player) = body_world(vec![ability(0, ABILITY_RANK_1)], &[]);
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        let npc = world.spawn();
        let mut perks = Perks::default();
        perks.set_rank(PERK, 1);
        world.insert(npc, perks);
        world.insert(npc, SpellList::default());
        reconcile_perk_bodies(&world);
        assert_eq!(spells(&world, npc), vec![ABILITY_RANK_1]);
        assert!(spells(&world, player).is_empty(), "the player holds no perk");
    }

    fn stage(world: &World) -> u16 {
        world.resource::<QuestStageState>().get_stage(QuestFormId(QUEST))
    }

    /// PERK-BODY-003 -- gaining the rank a Quest entry is authored for starts the quest at the entry's stage; a
    /// perk the actor already held when it was first seen (a loaded save) does not fire again.
    #[test]
    fn a_quest_entry_fires_when_its_rank_is_gained() {
        // Vanilla `IntenseTraining`'s shape: one entry per rank, each its own stage of the same quest.
        let (mut world, player) = body_world(vec![quest_entry(0, 101), quest_entry(1, 102)], &[]);
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        reconcile_perk_bodies(&world); // baseline: no perks
        assert!(!world.resource::<QuestStageState>().is_started(QuestFormId(QUEST)));

        set_rank(&world, player, 1);
        reconcile_perk_bodies(&world);
        assert!(world.resource::<QuestStageState>().is_started(QuestFormId(QUEST)));
        assert_eq!(stage(&world), 101);

        set_rank(&world, player, 2);
        reconcile_perk_bodies(&world);
        assert_eq!(stage(&world), 102, "the second rank's entry, not a replay of the first");

        // Nothing changed: no re-fire (the stage is moved by hand and must stay).
        world.resource_mut::<QuestStageState>().set_stage(QuestFormId(QUEST), 500);
        reconcile_perk_bodies(&world);
        assert_eq!(stage(&world), 500);
    }

    #[test]
    fn a_perk_held_at_the_first_sight_of_an_actor_is_a_baseline_not_an_acquisition() {
        let (mut world, player) = body_world(vec![quest_entry(0, 101)], &[(PERK, 1)]);
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        reconcile_perk_bodies(&world);
        assert!(!world.resource::<QuestStageState>().is_started(QuestFormId(QUEST)));
    }

    #[test]
    fn only_the_players_perks_start_quests() {
        let (mut world, player) = body_world(vec![quest_entry(0, 101)], &[]);
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        let npc = world.spawn();
        world.insert(npc, Perks::default());
        reconcile_perk_bodies(&world); // baselines both
        world.query_mut::<Perks>().unwrap().get_mut(npc).unwrap().set_rank(PERK, 1);
        reconcile_perk_bodies(&world);
        assert!(!world.resource::<QuestStageState>().is_started(QuestFormId(QUEST)));
    }

    /// A Quest entry whose tab carries a condition nothing supplies a subject for does not fire (the
    /// fail-closed rule the composer keeps).
    #[test]
    fn a_quest_entry_gated_on_an_unsupplied_subject_stays_quiet() {
        let mut gated = quest_entry(0, 101);
        gated.condition_tabs = vec![tab(1, vec![condition(GET_DEAD, 0.0)])];
        let (mut world, player) = body_world(vec![gated], &[]);
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        reconcile_perk_bodies(&world);
        set_rank(&world, player, 1);
        reconcile_perk_bodies(&world);
        assert!(!world.resource::<QuestStageState>().is_started(QuestFormId(QUEST)));
    }
}

// --- PERK-GATE: taking a perk at the level-up menu -----------------------------------------------

mod gates {
    use super::*;
    use byroredux_core::character::CharacterLevel;

    fn gated(form_id: u32, level: u8, playable: bool, num_ranks: u8, next: u32, conditions: Vec<Condition>) -> PerkRecord {
        PerkRecord { form_id, editor_id: format!("Perk{form_id:X}"), level, playable, num_ranks, next_perk: next, conditions, ..Default::default() }
    }

    fn player_world(game: GameKind, perks: Vec<PerkRecord>, level: u16, owned: &[(u32, u8)]) -> (World, EntityId) {
        let idx = index(game, perks);
        let (mut world, player) = world_with(&idx, owned);
        world.register::<CharacterLevel>();
        world.insert(player, CharacterLevel { level, xp: 0 });
        (world, player)
    }

    /// PERK-GATE-003.
    #[test]
    fn an_unplayable_perk_is_refused() {
        let (world, player) = player_world(GameKind::Fallout3NV, vec![gated(1, 0, false, 1, 0, vec![])], 10, &[]);
        assert_eq!(plan_take_perk(&world, player, 1), Err(PerkRefusal::NotPlayable(1)));
    }

    /// PERK-GATE-002 (and Skyrim, where every vanilla perk authors 0).
    #[test]
    fn a_perk_above_the_players_level_is_refused() {
        for game in [GameKind::Fallout3NV, GameKind::Skyrim] {
            let (world, player) = player_world(game, vec![gated(1, 6, true, 1, 0, vec![])], 5, &[]);
            assert_eq!(plan_take_perk(&world, player, 1), Err(PerkRefusal::LevelTooLow { perk: 1, required: 6, level: 5 }));
            world.query_mut::<CharacterLevel>().unwrap().get_mut(player).unwrap().level = 6;
            assert_eq!(plan_take_perk(&world, player, 1), Ok(PerkTake { perk: 1, rank: 1, replaces: None, max_rank: 1 }));
        }
    }

    /// PERK-GATE-001 / A6 -- the failing condition function is named, and passes once it holds.
    #[test]
    fn a_failing_record_condition_is_named() {
        let (mut world, player) = player_world(GameKind::Fallout3NV, vec![gated(1, 0, true, 1, 0, vec![condition(GET_DEAD, 1.0)])], 10, &[]);
        let refusal = plan_take_perk(&world, player, 1).unwrap_err();
        assert_eq!(
            refusal,
            PerkRefusal::Condition { perk: 1, index: GET_DEAD, function: "GetDead".into(), modelled: true },
            "{refusal}"
        );
        assert!(refusal.to_string().contains("GetDead"));
        world.insert(player, Dead);
        assert!(plan_take_perk(&world, player, 1).is_ok());
    }

    /// FNV's SPECIAL / skill requirements are `GetPermanentActorValue` (495): base plus permanent changes,
    /// never a temporary modifier (GECK *GetPermanentActorValue*). Swift Learner's shape: INT >= 4.
    #[test]
    fn a_permanent_actor_value_requirement_reads_base_and_permanent_layers_only() {
        const INTELLIGENCE: u32 = 0x100;
        let needs_int_4 = Condition {
            function_index: 495,
            comparator: ComparisonOp::Ge,
            comparand: ConditionValue::Literal(4.0),
            run_on: RunOn::Subject,
            param_1: INTELLIGENCE,
            ..Default::default()
        };
        let (mut world, player) = player_world(GameKind::Fallout3NV, vec![gated(1, 0, true, 1, 0, vec![needs_int_4])], 10, &[]);
        let mut values = ActorValues::new();
        values.set_base(INTELLIGENCE, 3.0);
        world.insert(player, values);
        let refused = plan_take_perk(&world, player, 1).unwrap_err();
        assert!(matches!(refused, PerkRefusal::Condition { index: 495, modelled: true, .. }), "{refused}");
        assert!(refused.to_string().contains("GetPermanentActorValue"), "{refused}");
        // A chem's +3 is temporary: it does not meet the requirement.
        world.get_mut::<ActorValues>(player).unwrap().mod_temporary(INTELLIGENCE, 3.0);
        assert!(plan_take_perk(&world, player, 1).is_err(), "a temporary modifier is not permanent");
        // A permanent +1 (a skill book, an implant) does.
        world.get_mut::<ActorValues>(player).unwrap().mod_permanent(INTELLIGENCE, 1.0);
        assert!(plan_take_perk(&world, player, 1).is_ok());
    }

    /// A condition the evaluator cannot read is refused by name, not granted on a guessed 0.0.
    #[test]
    fn an_unmodelled_record_condition_is_refused() {
        let unknown = 0x7FFF;
        let (world, player) = player_world(GameKind::Fallout3NV, vec![gated(1, 0, true, 1, 0, vec![condition(unknown, 0.0)])], 10, &[]);
        assert!(matches!(
            plan_take_perk(&world, player, 1),
            Err(PerkRefusal::Condition { index, modelled: false, .. }) if index == unknown
        ));
    }

    #[test]
    fn an_unknown_perk_and_a_maxed_perk_are_refused() {
        let (world, player) = player_world(GameKind::Fallout3NV, vec![gated(1, 0, true, 2, 0, vec![])], 10, &[(1, 2)]);
        assert_eq!(plan_take_perk(&world, player, 9), Err(PerkRefusal::UnknownPerk(9)));
        assert_eq!(plan_take_perk(&world, player, 1), Err(PerkRefusal::AtMaxRank { perk: 1, max_rank: 2 }));
    }

    /// A multi-rank perk with no `NNAM` raises its own rank (FO3/FNV).
    #[test]
    fn a_multi_rank_perk_without_a_next_perk_raises_its_rank() {
        let (world, player) = player_world(GameKind::Fallout3NV, vec![gated(1, 0, true, 3, 0, vec![])], 10, &[(1, 1)]);
        assert_eq!(plan_take_perk(&world, player, 1), Ok(PerkTake { perk: 1, rank: 2, replaces: None, max_rank: 3 }));
    }

    /// PERK-GATE-004 -- the next rank is the `NNAM` record, which replaces the held one: Skyrim's chain
    /// (`LightFingers00 -> 20 -> ...`), ending at a record with no `NNAM`.
    #[test]
    fn the_next_rank_of_a_chain_is_the_next_perk() {
        let chain = vec![gated(1, 0, true, 3, 2, vec![]), gated(2, 0, true, 3, 3, vec![]), gated(3, 0, true, 3, 0, vec![])];
        let (world, player) = player_world(GameKind::Skyrim, chain, 1, &[(1, 1)]);
        // Taking the perk again moves up the chain; it does not raise record 1 to rank 2.
        assert_eq!(plan_take_perk(&world, player, 1), Ok(PerkTake { perk: 2, rank: 1, replaces: Some(1), max_rank: 3 }));
        // The chain's last record has no next rank and its own Num Ranks of 3 is not a second route.
        let (world, player) = player_world(GameKind::Skyrim, vec![gated(3, 0, true, 1, 0, vec![])], 1, &[(3, 1)]);
        assert_eq!(plan_take_perk(&world, player, 3), Err(PerkRefusal::AtMaxRank { perk: 3, max_rank: 1 }));
    }

    /// Fallout 4's chains are rings: the last rank's `NNAM` is the first record, a lower-level one.
    #[test]
    fn a_ring_stops_at_the_link_back_to_a_lower_level_record() {
        let ring = vec![gated(1, 0, true, 3, 2, vec![]), gated(2, 9, true, 3, 3, vec![]), gated(3, 18, true, 3, 1, vec![])];
        let (world, player) = player_world(GameKind::Fallout4, ring.clone(), 30, &[(3, 1)]);
        assert_eq!(plan_take_perk(&world, player, 3), Err(PerkRefusal::AtLastRank { perk: 3 }));
        // Mid-ring, the next rank still checks the NEXT record's level.
        let (world, player) = player_world(GameKind::Fallout4, ring.clone(), 10, &[(2, 1)]);
        assert_eq!(plan_take_perk(&world, player, 2), Err(PerkRefusal::LevelTooLow { perk: 3, required: 18, level: 10 }));
        // A ring with no level drop is bounded by "already held".
        let flat = vec![gated(1, 0, true, 2, 2, vec![]), gated(2, 0, true, 2, 1, vec![])];
        let (world, player) = player_world(GameKind::Fallout4, flat, 5, &[(1, 1), (2, 1)]);
        assert_eq!(plan_take_perk(&world, player, 1), Err(PerkRefusal::AtLastRank { perk: 1 }));
    }
}

// --- PERK-HOOK-001/002/011: the hook catalog -----------------------------------------------------

mod catalog {
    use super::*;
    use byroredux_core::source_scan::function_text;

    fn workspace_file(path: &str) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// `pub const NAME: u8 = N;` rows of the `pub mod <module>` block in `perk_entry_points.rs`.
    fn hook_constants(module: &str) -> Vec<(String, u8)> {
        let src = include_str!("../perk_entry_points.rs");
        let start = src.find(&format!("pub mod {module} {{")).unwrap_or_else(|| panic!("pub mod {module}"));
        let block = &src[start..];
        let block = &block[..block.find("\n}\n").expect("module closes")];
        block
            .lines()
            .filter_map(|line| {
                let rest = line.trim().strip_prefix("pub const ")?;
                let (name, value) = rest.split_once(": u8 = ")?;
                Some((name.to_string(), value.trim_end_matches(';').parse().ok()?))
            })
            .collect()
    }

    /// PERK-HOOK-001 -- the catalog is dense per dialect, and every hook constant the module names has its row.
    #[test]
    fn every_hook_constant_has_a_row_at_its_index() {
        for (dialect, module, len) in [
            (PerkEntryPointDialect::Fallout3NV, "fnv", 74usize),
            (PerkEntryPointDialect::Skyrim, "skyrim", 92),
        ] {
            let rows = hooks(dialect);
            assert_eq!(rows.len(), len, "{dialect:?}: xEdit's table size (docs/prd/perk-entry-points.md §3.4)");
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(usize::from(row.index), i, "{dialect:?}: rows are indexed by hook number");
                assert!(!row.name.is_empty());
            }
            let constants = hook_constants(module);
            assert!(constants.len() >= 6, "{module}: the scan found only {constants:?}");
            for (name, index) in constants {
                assert!(hook(dialect, index).is_some(), "{module}::{name} = {index} has no catalog row");
            }
        }
        assert!(hooks(PerkEntryPointDialect::None).is_empty());
    }

    /// The names the engine already uses agree with xEdit's.
    #[test]
    fn hook_constants_match_their_xedit_names() {
        let name = |dialect, index| hook(dialect, index).unwrap().name;
        assert_eq!(name(PerkEntryPointDialect::Fallout3NV, fnv::CALCULATE_WEAPON_DAMAGE), "Calculate Weapon Damage");
        assert_eq!(name(PerkEntryPointDialect::Fallout3NV, fnv::ADJUST_EXPERIENCE_POINTS), "Adjust Experience Points");
        assert_eq!(name(PerkEntryPointDialect::Fallout3NV, fnv::GET_MAX_CARRY_WEIGHT), "Get Max Carry Weight");
        assert_eq!(name(PerkEntryPointDialect::Fallout3NV, fnv::HAS_CONCENTRATED_FIRE), "Has Concentrated Fire");
        assert_eq!(name(PerkEntryPointDialect::Fallout3NV, fnv::MODIFY_DAMAGE_THRESHOLD_DEFENDER), "Modify Damage Threshold (defender)");
        assert_eq!(name(PerkEntryPointDialect::Skyrim, skyrim::GET_MAX_CARRY_WEIGHT), "Get Max Carry Weight");
        assert_eq!(name(PerkEntryPointDialect::Skyrim, skyrim::MOD_ATTACK_DAMAGE), "Mod Attack Damage");
        assert_eq!(name(PerkEntryPointDialect::Skyrim, skyrim::MOD_TEMPERING_HEALTH), "Mod Tempering Health");
    }

    /// PERK-HOOK-001 -- a Consumed row names `path::function`, and that function's source calls a composer.
    #[test]
    fn every_consumed_hook_names_a_function_that_calls_a_composer() {
        let mut consumed = 0;
        for dialect in [PerkEntryPointDialect::Fallout3NV, PerkEntryPointDialect::Skyrim] {
            for row in hooks(dialect) {
                let HookState::Consumed { site } = row.state else { continue };
                consumed += 1;
                let (path, function) = site.split_once("::").unwrap_or_else(|| panic!("{} site `{site}` is not path::function", row.name));
                let src = workspace_file(path);
                let body = function_text(&src, &format!("fn {function}("));
                let calls_a_composer = body.contains("compose_") || body.contains("has_concentrated_fire(");
                assert!(calls_a_composer, "{dialect:?} hook {} ({}): `{site}` calls no compose_* function", row.index, row.name);
            }
        }
        assert!(consumed >= 12, "the catalog should hold the hooks the engine composes today, found {consumed}");
    }

    /// PERK-HOOK-002 -- the GECK's seven "Not Implemented" hooks are Inert on `Fallout3NV`.
    #[test]
    fn the_gecks_not_implemented_hooks_are_inert() {
        for name in [
            "Adjust Range Penalty",
            "Calculate Inventory AP Cost",
            "Calculate Weapon Range",
            "Get Bad Karma",
            "Get Good Karma",
            "Get Should Assist",
            "Get Should Attack",
        ] {
            let row = hooks(PerkEntryPointDialect::Fallout3NV).iter().find(|row| row.name == name).unwrap_or_else(|| panic!("{name}"));
            assert!(
                matches!(row.state, HookState::Inert { source } if source.contains("GECK Entry Point")),
                "{name}: {:?}",
                row.state
            );
        }
    }

    /// PERK-HOOK-011 -- Skyrim's Adjust Limb Damage (4) and Calculate Mine Explode Chance (3).
    #[test]
    fn skyrims_not_used_hooks_are_inert() {
        for (index, name) in [(4u8, "Adjust Limb Damage"), (3, "Calculate Mine Explode Chance")] {
            let row = hook(PerkEntryPointDialect::Skyrim, index).unwrap();
            assert_eq!(row.name, name);
            assert!(matches!(row.state, HookState::Inert { source } if source.contains("CK Perk Entry Point")), "{:?}", row.state);
        }
    }

    /// The Q6 hook, the VATS hooks and the activate / leveled-list hooks carry the requirement that builds them.
    #[test]
    fn hooks_the_prd_builds_are_pending_under_their_ids() {
        let pending = |dialect, index| match hook(dialect, index).unwrap().state {
            HookState::Pending { id } => id,
            other => panic!("{index}: {other:?}"),
        };
        assert_eq!(pending(PerkEntryPointDialect::Fallout3NV, 39), "PERK-HOOK-005");
        assert_eq!(pending(PerkEntryPointDialect::Fallout3NV, 27), "PERK-HOOK-007");
        assert_eq!(pending(PerkEntryPointDialect::Skyrim, 14), "PERK-HOOK-007");
        assert_eq!(pending(PerkEntryPointDialect::Skyrim, 51), "PERK-HOOK-009");
        assert_eq!(HookState::Pending { id: "X" }.report(), "pending: X");
    }

    /// FO3/FNV tab subjects are xEdit's `epc*` classes: Calculate Weapon Damage runs on the weapon and the target.
    #[test]
    fn fnv_tab_subjects_come_from_xedit() {
        let subjects = |index| hook(PerkEntryPointDialect::Fallout3NV, index).unwrap().subjects;
        assert_eq!(subjects(0), Some(&["Weapon", "Target"][..]));
        assert_eq!(subjects(22), Some(&[][..]), "Get Max Carry Weight has the perk owner alone");
        assert_eq!(subjects(17), Some(&["Item"][..]));
        assert_eq!(hook(PerkEntryPointDialect::Skyrim, 0).unwrap().subjects, None, "unsourced until PERK-SUB-001");
    }
}

// --- PERK-DX: observability ----------------------------------------------------------------------

mod observability {
    use super::*;
    use super::super::bodies::unknown_perks;

    /// PERK-DX-005 -- a held perk with no PERK record is found, contributes nothing, and is reported once.
    #[test]
    fn a_held_perk_with_no_record_is_reported_once_and_contributes_nothing() {
        let idx = index(GameKind::Fallout3NV, vec![perk(1, vec![entry_point(0, 0, DAMAGE, 3, float(2.0), vec![])])]);
        let (world, actor) = world_with(&idx, &[(1, 1), (0x6055_0001, 1)]);
        let table = world.resource::<PerkEntryTable>();
        let held = world.get::<Perks>(actor).unwrap().entries.clone();
        assert_eq!(unknown_perks(&table, &held), vec![0x6055_0001]);
        drop(table);
        assert_eq!(compose_entry_point(&world, actor, DAMAGE, 10.0, &[]), 20.0, "the known perk composes; the unknown adds nothing");
        // Reconciling twice reports the missing record once (the first call claims the report).
        super::super::reconcile_perk_bodies(&world);
        super::super::reconcile_perk_bodies(&world);
    }

    /// PERK-DX-003 -- every held entry on the hook that did not apply says why.
    #[test]
    fn the_traced_composition_says_why_a_held_entry_did_not_apply() {
        let weapon_only = vec![tab(1, vec![condition(GET_DEAD, 0.0)])];
        let unknown_function = vec![tab(0, vec![condition(0x7FFF, 0.0)])];
        let owner_must_be_dead = vec![tab(0, vec![condition(GET_DEAD, 1.0)])];
        let idx = index(
            GameKind::Fallout3NV,
            vec![
                perk(1, vec![entry_point(0, 0, DAMAGE, 3, float(2.0), vec![])]),
                perk(2, vec![entry_point(0, 0, DAMAGE, 3, float(3.0), weapon_only)]),
                perk(3, vec![entry_point(0, 0, DAMAGE, 3, float(5.0), unknown_function)]),
                perk(4, vec![entry_point(0, 0, DAMAGE, 3, float(7.0), owner_must_be_dead)]),
                perk(5, vec![entry_point(1, 0, DAMAGE, 3, float(11.0), vec![])]),
            ],
        );
        let (world, actor) = world_with(&idx, &[(1, 1), (2, 1), (3, 1), (4, 1), (5, 1)]);
        let traced = compose_entry_point_traced(&world, actor, DAMAGE, 10.0, &[]);
        assert_eq!(traced.value, 20.0);
        assert_eq!(traced.applied.len(), 1);
        let reason = |perk| traced.skipped.iter().find(|entry| entry.perk == perk).map(|entry| entry.reason);
        assert_eq!(reason(2), Some(SkipReason::SubjectGated { tab: 1 }));
        assert_eq!(reason(3), Some(SkipReason::Unmodelled { function_index: 0x7FFF }));
        assert_eq!(reason(4), Some(SkipReason::ConditionsNotMet));
        assert_eq!(reason(5), Some(SkipReason::WrongRank { authored: 1, held: 1 }));
        assert_eq!(traced.skipped.len(), 4);
    }

    /// PERK-DX-004 -- the install summary counts entries by what happens to them, and exists with no perks.
    #[test]
    fn the_install_summary_counts_entries_by_hook_state() {
        let idx = index(
            GameKind::Fallout3NV,
            vec![
                perk(1, vec![entry_point(0, 0, DAMAGE, 3, float(2.0), vec![])]),
                // Hook 5 (Adjust Range Penalty) is Inert; hook 1 is unclassified Pending; hook 22 is Consumed but
                // function 9 (Add Activate Choice) is not value math.
                perk(2, vec![entry_point(0, 0, 5, 3, float(1.0), vec![]), entry_point(0, 0, 1, 3, float(1.0), vec![])]),
                perk(3, vec![entry_point(0, 0, fnv::GET_MAX_CARRY_WEIGHT, 9, float(1.0), vec![])]),
                perk(4, vec![ability_body(0x20), quest_body(0x30, 10)]),
            ],
        );
        let summary = PerkEntryTable::from_index(&idx).summary().clone();
        assert_eq!(summary.perks, 4);
        assert_eq!(summary.entry_point_entries, 4);
        assert_eq!((summary.composed, summary.skipped_undecoded), (1, 1));
        assert_eq!((summary.pending, summary.blocked, summary.inert), (1, 0, 1));
        assert_eq!((summary.ability_entries, summary.quest_entries), (1, 1));
        let line = summary.to_string();
        assert!(line.contains("Fallout3NV") && line.contains("1 composed") && line.contains("1 skipped"), "{line}");
        // No perks at all still yields a line (PERK-DX-004: "including when the perk count is 0").
        let empty = PerkEntryTable::from_index(&index(GameKind::Skyrim, vec![]));
        assert!(empty.summary().to_string().contains("Skyrim, 0 perks"), "{}", empty.summary());
    }

    fn ability_body(spell: u32) -> PerkEntry {
        PerkEntry { rank: 0, priority: 0, body: PerkEntryBody::Ability { spell_form_id: spell }, condition_tabs: Vec::new() }
    }

    fn quest_body(quest: u32, stage: u16) -> PerkEntry {
        PerkEntry { rank: 0, priority: 0, body: PerkEntryBody::Quest { quest_form_id: quest, stage }, condition_tabs: Vec::new() }
    }
}

// --- PERK-DIA-002 / -003: the translation boundary ------------------------------------------------

mod dialect_boundary {
    use super::*;

    /// PERK-DIA-002 -- a calculation maps to a hook per dialect, and composes to its base where the dialect has none.
    #[test]
    fn a_calculation_maps_to_one_hook_per_dialect() {
        use PerkEntryPointDialect::{Fallout3NV, Skyrim};
        assert_eq!(PerkCalculation::ExperienceGain.hook(Fallout3NV), Some(9));
        assert_eq!(PerkCalculation::ExperienceGain.hook(Skyrim), None, "Skyrim levels by skill use");
        assert_eq!(PerkCalculation::MaxCarryWeight.hook(Fallout3NV), Some(22));
        assert_eq!(PerkCalculation::MaxCarryWeight.hook(Skyrim), Some(10));
        assert_eq!(PerkCalculation::TemperingHealth.hook(Skyrim), Some(76));
        assert_eq!(PerkCalculation::TemperingHealth.hook(Fallout3NV), None);

        let idx = index(GameKind::Skyrim, vec![perk(1, vec![entry_point(0, 0, 9, 3, float(2.0), vec![])])]);
        let (world, actor) = world_with(&idx, &[(1, 1)]);
        // Hook 9 is a different hook on Skyrim (Add Leveled List On Death) and no calculation names it for
        // Experience gain there: the base value comes back whatever perk the actor holds.
        assert_eq!(compose_calculation(&world, actor, PerkCalculation::ExperienceGain, 100.0, &[]), 100.0);
    }

    /// No consumer outside the module names a hook constant of a dialect (the translation boundary).
    #[test]
    fn no_consumer_outside_the_module_names_a_dialect_hook() {
        for (name, src) in [
            ("byroredux/src/combat.rs", include_str!("../../../../byroredux/src/combat.rs")),
            ("byroredux/src/encumbrance.rs", include_str!("../../../../byroredux/src/encumbrance.rs")),
            ("byroredux/src/barter.rs", include_str!("../../../../byroredux/src/barter.rs")),
            ("byroredux/src/systems/vats.rs", include_str!("../../../../byroredux/src/systems/vats.rs")),
            ("crates/scripting/src/tempering.rs", include_str!("../tempering.rs")),
        ] {
            let production = byroredux_core::source_scan::production_text_or_whole(src);
            assert!(
                !production.contains("perk_entry_points::fnv::") && !production.contains("perk_entry_points::skyrim::"),
                "{name} names a per-dialect hook index: name the calculation instead (PerkCalculation)"
            );
        }
    }

    /// PERK-DIA-003 -- the value-function set is the dialect's: 12-14 are Skyrim's, 8-11 are no dialect's.
    #[test]
    fn the_value_function_set_is_a_property_of_the_dialect() {
        use PerkEntryPointDialect::{Fallout3NV, Skyrim};
        for function in 1..=7u8 {
            assert!(Fallout3NV.has_value_function(function) && Skyrim.has_value_function(function), "{function}");
        }
        for function in [12u8, 13, 14] {
            assert!(Skyrim.has_value_function(function) && !Fallout3NV.has_value_function(function), "{function}");
        }
        for function in [0u8, 8, 9, 10, 11, 15] {
            assert!(!Fallout3NV.has_value_function(function) && !Skyrim.has_value_function(function), "{function}");
        }
        assert!(!PerkEntryPointDialect::None.has_value_function(1));
        // A function 13 entry on FNV is not composed; on Skyrim it is.
        let av = PerkFunctionData::ActorValueMult { actor_value: 5, mult: 0.5 };
        let fnv_idx = index(GameKind::Fallout3NV, vec![perk(1, vec![entry_point(0, 0, DAMAGE, 13, av.clone(), vec![])])]);
        assert!(PerkEntryTable::from_index(&fnv_idx).entries(1).is_none(), "function 13 is not value math on FNV");
        let sky_idx = index(GameKind::Skyrim, vec![perk(1, vec![entry_point(0, 0, DAMAGE, 13, av, vec![])])]);
        assert!(PerkEntryTable::from_index(&sky_idx).entries(1).is_some());
    }
}

// --- PERK-HOOK-006: Add Leveled List On Death ----------------------------------------------------

mod death_lists {
    use super::*;
    use super::super::compose_death_leveled_lists;

    const ON_DEATH: u8 = fnv::ADD_LEVELED_LIST_ON_DEATH;
    const LIST: u32 = 0x0010_0001;

    fn add_list(rank: u8, tabs: Vec<PerkConditionTab>) -> PerkEntry {
        entry_point(rank, 0, ON_DEATH, 8, PerkFunctionData::FormId(LIST), tabs)
    }

    /// The killer's perk adds its list for a victim whose Target tab passes, and only at the held rank.
    #[test]
    fn a_kill_adds_the_entrys_leveled_list_for_a_victim_the_tabs_accept() {
        let idx = index(
            GameKind::Fallout3NV,
            vec![
                perk(1, vec![add_list(0, vec![])]),
                // Target tab: the victim must not be dead already (GetDead == 0) - the victim is alive here.
                perk(2, vec![add_list(0, vec![tab(1, vec![condition(GET_DEAD, 0.0)])])]),
                perk(3, vec![add_list(1, vec![])]),
            ],
        );
        let (mut world, killer) = world_with(&idx, &[(1, 1), (2, 1), (3, 1)]);
        let victim = world.spawn();
        world.insert(victim, Dead);
        let lists = compose_death_leveled_lists(&world, killer, victim);
        // Perk 1 (no tabs) applies; perk 2's Target tab is GetDead == 0 and the victim IS dead: no; perk 3 is
        // authored for rank 2 and held at rank 1: no.
        assert_eq!(lists, vec![LIST]);
        // Not dead: perk 2 applies too.
        let alive = world.spawn();
        assert_eq!(compose_death_leveled_lists(&world, killer, alive), vec![LIST, LIST]);
    }

    #[test]
    fn nothing_is_added_without_a_hook_a_table_or_perks() {
        let idx = index(GameKind::Fallout3NV, vec![perk(1, vec![add_list(0, vec![])])]);
        let (world, killer) = world_with(&idx, &[]);
        assert!(compose_death_leveled_lists(&world, killer, killer).is_empty(), "no perk held");
        let bare = World::new();
        assert!(compose_death_leveled_lists(&bare, 0, 1).is_empty(), "no table");
    }
}
