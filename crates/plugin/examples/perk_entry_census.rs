//! Census every parsed `PERK` entry-point entry in a shipped master: which
//! entry points / math functions / parameter types occur, how the decoded
//! `EPFD` shapes line up with them, and how the condition tabs fall.
//!
//! Evidence harness for the perk entry-point composer — the layout (DATA =
//! `[entry point, function, tab count]`, `EPFT` = parameter type, `EPFD`
//! selected by `EPFT` + function) comes from xEdit's definitions and is
//! cross-checked here against the vanilla data: a `Float` parameter type must
//! decode a `Float`, a `Range` only under function 4, and so on. Any mismatch
//! is printed.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example perk_entry_census -- <ESM> [...]
//!
//! `CENSUS_ENTRY_POINT=<n>` also lists every entry on hook `n` with its
//! per-tab condition functions.

use std::collections::BTreeMap;

use byroredux_plugin::esm::records::{PerkEntryBody, PerkFunctionData};
use byroredux_plugin::esm::parse_esm;
use byroredux_plugin::esm::reader::GameKind;

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let index = parse_esm(&bytes)?;
        println!("=== {path} ({:?}): {} PERK", index.game, index.perks.len());
        let mut by_entry_point: BTreeMap<u8, usize> = BTreeMap::new();
        let mut by_function: BTreeMap<u8, usize> = BTreeMap::new();
        let mut by_param: BTreeMap<(u8, &'static str), usize> = BTreeMap::new();
        let mut tabs: BTreeMap<(u8, Vec<i8>), usize> = BTreeMap::new();
        let mut rank_priority: BTreeMap<(u8, u8), usize> = BTreeMap::new();
        let mut mismatches = Vec::new();
        let (mut quest, mut ability, mut entry_points) = (0usize, 0usize, 0usize);
        let mut undecoded = 0usize;
        for perk in index.perks.values() {
            for entry in &perk.entries {
                *rank_priority.entry((entry.rank, entry.priority)).or_default() += 1;
                match &entry.body {
                    PerkEntryBody::Quest { .. } => quest += 1,
                    PerkEntryBody::Ability { .. } => ability += 1,
                    PerkEntryBody::EntryPoint {
                        entry_point_index,
                        function,
                        tab_count,
                        param_type,
                        function_data,
                        ..
                    } => {
                        entry_points += 1;
                        *by_entry_point.entry(*entry_point_index).or_default() += 1;
                        *by_function.entry(*function).or_default() += 1;
                        let shape = match function_data {
                            PerkFunctionData::None => "None",
                            PerkFunctionData::Float(_) => "Float",
                            PerkFunctionData::Range { .. } => "Range",
                            PerkFunctionData::ActorValueMult { .. } => "ActorValueMult",
                            PerkFunctionData::FormId(_) => "FormId",
                            PerkFunctionData::LString(_) => "LString",
                            PerkFunctionData::AvifMult { .. } => "AvifMult",
                        };
                        *by_param.entry((*param_type, shape)).or_default() += 1;
                        let tab_ids: Vec<i8> =
                            entry.condition_tabs.iter().map(|t| t.tab).collect();
                        if tab_ids.iter().any(|t| *t < 0 || *t as u8 >= *tab_count) {
                            mismatches.push(format!(
                                "{} ({:08X}): tab ids {tab_ids:?} outside tab_count {tab_count}",
                                perk.editor_id, perk.form_id
                            ));
                        }
                        *tabs.entry((*entry_point_index, tab_ids)).or_default() += 1;
                        // The decoded shape must be what the parameter type
                        // promises.
                        // Starfield's `EPFD` shapes and `PRRK` rank blocks are not decoded (PERK-DEC-010): an entry
                        // whose parameter type promises data but decoded to nothing is counted apart, not as a mismatch.
                        if index.game == GameKind::Starfield
                            && matches!(function_data, PerkFunctionData::None)
                            && !matches!(*param_type, 0 | 4 | 6)
                        {
                            undecoded += 1;
                            continue;
                        }
                        let ok = match (index.game, *param_type, function_data) {
                            (_, 1, PerkFunctionData::Float(_)) => true,
                            (_, 2, PerkFunctionData::Range { .. }) => matches!(function, 4),
                            (_, 2, PerkFunctionData::ActorValueMult { .. }) => {
                                matches!(function, 5 | 12 | 13 | 14)
                            }
                            (GameKind::Fallout4, 2 | 8, PerkFunctionData::AvifMult { .. }) => {
                                *param_type == 8 || matches!(function, 5 | 12 | 13 | 14)
                            }
                            (_, 3, PerkFunctionData::FormId(_)) => true,
                            (GameKind::Skyrim | GameKind::Fallout4, 4 | 5, PerkFunctionData::FormId(_)) => true,
                            (GameKind::Skyrim | GameKind::Fallout4, 7, PerkFunctionData::LString(_)) => true,
                            (_, 0 | 4 | 6, PerkFunctionData::None) => true,
                            _ => false,
                        };
                        if !ok {
                            mismatches.push(format!(
                                "{} ({:08X}): entry {entry_point_index} fn {function} \
                                 EPFT {param_type} decoded {function_data:?}",
                                perk.editor_id, perk.form_id
                            ));
                        }
                    }
                }
            }
        }
        // `CENSUS_ENTRY_POINT=<n>`: every entry on that hook, with its tabs'
        // condition functions — which subjects a calculation must supply.
        if let Some(wanted) = std::env::var("CENSUS_ENTRY_POINT")
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
        {
            for perk in index.perks.values() {
                for e in &perk.entries {
                    let PerkEntryBody::EntryPoint {
                        entry_point_index,
                        function,
                        tab_count,
                        function_data,
                        ..
                    } = &e.body
                    else {
                        continue;
                    };
                    if *entry_point_index != wanted {
                        continue;
                    }
                    let tabs: Vec<String> = e
                        .condition_tabs
                        .iter()
                        .map(|t| {
                            let functions: Vec<String> = t
                                .conditions
                                .iter()
                                .map(|c| format!("{}({:08X})", c.function_index, c.param_1))
                                .collect();
                            format!("tab {}: {}", t.tab, functions.join(" "))
                        })
                        .collect();
                    println!(
                        "  ep {wanted} {} ({:08X}) rank {} fn {function} {function_data:?} tabs {tab_count} [{}]",
                        perk.editor_id,
                        perk.form_id,
                        e.rank,
                        tabs.join("; ")
                    );
                }
            }
        }
        // Multi-rank evidence: perks carrying entries above rank 0.
        let mut multi: Vec<_> = index
            .perks
            .values()
            .filter(|p| p.entries.iter().any(|e| e.rank > 0))
            .collect();
        multi.sort_by_key(|p| p.form_id);
        for perk in multi.into_iter().take(6) {
            println!("  multi-rank {} ({:08X}) num_ranks {}:", perk.editor_id, perk.form_id, perk.num_ranks);
            for e in &perk.entries {
                println!("    rank {} prio {} {:?}", e.rank, e.priority, e.body);
            }
        }
        for perk in index.perks.values() {
            for e in &perk.entries {
                if e.priority != 0 {
                    if let PerkEntryBody::EntryPoint { entry_point_index, function, function_data, .. } = &e.body {
                        println!("  prio {:>3} {} ep {} fn {} {:?}", e.priority, perk.editor_id, entry_point_index, function, function_data);
                    }
                }
            }
        }
        // Order-sensitivity: entry points where non-commuting math functions
        // (Set/Absolute vs the rest, Add vs Multiply) meet at DIFFERENT
        // priorities — the only place a priority-order choice is observable.
        let mut per_ep: BTreeMap<u8, Vec<(u8, u8, String)>> = BTreeMap::new();
        for perk in index.perks.values() {
            for e in &perk.entries {
                if let PerkEntryBody::EntryPoint { entry_point_index, function, .. } = &e.body {
                    per_ep.entry(*entry_point_index).or_default().push((e.priority, *function, perk.editor_id.clone()));
                }
            }
        }
        for (ep, list) in &per_ep {
            let funcs: std::collections::BTreeSet<u8> = list.iter().map(|x| x.1).collect();
            let prios: std::collections::BTreeSet<u8> = list.iter().map(|x| x.0).collect();
            let numeric: Vec<u8> = funcs.iter().copied().filter(|f| matches!(f, 1 | 2 | 3 | 6 | 7 | 12 | 13)).collect();
            let noncommuting = numeric.contains(&1) && numeric.len() > 1
                || (numeric.contains(&2) && numeric.contains(&3))
                || numeric.contains(&6) || numeric.contains(&7);
            if noncommuting && prios.len() > 1 {
                println!("  ORDER-SENSITIVE ep {ep}: funcs {funcs:?} prios {prios:?}");
                let mut rows = list.clone();
                rows.sort();
                for (prio, function, name) in rows.iter().filter(|r| r.1 != 14 || r.0 != 0) {
                    println!("      prio {prio:>3} fn {function:>2} {name}");
                }
            }
        }
        for perk in index.perks.values() {
            for e in &perk.entries {
                if let PerkEntryBody::EntryPoint { entry_point_index, function: 1, function_data, .. } = &e.body {
                    println!("  FN1 ep {entry_point_index} {} rank {} {:?}", perk.editor_id, e.rank, function_data);
                }
            }
        }
        println!("entries: {quest} quest, {ability} ability, {entry_points} entry-point");
        println!("entry points: {by_entry_point:?}");
        println!("functions: {by_function:?}");
        println!("(EPFT, decoded shape): {by_param:?}");
        println!("(rank, priority): {rank_priority:?}");
        println!("condition tabs per entry point:");
        for ((entry_point, tab_ids), n) in &tabs {
            println!("  entry {entry_point:>3} tabs {tab_ids:?}: {n}");
        }
        println!("mismatches: {}", mismatches.len());
        if index.game == GameKind::Starfield {
            println!("undecoded (Starfield EPFD, not a mismatch): {undecoded}");
        }
        // PERK-DEC-011's Phase 0 verify: every fragment's index equals one activate entry's `EPF3` index (Skyrim) or
        // `EPFB` (Fallout 4 / Starfield).
        let (mut fragments, mut matched) = (0usize, 0usize);
        for perk in index.perks.values() {
            for fragment in &perk.script_fragments {
                fragments += 1;
                let hit = perk.entries.iter().any(|e| match &e.body {
                    PerkEntryBody::EntryPoint { details, .. } => {
                        details.fragment_index.map(u32::from) == Some(fragment.index)
                            || details.entry_id.map(u32::from) == Some(fragment.index)
                    }
                    _ => false,
                });
                matched += usize::from(hit);
            }
        }
        println!("fragments: {fragments}, matching an entry's index: {matched}");
        for line in mismatches.iter().take(20) {
            println!("  {line}");
        }
    }
    Ok(())
}
