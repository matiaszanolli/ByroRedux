//! Ambient AI-package selection and behavior installation (M42.1–M42.9).
//!
//! Spawn still installs the first behavior immediately, but the actor also
//! retains its PKID candidate list in [`AmbientPackageRuntime`]. The runtime
//! system reuses the same selector at game-minute boundaries and on Papyrus
//! [`EvaluatePackageRequest`], replacing behavior state only when the winning
//! PACK FormID actually changes.

use super::{EsmIndex, NpcRecord};
use crate::components::{AmbientPackageRuntime, GameTimeRes, SeatReservations};
use byroredux_core::animation::AnimationPlayer;
use byroredux_core::ecs::components::{
    Dead, EscortBehavior, EscortState, Escorted, FollowBehavior, FollowState, GuardBehavior,
    GuardState, PatrolBehavior, PatrolState, SandboxBehavior, Seated, TravelBehavior, TravelState,
    Traveled, WanderBehavior, WanderState,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, World};
use byroredux_plugin::esm::records::{
    PackDataValue, PackLocationTarget, PackRecord, PackTargetKind,
};
use byroredux_scripting::condition::ConditionContext;
use byroredux_scripting::{EvaluatePackageRequest, PackageRegistry, QuestAliasInjectedOverlays};

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
        /// #3332 — authored collect range, `PKE2` first then
        /// `PTDT.count_or_distance`.
        collect_distance: Option<f32>,
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
    /// `template` is `package.package_template_form_id` chased through the
    /// packages catalog, falling back to `package` itself when there's no
    /// template ref — the exact convention `crates/scripting::package`'s
    /// Scene driver already uses (`resolve_command`'s `template` argument).
    /// FO3/FNV flat packages never set `package_template_form_id`, so for
    /// them `template` and `package` are always the same record and every
    /// existing branch below is unaffected. See `docs/engine/packal.md`.
    fn from_package(
        package: &PackRecord,
        template: &PackRecord,
        actor_form_id: u32,
    ) -> Option<Self> {
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
                // #3332 — `PKE2` is the field xEdit names "Escort Distance",
                // and it is authored on all 12 vanilla FNV Escort packages.
                // `PTDT.count_or_distance` is the fallback: it is the same
                // scalar Follow already consumes as a stand-off distance and
                // is non-zero on 5 of those 12, so it still beats the engine
                // constant when a mod omits PKE2. Both were being discarded.
                collect_distance: package
                    .escort_distance
                    .map(|d| d as f32)
                    .or(target_distance),
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
            Self::from_skyrim_procedure_tree(package, template, actor_form_id)
        }
    }

    /// PACKAL Skyrim+ tree/template fallback (`docs/engine/packal.md` §4,
    /// §5) — packages that fall through every `is_*` check above (those
    /// all read the FO3/FNV flat `procedure_type` byte, which Skyrim+
    /// packages don't populate meaningfully). Tries each leaf type PACKAL
    /// has verified safe against real `Skyrim.esm`, in priority order:
    ///
    /// - `"Sandbox"` (§4, shipped first) — a search radius from the first
    ///   `PackDataValue::Location` input. Highest-value leaf on real data
    ///   (`DefaultSandboxEditorLocation512` alone covers 307 vanilla NPCs).
    /// - `"Patrol"` (§5) — a patrol radius from the first
    ///   `PackDataValue::Float` input. Verified against the real `Patrol`
    ///   master template (form 0x017723, 701 ambient PKID edges) and 12
    ///   real per-NPC instances: the radius is consistently the *only*
    ///   resolved `Float`, never wrapped in a `Location` the way Sandbox's
    ///   is. `"Wander"` was investigated first and rejected — see §5 for
    ///   why every real `Wander`-named package either resolves to this
    ///   same `Sandbox` leaf via a shared `Travel → UnlockDoors → Sandbox`
    ///   template, or nests `"Wander"` as a narrow non-ambient fallback
    ///   inside an unrelated template (`UseWeapon`/`Sit`/…) that would be
    ///   wrong to surface as this actor's ambient behavior.
    ///
    /// Both branches share the same known v0 approximation: `.find()`
    /// picks the first matching leaf without evaluating per-procedure
    /// `conditions`, so a template with more than one occurrence of the
    /// same leaf name (real example: `DefaultMasterPackageAllowWander`,
    /// 2× `Sandbox` + 4× `Patrol` in one 10-entry tree) can pick a
    /// radius belonging to the wrong branch. Neither leaf needs
    /// `world`/`actor` context (no live-target or alias resolution), so
    /// neither needs `SceneActorBindings` (the one piece of
    /// `crates/scripting::package`'s executor this driver deliberately
    /// doesn't reuse).
    fn from_skyrim_procedure_tree(
        package: &PackRecord,
        template: &PackRecord,
        actor_form_id: u32,
    ) -> Option<Self> {
        if let Some(leaf) = template
            .procedures
            .iter()
            .find(|procedure| procedure.procedure_type == "Sandbox")
        {
            let search_radius = byroredux_scripting::package::procedure_inputs(leaf, package)
                .into_iter()
                .find_map(|input| match input.value {
                    PackDataValue::Location(location) => Some(location.radius as f32),
                    _ => None,
                })
                .filter(|radius| *radius > 0.0);
            return Some(Self::Sandbox { search_radius });
        }
        if let Some(leaf) = template
            .procedures
            .iter()
            .find(|procedure| procedure.procedure_type == "Patrol")
        {
            let patrol_radius = byroredux_scripting::package::procedure_inputs(leaf, package)
                .into_iter()
                .find_map(|input| match input.value {
                    PackDataValue::Float(radius) => Some(radius),
                    _ => None,
                })
                .filter(|radius| *radius > 0.0);
            return Some(Self::Patrol {
                patrol_radius,
                actor_form_id,
            });
        }
        None
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
                collect_distance,
                actor_form_id,
            } => {
                world.insert(
                    actor,
                    EscortBehavior {
                        target_form_id,
                        destination_form_id,
                        destination_radius,
                        collect_distance,
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
                collect_distance,
                actor_form_id,
            } => insert_component(
                world,
                actor,
                EscortBehavior {
                    target_form_id,
                    destination_form_id,
                    destination_radius,
                    collect_distance,
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

pub(crate) fn clear_ambient_behavior(world: &World, actor: EntityId) {
    if let Some(mut reservations) = world.try_resource_mut::<SeatReservations>() {
        reservations.0.retain(|_, claimant| *claimant != actor);
    }

    // #3333 — un-seating must undo the animation park, not just drop the
    // marker. `sandbox_seat_system` pins the actor on the sit-enter clip's
    // final frame with `playing = false` (the enter clip's Reverse cycle would
    // otherwise ping-pong it back to standing), and nothing else in the engine
    // ever writes `AnimationPlayer` for an NPC after spawn. Before M42.9 that
    // was unreachable — `SandboxBehavior` was attached once and never removed
    // — but `ambient_ai_package_system` now swaps behaviors on a schedule
    // handover, so a saloon patron whose daytime Sandbox package gives way to
    // an evening Travel package used to walk off in a frozen chair pose and
    // never animate again for the rest of the session.
    //
    // The snapshot rides on `Seated` itself, so this is a component read with
    // no archive access and no idle-clip re-resolution. Restored BEFORE the
    // marker is removed, for the same reason `SeatReservations` is retained
    // above it: this function is the only place that knows the actor was
    // seated at all.
    if let Some(seated) = world.get::<Seated>(actor).map(|s| *s) {
        if let Some(mut players) = world.query_mut::<AnimationPlayer>() {
            if let Some(player) = players.get_mut(actor) {
                let restore = seated.animation_restore;
                player.clip_handle = restore.clip_handle;
                player.local_time = restore.local_time;
                player.prev_time = restore.prev_time;
                player.playing = restore.playing;
                player.speed = restore.speed;
            }
        }
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
    // #2671 — `ConditionContext` gained a `pending_alias_bindings` slot for
    // the quest-alias fill loop; every other consumer, including this one,
    // reads the committed `SceneActorBindings` table exactly as before.
    let context = ConditionContext::for_subject(actor);
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
    if npc.ai_packages.is_empty() || world.get::<Dead>(placement_root).is_some() {
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
    let behavior = active.and_then(|package| {
        let template = package
            .package_template_form_id
            .and_then(|form_id| index.packages.get(&form_id))
            .unwrap_or(package);
        AmbientBehavior::from_package(package, template, npc.form_id)
    });

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
    // #3353 — the minute gate below is the whole point of this system's cost
    // model ("one evaluation per in-game minute per actor"), so nothing
    // per-actor may be paid before it. Pass 1 reads only `Copy` fields under
    // the query — no `package_candidates` clone, and no per-entity
    // `Dead` / `EvaluatePackageRequest` lock. At the default `time_scale`
    // an in-game minute is 2 real seconds, so ~119 of every 120 frames must
    // fall straight through this pass.
    let last_evaluated: Vec<(EntityId, Option<u16>)> = world
        .query::<AmbientPackageRuntime>()
        .map(|query| {
            query
                .iter()
                .map(|(actor, runtime)| (actor, runtime.last_evaluated_game_minute))
                .collect()
        })
        .unwrap_or_default();
    if last_evaluated.is_empty() {
        return;
    }

    // One query pass for the explicit-request marker instead of one
    // `world.has` per actor per frame. The marker is transient and rare, so
    // the collected set is nearly always empty.
    let requested: Vec<EntityId> = world
        .query::<EvaluatePackageRequest>()
        .map(|query| query.iter().map(|(actor, _)| actor).collect())
        .unwrap_or_default();

    // Pass 2 — the gate. Only the survivors (usually none) go on to pay for a
    // `package_candidates` clone and a `Dead` lookup.
    let due: Vec<EntityId> = last_evaluated
        .into_iter()
        .filter(|(actor, last)| requested.contains(actor) || *last != Some(minute))
        .map(|(actor, _)| actor)
        .collect();
    if due.is_empty() {
        return;
    }

    // Pass 3 — clone the candidate stacks, for the due subset only. Done in
    // one query pass rather than a `world.get` per actor, and outside the
    // `Dead` / overlay lookups below so no two component locks are ever held
    // at once (the TypeId-sorted-acquisition invariant).
    let runtimes: Vec<(EntityId, AmbientPackageRuntime)> = world
        .query::<AmbientPackageRuntime>()
        .map(|query| {
            due.iter()
                .filter_map(|&actor| query.get(actor).map(|r| (actor, r.clone())))
                .collect()
        })
        .unwrap_or_default();

    let Some(registry) = world.try_resource::<PackageRegistry>() else {
        return;
    };
    let mut updates = Vec::new();
    for (actor, runtime) in runtimes {
        if world.get::<Dead>(actor).is_some() {
            continue;
        }

        // QUST ALPC packages override the actor base's PKID stack for as
        // long as the alias remains filled. Stable source ordering avoids
        // depending on HashMap iteration when several quests overlay one
        // actor; authored order within each alias remains intact.
        let alias_overlays = world.get::<QuestAliasInjectedOverlays>(actor);
        let mut package_candidates = alias_overlays
            .map(|overlays| {
                let mut sources: Vec<_> = overlays.0.iter().collect();
                sources.sort_by_key(|((quest, alias), _)| (quest.0, *alias));
                sources
                    .into_iter()
                    .flat_map(|(_, injected)| injected.packages.iter().copied())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for &form_id in &runtime.package_candidates {
            if !package_candidates.contains(&form_id) {
                package_candidates.push(form_id);
            }
        }
        let packages: Vec<&PackRecord> = package_candidates
            .iter()
            .filter_map(|form_id| registry.package(*form_id))
            .collect();
        // Cell spawn can precede `populate_scene_runtime`. Do not erase the
        // valid spawn-time winner merely because the catalog is not ready yet.
        if packages.is_empty() && !package_candidates.is_empty() {
            continue;
        }
        let active = select_active_package(world, actor, game_hour, packages.iter().copied());
        let active_package_form_id = active.map(|package| package.form_id);
        let behavior = active.and_then(|package| {
            let template = package
                .package_template_form_id
                .and_then(|form_id| registry.package(form_id))
                .unwrap_or(package);
            AmbientBehavior::from_package(package, template, runtime.actor_form_id)
        });
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
        PackDataInput, PackLocation, PackProcedure, PackSchedule, PROCEDURE_GUARD,
        PROCEDURE_SANDBOX, PROCEDURE_TRAVEL, PROCEDURE_WANDER,
    };
    use byroredux_plugin::esm::records::{AliasInjectedData, SceneActionType};
    use byroredux_scripting::quest_stages::QuestStageState;
    use byroredux_scripting::{
        install_package_records, scene_package_system, QuestFormId, SceneEvent, SceneEventBatch,
        ScenePackageEvent, ScenePackageEventBatch, ScenePlayer,
    };

    fn register_runtime(world: &mut World, hour: f32) {
        byroredux_scripting::register(world);
        world.register::<AmbientPackageRuntime>();
        world.register::<Dead>();
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

    /// Real-data-shaped Skyrim+ type-19 template: a `Travel`-then-`Sandbox`
    /// tree, mirroring `Skyrim.esm`'s own `Sandbox` template (verified
    /// before this test was written; see `docs/engine/packal.md` §3). The
    /// `Sandbox` leaf's `data_input_indexes` name slot 0 (the shared
    /// Location every leaf in the tree travels to / sandboxes around).
    fn skyrim_sandbox_template(form_id: u32) -> PackRecord {
        PackRecord {
            form_id,
            procedures: vec![
                PackProcedure {
                    procedure_type: "Travel".to_owned(),
                    data_input_indexes: vec![0],
                    ..Default::default()
                },
                PackProcedure {
                    procedure_type: "Sandbox".to_owned(),
                    data_input_indexes: vec![0],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    /// A template *instance* — what an NPC's PKID list actually names.
    /// Carries no procedures of its own, just the template ref + concrete
    /// data-input values (slot 0 = the Location the template's leaves
    /// reference), mirroring `DefaultSandboxEditorLocation512`.
    fn skyrim_sandbox_instance(form_id: u32, template_form_id: u32, radius: i32) -> PackRecord {
        PackRecord {
            form_id,
            package_template_form_id: Some(template_form_id),
            data_inputs: vec![PackDataInput {
                index: 0,
                value_type: "Location".to_owned(),
                value: PackDataValue::Location(PackLocation {
                    location_type: 3,
                    target: PackLocationTarget::Other(0),
                    radius,
                }),
            }],
            ..Default::default()
        }
    }

    /// A single-leaf Skyrim+ Patrol template, mirroring the real master
    /// `Patrol` template (form 0x017723): one `"Patrol"` procedure with a
    /// `SingleRef` target input at slot 0 and a `Float` radius at slot 1
    /// (real per-NPC instances resolve slot 1 to 50.0–150.0; slot 0 is a
    /// route-anchor reference this v0 driver doesn't resolve).
    fn skyrim_patrol_template(form_id: u32) -> PackRecord {
        PackRecord {
            form_id,
            procedures: vec![PackProcedure {
                procedure_type: "Patrol".to_owned(),
                data_input_indexes: vec![1],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// A template *instance* — what an NPC's PKID list actually names.
    /// Slot 1 = the `Float` radius the template's `"Patrol"` leaf
    /// references, mirroring a real per-NPC Patrol package.
    fn skyrim_patrol_instance(form_id: u32, template_form_id: u32, radius: f32) -> PackRecord {
        PackRecord {
            form_id,
            package_template_form_id: Some(template_form_id),
            data_inputs: vec![PackDataInput {
                index: 1,
                value_type: "Float".to_owned(),
                value: PackDataValue::Float(radius),
            }],
            ..Default::default()
        }
    }

    fn setup_actor(hour: f32, packages: Vec<PackRecord>) -> (World, EntityId) {
        setup_actor_with_catalog(hour, packages, Vec::new())
    }

    /// Like [`setup_actor`], but `catalog_only` packages land in the package
    /// catalog (so `package_template_form_id` chasing can find them) without
    /// being listed on the actor's own PKID candidates — mirroring how a
    /// real Skyrim+ NPC's PKID list names a concrete template *instance* but
    /// never the type-19 template its procedure tree actually lives on.
    fn setup_actor_with_catalog(
        hour: f32,
        ai_packages: Vec<PackRecord>,
        catalog_only: Vec<PackRecord>,
    ) -> (World, EntityId) {
        let mut world = World::new();
        register_runtime(&mut world, hour);
        let actor = world.spawn();
        let candidates: Vec<u32> = ai_packages.iter().map(|package| package.form_id).collect();
        let npc = NpcRecord {
            form_id: 0xAA,
            ai_packages: candidates,
            ..Default::default()
        };
        let mut all_packages = ai_packages;
        all_packages.extend(catalog_only);
        let mut index = EsmIndex::default();
        for package in all_packages.iter().cloned() {
            index.packages.insert(package.form_id, package);
        }
        apply_ai_package_behavior(&mut world, actor, &npc, &index);
        install_package_records(&mut world, all_packages);
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
        world.query_mut::<Seated>().unwrap().insert(
            actor,
            Seated {
                furniture,
                animation_restore: Default::default(),
            },
        );
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

    /// #3353 — the minute gate must be reached before any per-actor work.
    /// `setup_actor` already ran one evaluation, stamping
    /// `last_evaluated_game_minute`; a second call at the same game hour must
    /// therefore short-circuit even when a re-evaluation *would* pick a new
    /// winner. Pre-fix every actor's candidate `Vec` was heap-cloned, and its
    /// `Dead` / `EvaluatePackageRequest` locks taken, before this point — on
    /// ~119 of every 120 frames at the default `time_scale`.
    #[test]
    fn second_evaluation_in_the_same_game_minute_is_gated_out() {
        let (mut world, actor) = setup_actor(10.0, vec![pack(0x100, PROCEDURE_WANDER, None)]);
        assert!(world.has::<WanderBehavior>(actor));
        let stamped = world
            .get::<AmbientPackageRuntime>(actor)
            .unwrap()
            .last_evaluated_game_minute;
        assert!(stamped.is_some(), "setup must have stamped the minute");

        // Overlay a package that would win outright on a fresh evaluation.
        install_package_records(&mut world, [pack(0x200, PROCEDURE_TRAVEL, None)]);
        let mut injected = AliasInjectedData::default();
        injected.packages.push(0x200);
        world.insert(
            actor,
            QuestAliasInjectedOverlays([((QuestFormId(0x900), 1), injected)].into_iter().collect()),
        );

        ambient_ai_package_system(&world, 0.0);

        assert!(
            world.has::<WanderBehavior>(actor),
            "the minute gate must suppress re-selection (#3353)"
        );
        assert!(!world.has::<TravelBehavior>(actor));
        assert_eq!(
            world
                .get::<AmbientPackageRuntime>(actor)
                .unwrap()
                .active_package_form_id,
            Some(0x100)
        );

        // …and an explicit request still gets through the same gate.
        world
            .query_mut::<EvaluatePackageRequest>()
            .unwrap()
            .insert(actor, EvaluatePackageRequest);
        ambient_ai_package_system(&world, 0.0);
        assert!(
            world.has::<TravelBehavior>(actor),
            "EvaluatePackageRequest must bypass the minute gate (#3353)"
        );
        assert!(!world.has::<WanderBehavior>(actor));
    }

    /// #3333 — un-seating must undo the animation park, not just drop the
    /// marker. `sandbox_seat_system` pins a seated actor on the sit-enter
    /// clip's final frame with `playing = false` (its Reverse cycle would
    /// otherwise ping-pong back to standing) and nothing else writes an NPC's
    /// `AnimationPlayer` after spawn. Before M42.9 that was unreachable —
    /// `SandboxBehavior` was attached once and never removed — but
    /// `ambient_ai_package_system` now swaps behaviors on a schedule
    /// handover, so the actor walked its next package in a frozen chair pose
    /// and never animated again for the rest of the session.
    #[test]
    fn unseating_restores_the_pre_seat_animation_player() {
        use byroredux_core::animation::AnimationPlayer;
        use byroredux_core::ecs::components::SeatedAnimationRestore;

        let mut world = World::new();
        world.register::<AnimationPlayer>();
        world.register::<Seated>();
        world.register::<SandboxBehavior>();
        let actor = world.spawn();

        // The actor's real idle, as spawn left it.
        let idle = AnimationPlayer {
            clip_handle: 7,
            local_time: 0.4,
            prev_time: 0.3,
            playing: true,
            speed: 1.05,
            reverse_direction: false,
            root_entity: None,
            last_delta: 0.0,
        };
        let (idle_clip, idle_local, idle_prev, idle_speed) = (
            idle.clip_handle,
            idle.local_time,
            idle.prev_time,
            idle.speed,
        );
        world.insert(actor, idle);
        let furniture = world.spawn();
        world.insert(
            actor,
            Seated {
                furniture,
                animation_restore: SeatedAnimationRestore {
                    clip_handle: idle_clip,
                    local_time: idle_local,
                    prev_time: idle_prev,
                    playing: true,
                    speed: idle_speed,
                },
            },
        );
        // …then the seat park, verbatim as `sandbox_seat_system` writes it.
        {
            let mut pq = world.query_mut::<AnimationPlayer>().unwrap();
            let p = pq.get_mut(actor).unwrap();
            p.clip_handle = 42;
            p.local_time = 2.5;
            p.prev_time = 2.5;
            p.playing = false;
            p.speed = 1.0;
        }

        clear_ambient_behavior(&world, actor);

        let restored = world.get::<AnimationPlayer>(actor).unwrap();
        assert_eq!(restored.clip_handle, 7, "idle clip must come back");
        assert!(restored.playing, "a walking actor must not stay frozen");
        assert_eq!(restored.local_time, 0.4);
        assert_eq!(restored.prev_time, 0.3);
        assert_eq!(restored.speed, 1.05);
        drop(restored);
        assert!(!world.has::<Seated>(actor));
    }

    /// The restore must be scoped to actors that were actually seated —
    /// `clear_ambient_behavior` runs on every package handover, including for
    /// the six locomotion procedures that never touch `AnimationPlayer`.
    #[test]
    fn unseating_does_not_touch_an_actor_that_was_never_seated() {
        use byroredux_core::animation::AnimationPlayer;

        let mut world = World::new();
        world.register::<AnimationPlayer>();
        world.register::<Seated>();
        world.register::<WanderBehavior>();
        let actor = world.spawn();
        let mid_clip = AnimationPlayer {
            clip_handle: 3,
            local_time: 1.25,
            prev_time: 1.2,
            playing: true,
            speed: 0.97,
            reverse_direction: true,
            root_entity: None,
            last_delta: 0.0,
        };
        world.insert(actor, mid_clip);

        clear_ambient_behavior(&world, actor);

        let after = world.get::<AnimationPlayer>(actor).unwrap();
        // No `Seated` means nothing to restore — the player must be untouched.
        assert_eq!(after.clip_handle, 3);
        assert_eq!(after.local_time, 1.25);
        assert_eq!(after.prev_time, 1.2);
        assert!(after.playing);
        assert_eq!(after.speed, 0.97);
        assert!(after.reverse_direction);
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
    fn dead_actor_never_reinstalls_ambient_behavior() {
        let (mut world, actor) = setup_actor(10.0, vec![pack(0x100, PROCEDURE_WANDER, None)]);
        clear_ambient_behavior(&world, actor);
        world.insert(actor, Dead);
        world
            .query_mut::<AmbientPackageRuntime>()
            .unwrap()
            .get_mut(actor)
            .unwrap()
            .active_package_form_id = None;

        ambient_ai_package_system(&world, 0.0);

        assert!(!world.has::<WanderBehavior>(actor));
    }

    #[test]
    fn quest_alias_package_overrides_actor_base_package_stack() {
        let wander = pack(0x100, PROCEDURE_WANDER, None);
        let travel = pack(0x200, PROCEDURE_TRAVEL, None);
        let (mut world, actor) = setup_actor(10.0, vec![wander]);
        install_package_records(&mut world, [travel]);
        let mut injected = AliasInjectedData::default();
        injected.packages.push(0x200);
        world.insert(
            actor,
            QuestAliasInjectedOverlays([((QuestFormId(0x900), 1), injected)].into_iter().collect()),
        );
        world
            .query_mut::<EvaluatePackageRequest>()
            .unwrap()
            .insert(actor, EvaluatePackageRequest);

        ambient_ai_package_system(&world, 0.0);

        assert!(world.has::<TravelBehavior>(actor));
        assert!(!world.has::<WanderBehavior>(actor));
        assert_eq!(
            world
                .get::<AmbientPackageRuntime>(actor)
                .unwrap()
                .active_package_form_id,
            Some(0x200)
        );
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

    /// PACKAL first slice (`docs/engine/packal.md` §4). The actor's PKID
    /// list names only the concrete instance — real Skyrim data never puts
    /// a template FormID on an NPC's own package list.
    #[test]
    fn skyrim_shaped_sandbox_resolves_search_radius_via_template() {
        let template = skyrim_sandbox_template(0x300);
        let instance = skyrim_sandbox_instance(0x301, 0x300, 512);
        let (world, actor) = setup_actor_with_catalog(10.0, vec![instance], vec![template]);

        assert!(world.has::<SandboxBehavior>(actor));
        assert_eq!(
            world.get::<SandboxBehavior>(actor).unwrap().search_radius,
            Some(512.0)
        );
    }

    /// A Skyrim+ package whose template has no recognized leaf type (only
    /// procedures this driver doesn't dispatch yet) installs no ambient
    /// behavior at all — silent no-op, not a wrong guess. Mirrors the
    /// FO3/FNV branch's existing behavior for procedure types this driver
    /// doesn't handle (Find/Eat/Sleep/…).
    #[test]
    fn skyrim_shaped_package_with_no_recognized_leaf_installs_nothing() {
        let template = PackRecord {
            form_id: 0x300,
            procedures: vec![PackProcedure {
                procedure_type: "UnlockDoors".to_owned(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let instance = PackRecord {
            form_id: 0x301,
            package_template_form_id: Some(0x300),
            ..Default::default()
        };
        let (world, actor) = setup_actor_with_catalog(10.0, vec![instance], vec![template]);

        assert!(!world.has::<SandboxBehavior>(actor));
    }

    /// A Skyrim+ Sandbox leaf whose referenced data-input slot resolves to
    /// nothing `Location`-typed (or the template ref is absent entirely —
    /// covered by the flat-shape tests already) still installs
    /// `SandboxBehavior`; `search_radius: None` is a legitimate value that
    /// `sandbox_seat_system` already treats as "use the default radius",
    /// same as the FO3/FNV no-PLDT case.
    #[test]
    fn skyrim_shaped_sandbox_with_no_location_input_installs_default_radius() {
        let template = skyrim_sandbox_template(0x300);
        let instance = PackRecord {
            form_id: 0x301,
            package_template_form_id: Some(0x300),
            ..Default::default()
        };
        let (world, actor) = setup_actor_with_catalog(10.0, vec![instance], vec![template]);

        assert!(world.has::<SandboxBehavior>(actor));
        assert_eq!(
            world.get::<SandboxBehavior>(actor).unwrap().search_radius,
            None
        );
    }

    /// A package with no `package_template_form_id` at all (neither
    /// FO3/FNV-flat nor Skyrim+-templated — an unusual but possible
    /// one-off custom procedure tree per `ai_packages_procedures.md`)
    /// still resolves through its own `procedures`, matching
    /// `resolve_command`'s identical `template = package_template_form_id
    /// .and_then(...).unwrap_or(package)` fallback in the Scene driver.
    #[test]
    fn skyrim_shaped_one_off_package_with_no_template_ref_resolves_its_own_procedures() {
        let one_off = PackRecord {
            form_id: 0x301,
            procedures: vec![PackProcedure {
                procedure_type: "Sandbox".to_owned(),
                data_input_indexes: vec![0],
                ..Default::default()
            }],
            data_inputs: vec![PackDataInput {
                index: 0,
                value_type: "Location".to_owned(),
                value: PackDataValue::Location(PackLocation {
                    location_type: 3,
                    target: PackLocationTarget::Other(0),
                    radius: 256,
                }),
            }],
            ..Default::default()
        };
        let (world, actor) = setup_actor(10.0, vec![one_off]);

        assert!(world.has::<SandboxBehavior>(actor));
        assert_eq!(
            world.get::<SandboxBehavior>(actor).unwrap().search_radius,
            Some(256.0)
        );
    }

    /// PACKAL second slice (`docs/engine/packal.md` §5) — a Skyrim+
    /// Patrol leaf resolves its radius from the first `Float` input,
    /// unlike Sandbox's `Location`-wrapped radius.
    #[test]
    fn skyrim_shaped_patrol_resolves_radius_via_template() {
        let template = skyrim_patrol_template(0x400);
        let instance = skyrim_patrol_instance(0x401, 0x400, 150.0);
        let (world, actor) = setup_actor_with_catalog(10.0, vec![instance], vec![template]);

        assert!(world.has::<PatrolBehavior>(actor));
        assert_eq!(
            world.get::<PatrolBehavior>(actor).unwrap().patrol_radius,
            Some(150.0)
        );
        assert_eq!(world.get::<PatrolBehavior>(actor).unwrap().form_id, 0xAA);
    }

    /// A Skyrim+ Patrol leaf with no resolvable `Float` input still
    /// installs `PatrolBehavior`; `patrol_radius: None` is a legitimate
    /// value `patrol_system` already treats as "use the default radius",
    /// same as Sandbox's no-Location case.
    #[test]
    fn skyrim_shaped_patrol_with_no_float_input_installs_default_radius() {
        let template = skyrim_patrol_template(0x400);
        let instance = PackRecord {
            form_id: 0x401,
            package_template_form_id: Some(0x400),
            ..Default::default()
        };
        let (world, actor) = setup_actor_with_catalog(10.0, vec![instance], vec![template]);

        assert!(world.has::<PatrolBehavior>(actor));
        assert_eq!(
            world.get::<PatrolBehavior>(actor).unwrap().patrol_radius,
            None
        );
    }

    /// Known limitation, pinned deliberately (`packal.md` §5): a template
    /// whose tree contains *both* a `"Sandbox"` and a `"Patrol"` leaf (a
    /// real shape — `DefaultMasterPackageAllowWander` has 2×Sandbox +
    /// 4×Patrol in one 10-entry tree) resolves Sandbox, matching the
    /// priority order `from_package`'s FO3/FNV `is_*` chain already uses.
    /// Neither leaf's `conditions` are evaluated — this is a `.find()`,
    /// not a condition-gated tree walk.
    #[test]
    fn skyrim_shaped_template_with_both_sandbox_and_patrol_prefers_sandbox() {
        let template = PackRecord {
            form_id: 0x400,
            procedures: vec![
                PackProcedure {
                    procedure_type: "Patrol".to_owned(),
                    data_input_indexes: vec![1],
                    ..Default::default()
                },
                PackProcedure {
                    procedure_type: "Sandbox".to_owned(),
                    data_input_indexes: vec![0],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let instance = PackRecord {
            form_id: 0x401,
            package_template_form_id: Some(0x400),
            data_inputs: vec![
                PackDataInput {
                    index: 0,
                    value_type: "Location".to_owned(),
                    value: PackDataValue::Location(PackLocation {
                        location_type: 3,
                        target: PackLocationTarget::Other(0),
                        radius: 200,
                    }),
                },
                PackDataInput {
                    index: 1,
                    value_type: "Float".to_owned(),
                    value: PackDataValue::Float(150.0),
                },
            ],
            ..Default::default()
        };
        let (world, actor) = setup_actor_with_catalog(10.0, vec![instance], vec![template]);

        assert!(world.has::<SandboxBehavior>(actor));
        assert!(!world.has::<PatrolBehavior>(actor));
        assert_eq!(
            world.get::<SandboxBehavior>(actor).unwrap().search_radius,
            Some(200.0)
        );
    }

    /// PACKAL first slice (`docs/engine/packal.md`), verified against the
    /// actual production code path (`apply_ai_package_behavior`), not a
    /// re-implementation. Opt-in — needs a real Skyrim SE install — mirrors
    /// `cell_loader::load_order`'s `real_skyrim_load_order_...` test's
    /// skip-if-unavailable convention. Run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn real_skyrim_esm_ambient_packages_now_resolve_for_previously_blind_npcs() {
        let data = std::env::var("BYROREDUX_SKYRIM_DATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(
                    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
                )
            });
        if !data.is_dir() {
            eprintln!("[PACKAL Skyrim ambient] skipping: game data unavailable");
            return;
        }
        let master = data.join("Skyrim.esm");
        let bytes = std::fs::read(&master).expect("read Skyrim.esm");
        let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse Skyrim.esm");

        let mut world = World::new();
        register_runtime(&mut world, 10.0);

        let mut with_packages = 0usize;
        let mut resolved_any_behavior = 0usize;
        let mut resolved_sandbox_with_radius = 0usize;
        let mut resolved_patrol_with_radius = 0usize;
        for npc in index.npcs.values() {
            if npc.ai_packages.is_empty() {
                continue;
            }
            with_packages += 1;
            let actor = world.spawn();
            apply_ai_package_behavior(&mut world, actor, npc, &index);
            if world.has::<SandboxBehavior>(actor) {
                resolved_any_behavior += 1;
                if world
                    .get::<SandboxBehavior>(actor)
                    .is_some_and(|behavior| behavior.search_radius.is_some())
                {
                    resolved_sandbox_with_radius += 1;
                }
            } else if world.has::<PatrolBehavior>(actor) {
                resolved_any_behavior += 1;
                if world
                    .get::<PatrolBehavior>(actor)
                    .is_some_and(|behavior| behavior.patrol_radius.is_some())
                {
                    resolved_patrol_with_radius += 1;
                }
            } else if world.has::<WanderBehavior>(actor)
                || world.has::<TravelBehavior>(actor)
                || world.has::<FollowBehavior>(actor)
                || world.has::<EscortBehavior>(actor)
                || world.has::<GuardBehavior>(actor)
            {
                resolved_any_behavior += 1;
            }
        }

        println!(
            "[PACKAL Skyrim ambient] {with_packages} NPCs carry a PKID list; \
             {resolved_any_behavior} now resolve some ambient behavior \
             ({resolved_sandbox_with_radius} a Sandbox with a real radius, \
             {resolved_patrol_with_radius} a Patrol with a real radius)"
        );
        // `pack_ambient_shape_survey` measured ~2052 package-carrying NPCs
        // and Sandbox-family templates alone covering 300+ each on real
        // Skyrim.esm (§3, docs/engine/packal.md) — 500 is a safe floor well
        // below the measured population, not a tight bound on it.
        assert!(
            resolved_sandbox_with_radius > 500,
            "expected the large majority of real Skyrim.esm's package-carrying \
             NPCs to resolve a Skyrim+ Sandbox leaf with a real radius — got \
             {resolved_sandbox_with_radius} of {with_packages}"
        );
        // The real `Patrol` master template (form 0x017723) alone covers
        // 701 ambient PKID edges (§5, docs/engine/packal.md) — floor sits
        // well below that to absorb NPCs whose Patrol package resolves no
        // Float input, not a tight bound.
        assert!(
            resolved_patrol_with_radius > 200,
            "expected a real chunk of Skyrim.esm's package-carrying NPCs to \
             resolve a Skyrim+ Patrol leaf with a real radius — got {resolved_patrol_with_radius} \
             of {with_packages}"
        );
    }
}
