//! The player's cast, from the console (player-spellcasting PRD PSC-RDY-002, PSC-DX-001 to -003): `spell.ready`,
//! `spell.cast`, `spell.list` and `cast.status`. `spell.cast` runs the same gate and settle as the cast key, so a smoke
//! script drives the real path; the console's `magic.cast` stays the uncharged, unfed script path.

use byroredux_core::ecs::World;
use byroredux_scripting::player_cast::{self, CastState, Impact};

use super::shared::*;
use crate::player_cast::CAST_INTERVAL_SECONDS;
use crate::systems::PlayerEntity;

/// `spell.ready <spell formid>` — ready a spell the player knows; reports its name, school and actual cost.
pub(crate) struct SpellReadyCommand;

impl ConsoleCommand for SpellReadyCommand {
    fn name(&self) -> &str {
        "spell.ready"
    }

    fn description(&self) -> &str {
        "Ready a spell the player knows for the cast action: spell.ready <spell formid>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(spell) = args.split_whitespace().next().and_then(parse_console_form_id) else {
            return CommandOutput::error("usage: spell.ready <spell formid>");
        };
        let Some(player) = PlayerEntity::get(world) else {
            return CommandOutput::error("spell.ready: no player");
        };
        match player_cast::ready_spell(world, player, spell) {
            Ok(preview) => CommandOutput::line(format!(
                "readied {} ({spell:08X}): school {}, tier {}, costs {:.1} magicka",
                preview.name,
                player_cast::school_name(preview.school),
                preview.tier,
                preview.cost
            )),
            Err(refusal) => CommandOutput::error(format!("spell.ready: {spell:08X}: {refusal}")),
        }
    }
}

/// `spell.cast` — one cast action for the player, the cast key's own path.
pub(crate) struct SpellCastCommand;

impl ConsoleCommand for SpellCastCommand {
    fn name(&self) -> &str {
        "spell.cast"
    }

    fn description(&self) -> &str {
        "Cast the readied spell as the cast key does (gate, Magicka and Fatigue, effects); magic.cast is the uncharged script path"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        match crate::player_cast::cast_now(world) {
            Some(report) if report.outcome.is_ok() => CommandOutput::line(report.to_string()),
            Some(report) => CommandOutput::error(format!("spell.cast: {report}")),
            None => CommandOutput::error("spell.cast: no player"),
        }
    }
}

/// `spell.list` — the player's castable spells with their school, tier and what they cost now.
pub(crate) struct SpellListCommand;

impl ConsoleCommand for SpellListCommand {
    fn name(&self) -> &str {
        "spell.list"
    }

    fn description(&self) -> &str {
        "List the player's castable spells (readied marked *): school, tier and actual cost"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(player) = PlayerEntity::get(world) else {
            return CommandOutput::error("spell.list: no player");
        };
        let readied = player_cast::readied(world, player);
        let spells = player_cast::castable_spells(world, player);
        if spells.is_empty() {
            return CommandOutput::line("no castable spells (none known, or this game has no cast rules)");
        }
        let mut lines = vec![format!("{} castable spell(s)", spells.len())];
        for spell in spells {
            if let Ok(preview) = player_cast::preview(world, player, spell) {
                lines.push(format!(
                    "{} {spell:08X} {:28} {:11} tier {} cost {:.1}",
                    if readied == Some(spell) { "*" } else { " " },
                    preview.name,
                    player_cast::school_name(preview.school),
                    preview.tier,
                    preview.cost
                ));
            }
        }
        CommandOutput::lines(lines)
    }
}

/// `cast.status` — the readied spell, the last cast and the interval in force (PSC-DX-001).
pub(crate) struct CastStatusCommand;

impl ConsoleCommand for CastStatusCommand {
    fn name(&self) -> &str {
        "cast.status"
    }

    fn description(&self) -> &str {
        "Report the readied spell, the last cast (started or the refusal), the interval, the touch reach and the last projectile's fate"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(player) = PlayerEntity::get(world) else {
            return CommandOutput::error("cast.status: no player");
        };
        let readied = player_cast::readied(world, player);
        let mut lines = vec![match readied {
            Some(spell) => match player_cast::preview(world, player, spell) {
                Ok(preview) => format!(
                    "readied={} ({spell:08X}) school={} tier={} cost={:.1}",
                    preview.name,
                    player_cast::school_name(preview.school),
                    preview.tier,
                    preview.cost
                ),
                Err(refusal) => format!("readied={spell:08X} ({refusal})"),
            },
            None => "readied=none".to_owned(),
        }];
        let state = world.try_resource::<CastState>();
        lines.push(match state.as_ref().and_then(|state| state.last.as_ref()) {
            Some(report) => format!("last={report}"),
            None => "last=none".to_owned(),
        });
        let remaining = state.as_ref().map_or(0.0, |state| state.cooldown_remaining);
        // #7235 — the interval the last cast armed (`CastState.interval`), the engine's default before any cast.
        let interval = state.as_ref().map(|state| state.interval).filter(|interval| *interval > 0.0).unwrap_or(CAST_INTERVAL_SECONDS);
        lines.push(format!("interval={interval:.2}s (provisional: the original's follows the cast animation) cooldown={remaining:.2}s"));
        let model = world.try_resource::<byroredux_core::character::SpellCastModel>().map(|model| *model);
        lines.push(match model {
            Some(model) => format!(
                "touch_reach={:.1} BU (iActivatePickLength) target_speed={:.0} BU/s x the MGEF's multiplier, target_range={:.0} BU",
                model.touch_pick_length,
                model.projectile_base_speed,
                model.projectile_max_distance
            ),
            None => "touch_reach=none (this game has no cast rules)".to_owned(),
        });
        lines.push(match state.as_ref().and_then(|state| state.impact) {
            Some(Impact::Flying) => "impact=flying".to_owned(),
            Some(Impact::Landed { target, started, fed }) => format!(
                "impact=landed on {target} (effects {}{})",
                if started { "applied" } else { "none applied" },
                fed.map_or(String::new(), |school| format!(", fed {}", player_cast::school_name(Some(school))))
            ),
            Some(Impact::Missed) => "impact=missed (no live actor met)".to_owned(),
            None => "impact=none".to_owned(),
        });
        CommandOutput::lines(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #7235 — `cast.status` reports the interval the last cast armed, and the engine's default before any cast.
    #[test]
    fn cast_status_reports_the_interval_the_last_cast_armed() {
        let mut world = World::new();
        let player = world.spawn();
        world.insert_resource(PlayerEntity(Some(player)));
        let interval_line = |world: &World| {
            CastStatusCommand.execute(world, "").lines.into_iter().find(|line| line.starts_with("interval=")).expect("an interval line")
        };
        world.insert_resource(CastState::default());
        assert!(interval_line(&world).starts_with(&format!("interval={CAST_INTERVAL_SECONDS:.2}s")), "no cast yet: the default");
        world.insert_resource(CastState { interval: 2.75, ..CastState::default() });
        assert!(interval_line(&world).starts_with("interval=2.75s"), "{}", interval_line(&world));
    }
}
