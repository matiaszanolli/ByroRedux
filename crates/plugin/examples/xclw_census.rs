//! WATAL W2 census — per-worldspace XCLW tri-state distribution for
//! exterior cells, cross-tabbed against LAND min height. Answers, from
//! shipped bytes: how many distinct water heights the distant-water LOD
//! would need if it honored per-cell overrides, and how many "wet at
//! distance" cells each height claims versus the single worldspace-default
//! sheet the current annulus draws.
//!
//! Scratch probe for the W2 LOD-coverage investigation — not a gate.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example xclw_census -- <ESM> [WORLD_SUBSTR]

use byroredux_plugin::esm;
use std::collections::BTreeMap;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(esm_path), world_filter) = (args.first(), args.get(1).map(String::as_str)) else {
        anyhow::bail!("usage: xclw_census ESM [WORLD_SUBSTR]");
    };
    let bytes = std::fs::read(esm_path)?;
    let index = esm::records::parse_esm(&bytes)?;

    let mut keys: Vec<_> = index.cells.exterior_cells.keys().cloned().collect();
    keys.sort();
    for wkey in keys {
        if let Some(f) = world_filter {
            if !wkey.to_ascii_lowercase().contains(&f.to_ascii_lowercase()) {
                continue;
            }
        }
        let cells = &index.cells.exterior_cells[&wkey];
        let wrld = index.cells.worldspaces.get(&wkey);
        // Same resolution the LOD spawn uses: NAM3/NAM4 first, else the
        // NAM2-carried default, else 0.0 when only the form is authored.
        let default_height = wrld
            .and_then(|w| w.lod_water_height.or(w.default_water_height))
            .or_else(|| wrld.and_then(|w| w.water_form.map(|_| 0.0)));

        let mut absent = 0usize; // inherits the worldspace default
        let mut dry_sentinel = 0usize; // authored XCLW sentinel: stays dry
        let mut overrides: BTreeMap<i64, usize> = BTreeMap::new(); // height -> cells
        let mut land_missing = 0usize;
        // Effective-height buckets for cells whose LAND dips below their
        // effective water ("wet at distance" — what distant water would draw).
        let mut wet_default = 0usize;
        let mut wet_override: BTreeMap<i64, usize> = BTreeMap::new();

        for cell in cells.values() {
            let effective = if cell.water_height_is_explicit {
                match cell.water_height {
                    Some(h) => {
                        *overrides.entry(h.round() as i64).or_default() += 1;
                        Some(h)
                    }
                    None => {
                        dry_sentinel += 1;
                        None
                    }
                }
            } else {
                absent += 1;
                default_height
            };
            let Some(eff) = effective else { continue };
            let Some(land) = cell.landscape.as_ref() else {
                land_missing += 1;
                continue;
            };
            let min = land.heights.iter().copied().fold(f32::INFINITY, f32::min);
            if min <= eff {
                if cell.water_height_is_explicit && cell.water_height.is_some() {
                    *wet_override.entry(eff.round() as i64).or_default() += 1;
                } else {
                    wet_default += 1;
                }
            }
        }
        let total = absent + dry_sentinel + overrides.values().sum::<usize>();
        println!("== {wkey} ({total} exterior cells) ==");
        println!(
            "  worldspace default water: {}",
            match default_height {
                Some(h) => format!("{h}"),
                None => "none authored".into(),
            }
        );
        println!("  absent XCLW (inherit): {absent}");
        println!("  dry sentinel:          {dry_sentinel}");
        if overrides.is_empty() {
            println!("  overrides:             none");
        } else {
            println!("  overrides:");
            for (h, n) in &overrides {
                println!("    {h:>10} -> {n} cells{}", if wet_override.get(h).copied().unwrap_or(0) > 0 { format!(" ({} wet at distance)", wet_override[h]) } else { String::new() });
            }
        }
        println!("  wet-at-distance at default: {wet_default} cells");
        if land_missing > 0 {
            println!("  (cells without parsed LAND: {land_missing})");
        }
    }
    Ok(())
}
