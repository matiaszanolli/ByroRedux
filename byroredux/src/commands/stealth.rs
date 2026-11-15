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
