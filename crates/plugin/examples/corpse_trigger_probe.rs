//! #5223 probe — measure FO3's dismember-trigger corpse idiom from the
//! shipped master: which placed triggers carry a base script of the
//! `GenericBiped*DismembermentSCRIPT` family, which placements their XLKR
//! links target, and which actor bases kill themselves from their own SCRI
//! on load. Usage: corpse_trigger_probe <Fallout3.esm> [<FalloutNV.esm>]

use byroredux_plugin::esm::parse_esm;
use std::collections::HashSet;

const DISMEMBER_FAMILY: &str = "DismembermentSCRIPT";
const SELF_KILL_SCRIPTS: [&str; 2] = ["GenericKillSCRIPT", "OnLoadKillSelf"];

fn script_name(index: &byroredux_plugin::esm::records::EsmIndex, scri: Option<u32>) -> String {
    scri.and_then(|id| index.scripts.get(&id))
        .map(|s| s.editor_id.clone())
        .unwrap_or_default()
}

fn base_script(
    index: &byroredux_plugin::esm::records::EsmIndex,
    form_id: u32,
) -> Option<u32> {
    if let Some(acti) = index.activators.get(&form_id) {
        return Some(acti.script_form_id);
    }
    // Any other base kind the trigger family sits on — walk the same maps
    // `base_record_script_form_id` documents.
    if let Some(npc) = index.npcs.get(&form_id) {
        return Some(npc.script_form_id);
    }
    None
}

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let index = parse_esm(&bytes)?;
        println!("== {path}");

        // Linked kills: a placed trigger whose base script is the
        // dismemberment family, targeting a positive-health base.
        let mut linked_targets: HashSet<u32> = HashSet::new();
        let mut family_seen: HashSet<String> = HashSet::new();
        let all_cells = index
            .cells
            .cells
            .values()
            .chain(index.cells.exterior_cells.values().flat_map(|tile| tile.values()))
            .chain(index.cells.worldspace_persistent_cells.values());
        let mut linked_placements = 0usize;
        for cell in all_cells {
            for placed in &cell.references {
                if !placed.linked_refs.is_empty() {
                    linked_placements += 1;
                }
                if placed.linked_refs.is_empty() {
                    continue;
                }
                let Some(base) = base_script(&index, placed.base_form_id) else {
                    continue;
                };
                let name = script_name(&index, Some(base));
                if !name.contains(DISMEMBER_FAMILY) {
                    continue;
                }
                family_seen.insert(name);
                for link in &placed.linked_refs {
                    linked_targets.insert(link.target);
                }
            }
        }
        let linked_positive = linked_targets
            .iter()
            .filter(|t| index.npcs.contains_key(t))
            .count();
        println!(
            "dismember-family scripts seen: {:?}",
            {
                let mut v: Vec<_> = family_seen.iter().cloned().collect();
                v.sort();
                v
            }
        );
        println!(
            "placements with XLKR: {linked_placements}; linked targets resolved to              npc/crea records: {linked_positive} (raw set {})",
            linked_targets.len()
        );
        for cell in index
            .cells
            .cells
            .values()
            .chain(index.cells.exterior_cells.values().flat_map(|tile| tile.values()))
        {
            for placed in &cell.references {
                if placed.form_id == 0x000A_B3EB {
                    let base = base_script(&index, placed.base_form_id);
                    let script = base.map(|b| script_name(&index, Some(b)));
                    println!(
                        "audit example: {:08X} base={:08X} script={script:?} links={}",
                        placed.form_id,
                        placed.base_form_id,
                        placed.linked_refs.len(),
                    );
                }
            }
        }

        // Full census — per cell: linked targets that resolve to an actor
        // placement whose base has positive health (the audit's 49), and
        // actor placements whose base SCRI self-kills (the audit's 3).
        let mut linked_positive_health = 0usize;
        let mut self_killed_placements = 0usize;
        let mut target_examples: Vec<String> = Vec::new();
        {
            let all_cells = index
                .cells
                .cells
                .values()
                .chain(index.cells.exterior_cells.values().flat_map(|t| t.values()))
                .chain(index.cells.worldspace_persistent_cells.values());
            for cell in all_cells {
                let mut killed: HashSet<u32> = HashSet::new();
                for placed in &cell.references {
                    if placed.linked_refs.is_empty() {
                        continue;
                    }
                    if let Some(base) = base_script(&index, placed.base_form_id) {
                        let name = script_name(&index, Some(base));
                        if name.contains(DISMEMBER_FAMILY) {
                            for link in &placed.linked_refs {
                                killed.insert(link.target);
                            }
                        }
                    }
                }
                for placed in &cell.references {
                    if killed.contains(&placed.form_id) {
                        let leveled = index.leveled_npcs.contains_key(&placed.base_form_id);
                        if leveled {
                            linked_positive_health += 1;
                            println!(
                                "LVLN-TARGET: {:08X} in cell {} ({:08X}) base {:08X}",
                                placed.form_id, cell.editor_id, cell.form_id, placed.base_form_id
                            );
                        }
                        if index.npcs.contains_key(&placed.base_form_id)
                            || leveled
                        {
                            println!(
                                "TARGET: {:08X} in cell {} ({:08X})",
                                placed.form_id, cell.editor_id, cell.form_id
                            );
                            if target_examples.len() < 8 {
                                let base_kind = index
                                    .npcs
                                    .get(&placed.base_form_id)
                                    .map(|npc| {
                                        format!(
                                            "{} health {:?}",
                                            npc.editor_id, npc.data_base_health
                                        )
                                    })
                                    .or_else(|| {
                                        index.leveled_npcs.get(&placed.base_form_id).map(
                                            |_| "LVLN (leveled)".to_owned(),
                                        )
                                    })
                                    .unwrap_or_else(|| "unresolved".to_owned());
                                let cell_name = cell.editor_id.clone();
                                target_examples.push(format!(
                                    "{:08X} in {cell_name} — base {:08X} {base_kind}",
                                    placed.form_id, placed.base_form_id,
                                ));
                            }
                        }
                    }
                    if let Some(npc) = index.npcs.get(&placed.base_form_id) {
                        let name = script_name(&index, Some(npc.script_form_id));
                        let self_kill = SELF_KILL_SCRIPTS.iter().any(|s| name == *s)
                            || name.contains(DISMEMBER_FAMILY);
                        if self_kill {
                            self_killed_placements += 1;
                        }
                    }
                }
            }
        }
        println!("linked positive-health targets: {linked_positive_health}");
        for e in &target_examples {
            println!("  e.g. {e}");
        }
        println!("self-killed actor placements: {self_killed_placements}");

        // Self-kills: actor bases whose own SCRI is a kill-on-load script.
        for npc in index.npcs.values() {
            let name = script_name(&index, Some(npc.script_form_id));
            if SELF_KILL_SCRIPTS.iter().any(|s| name == *s) {
                println!(
                    "self-kill base: {} {:08X} health={:?} script={name}",
                    npc.editor_id, npc.form_id, npc.data_base_health
                );
            }
        }
        // And the FFEU04NPC1 case — whatever its script is.
        if let Some(npc) = index.npcs.get(&0x00034102) {
            println!(
                "base 00034102: {} health={:?} script={:?}",
                npc.editor_id,
                npc.data_base_health,
                script_name(&index, Some(npc.script_form_id)),
            );
        }
        for npc in index.npcs.values() {
            if npc.editor_id.to_lowercase().contains("theo") {
                println!(
                    "theo-match: {} {:08X} script={:?}",
                    npc.editor_id,
                    npc.form_id,
                    script_name(&index, Some(npc.script_form_id)),
                );
            }
        }
        for npc in index.npcs.values() {
            if npc.editor_id.eq_ignore_ascii_case("rrtheo") {
                println!(
                    "rrtheo: {:08X} health={:?} script={:?}",
                    npc.form_id,
                    npc.data_base_health,
                    script_name(&index, Some(npc.script_form_id)),
                );
            }
        }
        for npc in index.npcs.values() {
            if npc.editor_id.eq_ignore_ascii_case("FFEU04NPC1")
                || npc.editor_id.eq_ignore_ascii_case("FFEU255NPC1")
                || npc.editor_id.eq_ignore_ascii_case("FFEU07Corpse")
            {
                println!(
                    "audit-named base: {} {:08X} health={:?} script={:?}",
                    npc.editor_id,
                    npc.form_id,
                    npc.data_base_health,
                    script_name(&index, Some(npc.script_form_id)),
                );
            }
        }
    }
    Ok(())
}
