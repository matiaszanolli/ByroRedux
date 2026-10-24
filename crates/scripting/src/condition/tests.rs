use super::*;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::{
    ComparisonOp, Condition, ConditionValue, RunOn,
};

#[test]
fn hardcore_condition_reads_the_live_mode_flag() {
    use byroredux_core::ecs::resources::HardcoreMode;
    let mut world = World::new();
    let actor = world.spawn();
    let condition = cond(586, ComparisonOp::Eq, 1.0, false);
    assert!(!evaluate_condition(&condition, &world, &ctx(actor)));
    world.insert_resource(HardcoreMode { enabled: true });
    assert!(evaluate_condition(&condition, &world, &ctx(actor)));
    world.resource_mut::<HardcoreMode>().enabled = false;
    assert!(!evaluate_condition(&condition, &world, &ctx(actor)));
}

/// #5979 — `entity_global_form_id` is the inverse of
/// `resolve_entity_by_global_form_id` for the player too: the player body's
/// pool pair is local 1, but the id scripts, ledgers and saved keys use is
/// `PlayerRef` (`0x14`). A placed reference still round-trips through its pair.
#[test]
fn entity_global_form_id_round_trips_the_player_as_player_ref() {
    use crate::papyrus_demo::PapyrusPlayerEntity;
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    let mut world = World::new();
    crate::register(&mut world);
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let player_pair = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(1),
    });
    let placed_pair = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x0001_0F90),
    });
    world.insert_resource(pool);
    let player = world.spawn();
    world.insert(player, FormIdComponent(player_pair));
    world.insert_resource(PapyrusPlayerEntity(player));
    let placed = world.spawn();
    world.insert(placed, FormIdComponent(placed_pair));

    assert_eq!(entity_global_form_id(&world, player), Some(crate::PLAYER_REFERENCE));
    assert_eq!(
        resolve_entity_by_global_form_id(&world, crate::PLAYER_REFERENCE),
        Some(player)
    );
    assert_eq!(entity_global_form_id(&world, placed), Some(0x0001_0F90));
    assert_eq!(resolve_entity_by_global_form_id(&world, 0x0001_0F90), Some(placed));
}

/// CK *GetPlayerTeammate*: "Returns 1 if an actor is currently set as a player
/// teammate via the Papyrus SetPlayerTeammate() function, 0 if not."
#[test]
fn get_player_teammate_reads_the_follower_flags() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    let mut world = World::new();
    crate::register(&mut world);
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x0001_0F90),
    });
    world.insert_resource(pool);
    let actor = world.spawn();
    world.insert(actor, FormIdComponent(id));
    let condition = cond(453, ComparisonOp::Eq, 1.0, false);
    assert!(!evaluate_condition(&condition, &world, &ctx(actor)));
    world.resource_mut::<crate::FollowerState>().set_teammate(0x0001_0F90, true, false);
    assert!(evaluate_condition(&condition, &world, &ctx(actor)));
    world.resource_mut::<crate::FollowerState>().set_teammate(0x0001_0F90, false, false);
    assert!(!evaluate_condition(&condition, &world, &ctx(actor)));
}

/// CK *GetKeywordDataForLocation*: "returns this value, if any. It otherwise
/// returns 0."
#[test]
fn keyword_data_conditions_read_the_stored_float() {
    const LOCATION: u32 = 0x0001_0F93;
    const KEYWORD: u32 = 0x0001_0F94;
    let mut world = World::new();
    crate::register(&mut world);
    let subject = world.spawn();
    let at_location = Condition {
        function_index: 606,
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(10.0),
        param_1: LOCATION,
        param_2: KEYWORD,
        ..Default::default()
    };
    let mut other_keyword = at_location;
    other_keyword.param_2 = KEYWORD + 1;
    assert!(!evaluate_condition(&at_location, &world, &ctx(subject)));
    world.resource_mut::<crate::LocationKeywordData>().set(LOCATION, KEYWORD, 10.0);
    assert!(evaluate_condition(&at_location, &world, &ctx(subject)));
    assert!(!evaluate_condition(&other_keyword, &world, &ctx(subject)), "another keyword reads 0");
}

fn ctx(subject: EntityId) -> ConditionContext<'static> {
    ConditionContext::for_subject(subject)
}

fn cond(
    function_index: u32,
    comparator: ComparisonOp,
    comparand: f32,
    or_next: bool,
) -> Condition {
    Condition {
        function_index,
        comparator,
        comparand: ConditionValue::Literal(comparand),
        run_on: RunOn::Subject,
        or_next,
        ..Default::default()
    }
}

/// #5990 — an index another game reuses for a different function must not run
/// the catalog row's function there: it is unmodeled (so the fail-closed
/// consumers decline it) and the fail-open evaluation passes it, exactly like
/// an uncatalogued function. The same index stays modeled on its own games.
#[test]
fn colliding_indices_are_unmodeled_on_the_games_that_reuse_them() {
    use byroredux_plugin::esm::reader::GameKind::{Fallout3NV, Fallout4, Oblivion, Skyrim, Starfield};

    // (index, a game that reuses it, a game the catalog row is right for)
    let cases = [
        (403, Fallout3NV, Skyrim),
        (449, Skyrim, Fallout3NV),
        (449, Fallout4, Fallout3NV),
        (533, Skyrim, Fallout3NV),
        (533, Fallout4, Fallout3NV),
        (550, Fallout3NV, Skyrim),
        (555, Fallout3NV, Fallout4),
        (630, Fallout4, Skyrim),
        // #5778 — 365 is Oblivion's GetPlayerInSEWorld, IsChild everywhere else.
        (365, Fallout3NV, Oblivion),
        (365, Skyrim, Oblivion),
        (365, Starfield, Oblivion),
    ];
    let mut world = World::new();
    crate::register(&mut world);
    let subject = world.spawn();
    for (index, foreign, native) in cases {
        // 12345 is never what the row's function reads for a bare subject.
        let condition = cond(index, ComparisonOp::Eq, 12345.0, false);

        world.insert_resource(crate::ScriptGame(foreign));
        assert!(!is_modeled(&world, &condition), "{index} is another function on {foreign:?}");
        assert!(
            evaluate_condition(&condition, &world, &ctx(subject)),
            "{index} passes through on {foreign:?} instead of running the wrong function"
        );

        world.insert_resource(crate::ScriptGame(native));
        assert!(is_modeled(&world, &condition), "{index} is modeled on {native:?}");
        assert!(
            !evaluate_condition(&condition, &world, &ctx(subject)),
            "{index} still evaluates its own function on {native:?}"
        );
    }
}

/// #5989 — an authored `GetActorValue` carries an enum code on Oblivion,
/// FO3/FNV and Skyrim, which the FormID-keyed evaluator would read as an absent
/// value (0.0); the fail-closed consumers must see it as unmodeled there.
#[test]
fn authored_get_actor_value_is_unmodeled_where_it_stores_an_enum_code() {
    use byroredux_plugin::esm::reader::GameKind;

    let mut world = World::new();
    crate::register(&mut world);
    let condition = cond(14, ComparisonOp::Ge, 25.0, false).with_param_1(0x2b);
    for game in [GameKind::Oblivion, GameKind::Fallout3NV, GameKind::Skyrim] {
        world.insert_resource(crate::ScriptGame(game));
        assert!(!is_modeled(&world, &condition), "{game:?} stores an actor-value enum code");
    }
    for game in [GameKind::Fallout4, GameKind::Starfield] {
        world.insert_resource(crate::ScriptGame(game));
        assert!(is_modeled(&world, &condition), "{game:?} stores an AVIF FormID");
    }
}

/// #5989 — an authored `GetActorValue` index is translated to the loaded AVIF
/// instead of being read as a FormID: Skyrim through the UESP index table
/// (`Speechcraft` 17 = AVIF `0x457`), Fallout 3 / New Vegas through the verified
/// `GetAV` enum (`Speech` 43). A `param_1` that is itself a loaded AVIF is an
/// engine-built FormID and passes through; Oblivion (no AVIF records) and an
/// index the record set lacks stay unreadable.
#[test]
fn authored_get_actor_value_index_translates_to_the_loaded_avif() {
    use byroredux_core::ecs::components::ActorValues;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::{AvifRecord, EsmIndex};

    let install = |game: GameKind, avifs: &[(u32, &str)]| {
        let mut index = EsmIndex { game, ..Default::default() };
        for &(form_id, editor_id) in avifs {
            index.actor_values.insert(
                form_id,
                AvifRecord { form_id, editor_id: editor_id.into(), ..Default::default() },
            );
        }
        let mut world = World::new();
        crate::register(&mut world);
        crate::install_actor_value_names(&mut world, &index);
        world
    };
    let check = |world: &mut World, pairs: &[(u32, f32)], authored: u32, op, bound: f32| {
        let actor = world.spawn();
        world.insert(actor, ActorValues::from_pairs(pairs.iter().copied()));
        let condition = cond(14, op, bound, false).with_param_1(authored);
        (is_modeled(world, &condition), evaluate_condition(&condition, world, &ctx(actor)))
    };

    // Skyrim: index 17 is Speechcraft = AVIF 0x457; engine FormID passes through.
    let mut skyrim = install(GameKind::Skyrim, &[(0x457, "AVSpeechcraft"), (0x3E8, "AVHealth")]);
    assert_eq!(check(&mut skyrim, &[(0x457, 30.0)], 17, ComparisonOp::Ge, 25.0), (true, true));
    assert_eq!(check(&mut skyrim, &[(0x457, 20.0)], 17, ComparisonOp::Ge, 25.0), (true, false));
    assert_eq!(
        check(&mut skyrim, &[(0x3E8, 90.0)], 0x3E8, ComparisonOp::Ge, 50.0),
        (true, true),
        "a loaded AVIF FormID is read as the FormID it is"
    );
    // The table lists AVIF 0x45A (Destruction, 20) but this load order has none.
    assert!(!check(&mut skyrim, &[], 20, ComparisonOp::Ge, 0.0).0, "no such AVIF loaded");
    assert!(!check(&mut skyrim, &[], 9999, ComparisonOp::Ge, 0.0).0, "outside the table");

    // Fallout 3 / New Vegas: index 43 is Speech.
    let mut fnv = install(GameKind::Fallout3NV, &[(0x3F5, "AVSpeech")]);
    assert_eq!(check(&mut fnv, &[(0x3F5, 30.0)], 43, ComparisonOp::Ge, 25.0), (true, true));
    assert_eq!(check(&mut fnv, &[(0x3F5, 10.0)], 43, ComparisonOp::Ge, 25.0), (true, false));
    assert!(!check(&mut fnv, &[], 21, ComparisonOp::Ge, 0.0).0, "SpeedMult has no AVIF in the master");

    // Oblivion has no AVIF records: unreadable, so the fail-closed gates decline.
    let mut oblivion = install(GameKind::Oblivion, &[]);
    assert!(!check(&mut oblivion, &[], 43, ComparisonOp::Ge, 0.0).0);

    // Fallout 4: always a FormID.
    let mut fo4 = install(GameKind::Fallout4, &[(0x2BC, "Strength")]);
    assert_eq!(check(&mut fo4, &[(0x2BC, 7.0)], 0x2BC, ComparisonOp::Ge, 5.0), (true, true));
}

#[test]
fn get_quest_variable_uses_obscript_locals_only_for_legacy_game_paths() {
    use byroredux_plugin::esm::reader::GameKind;

    const QUEST: u32 = 0x0001_2345;
    let mut world = World::new();
    crate::register(&mut world);
    let subject = world.spawn();
    let mut locals = crate::obscript_quests::ObScriptQuestLocals::default();
    locals.store(QUEST, &std::collections::HashMap::from([(3, 12.5)]));
    world.insert_resource(locals);
    let mut condition = cond(79, ComparisonOp::Eq, 12.5, false).with_param_1(QUEST);
    condition.param_2 = 3;

    for game in [GameKind::Oblivion, GameKind::Fallout3NV] {
        world.insert_resource(crate::ScriptGame(game));
        assert!(
            is_modeled(&world, &condition),
            "{game:?} models condition 79"
        );
        assert!(evaluate_condition(&condition, &world, &ctx(subject)));
    }

    // Without quest-local state, the legacy function reads its zero default.
    let mut empty = World::new();
    crate::register(&mut empty);
    empty.insert_resource(crate::ScriptGame(GameKind::Oblivion));
    let empty_subject = empty.spawn();
    assert!(evaluate_condition(
        &cond(79, ComparisonOp::Eq, 0.0, false).with_param_1(QUEST),
        &empty,
        &ctx(empty_subject),
    ));

    // Newer games use another function at index 79; preserve the pass-through.
    for game in [GameKind::Skyrim, GameKind::Fallout4] {
        world.insert_resource(crate::ScriptGame(game));
        assert!(
            !is_modeled(&world, &condition),
            "{game:?} does not model condition 79"
        );
        assert!(evaluate_condition(
            &cond(79, ComparisonOp::Eq, -1.0, false),
            &world,
            &ctx(subject),
        ));
    }
}

#[test]
fn empty_condition_list_returns_true() {
    let world = World::new();
    let result = evaluate(&Vec::new(), &world, &ctx(0));
    assert!(result, "no conditions = always fires");
}

#[test]
fn single_true_condition_returns_true() {
    use crate::quest_stages::{QuestFormId, QuestStageState};
    let mut world = World::new();
    crate::register(&mut world);
    let mut state = QuestStageState::default();
    state.set_stage(QuestFormId(0xAA), 42);
    world.insert_resource(state);

    // GetStage(0xAA) == 42
    let list = vec![cond(58, ComparisonOp::Eq, 42.0, false).with_param_1(0xAA)];
    assert!(evaluate(&list, &world, &ctx(0)));
}

#[test]
fn single_false_condition_returns_false() {
    use crate::quest_stages::{QuestFormId, QuestStageState};
    let mut world = World::new();
    crate::register(&mut world);
    let mut state = QuestStageState::default();
    state.set_stage(QuestFormId(0xAA), 42);
    world.insert_resource(state);

    // GetStage(0xAA) == 99 (actual is 42)
    let list = vec![cond(58, ComparisonOp::Eq, 99.0, false).with_param_1(0xAA)];
    assert!(!evaluate(&list, &world, &ctx(0)));
}

#[test]
fn unknown_function_index_returns_zero_and_satisfies_eq_zero() {
    // Bethesda's "unknown function safe-default" — function returns
    // 0.0, so a `== 0` comparator matches.
    let world = World::new();
    let list = vec![cond(99999, ComparisonOp::Eq, 0.0, false)];
    assert!(evaluate(&list, &world, &ctx(0)));
}

#[test]
fn and_chain_short_circuits_on_first_false() {
    // [false, true, true] = AND chain — first failure ends scan.
    let world = World::new();
    let list = vec![
        cond(99999, ComparisonOp::Ne, 0.0, false), // false (Unknown→0, !=0 fails)
        cond(99999, ComparisonOp::Eq, 0.0, false), // would be true
    ];
    assert!(!evaluate(&list, &world, &ctx(0)));
}

#[test]
fn or_block_returns_true_when_any_member_true() {
    // [c0 OR c1 OR c2] — c0 false, c1 true short-circuits the block.
    let world = World::new();
    let list = vec![
        cond(99999, ComparisonOp::Ne, 0.0, true),  // false, or_next
        cond(99999, ComparisonOp::Eq, 0.0, true),  // true (block hit)
        cond(99999, ComparisonOp::Ne, 0.0, false), // would be false; block already true
    ];
    assert!(evaluate(&list, &world, &ctx(0)));
}

#[test]
fn or_precedence_quirk_a_and_b_or_c_and_d_groups_b_or_c() {
    // The load-bearing test: `A AND B OR C AND D` evaluates as
    // `A AND (B OR C) AND D`, NOT `(A AND B) OR (C AND D)`.
    //
    // We construct:
    //   A: false (failing condition)
    //   B: true, or_next=true   ┐
    //   C: true, or_next=false  ┘  → (B OR C) = true
    //   D: true
    //
    // Standard interpretation `(A AND B) OR (C AND D)` would be
    // `(false AND true) OR (true AND true)` = false OR true = true.
    // OR-precedence interpretation `A AND (B OR C) AND D` is
    // `false AND true AND true` = false.
    //
    // Expecting the OR-precedence interpretation → false.
    let world = World::new();
    let list = vec![
        cond(99999, ComparisonOp::Ne, 0.0, false), // A: false
        cond(99999, ComparisonOp::Eq, 0.0, true),  // B: true, or_next
        cond(99999, ComparisonOp::Eq, 0.0, false), // C: true (no or_next: end of OR block)
        cond(99999, ComparisonOp::Eq, 0.0, false), // D: true
    ];
    assert!(
        !evaluate(&list, &world, &ctx(0)),
        "OR-precedence: A AND (B OR C) AND D = false AND (true OR true) AND true = false"
    );
}

#[test]
fn or_precedence_quirk_swap_test_a_true() {
    // Flip A to true so the AND chain succeeds — proves the
    // OR-precedence grouping isn't just always-false.
    //
    //   A: true
    //   B: false, or_next=true   ┐
    //   C: true,  or_next=false  ┘  → (B OR C) = true
    //   D: true
    //
    // → true AND (false OR true) AND true = true
    let world = World::new();
    let list = vec![
        cond(99999, ComparisonOp::Eq, 0.0, false), // A: true (Unknown=0, ==0)
        cond(99999, ComparisonOp::Ne, 0.0, true),  // B: false, or_next
        cond(99999, ComparisonOp::Eq, 0.0, false), // C: true
        cond(99999, ComparisonOp::Eq, 0.0, false), // D: true
    ];
    assert!(evaluate(&list, &world, &ctx(0)));
}

/// SCR-D6-NEW-01: a single condition whose `or_next` is set (a
/// malformed/truncated CTDA tail leaving the OR bit on the FINAL
/// member). The block-discovery loop walks the index to `len`; without
/// the clamp the inclusive range indexes one past the end and panics
/// when no earlier member short-circuits true. With the only member
/// false, evaluation must reach the clamp and return `false`, not panic.
#[test]
fn trailing_or_next_on_final_condition_does_not_panic() {
    let world = World::new();
    let list = vec![cond(99999, ComparisonOp::Ne, 0.0, true)]; // false, or_next
    assert!(
        !evaluate(&list, &world, &ctx(0)),
        "a trailing or_next on the final condition must clamp, not panic",
    );
}

/// SCR-D6-NEW-01, multi-member block: every OR member is false and the
/// LAST still carries `or_next`. The whole block is false, so the clamp
/// must be reached for all members (no short-circuit to mask the OOB).
#[test]
fn trailing_or_next_block_all_false_returns_false() {
    let world = World::new();
    let list = vec![
        cond(99999, ComparisonOp::Ne, 0.0, true), // false, or_next
        cond(99999, ComparisonOp::Ne, 0.0, true), // false, or_next (final → meaningless)
    ];
    assert!(
        !evaluate(&list, &world, &ctx(0)),
        "an all-false OR block ending in or_next must return false, not panic",
    );
}

#[test]
fn get_stage_returns_quest_state_stage() {
    use crate::quest_stages::{QuestFormId, QuestStageState};
    let mut world = World::new();
    crate::register(&mut world);
    let mut state = QuestStageState::default();
    state.set_stage(QuestFormId(0xAA), 42);
    world.insert_resource(state);

    // GetStage(0xAA) == 42
    let list = vec![cond(58, ComparisonOp::Eq, 42.0, false).with_param_1(0xAA)];
    assert!(evaluate(&list, &world, &ctx(0)));
    // GetStage(0xAA) < 100
    let list = vec![cond(58, ComparisonOp::Lt, 100.0, false).with_param_1(0xAA)];
    assert!(evaluate(&list, &world, &ctx(0)));
    // GetStage(0xAA) > 50 (false — actually 42)
    let list = vec![cond(58, ComparisonOp::Gt, 50.0, false).with_param_1(0xAA)];
    assert!(!evaluate(&list, &world, &ctx(0)));
}

// ── GetIsID (#1666) ─────────────────────────────────────────────────

/// #5041 — the param is the Run-On's base `NPC_`, not the placed ACHR:
/// an actor whose base is `param_1` passes even though its own
/// (reference) FormID differs, and naming the reference id fails.
#[test]
fn get_is_id_matches_run_on_base_object_not_placed_reference() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    const ACHR: u32 = 0x0001_4D8A;
    const NPC: u32 = 0x0000_A1B2;
    let mut world = World::new();
    let mut pool = FormIdPool::new();
    let fid = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("FalloutNV.esm"),
        local: LocalFormId(ACHR),
    });
    world.insert_resource(pool);
    let actor = world.spawn();
    world.insert(actor, FormIdComponent(fid));
    world.insert(
        actor,
        crate::scene::SceneAliasCandidate {
            reference_form_id: ACHR,
            base_form_id: NPC,
            ..Default::default()
        },
    );

    let list = vec![cond(72, ComparisonOp::Eq, 1.0, false).with_param_1(NPC)];
    assert!(evaluate(&list, &world, &ctx(actor)));
    let list = vec![cond(72, ComparisonOp::Eq, 0.0, false).with_param_1(ACHR)];
    assert!(
        evaluate(&list, &world, &ctx(actor)),
        "GetIsID must not match the placed reference's own FormID"
    );
    let list = vec![cond(72, ComparisonOp::Eq, 0.0, false).with_param_1(0x0001_9999)];
    assert!(evaluate(&list, &world, &ctx(actor)));
}

#[test]
fn get_is_id_zero_without_identity() {
    // No SceneAliasCandidate → GetIsID returns 0.0.
    let world = World::new();
    let actor: EntityId = 7;
    let list = vec![cond(72, ComparisonOp::Eq, 0.0, false).with_param_1(0x0001_4D8A)];
    assert!(evaluate(&list, &world, &ctx(actor)));
}

#[test]
fn get_vm_script_variable_reads_cis2_named_state_from_reference() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_plugin::esm::records::condition::ConditionStringId;

    const GATE: u32 = 0x0009_0A05;
    let mut world = World::new();
    crate::register(&mut world);
    let mut pool = FormIdPool::new();
    let gate_form_id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(GATE),
    });
    world.insert_resource(pool);
    let subject = world.spawn();
    let gate = world.spawn();
    world.insert(gate, FormIdComponent(gate_form_id));
    let mut variables = crate::ScriptVariables::default();
    variables.set_by_name("::isOpen_var", 1.0);
    world.insert(gate, variables);
    let mut condition = cond(630, ComparisonOp::Eq, 1.0, false).with_param_1(GATE);
    condition.param_2_text = Some(ConditionStringId::from_text("::isOpen_var"));

    assert!(evaluate(&vec![condition], &world, &ctx(subject)));
}

/// #5985 — a reference script the Papyrus VM runs keeps its variables under the
/// reference's FormID in the quest-variable store, not on the entity; CTDA 630
/// reads them there, whether or not the reference is loaded, and the entity
/// component (the lowered write path) still wins when it holds the variable.
#[test]
fn get_vm_script_variable_sees_a_vm_run_reference_scripts_variables() {
    use crate::quest_variables::QuestScriptVariables;
    use crate::QuestFormId;
    use byroredux_plugin::esm::records::condition::ConditionStringId;

    const REFERENCE: u32 = 0x0009_0A05;
    let mut world = World::new();
    crate::register(&mut world);
    let subject = world.spawn();
    let gate = |variable: &str, comparand: f32| {
        let mut condition = cond(630, ComparisonOp::Eq, comparand, false).with_param_1(REFERENCE);
        condition.param_2_text = Some(ConditionStringId::from_text(variable));
        vec![condition]
    };
    assert!(evaluate(&gate("::count_var", 0.0), &world, &ctx(subject)), "nothing written reads 0");

    // The VM's write (`write_quest_variable` under the reference FormID) — the
    // reference is not even loaded.
    world.init_resource::<QuestScriptVariables>();
    world
        .resource_mut::<QuestScriptVariables>()
        .set(QuestFormId(REFERENCE), QuestScriptVariables::property_key("count"), 3.0);
    assert!(evaluate(&gate("::count_var", 3.0), &world, &ctx(subject)));
    assert!(!evaluate(&gate("::count_var", 0.0), &world, &ctx(subject)));
}

/// M47.2 — `GetVMQuestVariable` (629) reads the named quest variable:
/// the scripted write, else the VMAD-authored value, else 0. A condition
/// without its CIS2 name reads 0.
#[test]
fn get_vm_quest_variable_reads_writes_then_authored_values() {
    use crate::quest_variables::{install_quest_script_variables, QuestScriptVariables};
    use crate::QuestFormId;
    use byroredux_plugin::esm::records::condition::ConditionStringId;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const MS11: u32 = 0x0004_4B4B;
    let mut world = World::new();
    let subject = world.spawn();
    install_quest_script_variables(
        &mut world,
        [(
            MS11,
            &ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "MS11QuestScript".into(),
                status: 0,
                properties: vec![ScriptProperty {
                    name: "FriendsKilled".into(),
                    status: 1,
                    value: PropertyValue::Int32(2),
                }],
            }],
            },
        )],
    );
    let gate = |variable: &str, comparand: f32| {
        let mut condition = cond(629, ComparisonOp::Eq, comparand, false).with_param_1(MS11);
        condition.param_2_text = Some(ConditionStringId::from_text(variable));
        vec![condition]
    };

    assert!(evaluate(&gate("::FriendsKilled_var", 2.0), &world, &ctx(subject)));
    assert!(evaluate(&gate("::Unwritten_var", 0.0), &world, &ctx(subject)));
    world
        .resource_mut::<QuestScriptVariables>()
        .set(QuestFormId(MS11), QuestScriptVariables::property_key("FriendsKilled"), 5.0);
    assert!(evaluate(&gate("::FriendsKilled_var", 5.0), &world, &ctx(subject)));
    assert!(!evaluate(&gate("::FriendsKilled_var", 2.0), &world, &ctx(subject)));

    let mut nameless = cond(629, ComparisonOp::Eq, 0.0, false).with_param_1(MS11);
    nameless.param_2_text = None;
    assert!(evaluate(&vec![nameless], &world, &ctx(subject)));
}

// ── GetDead / GetInCell (MQ101 scene gates) ─────────────────────────

#[test]
fn get_dead_tracks_sparse_actor_lifecycle_marker() {
    use byroredux_core::ecs::components::Dead;

    let mut world = World::new();
    let alive = world.spawn();
    let dead = world.spawn();
    world.insert(dead, Dead);

    let condition = vec![cond(46, ComparisonOp::Eq, 1.0, false)];
    assert!(!evaluate(&condition, &world, &ctx(alive)));
    assert!(evaluate(&condition, &world, &ctx(dead)));
}

#[test]
fn get_in_cell_resolves_authored_identity_through_cell_root() {
    use byroredux_core::ecs::components::{CellFormId, CellRoot};

    const HELGEN_KEEP: u32 = 0x000A_6A7D;
    let mut world = World::new();
    let root = world.spawn();
    let actor = world.spawn();
    let unowned = world.spawn();
    world.insert(root, CellRoot(root));
    world.insert(root, CellFormId(HELGEN_KEEP));
    world.insert(actor, CellRoot(root));

    let in_helgen = vec![cond(67, ComparisonOp::Eq, 1.0, false).with_param_1(HELGEN_KEEP)];
    assert!(evaluate(&in_helgen, &world, &ctx(actor)));
    assert!(evaluate(&in_helgen, &world, &ctx(root)));
    assert!(!evaluate(&in_helgen, &world, &ctx(unowned)));

    let in_other_cell = vec![cond(67, ComparisonOp::Eq, 1.0, false).with_param_1(0x0000_1234)];
    assert!(!evaluate(&in_other_cell, &world, &ctx(actor)));
}

// ── HasPerk (#1667) ─────────────────────────────────────────────────

#[test]
fn has_perk_checks_actor_perks() {
    use byroredux_core::character::Perks;

    let mut world = World::new();
    let actor = world.spawn();
    let mut perks = Perks::default();
    perks.set_rank(0x0005_8F80, 1);
    world.insert(actor, perks);

    // HasPerk(0x00058F80) == 1 — the actor holds it.
    let list = vec![cond(449, ComparisonOp::Eq, 1.0, false).with_param_1(0x0005_8F80)];
    assert!(evaluate(&list, &world, &ctx(actor)));

    // HasPerk(0x00058F81) == 0 — not held.
    let list = vec![cond(449, ComparisonOp::Eq, 0.0, false).with_param_1(0x0005_8F81)];
    assert!(evaluate(&list, &world, &ctx(actor)));
}

#[test]
fn has_perk_zero_without_perks() {
    // No Perks component → HasPerk returns 0.0.
    let world = World::new();
    let actor: EntityId = 3;
    let list = vec![cond(449, ComparisonOp::Eq, 0.0, false).with_param_1(0x0005_8F80)];
    assert!(evaluate(&list, &world, &ctx(actor)));
}

#[test]
fn catalog_names_round_trip_and_match_indices() {
    // Every catalog entry resolves by name (case-insensitively) back to
    // itself, and no entry is `Unknown`.
    for f in ConditionFunction::CATALOG {
        assert_ne!(f.name(), "Unknown");
        assert_eq!(ConditionFunction::from_name(f.name()), Some(f));
        assert_eq!(
            ConditionFunction::from_name(&f.name().to_lowercase()),
            Some(f)
        );
    }
    assert_eq!(ConditionFunction::from_name("nope"), None);
    // The console-facing name maps to the same variant `from_index` yields.
    assert_eq!(
        ConditionFunction::from_name("GetReputationThreshold"),
        Some(ConditionFunction::from_index(575))
    );
}

/// #5431 — CTDA fn 0 is not a catalogued function. It was briefly mapped
/// to a constant-`-1` `GetButtonPressed`, but xEdit has index 0 as
/// `GetWantBlocking` on TES5/FO4 and no index 0 on TES4/FO3/FNV, so that
/// mapping relabelled Skyrim's IDLE combat predicates. It must stay
/// `Unknown` (fail-open) until it is mapped from xEdit's table.
#[test]
fn function_index_zero_is_not_catalogued() {
    assert_eq!(ConditionFunction::from_index(0), ConditionFunction::Unknown(0));
    assert_eq!(ConditionFunction::from_name("GetButtonPressed"), None);
}

/// #5431 — rustdoc attaches a doc comment to the next item, so a variant
/// inserted between a doc comment and its variant silently re-homes the
/// doc (the same defect class as #5269). Pin `GetDistance`'s doc to it.
#[test]
fn get_distance_variant_keeps_its_doc_comment() {
    let src = include_str!("catalog.rs");
    let at = src
        .find("    GetDistance,\n")
        .expect("the GetDistance variant declaration");
    let doc_line = src[..at]
        .lines()
        .last()
        .expect("a line precedes the variant");
    assert!(
        doc_line.contains("FO3 / FNV / Skyrim index **1**"),
        "GetDistance's doc comment must sit directly above the variant, \
         found {doc_line:?}"
    );
}

// ── GetLevel / GetIsClass / GetIsRace / GetXPForNextLevel (CHARAL) ──

#[test]
fn get_level_reads_character_level() {
    use byroredux_core::character::CharacterLevel;
    let mut world = World::new();
    let actor = world.spawn();
    world.insert(actor, CharacterLevel { level: 12, xp: 40 });
    // GetLevel == 12.
    let list = vec![cond(80, ComparisonOp::Eq, 12.0, false)];
    assert!(evaluate(&list, &world, &ctx(actor)));
    // Absent component → 0.
    let bare: EntityId = 9;
    let list = vec![cond(80, ComparisonOp::Eq, 0.0, false)];
    assert!(evaluate(&list, &world, &ctx(bare)));
}

#[test]
fn get_equipped_resolves_occupied_inventory_item() {
    use byroredux_core::ecs::components::{EquipmentSlots, Inventory, ItemStack};

    const WORN: u32 = 0x0001_2E49;
    const CARRIED: u32 = 0x0001_2EB7;
    let mut world = World::new();
    let actor = world.spawn();
    let mut inventory = Inventory::new();
    let worn = inventory.push(ItemStack::new(WORN, 1));
    inventory.push(ItemStack::new(CARRIED, 1));
    let mut equipment = EquipmentSlots::new();
    equipment.equip(1 << 2, worn);
    world.insert(actor, inventory);
    world.insert(actor, equipment);

    assert!(evaluate(
        &vec![cond(182, ComparisonOp::Eq, 1.0, false).with_param_1(WORN)],
        &world,
        &ctx(actor),
    ));
    assert!(evaluate(
        &vec![cond(182, ComparisonOp::Eq, 0.0, false).with_param_1(CARRIED)],
        &world,
        &ctx(actor),
    ));
}

#[test]
fn has_loaded_3d_tracks_live_spatial_component() {
    use byroredux_core::ecs::components::GlobalTransform;

    let mut world = World::new();
    let loaded = world.spawn();
    let not_loaded = world.spawn();
    world.insert(loaded, GlobalTransform::default());

    let condition = vec![cond(555, ComparisonOp::Eq, 1.0, false)];
    assert!(evaluate(&condition, &world, &ctx(loaded)));
    assert!(!evaluate(&condition, &world, &ctx(not_loaded)));
}

/// #5778 — Oblivion's quest-level dialogue gates are `GetIsPlayableRace` (254)
/// and `GetPlayerInSEWorld` (365) on 339 of 390 quests. Both evaluate now:
/// the RACE Playable flag through `PlayableRaces`, the Shivering Isles flag
/// from the ledger `SetPlayerInSEWorld` writes.
#[test]
fn oblivion_quest_gate_functions_read_the_playable_flag_and_the_se_world_flag() {
    use byroredux_core::character::Background;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::{EsmIndex, RaceRecord};
    const IMPERIAL: u32 = 0x0000_0907;
    const DREMORA: u32 = 0x0003_1E53;
    let mut index = EsmIndex::default();
    for (form, flags) in [(IMPERIAL, 0x01), (DREMORA, 0x00)] {
        index.races.insert(
            form,
            RaceRecord {
                form_id: form,
                race_flags: flags,
                ..RaceRecord::default()
            },
        );
    }
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(GameKind::Oblivion));
    world.insert_resource(crate::obscript_more::PlayableRaces::from_index(&index));
    let race = |world: &mut World, race_form_id| {
        let actor = world.spawn();
        world.insert(
            actor,
            Background {
                race_form_id,
                class_form_id: 0,
            },
        );
        actor
    };
    let imperial = race(&mut world, IMPERIAL);
    let dremora = race(&mut world, DREMORA);

    let playable = vec![cond(254, ComparisonOp::Eq, 1.0, false)];
    assert!(is_modeled(&world, &playable[0]));
    assert!(evaluate(&playable, &world, &ctx(imperial)));
    assert!(!evaluate(&playable, &world, &ctx(dremora)));
    let raceless = world.spawn();
    assert!(!evaluate(&playable, &world, &ctx(raceless)), "no Background → 0");

    // The vanilla Tamriel gate: `GetPlayerInSEWorld == 0`.
    let in_tamriel = vec![cond(365, ComparisonOp::Eq, 0.0, false)];
    assert!(is_modeled(&world, &in_tamriel[0]));
    assert!(evaluate(&in_tamriel, &world, &ctx(imperial)), "a new game starts in Tamriel");
    world
        .resource_mut::<crate::obscript_ledger::ObScriptLedger>()
        .set_value(0x116c, 0, 1.0);
    assert!(!evaluate(&in_tamriel, &world, &ctx(imperial)), "SetPlayerInSEWorld 1");
}

/// #5778 — the two remaining functions of the Oblivion quest-level census
/// that every game authors at the same index: `GetIsSex` (70) off the
/// base's ACBS female flag, `GetInWorldspace` (310) off the cell kind.
#[test]
fn get_is_sex_and_get_in_worldspace_read_the_shared_helpers() {
    use crate::cell_kind::ExteriorCell;
    use byroredux_core::ecs::components::{CellFormId, CellRoot};
    use byroredux_plugin::esm::records::{EsmIndex, NpcRecord};
    const FEMALE: u32 = 0x0001_0001;
    const MALE: u32 = 0x0001_0002;
    const TAMRIEL: u32 = 0x0000_003C;
    let mut index = EsmIndex::default();
    for (form, acbs_flags) in [(FEMALE, 0x1), (MALE, 0x0)] {
        index.npcs.insert(
            form,
            NpcRecord {
                form_id: form,
                acbs_flags,
                ..Default::default()
            },
        );
    }
    let mut world = World::new();
    crate::register(&mut world);
    world.register::<ExteriorCell>();
    crate::actor_base_traits::install_actor_base_traits(&mut world, &index);
    let exterior = world.spawn();
    world.insert(exterior, CellFormId(2));
    world.insert(exterior, ExteriorCell(TAMRIEL));
    let interior = world.spawn();
    world.insert(interior, CellFormId(1));
    let actor = |world: &mut World, base_form_id, cell| {
        let actor = world.spawn();
        world.insert(
            actor,
            crate::scene::SceneAliasCandidate {
                base_form_id,
                ..Default::default()
            },
        );
        world.insert(actor, CellRoot(cell));
        actor
    };
    let outdoors_female = actor(&mut world, FEMALE, exterior);
    let indoors_male = actor(&mut world, MALE, interior);

    let female = vec![cond(70, ComparisonOp::Eq, 1.0, false).with_param_1(1)];
    let male = vec![cond(70, ComparisonOp::Eq, 1.0, false).with_param_1(0)];
    assert!(evaluate(&female, &world, &ctx(outdoors_female)));
    assert!(!evaluate(&male, &world, &ctx(outdoors_female)));
    assert!(evaluate(&male, &world, &ctx(indoors_male)));
    assert!(!evaluate(&female, &world, &ctx(indoors_male)));

    let in_tamriel = vec![cond(310, ComparisonOp::Eq, 1.0, false).with_param_1(TAMRIEL)];
    assert!(evaluate(&in_tamriel, &world, &ctx(outdoors_female)));
    assert!(!evaluate(&in_tamriel, &world, &ctx(indoors_male)), "0 in an interior");
    let elsewhere = vec![cond(310, ComparisonOp::Eq, 1.0, false).with_param_1(TAMRIEL + 1)];
    assert!(!evaluate(&elsewhere, &world, &ctx(outdoors_female)));
}

/// #5750 — `GetIsVoiceType` is catalogued at 426 (Skyrim / FO4) and 427 (FO3 /
/// FNV): true for the actor's `VTCK` voice type or for a `FLST` of voice types
/// that holds it, false for another voice or an actor with none. Each index is
/// foreign — and so not run — in the other family, which is what
/// `DialogueGenericHello`'s 124 uncatalogued conditions needed.
#[test]
fn get_is_voice_type_matches_a_voice_or_a_list_of_them() {
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::{EsmIndex, FlstRecord, NpcRecord};
    const NORD_FEMALE: u32 = 0x0001_3AE7;
    const ORC_MALE: u32 = 0x0001_3AE8;
    const SPEAKER_BASE: u32 = 0x0002_0001;
    const BARE_BASE: u32 = 0x0002_0002;
    const NORD_LIST: u32 = 0x0009_0001;
    const ORC_LIST: u32 = 0x0009_0002;

    let mut index = EsmIndex::default();
    index.npcs.insert(
        SPEAKER_BASE,
        NpcRecord { form_id: SPEAKER_BASE, voice_form_id: NORD_FEMALE, ..Default::default() },
    );
    index.npcs.insert(BARE_BASE, NpcRecord { form_id: BARE_BASE, ..Default::default() });
    index.form_lists.insert(
        NORD_LIST,
        FlstRecord { form_id: NORD_LIST, entries: vec![NORD_FEMALE, 0x0001_3AE9], ..Default::default() },
    );
    index.form_lists.insert(
        ORC_LIST,
        FlstRecord { form_id: ORC_LIST, entries: vec![ORC_MALE], ..Default::default() },
    );
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(GameKind::Skyrim));
    crate::actor_base_traits::install_actor_base_traits(&mut world, &index);
    crate::form_lists::install_form_lists(&mut world, &index);
    let actor = |world: &mut World, base_form_id| {
        let actor = world.spawn();
        world.insert(
            actor,
            crate::scene::SceneAliasCandidate { base_form_id, ..Default::default() },
        );
        actor
    };
    let speaker = actor(&mut world, SPEAKER_BASE);
    let bare = actor(&mut world, BARE_BASE);
    let is_voice = |world: &World, who, index_, param| {
        evaluate_condition(&cond_params(index_, param, 0, 1.0), world, &ctx(who))
    };
    // A voice that matches nothing: false when the index is answered as
    // `GetIsVoiceType`, true when it is foreign and passed.
    let answered_as_voice_type = |world: &World, who, index_| !is_voice(world, who, index_, ORC_MALE);

    // Skyrim authors 426.
    assert!(is_modeled(&world, &cond(426, ComparisonOp::Eq, 1.0, false)), "426 is modeled on Skyrim");
    assert!(is_voice(&world, speaker, 426, NORD_FEMALE), "the actor's own voice type");
    assert!(!is_voice(&world, speaker, 426, ORC_MALE), "another voice type");
    assert!(is_voice(&world, speaker, 426, NORD_LIST), "a FLST of voice types holding it");
    assert!(!is_voice(&world, speaker, 426, ORC_LIST), "a FLST that does not");
    assert!(!is_voice(&world, bare, 426, NORD_FEMALE), "an actor with no voice type");
    assert!(!is_voice(&world, bare, 426, NORD_LIST));
    // `== 0` reads the negation, as the Hello topic's gates do.
    assert!(evaluate_condition(
        &cond_params(426, ORC_LIST, 0, 0.0),
        &world,
        &ctx(speaker)
    ));

    // On Skyrim 427 is another function: not modeled, and passed like any
    // uncatalogued one rather than answered as this one. A voice that does NOT
    // match would read false if it were evaluated as `GetIsVoiceType`.
    assert!(!is_modeled(&world, &cond(427, ComparisonOp::Eq, 1.0, false)), "427 is foreign on Skyrim");
    assert!(
        is_voice(&world, speaker, 427, ORC_MALE),
        "a foreign index passes; it must not be answered as GetIsVoiceType"
    );

    // FO3 / FNV author 427 — the mirror image.
    world.insert_resource(crate::ScriptGame(GameKind::Fallout3NV));
    assert!(is_modeled(&world, &cond(427, ComparisonOp::Eq, 1.0, false)), "427 is modeled on FNV");
    assert!(is_voice(&world, speaker, 427, NORD_FEMALE));
    assert!(is_voice(&world, speaker, 427, NORD_LIST));
    assert!(answered_as_voice_type(&world, speaker, 427), "427 is answered as GetIsVoiceType on FNV");
    assert!(!is_modeled(&world, &cond(426, ComparisonOp::Eq, 1.0, false)), "426 is foreign on FNV");
    assert!(is_voice(&world, speaker, 426, ORC_MALE), "a foreign index passes on FNV too");
}

/// #5750 — `IsCommandedActor` (Skyrim / FO4 700) is modeled, and answers 0.0 for
/// every subject because the engine has no summon or reanimate mechanic. That is
/// what lets `DialogueGeneric`'s quest-level gate (`... IsCommandedActor == 0`)
/// be evaluated instead of declining the whole quest.
#[test]
fn is_commanded_actor_is_modeled_and_reads_false_without_a_summon_mechanic() {
    use byroredux_plugin::esm::reader::GameKind;
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(GameKind::Skyrim));
    let actor = world.spawn();
    assert!(is_modeled(&world, &cond(700, ComparisonOp::Eq, 0.0, false)));
    assert!(evaluate_condition(&cond(700, ComparisonOp::Eq, 0.0, false), &world, &ctx(actor)));
    assert!(!evaluate_condition(&cond(700, ComparisonOp::Eq, 1.0, false), &world, &ctx(actor)));
}

#[test]
fn get_is_class_and_race_match_background() {
    use byroredux_core::character::Background;
    const RACE: u32 = 0x0004_4C07;
    const CLASS: u32 = 0x0001_38B9;
    let mut world = World::new();
    let actor = world.spawn();
    world.insert(
        actor,
        Background {
            race_form_id: RACE,
            class_form_id: CLASS,
        },
    );
    // GetIsRace(RACE) == 1, wrong race == 0.
    assert!(evaluate(
        &vec![cond(69, ComparisonOp::Eq, 1.0, false).with_param_1(RACE)],
        &world,
        &ctx(actor)
    ));
    assert!(evaluate(
        &vec![cond(69, ComparisonOp::Eq, 0.0, false).with_param_1(0x9999)],
        &world,
        &ctx(actor)
    ));
    // GetIsClass(CLASS) == 1.
    assert!(evaluate(
        &vec![cond(68, ComparisonOp::Eq, 1.0, false).with_param_1(CLASS)],
        &world,
        &ctx(actor)
    ));
    // No Background → 0.
    assert!(evaluate(
        &vec![cond(68, ComparisonOp::Eq, 0.0, false).with_param_1(CLASS)],
        &world,
        &ctx(123)
    ));
}

#[test]
fn get_xp_for_next_level_uses_leveling_model() {
    use byroredux_core::character::{CharacterLevel, CharacterRuleset, LevelingModel};
    let mut world = World::new();
    // FNV curve: xp_to_next(L) = 150·L + 50 → L10 = 1550.
    world.insert_resource(CharacterRuleset::new(LevelingModel::FNV));
    let actor = world.spawn();
    world.insert(actor, CharacterLevel { level: 10, xp: 0 });
    let list = vec![cond(533, ComparisonOp::Eq, 1550.0, false)];
    assert!(evaluate(&list, &world, &ctx(actor)));
    // No ruleset resource → 0.0.
    let bare = World::new();
    let list = vec![cond(533, ComparisonOp::Eq, 0.0, false)];
    assert!(evaluate(&list, &bare, &ctx(1)));
}

// ── GetReputation / GetReputationThreshold (FNV, indices 573 / 575) ──

#[test]
fn get_reputation_reads_fame_and_infamy_by_axis() {
    use byroredux_core::character::FactionReputation;
    // Boomers — the vanilla FNV REPU FormID (top byte 00 = master slot).
    const BOOMERS: u32 = 0x000F_FAE8;

    let mut world = World::new();
    let actor = world.spawn();
    let mut rep = FactionReputation::default();
    rep.add_fame(BOOMERS, 30);
    rep.add_infamy(BOOMERS, 8);
    world.insert(actor, rep);

    // GetReputation(Boomers, axis=1=Fame) == 30.
    let list = vec![cond(573, ComparisonOp::Eq, 30.0, false)
        .with_param_1(BOOMERS)
        .with_param_2(1)];
    assert!(evaluate(&list, &world, &ctx(actor)));
    // axis=0=Infamy == 8.
    let list = vec![cond(573, ComparisonOp::Eq, 8.0, false)
        .with_param_1(BOOMERS)
        .with_param_2(0)];
    assert!(evaluate(&list, &world, &ctx(actor)));
}

#[test]
fn get_reputation_threshold_classifies_range() {
    use byroredux_core::character::FactionReputation;
    const BOOMERS: u32 = 0x000F_FAE8; // thresholds 0 / 8 / 25 / 50

    let mut world = World::new();
    let actor = world.spawn();
    let mut rep = FactionReputation::default();
    rep.add_fame(BOOMERS, 30); // 30 ≥ 25 → Range 2
    world.insert(actor, rep);

    // GetReputationThreshold(Boomers, Fame) == 2.
    let list = vec![cond(575, ComparisonOp::Eq, 2.0, false)
        .with_param_1(BOOMERS)
        .with_param_2(1)];
    assert!(evaluate(&list, &world, &ctx(actor)));
    // Infamy 0 → Range 0.
    let list = vec![cond(575, ComparisonOp::Eq, 0.0, false)
        .with_param_1(BOOMERS)
        .with_param_2(0)];
    assert!(evaluate(&list, &world, &ctx(actor)));
}

#[test]
fn get_reputation_zero_without_component_or_unknown_faction() {
    use byroredux_core::character::FactionReputation;
    // No FactionReputation component → 0.0.
    let world = World::new();
    let actor: EntityId = 7;
    let list = vec![cond(573, ComparisonOp::Eq, 0.0, false)
        .with_param_1(0x000F_FAE8)
        .with_param_2(1)];
    assert!(evaluate(&list, &world, &ctx(actor)));

    // Component present but faction has no captured thresholds → Range 0.
    let mut world = World::new();
    let actor = world.spawn();
    world.insert(actor, FactionReputation::default());
    let list = vec![cond(575, ComparisonOp::Eq, 0.0, false)
        .with_param_1(0xDEAD_BEEF)
        .with_param_2(1)];
    assert!(evaluate(&list, &world, &ctx(actor)));
}

// ── GetActorValue (#1663) ───────────────────────────────────────────

#[test]
fn get_actor_value_reads_composed_value() {
    use byroredux_core::ecs::components::ActorValues;
    // Stand-in global-space AVIF FormIDs (real ids come from
    // `EsmIndex::actor_values`; the evaluator keys on whatever `param_1`
    // resolves to).
    const AV_HEALTH: u32 = 0x0000_02C9;
    const AV_SNEAK: u32 = 0x0000_02E1;

    let mut world = World::new();
    let actor = world.spawn();
    let mut av = ActorValues::new();
    av.set_base(AV_HEALTH, 100.0);
    av.mod_permanent(AV_HEALTH, 20.0); // +20 perk
    av.apply_damage(AV_HEALTH, 30.0); // −30 damage → current 90
    world.insert(actor, av);

    // GetActorValue(Health) == 90 (the composed value, not the base).
    let list = vec![cond(14, ComparisonOp::Eq, 90.0, false).with_param_1(AV_HEALTH)];
    assert!(evaluate(&list, &world, &ctx(actor)));

    // GetActorValue(Health) > 50 — a typical gate passes on the composite.
    let list = vec![cond(14, ComparisonOp::Gt, 50.0, false).with_param_1(AV_HEALTH)];
    assert!(evaluate(&list, &world, &ctx(actor)));

    // An actor value the actor doesn't carry composes to 0.0.
    let list = vec![cond(14, ComparisonOp::Eq, 0.0, false).with_param_1(AV_SNEAK)];
    assert!(evaluate(&list, &world, &ctx(actor)));
}

#[test]
fn get_actor_value_derives_actor_general_stat_from_ruleset() {
    use byroredux_core::character::fallout4_ruleset;
    use byroredux_core::ecs::components::ActorValues;

    // Stand-in AVIF FormIDs the FO4 ruleset resolves against.
    let resolve = |id: &str| match id {
        "Strength" => Some(0x05u32),
        "Endurance" => Some(0x07),
        "Agility" => Some(0x0A),
        "CarryWeight" => Some(0x2D1),
        "Health" => Some(0x2C9),
        "ActionPoints" => Some(0x2D0),
        "MeleeDamage" => Some(0x2D2),
        _ => None,
    };
    let mut world = World::new();
    world.insert_resource(fallout4_ruleset(resolve));
    let actor = world.spawn();
    // Carries Strength 7, but NOT Carry Weight or Health.
    world.insert(actor, ActorValues::from_pairs([(0x05, 7.0)]));

    // Carry Weight is actor-general → derived on demand: 200 + 10·7 = 270.
    let list = vec![cond(14, ComparisonOp::Eq, 270.0, false).with_param_1(0x2D1)];
    assert!(
        evaluate(&list, &world, &ctx(actor)),
        "Carry Weight derived from SPECIAL"
    );

    // Health is player-only → NOT computed for an NPC; absent default 0.
    let list = vec![cond(14, ComparisonOp::Eq, 0.0, false).with_param_1(0x2C9)];
    assert!(
        evaluate(&list, &world, &ctx(actor)),
        "Health stays 0 (player-only)"
    );
}

/// #5042 — FNV `PerkFinesse` shape: a constant CritChance +5 on an NPC
/// whose derivation carries no CritChance. The modifier-only entry used
/// to take the carried fast path and read the bare `5`; the reading is
/// the formula (`1·Luck`) plus the modifier.
#[test]
fn get_actor_value_composes_a_modifier_only_entry_onto_the_formula() {
    use byroredux_core::character::falloutnv_ruleset;
    use byroredux_core::ecs::components::ActorValues;

    const LUCK: u32 = 0x0B;
    const CRIT: u32 = 0x2D5;
    let resolve = |id: &str| match id {
        "Luck" => Some(LUCK),
        "CritChance" => Some(CRIT),
        _ => None,
    };
    let mut world = World::new();
    world.insert_resource(falloutnv_ruleset(resolve));
    let actor = world.spawn();
    let mut values = ActorValues::from_pairs([(LUCK, 4.0)]);
    values.mod_permanent(CRIT, 5.0); // `stamp_spell_list` / `modav`
    world.insert(actor, values);

    let list = vec![cond(14, ComparisonOp::Eq, 9.0, false).with_param_1(CRIT)];
    assert!(
        evaluate(&list, &world, &ctx(actor)),
        "Luck 4 + Finesse 5, not the bare modifier"
    );

    // An authored CritChance still wins over the formula.
    world
        .get_mut::<ActorValues>(actor)
        .unwrap()
        .set_base(CRIT, 1.0);
    let list = vec![cond(14, ComparisonOp::Eq, 6.0, false).with_param_1(CRIT)];
    assert!(evaluate(&list, &world, &ctx(actor)), "authored 1 + 5");
}

/// `GetBaseValue` / `GetValuePercentage` read the same value with the
/// modifiers (and damage) dropped, for a derived stat as well as an
/// authored one.
#[test]
fn actor_value_views_drop_modifiers_and_damage() {
    use byroredux_core::character::falloutnv_ruleset;
    use byroredux_core::ecs::components::ActorValues;

    const LUCK: u32 = 0x0B;
    const CRIT: u32 = 0x2D5;
    let resolve = |id: &str| match id {
        "Luck" => Some(LUCK),
        "CritChance" => Some(CRIT),
        _ => None,
    };
    let mut world = World::new();
    world.insert_resource(falloutnv_ruleset(resolve));
    let actor = world.spawn();
    let mut values = ActorValues::from_pairs([(LUCK, 4.0)]);
    values.mod_permanent(CRIT, 5.0);
    values.mod_temporary(CRIT, 2.0);
    values.apply_damage(CRIT, 3.0);
    world.insert(actor, values);
    // Derived CritChance: formula 4 (Luck) plus the layers.
    let read = |world: &World, avif, view| actor_value_view(world, actor, avif, view);
    assert_eq!(read(&world, CRIT, ActorValueView::Current), 4.0 + 5.0 + 2.0 - 3.0);
    assert_eq!(read(&world, CRIT, ActorValueView::Undamaged), 4.0 + 5.0 + 2.0);
    assert_eq!(read(&world, CRIT, ActorValueView::Base), 4.0);
    // Authored Luck with a modifier and damage.
    {
        let avs = world.get_mut::<ActorValues>(actor).unwrap();
        avs.mod_permanent(LUCK, 2.0);
        avs.apply_damage(LUCK, 1.0);
    }
    assert_eq!(read(&world, LUCK, ActorValueView::Current), 5.0);
    assert_eq!(read(&world, LUCK, ActorValueView::Undamaged), 6.0);
    assert_eq!(read(&world, LUCK, ActorValueView::Base), 4.0);
}

/// #3441 — `pool_regen_tick_system` and `melee_damage_charal_bonus` both
/// acquire `CharacterRuleset` → `ActorValues` (the former for write), so
/// the `GetActorValue` read must not hold its `ActorValues` guard across
/// the ruleset acquire. `lock_tracker` keys storages and resources into
/// one `TypeId` graph, so the reverse hold is a real 2-cycle.
#[test]
fn get_actor_value_does_not_hold_actor_values_across_ruleset() {
    let src = include_str!("eval.rs");
    let arm_start = src
        .find("pub fn actor_value_view(")
        .expect("the GetActorValue read must exist");
    let arm = &src[arm_start..];
    let arm = &arm[..arm
        .find("/// Dispatch one of the M47.1-catalog functions")
        .expect("evaluate_function must follow the actor value read")];

    let snapshot = arm
        .find("avs.clone()")
        .expect("the arm must snapshot ActorValues instead of holding the guard");
    let level = arm
        .find("world.get::<CharacterLevel>")
        .expect("the arm must read CharacterLevel");
    let ruleset = arm
        .find("try_resource::<CharacterRuleset>()")
        .expect("the arm must consult the ruleset");
    assert!(
        snapshot < ruleset,
        "ActorValues must be cloned and its guard dropped before CharacterRuleset is acquired"
    );
    assert!(
        level < ruleset,
        "CharacterLevel must be read before CharacterRuleset is acquired"
    );

    // Under the opt-in detector, drive the real evaluator after the
    // production hold order has been observed: a surviving reverse hold
    // panics inside `record_and_check`.
    if std::env::var_os("BYRO_LOCK_ORDER_CHECK").as_deref() != Some(std::ffi::OsStr::new("1")) {
        return;
    }
    use byroredux_core::character::fallout4_ruleset;
    use byroredux_core::ecs::components::ActorValues;

    let resolve = |id: &str| match id {
        "Strength" => Some(0x05u32),
        "Endurance" => Some(0x07),
        "Agility" => Some(0x0A),
        "CarryWeight" => Some(0x2D1),
        _ => None,
    };
    let mut world = World::new();
    world.insert_resource(fallout4_ruleset(resolve));
    let actor = world.spawn();
    world.insert(actor, ActorValues::from_pairs([(0x05, 7.0)]));

    // CharacterRuleset → ActorValues (W) — `pool_regen_tick_system`'s order.
    {
        let _rs = world
            .try_resource::<byroredux_core::character::CharacterRuleset>()
            .unwrap();
        let _avs = world.query_mut::<ActorValues>().unwrap();
    }
    // Carry Weight is absent → the arm falls through to the ruleset branch.
    let list = vec![cond(14, ComparisonOp::Eq, 270.0, false).with_param_1(0x2D1)];
    assert!(evaluate(&list, &world, &ctx(actor)));
}

#[test]
fn get_actor_value_zero_without_component() {
    // No `ActorValues` component on the Run-On → GetActorValue returns 0.0
    // (the honest absent-data default — now a real component miss, not a
    // hardcoded stub).
    let world = World::new();
    let actor: EntityId = 7;
    let list = vec![cond(14, ComparisonOp::Eq, 0.0, false).with_param_1(0x0000_02C9)];
    assert!(evaluate(&list, &world, &ctx(actor)));
}

// ── GetDistance (#1664) ─────────────────────────────────────────────

#[test]
fn get_distance_measures_subject_to_target() {
    use byroredux_core::ecs::components::{FormIdComponent, GlobalTransform};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_core::math::{Quat, Vec3};

    let mut world = World::new();
    let mut pool = FormIdPool::new();
    // Target carries global FormID 0x000159E2 (cell loader stores the
    // full global id as the LocalFormId).
    let target_fid = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("FalloutNV.esm"),
        local: LocalFormId(0x0001_59E2),
    });
    world.insert_resource(pool);

    let subject = world.spawn();
    world.insert(
        subject,
        GlobalTransform::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, 1.0),
    );
    let target = world.spawn();
    world.insert(target, FormIdComponent(target_fid));
    world.insert(
        target,
        GlobalTransform::new(Vec3::new(3.0, 4.0, 0.0), Quat::IDENTITY, 1.0),
    );

    // GetDistance(0x000159E2) < 10 → 5.0 < 10 → true.
    let mut lt = cond(1, ComparisonOp::Lt, 10.0, false).with_param_1(0x0001_59E2);
    lt.comparand = ConditionValue::Literal(10.0);
    assert!(evaluate(&vec![lt], &world, &ctx(subject)));

    // GetDistance(0x000159E2) < 4 → 5.0 < 4 → false.
    let lt = cond(1, ComparisonOp::Lt, 4.0, false).with_param_1(0x0001_59E2);
    assert!(!evaluate(&vec![lt], &world, &ctx(subject)));
}

#[test]
fn get_distance_unresolved_target_is_far() {
    use byroredux_core::ecs::components::GlobalTransform;
    use byroredux_core::form_id::FormIdPool;
    use byroredux_core::math::{Quat, Vec3};

    let mut world = World::new();
    world.insert_resource(FormIdPool::new());
    let subject = world.spawn();
    world.insert(
        subject,
        GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
    );

    // No entity carries 0x000159E2 → distance is f32::MAX, so a
    // proximity gate `GetDistance < 100` fails.
    let lt = cond(1, ComparisonOp::Lt, 100.0, false).with_param_1(0x0001_59E2);
    assert!(!evaluate(&vec![lt], &world, &ctx(subject)));
}

// ── RunOn::Reference (#2123) ─────────────────────────────────────────

#[test]
fn run_on_reference_resolves_the_entity_carrying_the_form_id() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    let mut world = World::new();
    let mut pool = FormIdPool::new();
    let referenced_fid = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("FalloutNV.esm"),
        local: LocalFormId(0x0002_ABCD),
    });
    world.insert_resource(pool);

    let subject = world.spawn();
    let referenced = world.spawn();
    world.insert(referenced, FormIdComponent(referenced_fid));

    let mut condition = cond(0, ComparisonOp::Eq, 0.0, false);
    condition.run_on = RunOn::Reference;
    condition.reference_form_id = 0x0002_ABCD;

    let resolved = ctx(subject).resolve(RunOn::Reference, &condition, &world);
    assert_eq!(
        resolved,
        Some(referenced),
        "RunOn::Reference must resolve to the entity whose FormIdComponent \
         maps to reference_form_id via the FormIdPool (#2123)"
    );
}

#[test]
fn run_on_reference_unresolved_form_id_is_none() {
    use byroredux_core::form_id::FormIdPool;

    let mut world = World::new();
    world.insert_resource(FormIdPool::new());
    let subject = world.spawn();

    let mut condition = cond(0, ComparisonOp::Eq, 0.0, false);
    condition.run_on = RunOn::Reference;
    condition.reference_form_id = 0x0002_ABCD;

    // No entity carries this FormID — must decline (None), not panic
    // or fall back to subject.
    assert_eq!(
        ctx(subject).resolve(RunOn::Reference, &condition, &world),
        None
    );
}

// ── GetFactionRank (#1665) ──────────────────────────────────────────

#[test]
fn get_faction_rank_reads_membership() {
    use byroredux_core::ecs::components::FactionRanks;

    let mut world = World::new();
    let actor = world.spawn();
    world.insert(
        actor,
        FactionRanks::from_pairs([(0x0001_38B8, 2), (0x0001_5FFB, 0)]),
    );

    // GetFactionRank(0x000138B8) == 2.
    let eq = cond(73, ComparisonOp::Eq, 2.0, false).with_param_1(0x0001_38B8);
    assert!(evaluate(&vec![eq], &world, &ctx(actor)));

    // GetFactionRank(non-member) == -1.
    let eq = cond(73, ComparisonOp::Eq, -1.0, false).with_param_1(0x0009_9999);
    assert!(evaluate(&vec![eq], &world, &ctx(actor)));
}

#[test]
fn get_faction_rank_minus_one_without_component() {
    // No FactionRanks component → -1.0 (not-in-faction sentinel).
    let world = World::new();
    let actor: EntityId = 4;
    let eq = cond(73, ComparisonOp::Eq, -1.0, false).with_param_1(0x0001_38B8);
    assert!(evaluate(&vec![eq], &world, &ctx(actor)));
}

// ── Global comparand (#1668) ────────────────────────────────────────

#[test]
fn global_comparand_resolves_from_globals_resource() {
    use crate::globals::Globals;
    use crate::quest_stages::{QuestFormId, QuestStageState};

    let mut world = World::new();
    crate::register(&mut world);
    let mut state = QuestStageState::default();
    state.set_stage(QuestFormId(0xAA), 5);
    world.insert_resource(state);

    let mut globals = Globals::new();
    globals.set(0x0100_0042, 5.0); // a GLOB whose value is 5
    world.insert_resource(globals);

    // GetStage(0xAA) == Global(0x01000042) → 5 == 5 → true.
    let mut eq = cond(58, ComparisonOp::Eq, 0.0, false).with_param_1(0xAA);
    eq.comparand = ConditionValue::Global(0x0100_0042);
    assert!(evaluate(&vec![eq], &world, &ctx(0)));

    // GetStage(0xAA) > Global(0x01000042) → 5 > 5 → false.
    let mut gt = cond(58, ComparisonOp::Gt, 0.0, false).with_param_1(0xAA);
    gt.comparand = ConditionValue::Global(0x0100_0042);
    assert!(!evaluate(&vec![gt], &world, &ctx(0)));
}

#[test]
fn global_comparand_defaults_to_zero_when_unresolved() {
    use crate::quest_stages::{QuestFormId, QuestStageState};

    let mut world = World::new();
    crate::register(&mut world);
    let mut state = QuestStageState::default();
    state.set_stage(QuestFormId(0xAA), 0);
    world.insert_resource(state);

    // No Globals resource at all → comparand defaults to 0.0.
    // GetStage(0xAA) == Global(missing) → 0 == 0 → true.
    let mut eq = cond(58, ComparisonOp::Eq, 0.0, false).with_param_1(0xAA);
    eq.comparand = ConditionValue::Global(0x0100_0099);
    assert!(evaluate(&vec![eq], &world, &ctx(0)));
}

// ── CTDA remap table drift guard (#5637) ───────────────────────────

/// The plugin crate's `param1_is_form_id` mirrors this catalog by raw
/// index (it cannot depend on this crate). Walk every index the catalog
/// claims and require the two to agree, so a function catalogued
/// without its remap row — GetQuestRunning / GetInFaction /
/// GetGlobalValue were — fails here instead of silently reading the
/// "absent" default on every plugin whose masters are not a load-order
/// prefix.
#[test]
fn param1_form_id_table_matches_the_condition_catalog() {
    use byroredux_plugin::esm::records::condition::param1_is_form_id;
    let mut covered = 0usize;
    for index in 0..2048u32 {
        let function = ConditionFunction::from_index(index);
        if matches!(function, ConditionFunction::Unknown(_)) {
            continue;
        }
        covered += 1;
        assert_eq!(
            param1_is_form_id(index),
            function.first_param_is_form_id(),
            "fn {index} ({}): plugin remap table and catalog disagree on \
             whether param_1 is a FormID",
            function.name()
        );
    }
    // The Oblivion dialect is the same agreement, by its own indices.
    for &(index, function) in ConditionFunction::oblivion_dialect() {
        assert_eq!(
            param1_is_form_id(index),
            function.first_param_is_form_id(),
            "dialect fn {index} ({}): plugin remap table and catalog disagree on \
             whether param_1 is a FormID",
            function.name()
        );
    }
    // Every catalog entry, plus the two second indices: HasPerk's 448 and
    // GetIsVoiceType's 427 (#5750), less the Oblivion-dialect functions that
    // answer to no index of their own.
    assert_eq!(
        covered,
        ConditionFunction::CATALOG.len() + 2 - ConditionFunction::oblivion_dialect().len()
    );
}

// ── GetQuestRunning (#5636) ────────────────────────────────────────

/// A quest that has been started and then stopped (or completed /
/// failed) is not running: `GetQuestRunning` must read 0.0 for it, not
/// the "has ever entered the lifecycle" membership bit.
#[test]
fn get_quest_running_is_false_for_stopped_completed_and_failed_quests() {
    use crate::quest_stages::{QuestFormId, QuestStageState};
    // Fresh world per case: `setup` drives the quest through its
    // lifecycle, then `GetQuestRunning(0xA1) == 1` is evaluated.
    let running = |setup: &dyn Fn(&mut QuestStageState)| {
        let mut world = World::new();
        crate::register(&mut world);
        let mut state = QuestStageState::default();
        setup(&mut state);
        world.insert_resource(state);
        let eq1 = cond(56, ComparisonOp::Eq, 1.0, false).with_param_1(0xA1);
        evaluate(&vec![eq1], &world, &ctx(0))
    };
    let q = QuestFormId(0xA1);

    assert!(!running(&|_| {}), "never started");
    assert!(
        running(&|s| {
            s.start_quest(q, None);
        }),
        "started → running"
    );
    assert!(
        !running(&|s| {
            s.start_quest(q, None);
            s.stop(q);
            // Map membership stays true after a stop — the bit the
            // old `|| is_started` arm leaked into the answer.
            assert!(s.is_started(q));
        }),
        "stopped → not running"
    );
    assert!(
        !running(&|s| {
            s.start_quest(q, None);
            s.complete(q);
        }),
        "completed → not running"
    );
    assert!(
        !running(&|s| {
            s.start_quest(q, None);
            s.fail(q);
        }),
        "failed → not running"
    );
}

// ── subject_requirement ────────────────────────────────────────────

/// Subjects that differ in everything the shapes below read, and the
/// condition shapes: identity tests that do and don't fail on 0.0, a
/// subject-dependent non-identity test, and unknown functions (a
/// constant 0.0) that are false and true for every subject.
fn requirement_fixture() -> (World, Vec<EntityId>, Vec<Condition>) {
    use byroredux_core::character::Background;
    use byroredux_core::ecs::components::{Dead, FormIdComponent};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    let mut world = World::new();
    let mut pool = FormIdPool::new();
    let mut form_id = |local| {
        FormIdComponent(pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Fallout4.esm"),
            local: LocalFormId(local),
        }))
    };
    let subjects: Vec<EntityId> = (0..4).map(|_| world.spawn()).collect();
    let (a0, a1) = (form_id(0xA0), form_id(0xA1));
    world.insert(subjects[0], a0);
    world.insert(subjects[1], a1);
    world.insert(subjects[2], a0);
    world.insert_resource(pool);
    world.insert(
        subjects[0],
        Background {
            race_form_id: 0x10,
            class_form_id: 0x20,
        },
    );
    world.insert(
        subjects[1],
        Background {
            race_form_id: 0x11,
            class_form_id: 0x20,
        },
    );
    world.insert(
        subjects[3],
        Background {
            race_form_id: 0x10,
            class_form_id: 0x21,
        },
    );
    world.insert(subjects[1], Dead);

    let shapes = vec![
        cond(69, ComparisonOp::Eq, 1.0, false).with_param_1(0x10),
        cond(69, ComparisonOp::Ne, 0.0, false).with_param_1(0x11),
        cond(69, ComparisonOp::Eq, 0.0, false).with_param_1(0x10),
        cond(68, ComparisonOp::Ge, 1.0, false).with_param_1(0x20),
        cond(72, ComparisonOp::Eq, 1.0, false).with_param_1(0xA0),
        cond(46, ComparisonOp::Eq, 1.0, false),
        // An out-of-catalog index (GetInFaction claimed 71 in
        // #5366 Phase 2; pick one no master authors anywhere).
        cond(990, ComparisonOp::Eq, 1.0, false),
        cond(74, ComparisonOp::Eq, 0.0, false),
    ];
    (world, subjects, shapes)
}

/// Every list of one to three shapes, under every `or_next` pattern
/// (trailing flag included): a requirement must never rule out a subject
/// `evaluate` passes, so an alias scan narrowed by it binds exactly what
/// the full scan would.
#[test]
fn subject_requirement_never_excludes_a_passing_subject() {
    let (world, subjects, shapes) = requirement_fixture();
    let mut lists = Vec::new();
    for len in 1..=3u32 {
        for picks in 0..shapes.len().pow(len) {
            for ors in 0..1u32 << len {
                let list: Vec<Condition> = (0..len)
                    .map(|slot| {
                        let pick = picks / shapes.len().pow(slot) % shapes.len();
                        let mut condition = shapes[pick];
                        condition.or_next = ors >> slot & 1 == 1;
                        condition
                    })
                    .collect();
                lists.push(list);
            }
        }
    }

    let (mut unsatisfiable, mut narrowed) = (0, 0);
    for list in &lists {
        let requirement = subject_requirement(list, &world, &ctx(EntityId::MAX));
        for &subject in &subjects {
            if !evaluate(list, &world, &ctx(subject)) {
                continue;
            }
            match &requirement {
                SubjectRequirement::Unsatisfiable => {
                    panic!("{list:?} is unsatisfiable yet passes subject {subject}")
                }
                SubjectRequirement::IdentityBlocks(blocks) => {
                    for block in blocks {
                        assert!(
                            block.iter().any(|test| {
                                test.function.run_on_identity(subject, &world) == Some(test.id)
                            }),
                            "{list:?} passes subject {subject} outside block {block:?}"
                        );
                    }
                }
            }
        }
        match requirement {
            SubjectRequirement::Unsatisfiable => unsatisfiable += 1,
            SubjectRequirement::IdentityBlocks(blocks) if !blocks.is_empty() => narrowed += 1,
            SubjectRequirement::IdentityBlocks(_) => {}
        }
    }
    // Not vacuous: plenty of lists exercise both outcomes.
    assert!(
        unsatisfiable > 100 && narrowed > 100,
        "{unsatisfiable} / {narrowed}"
    );
}

/// #5645 — `GetRandomPercent` is stochastic: it is never folded into a
/// once-for-everyone value, never narrows a block, and the pre-filter
/// leaves the generator untouched. Whatever a subject's own `evaluate`
/// draws, the requirement must not have excluded it.
#[test]
fn a_random_percent_gate_neither_narrows_nor_consumes_the_rng() {
    use crate::story_manager::StoryManagerRng;
    assert!(!ConditionFunction::GetRandomPercent.is_deterministic());
    assert!(ConditionFunction::CATALOG
        .iter()
        .filter(|f| **f != ConditionFunction::GetRandomPercent)
        .all(|f| f.is_deterministic()));

    let (mut world, subjects, shapes) = requirement_fixture();
    world.insert_resource(StoryManagerRng { state: 0x5EED });
    let gate = cond(77, ComparisonOp::Lt, 50.0, false);
    let mut gated_or = gate;
    gated_or.or_next = true;
    let lists = [
        vec![gate],
        vec![gate, shapes[0]],
        vec![gated_or, shapes[0]],
        vec![shapes[0], gate],
    ];
    let race = IdentityTest {
        function: ConditionFunction::GetIsRace,
        id: 0x10,
    };
    let expected = [
        SubjectRequirement::IdentityBlocks(vec![]),
        SubjectRequirement::IdentityBlocks(vec![vec![race]]),
        SubjectRequirement::IdentityBlocks(vec![]),
        SubjectRequirement::IdentityBlocks(vec![vec![race]]),
    ];
    for (list, expected) in lists.iter().zip(expected) {
        for round in 0..64u64 {
            world.resource_mut::<StoryManagerRng>().state = round * 0x9E37 + 1;
            let before = world.resource::<StoryManagerRng>().state;
            let requirement = subject_requirement(list, &world, &ctx(EntityId::MAX));
            assert_eq!(
                world.resource::<StoryManagerRng>().state,
                before,
                "the pre-filter must not consume the RNG"
            );
            assert_eq!(requirement, expected, "{list:?}");
            // Whatever this subject's own draw is, it was not excluded.
            for &subject in &subjects {
                if evaluate(list, &world, &ctx(subject)) {
                    assert_ne!(requirement, SubjectRequirement::Unsatisfiable);
                }
            }
        }
    }
}

#[test]
fn subject_requirement_reports_what_each_block_demands() {
    let (world, _, shapes) = requirement_fixture();
    let race = |id| IdentityTest {
        function: ConditionFunction::GetIsRace,
        id,
    };
    let judge = |list: Vec<Condition>| subject_requirement(&list, &world, &ctx(EntityId::MAX));
    let or = |mut condition: Condition| {
        condition.or_next = true;
        condition
    };

    // An unknown function is 0.0 for everyone: `== 1` fails them all.
    assert_eq!(
        judge(vec![shapes[6]]),
        SubjectRequirement::Unsatisfiable
    );
    // `== 0` passes them all, leaving the identity block to narrow by.
    assert_eq!(
        judge(vec![shapes[7], shapes[0]]),
        SubjectRequirement::IdentityBlocks(vec![vec![race(0x10)]])
    );
    // An OR block of identity tests admits either identity.
    assert_eq!(
        judge(vec![or(shapes[0]), shapes[1]]),
        SubjectRequirement::IdentityBlocks(vec![vec![race(0x10), race(0x11)]])
    );
    // A test that passes on 0.0, or a non-identity member, leaves its
    // block unconstrained.
    assert_eq!(
        judge(vec![shapes[2]]),
        SubjectRequirement::IdentityBlocks(vec![])
    );
    assert_eq!(
        judge(vec![or(shapes[0]), shapes[5]]),
        SubjectRequirement::IdentityBlocks(vec![])
    );
}

// ── Helper: chainable param_1 setter for compact test construction ──

trait CondBuilder {
    fn with_param_1(self, p: u32) -> Self;
    fn with_param_2(self, p: u32) -> Self;
}
impl CondBuilder for Condition {
    fn with_param_1(mut self, p: u32) -> Self {
        self.param_1 = p;
        self
    }
    fn with_param_2(mut self, p: u32) -> Self {
        self.param_2 = p;
        self
    }
}

/// Regression for #2933 — `GetActorValue` must honour BOTH contract fields
/// on `DerivedStatFormula`, not just `scope`.
///
/// A `DerivedOutput::Multiplier` row's `eval` returns a *ratio* meant for a
/// combat/XP consumer to multiply by. FO4's Melee Damage is
/// `x(1 + 0.1*STR)` and actor-general, so it passed the scope check and
/// leaked a bare 1.0..=2.0 where Bethesda's own `GetActorValue` yields an
/// actor-value reading — small enough to satisfy `> 0` gates and fail
/// realistic thresholds, with nothing crashing and no test failing.
#[test]
fn get_actor_value_skips_multiplier_kind_rows() {
    use byroredux_core::character::derived::{DerivedInput, DerivedStatFormula};
    use byroredux_core::character::leveling::LevelingModel;
    use byroredux_core::character::CharacterRuleset;
    use byroredux_core::ecs::components::ActorValues;

    const STRENGTH: u32 = 0x20;
    const MELEE_DAMAGE: u32 = 0x21;
    const CARRY_WEIGHT: u32 = 0x22;

    let mut rs = CharacterRuleset::new(LevelingModel::FO4);
    // FO4 Melee Damage: x(1 + 0.1*STR), actor-general + MULTIPLIER.
    rs.push_derived(
        MELEE_DAMAGE,
        DerivedStatFormula::affine(DerivedInput::actor_value(STRENGTH), 0.1, 1.0)
            .as_multiplier(),
    );
    // A control row of the same scope but ABSOLUTE kind, so the test
    // proves the gate keys on `kind` and not on "derived rows are skipped".
    rs.push_derived(
        CARRY_WEIGHT,
        DerivedStatFormula::affine(DerivedInput::actor_value(STRENGTH), 10.0, 25.0),
    );

    let mut world = World::new();
    world.insert_resource(rs);
    let e = world.spawn();
    world.insert(e, ActorValues::from_pairs([(STRENGTH, 5.0)]));

    let multiplier = evaluate_function(
        ConditionFunction::GetActorValue,
        &Condition {
            param_1: MELEE_DAMAGE,
            ..cond(14, ComparisonOp::Eq, 0.0, false)
        },
        e,
        &world,
    );
    assert_eq!(
        multiplier, 0.0,
        "a Multiplier-kind row must fall through to the absent-AV default, \
         exactly as a player-only row already does — returning 1.5 here is \
         the #2933 leak"
    );

    let absolute = evaluate_function(
        ConditionFunction::GetActorValue,
        &Condition {
            param_1: CARRY_WEIGHT,
            ..cond(14, ComparisonOp::Eq, 0.0, false)
        },
        e,
        &world,
    );
    assert_eq!(
        absolute, 75.0,
        "absolute actor-general rows must still resolve (10*5 + 25) — the \
         gate keys on `kind`, not on derived-ness"
    );
}

/// #5366 Phase 2 — `RunOn::EventData` resolves the R1/R2 tag in the
/// CTDA tail against the context's event slots; L-tags and unknown
/// tags fail the condition; a context without event data fails it
/// exactly as before Phase 2.
#[test]
fn event_data_run_on_resolves_reference_tags_only() {
    use byroredux_core::ecs::components::FactionRanks;
    use byroredux_plugin::esm::records::condition::{event_data_slot, EventDataSlot};

    let tag = |bytes: &[u8; 2]| u32::from(bytes[0]) | (u32::from(bytes[1]) << 8);
    assert_eq!(event_data_slot(tag(b"R1")), Some(EventDataSlot::Reference1));
    assert_eq!(event_data_slot(tag(b"R2")), Some(EventDataSlot::Reference2));
    assert_eq!(event_data_slot(tag(b"L1")), Some(EventDataSlot::Location1));
    assert_eq!(event_data_slot(tag(b"L2")), Some(EventDataSlot::Location2));
    assert_eq!(event_data_slot(0x00FF_00FF), None);

    let mut world = World::new();
    let subject = world.spawn();
    let r1 = world.spawn();
    let r2 = world.spawn();
    // GetInFaction(0xF00D) == 1 on the tagged slot — true for r2 only.
    world.insert(r2, FactionRanks::from_pairs([(0xF00D, 0)]));
    let condition = |extra: u32| Condition {
        function_index: 71,
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(1.0),
        param_1: 0xF00D,
        run_on: RunOn::EventData,
        extra_data_id: extra,
        ..Default::default()
    };
    let slots = EventDataSlots {
        reference_1: Some(r1),
        reference_2: Some(r2),
        location_1: Some(0x18A49),
        location_2: Some(0x18A56),
        ..Default::default()
    };
    let ctx = ConditionContext::for_subject(subject).with_event_data(&slots);
    assert!(
        evaluate_condition(&condition(tag(b"R2")), &world, &ctx),
        "R2 is in the faction"
    );
    assert!(
        !evaluate_condition(&condition(tag(b"R1")), &world, &ctx),
        "R1 is not"
    );
    assert!(
        !evaluate_condition(&condition(tag(b"L2")), &world, &ctx),
        "location tags have no entity to run on"
    );
    assert!(
        !evaluate_condition(&condition(0x00FF_00FF), &world, &ctx),
        "unknown tags fail"
    );
    assert!(
        !evaluate_condition(
            &condition(tag(b"R2")),
            &world,
            &ConditionContext::for_subject(subject)
        ),
        "no event data in the context — fails as in Phase 1"
    );
}

/// GetInFaction (Skyrim 71) — membership collapses the same
/// `FactionRanks` read GetFactionRank does.
#[test]
fn get_in_faction_reads_membership() {
    use byroredux_core::ecs::components::FactionRanks;
    let mut world = World::new();
    let member = world.spawn();
    world.insert(member, FactionRanks::from_pairs([(0xF00D, 2), (0xBAAD, 0)]));
    let outsider = world.spawn();
    world.insert(outsider, FactionRanks::from_pairs([(0xBAAD, 3)]));
    let condition = Condition {
        function_index: 71,
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(1.0),
        param_1: 0xF00D,
        ..Default::default()
    };
    assert!(evaluate_condition(&condition, &world, &ctx(member)));
    assert!(!evaluate_condition(&condition, &world, &ctx(outsider)));
    let bare = world.spawn();
    assert!(
        !evaluate_condition(&condition, &world, &ctx(bare)),
        "no FactionRanks — not in the faction"
    );
}

/// #5989 — real-data census: every authored `GetActorValue` index the vanilla
/// INFO conditions carry translates to a loaded AVIF (the masters' own
/// `AVIF` set), except the indices with no record in the master.
///
/// Run with:
///   cargo test -p byroredux-scripting --lib authored_actor_value_indices_resolve -- --ignored --nocapture
#[test]
#[ignore = "needs Skyrim.esm and FalloutNV.esm on disk"]
fn authored_actor_value_indices_resolve_in_the_vanilla_masters() {
    use byroredux_plugin::esm::test_paths::{fnv_esm, skyrim_se_esm};
    use std::collections::BTreeMap;

    for (label, path) in [("Skyrim", skyrim_se_esm()), ("FalloutNV", fnv_esm())] {
        if !path.is_file() {
            if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
                panic!("BYROREDUX_REQUIRE_GAME_DATA is set but {path:?} is missing");
            }
            continue;
        }
        let index = byroredux_plugin::esm::parse_esm(&std::fs::read(&path).unwrap()).unwrap();
        let mut world = World::new();
        crate::register(&mut world);
        crate::install_actor_value_names(&mut world, &index);
        let mut seen: BTreeMap<u32, (usize, bool)> = BTreeMap::new();
        let conditions = index
            .dialogues
            .values()
            .flat_map(|dial| dial.infos.iter())
            .flat_map(|info| info.conditions.iter());
        for condition in conditions.filter(|c| c.function_index == 14) {
            let entry = seen.entry(condition.param_1).or_insert((0, false));
            entry.0 += 1;
            entry.1 = crate::actor_value_names::condition_actor_value_form(&world, condition.param_1).is_some();
        }
        let total: usize = seen.values().map(|(n, _)| n).sum();
        let unresolved: Vec<_> = seen.iter().filter(|(_, (_, ok))| !ok).collect();
        let lost: usize = unresolved.iter().map(|(_, (n, _))| n).sum();
        println!("{label}: {total} INFO GetActorValue conditions; unresolved indices (code, uses): {:?}",
            unresolved.iter().map(|(code, (n, _))| (**code, *n)).collect::<Vec<_>>());
        assert!(lost * 20 < total, "{label}: more than 5% of authored GetActorValue conditions stay unreadable ({lost}/{total})");
    }
}

/// #5778 — the census behind the issue, re-run against vanilla
/// `Oblivion.esm`: quest-level dialogue lists (`dialogue_conditions`) that
/// still hold a function the evaluator cannot read on Oblivion, and the
/// INFOs (and GREETING INFOs) owned by those quests, which the fail-closed
/// quest gate declines. Before 254/365 (and 70/310) were catalogued: 339
/// of 390 quests, 16,799 of 19,278 INFOs, 3,247 of 3,743 GREETING; after,
/// 1 quest, 9 INFOs, 9 GREETING.
#[test]
#[ignore = "needs Oblivion.esm on disk (~160 MB resident)"]
fn oblivion_quest_level_dialogue_gates_census() {
    use byroredux_plugin::esm::reader::GameKind;
    let path = byroredux_plugin::esm::test_paths::oblivion_esm();
    if !path.is_file() {
        if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
            panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but Oblivion.esm was not found at {path:?}");
        }
        return;
    }
    let esm = std::fs::read(&path).unwrap();
    let index = byroredux_plugin::esm::parse_esm(&esm).unwrap();
    let mut world = World::new();
    crate::register(&mut world);
    crate::actor_value_names::install_actor_value_names(&mut world, &index);
    assert!(matches!(world.resource::<crate::ScriptGame>().0, GameKind::Oblivion));

    let gated: Vec<u32> = index
        .quests
        .values()
        .filter(|q| !q.dialogue_conditions.is_empty())
        .map(|q| q.form_id)
        .collect();
    let mut unread_functions = std::collections::BTreeMap::<u32, usize>::new();
    let declined: std::collections::HashSet<u32> = index
        .quests
        .values()
        .filter(|q| {
            let mut unread = false;
            for c in &q.dialogue_conditions {
                if !is_modeled(&world, c) {
                    *unread_functions.entry(c.function_index).or_default() += 1;
                    unread = true;
                }
            }
            unread
        })
        .map(|q| q.form_id)
        .collect();
    let (mut infos, mut declined_infos, mut greetings, mut declined_greetings) = (0, 0, 0, 0);
    for dial in index.dialogues.values() {
        let greeting = dial.editor_id.eq_ignore_ascii_case("GREETING");
        for info in &dial.infos {
            let out = declined.contains(&info.quest);
            infos += 1;
            declined_infos += out as usize;
            if greeting {
                greetings += 1;
                declined_greetings += out as usize;
            }
        }
    }
    eprintln!(
        "[#5778] quests={} gated={} declined={} unread_functions={unread_functions:?} \
         infos={infos} declined_infos={declined_infos} greetings={greetings} \
         declined_greetings={declined_greetings}",
        index.quests.len(),
        gated.len(),
        declined.len(),
    );
    assert_eq!(gated.len(), 339, "the issue's quest-level list count");
    // Measured 2026-10-21: only 313 (IsActorEvil, one quest) stays
    // uncatalogued.
    assert_eq!(unread_functions.into_iter().collect::<Vec<_>>(), vec![(313, 1)]);
    assert_eq!(
        (declined.len(), declined_infos, declined_greetings),
        (1, 9, 9),
        "quests / INFOs / GREETING INFOs the quest gate still declines"
    );
}

/// #6251 — a world with the ObScript state the legacy getters read, and an
/// actor with a placed-reference FormID.
fn getter_world() -> (World, EntityId) {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Fallout3NV));
    world.register::<FormIdComponent>();
    let mut pool = FormIdPool::default();
    let id = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("FalloutNV.esm"),
        local: LocalFormId(0x0001_6251),
    });
    world.insert_resource(pool);
    let actor = world.spawn();
    world.insert(actor, FormIdComponent(id));
    (world, actor)
}

fn cond_params(function_index: u32, param_1: u32, param_2: u32, comparand: f32) -> Condition {
    Condition { param_1, param_2, ..cond(function_index, ComparisonOp::Eq, comparand, false) }
}

/// #6251 — the getters the ObScript VM answered are catalogued, so the
/// fail-closed dialogue / objective gates read them rather than decline.
#[test]
fn the_obscript_getters_are_catalogued_and_modeled_on_fnv() {
    let (world, _) = getter_world();
    for index in [35, 47, 64, 84, 131, 289, 420, 421, 431, 546] {
        assert!(
            !matches!(ConditionFunction::from_index(index), ConditionFunction::Unknown(_)),
            "CTDA {index} is catalogued"
        );
        assert!(is_modeled(&world, &cond(index, ComparisonOp::Eq, 1.0, false)), "CTDA {index} is modeled on FNV");
    }
}

/// #6251 — xEdit: 131 is `GetSceneActionPercent` from FO4 on, 431
/// `GetActiveBoostDuration` on Starfield.
#[test]
fn get_pc_is_sex_and_get_health_percentage_are_foreign_where_xedit_renames_them() {
    use super::catalog::index_is_foreign_in;
    use byroredux_plugin::esm::reader::GameKind;
    for game in [GameKind::Fallout4, GameKind::Fallout76, GameKind::Starfield] {
        assert!(index_is_foreign_in(131, game), "131 is GetSceneActionPercent on {game:?}");
    }
    for game in [GameKind::Oblivion, GameKind::Fallout3NV, GameKind::Skyrim] {
        assert!(!index_is_foreign_in(131, game), "131 is GetPCIsSex on {game:?}");
    }
    assert!(index_is_foreign_in(431, GameKind::Starfield));
    assert!(!index_is_foreign_in(431, GameKind::Fallout3NV));
}

#[test]
fn get_item_count_sums_the_inventory_and_a_form_lists_members() {
    use byroredux_core::ecs::components::{Inventory, ItemStack};
    let (mut world, actor) = getter_world();
    let mut inventory = Inventory::new();
    inventory.push(ItemStack::new(0x0F, 120)); // caps
    inventory.push(ItemStack::new(0x15000, 2));
    inventory.push(ItemStack::new(0x15001, 3));
    world.insert(actor, inventory);
    let mut index = byroredux_plugin::esm::records::EsmIndex::default();
    index.form_lists.insert(
        0x9000,
        byroredux_plugin::esm::records::FlstRecord {
            form_id: 0x9000,
            entries: vec![0x15000, 0x15001, 0x15002],
            ..Default::default()
        },
    );
    crate::form_lists::install_form_lists(&mut world, &index);
    assert!(evaluate_condition(&cond_params(47, 0x0F, 0, 120.0), &world, &ctx(actor)));
    assert!(evaluate_condition(&cond_params(47, 0x9000, 0, 5.0), &world, &ctx(actor)), "a form list sums its members");
    assert!(evaluate_condition(&cond_params(47, 0x15002, 0, 0.0), &world, &ctx(actor)));
}

#[test]
fn get_disabled_get_is_creature_and_is_in_combat_read_the_run_on() {
    let (mut world, actor) = getter_world();
    assert!(evaluate_condition(&cond_params(35, 0, 0, 0.0), &world, &ctx(actor)));
    world.resource_mut::<crate::fragment::ReferenceEnableState>().set_enabled(0x0001_6251, false);
    assert!(evaluate_condition(&cond_params(35, 0, 0, 1.0), &world, &ctx(actor)), "a scripted Disable");

    assert!(evaluate_condition(&cond_params(64, 0, 0, 0.0), &world, &ctx(actor)));
    world.insert(actor, byroredux_core::ecs::components::CreatureActor);
    assert!(evaluate_condition(&cond_params(64, 0, 0, 1.0), &world, &ctx(actor)));

    assert!(evaluate_condition(&cond_params(289, 0, 0, 0.0), &world, &ctx(actor)));
    let foe = world.spawn();
    world.insert(actor, crate::AiCombatState { target: foe, attack_cooldown_remaining: 0.0 });
    assert!(evaluate_condition(&cond_params(289, 0, 0, 1.0), &world, &ctx(actor)));
}

#[test]
fn get_health_percentage_is_current_over_the_undamaged_ceiling() {
    use byroredux_core::ecs::components::{ActorValues, ActorVitals};
    const HEALTH: u32 = 0x2D4;
    let (mut world, actor) = getter_world();
    world.insert(actor, ActorVitals { health: HEALTH });
    let mut values = ActorValues::from_pairs([(HEALTH, 200.0)]);
    values.apply_damage(HEALTH, 50.0);
    world.insert(actor, values);
    assert_eq!(
        evaluate_function(ConditionFunction::GetHealthPercentage, &cond_params(431, 0, 0, 0.0), actor, &world),
        0.75
    );
}

#[test]
fn quest_objective_dead_count_and_player_sex_getters_read_game_state() {
    use crate::quest_stages::{QuestFormId, QuestObjectiveState, QuestStageState};
    let (mut world, actor) = getter_world();
    const QUEST: u32 = 0x0001_0A00;
    world.insert_resource(QuestObjectiveState::default());
    world.insert_resource(QuestStageState::default());
    assert!(evaluate_condition(&cond_params(421, QUEST, 10, 0.0), &world, &ctx(actor)));
    world.resource_mut::<QuestObjectiveState>().set_displayed(QuestFormId(QUEST), 10, true);
    world.resource_mut::<QuestObjectiveState>().set_completed(QuestFormId(QUEST), 20, true);
    assert!(evaluate_condition(&cond_params(421, QUEST, 10, 1.0), &world, &ctx(actor)));
    assert!(evaluate_condition(&cond_params(420, QUEST, 10, 0.0), &world, &ctx(actor)));
    assert!(evaluate_condition(&cond_params(420, QUEST, 20, 1.0), &world, &ctx(actor)));

    assert!(evaluate_condition(&cond_params(546, QUEST, 0, 0.0), &world, &ctx(actor)));
    world.resource_mut::<QuestStageState>().start_quest(QuestFormId(QUEST), None);
    world.resource_mut::<QuestStageState>().complete(QuestFormId(QUEST));
    assert!(evaluate_condition(&cond_params(546, QUEST, 0, 1.0), &world, &ctx(actor)));

    const GECKO: u32 = 0x0002_0B00;
    world.resource_mut::<crate::obscript_state::KillRecords>().record(0x0003_0001, GECKO, None);
    world.resource_mut::<crate::obscript_state::KillRecords>().record(0x0003_0002, GECKO, None);
    assert!(evaluate_condition(&cond_params(84, GECKO, 0, 2.0), &world, &ctx(actor)));

    // GetPCIsSex asks about the player whatever the Run-On; no player, no match.
    assert!(evaluate_condition(&cond_params(131, 1, 0, 0.0), &world, &ctx(actor)));
}

// ── Oblivion's crime vocabulary (guard greetings) ──────────────────

/// The conditions of a vanilla Oblivion guard `GREETING` for a player who owes
/// a bounty they can pay (DIAL `GREETING` in Oblivion.esm): the player-side
/// ones (`GetCrimeGold > 0`, `CanPayCrimeGold == 1`) run on the listener, the
/// guard-side one (`IsGuard == 1`) on the speaker.
fn guard_greeting() -> Vec<Condition> {
    let on_target = |function_index, comparator, comparand| Condition {
        run_on: RunOn::Target,
        ..cond(function_index, comparator, comparand, false)
    };
    vec![
        on_target(116, ComparisonOp::Gt, 0.0),
        on_target(127, ComparisonOp::Eq, 1.0),
        cond(125, ComparisonOp::Eq, 1.0, false),
    ]
}

/// A world with a guard speaker and the player listener.
fn guard_and_player(game: byroredux_plugin::esm::reader::GameKind) -> (World, EntityId, EntityId) {
    use byroredux_core::character::Background;
    use byroredux_core::ecs::components::{Inventory, ItemStack};
    use byroredux_plugin::esm::records::{ClassRecord, EsmIndex};
    let mut index = EsmIndex::default();
    index.classes.insert(
        0x77,
        ClassRecord {
            form_id: 0x77,
            editor_id: String::new(),
            full_name: String::new(),
            description: String::new(),
            base_attributes: [0; 7],
            skyrim_skill_weights: None,
            skyrim_pool_weights: None,
            tag_skills: Vec::new(),
            primary_attributes: None,
            specialization: None,
            major_skills: Vec::new(),
            flags_oblivion: Some(0b11),
            class_flags: Some(0b11),
        },
    );
    let mut world = World::new();
    crate::register(&mut world);
    world.register::<Background>();
    world.register::<Inventory>();
    world.insert_resource(crate::ScriptGame(game));
    crate::guards::install_guard_classes(&mut world, &index);
    let guard = world.spawn();
    world.insert(guard, Background { race_form_id: 1, class_form_id: 0x77 });
    let player = world.spawn();
    world.insert(player, Inventory { items: vec![ItemStack::new(crate::guards::CURRENCY_FORM, 100)] });
    world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
    (world, guard, player)
}

#[test]
fn an_oblivion_guard_greeting_follows_the_players_bounty() {
    use byroredux_plugin::esm::reader::GameKind::Oblivion;
    let (world, guard, player) = guard_and_player(Oblivion);
    let mut context = ctx(guard);
    context.target = Some(player);
    let greeting = guard_greeting();
    assert!(!evaluate(&greeting, &world, &context), "no bounty, no arrest greeting");

    world.resource_mut::<crate::obscript_ledger::ObScriptLedger>().crime_gold = 40;
    assert!(evaluate(&greeting, &world, &context), "40 owed, 100 in the purse");

    world.resource_mut::<crate::obscript_ledger::ObScriptLedger>().crime_gold = 400;
    assert!(!evaluate(&greeting, &world, &context), "400 owed is more than the player can pay");
    let can_pay = Condition { run_on: RunOn::Target, ..cond(127, ComparisonOp::Eq, 0.0, false) };
    assert!(evaluate(&vec![can_pay], &world, &context), "the 'cannot pay' branch is the one that opens");
}

#[test]
fn a_citizen_is_not_a_guard_and_is_turn_arrest_needs_a_guard_and_a_bounty() {
    use byroredux_plugin::esm::reader::GameKind::Oblivion;
    let (mut world, guard, player) = guard_and_player(Oblivion);
    world.resource_mut::<crate::obscript_ledger::ObScriptLedger>().crime_gold = 5;
    let citizen = world.spawn();
    let mut context = ctx(guard);
    context.target = Some(player);
    let is_guard = cond(125, ComparisonOp::Eq, 1.0, false);
    let turn_arrest = cond(329, ComparisonOp::Eq, 1.0, false);
    assert!(evaluate_condition(&is_guard, &world, &context));
    assert!(evaluate_condition(&turn_arrest, &world, &context));
    let mut citizen_context = ctx(citizen);
    citizen_context.target = Some(player);
    assert!(!evaluate_condition(&is_guard, &world, &citizen_context));
    assert!(!evaluate_condition(&turn_arrest, &world, &citizen_context), "a citizen does not arrest");
    world.resource_mut::<crate::obscript_ledger::ObScriptLedger>().crime_gold = 0;
    assert!(!evaluate_condition(&turn_arrest, &world, &context), "a guard with nothing to arrest for");
}

/// The same numbers are other functions in the later games: they must stay
/// unmodeled there (the fail-closed gates decline them), not read the
/// Oblivion bounty.
#[test]
fn the_crime_dialect_indices_are_oblivions_alone() {
    use byroredux_plugin::esm::reader::GameKind::{Fallout3NV, Fallout4, Oblivion, Skyrim};
    for &(index, _) in ConditionFunction::oblivion_dialect() {
        let condition = cond(index, ComparisonOp::Eq, 1.0, false);
        let (oblivion, ..) = guard_and_player(Oblivion);
        assert!(is_modeled(&oblivion, &condition), "{index} on Oblivion");
        for game in [Fallout3NV, Skyrim, Fallout4] {
            let (world, ..) = guard_and_player(game);
            assert!(!is_modeled(&world, &condition), "{index} on {game:?}");
        }
    }
}

/// `GetCrime` (122) and `IsGuard` (125) are the same function in every game. `GetCrime` reads the
/// crime record, which any game fills; `IsGuard` reads the `CLAS` Guard flag, which Oblivion, FO3
/// and FNV author and the later games do not.
#[test]
fn get_crime_is_everywhere_and_is_guard_where_classes_flag_guards() {
    use byroredux_plugin::esm::reader::GameKind::{Fallout3NV, Fallout4, Oblivion, Skyrim};
    let get_crime = cond(122, ComparisonOp::Eq, 1.0, false);
    let is_guard = cond(125, ComparisonOp::Eq, 1.0, false);
    for game in [Oblivion, Fallout3NV, Skyrim, Fallout4] {
        let (world, ..) = guard_and_player(game);
        assert!(is_modeled(&world, &get_crime), "GetCrime on {game:?}");
        assert_eq!(is_modeled(&world, &is_guard), matches!(game, Oblivion | Fallout3NV), "IsGuard on {game:?}");
    }
}

/// `GetCrime criminal, type`: true for the actor a crime was committed against, once that actor
/// knows of it; the criminal is named by reference (the player is `0x14`).
#[test]
fn get_crime_reads_what_the_caller_knows_of_a_crime_against_them() {
    use crate::crime_knowledge::{CrimeEntry, CrimeKnowledge};
    use byroredux_core::character::CrimeKind;
    use byroredux_plugin::esm::reader::GameKind::Fallout3NV;
    let (mut world, guard, player) = guard_and_player(Fallout3NV);
    world.register::<byroredux_core::ecs::components::FormIdComponent>();
    let mut pool = byroredux_core::form_id::FormIdPool::default();
    let id = pool.intern(byroredux_core::form_id::FormIdPair {
        plugin: byroredux_core::form_id::PluginId::from_filename("FalloutNV.esm"),
        local: byroredux_core::form_id::LocalFormId(0xB0B),
    });
    world.insert_resource(pool);
    world.insert(guard, byroredux_core::ecs::components::FormIdComponent(id));
    world.insert_resource(CrimeKnowledge::default());
    let attack = Condition { param_1: crate::PLAYER_REFERENCE, param_2: 3, ..cond(122, ComparisonOp::Eq, 1.0, false) };
    let mut context = ctx(guard);
    context.target = Some(player);
    assert!(!evaluate_condition(&attack, &world, &context));
    world.resource_mut::<CrimeKnowledge>().record(CrimeEntry {
        knower: 0xB0B,
        criminal: crate::PLAYER_REFERENCE,
        kind: CrimeKind::Attack,
        victim: 0xB0B,
    });
    assert!(evaluate_condition(&attack, &world, &context), "the victim saw the assault");
    let murder = Condition { param_2: 4, ..attack.clone() };
    assert!(!evaluate_condition(&murder, &world, &context), "another crime type");
}
