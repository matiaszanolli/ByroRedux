//! Ambient AI-package selection and behavior installation (M42.1–M42.9).
//!
//! Spawn still installs the first behavior immediately, but the actor also
//! retains its PKID candidate list in [`AmbientPackageRuntime`]. The runtime
//! system reuses the same selector at game-minute boundaries and on Papyrus
//! [`EvaluatePackageRequest`], replacing behavior state only when the winning
//! PACK FormID actually changes.

use super::EsmIndex;
use crate::components::{AmbientPackageRuntime, GameTimeRes, NavPath, SeatReservations};
use byroredux_core::animation::AnimationPlayer;
use byroredux_core::ecs::components::{
    AmbushBehavior, Dead, FindItemKind, FindPackageBehavior, FindProgress, FindTarget, EatBehavior, EatSleepLocation, EatSleepState, EscortBehavior, EscortState, Escorted,
    FollowBehavior,
    FleePackageBehavior, FollowState, GuardBehavior, GuardState, PatrolBehavior, PatrolState, SandboxBehavior, Seated,
    SleepBehavior, TravelBehavior, TravelState, Traveled, UseWeaponPackageBehavior,
    UseWeaponPackageStarted, WanderBehavior, WanderState,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, World};
use byroredux_plugin::esm::records::misc::pack::{
    PROCEDURE_DIALOGUE, PROCEDURE_EAT, PROCEDURE_SLEEP, PROCEDURE_USE_WEAPON,
};
use byroredux_plugin::esm::records::{
    PackDataValue, PackLocation, PackLocationTarget, PackRecord, PackTargetKind,
};
use byroredux_scripting::condition::ConditionContext;
use byroredux_scripting::{
    AiCombatState, EvaluatePackageRequest, PackageRegistry, QuestAliasInjectedOverlays,
};

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
    use byroredux_scripting::condition::evaluate;

    if conditions.is_empty() {
        return true;
    }
    let all_known = conditions
        .iter()
        .all(|condition| byroredux_scripting::condition::is_modeled(world, condition));
    if !all_known {
        return true;
    }
    evaluate(conditions, world, ctx)
}

/// #5376 — whether a package's whole condition list could be evaluated
/// (no uncatalogued function anywhere). Distinct from
/// [`package_conditions_pass`]'s fail-open: that is the *selection*
/// answer ("may this package run?"), this is the *confidence* answer
/// ("was the selection decided on modeled terms?"). An intrusive
/// procedure — one that takes control of the actor toward the player —
/// must not install from a pass that fail-open manufactured: 56 of the
/// 60 FNV NPC bases whose first eligible package is a Dialogue package
/// pass only because a condition (fn 79 GetQuestVariable, 59
/// occurrences) is uncatalogued, so vanilla's quest-gated force-greets
/// fired on first contact regardless of quest state, and again after
/// every load and every combat.
fn package_conditions_fully_modeled(
    conditions: &byroredux_plugin::esm::records::condition::ConditionList,
    world: &World,
) -> bool {
    conditions
        .iter()
        .all(|condition| byroredux_scripting::condition::is_modeled(world, condition))
}

/// Whether a flat Dialogue package can install its ambient player-facing
/// conversation bridge. Keep this gate shared by package selection and
/// installation so a declined package cannot starve lower scheduled work.
fn dialogue_installs_ambient(package: &PackRecord, world: &World) -> bool {
    let targets_player = match package.target.map(|target| target.target) {
        None => true,
        Some(PackTargetKind::SpecificReference(0x0000_0014)) => true,
        Some(_) => false,
    };

    package_conditions_fully_modeled(&package.conditions, world)
        && targets_player
        && package.dialogue_type == 0
}

/// What a UseWeapon package installs, or `None` when it must not install:
///
/// - A package whose conditions could not all be evaluated (the same
///   fail-open hazard as Dialogue, #5376): its gates decide *when* the actor
///   opens fire, so a fail-open pass would have the Hoover Dam snipers
///   shooting the player on cell entry.
/// - A package with no specific weapon (`PTDT` type 1) or no specific
///   target reference (`PTD2` type 0): weapon classes are not decoded (the
///   object-type enum's numbering contradicts the vanilla data), and a
///   target-less package fires into the air.
fn use_weapon_behavior(package: &PackRecord, world: &World) -> Option<UseWeaponPackageBehavior> {
    if !package_conditions_fully_modeled(&package.conditions, world) {
        return None;
    }
    let fire = package.use_weapon?;
    let PackTargetKind::ObjectId(weapon_form_id) = package.target?.target else {
        return None;
    };
    let PackTargetKind::SpecificReference(target_form_id) = package.target2?.target else {
        return None;
    };
    Some(UseWeaponPackageBehavior {
        weapon_form_id,
        target_form_id,
        bursts: (!fire.repeat_fire).then(|| u32::from(fire.bursts).max(1)),
        // Volley fire picks a size in [min, max] each time; the midpoint is
        // the deterministic stand-in. Auto fire is `0`: a burst is a clip.
        volley_shots: if fire.volley_fire {
            (u32::from(fire.volley_min) + u32::from(fire.volley_max)).div_ceil(2).max(1)
        } else {
            0
        },
        harmless: fire.does_no_damage(),
    })
}

/// What a Find package installs, or `None` when it must not install:
///
/// - A package whose conditions could not all be evaluated: Find takes the
///   actor over to hunt, take or greet, so a fail-open pass (#5376) would send
///   it off on content the quest has not reached.
/// - A package with no `PTDT`, or one searching an object type the item
///   records cannot classify (only Ammo 13, Alchemy 17 and Food 18 are; the
///   enum's weapon classes are contradicted by the data and the rest have no
///   record to read).
fn find_behavior(package: &PackRecord, world: &World) -> Option<FindPackageBehavior> {
    if !package_conditions_fully_modeled(&package.conditions, world) {
        return None;
    }
    let target = package.target?;
    let (target_kind, count) = match target.target {
        PackTargetKind::SpecificReference(form_id) => (FindTarget::Reference(form_id), 1),
        PackTargetKind::ObjectId(base) => (FindTarget::Base(base), target.count_or_distance.max(1) as u32),
        PackTargetKind::Other(_) if target.target_type == 2 => {
            let PackTargetKind::Other(object_type) = target.target else { return None };
            // The object-type numbers are FO3/FNV's: Oblivion numbers the same
            // enum differently (there 13 is Weapons, 17 Soul Gems, 18 Keys), so
            // another game's type search would be misread, not just missed.
            let game = world
                .try_resource::<crate::cell_loader::LoadedCellIndex>()
                .map(|index| index.0.game);
            if game != Some(byroredux_plugin::esm::reader::GameKind::Fallout3NV) {
                return None;
            }
            let kind = match object_type {
                13 => FindItemKind::Ammo,
                17 => FindItemKind::Alchemy,
                18 => FindItemKind::Food,
                _ => return None,
            };
            (FindTarget::Kind(kind), target.count_or_distance.max(1) as u32)
        }
        PackTargetKind::Other(_) => return None,
    };
    Some(FindPackageBehavior {
        target: target_kind,
        count,
        area: location_anchor(package.location),
        area_radius: package.location.map_or(0.0, |area| area.radius.max(0) as f32),
    })
}

/// Whether selection may pick `package`: a Dialogue-procedure package must be
/// one the installer would take (#5499 / #5722), and so must a UseWeapon one;
/// every other procedure passes. Shared by the schedule scan and the
/// script-package slot so a scripted Say-To / off-target Dialogue package
/// cannot win and then install nothing (#6051).
fn selectable_by_installer(package: &PackRecord, world: &World) -> bool {
    // Find first, and by `is_find`: procedure 0 is also every Skyrim+ package's
    // unset byte, which `is_find` tells apart by shape.
    if package.is_find() {
        return find_behavior(package, world).is_some();
    }
    match package.procedure_type {
        PROCEDURE_DIALOGUE => dialogue_installs_ambient(package, world),
        PROCEDURE_USE_WEAPON => use_weapon_behavior(package, world).is_some(),
        _ => true,
    }
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
    /// M42 — Eat (procedure 3): walk to the PLDT dining location, then
    /// sit at the nearest furniture marker.
    Eat {
        radius: Option<f32>,
        location: EatSleepLocation,
        actor_form_id: u32,
    },
    /// M42 — Sleep (procedure 4): walk to the PLDT bedroom location,
    /// then occupy the nearest sleep marker (sit-pose v0).
    Sleep {
        radius: Option<f32>,
        location: EatSleepLocation,
        actor_form_id: u32,
    },
    /// M42 — FleeNotCombat (procedure 10): keep `min_distance` from the
    /// `PTDT` target, or hold at the `PLDT` refuge; `flee_package_system`.
    Flee {
        threat_form_id: Option<u32>,
        min_distance: Option<f32>,
        refuge_form_id: Option<u32>,
    },
    /// M42 — Ambush (procedure 9): wait at the `PLDT` location on Guard's
    /// runtime, and fight only an enemy inside the `PLD2` ambush location
    /// (the hostility pass reads `ambush`).
    Ambush {
        wait_form_id: Option<u32>,
        wait_radius: Option<f32>,
        ambush: AmbushBehavior,
        actor_form_id: u32,
    },
    /// M42 — Find (procedure 0): `find_package_system` hunts the target.
    Find(FindPackageBehavior),
    /// M42 — UseWeapon (procedure 16): `use_weapon_package_system` raises
    /// the `UseWeaponRequest` for it once.
    UseWeapon(UseWeaponPackageBehavior),
    /// M42 + #5367 — Dialogue (procedure 15): the active package
    /// installs the force-greet bridge; `forcegreet_system` does the
    /// approach + open. Consumed on the first greet — re-installed only
    /// when a different package wins and this one wins back.
    Dialogue {
        topic: Option<u32>,
    },
}

/// #5391 — an Eat/Sleep package's `PLDT` as the anchor the runtime walks
/// to and searches around. Types with no resolvable anchor here stay
/// where the actor stands (the old fallback hash-picked a random point
/// around the actor's *current* position for every non-reference type,
/// so "near editor location" followed wherever an earlier package had
/// walked it and re-rolled after every load).
fn eat_sleep_location(package: &PackRecord) -> EatSleepLocation {
    location_anchor(package.location)
}

/// A `PLDT`/`PLD2` as the anchor a runtime resolves (see [`eat_sleep_location`]).
fn location_anchor(location: Option<PackLocation>) -> EatSleepLocation {
    match location {
        Some(location) => match location.target {
            PackLocationTarget::NearReference(form_id) => EatSleepLocation::NearReference(form_id),
            PackLocationTarget::InCell(form_id) => EatSleepLocation::InCell(form_id),
            PackLocationTarget::Other(_) if location.location_type == 3 => {
                EatSleepLocation::NearEditorLocation
            }
            // #5500 — type 6 (Linked Ref) is resolvable: the actor's own
            // XLKR edge, held by `PackageTargetRegistry`.
            PackLocationTarget::Other(_) if location.location_type == 6 => {
                EatSleepLocation::NearLinkedReference
            }
            _ => EatSleepLocation::NearCurrentLocation,
        },
        None => EatSleepLocation::NearCurrentLocation,
    }
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
        world: &World,
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
        } else if package.is_follow() || package.is_accompany() {
            // Accompany authors Follow's data (PTDT reference + distance) and
            // the GECK calls it "similar to a Follow"; it differs by pathing
            // to the target's destination rather than trailing it. That
            // lead is not modelled, so it runs as the Follow it resembles.
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
        } else if package.is_flee_not_combat() {
            Some(Self::Flee {
                threat_form_id: target_reference,
                min_distance: target_distance,
                refuge_form_id: location_reference,
            })
        } else if package.is_ambush() {
            Some(Self::Ambush {
                wait_form_id: location_reference,
                wait_radius: location_radius,
                ambush: AmbushBehavior {
                    zone: location_anchor(package.location2),
                    zone_radius: package
                        .location2
                        .map_or(0.0, |zone| zone.radius.max(0) as f32),
                    target_form_id: target_reference,
                },
                actor_form_id,
            })
        } else if package.is_find() {
            find_behavior(package, world).map(Self::Find)
        } else if package.is_use_weapon() {
            use_weapon_behavior(package, world).map(Self::UseWeapon)
        } else if package.is_patrol() {
            Some(Self::Patrol {
                patrol_radius: location_radius,
                actor_form_id,
            })
        } else if package.procedure_type == PROCEDURE_EAT || package.is_use_item_at() {
            // UseItemAt's vanilla use is sitting: go to the PLDT location and
            // sit at the nearest seat marker, which is what Eat does. The
            // PTDT item is not read - an item that is not furniture (an
            // inventory item's animation) leaves the actor standing at the
            // location, the GECK's "animation (if available)" with none.
            Some(Self::Eat {
                radius: location_radius,
                location: eat_sleep_location(package),
                actor_form_id,
            })
        } else if package.procedure_type == PROCEDURE_SLEEP {
            Some(Self::Sleep {
                radius: location_radius,
                location: eat_sleep_location(package),
                actor_form_id,
            })
        } else if package.procedure_type == PROCEDURE_DIALOGUE {
            // #5367 Phase F's bridge, now driven by ambient selection —
            // but only from a fully-modeled, player-targeted
            // Conversation (#5376). The ambient selector fails
            // condition lists open (any uncatalogued function passes
            // the package), which used to arm a player-control
            // takeover from terms the evaluator never ran: 60 FNV NPC
            // bases force-greeted on first contact whatever the quest
            // state, again after every load and every combat. Three
            // gates now, all cheap and all corpus-shaped:
            //   - fully modeled: the fail-open pass does not install;
            //   - PTDT targets the player (or no target at all —
            //     4 of 141 FNV refs name a non-player reference, and
            //     FO3 is far worse at 179 of 365; those greets speak
            //     to their authored reference, not to the player);
            //   - PKDD Dialogue Type is Conversation (Say To is one
            //     spoken line with no menu).
            // The `dialogue.forcegreet` console door keeps installing
            // any Dialogue package unconditionally — it is the
            // drivable installer for gates and fixtures, not an
            // ambient behavior (#5367 Phase F's design).
            // The Skyrim tree dialect is NOT wired here — #5511's
            // census: 41 ForceGreet-tree packages sit on NPC_ `PKID`
            // default lists (87 NPC_ edges — VigilantofStendarr's
            // Daedric greeting ×39, the cart drivers ×6, Paarthurnax,
            // the Whiterun housecarls …), so unlike the FO3/FNV arm
            // there IS a default-list population to serve. Installing
            // them is deferred until the instance's condition lists
            // are modeled — the same #5376 fail-open gate this arm
            // already applies (an uncatalogued condition function
            // must not arm a player-control takeover).
            // The player reference is the literal 0x00000014 (never
            // plugin-local — Bethesda's special player REFR), so the
            // static check is exact: a PTDT that names anything else
            // (or names an object/other kind) greets its authored
            // reference, not the player.
            if dialogue_installs_ambient(package, world) {
                Some(Self::Dialogue {
                    topic: package.dialogue_topic,
                })
            } else {
                log::debug!(
                    "#5376: Dialogue package {} ('{}') declined ambient install \
                     (modeled={}, targets_player={}, dialogue_type={})",
                    package.form_id,
                    package.editor_id,
                    package_conditions_fully_modeled(&package.conditions, world),
                    matches!(
                        package.target.map(|target| target.target),
                        None | Some(PackTargetKind::SpecificReference(0x0000_0014))
                    ),
                    package.dialogue_type,
                );
                None
            }
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

    /// Install this behavior's component(s) on `actor` through `sink`. The one
    /// variant match: a new procedure edits this arm and the remover list below
    /// (#6361).
    fn install(self, sink: &mut impl ComponentSink, actor: EntityId) {
        match self {
            Self::Sandbox { search_radius } => {
                sink.put(actor, SandboxBehavior { search_radius });
            }
            Self::Wander {
                wander_radius,
                actor_form_id,
            } => {
                sink.put(actor,
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
                sink.put(actor,
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
                sink.put(actor,
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
                sink.put(actor,
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
                sink.put(actor,
                    GuardBehavior {
                        anchor_form_id,
                        radius,
                        form_id: actor_form_id,
                    },
                );
            }
            Self::Ambush {
                wait_form_id,
                wait_radius,
                ambush,
                actor_form_id,
            } => {
                sink.put(actor,
                    GuardBehavior {
                        anchor_form_id: wait_form_id,
                        radius: wait_radius,
                        form_id: actor_form_id,
                    },
                );
                sink.put(actor, ambush);
            }
            Self::Find(behavior) => {
                sink.put(actor, behavior);
            }
            Self::UseWeapon(behavior) => {
                sink.put(actor, behavior);
            }
            Self::Flee {
                threat_form_id,
                min_distance,
                refuge_form_id,
            } => {
                sink.put(actor,
                    FleePackageBehavior {
                        threat_form_id,
                        min_distance,
                        refuge_form_id,
                    },
                );
            }
            Self::Patrol {
                patrol_radius,
                actor_form_id,
            } => {
                sink.put(actor,
                    PatrolBehavior {
                        patrol_radius,
                        form_id: actor_form_id,
                    },
                );
            }
            Self::Eat {
                radius,
                location,
                actor_form_id,
            } => {
                sink.put(actor,
                    EatBehavior {
                        radius,
                        location,
                        form_id: actor_form_id,
                    },
                );
            }
            Self::Sleep {
                radius,
                location,
                actor_form_id,
            } => {
                sink.put(actor,
                    SleepBehavior {
                        radius,
                        location,
                        form_id: actor_form_id,
                    },
                );
            }
            Self::Dialogue { topic } => {
                sink.put(actor,
                    crate::systems::forcegreet::ForceGreetDirective {
                        topic,
                        radius: crate::systems::forcegreet::FORCE_GREET_RADIUS,
                    },
                );
            }
        }
    }

    /// Spawn path: the caller owns the world exclusively.
    fn insert_at_spawn(self, world: &mut World, actor: EntityId) {
        self.install(&mut SpawnSink(world), actor);
    }

    /// Runtime path: a system holding `&World` swaps the behavior in place.
    fn insert_at_runtime(self, world: &World, actor: EntityId) {
        self.install(&mut RuntimeSink(world), actor);
    }
}

/// Where an [`AmbientBehavior`] lands. The spawn path holds `&mut World` and
/// inserts structurally; the runtime path holds `&World` and goes through the
/// storage lock. Both used to carry their own copy of the ~160-line variant
/// match (#6361).
trait ComponentSink {
    fn put<T: Component>(&mut self, actor: EntityId, component: T);
}

struct SpawnSink<'a>(&'a mut World);

impl ComponentSink for SpawnSink<'_> {
    fn put<T: Component>(&mut self, actor: EntityId, component: T) {
        self.0.insert(actor, component);
    }
}

struct RuntimeSink<'a>(&'a World);

impl ComponentSink for RuntimeSink<'_> {
    fn put<T: Component>(&mut self, actor: EntityId, component: T) {
        insert_component(self.0, actor, component);
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

    // Declared in this body, not at module level, so the scheduler's source scan
    // (`acquired_in`) still sees every removal as a write acquisition.
    /// Every per-actor component an ambient package installs or leaves behind, one
    /// remover each. A new procedure adds its `Behavior` / `State` rows here and its
    /// arm to `AmbientBehavior::install` (#6361); a forgotten row is the bug #3255
    /// had to fix by hand for `NavPath`.
    const AMBIENT_COMPONENT_REMOVERS: &[fn(&World, EntityId)] = &[
        remove_component::<SandboxBehavior>,
        remove_component::<Seated>,
        remove_component::<WanderBehavior>,
        remove_component::<WanderState>,
        remove_component::<TravelBehavior>,
        remove_component::<TravelState>,
        remove_component::<Traveled>,
        remove_component::<FollowBehavior>,
        remove_component::<FollowState>,
        remove_component::<EscortBehavior>,
        remove_component::<EscortState>,
        remove_component::<Escorted>,
        remove_component::<GuardBehavior>,
        remove_component::<GuardState>,
        remove_component::<FleePackageBehavior>,
        remove_component::<AmbushBehavior>,
        remove_component::<FindPackageBehavior>,
        remove_component::<FindProgress>,
        remove_component::<UseWeaponPackageBehavior>,
        remove_component::<UseWeaponPackageStarted>,
        remove_component::<PatrolBehavior>,
        remove_component::<PatrolState>,
        remove_component::<EatBehavior>,
        remove_component::<SleepBehavior>,
        remove_component::<EatSleepState>,
        // The Dialogue procedure's bridge self-consumes on the open, but a
        // package handover mid-approach must not leave the actor walking at
        // the player under its next procedure.
        remove_component::<crate::systems::forcegreet::ForceGreetDirective>,
        // M42.10 — the oscillating walkers' blocked-timer scratch belongs to
        // the behavior runtime, not the actor: a package handover must not
        // carry a half-accumulated stuck timer into the next procedure.
        remove_component::<crate::components::WalkStuckTimer>,
        // #3255 — six of the seven M42 procedures (wander/travel/follow/
        // escort/guard/patrol) write NavPath as their shared per-actor pathing
        // cache. This list must cover every per-actor pathing/runtime
        // component, not just the Behavior/State pairs above it — a stale
        // NavPath survives a package-switch handover and can be reused by a
        // live-goal caller (FOLLOW_REPATH_THRESHOLD / ESCORT_COLLECT_REPATH_
        // THRESHOLD = 64.0) whose new target happens to land within that
        // threshold of the previous procedure's cached waypoints.
        remove_component::<NavPath>,
    ];

    // A UseWeapon package's volleys stop with it. The run is only the
    // package's when the package started it: a scripted `UseWeapon` volley
    // must survive an unrelated handover. Decided before the marker below goes.
    if world.has::<UseWeaponPackageStarted>(actor) {
        remove_component::<crate::systems::script_requests::UseWeaponRun>(world, actor);
    }
    for remove in AMBIENT_COMPONENT_REMOVERS {
        remove(world, actor);
    }
}

/// #4703 — combat pre-empts the ambient package. `StartCombat` arms
/// `AiCombatState` and `npc_combat_ai_system` moves the actor, but the
/// ambient movers never read it, so the package's own mover kept driving
/// the same actor every frame, and a seated actor slid toward its target
/// still in its sit pose. Called by the combat system for each live
/// attacker: tear the behavior down (unseating it, releasing its seat
/// claim, restoring its pre-seat animation) and forget the winner, so
/// [`ambient_ai_package_system`] — which skips actors in combat —
/// re-selects and reinstalls it on the first tick after combat ends. A
/// no-op once suspended, so the per-frame cost is two lookups.
///
/// #5017 — also the unconscious suspension: `ambient_ai_package_system`
/// calls it for every unconscious actor.
pub(crate) fn suspend_ambient_behavior_for_combat(world: &World, actor: EntityId) {
    let active = world
        .get::<AmbientPackageRuntime>(actor)
        .is_some_and(|runtime| runtime.active_package_form_id.is_some());
    if !active && world.get::<Seated>(actor).is_none() {
        return;
    }
    let previous = world
        .get::<AmbientPackageRuntime>(actor)
        .and_then(|runtime| runtime.active_package_form_id);
    byroredux_scripting::report_package_transition(world, actor, previous, None);
    clear_ambient_behavior(world, actor);
    if let Some(mut runtimes) = world.query_mut::<AmbientPackageRuntime>() {
        if let Some(runtime) = runtimes.get_mut(actor) {
            runtime.active_package_form_id = None;
            runtime.last_evaluated_game_minute = None;
        }
    }
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
    // #6079 / #6257 — PSDT's Day-of-Week and date are tested against the
    // game calendar where the game's enum and calendar are established;
    // `None` leaves the constraint unevaluated.
    let day = world.try_resource::<GameTimeRes>().map(|time| time.day);
    let clock = byroredux_plugin::esm::records::PackClock {
        hour: game_hour,
        weekday: day.and_then(|day| byroredux_scripting::globals::package_weekday(world, day)),
        day_of_month: day
            .and_then(|day| byroredux_scripting::globals::package_day_of_month(world, day)),
    };
    byroredux_plugin::esm::records::active_package(packages, clock, |package| {
        package_conditions_pass(&package.conditions, world, &context)
            // #5499 / #5722 — skip any Dialogue package the installer
            // would decline: fail-open conditions, non-player targets, or
            // Say-To type. This leaves the scan free to choose the next
            // runnable package below it. The fail-open policy is unchanged
            // for every other procedure.
            && selectable_by_installer(package, world)
    })
}

/// `BYRO_M42_DEBUG` diagnostics switch, read from the environment once. The
/// `[m42-tick]` probe runs every frame, and `std::env::var_os` is a global
/// environ read lock plus a linear scan (#5448).
fn m42_debug_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("BYRO_M42_DEBUG").is_some())
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
    resolved: &byroredux_plugin::equip::ResolvedNpc<'_>,
    index: &EsmIndex,
) {
    // #4093 (D5-02) / #4457 — the "Use AI Packages" terminal arrives
    // through the resolved record, so this consumer cannot read the
    // shell's own (possibly empty or stale) `PKID` list by accident.
    // `npc.form_id` stays the shell's own identity throughout (this
    // actor's placement, not the template's) — only the package *list*
    // comes from the resolved terminal.
    let packages_npc = resolved.ai_packages;
    // #4704 — an empty PKID stack still gets the runtime: it is the only
    // thing `ambient_ai_package_system` visits, and it is where a quest
    // alias's injected packages (ALPC) are merged. Returning early here left
    // every actor whose only package source is an alias (44 forced-ref
    // aliases on `Skyrim.esm`, e.g. `dunUstengravQST`'s warlocks) standing
    // idle for good. With no candidates the selection below is `None` and
    // nothing is installed until an overlay lands.
    if world.get::<Dead>(placement_root).is_some() {
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
        packages_npc
            .ai_packages
            .iter()
            .filter_map(|form_id| index.packages.get(form_id)),
    );
    let active_package_form_id = active.map(|package| package.form_id);
    let behavior = active.and_then(|package| {
        let template = package
            .package_template_form_id
            .and_then(|form_id| index.packages.get(&form_id))
            .unwrap_or(package);
        AmbientBehavior::from_package(package, template, resolved.shell.form_id, world)
    });

    // M47.2 / #5864 — the spawn-time winner is NOT reported here. It is chosen
    // before the quest-alias overlays (stamped after spawn) and, on a save
    // load, the restored clock exist, so the first-tick confirmation may pick
    // a different package; reporting `None -> A` now would run A's
    // Begin/Change for a package the actor never ran. The first tick
    // (`last_evaluated_game_minute: None`) reports the confirmed winner. This
    // call only clears what an earlier occupant of a reused entity id left.
    byroredux_scripting::report_package_transition(world, placement_root, None, None);
    world.insert(
        placement_root,
        AmbientPackageRuntime {
            package_candidates: packages_npc.ai_packages.clone(),
            active_package_form_id,
            actor_form_id: resolved.shell.form_id,
            // Force a first-tick confirmation after the cell loader installs
            // PACK, quest, and restored game-time resources.
            last_evaluated_game_minute: None,
        },
    );
    if let Some(behavior) = behavior {
        behavior.insert_at_spawn(world, placement_root);
    }
}

/// #6074 — make `package_form_id` `actor`'s running package now: the handover
/// `ambient_ai_package_system` performs on a change (clear the old procedure,
/// install the new one, record its id). Used by the stream-snapshot restore,
/// whose package — an alias overlay's or a script package's — is not what
/// spawn selected from the actor's own stack. `false`, with nothing touched,
/// when the package catalog does not hold the package (yet), the installer
/// would decline it, or the actor has no ambient runtime.
pub(crate) fn install_package_now(world: &World, actor: EntityId, package_form_id: u32) -> bool {
    let Some(actor_form_id) = world
        .get::<AmbientPackageRuntime>(actor)
        .map(|runtime| runtime.actor_form_id)
    else {
        return false;
    };
    let behavior = {
        let Some(registry) = world.try_resource::<PackageRegistry>() else {
            return false;
        };
        let Some(package) = registry.package(package_form_id) else {
            return false;
        };
        if !selectable_by_installer(package, world) {
            return false;
        }
        let template = package
            .package_template_form_id
            .and_then(|form_id| registry.package(form_id))
            .unwrap_or(package);
        AmbientBehavior::from_package(package, template, actor_form_id, world)
    };
    clear_ambient_behavior(world, actor);
    if let Some(behavior) = behavior {
        behavior.insert_at_runtime(world, actor);
    }
    if let Some(mut query) = world.query_mut::<AmbientPackageRuntime>() {
        if let Some(runtime) = query.get_mut(actor) {
            runtime.active_package_form_id = Some(package_form_id);
        }
    }
    true
}

/// #4815 — re-seat every ambient package against the *restored* clock and
/// quest state, before a save load overlays the saved procedure columns.
///
/// The reload spawns NPCs while the outgoing session's `GameTimeRes` and
/// quest state are still live, so `apply_ai_package_behavior` picks each
/// winner against the wrong clock. Left alone, the first Update tick
/// re-selects against the restored clock, sees a different winner, and
/// `clear_ambient_behavior` deletes the `WanderState`/`TravelState`/
/// `Traveled`/`GuardState`/`PatrolState`/`Escorted` the delta overlay just
/// restored. Running the selection here instead installs the saved winner's
/// behavior first, so the overlay lands on it and the first tick finds no
/// change. It also stamps the evaluation minute, so that tick is gated out.
pub(crate) fn reseat_ambient_packages_after_restore(world: &World) {
    ambient_ai_package_system(world, 0.0);
}

/// M42.9 / #2652 — reevaluate ambient NPC package stacks.
///
/// Scheduled immediately before `scene_package_system`: this system observes
/// `EvaluatePackageRequest` but deliberately does not remove it, allowing the
/// SCEN runtime to consume the same request afterward. Schedule-only checks are
/// bounded to one evaluation per in-game minute per actor.
pub(crate) fn ambient_ai_package_system(world: &World, dt: f32) {
    // M47.3 — script packages (`AddScriptPackage`) lapse on their own clock and
    // end when a Travel completes. Both queue a re-evaluation for the actor, so
    // they reach `requested` below. Nearly always a no-op: the component is rare.
    byroredux_scripting::script_packages::tick_script_packages(world, dt);
    for actor in byroredux_scripting::script_packages::running_script_package_actors(world) {
        if world.has::<Traveled>(actor) {
            byroredux_scripting::script_packages::finish_script_package(world, actor);
        }
    }
    // #5017 — an unconscious actor cannot move or pick packages. Tear down an
    // installed procedure (a no-op once suspended; the list is nearly always
    // empty) before the minute gate, so a knock-out takes effect this frame.
    // Waking leaves the reset evaluation clock, so selection resumes on the
    // next tick.
    let mut unconscious = Vec::new();
    byroredux_scripting::collect_unconscious(world, &mut unconscious);
    for &actor in &unconscious {
        suspend_ambient_behavior_for_combat(world, actor);
    }
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
    //
    // The explicit-request marker is one query pass instead of one
    // `world.has` per actor per frame. It is transient and rare, so the
    // collected set is nearly always empty; it is read first (and its
    // guard dropped) so the runtime scan below can build `due` directly —
    // no per-frame O(NPCs) intermediate Vec. The two query scopes never
    // overlap, preserving the TypeId-sorted lock-order invariant.
    let requested: Vec<EntityId> = world
        .query::<EvaluatePackageRequest>()
        .map(|query| query.iter().map(|(actor, _)| actor).collect())
        .unwrap_or_default();

    // Pass 2 — the gate. Only the survivors (usually none) go on to pay for a
    // `package_candidates` clone and a `Dead` lookup.
    let mut any_runtime = false;
    let mut due: Vec<EntityId> = Vec::new();
    if let Some(query) = world.query::<AmbientPackageRuntime>() {
        for (actor, runtime) in query.iter() {
            any_runtime = true;
            if requested.contains(&actor) || runtime.last_evaluated_game_minute != Some(minute) {
                due.push(actor);
            }
        }
    }
    if !any_runtime {
        return;
    }
    if m42_debug_enabled() {
        log::warn!("[m42-tick] minute={minute} due={}", due.len());
    }
    if due.is_empty() {
        return;
    }

    // Reject suspended actors before cloning their candidate stacks. Keep
    // the last evaluation minute untouched: combat clears it so ambient AI
    // resumes immediately when combat ends, even within the same minute.
    // Separate query scopes preserve the component lock-order invariant.
    if let Some(dead) = world.query::<Dead>() {
        due.retain(|&actor| dead.get(actor).is_none());
    }
    if let Some(combat) = world.query::<AiCombatState>() {
        due.retain(|&actor| combat.get(actor).is_none());
    }
    due.retain(|actor| !unconscious.contains(actor));
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
        // A script package "takes precedence over all other packages until it
        // is done" (GECK *AddScriptPackage*), so it skips the schedule and
        // condition scan. One the registry no longer holds falls back to the
        // actor's own stack, and so does a Dialogue package the installer
        // declines (Say-To, off-target, fail-open conditions): it would win,
        // install nothing and strip the actor's behaviour, and a Must-complete
        // one never lapses (#6051). The slot stays for the script to remove.
        let scripted = byroredux_scripting::script_packages::script_package_of(world, actor)
            .and_then(|slot| registry.package(slot.package))
            .filter(|package| selectable_by_installer(package, world));
        let active =
            scripted.or_else(|| select_active_package(world, actor, game_hour, packages.iter().copied()));
        let active_package_form_id = active.map(|package| package.form_id);
        let behavior = active.and_then(|package| {
            let template = package
                .package_template_form_id
                .and_then(|form_id| registry.package(form_id))
                .unwrap_or(package);
            AmbientBehavior::from_package(package, template, runtime.actor_form_id, world)
        });
        if m42_debug_enabled() {
            log::warn!(
                "[m42-eval] actor {actor} -> {:?}",
                active_package_form_id.map(|id| format!("{id:08X}"))
            );
        }
        updates.push((
            actor,
            active_package_form_id,
            behavior,
            active_package_form_id != runtime.active_package_form_id,
        ));
    }
    drop(registry);

    for &(actor, active_id, behavior, changed) in &updates {
        // M47.2 — the package fragments: the old package changed, the new
        // one began. #5864 — `from` is what was last REPORTED running, so the
        // first tick after spawn (nothing reported yet) reports `None -> A`
        // even when the winner is the one spawn stamped, and a package that
        // never began is never "changed". An unchanged, already-reported
        // winner is a no-op.
        let runtime_active = world
            .get::<AmbientPackageRuntime>(actor)
            .and_then(|runtime| runtime.active_package_form_id);
        let previous = byroredux_scripting::reported_package(world, actor).unwrap_or(runtime_active);
        byroredux_scripting::report_package_transition(world, actor, previous, active_id);
        if !changed {
            // Already the running package: start the script package's clock.
            if byroredux_scripting::script_packages::script_package_of(world, actor)
                .is_some_and(|slot| Some(slot.package) == active_id)
            {
                byroredux_scripting::script_packages::mark_running(world, actor);
            }
            continue;
        }
        log::info!(
            "[m42-reselect] actor {actor} -> pack {:?}",
            active_id.map(|id| format!("{id:08X}"))
        );
        clear_ambient_behavior(world, actor);
        if let Some(behavior) = behavior {
            behavior.insert_at_runtime(world, actor);
        }
        // Installed after the handover cleared the old procedure's markers, so a
        // stale `Traveled` cannot read as this package completing.
        if byroredux_scripting::script_packages::script_package_of(world, actor)
            .is_some_and(|slot| Some(slot.package) == active_id)
        {
            byroredux_scripting::script_packages::mark_running(world, actor);
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
    use byroredux_plugin::equip::ResolvedNpc;
    use byroredux_plugin::esm::records::NpcRecord;
    use byroredux_core::ecs::components::wander::WanderPhase;
    use byroredux_core::math::Vec3;
    use byroredux_plugin::esm::records::condition::{
        ComparisonOp, Condition, ConditionValue, RunOn,
    };
    use byroredux_plugin::esm::records::misc::pack::{
        PackDataInput, PackLocation, PackProcedure, PackSchedule, PROCEDURE_DIALOGUE,
        PROCEDURE_EAT, PROCEDURE_GUARD, PROCEDURE_SANDBOX, PROCEDURE_SLEEP, PROCEDURE_TRAVEL,
        PROCEDURE_WANDER,
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
        world.register::<FleePackageBehavior>();
        world.register::<UseWeaponPackageBehavior>();
        world.register::<AmbushBehavior>();
        world.register::<FindPackageBehavior>();
        world.register::<FindProgress>();
        world.register::<UseWeaponPackageStarted>();
        world.register::<PatrolBehavior>();
        world.register::<PatrolState>();
        world.register::<EatBehavior>();
        world.register::<SleepBehavior>();
        world.register::<EatSleepState>();
        world.register::<crate::systems::forcegreet::ForceGreetDirective>();
        world.register::<NavPath>();
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

    /// #6079 — the Primm Prospector Den patrol ships as MWF / TT / SS variants
    /// of one package that all pass the hour test; the first listed used to
    /// win every day. The game calendar's weekday now picks the variant.
    #[test]
    fn day_of_week_variants_select_by_the_game_calendar() {
        use byroredux_plugin::esm::reader::GameKind;
        use byroredux_plugin::esm::records::PackDays;

        let variant = |form_id: u32, days: PackDays| {
            pack(
                form_id,
                PROCEDURE_SANDBOX,
                Some(PackSchedule {
                    start_hour: Some(8),
                    duration_minutes: 4 * 60,
                    days,
                    ..Default::default()
                }),
            )
        };
        let packages = [
            variant(0x1, PackDays::MonWedFri),
            variant(0x2, PackDays::TueThu),
            variant(0x3, PackDays::Weekends),
        ];
        let mut world = World::new();
        world.insert_resource(byroredux_scripting::ScriptGame(GameKind::Fallout3NV));
        let actor = world.spawn();
        // No start date authored: day 0 falls on a Sunday.
        let winner = |world: &mut World, day: u64| {
            let mut time = GameTimeRes::frozen_at(9.0);
            time.day = day;
            world.insert_resource(time);
            select_active_package(world, actor, 9.0, packages.iter()).map(|p| p.form_id)
        };
        let week: Vec<_> = (0..7).map(|day| winner(&mut world, day)).collect();
        assert_eq!(
            week,
            [Some(3), Some(1), Some(2), Some(1), Some(2), Some(1), Some(3)],
            "Sun SS, Mon MWF, Tue TT, Wed MWF, Thu TT, Fri MWF, Sat SS"
        );
        assert_eq!(winner(&mut world, 7), Some(3), "the week repeats");

        // #6257 — Oblivion shares the enum; its calendar starts on Morndas,
        // so day 0 picks the MWF variant and day 6 (Sundas) the weekend one.
        world.insert_resource(byroredux_scripting::ScriptGame(GameKind::Oblivion));
        let week: Vec<_> = (0..7).map(|day| winner(&mut world, day)).collect();
        assert_eq!(
            week,
            [Some(1), Some(2), Some(1), Some(2), Some(1), Some(3), Some(3)],
            "Mor MWF, Tir TT, Mid MWF, Tur TT, Fre MWF, Log SS, Sun SS"
        );

        // A game without an established PSDT calendar leaves the constraint
        // unevaluated: the first listed package wins every day.
        world.insert_resource(byroredux_scripting::ScriptGame(GameKind::Skyrim));
        assert_eq!(winner(&mut world, 2), Some(1));
    }

    /// #6257 — an Oblivion package dated the Nth of the month (the
    /// `ImpExRequest02`..`27` family) runs on that day only, counted from the
    /// authored start date (27 Last Seed); off it the next package wins.
    #[test]
    fn dated_packages_select_by_the_oblivion_day_of_the_month() {
        use byroredux_plugin::esm::reader::GameKind;

        let dated = |form_id: u32, date: u8| {
            pack(
                form_id,
                PROCEDURE_SANDBOX,
                Some(PackSchedule {
                    start_hour: Some(12),
                    duration_minutes: 5 * 60,
                    date,
                    ..Default::default()
                }),
            )
        };
        let packages = [dated(0x2, 2), dated(0x1B, 27), dated(0xFF, 0)];
        let mut world = World::new();
        world.insert_resource(byroredux_scripting::ScriptGame(GameKind::Oblivion));
        world.insert_resource(byroredux_scripting::globals::GameClockGlobals {
            start_date: Some((433.0, 7.0, 27.0)),
            ..Default::default()
        });
        let actor = world.spawn();
        let winner = |world: &mut World, day: u64| {
            let mut time = GameTimeRes::frozen_at(13.0);
            time.day = day;
            world.insert_resource(time);
            select_active_package(world, actor, 13.0, packages.iter()).map(|p| p.form_id)
        };
        assert_eq!(winner(&mut world, 0), Some(0x1B), "27 Last Seed");
        assert_eq!(winner(&mut world, 1), Some(0xFF), "the 28th");
        assert_eq!(winner(&mut world, 6), Some(0x2), "2 Hearthfire (Last Seed has 31 days)");
        assert_eq!(winner(&mut world, 6 + 25), Some(0x1B), "27 Hearthfire");

        // Fallout authors no dates and has no established month table.
        world.insert_resource(byroredux_scripting::ScriptGame(GameKind::Fallout3NV));
        assert_eq!(winner(&mut world, 1), Some(0x2));
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
        apply_ai_package_behavior(&mut world, actor, &ResolvedNpc::resolve(&npc, &index), &index);
        install_package_records(&mut world, all_packages);
        ambient_ai_package_system(&world, 0.0);
        (world, actor)
    }

    /// #5376 — the three decline gates on the ambient Dialogue arm:
    /// a fail-open conditions pass, a non-player PTDT reference, and a
    /// Say-To dialogue type must each leave the bridge uninstalled
    /// (the console door keeps installing unconditionally — it is the
    /// drivable installer, not an ambient behavior).
    #[test]
    fn dialogue_ambient_install_declines_unmodeled_offtarget_sayto() {
        use byroredux_plugin::esm::records::condition::{
            ComparisonOp, Condition, ConditionValue,
        };
        use byroredux_plugin::esm::records::misc::pack::PackTarget;

        // Fn 79 (GetQuestVariable — the corpus's dominant blocker, 59 of the
        // 60 FNV bases) is modeled only on the pre-Papyrus games; this world
        // names no game, so it passes as fail-open: the fail-open SELECTS the
        // package (it is the first eligible) but the bridge must not install.
        let mut unmodeled = pack(0x600, PROCEDURE_DIALOGUE, None);
        unmodeled.conditions = vec![Condition {
            function_index: 79,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(0.0),
            ..Default::default()
        }];
        let (world, actor) = setup_actor(12.0, vec![unmodeled]);
        assert!(
            !world.has::<crate::systems::forcegreet::ForceGreetDirective>(actor),
            "a fail-open conditions pass must not arm the player-facing bridge"
        );

        // A PTDT naming a non-player reference (the FO3 shape: 179 of
        // 365 Dialogue refs target another reference).
        let mut offtarget = pack(0x601, PROCEDURE_DIALOGUE, None);
        offtarget.target = Some(PackTarget {
            target_type: 0,
            target: PackTargetKind::SpecificReference(0x000E_27FC),
            count_or_distance: 0,
        });
        let (world, actor) =
            setup_actor(12.0, vec![offtarget, pack(0x604, PROCEDURE_SANDBOX, None)]);
        assert!(
            !world.has::<crate::systems::forcegreet::ForceGreetDirective>(actor),
            "a package targeting another reference must not force a player conversation"
        );
        assert!(
            world.has::<byroredux_core::ecs::components::SandboxBehavior>(actor),
            "selection skips the fully-modeled off-target Dialogue package"
        );

        // Say-To (dialogue_type 1): one spoken line, no menu.
        let mut say_to = pack(0x602, PROCEDURE_DIALOGUE, None);
        say_to.dialogue_type = 1;
        let (world, actor) = setup_actor(12.0, vec![say_to, pack(0x605, PROCEDURE_SANDBOX, None)]);
        assert!(
            !world.has::<crate::systems::forcegreet::ForceGreetDirective>(actor),
            "a Say-To package is a spoken line, not a player conversation"
        );
        assert!(
            world.has::<byroredux_core::ecs::components::SandboxBehavior>(actor),
            "selection skips the fully-modeled Say-To Dialogue package"
        );

        // #5499 — the decline is now a *selection* decision: a fail-open
        // Dialogue package above a runnable package no longer starves the
        // whole stack. The dominant corpus shape (fn 79 GetQuestVariable
        // heads over a Sandbox/Eat/Sleep fallback) ran nothing at all.
        let mut starving = pack(0x610, PROCEDURE_DIALOGUE, None);
        starving.conditions = vec![Condition {
            function_index: 79,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(0.0),
            ..Default::default()
        }];
        let fallback = pack(0x611, PROCEDURE_SANDBOX, None);
        let (world, actor) = setup_actor(12.0, vec![starving, fallback]);
        assert!(
            world.has::<byroredux_core::ecs::components::SandboxBehavior>(actor),
            "the scan must continue past a fail-open Dialogue decline to the \
             scheduled Sandbox below it (#5499)"
        );
        assert!(
            !world.has::<crate::systems::forcegreet::ForceGreetDirective>(actor),
            "the declined Dialogue still installs nothing"
        );

        // The player reference (0x14) still installs.
        let mut player_targeted = pack(0x603, PROCEDURE_DIALOGUE, None);
        player_targeted.target = Some(PackTarget {
            target_type: 0,
            target: PackTargetKind::SpecificReference(0x0000_0014),
            count_or_distance: 0,
        });
        let (world, actor) = setup_actor(12.0, vec![player_targeted]);
        assert!(
            world.has::<crate::systems::forcegreet::ForceGreetDirective>(actor),
            "a player-targeted modeled Conversation still installs"
        );
    }

    /// M42 — the three newly wired procedures install their runtimes
    /// from ambient selection: Eat/Sleep their walk-then-seat behaviors,
    /// Dialogue the #5367 force-greet bridge carrying the package's
    /// authored `PKDD` topic (generic greeting when none). A schedule
    /// handover away from a Dialogue package must consume the bridge so
    /// the actor does not keep approaching the player under its next
    /// procedure.
    #[test]
    fn eat_sleep_dialogue_procedures_install_and_hand_over() {
        let (world, actor) = setup_actor(12.0, vec![pack(0x300, PROCEDURE_EAT, None)]);
        assert!(
            world.has::<EatBehavior>(actor),
            "an active Eat package installs the walk-then-seat behavior"
        );
        let (world, actor) = setup_actor(23.0, vec![pack(0x400, PROCEDURE_SLEEP, None)]);
        assert!(world.has::<SleepBehavior>(actor));

        let mut dialogue = pack(0x500, PROCEDURE_DIALOGUE, None);
        dialogue.dialogue_topic = Some(0xC8);
        let (world, actor) = setup_actor(12.0, vec![dialogue]);
        assert_eq!(
            world
                .get::<crate::systems::forcegreet::ForceGreetDirective>(actor)
                .map(|directive| directive.topic),
            Some(Some(0xC8)),
            "an active Dialogue package installs the force-greet bridge \
             with the package's PKDD topic"
        );

        // Handover: the Dialogue window (8-10) gives way to a Sandbox
        // package (10-12) at the minute boundary — the bridge is
        // consumed, the next behavior installs.
        let mut dialogue_scheduled = pack(
            0x500,
            PROCEDURE_DIALOGUE,
            Some(PackSchedule {
                start_hour: Some(8),
                duration_minutes: 2 * 60,
                ..Default::default()
            }),
        );
        dialogue_scheduled.dialogue_topic = Some(0xC8);
        let sandbox = pack(
            0x100,
            PROCEDURE_SANDBOX,
            Some(PackSchedule {
                start_hour: Some(10),
                duration_minutes: 2 * 60,
                ..Default::default()
            }),
        );
        let (mut world, actor) = setup_actor(9.0, vec![dialogue_scheduled, sandbox]);
        assert!(world.has::<crate::systems::forcegreet::ForceGreetDirective>(actor));
        world.insert_resource(GameTimeRes::frozen_at(11.0));
        // Clear the minute stamp so the minute gate re-runs this actor
        // (the game-minute boundary does this naturally).
        {
            let mut runtimes = world.query_mut::<AmbientPackageRuntime>().unwrap();
            runtimes.get_mut(actor).unwrap().last_evaluated_game_minute = None;
        }
        ambient_ai_package_system(&world, 0.0);
        assert!(
            !world.has::<crate::systems::forcegreet::ForceGreetDirective>(actor),
            "the package handover consumes the force-greet bridge"
        );
        assert!(world.has::<SandboxBehavior>(actor));
    }

    /// #6087 — a script package present at the re-seat is confirmed as
    /// the winner directly (a first-tick confirmation), so the markers
    /// the save overlay restores afterwards survive the next minute and
    /// no scheduled→script transition fires for an actor that was
    /// already running the script package. Pre-fix the drain ran the
    /// re-seat before the ScriptPackage column landed, and the next
    /// minute's tick treated the overlaid package as a *change*:
    /// `clear_ambient_behavior` deleted the freshly restored
    /// `TravelState`/`Traveled` (both ride the same clear) and a
    /// spurious fragment fired. Pinned together with the drain-order
    /// source scan (`post_reload_drain_steps_keep_their_order`).
    #[test]
    fn script_package_at_the_reseat_keeps_the_restored_markers() {
        use byroredux_core::math::Vec3;
        use byroredux_scripting::script_packages::ScriptPackage;

        let sandbox = pack(0x100, PROCEDURE_SANDBOX, None);
        let script_travel = pack(0x77, PROCEDURE_TRAVEL, None);
        let (mut world, actor) = setup_actor(10.0, vec![sandbox, script_travel]);
        assert!(
            world.has::<SandboxBehavior>(actor),
            "the scheduled winner installs at spawn"
        );

        // The #6087 drain order: the ScriptPackage pre-seat runs BEFORE
        // the re-seat, so the re-seat's confirmation selects it.
        world.insert(
            actor,
            ScriptPackage { package: 0x77, constrained: false, running_secs: None },
        );
        {
            let mut runtimes = world.query_mut::<AmbientPackageRuntime>().unwrap();
            runtimes.get_mut(actor).unwrap().last_evaluated_game_minute = None;
        }
        ambient_ai_package_system(&world, 0.0);
        assert_eq!(
            byroredux_scripting::reported_package(&world, actor),
            Some(Some(0x77)),
            "the confirmation names the script package, not the schedule"
        );

        // The overlay then lands its restored procedure state on the
        // confirmed winner.
        world.insert(actor, TravelState { destination: Vec3::ZERO });

        // The next in-game minute: still the script package — no change,
        // no clear, the restored marker survives.
        world.resource_mut::<GameTimeRes>().set_hour(10.1);
        {
            let mut runtimes = world.query_mut::<AmbientPackageRuntime>().unwrap();
            runtimes.get_mut(actor).unwrap().last_evaluated_game_minute = None;
        }
        ambient_ai_package_system(&world, 0.0);
        assert!(
            world.has::<TravelState>(actor),
            "the overlay's restored marker survives the next minute"
        );
        assert_eq!(
            byroredux_scripting::reported_package(&world, actor),
            Some(Some(0x77)),
            "no scheduled->script transition fired for the running package"
        );
    }

    #[test]
    fn schedule_boundary_replaces_behavior_and_releases_seat_claim() {
        let sandbox = pack(
            0x100,
            PROCEDURE_SANDBOX,
            Some(PackSchedule {
                start_hour: Some(8),
                duration_minutes: 12 * 60,
                ..Default::default()
            }),
        );
        let travel = pack(
            0x200,
            PROCEDURE_TRAVEL,
            Some(PackSchedule {
                start_hour: Some(20),
                duration_minutes: 2 * 60,
                ..Default::default()
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

    /// #4815 — the audit probe `audit_probe_saved_travel_progress_depends_
    /// on_live_clock`, kept as the regression. A save load spawns at the
    /// live hour, restores the saved hour, then overlays the saved procedure
    /// state. With the re-seat between the restore and the overlay, the
    /// saved `Traveled` survives the first tick; without it (the control
    /// arm) the first tick sees a package change and clears it.
    #[test]
    fn save_load_reseat_keeps_overlaid_procedure_state_across_clocks() {
        let packages = || {
            vec![
                pack(
                    0x100,
                    PROCEDURE_SANDBOX,
                    Some(PackSchedule {
                        start_hour: Some(8),
                        duration_minutes: 12 * 60,
                        ..Default::default()
                    }),
                ),
                pack(
                    0x200,
                    PROCEDURE_TRAVEL,
                    Some(PackSchedule {
                        start_hour: Some(20),
                        duration_minutes: 2 * 60,
                        ..Default::default()
                    }),
                ),
            ]
        };
        for reseat in [true, false] {
            // Spawned against the live session's 10:00 clock.
            let (world, actor) = setup_actor(10.0, packages());
            assert!(world.has::<SandboxBehavior>(actor));
            // restore_resources installs the saved 21:00 clock.
            world.resource_mut::<GameTimeRes>().set_hour(21.0);
            if reseat {
                reseat_ambient_packages_after_restore(&world);
            }
            // apply_deltas overlays the saved procedure state.
            world.query_mut::<Traveled>().unwrap().insert(actor, Traveled);
            // First Update tick.
            ambient_ai_package_system(&world, 0.0);
            assert_eq!(
                world.has::<Traveled>(actor),
                reseat,
                "reseat={reseat}: the saved Traveled must survive only with the re-seat"
            );
            if reseat {
                assert!(world.has::<TravelBehavior>(actor));
            }
        }
    }

    /// Find (procedure 0): a reference, a base object or a classifiable object
    /// type, searched for in the `PLDT` area; anything else, and a fail-open
    /// condition list, is declined.
    #[test]
    fn a_find_package_installs_only_what_the_runtime_can_hunt() {
        use byroredux_core::ecs::components::{EatSleepLocation, FindItemKind, FindTarget};
        use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
        use byroredux_plugin::esm::records::misc::pack::PROCEDURE_FIND;
        use byroredux_plugin::esm::records::PackTarget;

        let finding = |form_id, target_type, kind, count| {
            let mut package = pack(form_id, PROCEDURE_FIND, None);
            package.target = Some(PackTarget { target_type, target: kind, count_or_distance: count });
            package.location = Some(PackLocation {
                location_type: 2,
                target: PackLocationTarget::Other(0),
                radius: 1000,
            });
            package
        };
        let installed_in = |game, package: PackRecord| {
            let (mut world, actor) = setup_actor(10.0, vec![package]);
            world.insert_resource(crate::cell_loader::LoadedCellIndex(std::sync::Arc::new(EsmIndex {
                game,
                ..Default::default()
            })));
            // The behavior is chosen at spawn and again at each re-evaluation;
            // the index is in place by the time the second one runs.
            world.query_mut::<EvaluatePackageRequest>().unwrap().insert(actor, EvaluatePackageRequest);
            ambient_ai_package_system(&world, 0.0);
            world.get::<FindPackageBehavior>(actor).map(|behavior| *behavior)
        };
        let installed = |package: PackRecord| installed_in(byroredux_plugin::esm::reader::GameKind::Fallout3NV, package);

        // FNV `FFER08FindFoodDog`: a base object, count 1, a 1000 area.
        assert_eq!(
            installed(finding(0x100, 1, PackTargetKind::ObjectId(0x5001), 1)),
            Some(FindPackageBehavior {
                target: FindTarget::Base(0x5001),
                count: 1,
                area: EatSleepLocation::NearCurrentLocation,
                area_radius: 1000.0,
            })
        );
        // FNV `VCrGeckoFindFood`: object type 18, count 2.
        let food = installed(finding(0x101, 2, PackTargetKind::Other(18), 2)).expect("food installs");
        assert_eq!((food.target, food.count), (FindTarget::Kind(FindItemKind::Food), 2));
        // FNV `DefaultFindPlayerPackage`: the player.
        let player = installed(finding(0x102, 0, PackTargetKind::SpecificReference(0x14), 0)).expect("the player");
        assert_eq!((player.target, player.count), (FindTarget::Reference(0x14), 1));

        // Oblivion numbers the object types differently, so its type search is declined.
        assert_eq!(
            installed_in(
                byroredux_plugin::esm::reader::GameKind::Oblivion,
                finding(0x10A, 2, PackTargetKind::Other(18), 2)
            ),
            None,
            "18 is Keys in Oblivion's enum, not Food"
        );
        // Flora (10), furniture (11) and the contradicted weapon classes are not searched for.
        for unclassified in [10, 11, 21, 22, 23, 28] {
            assert_eq!(
                installed(finding(0x103, 2, PackTargetKind::Other(unclassified), 1)),
                None,
                "object type {unclassified} has no record to classify it by"
            );
        }
        // A fail-open conditions pass must not send the actor hunting, nor shadow the next package.
        let mut unmodeled = finding(0x104, 1, PackTargetKind::ObjectId(0x5001), 1);
        unmodeled.conditions = vec![Condition {
            function_index: 79,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(0.0),
            ..Default::default()
        }];
        assert_eq!(installed(unmodeled.clone()), None);
        let (world, actor) = setup_actor(10.0, vec![unmodeled, pack(0x105, PROCEDURE_GUARD, None)]);
        assert!(world.has::<GuardBehavior>(actor), "the lower package still wins");
    }

    /// UseItemAt (procedure 8) is furniture use in the vanilla data: it takes
    /// Eat's walk-and-sit runtime at its `PLDT` location.
    #[test]
    fn a_use_item_at_package_walks_to_its_location_and_sits() {
        use byroredux_core::ecs::components::{EatBehavior, EatSleepLocation};
        use byroredux_plugin::esm::records::misc::pack::PROCEDURE_USE_ITEM_AT;

        // FNV `PrimmNashStool`: near a reference, radius 0.
        let mut sit = pack(0x100, PROCEDURE_USE_ITEM_AT, None);
        sit.location = Some(PackLocation {
            location_type: 0,
            target: PackLocationTarget::NearReference(0x000A_23CF),
            radius: 0,
        });
        let (world, actor) = setup_actor(10.0, vec![sit]);
        assert_eq!(
            world.get::<EatBehavior>(actor).map(|behavior| (behavior.location, behavior.radius)),
            Some((EatSleepLocation::NearReference(0x000A_23CF), None))
        );
    }

    /// Ambush (procedure 9): wait on Guard's runtime at `PLDT`, gate the
    /// fight on `PLD2`; a handover to another package removes the gate.
    #[test]
    fn an_ambush_package_installs_a_guard_wait_and_a_zone_gate() {
        use byroredux_core::ecs::components::EatSleepLocation;
        use byroredux_plugin::esm::records::misc::pack::PROCEDURE_AMBUSH;

        // FNV `DefaultAmbushPackage1024Hide`: wait near the editor location
        // (256), the ambush location a 1024 radius around the linked ref.
        let mut ambush = pack(0x100, PROCEDURE_AMBUSH, None);
        ambush.location = Some(PackLocation { location_type: 3, target: PackLocationTarget::Other(0), radius: 256 });
        ambush.location2 = Some(PackLocation { location_type: 6, target: PackLocationTarget::Other(0), radius: 1024 });
        let (world, actor) = setup_actor(10.0, vec![ambush]);
        assert_eq!(
            world.get::<AmbushBehavior>(actor).map(|behavior| *behavior),
            Some(AmbushBehavior {
                zone: EatSleepLocation::NearLinkedReference,
                zone_radius: 1024.0,
                target_form_id: None,
            })
        );
        let guard = world.get::<GuardBehavior>(actor).map(|behavior| *behavior).expect("the wait runs on Guard");
        assert_eq!((guard.anchor_form_id, guard.radius), (None, Some(256.0)));

        // A named wait reference and target (FNV `BMRoadBlock1AmbushPackage`).
        let mut named = pack(0x200, PROCEDURE_AMBUSH, None);
        named.location = Some(PackLocation { location_type: 0, target: PackLocationTarget::NearReference(0xE_79CE), radius: 800 });
        named.location2 = Some(PackLocation { location_type: 0, target: PackLocationTarget::NearReference(0xE_84B9), radius: 0 });
        named.target = Some(byroredux_plugin::esm::records::PackTarget {
            target_type: 0,
            target: PackTargetKind::SpecificReference(0xE_686E),
            count_or_distance: 0,
        });
        let (world, actor) = setup_actor(10.0, vec![named]);
        let gate = world.get::<AmbushBehavior>(actor).map(|behavior| *behavior).unwrap();
        assert_eq!(gate.zone, EatSleepLocation::NearReference(0xE_84B9));
        assert_eq!((gate.zone_radius, gate.target_form_id), (0.0, Some(0xE_686E)), "radius 0 gates nothing");
        assert_eq!(world.get::<GuardBehavior>(actor).map(|b| b.anchor_form_id), Some(Some(0xE_79CE)));

        // Handover: a conditioned Ambush gives way and takes both parts with it.
        let mut gated = pack(0x300, PROCEDURE_AMBUSH, None);
        gated.location2 = Some(PackLocation { location_type: 3, target: PackLocationTarget::Other(0), radius: 512 });
        gated.conditions.push(Condition {
            function_index: 58,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(10.0),
            param_1: 0x900,
            run_on: RunOn::Subject,
            ..Default::default()
        });
        let wander = pack(0x400, PROCEDURE_WANDER, None);
        let (world, actor) = setup_actor(10.0, vec![gated, wander]);
        assert!(world.has::<WanderBehavior>(actor) && !world.has::<AmbushBehavior>(actor));
        world.resource_mut::<QuestStageState>().set_stage(QuestFormId(0x900), 10);
        world.query_mut::<EvaluatePackageRequest>().unwrap().insert(actor, EvaluatePackageRequest);
        ambient_ai_package_system(&world, 0.0);
        assert!(world.has::<AmbushBehavior>(actor) && world.has::<GuardBehavior>(actor));
        assert!(!world.has::<WanderBehavior>(actor));
    }

    /// Accompany (procedure 7) authors Follow's data - a `PTDT` reference and
    /// a distance - and runs on the Follow runtime.
    #[test]
    fn an_accompany_package_runs_as_a_follow() {
        use byroredux_plugin::esm::records::misc::pack::{PackTarget, PROCEDURE_ACCOMPANY};

        // FNV `VFSKingsPatrol01FollowerPackage`: a specific reference, 50 units.
        let mut accompany = pack(0x100, PROCEDURE_ACCOMPANY, None);
        accompany.target = Some(PackTarget {
            target_type: 0,
            target: PackTargetKind::SpecificReference(0x000E_73A0),
            count_or_distance: 50,
        });
        let (world, actor) = setup_actor(10.0, vec![accompany]);
        assert_eq!(
            world.get::<FollowBehavior>(actor).map(|behavior| *behavior),
            Some(FollowBehavior {
                target_form_id: Some(0x000E_73A0),
                follow_distance: Some(50.0),
            })
        );
    }

    /// UseWeapon (procedure 16): `PTDT` is the weapon, `PTD2` the target and
    /// `PKW3` the fire pattern; a package the runtime cannot honour is not
    /// installed and does not get selected over a lower one.
    #[test]
    fn a_use_weapon_package_installs_only_what_the_runtime_can_honour() {
        use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
        use byroredux_plugin::esm::records::misc::pack::{PackTarget, PackUseWeapon, PROCEDURE_USE_WEAPON};

        let target = |target_type, target| PackTarget { target_type, target, count_or_distance: 0 };
        let shooting = |form_id| {
            let mut package = pack(form_id, PROCEDURE_USE_WEAPON, None);
            package.target = Some(target(1, PackTargetKind::ObjectId(0x0017_24F2)));
            package.target2 = Some(target(0, PackTargetKind::SpecificReference(0x14)));
            package.use_weapon = Some(PackUseWeapon {
                flags: PackUseWeapon::DO_NO_DAMAGE,
                volley_fire: true,
                repeat_fire: false,
                bursts: 3,
                volley_min: 3,
                volley_max: 8,
                ..Default::default()
            });
            package
        };
        let installed = |package: PackRecord| {
            let (world, actor) = setup_actor(10.0, vec![package]);
            world.get::<UseWeaponPackageBehavior>(actor).map(|behavior| *behavior)
        };

        // Volley range 3..=8 -> 6 shots (the midpoint, rounded up).
        assert_eq!(
            installed(shooting(0x100)),
            Some(UseWeaponPackageBehavior {
                weapon_form_id: 0x0017_24F2,
                target_form_id: 0x14,
                bursts: Some(3),
                volley_shots: 6,
                harmless: true,
            })
        );

        let mut repeat = shooting(0x101);
        repeat.use_weapon = Some(PackUseWeapon { repeat_fire: true, ..repeat.use_weapon.unwrap() });
        let repeat = installed(repeat).expect("repeat fire installs");
        assert_eq!(repeat.bursts, None, "repeat fire never finishes");

        // A weapon class (PTDT type 2), a target-less package and a fail-open
        // condition list are all declined.
        let mut by_class = shooting(0x102);
        by_class.target = Some(target(2, PackTargetKind::Other(0x16)));
        assert_eq!(installed(by_class), None, "weapon classes are not decoded");
        let mut into_the_air = shooting(0x103);
        into_the_air.target2 = None;
        assert_eq!(installed(into_the_air), None, "no target: nothing to fire at");
        let mut unmodeled = shooting(0x104);
        unmodeled.conditions = vec![Condition {
            function_index: 79,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(0.0),
            ..Default::default()
        }];
        assert_eq!(installed(unmodeled.clone()), None, "a fail-open pass must not arm the shooter");

        // ... and a declined UseWeapon package does not shadow the next one.
        let (world, actor) = setup_actor(10.0, vec![unmodeled, pack(0x105, PROCEDURE_GUARD, None)]);
        assert!(world.has::<GuardBehavior>(actor), "the lower package still wins");
    }

    /// FleeNotCombat (procedure 10): `PTDT` becomes the threat and its
    /// distance the minimum to keep, `PLDT` a `NearReference` the refuge; a
    /// later package handover removes the marker.
    #[test]
    fn a_flee_not_combat_package_installs_its_threat_distance_and_refuge() {
        use byroredux_plugin::esm::records::misc::pack::PROCEDURE_FLEE_NOT_COMBAT;
        use byroredux_plugin::esm::records::{PackLocation, PackTarget};

        // FNV `VFSFreesideThugFleeFromPlayerPackage`: the player, 1500 units.
        let mut from_player = pack(0x100, PROCEDURE_FLEE_NOT_COMBAT, None);
        from_player.target = Some(PackTarget {
            target_type: 0,
            target: PackTargetKind::SpecificReference(0x14),
            count_or_distance: 1500,
        });
        // FNV `JacobstownNightkinFlee`: flee to a reference.
        let mut to_refuge = pack(0x200, PROCEDURE_FLEE_NOT_COMBAT, None);
        to_refuge.location = Some(PackLocation {
            location_type: 0,
            target: PackLocationTarget::NearReference(0x0013_F711),
            radius: 512,
        });
        let flee = |package: PackRecord| {
            let (world, actor) = setup_actor(10.0, vec![package]);
            world.get::<FleePackageBehavior>(actor).map(|behavior| *behavior)
        };
        assert_eq!(
            flee(from_player),
            Some(FleePackageBehavior {
                threat_form_id: Some(0x14),
                min_distance: Some(1500.0),
                refuge_form_id: None,
            })
        );
        assert_eq!(
            flee(to_refuge),
            Some(FleePackageBehavior {
                threat_form_id: None,
                min_distance: None,
                refuge_form_id: Some(0x0013_F711),
            })
        );

        // A gated Flee loses to the next package, and the marker goes.
        let mut gated = pack(0x300, PROCEDURE_FLEE_NOT_COMBAT, None);
        gated.conditions.push(Condition {
            function_index: 58,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(10.0),
            param_1: 0x900,
            run_on: RunOn::Subject,
            ..Default::default()
        });
        let wander = pack(0x400, PROCEDURE_WANDER, None);
        let (world, actor) = setup_actor(10.0, vec![gated, wander]);
        assert!(world.has::<WanderBehavior>(actor) && !world.has::<FleePackageBehavior>(actor));
        world.resource_mut::<QuestStageState>().set_stage(QuestFormId(0x900), 10);
        world.query_mut::<EvaluatePackageRequest>().unwrap().insert(actor, EvaluatePackageRequest);
        ambient_ai_package_system(&world, 0.0);
        assert!(world.has::<FleePackageBehavior>(actor), "the gate opened, Flee wins");
        assert!(!world.has::<WanderBehavior>(actor));
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

    /// #5448 — the `BYRO_M42_DEBUG` probe sits ahead of the due-set early-out,
    /// so it runs every frame; an environ scan per frame is the cost. The
    /// variable may be read from exactly one place — the `OnceLock` helper.
    #[test]
    fn m42_debug_env_is_read_once_not_per_frame() {
        let prod = byroredux_core::source_scan::production_text(include_str!("ai_package.rs"));
        assert_eq!(
            prod.matches("var_os(\"BYRO_M42_DEBUG\")").count(),
            1,
            "the env var must be read only inside `m42_debug_enabled` (#5448)"
        );
        let helper = prod
            .split("fn m42_debug_enabled")
            .nth(1)
            .and_then(|rest| rest.split("\nfn ").next())
            .expect("m42_debug_enabled helper");
        assert!(helper.contains("OnceLock"), "the read must be cached");
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

    /// #6074 — an actor whose running package came from a quest-alias overlay
    /// comes back from a stream round trip running that package, not the
    /// spawn winner. Respawn selects over the actor's own stack (the overlay is
    /// stamped later), so the snapshot's package has to be installed, not just
    /// recorded: a recorded id made the first tick see "no change" and leave
    /// the spawn winner's behavior in place.
    #[test]
    fn a_respawned_alias_package_is_installed_not_just_recorded() {
        use crate::cell_loader::stream_snapshot::{
            restore_actor_snapshot, ActorStreamSnapshot, StreamStateSnapshots,
        };
        use byroredux_core::math::Quat;
        const ACTOR: u32 = 0x0001_6074;
        let destination = Vec3::new(4000.0, 0.0, 0.0);
        let snapshot = ActorStreamSnapshot {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            active_package_form_id: Some(0x200),
            travel_destination: Some(destination),
            traveled: false,
            escorted: false,
            seated_furniture_form_id: None,
            seated_animation_restore: None,
            seated_marker_idx: None,
        };
        let respawn = |catalog: Vec<PackRecord>| {
            let (mut world, actor) =
                setup_actor_with_catalog(10.0, vec![pack(0x100, PROCEDURE_WANDER, None)], catalog);
            assert!(world.has::<WanderBehavior>(actor), "spawn installs its own winner");
            let mut store = StreamStateSnapshots::default();
            store.insert(ACTOR, snapshot);
            world.insert_resource(store);
            restore_actor_snapshot(&mut world, actor, ACTOR, Vec3::ZERO, Quat::IDENTITY);
            (world, actor)
        };

        let (world, actor) = respawn(vec![pack(0x200, PROCEDURE_TRAVEL, None)]);
        assert!(world.has::<TravelBehavior>(actor));
        assert!(!world.has::<WanderBehavior>(actor));
        // The overlay lands on the next Update and the tick confirms it.
        let mut injected = AliasInjectedData::default();
        injected.packages.push(0x200);
        world.query_mut::<QuestAliasInjectedOverlays>().unwrap().insert(
            actor,
            QuestAliasInjectedOverlays([((QuestFormId(0x900), 1), injected)].into_iter().collect()),
        );
        world
            .query_mut::<EvaluatePackageRequest>()
            .unwrap()
            .insert(actor, EvaluatePackageRequest);
        ambient_ai_package_system(&world, 0.0);
        assert!(world.has::<TravelBehavior>(actor), "the alias package keeps running");
        assert_eq!(
            world.get::<TravelState>(actor).map(|state| state.destination),
            Some(destination),
            "Travel progress resumes on top of the installed package"
        );

        // A package the catalog does not hold cannot be installed: the spawn
        // winner stays, unrecorded as anything else, and the snapshot's Travel
        // markers are not laid over it.
        let (world, actor) = respawn(Vec::new());
        assert!(world.has::<WanderBehavior>(actor));
        assert_eq!(
            world
                .get::<AmbientPackageRuntime>(actor)
                .unwrap()
                .active_package_form_id,
            Some(0x100)
        );
        assert!(!world.has::<TravelState>(actor));
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

    /// #3255 — `NavPath` is the shared per-actor pathing cache six of the
    /// seven M42 procedures write; a stale one surviving a package-switch
    /// handover can be reused by a live-goal caller whose new target lands
    /// within its repath threshold of the previous procedure's cached
    /// waypoints. `clear_ambient_behavior`'s teardown list must cover it
    /// alongside every Behavior/State pair.
    #[test]
    fn clear_ambient_behavior_removes_nav_path() {
        let mut world = World::new();
        world.register::<NavPath>();
        let actor = world.spawn();
        world.insert(
            actor,
            NavPath {
                goal: Vec3::new(10.0, 0.0, 0.0),
                residency_generation: 1,
                waypoints: std::collections::VecDeque::from(vec![Vec3::new(10.0, 0.0, 0.0)]),
            },
        );
        assert!(world.has::<NavPath>(actor));

        clear_ambient_behavior(&world, actor);

        assert!(
            !world.has::<NavPath>(actor),
            "a stale NavPath must not survive a package-switch handover"
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

    /// Regression for #4093 (D5-02). A templated `NPC_` shell — `Use AI
    /// Packages` set, no `PKID` list of its own — must run the template's
    /// package, not stand idle. Before this fix, `apply_ai_package_behavior`
    /// read `npc.ai_packages` directly and early-returned on the shell's
    /// empty list.
    #[test]
    fn templated_npc_runs_the_use_ai_packages_templates_behavior() {
        use byroredux_plugin::equip::TEMPLATE_FLAG_USE_AI_PACKAGES;

        let mut world = World::new();
        register_runtime(&mut world, 10.0);
        let actor = world.spawn();

        const TEMPLATE: u32 = 0x0040_0001;
        const SHELL: u32 = 0x0040_0002;
        let wander = pack(0x100, PROCEDURE_WANDER, None);

        let template_npc = NpcRecord {
            form_id: TEMPLATE,
            ai_packages: vec![wander.form_id],
            ..Default::default()
        };
        let shell_npc = NpcRecord {
            form_id: SHELL,
            template_flags: TEMPLATE_FLAG_USE_AI_PACKAGES,
            template_form_id: TEMPLATE,
            ai_packages: Vec::new(), // the shell authors no PKID list of its own
            ..Default::default()
        };

        let mut index = EsmIndex::default();
        index.npcs.insert(TEMPLATE, template_npc);
        index.packages.insert(wander.form_id, wander.clone());

        apply_ai_package_behavior(
            &mut world,
            actor,
            &ResolvedNpc::resolve(&shell_npc, &index),
            &index,
        );
        install_package_records(&mut world, vec![wander]);
        ambient_ai_package_system(&world, 0.0);

        assert!(
            world.has::<WanderBehavior>(actor),
            "a templated shell with Use AI Packages set must run the \
             template's package instead of standing idle"
        );
    }

    /// An actor on a scheduled Sandbox with two script-package candidates in the
    /// catalog: `0x300` unconstrained and `0x400` flagged *Must complete*.
    fn script_package_actor() -> (World, EntityId) {
        let sandbox = pack(
            0x100,
            PROCEDURE_SANDBOX,
            Some(PackSchedule {
                start_hour: Some(8),
                duration_minutes: 12 * 60,
                ..Default::default()
            }),
        );
        let unconstrained = pack(0x300, PROCEDURE_TRAVEL, None);
        let mut must_complete = pack(0x400, PROCEDURE_TRAVEL, None);
        must_complete.package_flags = 0x4;
        let (world, actor) = setup_actor_with_catalog(10.0, vec![sandbox], vec![unconstrained, must_complete]);
        assert!(world.has::<SandboxBehavior>(actor));
        (world, actor)
    }

    fn active_package(world: &World, actor: EntityId) -> Option<u32> {
        world.get::<AmbientPackageRuntime>(actor).unwrap().active_package_form_id
    }

    /// M47.3 — `AddScriptPackage` "takes precedence over all other packages":
    /// the scheduled Sandbox gives way at once (the add queues its own
    /// re-evaluation), and the package's clock starts when it is installed.
    #[test]
    fn a_script_package_preempts_the_scheduled_package() {
        use byroredux_scripting::script_packages::{add_script_package, script_package_of};
        let (world, actor) = script_package_actor();
        assert!(add_script_package(&world, actor, 0x300));
        assert_eq!(script_package_of(&world, actor).unwrap().running_secs, None, "pending until installed");
        ambient_ai_package_system(&world, 0.0);
        assert!(world.has::<TravelBehavior>(actor));
        assert!(!world.has::<SandboxBehavior>(actor));
        assert_eq!(active_package(&world, actor), Some(0x300));
        assert_eq!(script_package_of(&world, actor).unwrap().running_secs, Some(0.0));
    }

    /// #6051 — `AddScriptPackage` of a Dialogue package the installer declines
    /// (here a Say-To, the `ArcadeHooverDamDialoguePackage` shape: Must complete)
    /// must not win selection, or the actor loses its schedule to a package that
    /// installs nothing and never ends. It keeps its own stack; a Conversation
    /// the installer takes still preempts like any other script package.
    #[test]
    fn a_declined_dialogue_script_package_leaves_the_actor_on_its_schedule() {
        use byroredux_scripting::script_packages::{add_script_package, script_package_of};
        let sandbox = pack(
            0x100,
            PROCEDURE_SANDBOX,
            Some(PackSchedule {
                start_hour: Some(8),
                duration_minutes: 12 * 60,
                ..Default::default()
            }),
        );
        let mut say_to = pack(0x500, PROCEDURE_DIALOGUE, None);
        say_to.dialogue_type = 1;
        say_to.package_flags = 0x4;
        let conversation = pack(0x501, PROCEDURE_DIALOGUE, None);
        let (world, actor) = setup_actor_with_catalog(10.0, vec![sandbox], vec![say_to, conversation]);
        assert!(world.has::<SandboxBehavior>(actor));

        assert!(add_script_package(&world, actor, 0x500));
        ambient_ai_package_system(&world, 0.0);
        assert_eq!(active_package(&world, actor), Some(0x100), "the Say-To must not win");
        assert!(world.has::<SandboxBehavior>(actor), "the schedule's behavior stays installed");
        assert_eq!(
            script_package_of(&world, actor).unwrap().running_secs,
            None,
            "a package that never ran has no clock"
        );

        // A Conversation the installer takes still preempts, as before.
        assert!(add_script_package(&world, actor, 0x501));
        ambient_ai_package_system(&world, 0.0);
        assert_eq!(active_package(&world, actor), Some(0x501));
    }

    /// An unconstrained script package lapses at the next re-evaluation after
    /// it has run (10 s), and the actor goes back to its own stack.
    #[test]
    fn an_unconstrained_script_package_lapses_back_to_the_scheduled_package() {
        use byroredux_scripting::script_packages::{add_script_package, script_package_of};
        let (world, actor) = script_package_actor();
        add_script_package(&world, actor, 0x300);
        ambient_ai_package_system(&world, 0.0);
        ambient_ai_package_system(&world, 5.0);
        assert_eq!(active_package(&world, actor), Some(0x300), "still inside the interval");
        ambient_ai_package_system(&world, 5.0);
        assert_eq!(script_package_of(&world, actor), None);
        assert_eq!(active_package(&world, actor), Some(0x100));
        assert!(world.has::<SandboxBehavior>(actor));
        assert!(!world.has::<TravelBehavior>(actor));
    }

    /// A *Must complete* package stays until removed.
    #[test]
    fn a_constrained_script_package_stays_until_removed() {
        use byroredux_scripting::script_packages::{add_script_package, remove_script_package};
        let (world, actor) = script_package_actor();
        add_script_package(&world, actor, 0x400);
        ambient_ai_package_system(&world, 0.0);
        ambient_ai_package_system(&world, 600.0);
        assert_eq!(active_package(&world, actor), Some(0x400));
        assert!(remove_script_package(&world, actor));
        ambient_ai_package_system(&world, 0.0);
        assert_eq!(active_package(&world, actor), Some(0x100));
        assert!(world.has::<SandboxBehavior>(actor));
    }

    /// A finished Travel ends its script package; the arrival marker the travel
    /// system stamps is what the ambient system reads.
    #[test]
    fn a_script_travel_package_ends_when_the_actor_arrives() {
        use byroredux_scripting::script_packages::{add_script_package, script_package_of};
        let (mut world, actor) = script_package_actor();
        add_script_package(&world, actor, 0x400);
        ambient_ai_package_system(&world, 0.0);
        assert!(script_package_of(&world, actor).is_some(), "the install itself is not a completion");
        world.insert(actor, Traveled);
        ambient_ai_package_system(&world, 0.0);
        assert_eq!(script_package_of(&world, actor), None);
        assert_eq!(active_package(&world, actor), Some(0x100));
        assert!(!world.has::<Traveled>(actor), "the handover clears the arrival marker");
    }

    /// `EVP` after `AddScriptPackage` starts the package; a later `EVP` ends it,
    /// even a *Must complete* one (GECK *EvaluatePackage*).
    #[test]
    fn evaluate_package_starts_a_pending_script_package_and_ends_a_running_one() {
        use byroredux_scripting::script_packages::{add_script_package, script_package_of};
        let (world, actor) = script_package_actor();
        add_script_package(&world, actor, 0x400);
        byroredux_scripting::package::request_package_evaluation(&world, actor);
        ambient_ai_package_system(&world, 0.0);
        assert_eq!(active_package(&world, actor), Some(0x400), "the first EVP selects it");
        byroredux_scripting::package::request_package_evaluation(&world, actor);
        ambient_ai_package_system(&world, 0.0);
        assert_eq!(script_package_of(&world, actor), None);
        assert_eq!(active_package(&world, actor), Some(0x100));
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

    /// #4824: behavior tests alone cannot detect a wasted candidate clone.
    #[test]
    fn suspended_actors_are_filtered_before_candidate_stacks_are_cloned() {
        let source = byroredux_core::source_scan::production_text(include_str!("ai_package.rs"));
        let system = source
            .split("fn ambient_ai_package_system(")
            .nth(1)
            .unwrap();
        let clone = system
            .find("query.get(actor).map(|r| (actor, r.clone()))")
            .unwrap();
        for filter in [
            "due.retain(|&actor| dead.get(actor).is_none())",
            "due.retain(|&actor| combat.get(actor).is_none())",
        ] {
            assert!(
                system
                    .find(filter)
                    .expect("suspended actors must be filtered")
                    < clone
            );
        }
    }

    /// #4703 — `StartCombat` on a Wander-active actor: the combat tick
    /// suspends the package (no ambient mover left to fight the chase),
    /// the package system leaves it alone while combat lasts, and it is
    /// re-selected the tick after combat ends.
    #[test]
    fn combat_suspends_the_ambient_package_until_it_ends() {
        let (mut world, actor) = setup_actor(10.0, vec![pack(0x100, PROCEDURE_WANDER, None)]);
        assert!(world.has::<WanderBehavior>(actor));
        let target = world.spawn();
        world.insert(
            actor,
            byroredux_core::ecs::components::Transform::from_translation(Vec3::ZERO),
        );
        world.insert(
            target,
            byroredux_core::ecs::components::Transform::from_translation(Vec3::new(
                1000.0, 0.0, 0.0,
            )),
        );
        world.insert(
            actor,
            AiCombatState {
                target,
                attack_cooldown_remaining: 0.0,
            },
        );

        crate::systems::npc_combat_ai_system(&world, 0.1);
        assert!(!world.has::<WanderBehavior>(actor), "combat owns the actor");
        assert_eq!(
            world
                .get::<AmbientPackageRuntime>(actor)
                .unwrap()
                .active_package_form_id,
            None
        );
        ambient_ai_package_system(&world, 0.0);
        assert!(
            !world.has::<WanderBehavior>(actor),
            "no re-selection while in combat"
        );

        world.remove::<AiCombatState>(actor);
        ambient_ai_package_system(&world, 0.0);
        assert!(world.has::<WanderBehavior>(actor), "restored after combat");
        assert_eq!(
            world
                .get::<AmbientPackageRuntime>(actor)
                .unwrap()
                .active_package_form_id,
            Some(0x100)
        );
    }

    /// #5017 — an unconscious actor "cannot move or think": its installed
    /// Wander procedure is torn down on the next tick, no package is
    /// re-selected while it stays unconscious, and waking restores it.
    #[test]
    fn unconsciousness_suspends_the_ambient_package_until_woken() {
        let (mut world, actor) = setup_actor(10.0, vec![pack(0x100, PROCEDURE_WANDER, None)]);
        world.register::<byroredux_scripting::ActorControlState>();
        assert!(world.has::<WanderBehavior>(actor));
        byroredux_scripting::update_actor_control(&world, actor, |state| {
            state.set_unconscious(true)
        });

        ambient_ai_package_system(&world, 0.0);
        assert!(!world.has::<WanderBehavior>(actor), "knocked out this tick");
        ambient_ai_package_system(&world, 0.0);
        assert!(!world.has::<WanderBehavior>(actor), "no re-selection");

        byroredux_scripting::update_actor_control(&world, actor, |state| {
            state.set_unconscious(false)
        });
        ambient_ai_package_system(&world, 0.0);
        assert!(world.has::<WanderBehavior>(actor), "restored on waking");
    }

    /// #4703 — a seated Sandbox actor put into combat stands up: `Seated`
    /// and its seat claim go, instead of sliding toward the target in its
    /// sit pose.
    #[test]
    fn combat_unseats_a_seated_actor() {
        let (mut world, actor) = setup_actor(10.0, vec![pack(0x100, PROCEDURE_SANDBOX, None)]);
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
        let target = world.spawn();
        world.insert(
            actor,
            byroredux_core::ecs::components::Transform::from_translation(Vec3::ZERO),
        );
        world.insert(
            target,
            byroredux_core::ecs::components::Transform::from_translation(Vec3::new(
                1000.0, 0.0, 0.0,
            )),
        );
        world.insert(
            actor,
            AiCombatState {
                target,
                attack_cooldown_remaining: 0.0,
            },
        );

        crate::systems::npc_combat_ai_system(&world, 0.1);

        assert!(!world.has::<Seated>(actor));
        assert!(!world.has::<SandboxBehavior>(actor));
        assert!(world.resource::<SeatReservations>().0.is_empty());
    }

    /// #4704 — an actor with an empty PKID stack still gets the runtime,
    /// so a quest alias's injected package is evaluated and applied.
    #[test]
    fn alias_package_runs_on_an_actor_with_no_base_packages() {
        let (mut world, actor) = setup_actor(10.0, Vec::new());
        assert_eq!(
            world
                .get::<AmbientPackageRuntime>(actor)
                .map(|runtime| runtime.active_package_form_id),
            Some(None),
            "the runtime exists with nothing selected"
        );
        install_package_records(&mut world, [pack(0x200, PROCEDURE_WANDER, None)]);
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

        assert!(world.has::<WanderBehavior>(actor));
        assert_eq!(
            world
                .get::<AmbientPackageRuntime>(actor)
                .unwrap()
                .active_package_form_id,
            Some(0x200)
        );
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
    #[ignore = "needs Skyrim SE game data on disk"]
    fn real_skyrim_esm_ambient_packages_now_resolve_for_previously_blind_npcs() {
        let data = std::env::var(byroredux_plugin::esm::test_paths::SKYRIM_SE_ENV)
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
            apply_ai_package_behavior(&mut world, actor, &ResolvedNpc::resolve(npc, &index), &index);
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
                || world.has::<FleePackageBehavior>(actor)
                || world.has::<UseWeaponPackageBehavior>(actor)
                || world.has::<AmbushBehavior>(actor)
                || world.has::<FindPackageBehavior>(actor)
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

    /// #5864 — the spawn-time winner is not reported (it is chosen before the
    /// alias overlays / restored clock exist); the first-tick confirmation
    /// reports the confirmed one once, and a later unchanged tick reports
    /// nothing.
    #[test]
    fn the_package_transition_is_reported_by_the_first_tick_not_the_spawn() {
        use byroredux_scripting::{ActivePackages, PackageTransition, PendingPackageTransitions};
        let mut world = World::new();
        register_runtime(&mut world, 12.0);
        world.insert_resource(PendingPackageTransitions::default());
        world.insert_resource(ActivePackages::default());
        let actor = world.spawn();
        // Ambush (9), built with no `PLDT`/`PLD2` locations: the fixture authors no
        // wait or zone location, so the package wins without a located behavior.
        // (This used to be `PROCEDURE_DIALOGUE + 1`, which is UseWeapon now.)
        const AMBUSH: u32 = 9;
        let packages = vec![pack(0x700, AMBUSH, None)];
        let npc = NpcRecord {
            form_id: 0xAA,
            ai_packages: vec![0x700],
            ..Default::default()
        };
        let mut index = EsmIndex::default();
        for package in packages.iter().cloned() {
            index.packages.insert(package.form_id, package);
        }
        apply_ai_package_behavior(&mut world, actor, &ResolvedNpc::resolve(&npc, &index), &index);
        assert!(
            world.resource::<PendingPackageTransitions>().0.is_empty(),
            "spawn reported a package the actor has not confirmed"
        );
        assert_eq!(world.resource::<ActivePackages>().0.get(&actor), None);

        install_package_records(&mut world, packages);
        ambient_ai_package_system(&world, 0.0);
        assert_eq!(
            world.resource::<PendingPackageTransitions>().0,
            vec![PackageTransition { actor, from: None, to: Some(0x700) }]
        );
        assert_eq!(world.resource::<ActivePackages>().0.get(&actor), Some(&0x700));

        world.resource_mut::<PendingPackageTransitions>().0.clear();
        if let Some(mut runtimes) = world.query_mut::<AmbientPackageRuntime>() {
            runtimes.get_mut(actor).unwrap().last_evaluated_game_minute = None;
        }
        ambient_ai_package_system(&world, 0.0);
        assert!(
            world.resource::<PendingPackageTransitions>().0.is_empty(),
            "an already-reported winner is not reported again"
        );
    }

    /// #5870 — death reports the running package ending; #5864's load drain
    /// forgets everything the outgoing session reported.
    #[test]
    fn death_retires_the_reported_package_and_a_load_discards_reports() {
        use byroredux_scripting::{ActivePackages, PackageTransition, PendingPackageTransitions};
        let mut world = World::new();
        register_runtime(&mut world, 12.0);
        world.insert_resource(PendingPackageTransitions::default());
        world.insert_resource(ActivePackages::default());
        let actor = world.spawn();
        byroredux_scripting::report_package_transition(&world, actor, None, Some(0x700));
        world.resource_mut::<PendingPackageTransitions>().0.clear();
        world.insert(actor, Dead);

        crate::combat::reconcile_dead_actor(&world, actor);
        assert_eq!(
            world.resource::<PendingPackageTransitions>().0,
            vec![PackageTransition { actor, from: Some(0x700), to: None }]
        );
        assert_eq!(world.resource::<ActivePackages>().0.get(&actor), None);
        crate::combat::reconcile_dead_actor(&world, actor);
        assert_eq!(world.resource::<PendingPackageTransitions>().0.len(), 1, "a second reconcile reports nothing");

        byroredux_scripting::report_package_transition(&world, actor, None, Some(0x701));
        byroredux_scripting::discard_package_reports(&world);
        assert!(world.resource::<PendingPackageTransitions>().0.is_empty());
        assert!(world.resource::<ActivePackages>().0.is_empty());
    }
}
