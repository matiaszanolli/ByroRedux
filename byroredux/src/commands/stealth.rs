//! `stealth.*` — the sneak stance and detection console surface (M65,
//! `docs/engine/sneak-detection.md` §11). `stealth.sneak` sets the stance
//! through the same path the Sneak key takes (SNK-DX-002), so a headless smoke
//! can drive it; `stealth.status` joins it with Phase 2's detection tick.

use super::shared::*;
use crate::systems::stance::set_player_sneaking;
use crate::systems::PlayerEntity;
use byroredux_core::ecs::World;

/// `stealth.sneak [on|off]` — enter or leave the sneak stance through the
/// key's path; with no argument, report the stance.
pub(crate) struct StealthSneakCommand;

impl ConsoleCommand for StealthSneakCommand {
    fn name(&self) -> &str {
        "stealth.sneak"
    }
    fn description(&self) -> &str {
        "Enter (on) or leave (off) the sneak stance through the Sneak key's path; no argument reports it"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let sneak = match args.trim().to_ascii_lowercase().as_str() {
            "" => {
                let Some(player) = PlayerEntity::get(world) else {
                    return CommandOutput::error("stealth: no player");
                };
                let sneaking = byroredux_scripting::stance::is_sneaking(world, player);
                return CommandOutput::line(format!("stealth: {}", stance_word(sneaking)));
            }
            "on" | "1" => true,
            "off" | "0" => false,
            other => return CommandOutput::error(format!("stealth.sneak: expected on|off, got `{other}`")),
        };
        match set_player_sneaking(world, Some(sneak)) {
            Ok(sneaking) => CommandOutput::line(format!("stealth: {}", stance_word(sneaking))),
            Err(reason) => CommandOutput::error(format!("stealth.sneak: {reason}")),
        }
    }
}

/// `stealth.status` — the detection rules row, the player's stance and worst
/// level, then one line per detector: distance, line of sight, cone, each
/// formula term, the value, level, alert state and timer (SNK-DX-001).
pub(crate) struct StealthStatusCommand;

impl ConsoleCommand for StealthStatusCommand {
    fn name(&self) -> &str {
        "stealth.status"
    }
    fn description(&self) -> &str {
        "Detection rules, the player's stance, worst level and indicator, and every detector's terms, value, level and alert state"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(rules) = world.try_resource::<byroredux_core::stealth::StealthRules>().map(|r| *r) else {
            return CommandOutput::error("stealth: no detection rules (no load order installed)");
        };
        let player = PlayerEntity::get(world);
        let sneaking = player.is_some_and(|p| byroredux_scripting::stance::is_sneaking(world, p));
        let summary = world.try_resource::<crate::detection::PlayerDetection>().map(|d| *d).unwrap_or_default();
        let indicator = match crate::detection::indicator_view(world) {
            Some(view) => match view.look {
                byroredux_native_ui::StealthIndicatorLook::Label(label) => format!("{:?} \"{label}\"", view.state),
                byroredux_native_ui::StealthIndicatorLook::Eye => format!("{:?} (eye)", view.state),
            },
            None => "hidden".to_string(),
        };
        let mut lines = vec![format!(
            "stealth: rules {:?}, {}, worst level {:?} (alert {:?}, hostile {}), indicator {indicator}",
            rules.kind,
            stance_word(sneaking),
            summary.level,
            summary.alert,
            summary.hostile
        )];
        let mut rows: Vec<(byroredux_core::ecs::EntityId, byroredux_core::ecs::components::Awareness)> = world
            .query::<byroredux_core::ecs::components::Awareness>()
            .map(|q| q.iter().map(|(entity, awareness)| (entity, *awareness)).collect())
            .unwrap_or_default();
        rows.sort_by(|a, b| a.1.distance.total_cmp(&b.1.distance));
        for (entity, a) in rows {
            let form = world
                .get::<byroredux_core::ecs::components::FormIdComponent>(entity)
                .map_or(String::from("-"), |form| format!("{:?}", form.0));
            let at = world
                .get::<byroredux_core::ecs::components::GlobalTransform>(entity)
                .map_or(String::from("(unplaced)"), |t| {
                    format!("({:.0}, {:.0}, {:.0})", t.translation.x, t.translation.y, t.translation.z)
                });
            lines.push(format!(
                "  entity {} ref {form} at {at} dist {:.0} los {} cone {} sound {:.2} visual {:.2} skill {:.2} value {:.2} level {:?} alert {:?} hostile {} next {:.2}s",
                entity, a.distance, a.line_of_sight, a.in_cone, a.terms.sound, a.terms.visual, a.terms.skill, a.terms.value,
                a.level, a.alert, a.hostile, a.timer
            ));
        }
        CommandOutput::lines(lines)
    }
}

fn stance_word(sneaking: bool) -> &'static str {
    if sneaking { "sneaking" } else { "standing" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::PlayerMode;
    use byroredux_core::ecs::components::Sneaking;

    #[test]
    fn stealth_sneak_sets_and_reports_the_stance() {
        let mut world = World::new();
        byroredux_scripting::stance::register(&mut world);
        world.insert_resource(PlayerMode::Character);
        let player = world.spawn();
        world.insert_resource(PlayerEntity(Some(player)));
        let run = |args: &str| StealthSneakCommand.execute(&world, args).lines.join("\n");

        assert_eq!(run("on"), "stealth: sneaking");
        assert!(world.has::<Sneaking>(player));
        assert_eq!(run(""), "stealth: sneaking");
        assert_eq!(run("off"), "stealth: standing");
        assert!(!world.has::<Sneaking>(player));
        assert!(run("sideways").starts_with("Error:"));
    }
}
