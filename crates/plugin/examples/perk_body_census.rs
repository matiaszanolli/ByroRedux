//! Census the parts of a `PERK` record the level-up and acquisition paths read
//! (docs/prd/perk-entry-points.md Phase 0 verify items, PERK-BODY / PERK-GATE):
//!
//! * `Ability` entries — which ranks a perk authors them at, and whether a
//!   perk carries them at more than one rank (the rank rule, PERK-BODY-001);
//! * `Quest` entries — how many there are and at which ranks (PERK-BODY-003);
//! * `NNAM` "Next Perk" — how it coexists with `num_ranks` above 1
//!   (PERK-GATE-004);
//! * the `DATA` level and playable bytes (PERK-GATE-002 / -003).
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example perk_body_census -- <ESM> [...]

use std::collections::{BTreeMap, BTreeSet};

use byroredux_plugin::esm::parse_esm;
use byroredux_plugin::esm::records::PerkEntryBody;

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let index = parse_esm(&bytes)?;
        println!("=== {path} ({:?}): {} PERK", index.game, index.perks.len());

        let mut perks: Vec<_> = index.perks.values().collect();
        perks.sort_by_key(|perk| perk.form_id);

        // Ability entries.
        let mut ability_perks = 0usize;
        let mut ability_entries = 0usize;
        let mut ability_multi_rank = Vec::new();
        let mut ability_rank_hist: BTreeMap<u8, usize> = BTreeMap::new();
        // Quest entries.
        let mut quest_perks = 0usize;
        let mut quest_multi_rank = Vec::new();
        let mut quest_rank_hist: BTreeMap<u8, usize> = BTreeMap::new();
        // Gate bytes.
        let mut level_hist: BTreeMap<u8, usize> = BTreeMap::new();
        let mut playable = 0usize;
        let mut playable_level_nonzero = 0usize;
        // NNAM.
        let mut next_perk = 0usize;
        let mut next_with_multi_rank = Vec::new();
        let mut next_dangling = 0usize;
        let mut next_targets: BTreeSet<u32> = BTreeSet::new();

        for perk in &perks {
            *level_hist.entry(perk.level).or_default() += 1;
            if perk.playable {
                playable += 1;
                if perk.level != 0 {
                    playable_level_nonzero += 1;
                }
            }
            let mut ability_ranks: BTreeSet<u8> = BTreeSet::new();
            let mut quest_ranks: BTreeSet<u8> = BTreeSet::new();
            for entry in &perk.entries {
                match &entry.body {
                    PerkEntryBody::Ability { .. } => {
                        ability_entries += 1;
                        ability_ranks.insert(entry.rank);
                        *ability_rank_hist.entry(entry.rank).or_default() += 1;
                    }
                    PerkEntryBody::Quest { .. } => {
                        quest_ranks.insert(entry.rank);
                        *quest_rank_hist.entry(entry.rank).or_default() += 1;
                    }
                    PerkEntryBody::EntryPoint { .. } => {}
                }
            }
            if !ability_ranks.is_empty() {
                ability_perks += 1;
                if ability_ranks.len() > 1 {
                    ability_multi_rank.push((perk.editor_id.clone(), perk.form_id, perk.num_ranks, ability_ranks));
                }
            }
            if !quest_ranks.is_empty() {
                quest_perks += 1;
                if quest_ranks.len() > 1 {
                    quest_multi_rank.push((perk.editor_id.clone(), perk.form_id, perk.num_ranks, quest_ranks));
                }
            }
            if perk.next_perk != 0 {
                next_perk += 1;
                next_targets.insert(perk.next_perk);
                if perk.num_ranks > 1 {
                    next_with_multi_rank.push((perk.editor_id.clone(), perk.form_id, perk.num_ranks));
                }
                if !index.perks.contains_key(&perk.next_perk) {
                    next_dangling += 1;
                }
            }
        }

        println!("-- Ability: {ability_perks} perks, {ability_entries} entries; entries per rank {ability_rank_hist:?}");
        println!("   perks with Ability entries at more than one rank: {}", ability_multi_rank.len());
        for (editor, form, ranks, at) in ability_multi_rank.iter().take(12) {
            println!("     {editor} ({form:08X}) num_ranks {ranks}: ability ranks {at:?}");
        }
        let ability_multi_num_ranks = perks
            .iter()
            .filter(|perk| perk.num_ranks > 1 && perk.entries.iter().any(|e| matches!(e.body, PerkEntryBody::Ability { .. })))
            .count();
        println!("   Ability perks with num_ranks > 1: {ability_multi_num_ranks}");
        if std::env::var_os("CENSUS_QUEST_LIST").is_some() {
            for perk in perks.iter().filter(|perk| perk.entries.iter().any(|e| matches!(e.body, PerkEntryBody::Quest { .. }))) {
                for e in &perk.entries {
                    if let PerkEntryBody::Quest { quest_form_id, stage } = &e.body {
                        let stages = index.quests.get(quest_form_id).map(|q| q.stages.iter().map(|s| s.index).collect::<Vec<_>>());
                        let verdict = match &stages {
                            None => "quest not in file".to_string(),
                            Some(list) if list.contains(stage) => "stage exists".to_string(),
                            Some(list) if list.contains(&(stage & 0xFF)) => format!("only the low byte {} exists", stage & 0xFF),
                            Some(_) => "neither exists".to_string(),
                        };
                        println!("     QUEST {} ({:08X}) num_ranks {} rank {} quest {:08X} stage {} tabs {} -> {}", perk.editor_id, perk.form_id, perk.num_ranks, e.rank, quest_form_id, stage, e.condition_tabs.len(), verdict);
                    }
                }
            }
        }
        println!("-- Quest: {quest_perks} perks; entries per rank {quest_rank_hist:?}");
        println!("   perks with Quest entries at more than one rank: {}", quest_multi_rank.len());
        for (editor, form, ranks, at) in quest_multi_rank.iter().take(12) {
            println!("     {editor} ({form:08X}) num_ranks {ranks}: quest ranks {at:?}");
        }
        println!("-- DATA level histogram: {level_hist:?}");
        println!("   playable perks: {playable}, of which level != 0: {playable_level_nonzero}");
        println!(
            "-- NNAM: {next_perk} perks author a next perk ({} distinct targets, {next_dangling} not in this file)",
            next_targets.len()
        );
        println!("   NNAM perks with num_ranks > 1: {}", next_with_multi_rank.len());
        for (editor, form, ranks) in next_with_multi_rank.iter().take(12) {
            println!("     {editor} ({form:08X}) num_ranks {ranks}");
        }
    }
    Ok(())
}
