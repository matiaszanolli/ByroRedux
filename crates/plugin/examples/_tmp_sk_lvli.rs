//! TEMP scratch (audit 2026-08-16): Skyrim LVLI LVLF flag distribution over the
//! outfit (OTFT) reachable set, to test `expand_leveled_form_id`'s
//! `flags & 0x02 => multi-pick` rule against TES5's real "Use All" bit (0x04).
use byroredux_plugin::esm::parse_esm;
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    let path = std::env::args().nth(1).expect("esm path");
    let bytes = std::fs::read(&path).expect("read");
    let index = parse_esm(&bytes).expect("parse");
    println!("LVLI total: {}", index.leveled_items.len());
    println!("OTFT total: {}", index.outfits.len());

    let mut all_flags: BTreeMap<u8, usize> = BTreeMap::new();
    for l in index.leveled_items.values() {
        *all_flags.entry(l.flags).or_default() += 1;
    }
    println!("\nAll LVLI LVLF flag byte histogram:");
    for (f, c) in &all_flags {
        println!("  0x{f:02X}  {c}   (0x01 all-levels, 0x02 each-item, 0x04 USE-ALL, 0x08 special)");
    }

    // Reachable-from-outfit closure.
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut stack: Vec<u32> = Vec::new();
    for o in index.outfits.values() {
        stack.extend(o.items.iter().copied());
    }
    let mut reach_flags: BTreeMap<u8, usize> = BTreeMap::new();
    let mut use_all_samples: Vec<String> = Vec::new();
    while let Some(fid) = stack.pop() {
        if !seen.insert(fid) {
            continue;
        }
        if let Some(l) = index.leveled_items.get(&fid) {
            *reach_flags.entry(l.flags).or_default() += 1;
            if l.flags & 0x04 != 0 && use_all_samples.len() < 12 {
                use_all_samples.push(format!(
                    "{:08X} flags=0x{:02X} entries={}",
                    fid,
                    l.flags,
                    l.entries.len()
                ));
            }
            for e in &l.entries {
                stack.push(e.form_id);
            }
        }
    }
    println!("\nLVLI reachable from an OTFT: {}", reach_flags.values().sum::<usize>());
    for (f, c) in &reach_flags {
        println!("  0x{f:02X}  {c}");
    }
    println!("\nUSE-ALL (0x04) LVLIs reachable from outfits:");
    for s in &use_all_samples {
        println!("  {s}");
    }
    // Which outfits reference a USE-ALL LVLI directly, and what is inside.
    println!("\nOutfits directly referencing a USE-ALL LVLI (first 8):");
    let mut shown = 0;
    for (ofid, o) in index.outfits.iter() {
        for it in &o.items {
            if let Some(l) = index.leveled_items.get(it) {
                if l.flags & 0x04 != 0 && shown < 8 {
                    shown += 1;
                    println!("  OTFT {:08X} -> LVLI {:08X} flags=0x{:02X}", ofid, it, l.flags);
                    for e in &l.entries {
                        let name = index
                            .items
                            .get(&e.form_id)
                            .map(|i| i.common.editor_id.clone())
                            .unwrap_or_else(|| "<unresolved>".into());
                        println!("      lvl={:>3} {:08X} {}", e.level, e.form_id, name);
                    }
                }
            }
        }
    }

    // How many OTFTs contain at least one USE-ALL LVLI, and how many NPC_
    // records point at such an outfit.
    let mut bad_outfits: BTreeSet<u32> = BTreeSet::new();
    for (ofid, o) in index.outfits.iter() {
        for it in &o.items {
            if let Some(l) = index.leveled_items.get(it) {
                if l.flags & 0x04 != 0 {
                    bad_outfits.insert(*ofid);
                }
            }
        }
    }
    let npcs_hit = index
        .npcs
        .values()
        .filter(|n| n.default_outfit.is_some_and(|f| bad_outfits.contains(&f)))
        .count();
    println!("\nOTFTs containing >=1 USE-ALL LVLI: {} / {}", bad_outfits.len(), index.outfits.len());
    println!("NPC_ records whose default outfit is one of those: {} / {}", npcs_hit, index.npcs.len());
}
