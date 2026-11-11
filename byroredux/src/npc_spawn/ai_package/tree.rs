//! Skyrim+ ambient package trees (M42, AIP-TREE-001..008): which leaf an actor runs, and what each leaf installs.
//!
//! A Skyrim+ package names its behaviour in a procedure tree (the template's ordered leaves) and carries the
//! values the leaves read as data inputs. The tree's structural nodes (Sequence, Simultaneous, Stacked) stay
//! implicit in the decode (PACKAL §5), so the ordered leaves are all there is to walk:
//!
//! 1. **Tree order, own conditions** (AIP-TREE-001). The first leaf, in tree order, whose own conditions pass
//!    and whose behaviour the engine runs, is the package's behaviour. A leaf whose conditions the evaluator
//!    does not fully model, or that cannot be answered because there is no actor to answer them for, is skipped
//!    rather than passed (the #5376 rule: a fail-open pass must not arm a behaviour). 116 of the 1,627 leaves of
//!    the vanilla Skyrim packages the driver declined before this carried conditions.
//!
//!    The two leaves PACKAL verified against real data keep their precedence: a tree with a `Sandbox` leaf runs
//!    it, else a `Patrol` leaf, before any other leaf is considered (each the first, in tree order, whose
//!    conditions pass). The shared `Travel -> UnlockDoors -> Sandbox` template is a sequence that ends in the
//!    open-ended leaf: taking its `Travel` first would stop 300+ vanilla sandboxers at their first waypoint.
//!    The rest of the leaves are walked in tree order.
//! 2. **Per leaf**, with the values read from the leaf's data inputs ([`procedure_inputs`]):
//!    `Sandbox` / `Patrol` (PACKAL §4-5), `Travel` (AIP-TREE-002), `Sleep` / `Eat` (-003), `ForceGreet` (-005),
//!    `Follow` / `Escort` / `Guard` / `Flee`, and `Wander` when it is the whole tree (-006).
//! 3. **Anything else** is declined with the reason `no-runtime` and the leaf's name (AIP-TREE-007), in the one
//!    debug line the package's other declines use (AIP-DX-002).
//!
//! Nothing here names a game (AIP-TREE-008): FO4 and Starfield trees run through the same code, and a difference
//! in leaf names or input layout belongs in the `PackRecord` decode.

use super::*;
use byroredux_plugin::esm::records::misc::pack::ForceGreetLine;
use byroredux_plugin::esm::records::{PackDataTargetKind, PackProcedure};
use byroredux_scripting::package::procedure_inputs;

/// The player reference, as a package target names it.
const PLAYER_REFERENCE: u32 = 0x0000_0014;

/// Why a leaf installs nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LeafDecline {
    /// The engine has no runtime for this kind of leaf (AIP-TREE-007).
    NoRuntime,
    /// The leaf's own conditions use a function the evaluator does not model, or there was no actor to
    /// evaluate them for.
    UnansweredCondition,
    /// The leaf's conditions evaluated false.
    ConditionsNotMet,
    /// A runtime exists, but the data inputs did not give it something it can run on: Travel's destination is
    /// a quest alias the install cannot resolve, a ForceGreet that does not address the player.
    UnresolvedInput,
    /// Wander installs only as the whole tree (AIP-TREE-006).
    NotRootLeaf,
}

impl LeafDecline {
    pub(super) fn reason(self) -> &'static str {
        match self {
            Self::NoRuntime => "no-runtime",
            Self::UnansweredCondition => "unmodelled-condition",
            Self::ConditionsNotMet => "conditions-not-met",
            Self::UnresolvedInput => "unresolved-input",
            Self::NotRootLeaf => "not-root-leaf",
        }
    }
}

/// The package's behaviour, if any leaf of its tree installs; `actor` is the subject the leaves' own conditions
/// run on (`None` where there is no entity yet: a leaf with conditions is then skipped, not passed). On a decline,
/// every leaf the walk met with the reason it installed nothing (AIP-TREE-007).
pub(super) fn tree_decision(
    package: &PackRecord,
    template: &PackRecord,
    actor_form_id: u32,
    actor: Option<EntityId>,
    world: &World,
) -> Result<AmbientBehavior, Vec<(String, LeafDecline)>> {
    let whole_tree = template.procedures.len() == 1;
    let mut declines: Vec<(String, LeafDecline)> = Vec::new();
    // Sandbox leaves, then Patrol leaves (PACKAL's verified pair), then the rest; each group in tree order.
    let named = |name: &'static str| template.procedures.iter().filter(move |leaf| leaf.procedure_type == name);
    let ordered = named("Sandbox").chain(named("Patrol")).chain(
        template.procedures.iter().filter(|leaf| !matches!(leaf.procedure_type.as_str(), "Sandbox" | "Patrol")),
    );
    for leaf in ordered {
        let decided = leaf_conditions(leaf, actor, world)
            .and_then(|()| leaf_behavior(leaf, package, template, actor_form_id, whole_tree, world));
        match decided {
            Ok(behavior) => return Ok(behavior),
            Err(decline) => declines.push((leaf.procedure_type.clone(), decline)),
        }
    }
    Err(declines)
}

/// [`tree_decision`] as the installer uses it: the behaviour, or `None` with the one debug line.
pub(super) fn behavior_from_tree(
    package: &PackRecord,
    template: &PackRecord,
    actor_form_id: u32,
    actor: Option<EntityId>,
    world: &World,
) -> Option<AmbientBehavior> {
    match tree_decision(package, template, actor_form_id, actor, world) {
        Ok(behavior) => Some(behavior),
        Err(declines) => {
            if !declines.is_empty() {
                log::debug!(
                    "package decline: {:08X} {} tree leaves: {}",
                    package.form_id,
                    package.editor_id,
                    decline_text(&declines)
                );
            }
            None
        }
    }
}

/// `Sit no-runtime, Wait no-runtime`: the leaves of a declined tree with their reasons.
pub(super) fn decline_text(declines: &[(String, LeafDecline)]) -> String {
    declines.iter().map(|(leaf, decline)| format!("{leaf} {}", decline.reason())).collect::<Vec<_>>().join(", ")
}

/// AIP-TREE-001: the leaf's own conditions, fail-closed.
fn leaf_conditions(leaf: &PackProcedure, actor: Option<EntityId>, world: &World) -> Result<(), LeafDecline> {
    if leaf.conditions.is_empty() {
        return Ok(());
    }
    if !package_conditions_fully_modeled(&leaf.conditions, world) {
        return Err(LeafDecline::UnansweredCondition);
    }
    let Some(actor) = actor else {
        return Err(LeafDecline::UnansweredCondition);
    };
    if package_conditions_pass(&leaf.conditions, world, &ConditionContext::for_subject(actor)) {
        Ok(())
    } else {
        Err(LeafDecline::ConditionsNotMet)
    }
}

/// The first `Location` input of a leaf.
fn location_input(inputs: &[&byroredux_plugin::esm::records::PackDataInput]) -> Option<PackLocation> {
    inputs.iter().find_map(|input| match input.value {
        PackDataValue::Location(location) => Some(location),
        _ => None,
    })
}

/// The first `Target` input of a leaf.
fn target_input(
    inputs: &[&byroredux_plugin::esm::records::PackDataInput],
) -> Option<byroredux_plugin::esm::records::PackDataTarget> {
    inputs.iter().find_map(|input| match input.value {
        PackDataValue::Target(target) => Some(target),
        _ => None,
    })
}

fn radius_of(location: PackLocation) -> Option<f32> {
    Some(location.radius as f32).filter(|radius| *radius > 0.0)
}

fn reference_of(location: PackLocation) -> Option<u32> {
    match location.target {
        PackLocationTarget::NearReference(form_id) => Some(form_id),
        _ => None,
    }
}

/// A target input's reference, when it names one outright (an alias, a keyword link or a self target is resolved
/// at run time, not here).
fn target_reference(target: byroredux_plugin::esm::records::PackDataTarget) -> Option<u32> {
    match target.target {
        PackDataTargetKind::SpecificReference(form_id) => Some(form_id),
        _ => None,
    }
}

fn target_distance(target: byroredux_plugin::esm::records::PackDataTarget) -> Option<f32> {
    Some(target.count_or_distance as f32).filter(|distance| *distance > 0.0)
}

/// What one leaf installs.
fn leaf_behavior(
    leaf: &PackProcedure,
    package: &PackRecord,
    template: &PackRecord,
    actor_form_id: u32,
    whole_tree: bool,
    world: &World,
) -> Result<AmbientBehavior, LeafDecline> {
    let inputs = procedure_inputs(leaf, package);
    let location = location_input(&inputs);
    match leaf.procedure_type.as_str() {
        // PACKAL §4: a search radius from the Location input.
        "Sandbox" => Ok(AmbientBehavior::Sandbox { search_radius: location.and_then(radius_of) }),
        // PACKAL §5: the radius is the only resolved Float, never wrapped in a Location.
        "Patrol" => Ok(AmbientBehavior::Patrol {
            patrol_radius: inputs
                .iter()
                .find_map(|input| match input.value {
                    PackDataValue::Float(radius) => Some(radius),
                    _ => None,
                })
                .filter(|radius| *radius > 0.0),
            actor_form_id,
        }),
        // AIP-TREE-002: a destination that is a reference, or the actor's own place (the flat Travel's two
        // cases). A destination that is a quest alias or a keyword link has no reference at install time.
        "Travel" => {
            let location = location.ok_or(LeafDecline::UnresolvedInput)?;
            match location.target {
                PackLocationTarget::NearReference(form_id) => Ok(AmbientBehavior::Travel {
                    radius: radius_of(location),
                    target_form_id: Some(form_id),
                    actor_form_id,
                }),
                PackLocationTarget::Other(_) if matches!(location.location_type, 2 | 3) => Ok(AmbientBehavior::Travel {
                    radius: radius_of(location),
                    target_form_id: None,
                    actor_form_id,
                }),
                _ => Err(LeafDecline::UnresolvedInput),
            }
        }
        // AIP-TREE-003: the location from the leaf's Location input.
        "Sleep" => Ok(AmbientBehavior::Sleep {
            radius: location.and_then(radius_of),
            location: location_anchor(location),
            actor_form_id,
        }),
        "Eat" => Ok(AmbientBehavior::Eat {
            radius: location.and_then(radius_of),
            location: location_anchor(location),
            actor_form_id,
        }),
        // AIP-TREE-005: the #5376 rule - every condition of the package and the leaf modelled - and a target
        // that is the player or absent (a force-greet that addresses another reference speaks to it, not to
        // the player).
        "ForceGreet" => {
            let addresses_player = target_input(&inputs).is_none_or(|target| {
                matches!(target.target, PackDataTargetKind::SpecificReference(PLAYER_REFERENCE))
            });
            if !addresses_player
                || !package_conditions_fully_modeled(&package.conditions, world)
                || !package_conditions_fully_modeled(&template.conditions, world)
                || !package_conditions_fully_modeled(&leaf.conditions, world)
            {
                return Err(LeafDecline::UnresolvedInput);
            }
            match package.force_greet(template) {
                ForceGreetLine::Topic(topic) => Ok(AmbientBehavior::Dialogue { topic: Some(topic), trigger: None }),
                ForceGreetLine::GenericGreeting => Ok(AmbientBehavior::Dialogue { topic: None, trigger: None }),
                ForceGreetLine::NotAForceGreet => Err(LeafDecline::NoRuntime),
            }
        }
        // AIP-TREE-006 (SHOULD): the FO3/FNV counterparts' components.
        "Follow" => {
            let target = target_input(&inputs).ok_or(LeafDecline::UnresolvedInput)?;
            Ok(AmbientBehavior::Follow {
                target_form_id: Some(target_reference(target).ok_or(LeafDecline::UnresolvedInput)?),
                follow_distance: target_distance(target),
            })
        }
        "Escort" => {
            let target = target_input(&inputs).ok_or(LeafDecline::UnresolvedInput)?;
            Ok(AmbientBehavior::Escort {
                target_form_id: Some(target_reference(target).ok_or(LeafDecline::UnresolvedInput)?),
                destination_form_id: location.and_then(reference_of),
                destination_radius: location.and_then(radius_of),
                collect_distance: target_distance(target),
                actor_form_id,
            })
        }
        "Guard" => {
            let location = location.ok_or(LeafDecline::UnresolvedInput)?;
            Ok(AmbientBehavior::Guard {
                anchor_form_id: reference_of(location),
                radius: radius_of(location),
                actor_form_id,
            })
        }
        // Flee takes the actor over like the flat FleeNotCombat: only from conditions decided on modelled
        // terms (#6468).
        "Flee" => {
            if !package_conditions_fully_modeled(&package.conditions, world) {
                return Err(LeafDecline::UnansweredCondition);
            }
            let target = target_input(&inputs).ok_or(LeafDecline::UnresolvedInput)?;
            Ok(AmbientBehavior::Flee {
                threat_form_id: Some(target_reference(target).ok_or(LeafDecline::UnresolvedInput)?),
                min_distance: target_distance(target),
                refuge_form_id: location.and_then(reference_of),
            })
        }
        // PACKAL §5: every vanilla Wander is nested inside another template as a narrow fallback; it is the
        // package's behaviour only when it is the whole tree.
        "Wander" if whole_tree => Ok(AmbientBehavior::Wander {
            wander_radius: location.and_then(radius_of),
            actor_form_id,
        }),
        "Wander" => Err(LeafDecline::NotRootLeaf),
        // Sit / UseIdleMarker wait on the seat runtime (AIP-SIT-002, -003); the rest have none.
        _ => Err(LeafDecline::NoRuntime),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
    use byroredux_plugin::esm::records::{PackDataInput, PackDataTarget};

    fn leaf(name: &str, inputs: &[u8], conditions: Vec<Condition>) -> PackProcedure {
        PackProcedure {
            procedure_type: name.to_owned(),
            data_input_indexes: inputs.to_vec(),
            conditions: conditions.into(),
            ..Default::default()
        }
    }

    fn location(index: u8, location_type: u32, target: PackLocationTarget, radius: i32) -> PackDataInput {
        PackDataInput {
            index,
            value_type: "Location".into(),
            value: PackDataValue::Location(PackLocation { location_type, target, radius }),
        }
    }

    fn target(index: u8, kind: PackDataTargetKind, distance: i32) -> PackDataInput {
        PackDataInput {
            index,
            value_type: "Target".into(),
            value: PackDataValue::Target(PackDataTarget { target_type: 0, target: kind, count_or_distance: distance }),
        }
    }

    /// A template of `leaves` and an instance carrying `inputs`.
    fn tree(leaves: Vec<PackProcedure>, inputs: Vec<PackDataInput>) -> (PackRecord, PackRecord) {
        let template = PackRecord { form_id: 0x10, procedures: leaves, ..Default::default() };
        let package = PackRecord {
            form_id: 0x11,
            package_template_form_id: Some(0x10),
            data_inputs: inputs,
            ..Default::default()
        };
        (package, template)
    }

    fn decide(package: &PackRecord, template: &PackRecord, actor: Option<EntityId>, world: &World) -> Result<AmbientBehavior, Vec<(String, LeafDecline)>> {
        tree_decision(package, template, 0xAA, actor, world)
    }

    fn world() -> World {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.insert_resource(byroredux_scripting::ScriptGame(byroredux_plugin::esm::reader::GameKind::Skyrim));
        world
    }

    fn near_ref(form_id: u32) -> PackLocationTarget {
        PackLocationTarget::NearReference(form_id)
    }

    /// AIP-TREE-002 -- a Travel leaf installs `TravelBehavior`: a reference destination names it, the actor's own
    /// place (types 2, 3) leaves it open, and a destination the install cannot name declines the leaf.
    #[test]
    fn a_travel_leaf_resolves_its_destination() {
        let world = world();
        let (package, template) = tree(vec![leaf("Travel", &[0], vec![])], vec![location(0, 0, near_ref(0x1234), 300)]);
        assert_eq!(
            decide(&package, &template, None, &world),
            Ok(AmbientBehavior::Travel { radius: Some(300.0), target_form_id: Some(0x1234), actor_form_id: 0xAA })
        );
        let (package, template) = tree(vec![leaf("Travel", &[0], vec![])], vec![location(0, 3, PackLocationTarget::Other(0), 0)]);
        assert_eq!(
            decide(&package, &template, None, &world),
            Ok(AmbientBehavior::Travel { radius: None, target_form_id: None, actor_form_id: 0xAA })
        );
        for (location_type, target) in [(1, PackLocationTarget::InCell(0x40)), (6, PackLocationTarget::Other(0)), (12, PackLocationTarget::Other(0))] {
            let (package, template) = tree(vec![leaf("Travel", &[0], vec![])], vec![location(0, location_type, target, 0)]);
            assert_eq!(
                decide(&package, &template, None, &world),
                Err(vec![("Travel".to_owned(), LeafDecline::UnresolvedInput)]),
                "location type {location_type}"
            );
        }
        let (package, template) = tree(vec![leaf("Travel", &[], vec![])], vec![]);
        assert!(decide(&package, &template, None, &world).is_err(), "no Location input");
    }

    /// PACKAL's verified pair keeps its precedence: the shared `Travel -> Sandbox` template sandboxes, and a
    /// Patrol outranks the rest; without either, the leaves are walked in tree order.
    #[test]
    fn sandbox_and_patrol_keep_their_precedence_and_the_rest_run_in_tree_order() {
        let world = world();
        let inputs = vec![location(0, 0, near_ref(0x1234), 256)];
        let (package, template) = tree(vec![leaf("Travel", &[0], vec![]), leaf("Sandbox", &[0], vec![])], inputs.clone());
        assert_eq!(decide(&package, &template, None, &world), Ok(AmbientBehavior::Sandbox { search_radius: Some(256.0) }));
        let (package, template) = tree(vec![leaf("Travel", &[0], vec![]), leaf("Sleep", &[0], vec![])], inputs.clone());
        assert!(matches!(decide(&package, &template, None, &world), Ok(AmbientBehavior::Travel { .. })), "tree order");
        let (package, template) = tree(vec![leaf("Sleep", &[0], vec![]), leaf("Travel", &[0], vec![])], inputs);
        assert!(matches!(decide(&package, &template, None, &world), Ok(AmbientBehavior::Sleep { .. })), "tree order");
    }

    /// AIP-TREE-001 -- a leaf's own conditions decide it, and an unanswerable condition skips the leaf.
    #[test]
    fn a_leaf_runs_only_when_its_own_conditions_pass() {
        let mut world = world();
        let actor = world.spawn();
        let dead = |expected: f32| Condition {
            function_index: 46, // GetDead
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(expected),
            ..Default::default()
        };
        let uncatalogued = Condition {
            function_index: 79, // GetQuestVariable: not modelled on Skyrim
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(0.0),
            ..Default::default()
        };
        let inputs = vec![location(0, 0, near_ref(0x1234), 0)];
        let leaves = |first: Vec<Condition>| vec![leaf("Travel", &[0], first), leaf("Sleep", &[0], vec![])];

        let (package, template) = tree(leaves(vec![dead(0.0)]), inputs.clone());
        assert!(matches!(decide(&package, &template, Some(actor), &world), Ok(AmbientBehavior::Travel { .. })), "alive: GetDead == 0");
        let (package, template) = tree(leaves(vec![dead(1.0)]), inputs.clone());
        assert!(matches!(decide(&package, &template, Some(actor), &world), Ok(AmbientBehavior::Sleep { .. })), "GetDead == 1 fails");
        assert!(matches!(decide(&package, &template, None, &world), Ok(AmbientBehavior::Sleep { .. })), "no actor to answer it");
        let (package, template) = tree(leaves(vec![uncatalogued]), inputs);
        assert!(matches!(decide(&package, &template, Some(actor), &world), Ok(AmbientBehavior::Sleep { .. })), "unmodelled: skipped, not passed");
    }

    /// AIP-TREE-003 / -006 -- Sleep and Eat read their Location input; Wander installs only as the whole tree;
    /// Follow and Guard read their Target and Location inputs.
    #[test]
    fn sleep_eat_wander_follow_and_guard_read_their_inputs() {
        let world = world();
        let loc = vec![location(0, 0, near_ref(0x77), 128)];
        let (package, template) = tree(vec![leaf("Sleep", &[0], vec![])], loc.clone());
        assert_eq!(
            decide(&package, &template, None, &world),
            Ok(AmbientBehavior::Sleep { radius: Some(128.0), location: EatSleepLocation::NearReference(0x77), actor_form_id: 0xAA })
        );
        let (package, template) = tree(vec![leaf("Eat", &[0], vec![])], vec![location(0, 6, PackLocationTarget::Other(0), 0)]);
        assert_eq!(
            decide(&package, &template, None, &world),
            Ok(AmbientBehavior::Eat { radius: None, location: EatSleepLocation::NearLinkedReference, actor_form_id: 0xAA })
        );
        let (package, template) = tree(vec![leaf("Wander", &[0], vec![])], loc.clone());
        assert_eq!(
            decide(&package, &template, None, &world),
            Ok(AmbientBehavior::Wander { wander_radius: Some(128.0), actor_form_id: 0xAA })
        );
        let (package, template) = tree(vec![leaf("Find", &[], vec![]), leaf("Wander", &[0], vec![])], loc.clone());
        assert_eq!(
            decide(&package, &template, None, &world),
            Err(vec![("Find".to_owned(), LeafDecline::NoRuntime), ("Wander".to_owned(), LeafDecline::NotRootLeaf)]),
            "a nested Wander is a fallback, not the package's behaviour"
        );
        let (package, template) = tree(vec![leaf("Follow", &[1], vec![])], vec![target(1, PackDataTargetKind::SpecificReference(0x55), 200)]);
        assert_eq!(
            decide(&package, &template, None, &world),
            Ok(AmbientBehavior::Follow { target_form_id: Some(0x55), follow_distance: Some(200.0) })
        );
        let (package, template) = tree(vec![leaf("Follow", &[1], vec![])], vec![target(1, PackDataTargetKind::ReferenceAlias(3), 0)]);
        assert_eq!(decide(&package, &template, None, &world), Err(vec![("Follow".to_owned(), LeafDecline::UnresolvedInput)]));
        let (package, template) = tree(vec![leaf("Guard", &[0], vec![])], loc);
        assert_eq!(
            decide(&package, &template, None, &world),
            Ok(AmbientBehavior::Guard { anchor_form_id: Some(0x77), radius: Some(128.0), actor_form_id: 0xAA })
        );
    }

    /// AIP-TREE-005 -- a ForceGreet leaf installs the Dialogue behaviour only when every condition is modelled
    /// and the target is the player; AIP-TREE-007 -- any other leaf is declined by name.
    #[test]
    fn force_greet_needs_modelled_conditions_and_the_player_and_other_leaves_are_named() {
        let world = world();
        let (package, template) = tree(vec![leaf("ForceGreet", &[], vec![])], vec![]);
        assert_eq!(decide(&package, &template, None, &world), Ok(AmbientBehavior::Dialogue { topic: None, trigger: None }));
        let (package, template) = tree(
            vec![leaf("ForceGreet", &[2], vec![])],
            vec![target(2, PackDataTargetKind::SpecificReference(0x99), 0)],
        );
        assert_eq!(
            decide(&package, &template, None, &world),
            Err(vec![("ForceGreet".to_owned(), LeafDecline::UnresolvedInput)]),
            "it greets another reference, not the player"
        );
        let uncatalogued = Condition {
            function_index: 79,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(0.0),
            ..Default::default()
        };
        let (mut package, template) = tree(vec![leaf("ForceGreet", &[], vec![])], vec![]);
        package.conditions = vec![uncatalogued].into();
        assert!(decide(&package, &template, None, &world).is_err(), "a fail-open pass must not arm a takeover");

        let (package, template) = tree(vec![leaf("Sit", &[], vec![]), leaf("UseMagic", &[], vec![]), leaf("Wait", &[], vec![])], vec![]);
        let declined = decide(&package, &template, None, &world).unwrap_err();
        assert_eq!(decline_text(&declined), "Sit no-runtime, UseMagic no-runtime, Wait no-runtime");
    }
}
