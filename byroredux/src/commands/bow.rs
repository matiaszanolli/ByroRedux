//! The player's bow, from the console (player-archery PRD BOW-DX-001 to -003): `bow.status` and `bow.shoot`.
//! `bow.shoot` runs the same gate, spend and flight as the attack key's release, so a smoke script drives the real
//! path.

use byroredux_core::character::BowModel;
use byroredux_core::ecs::World;

use super::shared::*;
use crate::bow::{self, BowState};
use crate::systems::PlayerEntity;

/// `bow.shoot [draw seconds]` — release the player's bow after a draw of that many seconds (default: a full draw).
pub(crate) struct BowShootCommand;

impl ConsoleCommand for BowShootCommand {
    fn name(&self) -> &str {
        "bow.shoot"
    }

    fn description(&self) -> &str {
        "Release the player's bow as the attack key does (gate, arrow spent, arrow flies): bow.shoot [draw seconds]"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(model) = world.try_resource::<BowModel>().map(|model| *model) else {
            return CommandOutput::error("bow.shoot: this game has no bow rules");
        };
        let held = match args.split_whitespace().next() {
            Some(text) => match text.parse::<f32>() {
                Ok(seconds) if seconds.is_finite() && seconds >= 0.0 => seconds,
                _ => return CommandOutput::error("usage: bow.shoot [draw seconds]"),
            },
            None => PlayerEntity::get(world)
                .and_then(|player| bow::wielded_bow(world, player))
                .map_or(0.0, |wielded| model.full_draw_seconds(wielded.speed)),
        };
        match bow::shoot_now(world, held) {
            Ok(shot) => CommandOutput::line(shot.to_string()),
            Err(refusal) => CommandOutput::error(format!("bow.shoot: {refusal}")),
        }
    }
}

/// `bow.status` — the readied bow, the equipped arrow and its count, the last shot, the trajectory in effect, the
/// interval and the range (each labelled `documented` or `provisional`), and the skill feed.
pub(crate) struct BowStatusCommand;

impl ConsoleCommand for BowStatusCommand {
    fn name(&self) -> &str {
        "bow.status"
    }

    fn description(&self) -> &str {
        "Report the readied bow, the equipped arrow, the last shot, the trajectory, the interval and range, and the skill feed"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(model) = world.try_resource::<BowModel>().map(|model| *model) else {
            return CommandOutput::line("bow=none (this game has no bow rules)");
        };
        let Some(player) = PlayerEntity::get(world) else {
            return CommandOutput::error("bow.status: no player");
        };
        let mut lines = Vec::new();
        lines.push(match bow::wielded_bow(world, player) {
            Some(wielded) => format!(
                "bow={:08X} damage={:.0} speed={:.2} full_draw={:.2}s (documented)",
                wielded.weapon,
                wielded.damage,
                wielded.speed,
                model.full_draw_seconds(wielded.speed)
            ),
            None => "bow=none".to_owned(),
        });
        lines.push(match bow::equipped_ammo(world, player) {
            Some(arrow) => {
                let count = world
                    .get::<byroredux_core::ecs::components::Inventory>(player)
                    .map_or(0, |inventory| inventory.count_of(arrow));
                format!("arrow={arrow:08X} count={count}")
            }
            None => "arrow=none".to_owned(),
        });
        let state = world.try_resource::<BowState>();
        lines.push(match state.as_ref().and_then(|state| state.last.as_ref()) {
            Some(Ok(shot)) => format!("last={shot}"),
            Some(Err(refusal)) => format!("last=refused: {refusal}"),
            None => "last=none".to_owned(),
        });
        let (chosen, effective) = bow::trajectory(world);
        lines.push(if chosen == effective {
            format!("trajectory={} (documented)", effective.id())
        } else {
            format!(
                "trajectory={} (the setting asks for {}: no documented gravity rule yet, so it flies straight)",
                effective.id(),
                chosen.id()
            )
        });
        let remaining = state.as_ref().map_or(0.0, |state| state.cooldown);
        lines.push(format!(
            "interval={:.2}s (provisional: the original's follows the bow animation) cooldown={remaining:.2}s shots={}",
            bow::SHOT_INTERVAL_SECONDS,
            state.as_ref().map_or(0, |state| state.shots)
        ));
        let range = model.range(0.0);
        lines.push(if range.is_finite() {
            format!("range_cap={range:.0} BU (documented), the lesser of it and the arrow's own")
        } else {
            format!("range={:.0} BU (provisional: no source documents an arrow range here)", bow::OBLIVION_ARROW_RANGE_BU)
        });
        lines.push(damage_line(&model));
        lines.push(feed_line(world));
        CommandOutput::lines(lines)
    }
}

/// How the shot's damage is made and what is not modelled (BOW-DX-001): the rule and each declared gap.
fn damage_line(model: &BowModel) -> String {
    use byroredux_core::character::DamageRule;
    match model.damage {
        DamageRule::Rating { attribute, skill, .. } => format!(
            "damage=(bow + arrow) x {attribute}-and-{skill} rating x Fatigue term (documented); bow and arrow condition read as 1 (declared gap); the sneak multiplier, the target's armor rating and its Resist Normal Weapons act at the hit on the shared path (armor condition and Shield-like boosting effects are not modelled: declared gaps)"
        ),
        DamageRule::SkillBonus { skill, per_point } => format!(
            "damage=(bow + arrow) x (1 + {:.1}% x {skill}) (documented); a release before a full draw fires nothing (a declared deviation, no partial-draw rule is documented)",
            per_point * 100.0
        ),
    }
}

/// The skill feed: Oblivion's Marksman through the skill-use drain once code 28 is fed, and Skyrim's Archery unfed
/// until Skyrim has a weapon-skill emitter family (BOW-USE-003, BOW-USE-004).
fn feed_line(world: &World) -> String {
    let oblivion = world
        .try_resource::<byroredux_core::character::CharacterRuleset>()
        .is_some_and(|ruleset| ruleset.leveling.skill_use_costs().is_some());
    if oblivion && byroredux_scripting::oblivion_skill_use::FED_SKILLS.contains(&28) {
        "feed=Marksman fed".to_owned()
    } else if oblivion {
        "feed=Marksman unfed".to_owned()
    } else {
        "feed=Archery unfed".to_owned()
    }
}
