//! Ambient AI-package selection and behavior installation (M42.1–M42.9).
//!
//! Spawn still installs the first behavior immediately, but the actor also
//! retains its PKID candidate list in [`AmbientPackageRuntime`]. The runtime
//! system reuses the same selector at game-minute boundaries and on Papyrus
//! [`EvaluatePackageRequest`], replacing behavior state only when the winning
//! PACK FormID actually changes.

use super::{EsmIndex, NpcRecord};
use crate::components::{AmbientPackageRuntime, GameTimeRes, SeatReservations};
use byroredux_core::ecs::components::{
    EscortBehavior, EscortState, Escorted, FollowBehavior, FollowState, GuardBehavior, GuardState,
    PatrolBehavior, PatrolState, SandboxBehavior, Seated, TravelBehavior, TravelState, Traveled,
    WanderBehavior, WanderState,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, World};
use byroredux_plugin::esm::records::{PackLocationTarget, PackRecord, PackTargetKind};
use byroredux_scripting::condition::ConditionContext;
use byroredux_scripting::{EvaluatePackageRequest, PackageRegistry};

/// Whether an AI package's CTDA conditions permit it to be selected for this
/// actor (M42.2), evaluated through the M47.1 condition evaluator.
///
/// Unknown condition functions retain the established fail-open policy. The
/// evaluator cannot soundly preserve the authored OR/AND blocks when one leaf
/// is unknown, so rejecting the entire package would regress pre-CTDA behavior.
fn package_conditions_pass(
    conditions: &byroredux_plugin::esm::records::condition::ConditionList,
    world: &World,
    ctx: &ConditionContext,
) -> bool {
    use byroredux_scripting::condition::{evaluate, ConditionFunction};

    if conditions.is_empty() {
        return true;
    }
    let all_known = conditions.iter().all(|condition| {
        !matches!(
            ConditionFunction::from_index(condition.function_index),
            ConditionFunction::Unknown(_)
        )
    });
    if !all_known {
        return true;
    }
    evaluate(conditions, world, ctx)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum AmbientBehavior {
    Sandbox {
        search_radius: Option<f32>,
    },
    Wander {
        wander_radius: Option<f32>,
        actor_form_id: u32,
    },
    Travel {
        radius: Option<f32>,
        target_form_id: Option<u32>,
        actor_form_id: u32,
    },
    Follow {
        target_form_id: Option<u32>,
        follow_distance: Option<f32>,
    },
    Escort {
        target_form_id: Option<u32>,
        destination_form_id: Option<u32>,
        destination_radius: Option<f32>,
        actor_form_id: u32,
    },
    Guard {
        anchor_form_id: Option<u32>,
        radius: Option<f32>,
        actor_form_id: u32,
    },
    Patrol {
        patrol_radius: Option<f32>,
        actor_form_id: u32,
    },
}

impl AmbientBehavior {
    fn from_package(package: &PackRecord, actor_form_id: u32) -> Option<Self> {
        let location_radius = package
            .location
            .map(|location| location.radius as f32)
            .filter(|radius| *radius > 0.0);
        let location_reference = package.location.and_then(|location| match location.target {
            PackLocationTarget::NearReference(form_id) => Some(form_id),
            _ => None,
        });
        let target_reference = package.target.and_then(|target| match target.target {
            PackTargetKind::SpecificReference(form_id) | PackTargetKind::ObjectId(form_id) => {
                Some(form_id)
            }
            PackTargetKind::Other(_) => None,
        });
        let target_distance = package
            .target
            .map(|target| target.count_or_distance as f32)
            .filter(|distance| *distance > 0.0);

        if package.is_sandbox() {
            Some(Self::Sandbox {
                search_radius: location_radius,
            })
        } else if package.is_wander() {
            Some(Self::Wander {
                wander_radius: location_radius,
                actor_form_id,
            })
        } else if package.is_travel() {
            Some(Self::Travel {
                radius: location_radius,
                target_form_id: location_reference,
                actor_form_id,
            })
        } else if package.is_follow() {
            Some(Self::Follow {
                target_form_id: target_reference,
                follow_distance: target_distance,
            })
        } else if package.is_escort() {
            Some(Self::Escort {
                target_form_id: target_reference,
                destination_form_id: location_reference,
                destination_radius: location_radius,
                actor_form_id,
            })
        } else if package.is_guard() {
            Some(Self::Guard {
                anchor_form_id: location_reference,
                radius: location_radius,
                actor_form_id,
            })
        } else if package.is_patrol() {
            Some(Self::Patrol {
                patrol_radius: location_radius,
                actor_form_id,
            })
        } else {
            None
        }
    }

    fn insert_at_spawn(self, world: &mut World, actor: EntityId) {
        match self {
            Self::Sandbox { search_radius } => {
                world.insert(actor, SandboxBehavior { search_radius });
            }
            Self::Wander {
                wander_radius,
                actor_form_id,
            } => {
                world.insert(
                    actor,
                    WanderBehavior {
                        wander_radius,
                        form_id: actor_form_id,
                    },
                );
            }
            Self::Travel {
                radius,
                target_form_id,
                actor_form_id,
            } => {
                world.insert(
                    actor,
                    TravelBehavior {
                        radius,
                        target_form_id,
                        form_id: actor_form_id,
                    },
                );
            }
            Self::Follow {
                target_form_id,
                follow_distance,
            } => {
                world.insert(
                    actor,
                    FollowBehavior {
                        target_form_id,
                        follow_distance,
                    },
                );
            }
            Self::Escort {
                target_form_id,
                destination_form_id,
                destination_radius,
                actor_form_id,
            } => {
                world.insert(
                    actor,
                    EscortBehavior {
                        target_form_id,
                        destination_form_id,
                        destination_radius,
                        form_id: actor_form_id,
                    },
                );
            }
            Self::Guard {
                anchor_form_id,
                radius,
                actor_form_id,
            } => {
                world.insert(
                    actor,
                    GuardBehavior {
                        anchor_form_id,
                        radius,
                        form_id: actor_form_id,
                    },
                );
            }
            Self::Patrol {
                patrol_radius,
                actor_form_id,
            } => {
                world.insert(
                    actor,
                    PatrolBehavior {
                        patrol_radius,
                        form_id: actor_form_id,
                    },
                );
            }
        }
    }

    fn insert_at_runtime(self, world: &World, actor: EntityId) {
        match self {
            Self::Sandbox { search_radius } => {
                insert_component(world, actor, SandboxBehavior { search_radius });
            }
            Self::Wander {
                wander_radius,
                actor_form_id,
            } => insert_component(
                world,
                actor,
                WanderBehavior {
                    wander_radius,
                    form_id: actor_form_id,
                },
            ),
            Self::Travel {
                radius,
                target_form_id,
                actor_form_id,
            } => insert_component(
                world,
                actor,
                TravelBehavior {
                    radius,
                    target_form_id,
                    form_id: actor_form_id,
                },
            ),
            Self::Follow {
                target_form_id,
                follow_distance,
            } => insert_component(
                world,
                actor,
                FollowBehavior {
                    target_form_id,
                    follow_distance,
                },
            ),
            Self::Escort {
                target_form_id,
                destination_form_id,
                destination_radius,
                actor_form_id,
            } => insert_component(
                world,
                actor,
                EscortBehavior {
                    target_form_id,
                    destination_form_id,
                    destination_radius,
                    form_id: actor_form_id,
                },
            ),
            Self::Guard {
                anchor_form_id,
                radius,
                actor_form_id,
            } => insert_component(
                world,
                actor,
                GuardBehavior {
                    anchor_form_id,
                    radius,
                    form_id: actor_form_id,
                },
            ),
            Self::Patrol {
                patrol_radius,
                actor_form_id,
            } => insert_component(
                world,
                actor,
                PatrolBehavior {
                    patrol_radius,
                    form_id: actor_form_id,
                },
            ),
        }
    }
}

fn insert_component<T: Component>(world: &World, actor: EntityId, component: T) {
    world
        .query_mut::<T>()
        .expect("ambient AI component storage must be registered at boot")
        .insert(actor, component);
}

fn remove_component<T: Component>(world: &World, actor: EntityId) {
    if let Some(mut query) = world.query_mut::<T>() {
        query.remove(actor);
    }
}

fn clear_ambient_behavior(world: &World, actor: EntityId) {
    if let Some(mut reservations) = world.try_resource_mut::<SeatReservations>() {
        reservations.0.retain(|_, claimant| *claimant != actor);
    }

    remove_component::<SandboxBehavior>(world, actor);
    remove_component::<Seated>(world, actor);
    remove_component::<WanderBehavior>(world, actor);
    remove_component::<WanderState>(world, actor);
    remove_component::<TravelBehavior>(world, actor);
    remove_component::<TravelState>(world, actor);
    remove_component::<Traveled>(world, actor);
    remove_component::<FollowBehavior>(world, actor);
    remove_component::<FollowState>(world, actor);
    remove_component::<EscortBehavior>(world, actor);
    remove_component::<EscortState>(world, actor);
    remove_component::<Escorted>(world, actor);
    remove_component::<GuardBehavior>(world, actor);
    remove_component::<GuardState>(world, actor);
    remove_component::<PatrolBehavior>(world, actor);
    remove_component::<PatrolState>(world, actor);
}

fn select_active_package<'a>(
    world: &World,
    actor: EntityId,
    game_hour: f32,
    packages: impl IntoIterator<Item = &'a PackRecord>,
) -> Option<&'a PackRecord> {
    let context = ConditionContext {
        subject: actor,
        target: None,
        combat_target: None,
        linked_reference: None,
        quest: None,
    };
    byroredux_plugin::esm::records::active_package(packages, game_hour, |package| {
        package_conditions_pass(&package.conditions, world, &context)
    })
}

fn game_minute(game_hour: f32) -> u16 {
    if !game_hour.is_finite() {
        return 0;
    }
    (game_hour.rem_euclid(24.0) * 60.0).floor() as u16
}

/// Attach the spawn-time winner and retain enough stable identity to select a
/// different ambient package later. Shared by both NPC spawn pipelines.
pub(super) fn apply_ai_package_behavior(
    world: &mut World,
    placement_root: EntityId,
    npc: &NpcRecord,
    index: &EsmIndex,
) {
    if npc.ai_packages.is_empty() {
        return;
    }

    let game_hour = world
        .try_resource::<GameTimeRes>()
        .map(|time| time.hour)
        .unwrap_or(10.0);
    let active = select_active_package(
        world,
        placement_root,
        game_hour,
        npc.ai_packages
            .iter()
            .filter_map(|form_id| index.packages.get(form_id)),
    );
    let active_package_form_id = active.map(|package| package.form_id);
    let behavior = active.and_then(|package| AmbientBehavior::from_package(package, npc.form_id));

    world.insert(
        placement_root,
        AmbientPackageRuntime {
            package_candidates: npc.ai_packages.clone(),
            active_package_form_id,
            actor_form_id: npc.form_id,
            // Force a first-tick confirmation after the cell loader installs
            // PACK, quest, and restored game-time resources.
            last_evaluated_game_minute: None,
        },
    );
    if let Some(behavior) = behavior {
        behavior.insert_at_spawn(world, placement_root);
    }
}

/// M42.9 / #2652 — reevaluate ambient NPC package stacks.
///
/// Scheduled immediately before `scene_package_system`: this system observes
/// `EvaluatePackageRequest` but deliberately does not remove it, allowing the
/// SCEN runtime to consume the same request afterward. Schedule-only checks are
/// bounded to one evaluation per in-game minute per actor.
pub(crate) fn ambient_ai_package_system(world: &World, _dt: f32) {
    let game_hour = world
        .try_resource::<GameTimeRes>()
        .map(|time| time.hour)
        .unwrap_or(10.0);
    let minute = game_minute(game_hour);
    let runtimes: Vec<(EntityId, AmbientPackageRuntime)> = world
        .query::<AmbientPackageRuntime>()
        .map(|query| {
            query
                .iter()
                .map(|(actor, runtime)| (actor, runtime.clone()))
                .collect()
        })
        .unwrap_or_default();
    if runtimes.is_empty() {
        return;
    }

    let Some(registry) = world.try_resource::<PackageRegistry>() else {
        return;
    };
    let mut updates = Vec::new();
    for (actor, runtime) in runtimes {
        let explicitly_requested = world.has::<EvaluatePackageRequest>(actor);
        if !explicitly_requested && runtime.last_evaluated_game_minute == Some(minute) {
            continue;
        }

        let packages: Vec<&PackRecord> = runtime
            .package_candidates
            .iter()
            .filter_map(|form_id| registry.package(*form_id))
            .collect();
        // Cell spawn can precede `populate_scene_runtime`. Do not erase the
        // valid spawn-time winner merely because the catalog is not ready yet.
        if packages.is_empty() && !runtime.package_candidates.is_empty() {
            continue;
        }
        let active = select_active_package(world, actor, game_hour, packages.iter().copied());
        let active_package_form_id = active.map(|package| package.form_id);
        let behavior = active
            .and_then(|package| AmbientBehavior::from_package(package, runtime.actor_form_id));
        updates.push((
            actor,
            active_package_form_id,
            behavior,
            active_package_form_id != runtime.active_package_form_id,
        ));
    }
    drop(registry);

    for &(actor, _, behavior, changed) in &updates {
        if !changed {
            continue;
        }
        clear_ambient_behavior(world, actor);
        if let Some(behavior) = behavior {
            behavior.insert_at_runtime(world, actor);
        }
    }

    if let Some(mut query) = world.query_mut::<AmbientPackageRuntime>() {
        for (actor, active_package_form_id, _, _) in updates {
            if let Some(runtime) = query.get_mut(actor) {
                runtime.active_package_form_id = active_package_form_id;
                runtime.last_evaluated_game_minute = Some(minute);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::wander::WanderPhase;
    use byroredux_core::math::Vec3;
    use byroredux_plugin::esm::records::condition::{
        ComparisonOp, Condition, ConditionValue, RunOn,
    };
    use byroredux_plugin::esm::records::misc::pack::{
        PackSchedule, PROCEDURE_GUARD, PROCEDURE_SANDBOX, PROCEDURE_TRAVEL, PROCEDURE_WANDER,
    };
    use byroredux_plugin::esm::records::SceneActionType;
    use byroredux_scripting::quest_stages::QuestStageState;
    use byroredux_scripting::{
        install_package_records, scene_package_system, QuestFormId, SceneEvent, SceneEventBatch,
        ScenePackageEvent, ScenePackageEventBatch, ScenePlayer,
    };

    fn register_runtime(world: &mut World, hour: f32) {
        byroredux_scripting::register(world);
        world.register::<AmbientPackageRuntime>();
        world.register::<SandboxBehavior>();
        world.register::<Seated>();
        world.register::<WanderBehavior>();
        world.register::<WanderState>();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<FollowBehavior>();
        world.register::<FollowState>();
        world.register::<EscortBehavior>();
        world.register::<EscortState>();
        world.register::<Escorted>();
        world.register::<GuardBehavior>();
        world.register::<GuardState>();
        world.register::<PatrolBehavior>();
        world.register::<PatrolState>();
        world.insert_resource(QuestStageState::default());
        world.insert_resource(GameTimeRes::frozen_at(hour));
        world.insert_resource(SeatReservations::default());
    }

    fn pack(form_id: u32, procedure_type: u32, schedule: Option<PackSchedule>) -> PackRecord {
        PackRecord {
            form_id,
            procedure_type,
            schedule,
            ..Default::default()
        }
    }

    fn setup_actor(hour: f32, packages: Vec<PackRecord>) -> (World, EntityId) {
        let mut world = World::new();
        register_runtime(&mut world, hour);
        let actor = world.spawn();
        let candidates: Vec<u32> = packages.iter().map(|package| package.form_id).collect();
        let npc = NpcRecord {
            form_id: 0xAA,
            ai_packages: candidates,
            ..Default::default()
        };
        let mut index = EsmIndex::default();
        for package in packages.iter().cloned() {
            index.packages.insert(package.form_id, package);
        }
        apply_ai_package_behavior(&mut world, actor, &npc, &index);
        install_package_records(&mut world, packages);
        ambient_ai_package_system(&world, 0.0);
        (world, actor)
    }

    #[test]
    fn schedule_boundary_replaces_behavior_and_releases_seat_claim() {
        let sandbox = pack(
            0x100,
            PROCEDURE_SANDBOX,
            Some(PackSchedule {
                start_hour: Some(8),
                duration_hours: 12,
            }),
        );
        let travel = pack(
            0x200,
            PROCEDURE_TRAVEL,
            Some(PackSchedule {
                start_hour: Some(20),
                duration_hours: 2,
            }),
        );
        let (world, actor) = setup_actor(10.0, vec![sandbox, travel]);
        assert!(world.has::<SandboxBehavior>(actor));

        let furniture = 700;
        world
            .query_mut::<Seated>()
            .unwrap()
            .insert(actor, Seated { furniture });
        world
            .resource_mut::<SeatReservations>()
            .0
            .insert((furniture, 0), actor);
        world.resource_mut::<GameTimeRes>().set_hour(21.0);

        ambient_ai_package_system(&world, 0.0);

        assert!(!world.has::<SandboxBehavior>(actor));
        assert!(!world.has::<Seated>(actor));
        assert!(world.has::<TravelBehavior>(actor));
        assert!(world.resource::<SeatReservations>().0.is_empty());
        assert_eq!(
            world
                .get::<AmbientPackageRuntime>(actor)
                .unwrap()
                .active_package_form_id,
            Some(0x200)
        );
    }

    #[test]
    fn evaluate_package_reselects_after_quest_stage_change() {
        const QUEST: u32 = 0x900;
        let mut gated_wander = pack(0x100, PROCEDURE_WANDER, None);
        gated_wander.conditions.push(Condition {
            function_index: 58,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(10.0),
            param_1: QUEST,
            run_on: RunOn::Subject,
            ..Default::default()
        });
        let guard = pack(0x200, PROCEDURE_GUARD, None);
        let (world, actor) = setup_actor(10.0, vec![gated_wander, guard]);
        assert!(world.has::<GuardBehavior>(actor));

        world
            .resource_mut::<QuestStageState>()
            .set_stage(QuestFormId(QUEST), 10);
        world
            .query_mut::<EvaluatePackageRequest>()
            .unwrap()
            .insert(actor, EvaluatePackageRequest);

        ambient_ai_package_system(&world, 0.0);

        assert!(world.has::<WanderBehavior>(actor));
        assert!(!world.has::<GuardBehavior>(actor));
        assert!(
            world.has::<EvaluatePackageRequest>(actor),
            "ambient evaluation must leave the request for the SCEN consumer"
        );
    }

    #[test]
    fn same_winner_preserves_runtime_state() {
        let (world, actor) = setup_actor(10.0, vec![pack(0x100, PROCEDURE_WANDER, None)]);
        let state = WanderState {
            home: Vec3::new(1.0, 2.0, 3.0),
            target: Vec3::new(4.0, 5.0, 6.0),
            phase: WanderPhase::Paused { remaining: 2.5 },
            pick_count: 7,
        };
        world
            .query_mut::<WanderState>()
            .unwrap()
            .insert(actor, state);
        world
            .query_mut::<EvaluatePackageRequest>()
            .unwrap()
            .insert(actor, EvaluatePackageRequest);

        ambient_ai_package_system(&world, 0.0);

        assert_eq!(*world.get::<WanderState>(actor).unwrap(), state);
    }

    #[test]
    fn unknown_condition_function_remains_fail_open() {
        let mut unknown = pack(0x100, PROCEDURE_SANDBOX, None);
        unknown.conditions.push(Condition {
            function_index: 65_000,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(1.0),
            ..Default::default()
        });
        let fallback = pack(0x200, PROCEDURE_TRAVEL, None);

        let (world, actor) = setup_actor(10.0, vec![unknown, fallback]);

        assert!(world.has::<SandboxBehavior>(actor));
        assert!(!world.has::<TravelBehavior>(actor));
    }

    #[test]
    fn ambient_and_scene_runtimes_observe_the_same_evaluate_request() {
        const QUEST: u32 = 0x900;
        let mut gated_wander = pack(0x100, PROCEDURE_WANDER, None);
        gated_wander.conditions.push(Condition {
            function_index: 58,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(10.0),
            param_1: QUEST,
            ..Default::default()
        });
        let guard = pack(0x200, PROCEDURE_GUARD, None);
        let (mut world, actor) = setup_actor(10.0, vec![gated_wander, guard]);
        let scene_package = PackRecord {
            form_id: 0x500,
            ..Default::default()
        };
        install_package_records(&mut world, [scene_package]);
        let scene = world.spawn();
        world.insert(scene, ScenePlayer::new(0x700));
        world.insert(
            scene,
            SceneEventBatch(vec![SceneEvent::ActionStarted {
                action_index: 3,
                action_type: SceneActionType::Package,
                actor_alias: 1,
                actor_entity: Some(actor),
                topic_form_id: None,
                packages: vec![0x500],
            }]),
        );
        scene_package_system(&world, 0.0);

        world
            .resource_mut::<QuestStageState>()
            .set_stage(QuestFormId(QUEST), 10);
        world
            .query_mut::<EvaluatePackageRequest>()
            .unwrap()
            .insert(actor, EvaluatePackageRequest);

        ambient_ai_package_system(&world, 0.0);
        assert!(world.has::<WanderBehavior>(actor));
        assert!(world.has::<EvaluatePackageRequest>(actor));

        scene_package_system(&world, 0.0);
        assert!(!world.has::<EvaluatePackageRequest>(actor));
        assert!(world
            .get::<ScenePackageEventBatch>(scene)
            .is_some_and(|batch| {
                batch.0.iter().any(|event| {
                    matches!(
                        event,
                        ScenePackageEvent::Reevaluated(action) if action.actor == actor
                    )
                })
            }));
    }
}
