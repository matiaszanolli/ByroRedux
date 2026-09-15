//! List every placed reference within a radius of a world position in one
//! exterior cell, with its base form, editor ID and model path.
//!
//! Traces a spawned engine entity back to the REFR that placed it when the
//! entity itself carries no form ID — e.g. a mesh with mangled texture paths
//! whose source NIF needs identifying. Positions are Bethesda Z-up; the
//! engine's Y-up `GlobalTransform` maps back as `(x, -z, y)`.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example refs_near -- <ESM> <worldspace> <gx,gy> <x,y,z> <radius>

use byroredux_plugin::esm;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [esm_path, world, grid, pos, radius] = args.as_slice() else {
        anyhow::bail!("usage: refs_near <ESM> <worldspace> <gx,gy> <x,y,z> <radius>");
    };
    let parse_list =
        |s: &str| -> Vec<f32> { s.split(',').filter_map(|v| v.trim().parse().ok()).collect() };
    let grid = parse_list(grid);
    let pos = parse_list(pos);
    let radius: f32 = radius.parse()?;
    anyhow::ensure!(grid.len() == 2 && pos.len() == 3, "bad grid or position");
    let grid = (grid[0] as i32, grid[1] as i32);

    let bytes = std::fs::read(esm_path)?;
    let index = esm::records::parse_esm(&bytes)?;
    let world = world.to_ascii_lowercase();
    // Persistent references (activators, doors, actors) live in the
    // worldspace's single persistent CELL, not the grid cell they stand in.
    let mut sources: Vec<(String, &esm::cell::CellData)> = Vec::new();
    for (wkey, cells) in &index.cells.exterior_cells {
        if wkey.to_ascii_lowercase().contains(&world) {
            if let Some(cell) = cells.get(&grid) {
                sources.push((format!("{wkey} ({},{})", grid.0, grid.1), cell));
            }
        }
    }
    for (wkey, cell) in &index.cells.worldspace_persistent_cells {
        if wkey.to_ascii_lowercase().contains(&world) {
            sources.push((format!("{wkey} persistent"), cell));
        }
    }
    for (label, cell) in sources {
        let mut hits: Vec<_> = cell
            .references
            .iter()
            .map(|r| {
                let d = ((r.position[0] - pos[0]).powi(2)
                    + (r.position[1] - pos[1]).powi(2)
                    + (r.position[2] - pos[2]).powi(2))
                .sqrt();
                (d, r)
            })
            .filter(|(d, _)| *d <= radius)
            .collect();
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        println!("=== {label}: {} refs within {radius}", hits.len());
        for (d, r) in hits {
            let (edid, model) = index
                .cells
                .statics
                .get(&r.base_form_id)
                .map(|s| (s.editor_id.as_str(), s.model_path.as_str()))
                .unwrap_or(("?", "?"));
            println!(
                "  d={d:>6.1} refr={:08X} base={:08X} pos=[{:.1}, {:.1}, {:.1}] {edid} {model}",
                r.form_id, r.base_form_id, r.position[0], r.position[1], r.position[2]
            );
        }
    }
    Ok(())
}
