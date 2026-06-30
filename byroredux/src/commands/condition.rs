//! `cond` — evaluate a CTDA condition function live against an entity.
//!
//! Exposes the `byroredux_scripting` condition-function catalog
//! (`GetActorValue`, `GetLevel`, `GetReputation`, …) to the in-engine
//! console *and* `byro-dbg` (the debug server's `Eval` request checks the
//! [`CommandRegistry`] before falling back to the Papyrus evaluator, so a
//! single registration reaches both surfaces). Mirrors the Bethesda console's
//! `prid <ref>` + `getav <av>` workflow: pick a ref with `prid`, then run
//! `cond . <Func> …` to read the function against it.

use super::shared::*;
use byroredux_plugin::esm::records::condition::Condition;
use byroredux_scripting::condition::{evaluate_function, ConditionFunction};

/// `cond <entity|.> <Function> [param1] [param2]` — evaluate one condition
/// function and print its `f32` result. `entity` is a decimal `EntityId` or
/// `.` for the current `prid` selection (`SelectedRef`). `param1` / `param2`
/// accept decimal or `0x`-hex (FormIDs are hex). `cond` / `cond list` prints
/// the available function names.
pub(crate) struct CondCommand;

impl ConsoleCommand for CondCommand {
    fn name(&self) -> &str {
        "cond"
    }

    fn description(&self) -> &str {
        "Evaluate a condition function: cond <entity|.> <Func> [p1] [p2] (cond list)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let args = args.trim();
        if args.is_empty() || args.eq_ignore_ascii_case("list") || args.eq_ignore_ascii_case("help")
        {
            return catalog_output();
        }

        let mut tok = args.split_whitespace();
        let entity_tok = tok.next().unwrap_or_default();
        let Some(func_name) = tok.next() else {
            return CommandOutput::error("cond: missing function name (try `cond list`)");
        };

        let entity = match resolve_entity(world, entity_tok) {
            Ok(e) => e,
            Err(msg) => return CommandOutput::error(msg),
        };

        let Some(func) = ConditionFunction::from_name(func_name) else {
            return CommandOutput::error(format!(
                "cond: unknown function `{func_name}` (try `cond list`)"
            ));
        };

        let param_1 = match parse_opt_u32(tok.next()) {
            Ok(v) => v,
            Err(t) => return CommandOutput::error(format!("cond: bad param1 `{t}`")),
        };
        let param_2 = match parse_opt_u32(tok.next()) {
            Ok(v) => v,
            Err(t) => return CommandOutput::error(format!("cond: bad param2 `{t}`")),
        };

        let cond = Condition {
            param_1,
            param_2,
            ..Default::default()
        };
        let result = evaluate_function(func, &cond, entity, world);

        CommandOutput::line(format!(
            "{}(0x{:X}, {}) on entity {} = {}",
            func.name(),
            param_1,
            param_2,
            entity,
            result
        ))
    }
}

/// `.` → the `SelectedRef` selection; otherwise a decimal `EntityId`.
fn resolve_entity(world: &World, tok: &str) -> Result<EntityId, String> {
    if tok == "." {
        return match world.try_resource::<SelectedRef>() {
            Some(sel) => sel
                .0
                .ok_or_else(|| "cond: no selection — `prid <id>` first".to_string()),
            None => Err("cond: SelectedRef resource not present".to_string()),
        };
    }
    tok.parse::<EntityId>()
        .map_err(|_| format!("cond: bad entity `{tok}` (decimal id or `.`)"))
}

/// Parse an optional decimal-or-`0x`-hex `u32`; absent → `0`. `Err` carries the
/// offending token for the error message.
fn parse_opt_u32(tok: Option<&str>) -> Result<u32, String> {
    let Some(t) = tok else { return Ok(0) };
    let parsed = if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16)
    } else {
        t.parse::<u32>()
    };
    parsed.map_err(|_| t.to_string())
}

fn catalog_output() -> CommandOutput {
    let mut lines =
        vec!["Condition functions (cond <entity|.> <Func> [p1] [p2]):".to_string()];
    for f in ConditionFunction::CATALOG {
        lines.push(format!("  {}", f.name()));
    }
    CommandOutput::lines(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::character::{Background, CharacterLevel};
    use byroredux_core::ecs::components::ActorValues;

    fn run(world: &World, args: &str) -> String {
        CondCommand.execute(world, args).lines.join("\n")
    }

    #[test]
    fn cond_evaluates_get_level_on_explicit_entity() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, CharacterLevel { level: 9, xp: 0 });
        let out = run(&world, &format!("{e} GetLevel"));
        assert!(out.contains("= 9"), "got: {out}");
    }

    #[test]
    fn cond_reads_actor_value_with_hex_param_and_is_case_insensitive() {
        let mut world = World::new();
        let e = world.spawn();
        let mut av = ActorValues::new();
        av.set_base(0x2C9, 100.0);
        world.insert(e, av);
        // lowercase function name + hex param resolve.
        let out = run(&world, &format!("{e} getactorvalue 0x2C9"));
        assert!(out.contains("GetActorValue(0x2C9, 0)"), "got: {out}");
        assert!(out.contains("= 100"), "got: {out}");
    }

    #[test]
    fn cond_resolves_selected_ref_with_dot() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(
            e,
            Background {
                race_form_id: 0x44C07,
                class_form_id: 0,
            },
        );
        world.insert_resource(SelectedRef(Some(e)));
        let out = run(&world, ". GetIsRace 0x44C07");
        assert!(out.contains("= 1"), "got: {out}");
    }

    #[test]
    fn cond_list_and_errors() {
        let mut world = World::new();
        assert!(run(&world, "list").contains("GetActorValue"));
        assert!(run(&world, "").contains("GetReputation"));
        assert!(run(&world, "5 NotAFunction").contains("unknown function"));
        assert!(run(&world, "notanentity GetLevel").contains("bad entity"));
        // `.` with the resource present but nothing selected.
        world.insert_resource(SelectedRef(None));
        assert!(run(&world, ". GetLevel").contains("no selection"));
    }
}
