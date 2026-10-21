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
