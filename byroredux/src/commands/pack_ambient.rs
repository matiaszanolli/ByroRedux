//! `pack.ambient` — why an actor runs the package it runs (docs/prd/ai-packages-completion.md §5.10,
//! AIP-DX-001, -005, -006).

use super::shared::*;
use crate::components::{AmbientPackageRuntime, NavPath};
use crate::npc_spawn::ai_package::{explain_ambient_packages, PackageExplanation};

/// `pack.ambient [entity|.]` — with an entity: its package list in priority order, one row per package with the
/// procedure, whether its schedule is open, whether its conditions passed and were fully modelled, and the
/// install or decline result; then its walk source and cached waypoint count. With no argument: every actor
/// that carries an `AmbientPackageRuntime`, with its winning package or why it has none.
pub(crate) struct PackAmbientCommand;

impl ConsoleCommand for PackAmbientCommand {
    fn name(&self) -> &str {
        "pack.ambient"
    }

    fn description(&self) -> &str {
        "Ambient AI packages: pack.ambient <entity|.> lists an actor's packages with schedule / condition / install result; no argument lists every ambient actor"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let token = args.trim();
        if token.is_empty() {
            return list_all(world);
        }
        let actor = match resolve_console_entity(world, token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("pack.ambient: {message}")),
        };
        let Some(rows) = explain_ambient_packages(world, actor) else {
            return CommandOutput::error(format!(
                "pack.ambient: entity {actor} has no ambient package runtime (or no package catalog is installed)"
            ));
        };
        let name = resolve_entity_name(world, actor).unwrap_or_else(|| "(unnamed)".into());
        let mut lines = vec![format!("pack.ambient: entity {actor} {name}: {} package(s), priority order", rows.len())];
        lines.extend(rows.iter().map(format_row));
        lines.push(walk_line(world, actor));
        CommandOutput { lines }
    }
}

fn format_row(row: &PackageExplanation) -> String {
    format!(
        "{} {:08X} {:<28} {:<14} schedule {:<6} conditions {} {} -> {}",
        if row.winner { "*" } else { " " },
        row.form_id,
        if row.editor_id.is_empty() { "-" } else { &row.editor_id },
        row.procedure,
        if row.schedule_open { "open" } else { "closed" },
        if row.conditions_pass { "pass" } else { "fail" },
        if row.conditions_modelled { "(modelled)" } else { "(NOT fully modelled)" },
        row.result
    )
}

/// The actor's walk source and cached waypoint count (AIP-DX-005): a cached `NavPath` with waypoints is a
/// navmesh path, one without is the straight-line fallback the pathing falls back to, none means it has not
/// walked toward a goal yet.
fn walk_line(world: &World, actor: EntityId) -> String {
    match world.get::<NavPath>(actor) {
        Some(path) if !path.waypoints.is_empty() => {
            format!("walk: navmesh path, {} cached waypoint(s)", path.waypoints.len())
        }
        Some(_) => "walk: straight line (no resident-tile path for the goal), 0 cached waypoints".to_string(),
        None => "walk: none yet (no path cached), 0 cached waypoints".to_string(),
    }
}

fn list_all(world: &World) -> CommandOutput {
    let actors: Vec<EntityId> = match world.query::<AmbientPackageRuntime>() {
        Some(runtimes) => runtimes.iter().map(|(actor, _)| actor).collect(),
        None => Vec::new(),
    };
    let mut lines = vec![format!("pack.ambient: {} actor(s) with an ambient package runtime", actors.len())];
    for actor in actors {
        let name = resolve_entity_name(world, actor).unwrap_or_else(|| "(unnamed)".into());
        let summary = match explain_ambient_packages(world, actor) {
            None => "no package catalog".to_string(),
            Some(rows) => match rows.iter().find(|row| row.winner) {
                Some(winner) => format!(
                    "{:08X} {} ({})",
                    winner.form_id,
                    if winner.editor_id.is_empty() { "-" } else { &winner.editor_id },
                    winner.procedure
                ),
                None => no_winner_reason(&rows),
            },
        };
        lines.push(format!("  entity {actor} {name}: {summary}"));
    }
    CommandOutput { lines }
}

/// Why no package won: the first decline of a package whose schedule is open and conditions pass, else the
/// schedule or condition that held the rest back.
fn no_winner_reason(rows: &[PackageExplanation]) -> String {
    if rows.is_empty() {
        return "no winner: the actor has no packages".into();
    }
    if let Some(declined) = rows.iter().find(|row| row.schedule_open && row.conditions_pass && row.result.starts_with("declines")) {
        return format!("no winner: {:08X} {}", declined.form_id, declined.result);
    }
    if rows.iter().all(|row| !row.schedule_open) {
        return "no winner: every schedule is closed".into();
    }
    "no winner: every open package fails its conditions".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(form_id: u32, open: bool, pass: bool, result: &str, winner: bool) -> PackageExplanation {
        PackageExplanation {
            form_id,
            editor_id: format!("Pack{form_id:X}"),
            procedure: "Find".into(),
            schedule_open: open,
            conditions_pass: pass,
            conditions_modelled: true,
            result: result.into(),
            winner,
        }
    }

    /// AIP-DX-001 -- a row names the procedure, the schedule, the conditions and the install result.
    #[test]
    fn a_row_carries_every_selection_fact() {
        let line = format_row(&row(0x1234, true, true, "declines: unsupported-object-type 8", false));
        for needle in ["00001234", "Pack1234", "Find", "schedule open", "conditions pass", "(modelled)", "declines: unsupported-object-type 8"] {
            assert!(line.contains(needle), "{needle}: {line}");
        }
        assert!(format_row(&row(1, true, true, "installs", true)).starts_with('*'), "the winner is starred");
        let unmodelled = PackageExplanation { conditions_modelled: false, ..row(2, false, false, "installs", false) };
        let line = format_row(&unmodelled);
        assert!(line.contains("schedule closed") && line.contains("fail") && line.contains("NOT fully modelled"), "{line}");
    }

    /// AIP-DX-006 -- an actor with no winner says why.
    #[test]
    fn a_missing_winner_is_explained() {
        assert!(no_winner_reason(&[]).contains("no packages"));
        assert!(no_winner_reason(&[row(1, false, true, "installs", false)]).contains("every schedule is closed"));
        assert!(no_winner_reason(&[row(1, true, false, "installs", false)]).contains("fails its conditions"));
        let declined = no_winner_reason(&[row(7, true, true, "declines: unsupported-object-type 8", false)]);
        assert!(declined.contains("00000007") && declined.contains("unsupported-object-type 8"), "{declined}");
    }

    #[test]
    fn walk_source_reports_the_cached_waypoints() {
        let mut world = World::new();
        world.register::<NavPath>();
        let actor = world.spawn();
        assert!(walk_line(&world, actor).contains("none yet"));
        world.insert(
            actor,
            NavPath {
                goal: Vec3::ZERO,
                residency_generation: 0,
                waypoints: [Vec3::X, Vec3::Y].into_iter().collect(),
            },
        );
        assert!(walk_line(&world, actor).contains("navmesh path, 2 cached waypoint(s)"));
        world.insert(actor, NavPath { goal: Vec3::ZERO, residency_generation: 0, waypoints: Default::default() });
        assert!(walk_line(&world, actor).contains("straight line"));
    }
}
