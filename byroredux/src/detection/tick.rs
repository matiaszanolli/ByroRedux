//! The detection tick (SNK-DET-001; design §4.1): the only producer of
//! detection levels. For every live actor within the row's range of the
//! player it computes, on the detector's own timer, the detection value, the
//! level toward the player and the detector's alert state, and keeps the
//! worst counted detector and the indicator state in [`PlayerDetection`].
//!
//! Registered as the first `Stage::Update` segment, so every consumer reads
//! this frame's levels.

use super::{light_level_at, sight};
use byroredux_core::character::CharacterLevel;
use byroredux_core::ecs::components::{ActorValues, Awareness, GlobalTransform, SleepBehavior, Sneaking};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Vec3;
use byroredux_core::stealth::{
    step_alert, AlertState, DetectionInputs, DetectionLevel, DetectionTerms, IndicatorModel, IndicatorState, StealthIndicator,
    StealthRules,
};
use byroredux_plugin::esm::records::EsmIndex;

/// GECK *Actor Value Codes* (FO3 / FNV), resolved to AVIFs through
/// `obscript_actor_values`.
const AV_PERCEPTION: f32 = 6.0;
const AV_SNEAK: f32 = 42.0;
const AV_INVISIBILITY: f32 = 48.0;
const AV_CHAMELEON: f32 = 49.0;
const AV_NIGHT_EYE: f32 = 50.0;

/// The worst counted detector toward the player: its level, alert state and
/// hostility, and the indicator state it asks for, the highest any counted
/// detector asks for (SNK-HUD-002). An actor flagged by
/// `SetNotShowOnStealthMeter` and not hostile is not counted (SNK-DET-014).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct PlayerDetection {
    pub(crate) level: DetectionLevel,
    pub(crate) alert: AlertState,
    pub(crate) hostile: bool,
    pub(crate) indicator: IndicatorState,
    pub(crate) detector: Option<EntityId>,
}

impl Resource for PlayerDetection {}

/// The action sound the player made since the last tick (SNK-DET-016): the
/// action rating of the weapon's `VNAM` sound level, written when an attack
/// starts and taken by the next tick.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct PlayerActionNoise {
    pub(crate) rating: f32,
}

impl Resource for PlayerActionNoise {}

/// Install the load order's detection rules (SNK-DET-002) and log the row and
/// every setting it read, naming the ones that fell back to the executable's
/// default (SNK-DX-003).
pub(crate) fn install_stealth_rules(world: &mut World, index: &EsmIndex) {
    let kind = index.character_rules.stealth();
    let (rules, readings) = StealthRules::resolve(kind, |editor_id| index.game_setting_float(editor_id));
    let defaulted: Vec<&str> = readings.iter().filter(|r| !r.authored).map(|r| r.name).collect();
    log::info!(
        "Detection rules: {kind:?} ({} settings, {} from the executable default)",
        readings.len(),
        defaulted.len()
    );
    for reading in &readings {
        log::debug!(
            "  {} = {} ({})",
            reading.name,
            reading.value,
            if reading.authored { "authored" } else { "executable default" }
        );
    }
    world.insert_resource(rules);
    let indicator = StealthIndicator::resolve(
        index.character_rules.indicator(),
        |editor_id| index.game_setting_string(editor_id).map(str::to_owned),
        |editor_id| index.perks.values().find(|perk| perk.editor_id.eq_ignore_ascii_case(editor_id)).map(|perk| perk.form_id),
    );
    log::info!(
        "Detection indicator: {:?} [{}]{}",
        indicator.model.style,
        indicator.labels.iter().map(|label| &**label).collect::<Vec<_>>().join(" / "),
        match indicator.gate {
            byroredux_core::stealth::IndicatorGate::Open => String::new(),
            byroredux_core::stealth::IndicatorGate::Perk(form) => format!(", gated on {form:08X?}"),
        }
    );
    world.insert_resource(indicator);
    world.insert_resource(PlayerDetection::default());
    world.insert_resource(PlayerActionNoise::default());
}

/// The detector's level toward the player (`Lost` when it has none yet).
pub(crate) fn level_toward_player(world: &World, detector: EntityId) -> DetectionLevel {
    world.get::<Awareness>(detector).map_or(DetectionLevel::Lost, |awareness| awareness.level)
}

/// Whether `detector` has seen the player (level Seen): the witness, guard rush
/// and `GetDetected` bar (Q5). Without detection rules installed (no load order
/// yet) there are no levels, and awareness of the player is the actor-to-actor
/// sight test, as on a sight-only row.
pub(crate) fn player_seen_by(world: &World, detector: EntityId) -> bool {
    if world.try_resource::<StealthRules>().is_none() {
        return crate::systems::PlayerEntity::get(world).is_some_and(|player| sight::sees(world, detector, player));
    }
    level_toward_player(world, detector) == DetectionLevel::Seen
}

/// Whether `observer` is aware of `target` (`SightQuery`, hostility, flee):
/// toward the player, the observer's level is Seen (SNK-USE-002, Q5); toward
/// anyone else, the actor-to-actor sight test (Q7).
pub(crate) fn detects(world: &World, observer: EntityId, target: EntityId) -> bool {
    if crate::systems::PlayerEntity::get(world) == Some(target) {
        player_seen_by(world, observer)
    } else {
        sight::sees(world, observer, target)
    }
}

/// SNK-ATK-001 — whether `target` is unaware of the player at this moment: its
/// value re-run now, as the originals do at the strike (`Fallout3.exe`
/// `0x845580` calls the per-pair routine afresh), at or below `fSneakSeenMin`.
/// V12: level Unseen and the Noticed band both count as unaware. A sight-only
/// row, or no rules installed, answers from the sight test.
pub(crate) fn unaware_of_player(world: &World, target: EntityId) -> bool {
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return false;
    };
    let rules = world.try_resource::<StealthRules>().map(|r| *r);
    let Some(rules) = rules.filter(|rules| rules.kind != byroredux_core::stealth::StealthKind::SightOnly) else {
        return !sight::sees(world, target, player);
    };
    let Some(player_at) = world.get::<GlobalTransform>(player).map(|t| t.translation) else {
        return false;
    };
    let mut awareness = world.get::<Awareness>(target).map(|a| *a).unwrap_or_default();
    let target_inputs = player_inputs(world, player, player_at);
    evaluate(world, &rules, target, player, target_inputs, 0.0, &[], sight::in_exterior(world), &mut awareness);
    awareness.level != DetectionLevel::Seen
}

/// One frame of the detection tick.
pub(crate) fn detection_tick_system(world: &World, dt: f32) {
    let Some(rules) = world.try_resource::<StealthRules>().map(|r| *r) else {
        return;
    };
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return;
    };
    let Some(player_at) = world.get::<GlobalTransform>(player).map(|t| t.translation) else {
        return;
    };
    let exterior = sight::in_exterior(world);
    let range = rules.range(exterior);
    let target = player_inputs(world, player, player_at);
    let noise = world
        .try_resource_mut::<PlayerActionNoise>()
        .map(|mut noise| std::mem::take(&mut noise.rating))
        .unwrap_or(0.0);
    let events = player_detection_events(world);
    let indicator = world.try_resource::<StealthIndicator>().map(|i| i.model).unwrap_or(IndicatorModel::NONE);

    let mut summary = PlayerDetection::default();
    for detector in crate::systems::faction_hostility::living_actors(world) {
        if detector == player {
            continue;
        }
        let Some(detector_at) = world.get::<GlobalTransform>(detector).map(|t| t.translation) else {
            continue;
        };
        let previous = world.get::<Awareness>(detector).map(|a| *a);
        let distance = detector_at.distance(player_at);
        if distance > range {
            // Out of range: level Lost; the alert state keeps winding down.
            if let Some(previous) = previous {
                let since = previous.since_noticed + dt;
                let alert = step_alert(previous.alert, DetectionLevel::Lost, previous.hostile, false, since >= rules.alert_time());
                write(world, detector, Awareness { level: DetectionLevel::Lost, alert, since_noticed: since, distance, ..previous });
            }
            continue;
        }
        let mut awareness = previous.unwrap_or_default();
        awareness.timer -= dt;
        awareness.since_noticed += dt;
        if previous.is_some() && awareness.timer > 0.0 {
            write(world, detector, awareness);
            fold(world, &indicator, &mut summary, detector, &awareness);
            continue;
        }
        evaluate(world, &rules, detector, player, target, noise, &events, exterior, &mut awareness);
        // Re-armed on its own phase: the remainder carries over (`Fallout3.exe` `0x87445a`).
        awareness.timer = (awareness.timer + rules.period()).max(0.0);
        write(world, detector, awareness);
        fold(world, &indicator, &mut summary, detector, &awareness);
    }
    if let Some(mut detection) = world.try_resource_mut::<PlayerDetection>() {
        *detection = summary;
    }
}

/// Evaluate one detector toward the player.
#[allow(clippy::too_many_arguments)]
fn evaluate(
    world: &World,
    rules: &StealthRules,
    detector: EntityId,
    player: EntityId,
    target: DetectionInputs,
    noise: f32,
    events: &[(Vec3, f32)],
    exterior: bool,
    awareness: &mut Awareness,
) {
    let points = sight::sight_points(world, detector, player);
    let distance = points.map_or(f32::MAX, |(from, to)| from.distance(to));
    let line_of_sight = points.is_some_and(|_| sight::has_line_of_sight(world, detector, player));
    let in_cone = points.is_some_and(|(from, to)| sight::in_view_cone(world, detector, to - from));
    let hostile = crate::systems::faction_hostility::actor_would_attack(world, detector, player);
    let fighting = world.get::<byroredux_scripting::AiCombatState>(detector).map(|state| state.target);
    let inputs = DetectionInputs {
        distance,
        exterior,
        line_of_sight,
        sees: line_of_sight && in_cone,
        detector_perception: actor_value(world, detector, AV_PERCEPTION),
        detector_alert: matches!(awareness.alert, AlertState::Alert | AlertState::Lost),
        detector_sleeping: world.has::<SleepBehavior>(detector),
        detector_not_combat_target: fighting.is_some_and(|target| target != player),
        detector_level: world.get::<CharacterLevel>(detector).map_or(1, |level| level.level),
        detector_night_eye: actor_value(world, detector, AV_NIGHT_EYE) > 0.0,
        action: noise,
        ..target
    };
    let (terms, level, mut noticed) = match rules.value(&inputs) {
        Some(terms) => (terms, rules.level(terms.value), rules.noticed(terms.value)),
        // A row without a sourced formula answers from the sight test (Q2).
        None => {
            let seen = sight::sees(world, detector, player);
            (DetectionTerms::default(), if seen { DetectionLevel::Seen } else { DetectionLevel::Lost }, false)
        }
    };
    // A detection event the player made (SNK-DET-011): its sound level as the
    // action rating at its own position, heard without the player in sight.
    if let (Some(detector_at), Some(_)) = (world.get::<GlobalTransform>(detector).map(|t| t.translation), rules.value(&inputs)) {
        for &(at, sound_level) in events {
            let heard = DetectionInputs {
                distance: detector_at.distance(at),
                line_of_sight: true,
                sees: false,
                target_moving: false,
                target_running: false,
                action: sound_level,
                ..inputs
            };
            noticed |= rules.value(&heard).is_some_and(|terms| rules.noticed(terms.value));
        }
    }
    if noticed {
        awareness.since_noticed = 0.0;
    }
    let expired = awareness.since_noticed >= rules.alert_time();
    awareness.alert = step_alert(awareness.alert, level, hostile, noticed, expired);
    awareness.level = level;
    awareness.terms = terms;
    awareness.distance = distance;
    awareness.line_of_sight = line_of_sight;
    awareness.in_cone = in_cone;
    awareness.hostile = hostile;
}

/// The player's side of the inputs, read once per frame.
fn player_inputs(world: &World, player: EntityId, at: Vec3) -> DetectionInputs {
    let (moving, running) = world
        .try_resource::<crate::interaction::ActionState>()
        .map(|actions| {
            use crate::interaction::InputAction::*;
            let moving = [MoveForward, MoveBackward, StrafeLeft, StrafeRight].into_iter().any(|a| actions.is_held(a));
            (moving, moving && actions.is_held(Sprint))
        })
        .unwrap_or((false, false));
    // SNK-PERK-003 — Silent Running: "If the value is modified to be non-zero, running will be ignored" (GECK).
    let running = running && !byroredux_scripting::perk_entry_points::ignores_running_during_detection(world, player);
    let light_level = light_level_at(world, at) * 100.0;
    DetectionInputs {
        target_sneaking: world.has::<Sneaking>(player),
        target_sneak_skill: actor_value(world, player, AV_SNEAK),
        target_level: world.get::<CharacterLevel>(player).map_or(1, |level| level.level),
        target_weight: crate::inventory::equipped_weight(world, player),
        target_moving: moving,
        target_running: running,
        target_stealth_boy: actor_value(world, player, AV_CHAMELEON) > 0.0
            || actor_value(world, player, AV_INVISIBILITY) > 0.0,
        light_level,
        ..DetectionInputs::default()
    }
}


/// Drain `CreateDetectionEvent`s and keep the player's (owner 0x14) as
/// `(position, sound level)`.
fn player_detection_events(world: &World) -> Vec<(Vec3, f32)> {
    let events = world
        .try_resource_mut::<byroredux_scripting::detection_events::PendingDetectionEvents>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    events
        .into_iter()
        .filter(|event| event.owner == byroredux_scripting::PLAYER_REFERENCE)
        .filter_map(|event| {
            let entity = byroredux_scripting::condition::resolve_entity_by_global_form_id(world, event.source)?;
            let at = world.get::<GlobalTransform>(entity)?.translation;
            Some((at, event.sound_level.clamp(0, 100) as f32))
        })
        .collect()
}

fn actor_value(world: &World, actor: EntityId, code: f32) -> f32 {
    let Some(form) = byroredux_scripting::obscript_actor_values::actor_value_form(world, code) else {
        return 0.0;
    };
    world.get::<ActorValues>(actor).map_or(0.0, |values| values.current(form))
}

fn write(world: &World, detector: EntityId, awareness: Awareness) {
    if let Some(mut storage) = world.query_mut::<Awareness>() {
        storage.insert(detector, awareness);
    }
}

/// Fold one detector into the summary: the highest indicator state wins, then
/// the higher level, then the later alert state.
pub(crate) fn fold(world: &World, indicator: &IndicatorModel, summary: &mut PlayerDetection, detector: EntityId, awareness: &Awareness) {
    if !awareness.hostile && byroredux_scripting::is_hidden_from_stealth_meter(world, detector) {
        return;
    }
    let rank = |alert: AlertState| match alert {
        AlertState::Normal => 0,
        AlertState::Alert => 1,
        AlertState::Lost => 2,
        AlertState::Combat => 3,
    };
    let state = indicator.state(awareness.level, awareness.alert, awareness.hostile);
    let better =
        (state, awareness.level, rank(awareness.alert)) > (summary.indicator, summary.level, rank(summary.alert));
    if summary.detector.is_none() || better {
        *summary = PlayerDetection {
            level: awareness.level,
            alert: awareness.alert,
            hostile: awareness.hostile,
            indicator: state,
            detector: Some(detector),
        };
    }
}

/// SNK-USE-008 — a detector that discovers a dead body turns Alert (when it is
/// hostile to the player; a friendly one stays Normal, `step_alert`), and its
/// alert timer restarts.
pub(crate) fn alert_on_discovery(world: &World, detector: EntityId) {
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return;
    };
    let hostile = crate::systems::faction_hostility::actor_would_attack(world, detector, player);
    let mut awareness = world.get::<Awareness>(detector).map(|a| *a).unwrap_or_default();
    awareness.hostile = hostile;
    awareness.since_noticed = 0.0;
    awareness.alert = step_alert(awareness.alert, awareness.level, hostile, true, false);
    write(world, detector, awareness);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::DetectionConfig;
    use byroredux_core::math::Quat;
    use byroredux_core::stealth::{StealthKind, StealthRules};

    /// A detector facing the player 400 BU away down −Z, in an interior with no physics (nothing occludes).
    fn scene(kind: StealthKind) -> (World, EntityId, EntityId) {
        let mut world = World::new();
        world.register::<GlobalTransform>();
        world.register::<Awareness>();
        world.register::<Sneaking>();
        world.register::<crate::systems::faction_hostility::CombatDisposition>();
        world.register::<byroredux_core::ecs::components::Dead>();
        let player = world.spawn();
        world.insert(player, GlobalTransform::new(Vec3::new(0.0, 0.0, -400.0), Quat::IDENTITY, 1.0));
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        // A bright lamp on the player: light level 1 (100 in the formula's range).
        world.register::<byroredux_core::ecs::components::LightSource>();
        let lamp = world.spawn();
        world.insert(lamp, GlobalTransform::new(Vec3::new(0.0, 0.0, -400.0), Quat::IDENTITY, 1.0));
        world.insert(
            lamp,
            byroredux_core::ecs::components::LightSource {
                emitter: byroredux_core::lighting::Emitter::from_legacy_world_units(
                    512.0,
                    [1.0, 1.0, 1.0],
                    byroredux_core::lighting::EmitterKind::Point,
                    [0.0; 3],
                    0.0,
                    1.0,
                    byroredux_core::lighting::VisibilityMask::for_legacy_local_light(),
                ),
                ..Default::default()
            },
        );
        let detector = world.spawn();
        world.insert(detector, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert(
            detector,
            crate::systems::faction_hostility::CombatDisposition(byroredux_plugin::esm::records::ActorAiData {
                aggression: byroredux_plugin::esm::records::Aggression::Unaggressive,
                confidence: byroredux_plugin::esm::records::Confidence::Average,
                responsibility: None,
                attack_radius: None,
            }),
        );
        world.insert_resource(StealthRules::resolve(kind, |name| match name {
            "fSneakBaseValue" => Some(-35.0),
            "fSneakMaxDistance" => Some(2500.0),
            _ => None,
        }).0);
        world.insert_resource(DetectionConfig { max_distance: 2500.0, exterior_mult: 2.0 });
        world.insert_resource(PlayerDetection::default());
        world.insert_resource(PlayerActionNoise::default());
        (world, player, detector)
    }

    /// SNK-DET-001: the tick writes a level toward the player and the summary; a fresh detector is
    /// evaluated at once and then waits its own period (SNK-DET-006).
    #[test]
    fn the_tick_writes_each_detectors_level_on_its_own_timer() {
        let (world, player, detector) = scene(StealthKind::FalloutNv);
        detection_tick_system(&world, 0.016);
        let first = *world.get::<Awareness>(detector).expect("evaluated on first sight");
        assert!(first.timer > 0.0 && first.timer <= 0.3 + 1e-6, "re-armed to the period");
        assert!(first.line_of_sight && first.in_cone);
        assert_eq!(world.resource::<PlayerDetection>().detector, Some(detector));
        // Standing in the light, facing the detector, unsneaking: seen.
        assert_eq!(first.level, DetectionLevel::Seen, "{:?}", first.terms);
        assert!(player_seen_by(&world, detector));
        assert!(detects(&world, detector, player));

        // Sneaking, out of the cone (the detector turns away): below Seen.
        world.query_mut::<Sneaking>().unwrap().insert(player, Sneaking);
        world.query_mut::<GlobalTransform>().unwrap().insert(
            detector,
            GlobalTransform::new(Vec3::ZERO, Quat::from_rotation_y(std::f32::consts::PI), 1.0),
        );
        detection_tick_system(&world, 0.1);
        assert_eq!(world.get::<Awareness>(detector).unwrap().level, DetectionLevel::Seen, "the timer has not run out");
        detection_tick_system(&world, 0.3);
        let turned = *world.get::<Awareness>(detector).unwrap();
        assert!(!turned.in_cone, "190° cone, player straight behind");
        assert_eq!(turned.level, DetectionLevel::Lost, "{:?}", turned.terms);
        assert!(!detects(&world, detector, player));
    }

    /// SNK-USE-010 (`detection.getdetectionlevel-player-target`): `GetDetectionLevel` / `GetDetected`
    /// toward the player answer the caller's real level through `SightQuery`, where the original
    /// answered 0. Boot installs `detects` as the query.
    #[test]
    fn get_detection_level_toward_the_player_answers_the_real_level() {
        let (world, player, detector) = scene(StealthKind::Fallout3);
        let seen = Awareness { level: DetectionLevel::Seen, ..Awareness::default() };
        world.query_mut::<Awareness>().unwrap().insert(detector, seen);
        let query = byroredux_scripting::SightQuery(detects);
        assert!((query.0)(&world, detector, player), "Seen toward the player answers 3 / 1");
        world.query_mut::<Awareness>().unwrap().insert(detector, Awareness::default());
        assert!(!(query.0)(&world, detector, player), "level Lost answers 0");
        let boot = include_str!("../boot/world.rs");
        assert!(boot.contains("byroredux_scripting::SightQuery(crate::detection::detects)"));
    }

    /// A sight-only row (Oblivion and Skyrim until Phase 5) answers the level from the sight test.
    #[test]
    fn a_sight_only_row_levels_from_the_sight_test() {
        let (world, _, detector) = scene(StealthKind::SightOnly);
        detection_tick_system(&world, 0.016);
        let awareness = *world.get::<Awareness>(detector).unwrap();
        assert_eq!(awareness.level, DetectionLevel::Seen);
        assert_eq!(awareness.terms, DetectionTerms::default(), "no formula terms");
    }
}
