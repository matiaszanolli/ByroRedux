use super::*;
use byroredux_papyrus::ast::{CallArg, Identifier, ScriptItem, StateItem};
use byroredux_papyrus::parse_script;

/// Wrap a node with a dummy span, for ASTs the `.psc` parser cannot
/// express (e.g. a `::X_var` backing-variable identifier).
fn sp<T>(node: T) -> Spanned<T> {
    Spanned::new(node, byroredux_papyrus::span::Span::new(0, 0))
}

/// A bare `Self.GetStageDone(stage)` condition atom — the minimal
/// shape `classify_guard_atom`'s `prim_stage_done` accepts (a bare
/// call used as a boolean, `== 1` implied), so every level of a
/// synthetic nested-`If` chain classifies as a real `StageDoneGuard`
/// rather than declining before `lower_statements` ever recurses.
fn get_stage_done_condition(stage: i64) -> Spanned<Expr> {
    sp(Expr::Call {
        callee: Box::new(sp(Expr::MemberAccess {
            object: Box::new(sp(Expr::Ident(Identifier::new("Self")))),
            member: sp(Identifier::new("GetStageDone")),
        })),
        args: vec![CallArg {
            name: None,
            value: sp(Expr::IntLit(stage)),
        }],
    })
}

/// Build `depth` levels of nested `If Self.GetStageDone(0) ... EndIf`,
/// each level guarded by a real `StageDoneGuard`-classifiable
/// condition, with an empty (no-op) innermost body.
///
/// Built directly from AST nodes rather than `.psc` source text on
/// purpose (#3279): the papyrus parser's own `MAX_STMT_DEPTH` would
/// reject a `depth` this deep before `lower_statements` ever saw it,
/// which would prove nothing about `lower_statements`'s *own* cap —
/// the whole point of a defense-in-depth guard is that it holds even
/// if the upstream bound it currently rides on is ever loosened or
/// bypassed.
fn nested_if_chain(depth: usize) -> Vec<Spanned<Stmt>> {
    let mut body: Vec<Spanned<Stmt>> = Vec::new();
    for _ in 0..depth {
        body = vec![sp(Stmt::If {
            condition: get_stage_done_condition(0),
            body,
            elseif_clauses: vec![],
            else_body: None,
        })];
    }
    body
}

/// #3279 (SCR-D5-2026-08-24-02) — analogous to
/// `stmt_depth_cap_rejects_pathological_nested_if`
/// (`crates/papyrus/src/parser/stmt.rs`). `MAX_CONDITIONAL_DEPTH` is
/// this recursion's own cap, independent of the two upstream ones
/// (`MAX_STMT_DEPTH` on the `.psc` parser, `MAX_REBUILD_DEPTH` on the
/// `.pex` decompiler's control-flow rebuild) that transitively bound
/// it today.
#[test]
fn conditional_depth_cap_declines_pathological_nested_if() {
    let body = nested_if_chain((MAX_CONDITIONAL_DEPTH as usize) * 2);
    assert_eq!(
        lower_fragment(&body),
        None,
        "a pathologically nested If chain must decline once \
         MAX_CONDITIONAL_DEPTH is exceeded, not recurse unboundedly"
    );
}

/// A legitimate (if unusual) fragment comfortably under the cap must
/// still lower successfully — the cap must not be so tight it
/// rejects real content.
#[test]
fn conditional_depth_cap_accepts_legitimate_nesting() {
    let body = nested_if_chain(50);
    assert!(
        lower_fragment(&body).is_some(),
        "legitimate nesting well under MAX_CONDITIONAL_DEPTH must still lower"
    );
}

/// Parse a script and return the body of its first function/event
/// named like a fragment (or just the first function), to drive
/// lowering on realistic shapes.
fn first_fn_body(src: &str) -> Vec<Spanned<Stmt>> {
    let (script, errs) = parse_script(src).expect("parses");
    assert!(errs.is_empty(), "{errs:?}");
    for item in &script.body {
        match &item.node {
            ScriptItem::Function(f) => return f.body.clone(),
            ScriptItem::Event(e) => return e.body.clone(),
            ScriptItem::State(st) => {
                if let Some(si) = st.body.first() {
                    match &si.node {
                        StateItem::Function(f) => return f.body.clone(),
                        StateItem::Event(e) => return e.body.clone(),
                    }
                }
            }
            _ => {}
        }
    }
    panic!("no function/event body found");
}

#[test]
fn lowers_self_set_stage() {
    // The `{self.setstage(#)}` family — Self is the quest.
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n Self.SetStage(20)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 20
        }])
    );
}

#[test]
fn lowers_global_set_value_before_stage_handoff() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         GlobalVariable Property GameHour Auto\n\
         Function Fragment_0()\n\
         GameHour.SetValue(7.0)\n\
         Self.SetStage(10)\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::SetGlobalValue {
                global: ObjectRef::Property("GameHour".into()),
                value: crate::translate::effects::ValueExpr::Literal(7.0),
            },
            Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 10,
            },
        ])
    );
}

/// #6022 — a fragment whose longest path needs as many provider barriers as the
/// runtime will cross is declined whole at lowering; one barrier fewer lowers.
/// A conditional counts its deeper branch only (the branches never both run).
#[test]
fn a_fragment_past_the_provider_barrier_cap_declines_whole() {
    let providers = PapyrusProviderCatalog::engine_compatibility();
    let lower = |body: String| {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_0()\n{body}EndFunction\n"
        ));
        lower_fragment_with_quest_properties_and_providers(&body, &HashSet::new(), Some(&providers))
    };
    let cap = crate::fragment::MAX_PROVIDER_FRAGMENT_BARRIERS;
    let calls = |count: usize| "Game.GetModCount()\n".repeat(count);

    let fits = lower(format!("{}Self.SetStage(10)\n", calls(cap - 1)))
        .expect("one barrier under the cap lowers");
    assert_eq!(fits.len(), cap);
    assert_eq!(
        lower(format!("{}Self.SetStage(10)\n", calls(cap))),
        None,
        "a tail past barrier {cap} would be dropped after the prefix applied"
    );

    // Branches are alternatives: {cap - 1} + {cap - 1} across them is fine…
    let branched = format!(
        "{}If Self.GetStageDone(2) == false\n{}Else\n{}EndIf\nSelf.SetStage(20)\n",
        calls(0),
        calls(cap - 1),
        calls(cap - 1)
    );
    assert!(lower(branched).is_some());
    // …but the path through the deeper branch plus a call before it is not.
    let too_deep = format!(
        "{}If Self.GetStageDone(2) == false\n{}Else\n{}EndIf\n",
        calls(2),
        calls(cap - 1),
        calls(1)
    );
    assert_eq!(lower(too_deep), None);
}

#[test]
fn provider_calls_preserve_top_level_fragment_order() {
    let providers = PapyrusProviderCatalog::engine_compatibility();
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         Self.SetStage(10)\n\
         Game.GetModCount()\n\
         Game.IsPluginInstalled(\"Update.esm\")\n\
         EndFunction\n",
    );
    let effects = lower_fragment_with_quest_properties_and_providers(
        &body,
        &HashSet::new(),
        Some(&providers),
    )
    .unwrap();
    assert!(matches!(effects[0], Effect::SetStage { stage: 10, .. }));
    assert!(matches!(
        &effects[1],
        Effect::ProviderCall(call)
            if call.route == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_COUNT_ROUTE
    ));
    assert!(matches!(
        &effects[2],
        Effect::ProviderCall(call)
            if call.route
                == byroredux_sdk::compatibility::PAPYRUS_GAME_IS_PLUGIN_INSTALLED_ROUTE
    ));

    let reordered = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         Game.GetModCount()\n\
         Self.SetStage(10)\n\
         EndFunction\n",
    );
    let reordered_effects = lower_fragment_with_quest_properties_and_providers(
        &reordered,
        &HashSet::new(),
        Some(&providers),
    )
    .unwrap();
    assert!(matches!(reordered_effects[0], Effect::ProviderCall(_)));
    assert!(matches!(
        reordered_effects[1],
        Effect::SetStage { stage: 10, .. }
    ));

    let conditional = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         If Self.GetStageDone(2) == false\n\
           Game.GetModCount()\n\
           Self.SetStage(10)\n\
         Else\n\
           Game.IsPluginInstalled(\"Update.esm\")\n\
         EndIf\n\
         Self.SetStage(20)\n\
         EndFunction\n",
    );
    let conditional_effects = lower_fragment_with_quest_properties_and_providers(
        &conditional,
        &HashSet::new(),
        Some(&providers),
    )
    .unwrap();
    let Effect::Conditional {
        then_effects,
        else_effects,
        ..
    } = &conditional_effects[0]
    else {
        panic!("expected conditional effect");
    };
    assert!(matches!(then_effects[0], Effect::ProviderCall(_)));
    assert!(matches!(
        then_effects[1],
        Effect::SetStage { stage: 10, .. }
    ));
    assert!(matches!(else_effects[0], Effect::ProviderCall(_)));
    assert!(matches!(
        conditional_effects[1],
        Effect::SetStage { stage: 20, .. }
    ));
}

#[test]
fn lowers_quest_lifecycle_and_bulk_objective_effects() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         Self.Start()\n\
         Self.SetActive(true)\n\
         Self.FailAllObjectives()\n\
         Self.CompleteQuest()\n\
         Self.Stop()\n\
         Self.Reset()\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::StartQuest {
                quest: QuestRef::SelfRef,
            },
            Effect::SetQuestActive {
                quest: QuestRef::SelfRef,
                active: true,
            },
            Effect::FailAllObjectives {
                quest: QuestRef::SelfRef,
            },
            Effect::CompleteQuest {
                quest: QuestRef::SelfRef,
            },
            Effect::StopQuest {
                quest: QuestRef::SelfRef,
            },
            Effect::ResetQuest {
                quest: QuestRef::SelfRef,
            },
        ])
    );
}

#[test]
fn lowers_bound_local_then_set_stage() {
    // The top fragment template `{$=$;$.setstage(#)}`: a quest local
    // bound from GetOwningQuest then used.
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_1()\n\
         Quest kmyQuest = Self.GetOwningQuest()\n\
         kmyQuest.SetStage(30)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::SetStage {
            quest: QuestRef::OwningQuest,
            stage: 30
        }])
    );
}

/// The shape the `.pex` decompiler emits for a locals-table fragment:
/// a bare declaration, then the Champollion autocast as an assignment.
/// Both locals are declared, so the fragment lowers (#5638 only refuses
/// assignment to a name nothing declared).
#[test]
fn lowers_the_decompiled_autocast_prologue() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_1()\n\
         Quest __temp\n\
         Quest kmyQuest\n\
         __temp = Self as Quest\n\
         kmyQuest = __temp\n\
         kmyQuest.SetStage(30)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 30
        }])
    );
    // Without the declarations the same assignments are writes to
    // script state the lowerer cannot model, so the fragment declines.
    let undeclared = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_1()\n\
         kmyQuest = Self as Quest\n\
         kmyQuest.SetStage(30)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&undeclared), None);
}

/// #5634: `OwningQuest` is the quest owning THIS script's alias, so only
/// `Self.GetOwningQuest()` may map to it. `OtherAlias.GetOwningQuest()`
/// names another alias's quest and used to stage / gate on the context
/// quest — in a guard atom the mixed-quest check passed because both
/// sides resolved to `OwningQuest`.
#[test]
fn get_owning_quest_lowers_only_on_self() {
    let on_self = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_1()\n\
         Self.GetOwningQuest().SetStage(10)\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&on_self),
        Some(vec![Effect::SetStage { quest: QuestRef::OwningQuest, stage: 10 }])
    );

    let foreign_effect = first_fn_body(
        "ScriptName QF extends Quest\n\
         ReferenceAlias Property OtherAlias Auto\n\
         Function Fragment_1()\n\
         OtherAlias.GetOwningQuest().SetStage(10)\n\
         EndFunction\n",
    );
    assert_eq!(lower_fragment(&foreign_effect), None, "foreign receiver, effect");

    let foreign_guard = first_fn_body(
        "ScriptName QF extends Quest\n\
         ReferenceAlias Property OtherAlias Auto\n\
         Function Fragment_1()\n\
         If OtherAlias.GetOwningQuest().GetStageDone(5) == 0\n\
           Self.GetOwningQuest().SetStage(10)\n\
         EndIf\n\
         EndFunction\n",
    );
    assert_eq!(lower_fragment(&foreign_guard), None, "foreign receiver, guard atom");
}

#[test]
fn lowers_objective_pair() {
    // `{self.setobjectivecompleted(#,#);self.setobjectivedisplayed(#,#,#)}`.
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_2()\n\
         Self.SetObjectiveCompleted(10)\n\
         Self.SetObjectiveDisplayed(20)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::SetObjectiveCompleted {
                quest: QuestRef::SelfRef,
                objective: 10,
                completed: true,
            },
            Effect::SetObjectiveDisplayed {
                quest: QuestRef::SelfRef,
                objective: 20,
                displayed: true,
            },
        ])
    );
}

#[test]
fn objective_explicit_false_arg() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_3()\n Self.SetObjectiveDisplayed(5, false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::SetObjectiveDisplayed {
            quest: QuestRef::SelfRef,
            objective: 5,
            displayed: false,
        }])
    );
}

#[test]
fn objective_non_literal_arg_declines_whole_fragment() {
    // #2023 / SCR-D5-NEW2-01 — a present-but-non-literal 2nd argument
    // (an ordinary local bool, unconstrained Papyrus unlike the
    // auto-property-initializer case `bool_prop` guards) must NOT
    // silently collapse into the Papyrus-side default `true`. Pre-fix
    // `bool_arg` returned a single `None` for both "absent" and
    // "present but unevaluable," so `.unwrap_or(true)` masked this
    // exact case — asserting `completed: true` here would have
    // passed both before and after the fix, so the meaningful
    // assertion is that the whole fragment declines (`None`), not a
    // specific wrong value.
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_5()\n\
         Bool bWasSuccessful = true\n\
         Self.SetObjectiveCompleted(20, bWasSuccessful)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

#[test]
fn declines_on_unmodeled_effect() {
    // An object-targeting effect (PushActorAway) isn't in the table — the
    // whole fragment declines, never partially applies. (#3489 — this
    // used to exercise `Enable()`, then `Kill()`, then `SetGhost()`; all
    // are modeled now.)
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_4()\n\
         Self.SetStage(10)\n\
         akTarget.PushActorAway(akTarget, 5.0)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

#[test]
fn declines_on_side_effecting_binding() {
    // A non-quest binding whose initializer is a side-effecting call
    // (the spawn) must decline the whole fragment — otherwise the
    // spawn is silently dropped while the SetStage still applies (#1907).
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_5()\n\
         ObjectReference k = akActor.PlaceAtMe(SomeForm)\n\
         Self.SetStage(20)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

#[test]
fn side_effect_free_binding_is_recorded_not_declined() {
    // A pure-value non-quest local (an ident copy) has no side-effect
    // to drop, so it is recorded and lowering continues (#1907).
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_6()\n\
         ObjectReference k = akActor\n\
         Self.SetStage(20)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 20
        }])
    );
}

#[test]
fn lowers_add_item_on_bare_properties() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_7()\n\
         SomeContainer.AddItem(SomeItem, 5)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::AddItem {
            silent: false,
            container: ActorRef::Object(ObjectRef::Property("SomeContainer".into())),
            item: ObjectRef::Property("SomeItem".into()),
            count: ItemCount::Literal(5),
        }])
    );
}

#[test]
fn lowers_add_item_with_literal_silent_arg() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         SomeContainer.AddItem(SomeItem, 5, true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::AddItem {
            // #5913 — the literal `abSilent` is carried, not dropped.
            silent: true,
            container: ActorRef::Object(ObjectRef::Property("SomeContainer".into())),
            item: ObjectRef::Property("SomeItem".into()),
            count: ItemCount::Literal(5),
        }])
    );
}

/// M47.2 — the corpus's commonest reward shape: the player as the
/// container, the count defaulting to 1 when omitted.
#[test]
fn lowers_add_item_on_the_player_with_a_default_count() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Game.GetPlayer().AddItem(Reward as Form, 1, false)\n\
         Game.GetPlayer().AddItem(Gold001)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::AddItem {
                silent: false,
                container: ActorRef::Player,
                item: ObjectRef::Property("Reward".into()),
                count: ItemCount::Literal(1),
            },
            Effect::AddItem {
                silent: false,
                container: ActorRef::Player,
                item: ObjectRef::Property("Gold001".into()),
                count: ItemCount::Literal(1),
            },
        ])
    );
}

/// M47.2 — `RemoveItem(item, count = 1, silent = false, other = None)`:
/// the vanilla `Game.GetPlayer().RemoveItem(Gold, n, false, None)` shape,
/// the defaulted forms, and a move into another container.
#[test]
fn lowers_remove_item_shapes() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Game.GetPlayer().RemoveItem(Gold001 as Form, 50, false, None)\n\
         SomeChest.RemoveItem(SomeItem)\n\
         Game.GetPlayer().RemoveItem(SomeItem, 2, true, SomeChest)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::RemoveItem {
                container: ActorRef::Player,
                item: ObjectRef::Property("Gold001".into()),
                count: ItemCount::Literal(50),
                destination: None,
            },
            Effect::RemoveItem {
                container: ActorRef::Object(ObjectRef::Property("SomeChest".into())),
                item: ObjectRef::Property("SomeItem".into()),
                count: ItemCount::Literal(1),
                destination: None,
            },
            Effect::RemoveItem {
                container: ActorRef::Player,
                item: ObjectRef::Property("SomeItem".into()),
                count: ItemCount::Literal(2),
                destination: Some(ActorRef::Object(ObjectRef::Property(
                    "SomeChest".into()
                ))),
            },
        ])
    );
}

/// M47.2 — a count read from a global lowers to [`ItemCount::Global`] in
/// each spelling the corpus uses; the read happens when the effect applies.
#[test]
fn item_counts_read_from_a_global_lower() {
    for call in [
        "Game.GetPlayer().RemoveItem(Gold001, Ransom.GetValueInt(), false, None)",
        "Game.GetPlayer().RemoveItem(Gold001, Ransom.GetValue() as Int, false, None)",
        "Game.GetPlayer().RemoveItem(Gold001, Ransom.Value as Int, false, None)",
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
        ));
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::RemoveItem {
                container: ActorRef::Player,
                item: ObjectRef::Property("Gold001".into()),
                count: ItemCount::Global(ObjectRef::Property("Ransom".into())),
                destination: None,
            }]),
            "{call}"
        );
    }
}

/// M47.2 — `AddToMap` and the marker guards.
#[test]
fn map_markers_lower() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         If !LairMarker.IsMapMarkerVisible()\n LairMarker.AddToMap(false)\n EndIf\n\
         CityMarker.AddToMap(true)\n EndFunction\n",
    );
    let lair = || ObjectRef::Property("LairMarker".into());
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::Conditional {
                guards: vec![FragmentGuard::MapMarkerVisible {
                    marker: lair(),
                    expected: false
                }],
                then_effects: vec![Effect::AddToMap {
                    marker: lair(),
                    fast_travel: false
                }],
                else_effects: vec![],
            },
            Effect::AddToMap {
                marker: ObjectRef::Property("CityMarker".into()),
                fast_travel: true
            },
        ])
    );
}

/// M47.2 — `SetCleared` on a location property and on a location
/// alias's `GetLocation()`, under an `IsCleared` guard.
#[test]
fn location_clearing_lowers() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         If !Alias_Dungeon.GetLocation().IsCleared()\n Alias_Dungeon.GetLocation().SetCleared()\n EndIf\n\
         BleakFallsLocation.SetCleared(false)\n EndFunction\n",
    );
    let dungeon = || ObjectRef::Property("Alias_Dungeon".into());
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::Conditional {
                guards: vec![FragmentGuard::LocationCleared {
                    location: dungeon(),
                    expected: false
                }],
                then_effects: vec![Effect::SetLocationCleared {
                    location: dungeon(),
                    cleared: true
                }],
                else_effects: vec![],
            },
            Effect::SetLocationCleared {
                location: ObjectRef::Property("BleakFallsLocation".into()),
                cleared: false
            },
        ])
    );
}

/// M47.2 — enable-state, faction, 3D-loaded and distance guards.
#[test]
fn reference_faction_loaded_and_distance_guards_lower() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         If HallNormalState.IsDisabled() == False && Game.GetPlayer().IsInFaction(SonsFaction) == False\n\
         Self.SetStage(10)\n\
         EndIf\n\
         If !Alias_Dexion.GetReference().Is3DLoaded() && Game.GetPlayer().GetDistance(HitchingPost) < 3000.0\n\
         Self.SetStage(20)\n\
         EndIf\n EndFunction\n",
    );
    let stage = |stage| Effect::SetStage {
        quest: QuestRef::SelfRef,
        stage,
    };
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::Conditional {
                guards: vec![
                    FragmentGuard::ReferenceEnabled {
                        object: ObjectRef::Property("HallNormalState".into()),
                        expected: true,
                    },
                    FragmentGuard::InFaction {
                        actor: ActorRef::Player,
                        faction: ObjectRef::Property("SonsFaction".into()),
                        expected: false,
                    },
                ],
                then_effects: vec![stage(10)],
                else_effects: vec![],
            },
            Effect::Conditional {
                guards: vec![
                    FragmentGuard::Loaded3D {
                        object: ObjectRef::Property("Alias_Dexion".into()),
                        expected: false,
                    },
                    FragmentGuard::Distance {
                        from: ActorRef::Player,
                        to: ActorRef::Object(ObjectRef::Property("HitchingPost".into())),
                        comparison: GuardComparison::Lt,
                        value: 3000.0,
                    },
                ],
                then_effects: vec![stage(20)],
                else_effects: vec![],
            },
        ])
    );
}

/// M47.2 — FO4's `Game.RewardPlayerXP`, `GivePlayerCaps` /
/// `RemovePlayerCaps`, and `SetObjectiveSkipped` (its `Quest.psc` body).
#[test]
fn fallout4_game_rewards_and_skipped_objectives_lower() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Game.RewardPlayerXP(50)\n\
         Game.RewardPlayerXP(25, True)\n\
         Game.GivePlayerCaps(100)\n\
         Game.RemovePlayerCaps(20)\n\
         Self.SetObjectiveSkipped(10)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::RewardPlayerXp {
                amount: ItemCount::Literal(50),
                direct: false
            },
            Effect::RewardPlayerXp {
                amount: ItemCount::Literal(25),
                direct: true
            },
            Effect::PlayerCaps { delta: 100, silent: false },
            Effect::PlayerCaps { delta: -20, silent: false },
            Effect::Conditional {
                guards: vec![FragmentGuard::ObjectiveCompleted {
                    quest: QuestRef::SelfRef,
                    objective: 10,
                    expected: false,
                }],
                then_effects: vec![Effect::SetObjectiveDisplayed {
                    quest: QuestRef::SelfRef,
                    objective: 10,
                    displayed: false,
                }],
                else_effects: vec![],
            },
        ])
    );
}

/// M47.2 — `SetRelationshipRank` with a literal (negative too) and the
/// `GetRelationshipRank` guard.
#[test]
fn relationship_rank_lowers() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         If Alias_Ysolda.GetActorRef().GetRelationshipRank(Game.GetPlayer()) < 1\n\
         Alias_Ysolda.GetActorRef().SetRelationshipRank(Game.GetPlayer(), 1)\n\
         EndIf\n\
         Alias_Nazeem.GetActorRef().SetRelationshipRank(Game.GetPlayer(), -2)\n EndFunction\n",
    );
    let alias = |name: &str| ActorRef::Object(ObjectRef::Property(name.into()));
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::Conditional {
                guards: vec![FragmentGuard::RelationshipRank {
                    actor: alias("Alias_Ysolda"),
                    other: ActorRef::Player,
                    comparison: GuardComparison::Lt,
                    value: 1.0,
                }],
                then_effects: vec![Effect::SetRelationshipRank {
                    actor: alias("Alias_Ysolda"),
                    other: ActorRef::Player,
                    rank: 1,
                }],
                else_effects: vec![],
            },
            Effect::SetRelationshipRank {
                actor: alias("Alias_Nazeem"),
                other: ActorRef::Player,
                rank: -2,
            },
        ])
    );
}

/// M47.2 — `Kill` with and without a killer (a player victim declines),
/// and an alias's `TryToEvaluatePackage`.
#[test]
fn kill_and_try_to_evaluate_package_lower() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Alias_Kyr.GetActorRef().Kill(None)\n\
         Alias_Victim.GetActorRef().Kill(Game.GetPlayer())\n\
         Alias_Spectator1.TryToEvaluatePackage()\n EndFunction\n",
    );
    let alias = |name: &str| ObjectRef::Property(name.into());
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::Kill {
                actor: ActorRef::Object(alias("Alias_Kyr")),
                killer: None,
                silent: false,
            },
            Effect::Kill {
                actor: ActorRef::Object(alias("Alias_Victim")),
                killer: Some(ActorRef::Player),
                silent: false,
            },
            Effect::EvaluatePackage {
                actor: ActorRef::Object(alias("Alias_Spectator1")),
            },
        ])
    );
    let silent = first_fn_body(
        "ScriptName QF extends Quest\nFunction Fragment_8()\nAlias_Kyr.GetActorRef().KillSilent(Game.GetPlayer())\nEndFunction\n",
    );
    assert_eq!(
        lower_fragment(&silent),
        Some(vec![Effect::Kill {
            actor: ActorRef::Object(alias("Alias_Kyr")),
            killer: Some(ActorRef::Player),
            silent: true,
        }])
    );
    let player = first_fn_body(
        "ScriptName QF extends Quest\nFunction Fragment_8()\nGame.GetPlayer().Kill(None)\nEndFunction\n",
    );
    assert_eq!(lower_fragment(&player), None, "the player's death is not modelled");
}

/// M47.2 — `SetEssential` / `SetProtected` on an actor's base and on an
/// ActorBase property; FO4's two-argument `StartCombat`.
#[test]
fn base_protection_and_preferred_target_combat_lower() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Alias_Thonar.GetActorRef().GetActorBase().SetEssential(false)\n\
         EmperorBase.SetProtected()\n\
         Alias_Guard.GetActorRef().StartCombat(Game.GetPlayer(), false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::SetBaseProtection {
                base: ActorBaseRef::OfActor(ActorRef::Object(ObjectRef::Property(
                    "Alias_Thonar".into()
                ))),
                protected: false,
                value: false,
            },
            Effect::SetBaseProtection {
                base: ActorBaseRef::Form(ObjectRef::Property("EmperorBase".into())),
                protected: true,
                value: true,
            },
            Effect::StartCombat {
                actor: ActorRef::Object(ObjectRef::Property("Alias_Guard".into())),
                target: ActorRef::Player,
            },
        ])
    );
}

/// #5972 — Fallout 4 / Starfield declare `SetEssential` / `SetProtected` on
/// `Actor` (per instance) and on the alias types; Skyrim's live on `ActorBase`.
/// With the script's declared property types, only an ActorBase receiver lowers
/// into a base write; an `Actor` / alias property, and an alias getter's result
/// (always an actor), decline to the VM instead of lowering into a
/// wrong-keyed write nothing reads.
#[test]
fn per_instance_actor_receivers_decline_base_protection() {
    const SOURCE: &str = "ScriptName QF extends Quest\n\
         Actor Property Companion Auto\n\
         ReferenceAlias Property Ally Auto\n\
         RefCollectionAlias Property Squad Auto\n\
         ActorBase Property EmperorBase Auto\n\
         Function Fragment_0()\n\
         EmperorBase.SetProtected()\n\
         EndFunction\n";
    let (script, errs) = parse_script(SOURCE).expect("parses");
    assert!(errs.is_empty(), "{errs:?}");
    let declared = crate::fragment::per_instance_actor_property_names(&script);
    assert_eq!(
        declared,
        ["companion", "ally", "squad"].map(String::from).into_iter().collect(),
        "ActorBase is not a per-instance type"
    );
    let lower = |statement: &str, declared: &HashSet<String>| {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_0()\n{statement}\nEndFunction\n"
        ));
        lower_fragment_with_script_metadata(&body, &HashSet::new(), &HashSet::new(), declared, None)
    };
    // The ActorBase shapes lower, with or without metadata.
    for statement in ["EmperorBase.SetProtected()", "Companion.GetActorBase().SetEssential(true)"] {
        assert!(lower(statement, &declared).is_some(), "{statement}");
        assert!(lower(statement, &HashSet::new()).is_some(), "{statement} (no metadata)");
    }
    // A property declared Actor / alias is the per-instance form: declines.
    for statement in ["Companion.SetEssential(true)", "Ally.SetProtected()", "Squad.SetEssential(false)"] {
        assert_eq!(lower(statement, &declared), None, "{statement}");
    }
    // An alias getter returns an Actor, never an ActorBase: declines on its own.
    assert_eq!(lower("Ally.GetActorRef().SetEssential(true)", &declared), None);
    assert_eq!(lower("Ally.GetActorRef().SetEssential(true)", &HashSet::new()), None);
}

/// M47.2 — `SendStoryEvent` with nothing, and with a location, the
/// speaker and a value.
#[test]
fn send_story_event_lowers() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         MQGreybeardCallKeyword.SendStoryEvent(None, None, None, 0, 0)\n\
         SomeKeyword.SendStoryEvent(SomeLocation, Game.GetPlayer() as ObjectReference, None, 3, 0)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::SendStoryEvent {
                keyword: ObjectRef::Property("MQGreybeardCallKeyword".into()),
                location: None,
                references: [None, None],
                values: [0, 0],
            },
            Effect::SendStoryEvent {
                keyword: ObjectRef::Property("SomeKeyword".into()),
                location: Some(ObjectRef::Property("SomeLocation".into())),
                references: [Some(ActorRef::Player), None],
                values: [3, 0],
            },
        ])
    );
}

/// M47.2 — `BlockActivation` with its default, an explicit unblock, and
/// FO4's prompt flag.
#[test]
fn block_activation_lowers() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         SomeDoor.BlockActivation()\n\
         Alias_Gate.GetRef().BlockActivation(false)\n\
         SomeDoor.BlockActivation(true, true)\n EndFunction\n",
    );
    let door = || ObjectRef::Property("SomeDoor".into());
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::BlockActivation {
                object: door(),
                blocked: true
            },
            Effect::BlockActivation {
                object: ObjectRef::Property("Alias_Gate".into()),
                blocked: false
            },
            Effect::BlockActivation {
                object: door(),
                blocked: true
            },
        ])
    );
}

/// M47.2 — FO4's alias `TryToSetActorValue` and Skyrim's named `SetAV` /
/// `SetActorValue`.
#[test]
fn actor_value_writes_lower_in_each_spelling() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Alias_Danse.TryToSetActorValue(CA_WantsToTalk, 2.0)\n\
         Game.GetPlayer().SetAV(\"Variable06\", 3 as Float)\n\
         Alias_Saadia.GetActorRef().SetActorValue(\"Aggression\", 0.0)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::SetActorValue {
                actor: ActorRef::Object(ObjectRef::Property("Alias_Danse".into())),
                actor_value: ObjectRef::Property("CA_WantsToTalk".into()),
                value: crate::translate::effects::ValueExpr::Literal(2.0),
            },
            Effect::SetActorValueNamed {
                actor: ActorRef::Player,
                name: "Variable06".into(),
                value: crate::translate::effects::ValueExpr::Literal(3.0),
            },
            Effect::SetActorValueNamed {
                actor: ActorRef::Object(ObjectRef::Property("Alias_Saadia".into())),
                name: "Aggression".into(),
                value: crate::translate::effects::ValueExpr::Literal(0.0),
            },
        ])
    );
}

/// M47.2 — writes to a global's `.Value` (directly or through the quest
/// script's property) and quest-variable increments.
#[test]
fn global_value_assignments_and_counters_lower() {
    let property = |name: &str| ObjectRef::Property(name.into());
    for (statement, expected) in [
        (
            "RadioReady.Value = 1.0",
            Effect::SetGlobalValue {
                global: property("RadioReady"),
                value: crate::translate::effects::ValueExpr::Literal(1.0),
            },
        ),
        (
            "kmyQuest.pRDSGlobal.Value = 1 as Float",
            Effect::SetGlobalValue {
                global: property("pRDSGlobal"),
                value: crate::translate::effects::ValueExpr::Literal(1.0),
            },
        ),
        (
            "Counter.Value = Counter.Value + 1.0",
            Effect::SetGlobalValue {
                global: property("Counter"),
                value: ValueExpr::Arith(
                    Box::new(ValueExpr::Global(property("Counter"))),
                    ArithOp::Add,
                    Box::new(ValueExpr::Literal(1.0)),
                ),
            },
        ),
        (
            "kmyQuest.SynthsFreed = kmyQuest.SynthsFreed + 1",
            Effect::ModQuestVariable {
                quest: QuestRef::SelfRef,
                variable: "SynthsFreed".into(),
                delta: 1.0,
            },
        ),
        (
            "kmyQuest.Tries = kmyQuest.Tries - 2",
            Effect::ModQuestVariable {
                quest: QuestRef::SelfRef,
                variable: "Tries".into(),
                delta: -2.0,
            },
        ),
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Quest __temp = Self as Quest\n\
             MyQuestScript kmyQuest = __temp as MyQuestScript\n\
             {statement}\n EndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), Some(vec![expected]), "{statement}");
    }
    // A different variable on the right is not a counter.
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Quest __temp = Self as Quest\n\
         MyQuestScript kmyQuest = __temp as MyQuestScript\n\
         kmyQuest.A = kmyQuest.B + 1\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

/// M47.2 — `ForceRefTo` with the player, an alias's reference or a
/// property, and `Clear`, on an alias property receiver.
#[test]
fn alias_overrides_lower() {
    let alias = || ObjectRef::Property("Alias_Giver".into());
    for (call, target) in [
        (
            "Alias_Giver.ForceRefTo(Game.GetPlayer() as ObjectReference)",
            Some(ActorRef::Player),
        ),
        (
            "Alias_Giver.ForceRefTo(Alias_Other.GetReference())",
            Some(ActorRef::Object(ObjectRef::Property("Alias_Other".into()))),
        ),
        ("Alias_Giver.Clear()", None),
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
        ));
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::ForceAlias {
                alias: alias(),
                target,
            }]),
            "{call}"
        );
    }
}

/// M47.2 — `AddPerk` in both arities and `RemovePerk`.
#[test]
fn perk_changes_lower() {
    let perk = || ObjectRef::Property("SomePerk".into());
    for (call, expected) in [
        (
            "Game.GetPlayer().AddPerk(SomePerk)",
            Effect::ChangePerk {
                actor: ActorRef::Player,
                perk: perk(),
                add: true,
            },
        ),
        (
            "Companion.AddPerk(SomePerk, false)",
            Effect::ChangePerk {
                actor: ActorRef::Object(ObjectRef::Property("Companion".into())),
                perk: perk(),
                add: true,
            },
        ),
        (
            "Game.GetPlayer().RemovePerk(SomePerk)",
            Effect::ChangePerk {
                actor: ActorRef::Player,
                perk: perk(),
                add: false,
            },
        ),
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), Some(vec![expected]), "{call}");
    }
}

/// M47.2 — the FO4 / Starfield spellings of the enable and lock writes.
#[test]
fn no_wait_try_to_and_unlock_spellings_lower() {
    let marker = || ObjectRef::Property("Marker".into());
    for (call, expected) in [
        (
            "Marker.EnableNoWait(false)",
            Effect::Enable {
                object: marker(),
                fade_in: false,
            },
        ),
        (
            "Marker.TryToEnable()",
            Effect::Enable {
                object: marker(),
                fade_in: false,
            },
        ),
        (
            "Marker.DisableNoWait(true)",
            Effect::Disable {
                object: marker(),
                fade_out: true,
            },
        ),
        (
            "Marker.TryToDisable()",
            Effect::Disable {
                object: marker(),
                fade_out: false,
            },
        ),
        (
            "Marker.Unlock(false)",
            Effect::SetLocked {
                target: marker(),
                locked: false,
            },
        ),
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), Some(vec![expected]), "{call}");
    }
}

/// M47.2 — global writes from run-time numbers: `Mod`, a
/// self-increment, a value derived from another global, arithmetic, and
/// `SetValueInt` with a literal.
#[test]
fn relative_global_writes_lower() {
    let property = |name: &str| ObjectRef::Property(name.into());
    let global = |name: &str| Box::new(ValueExpr::Global(property(name)));
    let literal = |value: f32| Box::new(ValueExpr::Literal(value));
    let set = |target: &str, value| Effect::SetGlobalValue {
        global: property(target),
        value,
    };
    for (call, expected) in [
        (
            "Counter.Mod(1.0)",
            set("Counter", ValueExpr::Arith(global("Counter"), ArithOp::Add, literal(1.0))),
        ),
        (
            "Counter.SetValue(Counter.GetValue() + 1.0)",
            set("Counter", ValueExpr::Arith(global("Counter"), ArithOp::Add, literal(1.0))),
        ),
        (
            "RadioReady.SetValue(GameDaysPassed.GetValue() + 1.0)",
            set(
                "RadioReady",
                ValueExpr::Arith(global("GameDaysPassed"), ArithOp::Add, literal(1.0)),
            ),
        ),
        (
            "Counter.SetValue(Counter.GetValue() * 2.0)",
            set("Counter", ValueExpr::Arith(global("Counter"), ArithOp::Mul, literal(2.0))),
        ),
        (
            "Counter.Mod(Other.GetValue())",
            set("Counter", ValueExpr::Arith(global("Counter"), ArithOp::Add, global("Other"))),
        ),
        ("GameHour.SetValueInt(9)", set("GameHour", ValueExpr::Literal(9.0))),
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), Some(vec![expected]), "{call}");
    }
    // #6215 — `GetValueInt` truncates, which `ValueExpr` cannot: the write is left to the VM.
    let body = first_fn_body(
        "ScriptName QF extends Quest\nFunction Fragment_8()\nCounter.SetValue(Counter.GetValueInt() - 2)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
    let call =
        "Counter.SetValue(Counter.GetValue() * Game.GetPlayer().GetEquippedWeapon().GetBaseDamage())";
    let body = first_fn_body(&format!(
        "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
    ));
    assert_eq!(lower_fragment(&body), None, "{call}");
}

/// A computed item (`Game.GetCaps()`), a count from an unknown call, a
/// non-literal silent flag, a negative count or a fifth argument all
/// decline rather than guess.
#[test]
fn remove_item_declines_what_it_cannot_read() {
    for call in [
        "Game.GetPlayer().RemoveItem(Gold001, GetRansom(), false, None)",
        "Game.GetPlayer().RemoveItem(Game.GetCaps(), GetRansom(), false, None)",
        "Game.GetPlayer().RemoveItem(Gold001, 5, bQuiet, None)",
        "Game.GetPlayer().RemoveItem(Gold001, -5)",
        "Game.GetPlayer().RemoveItem(Gold001, 5, false, None, 1)",
        "Game.GetPlayer().RemoveItem()",
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), None, "{call}");
    }
}

/// M47.2 — the player gaining or losing a literal count of
/// `Game.GetCaps()` is `PlayerCaps`, like `GivePlayerCaps` /
/// `RemovePlayerCaps`.
#[test]
fn get_caps_transfers_are_player_caps() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\nFunction Fragment_8()\n\
         Game.GetPlayer().RemoveItem(Game.GetCaps() as Form, 75, false, None)\n\
         Game.GetPlayer().AddItem(Game.GetCaps() as Form, 20, false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::PlayerCaps { delta: -75, silent: false },
            Effect::PlayerCaps { delta: 20, silent: false },
        ])
    );
}

/// M47.2 — quest-script property writes: through a quest local bound
/// from `Self as Quest` (QF fragments), through `GetOwningQuest()`'s
/// cast (topic-info fragments), and a Bool literal as 0 / 1.
#[test]
fn lowers_quest_variable_writes() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Quest __temp = Self as Quest\n\
         MS11QuestScript kmyQuest = __temp as MS11QuestScript\n\
         kmyQuest.FriendsKilled = 3\n\
         (Self.GetOwningQuest() as MS11QuestScript).GetInHjerimTovaPointer = true\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::SetQuestVariable {
                quest: QuestRef::SelfRef,
                variable: "FriendsKilled".into(),
                value: 3.0,
            },
            Effect::SetQuestVariable {
                quest: QuestRef::OwningQuest,
                variable: "GetInHjerimTovaPointer".into(),
                value: 1.0,
            },
        ])
    );
}

/// A compound operator, a bare `Self` receiver (a topic-info fragment's
/// own script, not the quest), an unknown property receiver and an index
/// target all decline. (`X = X + 1` lowers since M47.2.)
#[test]
fn quest_variable_writes_decline_what_they_cannot_name() {
    for statement in [
        "kmyQuest.Count += 1",
        "Self.Count = 1",
        "SomeObjectProperty.Count = 1",
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Quest __temp = Self as Quest\n\
             MyQuestScript kmyQuest = __temp as MyQuestScript\n\
             {statement}\n EndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), None, "{statement}");
    }
}

/// The first function of `src`, params and body, lowered as an INFO
/// fragment.
fn lower_first_info_fragment(src: &str) -> Option<Vec<Effect>> {
    let (script, errs) = parse_script(src).expect("parses");
    assert!(errs.is_empty(), "{errs:?}");
    let function = script
        .body
        .iter()
        .find_map(|item| match &item.node {
            ScriptItem::Function(f) => Some(f.clone()),
            _ => None,
        })
        .expect("a function");
    lower_info_fragment(
        &function.params,
        &function.body,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        None,
        None,
    )
}

/// M47.2 — an INFO fragment's `akSpeakerRef`, and the Champollion
/// `akSpeaker = akSpeakerRef as Actor` local, are the line's speaker.
#[test]
fn info_fragments_lower_the_speaker() {
    let effects = lower_first_info_fragment(
        "ScriptName TIF__0005520F extends TopicInfo\n\
         Function Fragment_0(ObjectReference akSpeakerRef)\n\
         Actor akSpeaker = akSpeakerRef as Actor\n\
         akSpeaker.AddItem(Gold001, 10)\n\
         akSpeakerRef.RemoveItem(Ale)\n EndFunction\n",
    );
    assert_eq!(
        effects,
        Some(vec![
            Effect::AddItem {
                silent: false,
                container: ActorRef::Speaker,
                item: ObjectRef::Property("Gold001".into()),
                count: ItemCount::Literal(10),
            },
            Effect::RemoveItem {
                container: ActorRef::Speaker,
                item: ObjectRef::Property("Ale".into()),
                count: ItemCount::Literal(1),
                destination: None,
            },
        ])
    );
    // The quest/scene path has no speaker: the same body declines there
    // rather than treating `akSpeaker` as a VMAD property.
    let body = first_fn_body(
        "ScriptName TIF__0005520F extends TopicInfo\n\
         Function Fragment_0(ObjectReference akSpeakerRef)\n\
         Actor akSpeaker = akSpeakerRef as Actor\n\
         akSpeaker.AddItem(Gold001, 10)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

/// A speaker fragment with a latent tail declines (the speaker is a live
/// entity a saved continuation cannot name) — also when the speaker is
/// only used inside an `If` branch, which lowers against a scope clone.
#[test]
fn a_speaker_fragment_with_a_latent_tail_declines() {
    for body in [
        "akSpeaker.AddItem(Gold001, 10)\nUtility.Wait(1.0)",
        "If Self.GetOwningQuest().GetStageDone(10)\nakSpeaker.AddItem(Gold001, 10)\nEndIf\nUtility.Wait(1.0)",
    ] {
        let effects = lower_first_info_fragment(&format!(
            "ScriptName TIF__1 extends TopicInfo\n\
             Function Fragment_0(ObjectReference akSpeakerRef)\n\
             Actor akSpeaker = akSpeakerRef as Actor\n\
             {body}\n EndFunction\n"
        ));
        assert_eq!(effects, None, "{body}");
    }
    // Without the speaker the same wait lowers.
    assert!(lower_first_info_fragment(
        "ScriptName TIF__1 extends TopicInfo\n\
         Function Fragment_0(ObjectReference akSpeakerRef)\n\
         Game.GetPlayer().AddItem(Gold001, 10)\n\
         Utility.Wait(1.0)\n EndFunction\n"
    )
    .is_some());
}

/// M47.2 — faction membership calls on an alias actor, the player and
/// (in a topic-info fragment) the speaker; `SetFactionRank` takes a
/// literal rank in `i8` range, negative included.
#[test]
fn lowers_faction_membership_calls() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Alias_VictimFather.GetActorRef().AddToFaction(CR08ExclusionFaction)\n\
         Game.GetPlayer().RemoveFromFaction(PlayerFaction)\n\
         Alias_Boss.GetActorRef().SetFactionRank(BanditFaction, -1)\n EndFunction\n",
    );
    let object = |name: &str| ObjectRef::Property(name.into());
    assert_eq!(
        lower_fragment(&first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_9()\n\
             Game.GetPlayer().RemoveFromAllFactions()\n EndFunction\n"
        )),
        Some(vec![Effect::ChangeFaction {
            actor: ActorRef::Player,
            faction: object(""),
            change: crate::FactionChange::RemoveAll,
        }])
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::ChangeFaction {
                actor: ActorRef::Object(object("Alias_VictimFather")),
                faction: object("CR08ExclusionFaction"),
                change: crate::FactionChange::Add,
            },
            Effect::ChangeFaction {
                actor: ActorRef::Player,
                faction: object("PlayerFaction"),
                change: crate::FactionChange::Remove,
            },
            Effect::ChangeFaction {
                actor: ActorRef::Object(object("Alias_Boss")),
                faction: object("BanditFaction"),
                change: crate::FactionChange::SetRank(-1),
            },
        ])
    );
    assert_eq!(
        lower_first_info_fragment(
            "ScriptName TIF__1 extends TopicInfo\n\
             Function Fragment_0(ObjectReference akSpeakerRef)\n\
             Actor akSpeaker = akSpeakerRef as Actor\n\
             akSpeaker.RemoveFromFaction(WIKillThankFaction)\n EndFunction\n"
        ),
        Some(vec![Effect::ChangeFaction {
            actor: ActorRef::Speaker,
            faction: object("WIKillThankFaction"),
            change: crate::FactionChange::Remove,
        }])
    );
    for call in [
        "Game.GetPlayer().SetFactionRank(SomeFaction, 300)",
        "Game.GetPlayer().SetFactionRank(SomeFaction, iRank)",
        "Game.GetPlayer().AddToFaction()",
        "Game.GetPlayer().AddToFaction(SomeFaction, 1)",
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), None, "{call}");
    }
}

/// M47.2 — Skyrim's `ReferenceAlias.GetReference()` /
/// `GetActorReference()` resolve like FO4's `GetRef()` / `GetActorRef()`.
#[test]
fn skyrim_alias_getters_resolve_like_fo4s() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Alias_Treasure.GetReference().Enable(false)\n\
         Alias_Cicero.GetActorReference().EvaluatePackage()\n EndFunction\n",
    );
    let effects = lower_fragment(&body).expect("both lower");
    assert!(matches!(
        &effects[0],
        Effect::Enable { object, .. } if *object == ObjectRef::Property("Alias_Treasure".into())
    ));
    assert_eq!(
        effects[1],
        Effect::EvaluatePackage { actor: ActorRef::Object(ObjectRef::Property("Alias_Cicero".into())) }
    );
}

/// M47.2 — FO4 / Starfield two-argument `SetValue(akAV, afValue)` sets an
/// actor value; the one-argument form stays `GlobalVariable.SetValue`.
#[test]
fn lowers_actor_value_set_value() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Alias_SkinnyMalone.GetActorRef().SetValue(pAggression, 2.0)\n\
         SomeGlobal.SetValue(3.0)\n EndFunction\n",
    );
    let effects = lower_fragment(&body).expect("lowers");
    assert_eq!(
        effects[0],
        Effect::SetActorValue {
            actor: ActorRef::Object(ObjectRef::Property("Alias_SkinnyMalone".into())),
            actor_value: ObjectRef::Property("pAggression".into()),
            value: crate::translate::effects::ValueExpr::Literal(2.0),
        }
    );
    assert!(matches!(effects[1], Effect::SetGlobalValue { .. }));
    let computed = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_8()\n\
         Alias_A.GetActorRef().SetValue(pAggression, fLevel)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&computed), None);
}

/// A [`ScriptLibrary`] over `.psc` sources, keyed by lowercased name.
struct PscLibrary(HashMap<String, std::sync::Arc<byroredux_papyrus::ast::Script>>);

impl PscLibrary {
    fn new(sources: &[&str]) -> Self {
        Self(
            sources
                .iter()
                .map(|src| {
                    let (script, errs) = parse_script(src).expect("parses");
                    assert!(errs.is_empty(), "{errs:?}");
                    (script.name.node.0.to_ascii_lowercase(), std::sync::Arc::new(script))
                })
                .collect(),
        )
    }
}

impl ScriptLibrary for PscLibrary {
    fn script(&self, name: &str) -> Option<std::sync::Arc<byroredux_papyrus::ast::Script>> {
        self.0.get(&name.to_ascii_lowercase()).cloned()
    }
}

/// M47.2 — a property typed with a script that (transitively) extends
/// Quest is a quest property once the library supplies the `extends`
/// chain, so a write to its variable lowers.
#[test]
fn quest_script_typed_properties_resolve_through_the_library() {
    let library = PscLibrary::new(&[
        "ScriptName TG04PQSScript extends TG04BaseScript\nInt Property pTG04PostChoice Auto\n",
        "ScriptName TG04BaseScript extends Quest\n",
        "ScriptName NotAQuestScript extends ObjectReference\n",
    ]);
    const SOURCE: &str = "ScriptName TIF_000D3AB3 extends TopicInfo\n\
         TG04PQSScript Property pTG04PQS Auto\n\
         NotAQuestScript Property Other Auto\n\
         Function Fragment_0(ObjectReference akSpeakerRef)\n\
         pTG04PQS.pTG04PostChoice = 3\n\
         EndFunction\n";
    let (script, errs) = parse_script(SOURCE).expect("parses");
    assert!(errs.is_empty(), "{errs:?}");
    let without = crate::fragment::quest_property_names(&script);
    assert!(without.is_empty());
    let with = crate::fragment::quest_property_names_with_library(&script, Some(&library));
    assert_eq!(with, ["ptg04pqs".to_owned()].into_iter().collect());
    let body = first_fn_body(SOURCE);
    assert_eq!(
        lower_fragment_with_script_metadata(&body, &with, &HashSet::new(), &HashSet::new(), None),
        Some(vec![Effect::SetQuestVariable {
            quest: QuestRef::Property("pTG04PQS".into()),
            variable: "pTG04PostChoice".into(),
            value: 3.0,
        }])
    );
    assert_eq!(
        lower_fragment_with_script_metadata(&body, &without, &HashSet::new(), &HashSet::new(), None),
        None
    );
}

/// #5978 — inside an inlined quest script, a variable or property that holds
/// a reference (not a number) must not classify as a numeric quest-variable
/// read: an unknown variable reads 0.0, so `If A == B` over two
/// `ObjectReference` properties compared `0.0 == 0.0` and was always true.
/// The same comparison over `Int` properties still lowers.
#[test]
fn non_numeric_quest_script_variables_do_not_classify_as_numeric_reads() {
    let library = PscLibrary::new(&[
        "ScriptName CheckScript extends Quest\n\
         ObjectReference Property A Auto\n\
         ObjectReference Property B Auto\n\
         Int Property N Auto\n\
         Int Property M Auto\n\
         Function SameRef()\n\
         If A == B\n Self.SetStage(30)\n EndIf\n EndFunction\n\
         Function SameNumber()\n\
         If N == M\n Self.SetStage(30)\n EndIf\n EndFunction\n",
    ]);
    let lowered = |function: &str| {
        let source = format!(
            "ScriptName TIF_Test extends TopicInfo\n\
             Function Fragment_0(ObjectReference akSpeakerRef)\n\
             (Self.GetOwningQuest() as CheckScript).{function}()\n EndFunction\n"
        );
        let (script, errs) = parse_script(&source).expect("parses");
        assert!(errs.is_empty(), "{errs:?}");
        let function = script
            .body
            .iter()
            .find_map(|item| match &item.node {
                ScriptItem::Function(f) => Some(f.clone()),
                _ => None,
            })
            .expect("a function");
        lower_info_fragment(
            &function.params,
            &function.body,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
            None,
            Some(&library),
        )
    };
    assert_eq!(lowered("SameRef"), None, "two ObjectReference properties are not numbers");
    assert!(lowered("SameNumber").is_some(), "the control: the Int comparison lowers");

    // A decompiled `.pex` reads a property through its `::Name_var` backing
    // variable; a non-numeric property's backing name declines the same way.
    let ident = |name: &str| Expr::Ident(byroredux_papyrus::ast::Identifier(name.into()));
    let mut scope = Scope { vmad_is_quest: true, ..Scope::default() };
    assert!(
        classify_value(&ident("::A_var"), &scope).is_some(),
        "an unclassified backing name reads as a quest variable"
    );
    scope.non_numeric_script_variables.insert("::a_var".into());
    assert_eq!(classify_value(&ident("::A_var"), &scope), None);
}

/// M47.2 — arguments to an inlined quest-script function: the speaker
/// binds the parameter to the speaker, a fragment property rides the
/// quest scope's bindings, and a number becomes a run-time local set in
/// the caller's context.
#[test]
fn inlined_quest_script_calls_bind_their_arguments() {
    let library = PscLibrary::new(&[
        "ScriptName SpouseScript extends Quest\n\
         Function MoveSpouse(Actor Spouse, ReferenceAlias NewHouse, Int Stage)\n\
         Spouse.EvaluatePackage()\n NewHouse.TryToEvaluatePackage()\n\
         If Stage > 10\n Self.SetStage(30)\n EndIf\n EndFunction\n",
    ]);
    let source = "ScriptName TIF_Test extends TopicInfo\n\
         ReferenceAlias Property SolitudeHouse Auto\n\
         Function Fragment_0(ObjectReference akSpeakerRef)\n\
         Actor akSpeaker = akSpeakerRef as Actor\n\
         (Self.GetOwningQuest() as SpouseScript).MoveSpouse(akSpeaker, SolitudeHouse, 20)\n EndFunction\n";
    let (script, errs) = parse_script(source).expect("parses");
    assert!(errs.is_empty(), "{errs:?}");
    let function = script
        .body
        .iter()
        .find_map(|item| match &item.node {
            ScriptItem::Function(f) => Some(f.clone()),
            _ => None,
        })
        .expect("a function");
    let lowered = lower_info_fragment(
        &function.params,
        &function.body,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        None,
        Some(&library),
    )
    .expect("lowers");
    let [Effect::SetLocal { local, value }, Effect::QuestScope {
        quest,
        effects,
        bindings,
    }] = lowered.as_slice()
    else {
        panic!("{lowered:?}");
    };
    assert_eq!(local, "spousescript.movespouse.stage");
    assert_eq!(value, &ValueExpr::Literal(20.0));
    assert_eq!(quest, &QuestRef::OwningQuest);
    assert_eq!(
        bindings,
        &vec![(
            "inline.spousescript.movespouse.newhouse".to_owned(),
            "SolitudeHouse".to_owned()
        )]
    );
    let [Effect::EvaluatePackage {
        actor: ActorRef::Speaker,
    }, Effect::EvaluatePackage {
        actor: ActorRef::Object(house),
    }, Effect::Conditional { guards, .. }] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert_eq!(
        house,
        &ObjectRef::Property("inline.spousescript.movespouse.newhouse".into())
    );
    assert!(matches!(
        guards.as_slice(),
        [FragmentGuard::Compare { left: ValueExpr::Local(local), .. }]
            if local == "spousescript.movespouse.stage"
    ), "{guards:?}");
}

/// #5970 — inlining re-lowers a callee at every call site, so F1 calling F2
/// N times, F2 calling F3 N times, … expanded to N^depth effects with no cap
/// (a mod `.pex` exhausts memory at cell load). The shared statement budget
/// declines such a fragment; an ordinary fan-out still lowers.
#[test]
fn multiplicative_inlining_declines_past_the_statement_budget() {
    fn chain(fan_out: usize) -> PscLibrary {
        let calls = |next: &str| format!("Self.{next}()\n").repeat(fan_out);
        let source = format!(
            "ScriptName ChainScript extends Quest\n\
             Function F1()\n{}EndFunction\n\
             Function F2()\n{}EndFunction\n\
             Function F3()\n{}EndFunction\n\
             Function F4()\n Self.SetStage(10)\nEndFunction\n",
            calls("F2"),
            calls("F3"),
            calls("F4"),
        );
        PscLibrary::new(&[source.as_str()])
    }
    fn lower_with(library: &PscLibrary) -> Option<Vec<Effect>> {
        let source = "ScriptName TIF_Chain extends TopicInfo\n\
             Function Fragment_0(ObjectReference akSpeakerRef)\n\
             (Self.GetOwningQuest() as ChainScript).F1()\nEndFunction\n";
        let (script, errs) = parse_script(source).expect("parses");
        assert!(errs.is_empty(), "{errs:?}");
        let function = script
            .body
            .iter()
            .find_map(|item| match &item.node {
                ScriptItem::Function(f) => Some(f.clone()),
                _ => None,
            })
            .expect("a function");
        lower_info_fragment(
            &function.params,
            &function.body,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
            None,
            Some(library),
        )
    }
    assert!(lower_with(&chain(2)).is_some(), "a small fan-out still lowers");
    assert_eq!(lower_with(&chain(30)), None, "30^3 leaf effects blow the budget and decline");
}

/// M47.2 — an INFO fragment's call into its quest's script inlines as a
/// `QuestScope` (the callee's properties live on the quest's VMAD), and so
/// does a statement reading a quest-script property (`kmyQuest.CW`); a
/// latent callee declines.
#[test]
fn info_fragment_calls_into_the_quest_script_lower_as_quest_scopes() {
    let library = PscLibrary::new(&[
        "ScriptName RadioScript extends Quest\n\
         ObjectReference Property RadioMarker Auto\n\
         CWScript Property CW Auto\n\
         Function UpdateRadio()\n RadioMarker.Enable()\n Self.SetStage(10)\n EndFunction\n\
         Function SlowUpdate()\n Utility.Wait(1.0)\n Self.SetStage(20)\n EndFunction\n\
         Int SongCount\nInt CurrentSong\nInt LastSong01\n\
         Function Rotate()\n SongCount = SongCount + 1\n LastSong01 = CurrentSong\n CurrentSong = 0\n EndFunction\n",
        "ScriptName CWScript extends Quest\n",
    ]);
    let lowered = |statement: &str| {
        let source = format!(
            "ScriptName TIF_Test extends TopicInfo\n\
             Function Fragment_0(ObjectReference akSpeakerRef)\n\
             RadioScript kmyQuest = Self.GetOwningQuest() as RadioScript\n\
             {statement}\n EndFunction\n"
        );
        let (script, errs) = parse_script(&source).expect("parses");
        assert!(errs.is_empty(), "{errs:?}");
        let function = script
            .body
            .iter()
            .find_map(|item| match &item.node {
                ScriptItem::Function(f) => Some(f.clone()),
                _ => None,
            })
            .expect("a function");
        lower_info_fragment(
            &function.params,
            &function.body,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
            None,
            Some(&library),
        )
    };
    assert_eq!(
        lowered("kmyQuest.UpdateRadio()"),
        Some(vec![Effect::QuestScope {
            bindings: Vec::new(),
            quest: QuestRef::OwningQuest,
            effects: vec![
                Effect::Enable {
                    object: ObjectRef::Property("RadioMarker".into()),
                    fade_in: false,
                },
                Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 10,
                },
            ],
        }])
    );
    assert_eq!(lowered("kmyQuest.SlowUpdate()"), None, "latent callee");
    // The speaker is VMAD-free; a fragment property beside a quest
    // property would resolve against the wrong VMAD, so it declines.
    assert_eq!(
        lowered("kmyQuest.RadioMarker.Disable()"),
        Some(vec![Effect::QuestScope {
            bindings: Vec::new(),
            quest: QuestRef::OwningQuest,
            effects: vec![Effect::Disable {
                object: ObjectRef::Property("RadioMarker".into()),
                fade_out: false,
            }],
        }])
    );
    assert_eq!(
        lowered("FragmentDoor.MoveTo(kmyQuest.RadioMarker)"),
        None,
        "a fragment property beside a quest property"
    );
    // The FO4 radio shape: the quest script's own variables.
    assert_eq!(
        lowered("kmyQuest.Rotate()"),
        Some(vec![Effect::QuestScope {
            bindings: Vec::new(),
            quest: QuestRef::OwningQuest,
            effects: vec![
                Effect::ModQuestVariable {
                    quest: QuestRef::SelfRef,
                    variable: "SongCount".into(),
                    delta: 1.0,
                },
                Effect::CopyQuestVariable {
                    quest: QuestRef::SelfRef,
                    from: "CurrentSong".into(),
                    to: "LastSong01".into(),
                },
                Effect::SetQuestVariable {
                    quest: QuestRef::SelfRef,
                    variable: "CurrentSong".into(),
                    value: 0.0,
                },
            ],
        }])
    );
    // A quest-script property read from the INFO resolves through the
    // quest's VMAD: the statement is scoped.
    assert_eq!(
        lowered("kmyQuest.CW.SetStage(2)"),
        Some(vec![Effect::QuestScope {
            bindings: Vec::new(),
            quest: QuestRef::OwningQuest,
            effects: vec![Effect::SetStage {
                quest: QuestRef::Property("CW".into()),
                stage: 2,
            }],
        }])
    );
}

/// M47.2 — a quest fragment's write to its own script variable
/// (`::SudiFound_var = True`, `::chestCount_var += 1` as decompiled) is a
/// quest script variable; without a quest VMAD it still declines.
#[test]
fn quest_fragment_script_variable_writes_lower() {
    use byroredux_papyrus::ast::AssignOp;
    let var = |name: &str| sp(Expr::Ident(Identifier(name.into())));
    let body = vec![
        sp(Stmt::Assign {
            target: var("::SudiFound_var"),
            op: AssignOp::Eq,
            value: sp(Expr::BoolLit(true)),
        }),
        sp(Stmt::Assign {
            target: var("::chestCount_var"),
            op: AssignOp::Eq,
            value: sp(Expr::BinaryOp {
                left: Box::new(var("::chestCount_var")),
                op: BinaryOp::Add,
                right: Box::new(sp(Expr::IntLit(1))),
            }),
        }),
    ];
    let library = PscLibrary::new(&[]);
    assert_eq!(
        lower_quest_fragment(&body, &HashSet::new(), &HashSet::new(), &HashSet::new(), None, Some(&library)),
        Some(vec![
            Effect::SetQuestVariable {
                quest: QuestRef::SelfRef,
                variable: "::SudiFound_var".into(),
                value: 1.0,
            },
            Effect::ModQuestVariable {
                quest: QuestRef::SelfRef,
                variable: "::chestCount_var".into(),
                delta: 1.0,
            },
        ])
    );
    assert_eq!(lower_fragment(&body), None, "no quest VMAD");
}

/// M47.2 — a reference script's variable, written through a cast to its
/// script type; a quest-script cast, an undeclared variable and an
/// untyped receiver decline.
#[test]
fn reference_script_variable_writes_lower_through_a_typed_cast() {
    let library = PscLibrary::new(&[
        "ScriptName MiraakBattleBase extends Actor\nBool fireMiraakKill\n",
        "ScriptName MiraakBattleScript extends MiraakBattleBase\n",
        "ScriptName SomeQuestScript extends Quest\nBool fireMiraakKill\n",
    ]);
    let lowered = |statement: &str| {
        let body = first_fn_body(&format!(
            "ScriptName QF_Test extends Quest\nFunction Fragment_1()\n{statement}\nEndFunction\n"
        ));
        lower_quest_fragment(&body, &HashSet::new(), &HashSet::new(), &HashSet::new(), None, Some(&library))
    };
    assert_eq!(
        lowered("(Alias_Miraak.GetReference() as MiraakBattleScript).fireMiraakKill = True"),
        Some(vec![Effect::SetReferenceVariable {
            object: ObjectRef::Property("Alias_Miraak".into()),
            variable: "::fireMiraakKill_var".into(),
            value: 1.0,
        }])
    );
    assert_eq!(lowered("(Alias_Miraak.GetReference() as MiraakBattleScript).Missing = True"), None);
    assert_eq!(lowered("(Alias_Miraak.GetReference() as SomeQuestScript).fireMiraakKill = True"), None);
    assert_eq!(lowered("SomeProperty.fireMiraakKill = True"), None);
}

/// M47.2 — a quest reached through the fragment quest's own typed
/// property (`kmyQuest.CW`), and a property cast to a quest script,
/// lower as quest properties on the quest path (the library supplies the
/// types); a non-quest property type declines.
#[test]
fn owner_quest_properties_and_quest_script_casts_lower_with_the_library() {
    let library = PscLibrary::new(&[
        "ScriptName CWQuickstartScript extends Quest\n\
         CWScript Property CW Auto\n\
         ObjectReference Property Door Auto\n",
        "ScriptName CWScript extends Quest\nInt Property PlayerGotIntro Auto\n",
        "ScriptName MS13QuestScript extends Quest\nBool Property CamillaEscort Auto\n",
    ]);
    let lowered = |statements: &str| {
        let body = first_fn_body(&format!(
            "ScriptName QF_Test extends Quest\n\
             Quest Property MS13 Auto\n\
             Function Fragment_2()\n\
             CWQuickstartScript kmyQuest = Self as CWQuickstartScript\n\
             {statements}\n EndFunction\n"
        ));
        lower_quest_fragment(
            &body,
            &["ms13".to_owned()].into_iter().collect(),
            &HashSet::new(),
            &HashSet::new(),
            None,
            Some(&library),
        )
    };
    assert_eq!(
        lowered("kmyQuest.CW.PlayerGotIntro = 1\n kmyQuest.CW.SetStage(2)\n (MS13 as MS13QuestScript).CamillaEscort = True"),
        Some(vec![
            Effect::SetQuestVariable {
                quest: QuestRef::Property("CW".into()),
                variable: "PlayerGotIntro".into(),
                value: 1.0,
            },
            Effect::SetStage {
                quest: QuestRef::Property("CW".into()),
                stage: 2,
            },
            Effect::SetQuestVariable {
                quest: QuestRef::Property("MS13".into()),
                variable: "CamillaEscort".into(),
                value: 1.0,
            },
        ])
    );
    assert_eq!(lowered("kmyQuest.Door.SetStage(2)"), None, "not a quest");
    // An object property of the quest script is an object receiver; a
    // quest-typed one is not.
    assert_eq!(
        lowered("kmyQuest.Door.Disable()"),
        Some(vec![Effect::Disable {
            object: ObjectRef::Property("Door".into()),
            fade_out: false,
        }])
    );
    assert_eq!(lowered("kmyQuest.CW.Disable()"), None, "a quest is no reference");
}

const MOTHPRIEST_SCRIPT: &str = "ScriptName DLC1VQ03MothpriestFightScript extends Quest\n\
    ReferenceAlias Property Dexion Auto\n\
    Spell Property DLC1AbMothpriestThrallFX1 Auto\n\
    Function StartUp()\n\
    Dexion.GetActorReference().AddSpell(DLC1AbMothpriestThrallFX1, false)\n\
    Self.Finish()\n\
    EndFunction\n\
    Function Finish()\n\
    Self.SetStage(99)\n\
    EndFunction\n\
    Function Loops()\n\
    Self.Loops()\n\
    EndFunction\n\
    Function WithArgs(Int n)\n\
    Self.SetStage(1)\n\
    EndFunction\n";

/// M47.2 — a quest fragment's call into its quest's own script inlines
/// the callee's body (transitively, `Self.Finish()` included), in the
/// quest's scope.
#[test]
fn quest_fragments_inline_calls_into_the_quest_script() {
    let library = PscLibrary::new(&[MOTHPRIEST_SCRIPT]);
    let body = first_fn_body(
        "ScriptName QF_DLC1VQ03 extends Quest\n\
         Function Fragment_25()\n\
         Quest __temp\n\
         DLC1VQ03MothpriestFightScript kmyQuest\n\
         __temp = Self as Quest\n\
         kmyQuest = __temp as DLC1VQ03MothpriestFightScript\n\
         Self.SetObjectiveCompleted(41)\n\
         kmyQuest.StartUp()\n EndFunction\n",
    );
    let lowered = lower_quest_fragment(&body, &HashSet::new(), &HashSet::new(), &HashSet::new(), None, Some(&library));
    assert_eq!(
        lowered,
        Some(vec![
            Effect::SetObjectiveCompleted {
                quest: QuestRef::SelfRef,
                objective: 41,
                completed: true,
            },
            Effect::AddSpell {
                actor: ActorRef::Object(ObjectRef::Property("Dexion".into())),
                spell: ObjectRef::Property("DLC1AbMothpriestThrallFX1".into()),
            },
            Effect::SetStage { quest: QuestRef::SelfRef, stage: 99 },
        ])
    );
    // Without a library (and on every non-quest path) the call declines.
    assert_eq!(lower_quest_fragment(&body, &HashSet::new(), &HashSet::new(), &HashSet::new(), None, None), None);
}

/// Recursion, a callee with parameters, an unknown function and a
/// receiver that is another quest all decline.
#[test]
fn quest_script_inlining_declines_what_it_cannot_bound() {
    let library = PscLibrary::new(&[MOTHPRIEST_SCRIPT]);
    for call in ["kmyQuest.Loops()", "kmyQuest.Missing()", "OtherQuest.StartUp()"] {
        let body = first_fn_body(&format!(
            "ScriptName QF_DLC1VQ03 extends Quest\n\
             Function Fragment_25()\n\
             DLC1VQ03MothpriestFightScript kmyQuest = Self as DLC1VQ03MothpriestFightScript\n\
             {call}\n EndFunction\n"
        ));
        assert_eq!(
            lower_quest_fragment(&body, &HashSet::new(), &HashSet::new(), &HashSet::new(), None, Some(&library)),
            None,
            "{call}"
        );
    }
}

#[test]
fn add_item_declines_with_non_literal_silent_arg() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_9()\n\
         Bool bQuiet = true\n\
         SomeContainer.AddItem(SomeItem, 5, bQuiet)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

#[test]
fn add_item_declines_on_local_receiver() {
    // A local copy of a property is not tracked back to its name in
    // this increment — the receiver must be a bare property.
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_10()\n\
         ObjectReference k = SomeContainer\n\
         k.AddItem(SomeItem, 5)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

/// CK *IsStopped - Quest*: "completely stopped" is the opposite of running.
#[test]
fn is_stopped_is_the_negation_of_running() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_11()\n\
         If Self.IsStopped()\n\
           Self.SetStage(20)\n\
         EndIf\n EndFunction\n",
    );
    let effects = lower_fragment(&body).expect("lowers");
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Conditional { guards, .. }]
                if matches!(guards.as_slice(), [FragmentGuard::QuestRunning { running: false, .. }])
        ),
        "{effects:?}"
    );
}

#[test]
fn lowers_set_loc_ref_type() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_11()\n\
         SomeRef.SetLocRefType(SomeLoc, BossType)\n\
         SomeRef.SetLocRefType(SomeLoc, None)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::SetLocRefType {
                object: ObjectRef::Property("SomeRef".into()),
                ref_type: Some(ObjectRef::Property("BossType".into())),
            },
            Effect::SetLocRefType {
                object: ObjectRef::Property("SomeRef".into()),
                ref_type: None,
            },
        ])
    );
}

#[test]
fn lowers_move_to_two_arg() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_11()\n\
         SomeRef.MoveTo(SomeMarker)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::MoveTo {
            moved: ObjectRef::Property("SomeRef".into()),
            destination: ObjectRef::Property("SomeMarker".into()),
            offset: [0.0; 3],
            match_rotation: true,
        }])
    );
}

/// M47.2 — the player's `MoveTo` (a teleport) lowers to its own effect;
/// the default-offsets rule still applies.
#[test]
fn lowers_the_players_move_to() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_11()\n\
         Game.GetPlayer().MoveTo(SomeMarker, 0.0, 0.0, 0.0, true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::MovePlayerTo {
            destination: ObjectRef::Property("SomeMarker".into()),
        }])
    );
    let offset = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_11()\n\
         Game.GetPlayer().MoveTo(SomeMarker, 0.0, 0.0, 64.0, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&offset), None);
}

#[test]
fn lowers_disable_with_optional_fade_out() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_12()\n\
         SomeMarker.Disable(false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::Disable {
            object: ObjectRef::Property("SomeMarker".into()),
            fade_out: false,
        }])
    );
}

/// #3489 — mirrors `lowers_disable_with_optional_fade_out`: `Enable()` is
/// the more common of the pair in the real corpus (3,005 vs 2,587 calls)
/// and had no primitive at all before this fix, so every fragment
/// containing one declined in full.
#[test]
fn lowers_enable_with_optional_fade_in() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_12()\n\
         SomeMarker.Enable(false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::Enable {
            object: ObjectRef::Property("SomeMarker".into()),
            fade_in: false,
        }])
    );
}

/// CK *MoveTo - ObjectReference*: the offsets and `abMatchRotation` are
/// part of the instruction, and lower with it.
#[test]
fn move_to_carries_its_offset_and_rotation_flag() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_12()\n\
         SomeRef.MoveTo(SomeMarker, 1.0, -2.0, 10.0, false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::MoveTo {
            moved: ObjectRef::Property("SomeRef".into()),
            destination: ObjectRef::Property("SomeMarker".into()),
            offset: [1.0, -2.0, 10.0],
            match_rotation: false,
        }])
    );
    // The player's teleport carries neither, so a real one declines.
    let player = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_12()\n\
         Game.GetPlayer().MoveTo(SomeMarker, 0.0, 0.0, 10.0, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&player), None);
}

/// Regression for #3487. The Papyrus compiler emits every parameter
/// of a call, including omitted ones left at their declared default,
/// and quest fragments only ever reach the effect table through the
/// `.pex` frontend. The two shapes here are what
/// `SomeRef.MoveTo(SomeMarker)` compiles to on Skyrim/FO4 (five args)
/// and on Starfield (six, with `abRotateOffset`); between them they
/// are 3,253 of the corpus's 3,334 `MoveTo` calls, every one of which
/// the old `args.len() == 1` guard declined.
#[test]
fn lowers_the_default_materialized_move_to_shapes() {
    let expected = Some(vec![Effect::MoveTo {
        moved: ObjectRef::Property("SomeRef".into()),
        destination: ObjectRef::Property("SomeMarker".into()),
        offset: [0.0; 3],
        match_rotation: true,
    }]);

    // Skyrim / Fallout 4: akTarget + 3 offsets + abMatchRotation.
    let skyrim = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_12()\n\
         SomeRef.MoveTo(SomeMarker, 0.0, 0.0, 0.0, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&skyrim), expected);

    // Starfield: the same plus `abRotateOffset = false`.
    let starfield = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_12()\n\
         SomeRef.MoveTo(SomeMarker, 0.0, 0.0, 0.0, true, false)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&starfield), expected);
}

/// #3487 — the acceptance above is exactly "every trailing argument
/// is a literal at its Papyrus default", not "ignore the tail".
#[test]
fn move_to_declines_rotate_offset_variables_and_over_long_tails() {
    // Starfield's `abRotateOffset = true` is not modelled.
    let rotate_offset = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_12()\n\
         SomeRef.MoveTo(SomeMarker, 0.0, 0.0, 0.0, true, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&rotate_offset), None);

    // A non-literal offset: the runtime value is unknown, so the
    // default cannot be assumed.
    let variable_offset = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_12()\n\
         float dz = 4.0\n\
         SomeRef.MoveTo(SomeMarker, 0.0, 0.0, dz, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&variable_offset), None);

    // Longer than any shipped game's signature.
    let too_long = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_12()\n\
         SomeRef.MoveTo(SomeMarker, 0.0, 0.0, 0.0, true, false, false)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&too_long), None);
}

#[test]
fn lowers_scene_start_and_stop_requests() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_13()\n\
         IntroScene.Start()\n\
         OldScene.Stop()\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::StartScene {
                scene: ObjectRef::Property("IntroScene".into()),
            },
            Effect::StopScene {
                scene: ObjectRef::Property("OldScene".into()),
            },
        ])
    );
}

/// Regression for #2538 / SCR-D5-NEW10-01. `Quest Property
/// MQ101 Auto` called with `.Start()` shares the identical bare-
/// identifier `.Start()` AST shape `lowers_scene_start_and_stop_requests`
/// above pins for a genuine scene property — nothing in the AST
/// distinguishes them. `lower_fragment` (no property-type context —
/// the state every call site had pre-#2538) cannot tell them apart
/// and reproduces the original bug's exact symptom: this assertion
/// pins that documented limitation of the context-free path, not a
/// desired outcome — it's why `lower_fragment_with_quest_properties`
/// (below) exists. `populate_quest_fragments_from_script` (the real
/// production caller) always supplies context, so this path is
/// exercised by tests only.
#[test]
fn quest_start_on_a_direct_property_declines_rather_than_mislowering_to_scene() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Quest Property MQ101 Auto\n\
         Function Fragment_99()\n\
         MQ101.Start()\n EndFunction\n",
    );
    // Without quest-property context, the ambiguity genuinely can't
    // be resolved from the AST alone — this is the same "silently
    // becomes StartScene" outcome #2538 reported, still reachable
    // whenever no context is available. Pinned here so a future
    // change to the context-free fallback is a deliberate choice,
    // not an accident.
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::StartScene {
            scene: ObjectRef::Property("MQ101".into()),
        }])
    );

    // With quest-property context (the real `populate_quest_fragments_
    // from_script` call path), the ambiguity is resolved *positively*:
    // the receiver is a known `Quest` property, so it lowers to
    // `StartQuest` — not to `StartScene` (the #2538 bug) and not to a
    // whole-fragment decline (#2538's original fix, which discarded
    // every sibling effect to avoid guessing).
    let quest_properties: HashSet<String> = ["mq101".to_string()].into_iter().collect();
    assert_eq!(
        lower_fragment_with_quest_properties(&body, &quest_properties),
        Some(vec![Effect::StartQuest {
            quest: QuestRef::Property("MQ101".into()),
        }]),
        "a known Quest Property called with .Start() must lower to StartQuest"
    );

    // Same statement as the decompiler actually emits it — an
    // auto-property read arrives as its backing variable `::MQ101_var`,
    // which the `.psc` Ident regex cannot express, so this shape has to
    // be built by hand (#2657). It must resolve identically.
    let decompiled = Expr::Call {
        callee: Box::new(sp(Expr::MemberAccess {
            object: Box::new(sp(Expr::Ident(Identifier("::MQ101_var".into())))),
            member: sp(Identifier("Start".into())),
        })),
        args: vec![],
    };
    assert_eq!(
        classify_effect(
            &decompiled,
            &Scope {
                known_quest_properties: quest_properties.clone(),
                ..Scope::default()
            }
        ),
        Some(Effect::StartQuest {
            quest: QuestRef::Property("::MQ101_var".into()),
        }),
        "the backing-variable form the decompiler emits must resolve too"
    );
}

/// SCR-D5-NEW11-02 (#2653) — `Reset` is declared on `Quest`,
/// `ObjectReference` and `Cell`; `SetActive` on `Quest` and `Weather`.
/// The zero-arg AST shape is identical, so a bare non-Quest property
/// receiver must decline rather than be claimed as a quest effect
/// (which would also claim every sibling effect in the fragment).
#[test]
fn reset_and_set_active_decline_on_a_non_quest_property_receiver() {
    let reset = first_fn_body(
        "ScriptName QF extends Quest\n\
         ObjectReference Property MyContainer Auto\n\
         Function Fragment_99()\n\
         MyContainer.Reset()\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&reset),
        None,
        "ObjectReference.Reset() must not be claimed as ResetQuest"
    );

    let set_active = first_fn_body(
        "ScriptName QF extends Quest\n\
         Weather Property SomeWeather Auto\n\
         Function Fragment_99()\n\
         SomeWeather.SetActive(false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&set_active),
        None,
        "Weather.SetActive() must not be claimed as SetQuestActive"
    );
}

/// The narrowing must not cost the genuine cases: a real `Quest`
/// property, `Self`, and `GetOwningQuest()` all still lower.
#[test]
fn reset_and_set_active_still_lower_on_an_unambiguous_quest_receiver() {
    let quest_properties: HashSet<String> = ["mq101".to_string()].into_iter().collect();

    let reset = first_fn_body(
        "ScriptName QF extends Quest\n\
         Quest Property MQ101 Auto\n\
         Function Fragment_99()\n\
         MQ101.Reset()\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment_with_quest_properties(&reset, &quest_properties),
        Some(vec![Effect::ResetQuest {
            quest: QuestRef::Property("MQ101".into()),
        }])
    );

    let self_active = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_99()\n\
         Self.SetActive(true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&self_active),
        Some(vec![Effect::SetQuestActive {
            quest: QuestRef::SelfRef,
            active: true,
        }])
    );
}

/// Companion: a genuinely scene-typed property with the same context
/// present (but not naming the scene property) must still lower
/// correctly — the fix must not make `prim_start_scene` overly broad
/// in its refusal.
#[test]
fn scene_start_still_lowers_when_quest_property_context_is_present_but_unrelated() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Quest Property MQ101 Auto\n\
         Function Fragment_99()\n\
         IntroScene.Start()\n EndFunction\n",
    );
    let quest_properties: HashSet<String> = ["mq101".to_string()].into_iter().collect();
    assert_eq!(
        lower_fragment_with_quest_properties(&body, &quest_properties),
        Some(vec![Effect::StartScene {
            scene: ObjectRef::Property("IntroScene".into()),
        }])
    );
}

// ---- #3159: Lock / SetLockLevel ----

/// The motivating shape. Before this primitive existed the whole
/// fragment declined on `Lock(false)` — `lower_fragment`'s
/// `_ => return None` arm is all-or-nothing — so the objective and the
/// stage advance were discarded along with the unlock, and the failure
/// presented to the player as "the quest stalled" rather than "the door
/// is locked".
#[test]
fn an_unlock_no_longer_takes_its_sibling_effects_down_with_it() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_7()\n\
         MyDoor.Lock(false)\n\
         Self.SetObjectiveCompleted(10, true)\n\
         Self.SetStage(20)\n EndFunction\n",
    );
    let lowered = lower_fragment(&body).expect("the fragment must lower");
    assert_eq!(
        lowered.len(),
        3,
        "all three effects must survive, not just the two that were \
         already modeled: {lowered:?}"
    );
    assert_eq!(
        lowered[0],
        Effect::SetLocked {
            target: ObjectRef::Property("MyDoor".into()),
            locked: false,
        }
    );
}

/// Papyrus defaults `abLock` to `true`, so a bare `Lock()` locks.
#[test]
fn a_bare_lock_call_defaults_to_locking() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_7()\n\
         MyDoor.Lock()\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::SetLocked {
            target: ObjectRef::Property("MyDoor".into()),
            locked: true,
        }])
    );
}

/// #2289 — the decline half. A non-literal lock state, a non-literal
/// `abAsOwner`/`abTraverseLinkedLockables`, and an argument past the
/// declared three (Starfield's dialect) must all decline rather than
/// be guessed at, matching `prim_set_open`'s conservative shape.
/// (#5653 narrowed the over-long leg from two arguments to three:
/// `Lock` declares two parameters, and the compiler emits the
/// declared arity, so the old bound at one never matched a compiled
/// call. The Starfield dialect then added a third declared parameter —
/// see `lock_lowers_at_the_starfield_three_parameter_arity` — moving
/// the over-long bound to four.)
#[test]
fn lock_declines_on_a_non_literal_or_over_long_argument() {
    let computed = first_fn_body(
        "ScriptName QF extends Quest\n\
         Bool Property ShouldLock Auto\n\
         Function Fragment_7()\n\
         MyDoor.Lock(ShouldLock)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&computed),
        None,
        "a variable-driven lock state must decline, not be assumed"
    );

    let computed_owner = first_fn_body(
        "ScriptName QF extends Quest\n\
         Bool Property ShouldFlag Auto\n\
         Function Fragment_7()\n\
         MyDoor.Lock(true, ShouldFlag)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&computed_owner),
        None,
        "a variable-driven abAsOwner must decline, not be assumed"
    );

    let computed_traverse = first_fn_body(
        "ScriptName QF extends Quest\n\
         Bool Property ShouldTraverse Auto\n\
         Function Fragment_7()\n\
         MyDoor.Lock(true, false, ShouldTraverse)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&computed_traverse),
        None,
        "a variable-driven abTraverseLinkedLockables must decline, not be assumed"
    );

    let extra = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_7()\n\
         MyDoor.Lock(true, false, true, true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&extra),
        None,
        "an argument past the three the widest Lock dialect declares must decline"
    );
}

/// #5653 — the compiled shape lowers: the CK compiler materializes the
/// declared arity, so real content's `Lock(abLock, abAsOwner)` carries
/// two arguments (the wiki's own example is `Lock(false, true)` —
/// unlock, flagging the adjoining cell public). `abAsOwner`'s
/// cell-privacy side effect is unmodeled and parsed-not-applied.
#[test]
fn lock_lowers_at_its_declared_two_parameter_arity() {
    let wiki_example = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_7()\n\
         MyDoor.Lock(false, true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&wiki_example),
        Some(vec![Effect::SetLocked {
            target: ObjectRef::Property("MyDoor".into()),
            locked: false,
        }])
    );

    let compiled_default = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_7()\n\
         MyDoor.Lock(true, false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&compiled_default),
        Some(vec![Effect::SetLocked {
            target: ObjectRef::Property("MyDoor".into()),
            locked: true,
        }])
    );
}

/// Starfield's `Lock` dialect declares a third parameter — `Lock(bool
/// abLock, bool abAsOwner, bool abTraverseLinkedLockables = true)`,
/// read off the shipped `ObjectReference` engine-class stub (FO4 and
/// FO76 stubs stop at two) — and its compiler materializes all three,
/// so the near-universal compiled spelling is `Lock(bool, false,
/// true)` (the whole SF `QF_` lock family). The traversal flag
/// qualifies linked lockables, which `SetLocked` does not model for
/// the receiver either way: literal-only, parsed-not-applied, same
/// discipline as `abAsOwner`.
#[test]
fn lock_lowers_at_the_starfield_three_parameter_arity() {
    let compiled = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_7()\n\
         MyDoor.Lock(true, false, true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&compiled),
        Some(vec![Effect::SetLocked {
            target: ObjectRef::Property("MyDoor".into()),
            locked: true,
        }])
    );

    let unlocked = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_7()\n\
         MyDoor.Lock(false, false, true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&unlocked),
        Some(vec![Effect::SetLocked {
            target: ObjectRef::Property("MyDoor".into()),
            locked: false,
        }])
    );
}

/// `SetLockLevel` is a *separate* effect, never a spelling of
/// `SetLocked`: in Papyrus it sets difficulty and does not lock or
/// unlock anything. Folding the two together would make a fragment that
/// merely re-tunes a lock silently lock a door.
#[test]
fn set_lock_level_lowers_to_its_own_effect_not_to_a_lock() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_7()\n\
         MyDoor.SetLockLevel(75)\n EndFunction\n",
    );
    let lowered = lower_fragment(&body).expect("must lower");
    assert_eq!(
        lowered,
        vec![Effect::SetLockLevel {
            target: ObjectRef::Property("MyDoor".into()),
            level: 75,
        }]
    );
    assert!(
        !matches!(lowered[0], Effect::SetLocked { .. }),
        "SetLockLevel must never lower to a lock state change"
    );
}

/// The wire format carries lock level as one byte, so a literal outside
/// `0..=255` is not a difficulty this engine can represent — decline
/// rather than truncate into a *different* difficulty. A bare call
/// declines too: unlike `Lock`, Papyrus gives `SetLockLevel` no default,
/// so there is nothing to assume.
#[test]
fn set_lock_level_declines_on_an_unrepresentable_or_missing_level() {
    for source in [
        "MyDoor.SetLockLevel(256)",
        "MyDoor.SetLockLevel(-1)",
        "MyDoor.SetLockLevel()",
        "MyDoor.SetLockLevel(50, 1)",
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_7()\n\
             {source}\n EndFunction\n"
        ));
        assert_eq!(
            lower_fragment(&body),
            None,
            "`{source}` must decline rather than truncate or assume"
        );
    }
}

#[test]
fn lowers_activate_and_set_open_gate_sequence() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_14()\n\
         KeepGate.Activate(Game.GetPlayer())\n\
         KeepGate.SetOpen()\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::Activate {
                target: ObjectRef::Property("KeepGate".into()),
                activator: None,
                native_only: false,
            },
            Effect::SetOpen {
                target: ObjectRef::Property("KeepGate".into()),
                open: true,
            },
        ])
    );
}

#[test]
fn activate_default_processing_only_lowers_native_only() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_15()\n\
         KeepGate.Activate(Game.GetPlayer(), true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::Activate {
            target: ObjectRef::Property("KeepGate".into()),
            activator: None,
            native_only: true,
        }])
    );
}

#[test]
fn lowers_real_mq101_alias_activate_and_cast_set_open_shape() {
    // Parse the two statements separately: the source parser deliberately
    // discards newlines, so two adjacent chained calls are ambiguous in
    // handwritten PSC. The PEX decompiler supplies two distinct AST
    // statements, which is the production shape this combined body models.
    let mut body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_76()\n\
         Alias_KeepLever1.GetRef().Activate(Alias_Soldier.GetRef(), false)\n EndFunction\n",
    );
    body.extend(first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_76()\n\
         (KeepGate1 as default2stateactivator).SetOpen(true)\n EndFunction\n",
    ));
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::Activate {
                target: ObjectRef::Property("Alias_KeepLever1".into()),
                activator: Some(ObjectRef::Property("Alias_Soldier".into())),
                native_only: false,
            },
            Effect::SetOpen {
                target: ObjectRef::Property("KeepGate1".into()),
                open: true,
            },
        ])
    );
}

#[test]
fn lowers_exact_mq101_player_control_selection() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_191()\n\
         Game.DisablePlayerControls(false, true, true, false, false, false, true, true, 0)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::SetPlayerControls {
            enabled: false,
            selection: PlayerControlSelection {
                movement: false,
                fighting: true,
                camera_switching: true,
                looking: false,
                sneaking: false,
                menu: false,
                activation: true,
                journal_tabs: true,
                pov_type: 0,
            },
        }])
    );
}

/// #5651 — the two `Game.*PlayerControls` calls fill absent arguments
/// from their OWN default tables (CK wiki): `EnablePlayerControls`
/// defaults every domain — including camera/looking/sneaking and the
/// journal tabs — to true, while `DisablePlayerControls` defaults
/// camera/looking/sneaking/journal to false. The single shared
/// `PAPYRUS_DEFAULT` this replaced was a hybrid (Enable's
/// camera/looking/sneaking, Disable's journal tabs) matching neither
/// signature; only `.psc` fragment bodies ever saw the fill-in, so
/// every partial call was mis-translated.
#[test]
fn partial_player_controls_calls_fill_from_their_own_default_tables() {
    let disable = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_1()\n\
         Game.DisablePlayerControls(true, true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&disable),
        Some(vec![Effect::SetPlayerControls {
            enabled: false,
            selection: PlayerControlSelection {
                movement: true,
                fighting: true,
                camera_switching: false,
                looking: false,
                sneaking: false,
                menu: true,
                activation: true,
                journal_tabs: false,
                pov_type: 0,
            },
        }])
    );

    let enable = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_1()\n\
         Game.EnablePlayerControls(true, true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&enable),
        Some(vec![Effect::SetPlayerControls {
            enabled: true,
            selection: PlayerControlSelection {
                movement: true,
                fighting: true,
                camera_switching: true,
                looking: true,
                sneaking: true,
                menu: true,
                activation: true,
                journal_tabs: true,
                pov_type: 0,
            },
        }])
    );
}

#[test]
fn lowers_player_release_and_cinematic_state_calls() {
    let mut body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_316()\n\
         Game.GetPlayer().SetRestrained(false)\n EndFunction\n",
    );
    body.extend(first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_316()\n\
         Game.SetPlayerAIDriven(false)\n EndFunction\n",
    ));
    body.extend(first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_316()\n\
         Game.SetHudCartMode(false)\n EndFunction\n",
    ));
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::SetPlayerRestrained { restrained: false },
            Effect::SetPlayerAiDriven { ai_driven: false },
            Effect::SetHudCartMode { cart_mode: false },
        ])
    );
}

#[test]
fn lowers_mq101_chargen_exit_and_save_request() {
    // Real vanilla shape: `QF_MQ101_0003372B::Fragment_318` (stage 65).
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_318()\n\
         Game.SetInChargen(false, true, true)\n\
         Game.RequestSave()\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::SetInChargen {
                disable_saving: false,
                disable_waiting: true,
                show_controls_disabled_message: true,
            },
            Effect::RequestSave { auto: false },
        ])
    );
}

#[test]
fn lowers_mq101_show_race_menu() {
    // Real vanilla shape: `QF_MQ101_0003372B::Fragment_13` (stage 75).
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_13()\n\
         Game.ShowRaceMenu()\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), Some(vec![Effect::ShowRaceMenu]));
}

#[test]
fn lowers_mq101_chargen_exit_autosave_variant() {
    // Real vanilla shape: `QF_MQ101_0003372B::Fragment_260` (stage 255) —
    // the `false, false, false` cleanup/fallback variant of the same call.
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_260()\n\
         Game.SetInChargen(false, false, false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::SetInChargen {
            disable_saving: false,
            disable_waiting: false,
            show_controls_disabled_message: false,
        }])
    );
}

#[test]
fn set_in_chargen_declines_on_wrong_arg_count() {
    let two_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_28()\n\
         Game.SetInChargen(true, false)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&two_args), None);
}

#[test]
fn show_race_menu_declines_with_args() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_29()\n\
         Game.ShowRaceMenu(true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

#[test]
fn request_save_primitives_decline_with_args() {
    // #4328 — both are 0-arg natives; the `is_empty` arity guard had no
    // decline test of its own.
    for call in ["Game.RequestSave(true)", "Game.RequestAutoSave(1)"] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_31()\n{call}\n EndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), None, "{call}");
    }
}

#[test]
fn only_the_counter_only_primitives_are_placeholders() {
    assert!(Effect::ShowRaceMenu.is_placeholder());
    // #5975 — `Achievements` has no production reader, so the arm only records.
    assert!(Effect::AddAchievement { id: 7 }.is_placeholder());
    assert!(Effect::RequestSave { auto: false }.is_placeholder());
    assert!(Effect::RequestSave { auto: true }.is_placeholder());
    // #4372 — `hud_cart_mode` has no reader, so the arm only records.
    assert!(Effect::SetHudCartMode { cart_mode: true }.is_placeholder());
    // …whereas SetInChargen's `disable_saving` gates the save command.
    assert!(!Effect::SetInChargen {
        disable_saving: true,
        disable_waiting: false,
        show_controls_disabled_message: false,
    }
    .is_placeholder());
}

/// #5975 — the harness check must see a placeholder nested under an `If` /
/// `QuestScope`, and the kind histogram must count the nested effects, not just
/// the wrapper.
#[test]
fn a_nested_placeholder_is_found_and_nested_effects_are_visited() {
    let stage = || Effect::SetStage { quest: QuestRef::SelfRef, stage: 10 };
    let nested_in_if = Effect::Conditional {
        guards: vec![],
        then_effects: vec![stage()],
        else_effects: vec![Effect::ShowRaceMenu],
    };
    let nested_in_scope = Effect::QuestScope {
        quest: QuestRef::OwningQuest,
        effects: vec![Effect::SetHudCartMode { cart_mode: true }],
        bindings: vec![],
    };
    // The flat test is about the one effect; the harness test recurses.
    assert!(!nested_in_if.is_placeholder());
    assert!(nested_in_if.contains_placeholder());
    assert!(!nested_in_scope.is_placeholder());
    assert!(nested_in_scope.contains_placeholder());
    // Two levels down.
    let deep = Effect::Conditional {
        guards: vec![],
        then_effects: vec![nested_in_scope],
        else_effects: vec![],
    };
    assert!(deep.contains_placeholder());
    assert!(!Effect::Conditional { guards: vec![], then_effects: vec![stage()], else_effects: vec![] }
        .contains_placeholder());

    let mut kinds = Vec::new();
    nested_in_if.visit_nested(&mut |effect| kinds.push(effect.kind()));
    assert_eq!(kinds, ["Conditional", "SetStage", "ShowRaceMenu"], "wrapper first, then contents");
}

#[test]
fn lowers_request_auto_save() {
    // Real vanilla shape: `QF_MQ101_0003372B::Fragment_11` (stage 80),
    // minus the trailing `AddRaceSpells()` call — that one is a
    // MQ101QuestScript-authored helper (not a `Game` global) and still
    // declines the whole fragment. Its body (RemoveRaceSpells, then an
    // if/elseif chain over the player's race adding Conjure Familiar /
    // Sparks / Fury) needs cross-script helper inlining, elseif and a
    // player-race guard; AddSpell/RemoveSpell themselves lower (#4415).
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_30()\n\
         Game.RequestAutoSave()\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::RequestSave { auto: true }])
    );
}

#[test]
fn lowers_mq101_set_enemy() {
    // Real vanilla shape: `QF_MQ101_0003372B::Fragment_112`. The
    // decompiler emits the `Faction Property` auto-read as its backing
    // variable (`::MQ101StormcloakFaction_var`), a shape the `.psc`
    // `Ident` regex cannot express (#2657 precedent above), so this is
    // built by hand rather than parsed from source text.
    let expr = Expr::Call {
        callee: Box::new(sp(Expr::MemberAccess {
            object: Box::new(sp(Expr::Ident(Identifier(
                "::MQ101StormcloakFaction_var".into(),
            )))),
            member: sp(Identifier("SetEnemy".into())),
        })),
        args: vec![
            CallArg {
                name: None,
                value: sp(Expr::Ident(Identifier("::PlayerFaction_var".into()))),
            },
            CallArg {
                name: None,
                value: sp(Expr::BoolLit(false)),
            },
            CallArg {
                name: None,
                value: sp(Expr::BoolLit(false)),
            },
        ],
    };
    assert_eq!(
        classify_effect(&expr, &Scope::default()),
        Some(Effect::SetEnemy {
            faction: ObjectRef::Property("::MQ101StormcloakFaction_var".into()),
            other_faction: ObjectRef::Property("::PlayerFaction_var".into()),
            self_neutral: false,
            other_neutral: false,
        })
    );
}

#[test]
fn lowers_mq101_start_combat() {
    // Real vanilla shape: `QF_MQ101_0003372B::Fragment_112`.
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_112()\n\
         Alias_TrophyRoomPrisoner01.GetActorRef().StartCombat(Game.GetPlayer())\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::StartCombat {
            actor: ActorRef::Object(ObjectRef::Property(
                "Alias_TrophyRoomPrisoner01".into()
            )),
            target: ActorRef::Player,
        }])
    );
}

/// #5407 — `AddRaceSpells`/`RemoveRaceSpells` lower ONLY as the
/// unqualified call in a script that itself defines the helper
/// argless (MQ101QuestScript's shape). The member-call spelling and
/// a script that doesn't define the name (another script's
/// same-named helper, or one with parameters) must decline: both
/// used to lower onto the player unconditionally.
#[test]
fn race_spells_lower_only_when_the_script_defines_the_helper() {
    let unqualified = first_fn_body(
        "ScriptName MQ101QuestScript extends Quest\n\
         Function Fragment_11()\n\
         AddRaceSpells()\n\
         RemoveRaceSpells()\n\
         EndFunction\n\
         Function AddRaceSpells()\n\
         EndFunction\n\
         Function RemoveRaceSpells()\n\
         EndFunction\n",
    );
    let own = std::collections::HashSet::from([
        "addracespells".to_string(),
        "removeracespells".to_string(),
    ]);
    assert_eq!(
        lower_fragment_with_script_metadata(&unqualified, &HashSet::new(), &own, &HashSet::new(), None),
        Some(vec![
            Effect::AddRaceSpells {
                actor: ActorRef::Player
            },
            Effect::RemoveRaceSpells {
                actor: ActorRef::Player
            },
        ]),
        "the vanilla shape (unqualified, own-script, argless) lowers"
    );

    // No function table: the same call declines — a script that does
    // not define the helper has no contract to lower by.
    assert_eq!(
        lower_fragment_with_script_metadata(&unqualified, &HashSet::new(), &HashSet::new(), &HashSet::new(), None),
        None,
        "an undefined helper name declines"
    );

    // Member-call spelling: declined even when the script defines
    // the name — `x.AddRaceSpells()` on any object used to lower
    // onto the player.
    let member = first_fn_body(
        "ScriptName MQ101QuestScript extends Quest\n\
         Function Fragment_11()\n\
         SomeAlias.AddRaceSpells()\n\
         EndFunction\n\
         Function AddRaceSpells()\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment_with_script_metadata(&member, &HashSet::new(), &own, &HashSet::new(), None),
        None,
        "a member-call receiver is never accepted"
    );

    // The helper exists but takes parameters: same-named, different
    // contract — decline.
    let parameterized = first_fn_body(
        "ScriptName ModScript extends Quest\n\
         Function Fragment_0()\n\
         AddRaceSpells()\n\
         EndFunction\n\
         Function AddRaceSpells(Bool abLoud)\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment_with_script_metadata(
            &parameterized,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
            None
        ),
        None,
        "a parameterized same-named helper is not the vanilla helper"
    );
}

/// #4415 — `AddSpell` with or without its `abVerbose` flag, and
/// `RemoveSpell`, lower onto the spell-list effects (the shapes
/// `MQ101QuestScript.AddRaceSpells` / `RemoveRaceSpells` use).
#[test]
fn lowers_add_and_remove_spell() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Spell Property Fury Auto\n\
         Function Fragment_9()\n\
         Game.GetPlayer().AddSpell(Fury, true)\n\
         Game.GetPlayer().AddSpell(Fury)\n\
         Game.GetPlayer().RemoveSpell(Fury)\n\
         EndFunction\n",
    );
    let fury = || ObjectRef::Property("Fury".into());
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::AddSpell {
                actor: ActorRef::Player,
                spell: fury(),
            },
            Effect::AddSpell {
                actor: ActorRef::Player,
                spell: fury(),
            },
            Effect::RemoveSpell {
                actor: ActorRef::Player,
                spell: fury(),
            },
        ])
    );
    let variable_flag = first_fn_body(
        "ScriptName QF extends Quest\n\
         Spell Property Fury Auto\n\
         Bool Property bLoud Auto\n\
         Function Fragment_9()\n\
         Game.GetPlayer().AddSpell(Fury, bLoud)\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&variable_flag),
        None,
        "a non-literal flag declines"
    );
}

#[test]
fn set_enemy_defaults_omitted_flags_and_carries_literal_ones() {
    // #4318 / #4414 — each flag makes one direction neutral; an omitted
    // flag is its declared `false` default, and a non-literal flag is an
    // unknown runtime value that declines.
    let call = |flags: Vec<Expr>| Expr::Call {
        callee: Box::new(sp(Expr::MemberAccess {
            object: Box::new(sp(Expr::Ident(Identifier("::SomeFaction_var".into())))),
            member: sp(Identifier("SetEnemy".into())),
        })),
        args: std::iter::once(Expr::Ident(Identifier("::OtherFaction_var".into())))
            .chain(flags)
            .map(|value| CallArg {
                name: None,
                value: sp(value),
            })
            .collect(),
    };
    let lowered = |self_neutral, other_neutral| {
        Some(Effect::SetEnemy {
            faction: ObjectRef::Property("::SomeFaction_var".into()),
            other_faction: ObjectRef::Property("::OtherFaction_var".into()),
            self_neutral,
            other_neutral,
        })
    };
    let scope = Scope::default();
    assert_eq!(
        classify_effect(&call(vec![]), &scope),
        lowered(false, false)
    );
    assert_eq!(
        classify_effect(&call(vec![Expr::BoolLit(true)]), &scope),
        lowered(true, false)
    );
    assert_eq!(
        classify_effect(
            &call(vec![Expr::BoolLit(false), Expr::BoolLit(true)]),
            &scope
        ),
        lowered(false, true)
    );
    assert_eq!(
        classify_effect(
            &call(vec![Expr::BoolLit(true), Expr::BoolLit(true)]),
            &scope
        ),
        lowered(true, true)
    );
    for declined in [
        vec![
            Expr::Ident(Identifier("bNeutral".into())),
            Expr::BoolLit(false),
        ],
        vec![
            Expr::BoolLit(false),
            Expr::BoolLit(false),
            Expr::BoolLit(false),
        ],
    ] {
        let expr = call(declined);
        assert_eq!(classify_effect(&expr, &scope), None, "{expr:?}");
    }
}

/// #4816 — `StopCombat()` on an NPC lowers; with an argument, or on the
/// player, it declines.
#[test]
fn lowers_stop_combat_on_an_npc_only() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_40()\n\
         Alias_TrophyRoomPrisoner01.GetActorRef().StopCombat()\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::StopCombat {
            actor: ActorRef::Object(ObjectRef::Property(
                "Alias_TrophyRoomPrisoner01".into()
            )),
        }])
    );
    for call in [
        "Game.GetPlayer().StopCombat()",
        "Alias_TrophyRoomPrisoner01.GetActorRef().StopCombat(Game.GetPlayer())",
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_41()\n{call}\nEndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), None, "{call}");
    }
}

/// #5017 — `SetUnconscious` on an NPC lowers with its default (`true`)
/// and an explicit `false` (the wake-up call); the player, or a second
/// argument, declines.
#[test]
fn lowers_set_unconscious_on_an_npc_only() {
    let lowered = |call: &str| {
        lower_fragment(&first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_1()\n{call}\nEndFunction\n"
        )))
    };
    let robot = || ActorRef::Object(ObjectRef::Property("Alias_Protectron".into()));
    assert_eq!(
        lowered("Alias_Protectron.GetActorRef().SetUnconscious()"),
        Some(vec![Effect::SetUnconscious {
            actor: robot(),
            unconscious: true,
        }])
    );
    assert_eq!(
        lowered("Alias_Protectron.GetActorRef().SetUnconscious(false)"),
        Some(vec![Effect::SetUnconscious {
            actor: robot(),
            unconscious: false,
        }])
    );
    for call in [
        "Game.GetPlayer().SetUnconscious(true)",
        "Alias_Protectron.GetActorRef().SetUnconscious(false, true)",
    ] {
        assert_eq!(lowered(call), None, "{call}");
    }
}

#[test]
fn start_combat_declines_on_wrong_arg_count() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_32()\n\
         Game.GetPlayer().StartCombat()\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

#[test]
fn start_combat_declines_a_player_combatant() {
    // #4323 — the player target in `lowers_mq101_start_combat` resolves
    // fine, so this declines on the receiver alone: the NPC chase-and-
    // strike runtime must never be armed on the player.
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_33()\n\
         Game.GetPlayer().StartCombat(Game.GetPlayer())\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

#[test]
fn lowers_alias_actor_evaluate_package() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_292()\n\
         Alias_Hadvar.GetActorRef().EvaluatePackage()\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::EvaluatePackage {
            actor: ActorRef::Object(ObjectRef::Property("Alias_Hadvar".into())),
        }])
    );
}

#[test]
fn lowers_mq101_player_idle_and_animation_event_helpers() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_295()\n\
         Quest temp = Self as Quest\n\
         mq101questscript kmyQuest = temp as mq101questscript\n\
         Game.GetPlayer().PlayIdle(IdleExecutionerAlduinReactionDeath)\n\
         kmyQuest.PlayerImodAnimation()\n\
         kmyQuest.PlayerFurnitureAnimation()\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::PlayIdle {
                actor: ActorRef::Player,
                idle: ObjectRef::Property("IdleExecutionerAlduinReactionDeath".into()),
            },
            Effect::RegisterPlayerAnimationEvent {
                event: CinematicAnimationEvent::PlayImod,
            },
            Effect::RegisterPlayerAnimationEvent {
                event: CinematicAnimationEvent::IdleFurnitureExit,
            },
        ])
    );
}

#[test]
fn lowers_mq101_cart_exit_motion_and_sitting_rotation() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_177()\n\
         Quest temp = Self as Quest\n\
         mq101questscript kmyQuest = temp as mq101questscript\n\
         kmyQuest.ExitCart(Alias_Prisoner, 3)\n\
         Alias_Cart.GetRef().SetMotionType(Alias_Cart.GetRef().Motion_Keyframed, true)\n\
         Game.SetSittingRotation(-55.0)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::ExitCart {
                actor: ObjectRef::Property("Alias_Prisoner".into()),
                seat: 3,
            },
            Effect::SetMotionType {
                target: ObjectRef::Property("Alias_Cart".into()),
                motion_type: MotionType::Keyframed,
                allow_activate: true,
            },
            Effect::SetSittingRotation { degrees: -55.0 },
        ])
    );
}

/// #5633 (premise error in #2286): the integer argument of Papyrus
/// `SetMotionType` uses per-game constants (Skyrim Keyframed 4 / Fixed 5
/// / Character 7; Fallout 4 Fixed 0 / Keyframed 2), not the NIF
/// `hkMotionType` table the literal arm used to copy (Keyframed 6, Static
/// 7). Only `1` (Dynamic) is the same in both games; every other literal
/// declines. The named `Motion_*` property read — what vanilla MQ101
/// actually carries — still lowers (see the MQ101 cart test).
#[test]
fn literal_motion_types_lower_only_where_every_game_agrees() {
    let lower = |raw: i64| {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_177()\n\
             Cart.SetMotionType({raw}, true)\n\
             EndFunction\n"
        ));
        lower_fragment(&body)
    };
    assert_eq!(
        lower(1),
        Some(vec![Effect::SetMotionType {
            target: ObjectRef::Property("Cart".into()),
            motion_type: MotionType::Dynamic,
            allow_activate: true,
        }]),
        "1 is Motion_Dynamic in Skyrim and Fallout 4"
    );
    for raw in [0, 2, 3, 4, 5, 6, 7, 8, 9] {
        assert_eq!(
            lower(raw),
            None,
            "literal {raw} means a different motion type per game — decline"
        );
    }
}

#[test]
fn tracks_actor_and_vehicle_locals_across_a_latent_wait() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_175()\n\
         Actor rider = Alias_Rider.GetActorRef()\n\
         ObjectReference cart = Alias_Cart.GetRef()\n\
         rider.SetVehicle(cart)\n\
         Utility.Wait(0.25)\n\
         rider.SetVehicle(None)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::SetVehicle {
                actor: ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                vehicle: Some(ObjectRef::Property("Alias_Cart".into())),
            },
            Effect::Wait { seconds: 0.25 },
            Effect::SetVehicle {
                actor: ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                vehicle: None,
            },
        ])
    );
}

/// #5681 — a wait past the shared cap (or one whose f32 shape is
/// infinite) must decline, not lower into an immortal continuation.
/// The boundary itself still lowers.
#[test]
fn waits_past_the_shared_cap_decline() {
    let at_cap = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_1()\n\
         Utility.Wait(86400.0)\n\
         Self.SetStage(20)\n EndFunction\n",
    );
    assert!(
        lower_fragment(&at_cap).is_some(),
        "exactly MAX_PROVIDER_WAIT_SECONDS is a legal (if long) wait"
    );
    for seconds in ["86401.0", "340282346638528859811704183484516925440.0"] {
        let over = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             Utility.Wait({seconds})\n\
             Self.SetStage(20)\n EndFunction\n"
        ));
        assert_eq!(
            lower_fragment(&over),
            None,
            "Utility.Wait({seconds}) must decline, not park an immortal continuation"
        );
    }
}

#[test]
fn lowers_mq101_cart_load_gate_tether_and_equipment_sequence() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_175()\n\
         Actor player = Game.GetPlayer()\n\
         Actor rider = Alias_Rider.GetActorRef()\n\
         ObjectReference cart = Alias_Cart.GetRef()\n\
         While !player.Is3DLoaded() || !rider.Is3DLoaded()\n\
             Utility.Wait(0.2)\n\
         EndWhile\n\
         cart.TetherToHorse(Alias_Horse.GetActorRef() as ObjectReference)\n\
         rider.EquipItem(ArmorGag as Form, false, true)\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::WaitForActors3DLoaded {
                actors: vec![
                    ActorRef::Player,
                    ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                ],
                poll_seconds: 0.2,
            },
            Effect::TetherToHorse {
                cart: ObjectRef::Property("Alias_Cart".into()),
                horse: ObjectRef::Property("Alias_Horse".into()),
            },
            Effect::EquipItem {
                actor: ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                item: ObjectRef::Property("ArmorGag".into()),
                silent: true,
            },
        ])
    );
}

/// M47.2 — `ElseIf` is the else branch of the clause before it.
#[test]
fn else_if_chains_lower_as_nested_conditionals() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\nFunction Fragment_1()\n\
         If Self.GetStageDone(10)\n Self.SetStage(20)\n\
         ElseIf Self.GetStageDone(5)\n Self.SetStage(15)\n\
         Else\n Self.SetStage(1)\n EndIf\n EndFunction\n",
    );
    let stage = |stage| Effect::SetStage {
        quest: QuestRef::SelfRef,
        stage,
    };
    let done = |stage| FragmentGuard::StageDone {
        quest: QuestRef::SelfRef,
        stage,
        done: true,
    };
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::Conditional {
            guards: vec![done(10)],
            then_effects: vec![stage(20)],
            else_effects: vec![Effect::Conditional {
                guards: vec![done(5)],
                then_effects: vec![stage(15)],
                else_effects: vec![stage(1)],
            }],
        }])
    );
}

/// M47.2 — a `While … Wait` poll on a modelled condition lowers to
/// `WaitWhile`; a loop doing more than waiting still declines.
#[test]
fn declines_unmodeled_loop_and_prevent_unequip() {
    let poll = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_1()\n\
         While Self.GetStage() < 10\n Utility.Wait(0.2)\n EndWhile\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&poll),
        Some(vec![Effect::WaitWhile {
            guards: vec![FragmentGuard::Stage {
                quest: QuestRef::SelfRef,
                comparison: GuardComparison::Lt,
                value: 10.0,
            }],
            poll_seconds: 0.2,
        }])
    );
    let arbitrary_loop = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_1()\n\
         While Self.GetStage() < 10\n Self.SetStage(5)\n Utility.Wait(0.2)\n EndWhile\n\
         EndFunction\n",
    );
    assert_eq!(lower_fragment(&arbitrary_loop), None);

    let locked_item = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_2()\n\
         Alias_Rider.GetActorRef().EquipItem(ArmorGag, true, false)\n\
         EndFunction\n",
    );
    assert_eq!(lower_fragment(&locked_item), None);
}

#[test]
fn lowers_get_stage_done_conditional() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_5()\n\
         If Self.GetStageDone(2) == false && Self.GetStageDone(6) == 0\n\
           Self.SetStage(35)\n\
         Else\n\
           Self.SetStage(7)\n\
         EndIf\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::Conditional {
            guards: vec![
                FragmentGuard::StageDone {
                    quest: QuestRef::SelfRef,
                    stage: 2,
                    done: false,
                },
                FragmentGuard::StageDone {
                    quest: QuestRef::SelfRef,
                    stage: 6,
                    done: false,
                },
            ],
            then_effects: vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 35,
            }],
            else_effects: vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 7,
            }],
        }]),
    );
}

/// #5631: `Return` is Champollion's terminator only as the body's final
/// statement. An early one is control flow — lowering it as nothing
/// applied the rest of the body unconditionally.
#[test]
fn return_lowers_only_as_the_final_statement() {
    let set = |stage| Effect::SetStage { quest: QuestRef::SelfRef, stage };
    let trailing = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         Self.SetStage(10)\n\
         Return\n\
         EndFunction\n",
    );
    assert_eq!(lower_fragment(&trailing), Some(vec![set(10)]));

    let early_in_if = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         If Self.GetStageDone(10)\n\
           Return\n\
         EndIf\n\
         Self.SetStage(20)\n\
         EndFunction\n",
    );
    assert_eq!(lower_fragment(&early_in_if), None, "early return inside an If");

    let mid_body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         Self.SetStage(10)\n\
         Return\n\
         Self.SetStage(20)\n\
         EndFunction\n",
    );
    assert_eq!(lower_fragment(&mid_body), None, "return before more statements");

    let tail_of_a_branch = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         If Self.GetStageDone(10)\n\
           Self.SetStage(20)\n\
           Return\n\
         EndIf\n\
         EndFunction\n",
    );
    assert_eq!(lower_fragment(&tail_of_a_branch), None, "return nested in a branch");
}

/// #5632: the branches lower against clones of the scope, so a local
/// DECLARED inside one stays inside it — but re-binding a local declared
/// before the `If` changes what the name means after `EndIf`, and the
/// clone is dropped. The tail used to lower against the old binding.
#[test]
fn rebinding_a_pre_branch_local_inside_a_branch_declines() {
    let rebinds = first_fn_body(
        "ScriptName QF extends Quest\n\
         Quest Property OtherQ Auto\n\
         Function Fragment_0()\n\
         Quest k = Self\n\
         If Self.GetStageDone(10)\n\
           k = OtherQ\n\
         EndIf\n\
         k.SetStage(20)\n\
         EndFunction\n",
    );
    assert_eq!(lower_fragment(&rebinds), None);

    // Re-binding to the very same target changes nothing after the
    // branch, so it still lowers.
    let same = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         Quest k = Self\n\
         If Self.GetStageDone(10)\n\
           k = Self\n\
         EndIf\n\
         k.SetStage(20)\n\
         EndFunction\n",
    );
    assert!(lower_fragment(&same).is_some());

    // A local declared inside the branch never leaks out, so it is fine.
    let declared_inside = first_fn_body(
        "ScriptName QF extends Quest\n\
         Quest Property OtherQ Auto\n\
         Function Fragment_0()\n\
         If Self.GetStageDone(10)\n\
           Quest j = OtherQ\n\
           j.SetStage(5)\n\
         EndIf\n\
         EndFunction\n",
    );
    assert!(lower_fragment(&declared_inside).is_some());
}

/// #5638: only a local this function declared can be re-bound. A write
/// to a script variable (or property) by bare name is persistent state;
/// it used to be recorded as a side-effect-free local and dropped.
#[test]
fn assignment_to_a_script_variable_declines() {
    let script_var = first_fn_body(
        "ScriptName QF extends Quest\n\
         Bool bFlag\n\
         Function Fragment_0()\n\
         bFlag = True\n\
         Self.SetStage(10)\n\
         EndFunction\n",
    );
    assert_eq!(lower_fragment(&script_var), None);

    let declared_local = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         Int n = 1\n\
         n = 2\n\
         Self.SetStage(10)\n\
         EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&declared_local),
        Some(vec![Effect::SetStage { quest: QuestRef::SelfRef, stage: 10 }])
    );
}

/// #5973 — Skyrim's `DamageActorValue` / `RestoreActorValue` discard the
/// amount's sign (CK: "Negative numbers will be converted to positive"), so
/// `DamageActorValue("Health", -10.0)` is 10 points of damage, not a heal; a
/// run-time amount lowers through `ValueExpr::Abs`. The Fallout 4 spellings
/// keep the amount as written (their CK pages are silent on sign).
#[test]
fn skyrim_damage_and_restore_discard_the_amount_sign() {
    let damage_of = |statement: &str| {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             {statement}\n\
             EndFunction\n"
        ));
        match lower_fragment(&body).expect("lowers").remove(0) {
            Effect::DamageActorValue { amount, restore, .. } => (amount, restore),
            other => panic!("expected DamageActorValue, got {other:?}"),
        }
    };
    // Skyrim, negative literal: folded to the magnitude, still damage.
    assert_eq!(
        damage_of("Game.GetPlayer().DamageActorValue(\"Health\", -10.0)"),
        (ValueExpr::Literal(10.0), false)
    );
    assert_eq!(
        damage_of("Game.GetPlayer().RestoreActorValue(\"Health\", -10.0)"),
        (ValueExpr::Literal(10.0), true)
    );
    assert_eq!(
        damage_of("Game.GetPlayer().DamageAV(\"Health\", 4.0)"),
        (ValueExpr::Literal(4.0), false)
    );
    // Skyrim, run-time amount: its magnitude.
    let (amount, restore) = damage_of("Game.GetPlayer().DamageActorValue(\"Health\", Self.GetStage())");
    assert!(!restore);
    assert!(
        matches!(amount, ValueExpr::Abs(ref inner) if matches!(**inner, ValueExpr::Stage(_))),
        "{amount:?}"
    );
    // Fallout 4 / Starfield spellings: the amount as written.
    assert_eq!(
        damage_of("Game.GetPlayer().DamageValue(HealthAV, -10.0)"),
        (ValueExpr::Literal(-10.0), false)
    );
    assert_eq!(
        damage_of("Game.GetPlayer().RestoreValue(HealthAV, 10.0)"),
        (ValueExpr::Literal(10.0), true)
    );
}

/// #5974 — `ValueExpr` arithmetic is `f32`, so Papyrus `Int / Int` (which
/// truncates) and `SetValueInt(<float>)` (which stores an `Int`) cannot be
/// modelled. Division lowers only when an operand is provably `Float`;
/// `SetValueInt` only when its value is integral by construction.
#[test]
fn integer_division_and_non_integral_int_stores_decline() {
    let lowers = |statement: &str| {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             GlobalVariable Property G Auto\n\
             GlobalVariable Property H Auto\n\
             Function Fragment_0()\n\
             {statement}\n\
             EndFunction\n"
        ));
        lower_fragment(&body).is_some()
    };
    // Integer division: the truncation is lost, so decline.
    assert!(!lowers("G.SetValueInt(H.GetValueInt() / 2)"));
    assert!(!lowers("G.SetValue((Self.GetStage() / 10) as Float)"));
    // A float operand makes it float division, as in Papyrus: lowers.
    assert!(lowers("G.SetValue(H.GetValue() / 2)"));
    assert!(lowers("G.SetValue(H.GetValue() / 2.0)"));
    assert!(lowers("G.SetValue(Self.GetStage() / 2.0)"));
    // SetValueInt stores an Int: integral-by-construction values keep lowering.
    assert!(lowers("G.SetValueInt(5)"));
    assert!(lowers("G.SetValueInt(Self.GetStage() + 1)"));
    assert!(lowers("G.SetValueInt(Self.GetStage() * 2)"));
    // #6215 — `GetValueInt` truncates the stored float, which `ValueExpr` cannot:
    // `Counter.SetValueInt(Clock.GetValueInt())` would store 3.7. Left to the VM.
    assert!(!lowers("G.SetValueInt(H.GetValueInt())"));
    assert!(!lowers("G.SetValueInt(H.GetValueInt() + 1)"));
    assert!(!lowers("G.SetValue(H.GetValueInt())"));
    // …a float value (or a `Float as Int` truncation the lowering strips) declines.
    assert!(!lowers("G.SetValueInt(H.GetValue() * 1.5)"));
    assert!(!lowers("G.SetValueInt(H.GetValue() as Int)"));
    // SetValue is untouched.
    assert!(lowers("G.SetValue(H.GetValue() * 1.5)"));
}

/// #5974 — the same typing applies to a guard: `If Self.GetStage() / 10 == 3`
/// must not lower as a float comparison.
#[test]
fn integer_division_in_a_guard_declines() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         If Self.GetStage() / 10 == 3\n\
         Self.SetStage(40)\n\
         EndIf\n\
         EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
    // The control: the same guard over `+ - *` or a float division lowers, so
    // the decline above is the division, not the guard shape.
    for expression in ["Self.GetStage() * 10 == 30", "Self.GetStage() / 10.0 == 3"] {
        let control = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             If {expression}\n\
             Self.SetStage(40)\n\
             EndIf\n\
             EndFunction\n"
        ));
        assert!(lower_fragment(&control).is_some(), "{expression}");
    }
}

/// #6215 — a guard on a truncated global reads the stored float through exact
/// bounds: `G.GetValueInt() == 12` holds for 12 <= G < 13 (GameHour 12.5
/// included, which an untruncated `12.5 == 12` refused), and a cast of a
/// float reading the lowering cannot bound declines.
#[test]
fn a_truncated_global_guard_lowers_to_exact_float_bounds() {
    let guards = |condition: &str| {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             GlobalVariable Property G Auto\n\
             Function Fragment_0()\n\
             If {condition}\n\
             Self.SetStage(40)\n\
             EndIf\n\
             EndFunction\n"
        ));
        lower_fragment(&body)
    };
    let global = |comparison, value| FragmentGuard::Global { global: ObjectRef::Property("G".into()), comparison, value };
    for condition in ["G.GetValueInt() == 12", "(G.GetValue() as Int) == 12", "(G.Value as Int) == 12"] {
        let effects = guards(condition).unwrap_or_else(|| panic!("{condition} lowers"));
        let text = format!("{effects:?}");
        let expected = format!(
            "{:?}",
            FragmentGuard::Any(vec![vec![global(GuardComparison::Ge, 12.0), global(GuardComparison::Lt, 13.0)]])
        );
        assert!(text.contains(&expected), "{condition}: {text}");
    }
    // A non-integral literal against an Int has no exact float bound here.
    assert_eq!(guards("G.GetValueInt() == 12.5"), None);
}

/// #5976: a compound assignment to a declared local is not a re-bind to the
/// right-hand side (`n += 2` would have lowered as `n = 2`). Every operator
/// but plain `=` declines; plain `=` still lowers.
#[test]
fn compound_assignment_to_a_declared_local_declines() {
    for op in ["+=", "-=", "*=", "/=", "%="] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Int n = 1\n\
             n {op} 2\n\
             Self.SetStage(10)\n\
             EndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), None, "`n {op} 2` must decline");
    }
}

/// M47.2 — actor values as run-time numbers: by property, by name, and
/// by FO4's `Game.Get<Name>AV()` accessor; and `SetValue` with one.
#[test]
fn actor_values_lower_as_run_time_numbers() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\nActorValue Property Counter Auto\n\
         Function Fragment_5()\n\
         If Self.GetStage() >= Game.GetPlayer().GetAV(\"Health\")\n Self.SetStage(10)\n EndIf\n\
         Game.GetPlayer().SetValue(Counter, Game.GetPlayer().GetValue(Counter) + 1)\n\
         Game.GetPlayer().SetValue(Game.GetHealthAV(), 100.0)\n EndFunction\n",
    );
    let effects = lower_fragment(&body).expect("lowers");
    let counter = || ObjectRef::Property("Counter".into());
    assert!(matches!(
        &effects[0],
        Effect::Conditional { guards, .. } if matches!(
            guards.as_slice(),
            [FragmentGuard::Compare { right: ValueExpr::ActorValueNamed(ActorRef::Player, name), .. }]
                if name == "Health"
        )
    ), "{effects:?}");
    assert_eq!(
        effects[1],
        Effect::SetActorValue {
            actor: ActorRef::Player,
            actor_value: counter(),
            value: ValueExpr::Arith(
                Box::new(ValueExpr::ActorValue(ActorRef::Player, counter())),
                ArithOp::Add,
                Box::new(ValueExpr::Literal(1.0)),
            ),
        }
    );
    assert_eq!(
        effects[2],
        Effect::SetActorValueNamed {
            actor: ActorRef::Player,
            name: "Health".into(),
            value: ValueExpr::Literal(100.0),
        }
    );
}

#[test]
fn declines_unmodeled_conditional_guard() {
    // `GetStage() >= 5`, disjunctions, faction tests and comparisons of
    // two run-time numbers lower since M47.2; an unmodelled read (alone,
    // inside a disjunction or inside a comparison) and a fragment-script
    // variable (`Self.x`) still decline.
    for condition in [
        "Game.GetPlayer().IsSneaking()",
        "Self.GetStageDone(5) || Game.GetPlayer().IsSneaking()",
        "Self.bDone",
        "Self.GetStage() >= Game.GetPlayer().GetEquippedWeapon().GetBaseDamage()",
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             If {condition}\n Self.SetStage(10)\n EndIf\n EndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), None, "{condition}");
    }
}

/// M47.2 — `ShowAsHelpMessage` lowers with its Fallout 4 context and
/// priority and Starfield's gamepad message; a menu context declines.
#[test]
fn help_message_lowers_with_context_priority_and_gamepad_message() {
    let lowered = |call: &str| {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nMessage Property Help Auto\nMessage Property PadHelp Auto\n\
             Function Fragment_1()\n {call}\n EndFunction\n"
        ));
        lower_fragment(&body)
    };
    for call in [
        "Help.ShowAsHelpMessage(\"Jump\", 5.0, 30.0, 3)",
        "Help.ShowAsHelpMessage(\"Jump\", 5.0, 30.0, 3, \"NoMenu\", 2)",
        "Help.ShowAsHelpMessage(\"Jump\", 5.0, 30.0, 3, \"\", 0, PadHelp)",
        "Help.ShowAsHelpMessage(\"Jump\", 5.0, 30.0, 3, \"\", 0, None)",
    ] {
        assert!(lowered(call).is_some(), "{call}");
    }
    assert!(lowered("Help.ShowAsHelpMessage(\"Jump\", 5.0, 30.0, 3, \"ContainerMenu\", 0)").is_none());
}

/// M47.2 — `IsInCombat()` is a guard on the actor's combat state.
#[test]
fn lowers_the_in_combat_guard() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_5()\n\
         If Game.GetPlayer().IsInCombat()\n Self.SetStage(10)\n EndIf\n EndFunction\n",
    );
    let lowered = lower_fragment(&body).expect("lowers");
    assert!(
        format!("{lowered:?}").contains("ActorInCombat"),
        "{lowered:?}"
    );
}

/// M47.2 — two run-time numbers compared: a random roll, a level against
/// a global, and arithmetic.
#[test]
fn run_time_number_comparisons_lower_as_compare_guards() {
    let lowered = |condition: &str| {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             If {condition}\n Self.SetStage(10)\n EndIf\n EndFunction\n"
        ));
        match lower_fragment(&body)?.as_slice() {
            [Effect::Conditional { guards, .. }] => Some(guards.clone()),
            _ => None,
        }
    };
    assert_eq!(
        lowered("Utility.RandomInt(1, 3) == 1"),
        Some(vec![FragmentGuard::Compare {
            left: ValueExpr::RandomInt(1, 3),
            comparison: GuardComparison::Eq,
            right: ValueExpr::Literal(1.0),
        }])
    );
    assert_eq!(
        lowered("Game.GetPlayer().GetLevel() < MinLevel.GetValue() + 2"),
        Some(vec![FragmentGuard::Compare {
            left: ValueExpr::Level(ActorRef::Player),
            comparison: GuardComparison::Lt,
            right: ValueExpr::Arith(
                Box::new(ValueExpr::Global(ObjectRef::Property("MinLevel".into()))),
                ArithOp::Add,
                Box::new(ValueExpr::Literal(2.0)),
            ),
        }])
    );
    assert_eq!(
        lowered("!(Self.GetStage() >= Self.GetStage())"),
        Some(vec![FragmentGuard::Compare {
            left: ValueExpr::Stage(QuestRef::SelfRef),
            comparison: GuardComparison::Lt,
            right: ValueExpr::Stage(QuestRef::SelfRef),
        }]),
        "negation flips the comparison"
    );
}

/// M47.2 — a local holding a run-time number feeds a later comparison;
/// with a suspension in the same fragment it declines.
#[test]
fn run_time_locals_lower_and_never_suspend() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_5()\n\
         Int roll = Utility.RandomInt(1, 3)\n\
         If roll == 1\n Self.SetStage(10)\n Else\n Self.SetStage(20)\n EndIf\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::SetLocal {
                local: "roll".into(),
                value: ValueExpr::RandomInt(1, 3),
            },
            Effect::Conditional {
                guards: vec![FragmentGuard::Compare {
                    left: ValueExpr::Local("roll".into()),
                    comparison: GuardComparison::Eq,
                    right: ValueExpr::Literal(1.0),
                }],
                then_effects: vec![Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 10
                }],
                else_effects: vec![Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 20
                }],
            },
        ])
    );
    let suspends = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_5()\n\
         Int roll = Utility.RandomInt(1, 3)\n\
         Utility.Wait(1.0)\n\
         If roll == 1\n Self.SetStage(10)\n EndIf\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&suspends), None, "locals do not survive a suspension");
}

/// M47.2 — actor, quest, scene and location guard atoms.
#[test]
fn actor_quest_scene_and_location_guards_lower() {
    let property = |name: &str| ObjectRef::Property(name.into());
    for (condition, guard) in [
        (
            "Victim.IsDead()",
            FragmentGuard::ActorDead {
                actor: ActorRef::Object(property("Victim")),
                dead: true,
            },
        ),
        (
            "!Game.GetPlayer().HasPerk(SomePerk)",
            FragmentGuard::HasPerk {
                actor: ActorRef::Player,
                perk: property("SomePerk"),
                expected: false,
            },
        ),
        (
            "Game.GetPlayer().GetItemCount(Gold001) >= 100",
            FragmentGuard::ItemCount {
                container: ActorRef::Player,
                item: property("Gold001"),
                comparison: GuardComparison::Ge,
                value: 100.0,
            },
        ),
        (
            "Self.IsCompleted()",
            FragmentGuard::QuestCompleted {
                quest: QuestRef::SelfRef,
                completed: true,
            },
        ),
        (
            "SomeScene.IsPlaying() == False",
            FragmentGuard::ScenePlaying {
                scene: property("SomeScene"),
                playing: false,
            },
        ),
        (
            "Game.GetPlayer().GetCurrentLocation().HasKeyword(LocTypeCity)",
            FragmentGuard::LocationKeyword {
                actor: ActorRef::Player,
                keyword: property("LocTypeCity"),
                expected: true,
            },
        ),
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             If {condition}\n Self.SetStage(10)\n EndIf\n EndFunction\n"
        ));
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::Conditional {
                guards: vec![guard],
                then_effects: vec![Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 10
                }],
                else_effects: vec![],
            }]),
            "{condition}"
        );
    }
    // M47.2 — an actor's own keywords.
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_5()\n\
         If Game.GetPlayer().HasKeyword(SomeKeyword)\n Self.SetStage(10)\n EndIf\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::Conditional {
            guards: vec![FragmentGuard::HasKeyword {
                object: ActorRef::Player,
                keyword: ObjectRef::Property("SomeKeyword".into()),
                expected: true,
            }],
            then_effects: vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 10
            }],
            else_effects: vec![],
        }])
    );
}

/// M47.2 — `GetLinkedRef([keyword])` as a receiver and as a local's
/// initializer.
#[test]
fn linked_refs_lower_as_linked_object_refs() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\nReferenceAlias Property Alias_Gate Auto\n\
         Keyword Property LinkGate Auto\nFunction Fragment_5()\n\
         Alias_Gate.GetReference().GetLinkedRef(LinkGate).Disable()\n\
         ObjectReference lever = Alias_Gate.GetRef().GetLinkedRef()\n\
         lever.Enable()\n EndFunction\n",
    );
    let gate = || Box::new(ObjectRef::Property("Alias_Gate".into()));
    assert_eq!(
        lower_fragment(&body),
        Some(vec![
            Effect::Disable {
                object: ObjectRef::Linked {
                    from: gate(),
                    keyword: Some(Box::new(ObjectRef::Property("LinkGate".into()))),
                },
                fade_out: false,
            },
            Effect::Enable {
                object: ObjectRef::Linked {
                    from: gate(),
                    keyword: None,
                },
                fade_in: false,
            },
        ])
    );
}

/// M47.2 — reference identity: two aliases' references, and an alias
/// against `None`; a bare property comparison (a number, as far as the
/// name says) is not a reference comparison.
#[test]
fn reference_comparisons_lower_as_same_reference_guards() {
    let lower = |condition: &str| {
        lower_fragment(&first_fn_body(&format!(
            "ScriptName QF extends Quest\nReferenceAlias Property Alias_A Auto\n\
             ReferenceAlias Property Alias_B Auto\nFunction Fragment_5()\n\
             If {condition}\n Self.SetStage(10)\n EndIf\n EndFunction\n"
        )))
    };
    let guard = |condition: &str| match lower(condition).as_deref() {
        Some([Effect::Conditional { guards, .. }]) => guards.clone(),
        other => panic!("{condition}: {other:?}"),
    };
    let a = || ActorRef::Object(ObjectRef::Property("Alias_A".into()));
    assert_eq!(
        guard("Alias_A.GetActorRef() == Alias_B.GetActorRef()"),
        vec![FragmentGuard::SameReference {
            left: a(),
            right: Some(ActorRef::Object(ObjectRef::Property("Alias_B".into()))),
            expected: true,
        }]
    );
    assert_eq!(
        guard("!(Alias_A.GetReference() == None)"),
        vec![FragmentGuard::SameReference {
            left: a(),
            right: None,
            expected: false,
        }]
    );
    assert_eq!(
        guard("Game.GetPlayer() != Alias_A.GetActorRef()"),
        vec![FragmentGuard::SameReference {
            left: ActorRef::Player,
            right: Some(a()),
            expected: false,
        }]
    );
    assert_eq!(lower("SomeCount == OtherCount"), None);
}

/// M47.2 — disjunctions: `||` becomes [`FragmentGuard::Any`], nested ones
/// flatten, and negation distributes by De Morgan.
#[test]
fn disjunctive_guards_lower() {
    let done = |stage, done| FragmentGuard::StageDone {
        quest: QuestRef::SelfRef,
        stage,
        done,
    };
    for (condition, guards) in [
        (
            "Self.GetStageDone(5) || Self.GetStageDone(6) || Self.GetStageDone(7)",
            vec![FragmentGuard::Any(vec![
                vec![done(5, true)],
                vec![done(6, true)],
                vec![done(7, true)],
            ])],
        ),
        (
            "Self.GetStageDone(1) && (Self.GetStageDone(5) || !Self.GetStageDone(6))",
            vec![
                done(1, true),
                FragmentGuard::Any(vec![vec![done(5, true)], vec![done(6, false)]]),
            ],
        ),
        (
            // !(A || B) is !A && !B.
            "!(Self.GetStageDone(5) || Self.GetStageDone(6))",
            vec![done(5, false), done(6, false)],
        ),
        (
            // !(A && B) is !A || !B.
            "!(Self.GetStageDone(5) && Self.GetStageDone(6))",
            vec![FragmentGuard::Any(vec![vec![done(5, false)], vec![done(6, false)]])],
        ),
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             If {condition}\n Self.SetStage(10)\n EndIf\n EndFunction\n"
        ));
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::Conditional {
                guards,
                then_effects: vec![Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 10
                }],
                else_effects: vec![],
            }]),
            "{condition}"
        );
    }
}

/// M47.2 — the guard atoms the corpus uses beyond `GetStageDone`: negated
/// stage tests (and FO4's `IsStageDone` spelling), `IsRunning`, stage
/// comparisons with the literal on either side, objective state, global
/// reads and quest-script variables.
#[test]
fn lowers_the_widened_guard_atoms() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_5()\n\
         Quest __temp = Self as Quest\n\
         MyQuestScript kmyQuest = __temp as MyQuestScript\n\
         If !Self.GetStageDone(100) && Self.IsStageDone(5) == false && OtherQuest.IsRunning()\n\
         Self.SetStage(10)\n\
         EndIf\n\
         If 20 > OtherQuest.GetStage() && Self.IsObjectiveDisplayed(10) && !Self.IsObjectiveCompleted(60)\n\
         Self.SetStage(11)\n\
         EndIf\n\
         If Ransom.GetValueInt() == 3 && Ransom.Value < 1.0 && kmyQuest.bDeaconTalkedTo == 1\n\
         Self.SetStage(12)\n\
         EndIf\n EndFunction\n",
    );
    let mut quests = HashSet::new();
    quests.insert("otherquest".to_string());
    let other = || QuestRef::Property("OtherQuest".into());
    let then = |stage| vec![Effect::SetStage { quest: QuestRef::SelfRef, stage }];
    assert_eq!(
        lower_fragment_with_quest_properties(&body, &quests),
        Some(vec![
            Effect::Conditional {
                guards: vec![
                    FragmentGuard::StageDone { quest: QuestRef::SelfRef, stage: 100, done: false },
                    FragmentGuard::StageDone { quest: QuestRef::SelfRef, stage: 5, done: false },
                    FragmentGuard::QuestRunning { quest: other(), running: true },
                ],
                then_effects: then(10),
                else_effects: vec![],
            },
            Effect::Conditional {
                guards: vec![
                    FragmentGuard::Stage {
                        quest: other(),
                        comparison: GuardComparison::Lt,
                        value: 20.0,
                    },
                    FragmentGuard::ObjectiveDisplayed {
                        quest: QuestRef::SelfRef,
                        objective: 10,
                        expected: true,
                    },
                    FragmentGuard::ObjectiveCompleted {
                        quest: QuestRef::SelfRef,
                        objective: 60,
                        expected: false,
                    },
                ],
                then_effects: then(11),
                else_effects: vec![],
            },
            Effect::Conditional {
                guards: vec![
                    // #6215 — `GetValueInt() == 3` is `3 <= Ransom < 4` on the stored float.
                    FragmentGuard::Any(vec![vec![
                        FragmentGuard::Global {
                            global: ObjectRef::Property("Ransom".into()),
                            comparison: GuardComparison::Ge,
                            value: 3.0,
                        },
                        FragmentGuard::Global {
                            global: ObjectRef::Property("Ransom".into()),
                            comparison: GuardComparison::Lt,
                            value: 4.0,
                        },
                    ]]),
                    FragmentGuard::Global {
                        global: ObjectRef::Property("Ransom".into()),
                        comparison: GuardComparison::Lt,
                        value: 1.0,
                    },
                    FragmentGuard::QuestVariable {
                        quest: QuestRef::SelfRef,
                        variable: "bDeaconTalkedTo".into(),
                        comparison: GuardComparison::Eq,
                        value: 1.0,
                    },
                ],
                then_effects: then(12),
                else_effects: vec![],
            },
        ])
    );
}

#[test]
fn empty_fragment_is_understood_as_noop() {
    let body =
        first_fn_body("ScriptName QF extends Quest\n Function Fragment_6()\n EndFunction\n");
    assert_eq!(lower_fragment(&body), Some(vec![]));
}

// ── Decline-path coverage for SCR-D5-NEW5-02 / #2289 ────────────────
// One `?`/arg-count/arg-type guard test per primitive that previously
// shipped with a positive-path test only.

/// Regression for #3496. `prim_set_stage` and the three objective
/// primitives read only positional args 0 and 1 and, alone among the
/// primitives, bounded neither — so an over-arity call lowered
/// silently instead of declining. Vanilla content cannot produce one
/// (the compiler emits exactly the declared arity, and all four
/// signatures are inside the read range), but a modded `.pex` is
/// untrusted input that reaches this code.
#[test]
fn stage_and_objective_primitives_decline_over_arity_calls() {
    // `Quest.SetStage(int aiStage)` — one parameter.
    let set_stage = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_50()\n\
         Self.SetStage(10, 1)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&set_stage), None);

    // `SetObjectiveDisplayed(aiObjective, abDisplayed = true,
    // abForce = false)` — three.
    let displayed = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_51()\n\
         Self.SetObjectiveDisplayed(10, true, false, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&displayed), None);

    // `SetObjectiveCompleted`/`SetObjectiveFailed(aiObjective,
    // ab… = true)` — two.
    let completed = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_52()\n\
         Self.SetObjectiveCompleted(10, true, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&completed), None);

    let failed = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_53()\n\
         Self.SetObjectiveFailed(10, true, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&failed), None);
}

/// The positive side of the bounds above: each primitive's real
/// signature must still lower at full arity, so the fix is a bound at
/// the declared parameter count and not a narrowing (#3496).
#[test]
fn stage_and_objective_primitives_still_lower_at_full_arity() {
    let displayed = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_54()\n\
         Self.SetObjectiveDisplayed(10, true, false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&displayed),
        Some(vec![Effect::SetObjectiveDisplayed {
            quest: QuestRef::SelfRef,
            objective: 10,
            displayed: true,
        }])
    );

    let completed = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_55()\n\
         Self.SetObjectiveCompleted(10, false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&completed),
        Some(vec![Effect::SetObjectiveCompleted {
            quest: QuestRef::SelfRef,
            objective: 10,
            completed: false,
        }])
    );

    let failed = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_56()\n\
         Self.SetObjectiveFailed(10, false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&failed),
        Some(vec![Effect::SetObjectiveFailed {
            quest: QuestRef::SelfRef,
            objective: 10,
            failed: false,
        }])
    );
}

/// #5652 — `SetObjectiveDisplayed` accepts its 3rd argument only as a
/// literal: a non-literal (or side-effecting) `abForce` expression
/// must decline the call instead of being silently dropped while the
/// arity bound pretends three arguments were handled.
#[test]
fn set_objective_displayed_declines_non_literal_third_arguments() {
    // A variable is the simplest non-literal 3rd arg a fragment body
    // can carry — not a bool literal, so `bool_arg` declines.
    let non_literal = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_57()\n\
         Int force\n\
         force = 1\n\
         Self.SetObjectiveDisplayed(10, true, force)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&non_literal), None);

    // The literal shapes still lower, including the 3rd arg true.
    let forced = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_58()\n\
         Self.SetObjectiveDisplayed(10, false, true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&forced),
        Some(vec![Effect::SetObjectiveDisplayed {
            quest: QuestRef::SelfRef,
            objective: 10,
            displayed: false,
        }])
    );
}

/// #5068 — a named call argument binds nothing: the whole translate
/// chain indexes `CallArg`s positionally, so a named argument
/// (`Self.SetObjectiveDisplayed(objective = 10, true)`, or the issue's
/// `SetEnemy(other, abOtherIsNeutralToSelf = true)` inverting the flag
/// direction) would have lowered into the wrong parameter slot instead
/// of declining. `.pex` never produces the shape (the decompiler emits
/// `name: None`); hand-authored `.psc` can, and `method_call` now
/// declines any named argument — the same discipline the provider seam
/// applies.
#[test]
fn named_call_arguments_decline_instead_of_binding_positionally() {
    let named = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_1()\n\
         Self.SetObjectiveDisplayed(objective = 10, true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&named),
        None,
        "a named argument must decline, not bind positionally"
    );

    let positional = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_1()\n\
         Self.SetObjectiveDisplayed(10, true)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&positional),
        Some(vec![Effect::SetObjectiveDisplayed {
            quest: QuestRef::SelfRef,
            objective: 10,
            displayed: true,
        }]),
        "the equivalent positional shape still lowers"
    );
}

/// Every module-level `fn` in the concatenated primitive sources, as
/// `(name, code)` with `//` comment lines removed: a doc line or comment that
/// mentions `args.len()` or another function must not satisfy the arity gate.
fn primitive_source_fns(src: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let mut current: Option<(String, Vec<&str>)> = None;
    let mut flush = |current: &mut Option<(String, Vec<&str>)>| {
        if let Some((name, lines)) = current.take() {
            out.insert(name, lines.join("\n"));
        }
    };
    for line in src.lines() {
        let decl = ["pub(super) fn ", "pub(crate) fn ", "pub fn ", "fn "]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix));
        if let Some(rest) = decl {
            flush(&mut current);
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            current = Some((name, vec![line]));
        } else if let Some((_, lines)) = current.as_mut() {
            if !line.trim_start().starts_with("//") {
                lines.push(line);
            }
        }
    }
    flush(&mut current);
    out
}

/// Whether `code` compares the call's argument count against a bound that an
/// OVER-arity call fails: an exact or capped count, a `1..=N` range, a
/// zero-argument native (`!args.is_empty()` declines, or `args.is_empty()`
/// is the accept condition), or a fixed-arity slice pattern. A lone
/// `args.is_empty()` decline or `args.len() < N` is a lower bound only.
fn bounds_arity_from_above(code: &str) -> bool {
    [
        "args.len() !=",
        "args.len() >",
        "args.len() <=",
        "args.len() ==",
        "matches!(args.len()",
        ".contains(&args.len())",
        "!args.is_empty()",
        "args.is_empty().then",
        "args.is_empty() &&",
        "] = args else",
        "] = args.as_slice() else",
    ]
    .iter()
    .any(|pattern| code.contains(pattern))
}

/// Whether `name`'s body bounds its arguments from above, directly or through
/// a function it calls (to `depth` levels): the delegating primitives
/// (`prim_add_race_spells` -> `lower_race_spells` -> `prim_race_spells_arity_ok`)
/// inherit the helper's bound, but only through a real call.
fn bounds_arity(
    name: &str,
    fns: &std::collections::HashMap<String, String>,
    depth: usize,
) -> bool {
    let Some(code) = fns.get(name) else {
        return false;
    };
    if bounds_arity_from_above(code) {
        return true;
    }
    if depth == 0 {
        return false;
    }
    let mut callees = Vec::new();
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            let end = i;
            let mut start = end;
            while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
                start -= 1;
            }
            if start < end {
                callees.push(&code[start..end]);
            }
        }
        i += 1;
    }
    callees
        .into_iter()
        .filter(|callee| *callee != name && fns.contains_key(*callee))
        .any(|callee| bounds_arity(callee, fns, depth - 1))
}

/// #3496's completeness check, mechanized: every effect primitive must
/// bound its argument count from above, directly or through a function it
/// really calls. The finding came out of a hand sweep of all 31 `prim_*`
/// bodies that found exactly four unguarded; a hand sweep does not survive the
/// next primitive being added, so it becomes a gate here.
///
/// #5977 — the gate used to pass a body that merely contained `args.len()` or
/// `args.is_empty()` anywhere (a lower-bound-only check qualifies) or the text
/// `prim_` (a comment mention counts as "delegates"), and its completeness
/// margin was slack by the three helper bodies. Now: comments are stripped, a
/// bound must be an upper bound, delegation must be a call that resolves to a
/// bounded function, and the helpers are excluded by name so the scan must
/// match the table row for row.
#[test]
fn every_effect_primitive_bounds_its_argument_count() {
    // #5801 — the primitives live in `effects/prims/*.rs` now: scan the
    // module's concatenation, which leaves this file out, so the scan
    // cannot pick up its own body.
    let fns = primitive_source_fns(super::SOURCES);
    // `prim_*` helpers that are not rows of `EFFECT_PRIMITIVES`.
    const NON_TABLE_HELPERS: &[&str] = &[
        "prim_player_controls",
        "prim_player_animation_event",
        "prim_race_spells_arity_ok",
    ];
    let mut names: Vec<&String> = fns
        .keys()
        .filter(|name| name.starts_with("prim_") && !NON_TABLE_HELPERS.contains(&name.as_str()))
        .collect();
    names.sort();
    let unguarded: Vec<&String> = names
        .iter()
        .copied()
        .filter(|name| !bounds_arity(name, &fns, 3))
        .collect();
    assert!(names.len() > 30, "the primitive scan matched only {} bodies", names.len());
    // Every table row is a `prim_*` body: with the helpers excluded by name the
    // counts must be equal, so a scan that lost (or gained) a file fails.
    assert_eq!(
        names.len(),
        EFFECT_PRIMITIVES.len(),
        "the primitive scan matched {} bodies for {} table rows",
        names.len(),
        EFFECT_PRIMITIVES.len()
    );
    assert!(
        unguarded.is_empty(),
        "every effect primitive must bound its argument count from above \
         against its real Papyrus signature — an unbounded one silently lowers \
         an over-arity call from a modded .pex instead of declining \
         (#3496, #5977). Unguarded: {unguarded:?}"
    );
}

/// #5977 — the gate's own predicates reject the shapes it used to accept.
#[test]
fn the_arity_gate_rejects_lower_bounds_and_comment_mentions() {
    let fns = primitive_source_fns(
        "fn prim_lower_only(e: &Expr) -> Option<Effect> {\n\
         \x20   if args.len() < 1 { return None; }\n\
         \x20   if args.is_empty() { return None; }\n\
         }\n\
         fn prim_comment_only(e: &Expr) -> Option<Effect> {\n\
         \x20   // bounded by args.len() != 2 inside prim_other_arity_ok\n\
         \x20   lower(e)\n\
         }\n\
         fn prim_exact(e: &Expr) -> Option<Effect> {\n\
         \x20   if args.len() != 2 { return None; }\n\
         }\n\
         fn prim_zero(e: &Expr) -> Option<Effect> {\n\
         \x20   if !args.is_empty() { return None; }\n\
         }\n\
         fn prim_delegates(e: &Expr) -> Option<Effect> {\n\
         \x20   helper(e)\n\
         }\n\
         fn helper(e: &Expr) -> Option<Effect> {\n\
         \x20   if args.len() > 3 { return None; }\n\
         }\n",
    );
    assert!(!bounds_arity("prim_lower_only", &fns, 3), "a lower bound is not a bound");
    assert!(!bounds_arity("prim_comment_only", &fns, 3), "a comment is not a bound");
    assert!(bounds_arity("prim_exact", &fns, 3));
    assert!(bounds_arity("prim_zero", &fns, 3));
    assert!(bounds_arity("prim_delegates", &fns, 3), "a real call to a bounded helper is");
}

#[test]
fn set_open_declines_with_extra_arg() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_20()\n\
         KeepGate.SetOpen(true, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

#[test]
fn set_player_restrained_declines_on_non_player_receiver_and_extra_arg() {
    let non_player = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_21()\n\
         SomeActor.SetRestrained(true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&non_player), None);

    let extra_arg = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_45()\n\
         Game.GetPlayer().SetRestrained(true, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&extra_arg), None);
}

#[test]
fn player_controls_toggles_decline_with_too_many_args() {
    // Shared `prim_player_controls`'s `args.len() > 9` guard — covers
    // both DisablePlayerControls and EnablePlayerControls.
    let disable = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_22()\n\
         Game.DisablePlayerControls(false, true, true, false, false, false, true, true, 0, false)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&disable), None);

    let enable = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_23()\n\
         Game.EnablePlayerControls(false, true, true, false, false, false, true, true, 0, false)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&enable), None);
}

#[test]
fn set_player_ai_driven_declines_with_extra_arg() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_24()\n\
         Game.SetPlayerAIDriven(true, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

#[test]
fn set_hud_cart_mode_declines_with_extra_arg() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_25()\n\
         Game.SetHudCartMode(true, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

#[test]
fn play_idle_declines_on_wrong_arg_count() {
    let zero_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_26()\n\
         Game.GetPlayer().PlayIdle()\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&zero_args), None);

    let two_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_27()\n\
         Game.GetPlayer().PlayIdle(SomeIdle, SomeIdle)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&two_args), None);
}

#[test]
fn set_vehicle_declines_on_wrong_arg_count() {
    let zero_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_28()\n\
         Alias_Rider.GetActorRef().SetVehicle()\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&zero_args), None);

    let two_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_29()\n\
         Alias_Rider.GetActorRef().SetVehicle(Alias_Cart.GetRef(), Alias_Cart.GetRef())\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&two_args), None);
}

#[test]
fn tether_to_horse_declines_on_wrong_arg_count() {
    let zero_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_30()\n\
         Alias_Cart.GetRef().TetherToHorse()\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&zero_args), None);

    let two_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_47()\n\
         Alias_Cart.GetRef().TetherToHorse(Alias_Horse.GetActorRef() as ObjectReference, Alias_Horse.GetActorRef() as ObjectReference)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&two_args), None);
}

#[test]
fn set_motion_type_declines_on_arg_count_and_unrecognized_member() {
    // #2289 flags this as the most structurally intricate untested case.
    let zero_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_31()\n\
         Cart.SetMotionType()\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&zero_args), None);

    let three_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_32()\n\
         Cart.SetMotionType(6, true, false)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&three_args), None);

    // An unrecognized member access on the (correctly matched) receiver
    // falls through `motion_type_arg`'s match arm to `None`.
    let unrecognized_member = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_33()\n\
         Cart.SetMotionType(Cart.Motion_NotARealValue, true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&unrecognized_member), None);
}

#[test]
fn set_sitting_rotation_declines_on_wrong_arg_count() {
    let zero_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_34()\n\
         Game.SetSittingRotation()\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&zero_args), None);

    let two_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_48()\n\
         Game.SetSittingRotation(-55.0, 1.0)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&two_args), None);
}

#[test]
fn exit_cart_declines_on_wrong_arg_count_and_seat_out_of_range() {
    let wrong_arg_count = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_35()\n\
         Quest temp = Self as Quest\n\
         mq101questscript kmyQuest = temp as mq101questscript\n\
         kmyQuest.ExitCart(Alias_Prisoner)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&wrong_arg_count), None);

    let seat_too_low = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_36()\n\
         Quest temp = Self as Quest\n\
         mq101questscript kmyQuest = temp as mq101questscript\n\
         kmyQuest.ExitCart(Alias_Prisoner, 0)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&seat_too_low), None);

    let seat_too_high = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_37()\n\
         Quest temp = Self as Quest\n\
         mq101questscript kmyQuest = temp as mq101questscript\n\
         kmyQuest.ExitCart(Alias_Prisoner, 6)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&seat_too_high), None);
}

#[test]
fn player_animation_events_decline_with_args() {
    let imod = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_38()\n\
         Quest temp = Self as Quest\n\
         mq101questscript kmyQuest = temp as mq101questscript\n\
         kmyQuest.PlayerImodAnimation(true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&imod), None);

    let furniture = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_39()\n\
         Quest temp = Self as Quest\n\
         mq101questscript kmyQuest = temp as mq101questscript\n\
         kmyQuest.PlayerFurnitureAnimation(true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&furniture), None);
}

/// `abResetAI = true` resets the actor's AI on top of the package
/// re-evaluation, which `Effect::EvaluatePackage` does not model —
/// so it still declines after #3487 widened the arity bound.
#[test]
fn evaluate_package_declines_with_args() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_40()\n\
         Alias_Hadvar.GetActorRef().EvaluatePackage(true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&body), None);
}

/// Regression for #3487. Skyrim's `Actor.EvaluatePackage()` takes no
/// parameters; FO4's and Starfield's take `abResetAI = false`, so the
/// identical statement compiles to a one-argument call there. The
/// old zero-argument guard made this primitive silently
/// game-asymmetric — working on Skyrim, declining 2,628 of 3,481
/// calls on FO4/Starfield.
#[test]
fn lowers_the_default_materialized_evaluate_package_shape() {
    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_40()\n\
         Alias_Hadvar.GetActorRef().EvaluatePackage(false)\n EndFunction\n",
    );
    assert_eq!(
        lower_fragment(&body),
        Some(vec![Effect::EvaluatePackage {
            actor: ActorRef::Object(ObjectRef::Property("Alias_Hadvar".into())),
        }])
    );
}

#[test]
fn wait_declines_on_wrong_arg_count_and_negative_seconds() {
    let zero_args = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_41()\n\
         Utility.Wait()\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&zero_args), None);

    let negative = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_42()\n\
         Utility.Wait(-1.0)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&negative), None);
}

#[test]
fn start_and_stop_scene_decline_with_args() {
    let start = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_43()\n\
         IntroScene.Start(true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&start), None);

    let stop = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_44()\n\
         OldScene.Stop(true)\n EndFunction\n",
    );
    assert_eq!(lower_fragment(&stop), None);
}

// ── #2540 / SCR-D5-NEW10-02 — the objective-index `u16`→`i32` widen ──

/// A genuinely negative index (i32 on FO3/FNV per `QuestObjective::
/// index`'s doc) arrives from the PEX decompiler as a direct signed
/// `IntLit`, never as a `.psc`-source `UnaryOp::Neg` — the `.psc` text
/// parser lexes `-N` as `Minus` + `IntLit(N)`, which `int_arg` does not
/// unwrap. Matching #2657's `::X_var` precedent, this decompiled shape
/// has to be built by hand rather than round-tripped through the `.psc`
/// parser to exercise the real input this primitive actually receives.
#[test]
fn objective_primitives_lower_a_negative_index() {
    for (method, expected) in [
        (
            "SetObjectiveDisplayed",
            Effect::SetObjectiveDisplayed {
                quest: QuestRef::SelfRef,
                objective: -5,
                displayed: true,
            },
        ),
        (
            "SetObjectiveCompleted",
            Effect::SetObjectiveCompleted {
                quest: QuestRef::SelfRef,
                objective: -5,
                completed: true,
            },
        ),
        (
            "SetObjectiveFailed",
            Effect::SetObjectiveFailed {
                quest: QuestRef::SelfRef,
                objective: -5,
                failed: true,
            },
        ),
    ] {
        let call = Expr::Call {
            callee: Box::new(sp(Expr::MemberAccess {
                object: Box::new(sp(Expr::Ident(Identifier("Self".into())))),
                member: sp(Identifier(method.into())),
            })),
            args: vec![CallArg {
                name: None,
                value: sp(Expr::IntLit(-5)),
            }],
        };
        assert_eq!(
            classify_effect(&call, &Scope::default()),
            Some(expected),
            "{method} must lower a negative index correctly"
        );
    }
}

/// An index literal outside `i32`'s range must still decline via
/// `i32::try_from(..).ok()?`, not silently truncate.
#[test]
fn objective_primitives_decline_on_index_overflow() {
    for method in [
        "SetObjectiveDisplayed",
        "SetObjectiveCompleted",
        "SetObjectiveFailed",
    ] {
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\n\
             Function Fragment_46()\n\
             Self.{method}(5000000000)\n EndFunction\n"
        ));
        assert_eq!(
            lower_fragment(&body),
            None,
            "{method} must decline an i32-overflowing index"
        );
    }
}

/// #3934 — one installed extension must not change how vanilla content
/// lowers. `Self` is the SDK's reserved provider for instance-method
/// aliases, so any extension that publishes one makes
/// `catalog.contains_provider("self")` true; with the catalog consulted
/// first, `Self.SetStage(10)` then missed the route, hard-errored as an
/// unbound extender call, and declined the whole fragment — across the
/// entire quest-fragment population, on every game.
#[test]
fn a_self_provider_alias_does_not_shadow_the_canonical_effect_table() {
    use byroredux_sdk::identity::{ComponentId, ExtensionId, ScriptFunctionId};
    use byroredux_sdk::script_function::{PapyrusFunctionAlias, ScriptFunctionDeclaration};

    let extension = ExtensionId::new("audit.fixture").expect("valid extension id");
    let mut providers = PapyrusProviderCatalog::engine_compatibility();
    providers
        .insert(
            &extension,
            &ScriptFunctionDeclaration {
                id: ScriptFunctionId::new("touch").expect("valid function id"),
                component: ComponentId::new("runtime").expect("valid component id"),
                parameters: Vec::new(),
                result: None,
                // Reserved instance-method spelling: `Self.Touch()`.
                papyrus: Some(PapyrusFunctionAlias {
                    provider: "Self".to_owned(),
                    function: "Touch".to_owned(),
                }),
                description: "fixture instance method".to_owned(),
            },
        )
        .expect("the alias must install");

    let body = first_fn_body(
        "ScriptName QF extends Quest\n\
         Function Fragment_0()\n\
         Self.SetStage(10)\n\
         EndFunction\n",
    );
    let effects = lower_fragment_with_quest_properties_and_providers(
        &body,
        &HashSet::new(),
        Some(&providers),
    )
    .expect("a canonical SetStage must still lower with a Self alias installed");
    assert_eq!(
        effects,
        vec![Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 10,
        }],
        "the canonical effect table must win over a manifest alias"
    );
}
