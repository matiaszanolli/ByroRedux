//! `perk.hooks` and `perk.compose` — the perk entry-point catalog and the traced composer on the console
//! and `byro-dbg` (docs/prd/perk-entry-points.md, PERK-DX-001..003).

use super::shared::*;
use byroredux_scripting::perk_entry_points::{
    compose_entry_point_traced, hook, hooks, HookState, PerkEntryTable, SkipReason,
};

/// `perk.hooks [consumed|pending|blocked|inert]` — the hook catalog for the loaded dialect, one line per hook
/// with its state (PERK-DX-002).
pub(crate) struct PerkHooksCommand;

impl ConsoleCommand for PerkHooksCommand {
    fn name(&self) -> &str {
        "perk.hooks"
    }

    fn description(&self) -> &str {
        "Perk entry-point catalog for the loaded game (usage: perk.hooks [consumed|pending|blocked|inert])"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let filter = args.trim().to_ascii_lowercase();
        if !filter.is_empty() && !["consumed", "pending", "blocked", "inert"].contains(&filter.as_str()) {
            return CommandOutput::error("usage: perk.hooks [consumed|pending|blocked|inert]");
        }
        let Some(dialect) = world.try_resource::<PerkEntryTable>().map(|table| table.dialect()) else {
            return CommandOutput::error("perk.hooks: no PERK table is installed (no plugin loaded)");
        };
        let rows = hooks(dialect);
        if rows.is_empty() {
            return CommandOutput::line(format!("perk.hooks: {dialect:?} has no perk entry-point catalog"));
        }
        let mut lines = vec![format!("perk.hooks: {dialect:?}, {} hooks", rows.len())];
        let mut shown = 0usize;
        for row in rows.iter().filter(|row| filter.is_empty() || row.state.name() == filter) {
            shown += 1;
            let subjects = match row.subjects {
                Some([]) => String::new(),
                Some(subjects) => format!(" [tabs: {}]", subjects.join(", ")),
                None => " [tabs: unsourced]".to_string(),
            };
            lines.push(format!("{:>3} {}{subjects} — {}", row.index, row.name, row.state.report()));
        }
        if !filter.is_empty() {
            lines.push(format!("({shown} {filter})"));
        }
        CommandOutput { lines }
    }
}

/// `perk.compose <hook> <base> [entity|player|.]` — compose `base` through one hook for an actor and print
/// every held entry on it: those applied with the running value, and those that did not with why
/// (PERK-DX-001, -003). No tab subject is supplied, which is what a call site with none sees.
pub(crate) struct PerkComposeCommand;

impl ConsoleCommand for PerkComposeCommand {
    fn name(&self) -> &str {
        "perk.compose"
    }

    fn description(&self) -> &str {
        "Compose a value through a perk hook (usage: perk.compose <hook> <base> [entity|player|.])"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut parts = args.split_whitespace();
        let (Some(hook_token), Some(base_token)) = (parts.next(), parts.next()) else {
            return CommandOutput::error("usage: perk.compose <hook> <base> [entity|player|.]");
        };
        let Ok(entry_point) = hook_token.parse::<u8>() else {
            return CommandOutput::error(format!("perk.compose: bad hook `{hook_token}` (a decimal hook index)"));
        };
        let Ok(base) = base_token.parse::<f32>() else {
            return CommandOutput::error(format!("perk.compose: bad base value `{base_token}`"));
        };
        let owner = match parts.next() {
            None | Some("player") => match crate::systems::PlayerEntity::get(world) {
                Some(player) => player,
                None => return CommandOutput::error("perk.compose: no player"),
            },
            Some(token) => match resolve_console_entity(world, token) {
                Ok(entity) => entity,
                Err(message) => return CommandOutput::error(format!("perk.compose: {message}")),
            },
        };
        let Some(dialect) = world.try_resource::<PerkEntryTable>().map(|table| table.dialect()) else {
            return CommandOutput::error("perk.compose: no PERK table is installed (no plugin loaded)");
        };
        let name = hook(dialect, entry_point).map_or("(no catalog row)", |row| row.name);
        let state = hook(dialect, entry_point).map_or_else(String::new, |row| format!(" — {}", row.state.report()));
        let composition = compose_entry_point_traced(world, owner, entry_point, base, &[]);

        let mut lines = vec![format!(
            "perk.compose: hook {entry_point} ({name}){state}: {base} -> {} for entity {owner}",
            composition.value
        )];
        for applied in &composition.applied {
            lines.push(format!("  applied  perk {:08X}  {:?}  -> {}", applied.perk, applied.op, applied.value));
        }
        for skipped in &composition.skipped {
            let why = match skipped.reason {
                SkipReason::WrongRank { authored, held } => {
                    format!("authored for rank {}, held at rank {held}", u32::from(authored) + 1)
                }
                SkipReason::SubjectGated { tab } => format!("Subject-gated: tab {tab} has no subject here"),
                SkipReason::Unmodelled { function_index } => {
                    format!("Unmodelled: condition function {function_index}")
                }
                SkipReason::ConditionsNotMet => "Fireable, conditions not met".to_string(),
            };
            lines.push(format!("  skipped  perk {:08X}  {:?}  ({why})", skipped.perk, skipped.op));
        }
        if composition.applied.is_empty() && composition.skipped.is_empty() {
            lines.push("  (the owner holds no entry on this hook)".to_string());
        }
        // A hook the catalog does not call Consumed is composed by nothing in the game yet.
        if let Some(row) = hook(dialect, entry_point) {
            if !matches!(row.state, HookState::Consumed { .. }) {
                lines.push("  note: no engine calculation composes this hook; the value is what a call site would see".to_string());
            }
        }
        CommandOutput { lines }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::character::Perks;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::{
        EsmIndex, PerkEntry, PerkEntryBody, PerkFunctionData, PerkRecord,
    };

    const SWIFT_LEARNER: u32 = 0x0003_1DD3;
    const WRONG_RANK: u32 = 0x0003_1DD4;

    fn entry(rank: u8, multiplier: f32) -> PerkEntry {
        PerkEntry {
            rank,
            priority: 0,
            body: PerkEntryBody::EntryPoint {
                entry_point_index: 9,
                function: 3,
                tab_count: 1,
                param_type: 1,
                function_data: PerkFunctionData::Float(multiplier),
                formatter: Vec::new(),
                extra_flags: Vec::new(),
                details: Default::default(),
            },
            condition_tabs: Vec::new(),
        }
    }

    fn world() -> (World, EntityId) {
        let mut index = EsmIndex {
            game: GameKind::Fallout3NV,
            character_rules: byroredux_core::character::CharacterRulesProfile::FALLOUT_NEW_VEGAS,
            ..Default::default()
        };
        for (form_id, rank) in [(SWIFT_LEARNER, 0), (WRONG_RANK, 1)] {
            index.perks.insert(form_id, PerkRecord { form_id, entries: vec![entry(rank, 1.1)], ..Default::default() });
        }
        let mut world = World::new();
        world.register::<Perks>();
        byroredux_scripting::perk_entry_points::install_perk_entry_points(&mut world, &index);
        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        let mut perks = Perks::default();
        perks.set_rank(SWIFT_LEARNER, 1);
        perks.set_rank(WRONG_RANK, 1);
        world.insert(player, perks);
        (world, player)
    }

    /// PERK-DX-001 / -003 -- the composed value, each applied entry with its running value, and the entry that
    /// did not apply with its reason.
    #[test]
    fn perk_compose_prints_applied_and_skipped_entries() {
        let (world, _) = world();
        let out = PerkComposeCommand.execute(&world, "9 100 player").lines.join("\n");
        assert!(out.contains("hook 9 (Adjust Experience Points)"), "{out}");
        assert!(out.contains("consumed: byroredux/src/combat.rs::award_kill_experience"), "{out}");
        assert!(out.contains("100 -> 110"), "{out}");
        assert!(out.contains(&format!("applied  perk {SWIFT_LEARNER:08X}")), "{out}");
        assert!(out.contains(&format!("skipped  perk {WRONG_RANK:08X}")) && out.contains("authored for rank 2, held at rank 1"), "{out}");
        // The default entity is the player.
        assert_eq!(PerkComposeCommand.execute(&world, "9 100").lines, PerkComposeCommand.execute(&world, "9 100 player").lines);
    }

    #[test]
    fn perk_compose_rejects_bad_arguments() {
        let (world, _) = world();
        for bad in ["", "9", "x 100", "9 y", "9 100 nobody"] {
            let out = PerkComposeCommand.execute(&world, bad).lines.join("\n");
            assert!(!out.is_empty(), "`{bad}` should explain itself");
        }
    }

    /// PERK-DX-002 -- one line per hook with its state; the filter narrows by state.
    #[test]
    fn perk_hooks_lists_the_catalog_and_filters_by_state() {
        let (world, _) = world();
        let all = PerkHooksCommand.execute(&world, "").lines;
        assert_eq!(all.len(), 1 + 74, "a header and one line per FNV hook");
        let inert = PerkHooksCommand.execute(&world, "inert").lines.join("\n");
        assert!(inert.contains("Adjust Range Penalty") && inert.contains("inert: GECK Entry Point"), "{inert}");
        assert!(!inert.contains("Calculate Weapon Damage"), "{inert}");
        assert!(PerkHooksCommand.execute(&world, "bogus").lines.join("").contains("usage"));
    }
}
