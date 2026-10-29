//! Closing authored-workload inventory (informational).

use super::*;

/// Closing authored-workload inventory (informational).
pub(super) fn print_inventory(
    quest: &QustRecord,
    scenes: &SceneSet,
    inventory: &SceneInventory,
    corpus: &ScriptCorpus,
) {
    println!();
    println!("-- authored workload inventory --");
    println!("quest aliases:          {}", quest.aliases.len());
    println!("quest stage bindings:   {}", quest.fragments.len());
    println!("MQ101 scene records:    {}", scenes.scenes.len());
    println!("scene phases:           {}", inventory.total_scene_phases);
    println!("scene actions:          {}", inventory.total_scene_actions);
    println!(
        "package action stacks:  {:?} ({} end on phases with completion CTDAs)",
        inventory.package_action_stacks, inventory.package_actions_with_phase_completion_conditions
    );
    println!("package procedures:     {:?}", inventory.package_procedures);
    for (procedure, samples) in &inventory.package_procedure_samples {
        println!("  {procedure}: {}", samples.join(", "));
    }
    println!(
        "package templates:      {} unique",
        inventory.package_templates.len()
    );
    for (template, count) in &inventory.package_templates {
        println!("  {template}: {count}");
    }
    println!(
        "procedure-tree leaves:  {:?}",
        inventory.package_tree_procedures
    );
    for (procedure, samples) in &inventory.package_tree_procedure_samples {
        println!("  {procedure}: {}", samples.join(", "));
    }
    println!(
        "package execution plan: {:?}",
        inventory.package_execution_kinds
    );
    if !inventory.unresolved_movement_targets.is_empty() {
        println!(
            "movement targets awaiting richer resolution: {}: {}",
            inventory.unresolved_movement_targets.len(),
            preview(&inventory.unresolved_movement_targets, 12)
        );
    }
    println!(
        "scene event bindings:   {}",
        inventory.total_scene_fragments
    );
    println!(
        "scene CTDAs:             {} across {} functions ({} RunOn::QuestAlias)",
        inventory.scene_condition_functions.values().sum::<usize>(),
        inventory.scene_condition_functions.len(),
        inventory.quest_alias_conditions,
    );
    if !inventory.unsupported_scene_condition_functions.is_empty() {
        println!(
            "unsupported scene CTDA functions: {:?}",
            inventory.unsupported_scene_condition_functions
        );
        for (function, sites) in &inventory.unsupported_scene_condition_sites {
            println!("  {function}: {}", preview(sites, 8));
        }
    }
    if let Some(scene) = scenes.primary {
        println!("primary scene actor aliases:");
        for actor in &scene.actors {
            let alias = quest
                .aliases
                .iter()
                .find(|alias| alias.alias_id == actor.actor_id as i32);
            match alias {
                Some(alias) => println!(
                    "  {:>3} {:<28} {:?}",
                    actor.actor_id, alias.name, alias.fill_type
                ),
                None => println!("  {:>3} <missing>", actor.actor_id),
            }
        }
    }
    println!("MQ101 compiled scripts: {}", corpus.mq101_pex_count);
    println!("scene fragment scripts: {}", corpus.scene_count);
    println!("package fragment scripts: {}", corpus.package_count);
}
