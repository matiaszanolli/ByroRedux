//! Throwaway: find exterior cells whose EditorID contains a substring, in a
//! given worldspace, and print their grid + REFR count + LAND presence so the
//! densest "center" cell can be picked for a benchmark grid.
//!
//! Usage: cargo run -p byroredux-plugin --example find_ext_cell -- <ESM> <WORLD_SUBSTR> <CELL_SUBSTR>

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm = args.next().expect("usage: ESM WORLD_SUBSTR CELL_SUBSTR");
    let world_sub = args.next().unwrap_or_default().to_ascii_lowercase();
    let cell_sub = args.next().unwrap_or_default().to_ascii_lowercase();

    let bytes = std::fs::read(&esm)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes)?;

    for (wkey, cells) in &index.cells.exterior_cells {
        if !world_sub.is_empty() && !wkey.to_ascii_lowercase().contains(&world_sub) {
            continue;
        }
        let mut hits: Vec<(&(i32, i32), &str, usize, bool)> = cells
            .iter()
            .filter(|(_, c)| c.editor_id.to_ascii_lowercase().contains(&cell_sub))
            .map(|(g, c)| (g, c.editor_id.as_str(), c.references.len(), c.landscape.is_some()))
            .collect();
        if hits.is_empty() {
            continue;
        }
        hits.sort_by_key(|(g, _, _, _)| (g.1, g.0));
        println!("worldspace '{}' ({} ext cells) — matches for '{}':", wkey, cells.len(), cell_sub);
        let (mut sx, mut sy, mut n) = (0i64, 0i64, 0i64);
        for (g, edid, refs, land) in &hits {
            println!("  grid ({:>4},{:>4})  refs={:<4} land={}  edid='{}'", g.0, g.1, refs, land, edid);
            sx += g.0 as i64;
            sy += g.1 as i64;
            n += 1;
        }
        if n > 0 {
            println!("  → centroid ≈ ({}, {})  [{} cells]", sx / n, sy / n, n);
        }
    }
    Ok(())
}
