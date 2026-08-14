//! Quest-alias resolution, injection, and diagnostics tests.
//!
//! Extracted from `scene.rs`'s inline `mod tests` (#2408 / TD1-005),
//! following the repo's sibling-test convention. Contents unchanged.

use super::*;

use byroredux_core::ecs::components::{Dead, FactionRanks, GlobalTransform, Inventory};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{
    AliasFillType, QuestAlias, QustRecord, ALIAS_FLAG_ALLOW_DEAD, ALIAS_FLAG_ALLOW_RESERVED,
    ALIAS_FLAG_RESERVES,
};

use crate::quest_stages::QuestStageState;

const QUEST: u32 = 0x200;

#[test]
fn quest_alias_diagnostics_classify_runtime_boundaries_and_dependencies() {
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(QuestStageState::default());
    world
        .resource_mut::<QuestStageState>()
        .start_quest(QuestFormId(QUEST), None);
    install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: QUEST,
            aliases: vec![
                QuestAlias {
                    alias_id: 1,
                    fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                    force_into_alias: Some(6),
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 2,
                    fill_type: Some(AliasFillType::CreatedObject {
                        base: 0xB1,
                        target_alias: 1,
                        create_mode: 0,
                        level: 1,
                    }),
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 3,
                    fill_type: Some(AliasFillType::FromEvent {
                        event_type: *b"ACTV",
                        data: 0,
                    }),
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 4,
                    fill_type: Some(AliasFillType::ExternalAlias {
                        quest: 0x300,
                        alias_id: 9,
                    }),
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 5,
                    fill_type: Some(AliasFillType::NearAlias {
                        alias_id: 1,
                        relation: 0,
                    }),
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 6,
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 7,
                    is_location: true,
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 8,
                    is_collection: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
    );
    refresh_scene_actor_bindings(&world);

    let diagnostics = quest_alias_diagnostics(&world, QuestFormId(QUEST)).unwrap();
    let state = |alias_id| {
        &diagnostics
            .iter()
            .find(|diagnostic| diagnostic.alias.alias_id == alias_id)
            .unwrap()
            .state
    };
    assert_eq!(
        state(1),
        &QuestAliasResolutionState::NoEligibleLoadedCandidate
    );
    assert_eq!(
        state(2),
        &QuestAliasResolutionState::CreatedObjectRuntimeUnavailable
    );
    assert_eq!(
        state(3),
        &QuestAliasResolutionState::StoryManagerEventUnavailable
    );
    assert_eq!(
        state(4),
        &QuestAliasResolutionState::ExternalSourceUnbound {
            quest: QuestFormId(0x300),
            alias_id: 9,
        }
    );
    assert_eq!(
        state(5),
        &QuestAliasResolutionState::DependencyAliasUnbound(1)
    );
    assert_eq!(
        state(6),
        &QuestAliasResolutionState::ForceIntoSourcesUnbound(vec![1])
    );
    assert_eq!(
        state(7),
        &QuestAliasResolutionState::LocationRuntimeUnavailable
    );
    assert_eq!(
        state(8),
        &QuestAliasResolutionState::ReferenceCollectionRuntimeUnavailable
    );

    world
        .resource_mut::<QuestStageState>()
        .stop(QuestFormId(QUEST));
    assert!(quest_alias_diagnostics(&world, QuestFormId(QUEST))
        .unwrap()
        .iter()
        .all(|diagnostic| diagnostic.state == QuestAliasResolutionState::QuestNotRunning));
}

/// Regression for #2661 (SCR-D6-NEW11-04) — an `ALCS` reference-collection
/// alias whose fill mechanism WOULD otherwise match a real candidate
/// (unlike the no-fill-type/no-match-conditions alias 8 in the diagnostics
/// test above, which never had anything to bind to even pre-fix) must
/// still decline entirely: no candidate binds, the diagnostic reports
/// `ReferenceCollectionRuntimeUnavailable` rather than `Bound`, and the
/// candidate that would have matched receives none of the collection's
/// injected overlays. Pre-fix, `refresh_scene_actor_bindings`'s fill loop
/// only excluded `is_location` aliases, so this collection alias fell
/// through to the ordinary single-entity `eligible` path and bound the one
/// matching candidate, which then received the whole collection's
/// injected faction — exactly the "one arbitrary member gets the
/// collection's data, and diagnostics say it filled correctly" failure
/// mode the design doc's Phase 4+ deferral is meant to prevent.
#[test]
fn quest_alias_collection_fill_type_still_declines_not_binds() {
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(QuestStageState::default());
    world
        .resource_mut::<QuestStageState>()
        .start_quest(QuestFormId(QUEST), None);

    let candidate_entity = world.spawn();
    world.insert(
        candidate_entity,
        SceneAliasCandidate {
            reference_form_id: 0xA1,
            base_form_id: 0xB1,
            linked_refs: Vec::new(),
            location_ref_types: Vec::new(),
        },
    );

    const COLLECTION_FACTION: u32 = 0xF00D;
    install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: QUEST,
            aliases: vec![QuestAlias {
                alias_id: 8,
                is_collection: true,
                // A fill mechanism that WOULD match `candidate_entity` if
                // this alias were treated as an ordinary single-entity
                // alias — this is exactly the shape the audit's "match
                // conditions" case exercises (a fill path that CAN
                // resolve, not one that trivially can't).
                fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                injected: byroredux_plugin::esm::records::AliasInjectedData {
                    factions: vec![COLLECTION_FACTION],
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }],
    );

    assert_eq!(
        refresh_scene_actor_bindings(&world),
        0,
        "a collection alias must never bind via the ordinary single-entity fill loop (#2661)"
    );
    let bindings = world.resource::<SceneActorBindings>();
    assert_eq!(
        bindings.resolve(QuestFormId(QUEST), 8),
        None,
        "collection alias must stay unbound even though its fill_type matches a real candidate"
    );

    let diagnostics = quest_alias_diagnostics(&world, QuestFormId(QUEST)).unwrap();
    assert_eq!(
        diagnostics[0].state,
        QuestAliasResolutionState::ReferenceCollectionRuntimeUnavailable,
        "must decline with the documented diagnostic, not report false success"
    );

    let ranks = world.get::<FactionRanks>(candidate_entity);
    assert!(
        ranks.is_none_or(|ranks| ranks.rank(COLLECTION_FACTION).is_none()),
        "the matching candidate must NOT receive the collection's injected faction (#2661)"
    );
}

#[test]
fn quest_alias_refresh_resolves_direct_unique_and_distinct_xlrt_roles() {
    let mut world = World::new();
    crate::register(&mut world);
    let forced = world.spawn();
    let unique = world.spawn();
    let soldier_a = world.spawn();
    let soldier_b = world.spawn();
    for (entity, candidate) in [
        (
            forced,
            SceneAliasCandidate {
                reference_form_id: 0xA1,
                base_form_id: 0xB1,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        ),
        (
            unique,
            SceneAliasCandidate {
                reference_form_id: 0xA2,
                base_form_id: 0xB2,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        ),
        (
            soldier_a,
            SceneAliasCandidate {
                reference_form_id: 0xA3,
                base_form_id: 0xB3,
                linked_refs: Vec::new(),
                location_ref_types: vec![0xC1],
            },
        ),
        (
            soldier_b,
            SceneAliasCandidate {
                reference_form_id: 0xA4,
                base_form_id: 0xB3,
                linked_refs: Vec::new(),
                location_ref_types: vec![0xC1],
            },
        ),
    ] {
        world.insert(entity, candidate);
    }
    install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: QUEST,
            aliases: vec![
                QuestAlias {
                    alias_id: 1,
                    name: "Forced".to_owned(),
                    fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                    force_into_alias: Some(10),
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 2,
                    name: "Unique".to_owned(),
                    fill_type: Some(AliasFillType::UniqueActor(0xB2)),
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 3,
                    name: "SoldierA".to_owned(),
                    fill_type: Some(AliasFillType::LocationAliasReference {
                        alias_id: 0,
                        keyword: None,
                        ref_type: Some(0xC1),
                    }),
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 4,
                    name: "SoldierB".to_owned(),
                    fill_type: Some(AliasFillType::LocationAliasReference {
                        alias_id: 0,
                        keyword: None,
                        ref_type: Some(0xC1),
                    }),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
    );

    assert_eq!(refresh_scene_actor_bindings(&world), 5);
    let bindings = world.resource::<SceneActorBindings>();
    assert_eq!(bindings.resolve(QuestFormId(QUEST), 1), Some(forced));
    assert_eq!(bindings.resolve(QuestFormId(QUEST), 10), Some(forced));
    assert_eq!(bindings.resolve(QuestFormId(QUEST), 2), Some(unique));
    assert_eq!(bindings.resolve(QuestFormId(QUEST), 3), Some(soldier_a));
    assert_eq!(bindings.resolve(QuestFormId(QUEST), 4), Some(soldier_b));
    assert_eq!(refresh_scene_actor_bindings(&world), 0, "clean fast path");
}

#[test]
fn quest_alias_reservations_block_later_quests_unless_allowed() {
    let mut world = World::new();
    crate::register(&mut world);
    let actor = world.spawn();
    world.insert(
        actor,
        SceneAliasCandidate {
            reference_form_id: 0xA1,
            base_form_id: 0xB1,
            linked_refs: Vec::new(),
            location_ref_types: Vec::new(),
        },
    );
    install_scene_quest_aliases(
        &mut world,
        [
            QustRecord {
                form_id: 0x100,
                aliases: vec![QuestAlias {
                    alias_id: 1,
                    fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                    flags: byroredux_plugin::esm::records::AliasFlags(ALIAS_FLAG_RESERVES),
                    ..Default::default()
                }],
                ..Default::default()
            },
            QustRecord {
                form_id: 0x200,
                aliases: vec![QuestAlias {
                    alias_id: 1,
                    fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                    ..Default::default()
                }],
                ..Default::default()
            },
            QustRecord {
                form_id: 0x300,
                aliases: vec![QuestAlias {
                    alias_id: 1,
                    fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                    flags: byroredux_plugin::esm::records::AliasFlags(ALIAS_FLAG_ALLOW_RESERVED),
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
    );

    refresh_scene_actor_bindings(&world);
    let bindings = world.resource::<SceneActorBindings>();
    assert_eq!(bindings.resolve(QuestFormId(0x100), 1), Some(actor));
    assert_eq!(bindings.resolve(QuestFormId(0x200), 1), None);
    assert_eq!(bindings.resolve(QuestFormId(0x300), 1), Some(actor));
}

#[test]
fn quest_alias_closest_to_alias_selects_by_world_distance() {
    use byroredux_core::math::{Quat, Vec3};

    let mut world = World::new();
    crate::register(&mut world);
    world.register::<GlobalTransform>();
    let anchor = world.spawn();
    let far = world.spawn();
    let near = world.spawn();
    for (entity, reference, base, x) in [
        (anchor, 0xA0, 0xB0, 0.0),
        (far, 0xA1, 0xB1, 20.0),
        (near, 0xA2, 0xB1, 2.0),
    ] {
        world.insert(
            entity,
            SceneAliasCandidate {
                reference_form_id: reference,
                base_form_id: base,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        world.insert(
            entity,
            GlobalTransform::new(Vec3::new(x, 0.0, 0.0), Quat::IDENTITY, 1.0),
        );
    }
    install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: QUEST,
            aliases: vec![
                QuestAlias {
                    alias_id: 0,
                    fill_type: Some(AliasFillType::ForcedReference(0xA0)),
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 1,
                    fill_type: Some(AliasFillType::UniqueActor(0xB1)),
                    closest_to_alias: Some(0),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
    );

    refresh_scene_actor_bindings(&world);
    assert_eq!(
        world
            .resource::<SceneActorBindings>()
            .resolve(QuestFormId(QUEST), 1),
        Some(near)
    );
}

/// Regression for #2664 (SCR-D7-NEW11-03): a distance-ranked alias whose only
/// candidate is a logical identity stub — a REFR that produced no 3D — must
/// still fill.
///
/// The ranking loop reads `world.get::<GlobalTransform>(entity)?` *inside a
/// `filter_map`*, so a candidate without one is not ranked last, it is dropped
/// from the `min_by` entirely and `chosen` comes back `None`. The alias then
/// stays unfilled with no log line and no error. That is exactly the shape the
/// worldspace persistent-cell loader used to spawn for remote / spawn-less
/// persistent `ACHR`s — the population M47.3 was built around — because it
/// open-coded `stamp_quest_reference` and inserted no transform.
///
/// Both directions are asserted: the transform-bearing stub fills, and the
/// transform-less one silently does not. The second half is what makes the
/// first half meaningful rather than incidentally true.
#[test]
fn quest_alias_closest_fill_needs_a_transform_on_its_only_candidate() {
    use byroredux_core::math::{Quat, Vec3};

    fn resolve_with_stub(stub_has_transform: bool) -> (World, Option<EntityId>) {
        let mut world = World::new();
        crate::register(&mut world);
        world.register::<GlobalTransform>();
        let anchor = world.spawn();
        let stub = world.spawn();
        for (entity, reference, base) in [(anchor, 0xA0, 0xB0), (stub, 0xA1, 0xB1)] {
            world.insert(
                entity,
                SceneAliasCandidate {
                    reference_form_id: reference,
                    base_form_id: base,
                    linked_refs: Vec::new(),
                    location_ref_types: Vec::new(),
                },
            );
        }
        world.insert(
            anchor,
            GlobalTransform::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, 1.0),
        );
        if stub_has_transform {
            world.insert(
                stub,
                GlobalTransform::new(Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY, 1.0),
            );
        }
        install_scene_quest_aliases(
            &mut world,
            [QustRecord {
                form_id: QUEST,
                aliases: vec![
                    QuestAlias {
                        alias_id: 0,
                        fill_type: Some(AliasFillType::ForcedReference(0xA0)),
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 1,
                        fill_type: Some(AliasFillType::UniqueActor(0xB1)),
                        closest_to_alias: Some(0),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
        );
        refresh_scene_actor_bindings(&world);
        let resolved = world
            .resource::<SceneActorBindings>()
            .resolve(QuestFormId(QUEST), 1);
        (world, resolved)
    }

    let (_world, transform_bearing) = resolve_with_stub(true);
    assert!(
        transform_bearing.is_some(),
        "a logical stub carrying a transform must still be rankable, or every \
         distance-anchored alias over 3D-less references is unfillable"
    );

    let (_world, positionless) = resolve_with_stub(false);
    assert_eq!(
        positionless, None,
        "premise check: without a transform the candidate is filtered out of the \
         ranking entirely — this is the silent failure #2664 fixes at the spawn \
         site, not something the resolver can recover from"
    );
}

#[test]
fn quest_alias_near_alias_resolves_linked_ref_child() {
    let mut world = World::new();
    crate::register(&mut world);
    let source = world.spawn();
    let child = world.spawn();
    let unrelated = world.spawn();
    for (entity, candidate) in [
        (
            source,
            SceneAliasCandidate {
                reference_form_id: 0xA0,
                base_form_id: 0xB0,
                linked_refs: vec![(0, 0xA1)],
                location_ref_types: Vec::new(),
            },
        ),
        (
            child,
            SceneAliasCandidate {
                reference_form_id: 0xA1,
                base_form_id: 0xB1,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        ),
        (
            unrelated,
            SceneAliasCandidate {
                reference_form_id: 0xA2,
                base_form_id: 0xB1,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        ),
    ] {
        world.insert(entity, candidate);
    }
    install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: QUEST,
            aliases: vec![
                QuestAlias {
                    alias_id: 0,
                    fill_type: Some(AliasFillType::ForcedReference(0xA0)),
                    ..Default::default()
                },
                QuestAlias {
                    alias_id: 1,
                    fill_type: Some(AliasFillType::NearAlias {
                        alias_id: 0,
                        relation: 0,
                    }),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
    );

    refresh_scene_actor_bindings(&world);
    assert_eq!(
        world
            .resource::<SceneActorBindings>()
            .resolve(QuestFormId(QUEST), 1),
        Some(child)
    );
}

#[test]
fn quest_aliases_exclude_dead_actors_unless_allowed() {
    let mut world = World::new();
    crate::register(&mut world);
    world.register::<Dead>();
    let actor = world.spawn();
    world.insert(
        actor,
        SceneAliasCandidate {
            reference_form_id: 0xA1,
            base_form_id: 0xB1,
            linked_refs: Vec::new(),
            location_ref_types: Vec::new(),
        },
    );
    world.insert(actor, Dead);
    install_scene_quest_aliases(
        &mut world,
        [
            QustRecord {
                form_id: 0x100,
                aliases: vec![QuestAlias {
                    alias_id: 1,
                    fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                    ..Default::default()
                }],
                ..Default::default()
            },
            QustRecord {
                form_id: 0x200,
                aliases: vec![QuestAlias {
                    alias_id: 1,
                    fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                    flags: byroredux_plugin::esm::records::AliasFlags(ALIAS_FLAG_ALLOW_DEAD),
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
    );

    refresh_scene_actor_bindings(&world);
    let bindings = world.resource::<SceneActorBindings>();
    assert_eq!(bindings.resolve(QuestFormId(0x100), 1), None);
    assert_eq!(bindings.resolve(QuestFormId(0x200), 1), Some(actor));
}

#[test]
fn quest_alias_injections_are_idempotent_and_clear_transient_factions() {
    let mut world = World::new();
    crate::register(&mut world);
    let actor = world.spawn();
    world.insert(
        actor,
        SceneAliasCandidate {
            reference_form_id: 0xA1,
            base_form_id: 0xB1,
            linked_refs: Vec::new(),
            location_ref_types: Vec::new(),
        },
    );
    world.insert(actor, FactionRanks::from_pairs([(0xF1, 3)]));
    let injected_alias = QuestAlias {
        alias_id: 1,
        fill_type: Some(AliasFillType::ForcedReference(0xA1)),
        injected: byroredux_plugin::esm::records::AliasInjectedData {
            factions: vec![0xF1, 0xF2],
            inventory: vec![(0xC1, 2), (0xC2, 1)],
            ..Default::default()
        },
        ..Default::default()
    };
    install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: QUEST,
            aliases: vec![injected_alias],
            ..Default::default()
        }],
    );

    refresh_scene_actor_bindings(&world);
    assert_eq!(
        world.get::<FactionRanks>(actor).unwrap().rank(0xF1),
        Some(3)
    );
    assert_eq!(
        world.get::<FactionRanks>(actor).unwrap().rank(0xF2),
        Some(0)
    );
    assert_eq!(world.get::<Inventory>(actor).unwrap().items.len(), 2);
    assert_eq!(
        world
            .get::<QuestAliasInjectedOverlays>(actor)
            .unwrap()
            .0
            .len(),
        1
    );
    assert_eq!(
        world
            .get::<QuestAliasRuntimeOverlays>(actor)
            .unwrap()
            .0
            .len(),
        1
    );

    mark_scene_actor_bindings_dirty(&world);
    refresh_scene_actor_bindings(&world);
    assert_eq!(
        world.get::<Inventory>(actor).unwrap().items.len(),
        2,
        "permanent CNTO grants must not duplicate on a dirty refresh"
    );

    install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: QUEST,
            aliases: vec![QuestAlias {
                alias_id: 1,
                fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                ..Default::default()
            }],
            ..Default::default()
        }],
    );
    refresh_scene_actor_bindings(&world);
    assert_eq!(
        world.get::<FactionRanks>(actor).unwrap().rank(0xF1),
        Some(3)
    );
    assert_eq!(world.get::<FactionRanks>(actor).unwrap().rank(0xF2), None);
    assert!(world.get::<QuestAliasInjectedOverlays>(actor).is_none());
    assert!(world.get::<QuestAliasRuntimeOverlays>(actor).is_some());
    assert_eq!(
        world.get::<Inventory>(actor).unwrap().items.len(),
        2,
        "CNTO grants are permanent when an alias clears"
    );
}

#[test]
fn quest_aliases_fill_only_while_the_quest_is_running() {
    let mut world = World::new();
    crate::register(&mut world);
    world.insert_resource(QuestStageState::default());
    let actor = world.spawn();
    world.insert(
        actor,
        SceneAliasCandidate {
            reference_form_id: 0xA1,
            base_form_id: 0xB1,
            linked_refs: Vec::new(),
            location_ref_types: Vec::new(),
        },
    );
    install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: QUEST,
            aliases: vec![QuestAlias {
                alias_id: 1,
                fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                ..Default::default()
            }],
            ..Default::default()
        }],
    );

    assert_eq!(refresh_scene_actor_bindings(&world), 0);
    assert_eq!(
        world
            .resource::<SceneActorBindings>()
            .resolve(QuestFormId(QUEST), 1),
        None
    );

    world
        .resource_mut::<QuestStageState>()
        .start_quest(QuestFormId(QUEST), None);
    mark_scene_actor_bindings_dirty(&world);
    assert_eq!(refresh_scene_actor_bindings(&world), 1);
    assert_eq!(
        world
            .resource::<SceneActorBindings>()
            .resolve(QuestFormId(QUEST), 1),
        Some(actor)
    );

    world
        .resource_mut::<QuestStageState>()
        .stop(QuestFormId(QUEST));
    mark_scene_actor_bindings_dirty(&world);
    assert_eq!(refresh_scene_actor_bindings(&world), 0);
    assert_eq!(
        world
            .resource::<SceneActorBindings>()
            .resolve(QuestFormId(QUEST), 1),
        None
    );
    assert!(world.get::<QuestAliasRuntimeOverlays>(actor).is_none());
}
