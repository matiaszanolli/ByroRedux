//! M65 Phase 4 (SNK-HUD-001, -002, -005) — the detection indicator's
//! presentation: what the native HUD's `PanelSnapshot` and the MenuXml HUD
//! draw (`docs/engine/sneak-detection.md` §10).
//!
//! The state is the detection tick's fold ([`PlayerDetection::indicator`]); the
//! game's table, labels and gate are the resolved [`StealthIndicator`]. Shown
//! only while the player is in the stance and not swimming (`Fallout3.exe`
//! calls the meter routine only while its movement flags hold sneaking `0x400`
//! and not swimming `0x800`, `0x92dff6`), and on Starfield only with the
//! Stealth skill.

use super::PlayerDetection;
use byroredux_core::character::Perks;
use byroredux_core::ecs::World;
use byroredux_core::stealth::{IndicatorState, IndicatorStyle, StealthIndicator};
use byroredux_debug_ui::{StealthIndicatorLook, StealthIndicatorView};

/// The indicator the HUDs draw this frame, or `None` when it is hidden.
pub(crate) fn indicator_view(world: &World) -> Option<StealthIndicatorView> {
    let player = crate::systems::PlayerEntity::get(world)?;
    if !byroredux_scripting::stance::is_sneaking(world, player) || crate::systems::actor_is_swimming(world, player) {
        return None;
    }
    let indicator = world.try_resource::<StealthIndicator>().map(|i| i.clone())?;
    let passes = {
        let perks = world.get::<Perks>(player);
        indicator.gate_passes(|form| perks.as_ref().map_or(0, |p| p.rank(form)))
    };
    if !passes {
        return None;
    }
    let state = world.try_resource::<PlayerDetection>().map_or(IndicatorState::Hidden, |d| d.indicator);
    let look = match indicator.model.style {
        IndicatorStyle::None => return None,
        IndicatorStyle::Labels { .. } => StealthIndicatorLook::Label(indicator.label(state).clone()),
        IndicatorStyle::Eye => StealthIndicatorLook::Eye,
    };
    Some(StealthIndicatorView { state, look })
}

#[cfg(test)]
mod tests {
    //! PRD §7.2 scenario 9 (A9): the native HUD's `PanelSnapshot` field for
    //! each game's ruleset row, over every (level, hostility) pair and the
    //! alert states a detector carries with it, and its absence outside the
    //! stance. No image is compared.

    use super::*;
    use crate::detection::tick::fold;
    use byroredux_core::character::{CharacterRulesProfile, PerkRank};
    use byroredux_core::ecs::components::{Awareness, Sneaking};
    use byroredux_core::ecs::EntityId;
    use byroredux_core::stealth::{AlertState, DetectionLevel};

    const LEVELS: [DetectionLevel; 4] =
        [DetectionLevel::Lost, DetectionLevel::Unseen, DetectionLevel::Noticed, DetectionLevel::Seen];
    const ALERTS: [AlertState; 4] = [AlertState::Normal, AlertState::Alert, AlertState::Lost, AlertState::Combat];

    /// `Fallout3.exe` `0x937a70` read per detector (core `indicator` docs).
    fn fallout(level: DetectionLevel, alert: AlertState, hostile: bool) -> IndicatorState {
        match (level, alert, hostile) {
            (_, AlertState::Combat, _) => IndicatorState::Danger,
            (_, AlertState::Lost, _) => IndicatorState::Caution,
            (DetectionLevel::Seen, _, true) => IndicatorState::Caution,
            (DetectionLevel::Seen, _, false) => IndicatorState::Detected,
            _ => IndicatorState::Hidden,
        }
    }

    /// Oblivion's eye: lit while a detector sees or fights the player.
    fn oblivion(level: DetectionLevel, alert: AlertState, _hostile: bool) -> IndicatorState {
        let knows = level == DetectionLevel::Seen || matches!(alert, AlertState::Combat | AlertState::Lost);
        if knows { IndicatorState::Detected } else { IndicatorState::Hidden }
    }

    fn world_for(profile: CharacterRulesProfile, sneaking: bool) -> (World, EntityId, EntityId) {
        let mut world = World::new();
        world.register::<Sneaking>();
        world.register::<Perks>();
        world.register::<Awareness>();
        world.register::<byroredux_scripting::ActorControlState>();
        let player = world.spawn();
        if sneaking {
            world.insert(player, Sneaking);
        }
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        let indicator = StealthIndicator::resolve(profile.indicator(), |_| None, |id| (id == "Skill_Stealth").then_some(0x2CFCB2));
        world.insert_resource(indicator);
        world.insert_resource(PlayerDetection::default());
        let detector = world.spawn();
        (world, player, detector)
    }

    /// The view after one detector at (level, alert, hostile) is folded.
    fn view_with(world: &World, detector: EntityId, level: DetectionLevel, alert: AlertState, hostile: bool) -> Option<StealthIndicatorView> {
        let model = world.resource::<StealthIndicator>().model;
        let awareness = Awareness { level, alert, hostile, ..Default::default() };
        let mut summary = PlayerDetection::default();
        fold(world, &model, &mut summary, detector, &awareness);
        *world.resource_mut::<PlayerDetection>() = summary;
        indicator_view(world)
    }

    #[test]
    fn each_games_snapshot_shows_its_table_for_every_level_and_hostility() {
        type Expected = fn(DetectionLevel, AlertState, bool) -> IndicatorState;
        let games: [(CharacterRulesProfile, Expected, Option<[&str; 4]>); 6] = [
            (CharacterRulesProfile::FALLOUT3, fallout, Some(["[HIDDEN]", "[DETECTED]", "[CAUTION]", "[DANGER]"])),
            (CharacterRulesProfile::FALLOUT_NEW_VEGAS, fallout, Some(["[HIDDEN]", "[DETECTED]", "[CAUTION]", "[DANGER]"])),
            (CharacterRulesProfile::SKYRIM, fallout, Some(["[HIDDEN]", "[DETECTED]", "[CAUTION]", "[DANGER]"])),
            (CharacterRulesProfile::FALLOUT4, fallout, Some(["HIDDEN", "DETECTED", "CAUTION", "DANGER"])),
            (CharacterRulesProfile::STARFIELD, fallout, Some(["HIDDEN", "DETECTED", "CAUTION", "DANGER"])),
            (CharacterRulesProfile::OBLIVION, oblivion, None),
        ];
        for (profile, expected, labels) in games {
            let (world, player, detector) = world_for(profile, true);
            if profile.indicator().gate_perk.is_some() {
                assert_eq!(view_with(&world, detector, DetectionLevel::Seen, AlertState::Normal, false), None, "{}: ungated", profile.name());
                world.query_mut::<Perks>().unwrap().insert(player, Perks { entries: vec![PerkRank { perk_form_id: 0x2CFCB2, rank: 1 }] });
            }
            for level in LEVELS {
                for hostile in [false, true] {
                    for alert in ALERTS {
                        let want = expected(level, alert, hostile);
                        let view = view_with(&world, detector, level, alert, hostile)
                            .unwrap_or_else(|| panic!("{}: no indicator in the stance", profile.name()));
                        assert_eq!(view.state, want, "{}: {level:?} {alert:?} hostile {hostile}", profile.name());
                        let look = match labels {
                            Some(labels) => StealthIndicatorLook::Label(labels[want.index()].into()),
                            None => StealthIndicatorLook::Eye,
                        };
                        assert_eq!(view.look, look, "{}: {want:?}", profile.name());
                    }
                }
            }
        }
    }

    #[test]
    fn no_indicator_outside_the_stance_or_without_a_source() {
        for profile in [CharacterRulesProfile::FALLOUT_NEW_VEGAS, CharacterRulesProfile::OBLIVION, CharacterRulesProfile::SKYRIM] {
            let (world, _, detector) = world_for(profile, false);
            for level in LEVELS {
                assert_eq!(view_with(&world, detector, level, AlertState::Combat, true), None, "{}", profile.name());
            }
        }
        let (world, _, detector) = world_for(CharacterRulesProfile::FALLOUT76, true);
        assert_eq!(view_with(&world, detector, DetectionLevel::Seen, AlertState::Combat, true), None, "no sourced indicator");
    }

    /// SNK-DET-014: a flagged detector does not count while it is not hostile,
    /// and counts again once it is.
    #[test]
    fn an_actor_off_the_stealth_meter_counts_only_when_hostile() {
        let (mut world, _, detector) = world_for(CharacterRulesProfile::SKYRIM, true);
        world.insert(detector, byroredux_scripting::ActorControlState { not_shown_on_stealth_meter: true, ..Default::default() });
        let friendly = view_with(&world, detector, DetectionLevel::Seen, AlertState::Normal, false).unwrap();
        assert_eq!(friendly.state, IndicatorState::Hidden);
        let hostile = view_with(&world, detector, DetectionLevel::Seen, AlertState::Combat, true).unwrap();
        assert_eq!(hostile.state, IndicatorState::Danger);
    }

    /// Detectors disagree: the highest state wins, whatever the order.
    #[test]
    fn the_highest_state_any_detector_asks_for_wins() {
        let (mut world, _, first) = world_for(CharacterRulesProfile::FALLOUT3, true);
        let second = world.spawn();
        let model = world.resource::<StealthIndicator>().model;
        let seen = Awareness { level: DetectionLevel::Seen, ..Default::default() };
        let fighting = Awareness { level: DetectionLevel::Lost, alert: AlertState::Combat, hostile: true, ..Default::default() };
        for order in [[(first, seen), (second, fighting)], [(second, fighting), (first, seen)]] {
            let mut summary = PlayerDetection::default();
            for (entity, awareness) in order {
                fold(&world, &model, &mut summary, entity, &awareness);
            }
            assert_eq!(summary.indicator, IndicatorState::Danger);
            assert_eq!(summary.detector, Some(second));
        }
    }
}
