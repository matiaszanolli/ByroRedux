//! Native HUD objective presentation — the P3 objective-text consumer.
//!
//! Composes the player-facing objective lines from canonical quest state:
//! [`QuestStageState`] (which quests are running), [`QuestObjectiveState`]
//! (which objectives a fragment has displayed and not yet completed/failed),
//! and [`QuestDefinitionRegistry`] (the authored display text). Pure
//! presentation consumer: no quest state is read through debug plumbing,
//! mutated, or serialized here, mirroring the [`crate::notifications`] and
//! [`crate::inventory`] HUD producers.
//!
//! Bethesda's journal shows one active objective per quest; this snapshot
//! keeps the same spirit with a bounded line cap, ordered deterministically
//! by (quest FormID, objective index) so a HashMap iteration order can never
//! reorder or reshuffle what the HUD shows.

use byroredux_core::ecs::World;
use byroredux_scripting::quest_stages::{
    ObjectiveStatus, QuestDefinitionRegistry, QuestObjectiveState, QuestStageState,
};
use byroredux_scripting::QuestStatus;

/// Maximum objective lines the HUD shows. Bounds the per-frame allocation
/// and keeps a many-quest session readable; selection is deterministic, so
/// truncation cuts the highest (quest, objective) pair, not a random one.
const MAX_OBJECTIVE_LINES: usize = 4;

/// Journal prose is single-line on the HUD; legacy content ships CRLF and
/// long `NNAM`/`CNAM` strings, so flatten and cap rather than wrap.
const MAX_OBJECTIVE_CHARS: usize = 96;

/// Compose the active-objective HUD lines. Returns `None` when quest state
/// is absent (no quest runtime installed) or no running quest has a
/// displayed, unfinished, authored objective — the HUD then draws nothing.
///
/// Lock-order posture (#313): exactly one resource guard is held at a time,
/// each dropped before the next is taken, so this producer can never
/// participate in a cross-storage acquisition cycle — the parallel-systems
/// discipline `inventory::consume_item` documents for its catalog guard.
pub(crate) fn snapshot(world: &World) -> Option<Vec<byroredux_debug_ui::ObjectiveView>> {
    let running: Vec<byroredux_scripting::QuestFormId> = world
        .try_resource::<QuestStageState>()?
        .iter()
        .filter(|(_, data)| data.status == QuestStatus::Running)
        .map(|(quest, _)| quest)
        .collect();
    if running.is_empty() {
        return None;
    }
    let candidates: Vec<(byroredux_scripting::QuestFormId, Vec<(i32, ObjectiveStatus)>)> = {
        let objective_state = world.try_resource::<QuestObjectiveState>()?;
        running
            .iter()
            .map(|&quest| (quest, objective_state.iter_quest(quest).collect()))
            .collect()
    };
    let candidates = candidates
        .into_iter()
        .filter(|(_, objectives)| {
            objectives
                .iter()
                .any(|(_, status)| status.displayed && !status.completed && !status.failed)
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    let definitions = world.try_resource::<QuestDefinitionRegistry>()?;
    // Collect every candidate, then order — an early break would let the
    // stage map's iteration order decide which quest survives the cap.
    let mut lines: Vec<(u32, i32, byroredux_debug_ui::ObjectiveView)> = Vec::new();
    for (quest, objectives) in candidates {
        for (index, status) in objectives {
            if !status.displayed || status.completed || status.failed {
                continue;
            }
            let Some(record) = definitions.objective(quest, index) else {
                continue;
            };
            let text = record.text.trim();
            if text.is_empty() {
                continue;
            }
            lines.push((
                quest.0,
                index,
                byroredux_debug_ui::ObjectiveView {
                    quest: quest_display_name(&definitions, quest),
                    text: flatten_text(text),
                },
            ));
        }
    }

    lines.sort_by_key(|line| (line.0, line.1));
    lines.truncate(MAX_OBJECTIVE_LINES);
    (!lines.is_empty()).then(|| lines.into_iter().map(|(_, _, view)| view).collect())
}

/// Authored display name for a quest, falling back editor id → formatted
/// FormID so an unnamed quest still labels its objective honestly.
fn quest_display_name(definitions: &QuestDefinitionRegistry, quest: byroredux_scripting::QuestFormId) -> String {
    definitions
        .full_name(quest)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            definitions
                .editor_id(quest)
                .map(str::trim)
                .filter(|name| !name.is_empty())
        })
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Quest 0x{:08X}", quest.0))
}

fn flatten_text(text: &str) -> String {
    let flattened = text.replace(['\r', '\n'], " ");
    let mut chars = flattened.chars();
    let mut out: String = chars.by_ref().take(MAX_OBJECTIVE_CHARS).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::{QuestObjective as AuthoredObjective, QustRecord};
    use byroredux_scripting::QuestFormId;

    fn quest_world() -> World {
        let mut world = World::new();
        byroredux_scripting::quest_stages::register(&mut world);
        // `register` installs the definition/start registries; the live
        // stage/objective stores are inserted by the boot quest setup, so the
        // test inserts them the same way.
        world.insert_resource(QuestStageState::default());
        world.insert_resource(QuestObjectiveState::default());
        world
    }

    fn authored_quest(raw: u32, name: &str) -> QustRecord {
        QustRecord {
            form_id: raw,
            editor_id: format!("Q{raw:04X}"),
            full_name: name.to_owned(),
            skyrim_plus: true,
            objectives: vec![
                AuthoredObjective {
                    index: 10,
                    text: format!("Do the thing in {name}"),
                    ..Default::default()
                },
                AuthoredObjective {
                    index: 20,
                    text: format!("Finish {name}"),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    /// Install through the real definition-install path
    /// (`install_start_game_quests` is public and builds
    /// `QuestDefinitionRegistry` from QUST records; none of these carry the
    /// Start-Game-Enabled flag, so only the definition table fills). One
    /// call per registry, because the call replaces the definition table.
    fn install_authored_quests(world: &mut World, quests: &[QustRecord]) {
        byroredux_scripting::quest_stages::install_start_game_quests(world, quests.to_vec());
    }

    #[test]
    fn displayed_unfinished_objectives_compose_into_named_lines() {
        let mut world = quest_world();
        let quest = QuestFormId(0x1000);
        install_authored_quests(&mut world, &[authored_quest(0x1000, "A Testable Errand")]);
        world
            .resource_mut::<QuestStageState>()
            .start_quest(quest, Some(10));
        world
            .resource_mut::<QuestObjectiveState>()
            .set_displayed(quest, 10, true);

        let lines = snapshot(&world).expect("a displayed objective must compose");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].quest, "A Testable Errand");
        assert_eq!(lines[0].text, "Do the thing in A Testable Errand");
    }

    #[test]
    fn finished_recordless_and_undisplayed_objectives_never_show() {
        let mut world = quest_world();
        let quest = QuestFormId(0x1000);
        install_authored_quests(&mut world, &[authored_quest(0x1000, "A Testable Errand")]);
        world
            .resource_mut::<QuestStageState>()
            .start_quest(quest, Some(10));
        {
            let mut objectives = world.resource_mut::<QuestObjectiveState>();
            objectives.set_displayed(quest, 10, true);
            objectives.set_completed(quest, 10, true);
            objectives.set_displayed(quest, 20, true);
            objectives.set_failed(quest, 20, true);
        }
        assert!(snapshot(&world).is_none(), "nothing displayable remains");

        // A fragment displaying an index the QUST never authored is skipped,
        // and an authored-but-undisplayed objective stays invisible.
        let mut objectives = world.resource_mut::<QuestObjectiveState>();
        objectives.set_displayed(quest, 30, true);
        objectives.set_displayed(quest, 20, false);
        drop(objectives);
        assert!(
            snapshot(&world).is_none(),
            "record-less and undisplayed objectives must not draw"
        );
    }

    #[test]
    fn stopped_quests_drop_their_lines_and_lines_are_ordered() {
        let mut world = quest_world();
        install_authored_quests(
            &mut world,
            &[
                authored_quest(0x3000, "Later Quest"),
                authored_quest(0x1000, "Earlier Quest"),
            ],
        );
        for raw in [0x3000u32, 0x1000] {
            let quest = QuestFormId(raw);
            world
                .resource_mut::<QuestStageState>()
                .start_quest(quest, None);
            world
                .resource_mut::<QuestObjectiveState>()
                .set_displayed(quest, 10, true);
        }
        world
            .resource_mut::<QuestStageState>()
            .stop(QuestFormId(0x1000));

        let lines = snapshot(&world).expect("the still-running quest must show");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].quest, "Later Quest");

        // Restarting restores both, ordered by FormID, not insertion order.
        world
            .resource_mut::<QuestStageState>()
            .start_quest(QuestFormId(0x1000), None);
        let lines = snapshot(&world).expect("both quests running again");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].quest, "Earlier Quest", "0x1000 sorts before 0x3000");
    }

    #[test]
    fn no_quest_state_or_no_lines_is_none() {
        let world = World::new();
        assert!(snapshot(&world).is_none(), "no quest runtime at all");

        let mut world = quest_world();
        assert!(
            snapshot(&world).is_none(),
            "registered but untouched quest state draws nothing"
        );
    }
}
