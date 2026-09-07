//! Starfield `BSShaderTextureSet` slot-occupancy census (#3900).
//!
//! `slot_role.rs` gives two different answers about which slot vocabulary
//! Starfield uses: `canonical_shader_type` groups it with FO76, `slot_to_role`
//! groups it with Skyrim on slots 2/3/6/7. Neither arm cites Starfield
//! evidence. Per the project's no-guessing rule the disagreement has to be
//! settled from the corpus, not by argument — this is that census.
//!
//! Counts, per raw slot index, how many non-empty texture bindings the
//! Starfield-layout (`bsver >= STARFIELD`) `BSShaderTextureSet` blocks carry,
//! and bins the file-name suffixes so a slot's *role* is readable from its
//! occupants (the FO76 slot-6 finding was "1,616 of 1,664 are `_s.dds`").
//!
//! Usage:
//!   cargo run --release -p byroredux-nif --example sf_slot_census -- <archive>...
//!
//! Reports per-layout so a mixed archive set stays interpretable.

use std::collections::BTreeMap;

use byroredux_nif::blocks::shader::BSShaderTextureSet;
use byroredux_nif::import::TextureSlotLayout;

fn layout_name(l: TextureSlotLayout) -> &'static str {
    match l {
        TextureSlotLayout::Skyrim => "skyrim",
        TextureSlotLayout::Fallout4 => "fo4",
        TextureSlotLayout::Fallout76 => "fo76",
        TextureSlotLayout::Starfield => "starfield",
    }
}

/// `textures/foo/bar_s.dds` -> `_s.dds`. Anything without a recognisable
/// `_x` tail is bucketed by extension alone so nothing is silently dropped.
fn suffix_of(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    let stem = lower.rsplit(['\\', '/']).next().unwrap_or(&lower);
    match stem.rsplit_once('.') {
        Some((name, ext)) => match name.rsplit_once('_') {
            Some((_, tail)) if tail.len() <= 4 => format!("_{tail}.{ext}"),
            _ => format!("*.{ext}"),
        },
        None => "<no-ext>".to_string(),
    }
}

fn main() {
    let archives: Vec<String> = std::env::args().skip(1).collect();
    assert!(!archives.is_empty(), "usage: sf_slot_census <archive>...");

    // layout -> slot -> suffix -> count
    let mut hist: BTreeMap<&'static str, BTreeMap<u32, BTreeMap<String, usize>>> = BTreeMap::new();
    // layout -> texture-set block count (the denominator for occupancy)
    let mut sets: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut files = 0usize;
    let mut parsed = 0usize;

    for path in &archives {
        let entries: Vec<(String, Vec<u8>)> = if path.to_ascii_lowercase().ends_with(".ba2") {
            let a = byroredux_bsa::Ba2Archive::open(path).expect("open ba2");
            a.list_files()
                .iter()
                .filter(|f| f.to_ascii_lowercase().ends_with(".nif"))
                .map(|f| (f.to_string(), a.extract(f).unwrap_or_default()))
                .collect()
        } else {
            let a = byroredux_bsa::BsaArchive::open(path).expect("open bsa");
            a.list_files()
                .iter()
                .filter(|f| f.to_ascii_lowercase().ends_with(".nif"))
                .map(|f| (f.to_string(), a.extract(f).unwrap_or_default()))
                .collect()
        };
        for (_name, bytes) in entries {
            if bytes.is_empty() {
                continue;
            }
            files += 1;
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            parsed += 1;
            // The layout the importer would pick for this file.
            let layout = layout_name(TextureSlotLayout::from_bsver(scene.bsver));
            for block in scene.blocks.iter() {
                let Some(tex_set) = block.as_any().downcast_ref::<BSShaderTextureSet>() else {
                    continue;
                };
                *sets.entry(layout).or_insert(0) += 1;
                for (slot, raw) in tex_set.textures.iter().enumerate() {
                    if raw.is_empty() {
                        continue;
                    }
                    *hist
                        .entry(layout)
                        .or_default()
                        .entry(slot as u32)
                        .or_default()
                        .entry(suffix_of(raw))
                        .or_insert(0) += 1;
                }
            }
        }
    }

    println!("# {files} NIFs seen, {parsed} parsed");
    for (layout, count) in &sets {
        println!("\n## {layout}: {count} BSShaderTextureSet blocks");
        let Some(slots) = hist.get(layout) else {
            println!("  (no non-empty bindings)");
            continue;
        };
        for slot in 0..8u32 {
            match slots.get(&slot) {
                None => println!("  slot {slot}: 0"),
                Some(suffixes) => {
                    let total: usize = suffixes.values().sum();
                    let mut top: Vec<(&String, &usize)> = suffixes.iter().collect();
                    top.sort_by(|a, b| b.1.cmp(a.1));
                    let shown: Vec<String> = top
                        .iter()
                        .take(4)
                        .map(|(s, n)| format!("{s}×{n}"))
                        .collect();
                    println!("  slot {slot}: {total}  [{}]", shown.join(", "));
                }
            }
        }
    }
}
