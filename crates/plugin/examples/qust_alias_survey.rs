//! M47.3 Phase 0 empirical validation: parse a real ESM and report the
//! `ALST`/`ALLS` alias fill-type distribution + a few sanity spot-checks,
//! cross-validating the UESP/xEdit-sourced field table
//! (`docs/engine/m47-3-quest-alias-design.md`) against real bytes rather
//! than trusting the wiki table alone.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example qust_alias_survey -- <ESM>

use byroredux_plugin::esm::records::AliasFillType;
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args.next().expect("usage: qust_alias_survey <ESM>");
    let bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes).map_err(|e| anyhow::anyhow!(e))?;

    let mut quests_with_aliases = 0usize;
    let mut total_aliases = 0usize;
    let mut fill_type_hist: HashMap<&'static str, usize> = HashMap::new();
    let mut no_fill_no_cond = 0usize; // suspicious: neither a fill type nor Match Conditions
    let mut no_fill_no_cond_but_alfi = 0usize; // ... but has a Force Into Alias target
    let mut with_alfi = 0usize;
    let mut companion_without_primary = 0usize; // sanity check, should stay near zero
    let mut with_injected = 0usize;
    let mut examples: Vec<String> = Vec::new();

    for (&form_id, quest) in index.quests.iter() {
        if quest.aliases.is_empty() {
            continue;
        }
        quests_with_aliases += 1;
        for alias in &quest.aliases {
            total_aliases += 1;
            if alias.force_into_alias.is_some() {
                with_alfi += 1;
            }
            let kind = match &alias.fill_type {
                None if alias.match_conditions.is_empty() => {
                    no_fill_no_cond += 1;
                    if alias.force_into_alias.is_some() {
                        no_fill_no_cond_but_alfi += 1;
                    }
                    "NONE (no fill, no conditions)"
                }
                None => "FindMatching (conditions only)",
                Some(AliasFillType::ForcedReference(_)) => "ForcedReference",
                Some(AliasFillType::ForcedLocation(_)) => "ForcedLocation",
                Some(AliasFillType::UniqueActor(_)) => "UniqueActor",
                Some(AliasFillType::CreatedObject { .. }) => "CreatedObject",
                Some(AliasFillType::ExternalAlias { .. }) => "ExternalAlias",
                Some(AliasFillType::LocationAliasReference { .. }) => "LocationAliasReference",
                Some(AliasFillType::FromEvent { .. }) => "FromEvent",
            };
            *fill_type_hist.entry(kind).or_default() += 1;

            let has_injected = alias.injected.display_name.is_some()
                || alias.injected.voice_type.is_some()
                || alias.injected.combat_override.is_some()
                || !alias.injected.factions.is_empty()
                || !alias.injected.packages.is_empty()
                || !alias.injected.spells.is_empty()
                || !alias.injected.keywords.is_empty()
                || !alias.injected.inventory.is_empty();
            if has_injected {
                with_injected += 1;
            }

            if examples.len() < 20 {
                examples.push(format!(
                    "{form_id:08X} alias {} '{}' (loc={}) -> {kind} flags={:#06x}",
                    alias.alias_id, alias.name, alias.is_location, alias.flags.0
                ));
            }
        }
    }
    // Companion-without-primary is only detectable by re-deriving from
    // raw bytes; approximate here by checking the CreatedObject/
    // ExternalAlias/LocationAliasReference variants never carry an
    // all-zero companion pair that would indicate a dangling ALCA/ALCL/
    // ALEA/ALFA with no ALCO/ALEQ/ALRT before it (a weak signal, but a
    // real bug would show as a suspicious spike).
    for quest in index.quests.values() {
        for alias in &quest.aliases {
            if let Some(AliasFillType::CreatedObject { base: 0, .. }) = &alias.fill_type {
                companion_without_primary += 1;
            }
        }
    }

    println!("== M47.3 QUST alias survey ==");
    println!("ESM: {esm_path}");
    println!("quests with >=1 alias: {quests_with_aliases}");
    println!("total aliases: {total_aliases}");
    println!("aliases with injected data: {with_injected}");
    println!("suspicious (no fill type, no conditions): {no_fill_no_cond}");
    println!("  of which have a Force Into Alias (ALFI) target: {no_fill_no_cond_but_alfi}");
    println!("aliases with a Force Into Alias (ALFI) target (any fill type): {with_alfi}");
    println!("suspicious (CreatedObject with base==0): {companion_without_primary}");
    println!("\n-- fill-type distribution --");
    let mut hist: Vec<(&&str, &usize)> = fill_type_hist.iter().collect();
    hist.sort_by(|a, b| b.1.cmp(a.1));
    for (kind, count) in hist {
        let pct = 100.0 * *count as f64 / total_aliases.max(1) as f64;
        println!("  {count:>6} ({pct:4.1}%)  {kind}");
    }
    println!("\n-- first 20 aliases (spot-check) --");
    for e in &examples {
        println!("  {e}");
    }
    Ok(())
}
