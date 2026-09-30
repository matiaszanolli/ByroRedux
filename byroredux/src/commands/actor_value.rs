//! `setav` / `modav` — live-edit an actor's [`ActorValues`] from the console.
//!
//! The write complement to the `cond` read functions: change a SPECIAL / skill
//! / resource on a spawned actor, then read the dependent derived stat straight
//! back with `cond <e> GetActorValue <derivedAVIF>` (e.g. raise Strength and
//! watch Carry Weight recompute). Mirrors the Bethesda console `setav` /
//! `modav`. Both reach the in-engine console *and* `byro-dbg` via the shared
//! [`CommandRegistry`]. They edit an **existing** `ActorValues` component
//! (populated at NPC spawn); they do not create one (structural insertion
//! needs `&mut World`, which a command doesn't hold).

use super::shared::*;
use byroredux_core::ecs::components::ActorValues;

/// `setav <entity|.> <av_formid> <value>` — set an actor value's **base**.
pub(crate) struct SetAvCommand;

impl ConsoleCommand for SetAvCommand {
    fn name(&self) -> &str {
        "setav"
    }

    fn description(&self) -> &str {
        "Set an actor value's base: setav <entity|.> <av_formid> <value>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        edit_av(world, args, "setav", AvEdit::SetBase)
    }
}

/// `modav <entity|.> <av_formid> <delta>` — add a **permanent** modifier to an
/// actor value (the composed `current` shifts by `delta`).
pub(crate) struct ModAvCommand;

impl ConsoleCommand for ModAvCommand {
    fn name(&self) -> &str {
        "modav"
    }

    fn description(&self) -> &str {
        "Add a permanent modifier: modav <entity|.> <av_formid> <delta>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        edit_av(world, args, "modav", AvEdit::ModPermanent)
    }
}

enum AvEdit {
    SetBase,
    ModPermanent,
}

fn edit_av(world: &World, args: &str, cmd: &str, edit: AvEdit) -> CommandOutput {
    let mut tok = args.split_whitespace();
    let (Some(entity_tok), Some(av_tok), Some(val_tok)) = (tok.next(), tok.next(), tok.next())
    else {
        return CommandOutput::error(format!("usage: {cmd} <entity|.> <av_formid> <value>"));
    };

    let entity = match resolve_console_entity(world, entity_tok) {
        Ok(e) => e,
        Err(msg) => return CommandOutput::error(format!("{cmd}: {msg}")),
    };
    let Some(av) = parse_console_u32(av_tok) else {
        return CommandOutput::error(format!("{cmd}: bad actor-value FormID `{av_tok}`"));
    };
    let Ok(value) = val_tok.parse::<f32>() else {
        return CommandOutput::error(format!("{cmd}: bad value `{val_tok}`"));
    };

    let Some(mut q) = world.query_mut::<ActorValues>() else {
        return CommandOutput::error(format!("{cmd}: no ActorValues storage in the world"));
    };
    let Some(avs) = q.get_mut(entity) else {
        return CommandOutput::error(format!(
            "{cmd}: entity {entity} has no ActorValues (not a populated actor?)"
        ));
    };

    let before = avs.current(av);
    match edit {
        AvEdit::SetBase => avs.set_base(av, value),
        AvEdit::ModPermanent => avs.mod_permanent(av, value),
    }
    let after = avs.current(av);

    CommandOutput::line(format!(
        "{cmd} 0x{av:X} on entity {entity}: {before} -> {after}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(world: &World, args: &str, set: bool) -> String {
        let out = if set {
            SetAvCommand.execute(world, args)
        } else {
            ModAvCommand.execute(world, args)
        };
        out.lines.join("\n")
    }

    #[test]
    fn setav_sets_base_and_modav_adds_permanent() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, ActorValues::new());

        // setav Strength(0x05) = 7
        let out = run(&world, &format!("{e} 0x05 7"), true);
        assert!(out.contains("0 -> 7"), "got: {out}");
        // modav +3 → current 10
        let out = run(&world, &format!("{e} 0x05 3"), false);
        assert!(out.contains("7 -> 10"), "got: {out}");
        // Confirm via the component.
        let q = world.query::<ActorValues>().unwrap();
        assert_eq!(q.get(e).unwrap().current(0x05), 10.0);
    }

    /// #5039 — `modav Endurance` on the player moves FO4 Health once the
    /// refresh system runs; the spawn stamp is not frozen.
    #[test]
    fn modav_endurance_rescales_the_players_fo4_health() {
        use byroredux_core::character::{
            CharacterLevel, CharacterRuleset, DerivedInput, DerivedStatFormula, LevelingModel,
        };
        const END: u32 = 0x07;
        const HEALTH: u32 = 0x2D4;
        let ruleset = CharacterRuleset::new(LevelingModel::FO4).with_derived(
            HEALTH,
            // FO4 player Health, as `fallout4_ruleset` registers it.
            DerivedStatFormula::bilinear(
                DerivedInput::actor_value(END),
                4.5,
                DerivedInput::LEVEL,
                2.5,
                0.5,
                77.5,
            )
            .floored()
            .player_only(),
        );
        let mut world = World::new();
        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        let mut values = ActorValues::from_pairs([(END, 5.0)]);
        ruleset.refresh_player_only_bases(&mut values, 1); // the #4674 stamp
        world.insert(player, values);
        world.insert(player, CharacterLevel { level: 1, xp: 0 });
        world.insert_resource(ruleset);
        let health = |world: &World| world.get::<ActorValues>(player).unwrap().current(HEALTH);
        assert_eq!(health(&world), 105.0);

        run(&world, &format!("{player} 0x07 2"), false);
        crate::systems::player_derived_stats_system(&world, 0.0);
        // floor(77.5 + 4.5·7 + 2.5 + 0.5·7) = 115.
        assert_eq!(health(&world), 115.0);
    }

    #[test]
    fn errors_on_missing_component_and_bad_args() {
        let mut world = World::new();
        let bare = world.spawn(); // no ActorValues
        assert!(run(&world, &format!("{bare} 0x05 7"), true).contains("no ActorValues"));
        assert!(run(&world, "5 0x05", true).contains("usage"));
        assert!(run(&world, "notanentity 0x05 7", true).contains("bad entity"));
        assert!(run(&world, "5 nothex 7", true).contains("bad actor-value FormID"));
        assert!(run(&world, "5 0x05 notnum", true).contains("bad value"));
    }
}
