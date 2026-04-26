//! Quick diagnostic: dump every REFR in a named interior cell, with
//! base form → STAT model path lookup. Used to ground-truth what
//! content a cell actually places. Invoke with:
//!
//! ```text
//! cargo run -p byroredux-plugin --example dump_cell_refs -- <ESM> <CELL_EDID>
//! ```

use byroredux_plugin::esm;
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: ESM CELL_EDID"))?;
    let cell_edid = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: ESM CELL_EDID"))?;

    let bytes = std::fs::read(&esm_path)?;
    println!(
        "Parsing {} ({:.1} MB)…",
        esm_path,
        bytes.len() as f64 / 1_048_576.0
    );

    let index = esm::records::parse_esm(&bytes)?;
    // `EsmCellIndex.statics` already holds every base form with a MODL
    // field extracted during the cell-reference walk, so we don't need
    // to pull per-record kind ourselves.
    let mut form_to_model: HashMap<u32, String> = HashMap::new();
    for (fid, stat) in index.cells.statics.iter() {
        if !stat.model_path.is_empty() {
            form_to_model.insert(*fid, stat.model_path.clone());
        }
    }

    let key = cell_edid.to_ascii_lowercase();
    let cell = index
        .cells
        .cells
        .get(&key)
        .ok_or_else(|| anyhow::anyhow!("cell '{}' not in index", cell_edid))?;

    println!(
        "\nCell {} (form {:08X}): {} references",
        cell.editor_id,
        cell.form_id,
        cell.references.len()
    );

    let mut resolved = 0usize;
    let mut unknown = 0usize;
    let mut sky_candidates: Vec<(u32, [f32; 3], String)> = Vec::new();

    for r in &cell.references {
        let model = form_to_model
            .get(&r.base_form_id)
            .cloned()
            .unwrap_or_else(|| String::from("<unknown base>"));
        if form_to_model.contains_key(&r.base_form_id) {
            resolved += 1;
        } else {
            unknown += 1;
        }
        let lower = model.to_ascii_lowercase();
        // Anything with "sky", "cloud", "window", "pane", "glass",
        // "backdrop" in the path is a sky-backdrop candidate.
        if lower.contains("sky")
            || lower.contains("cloud")
            || lower.contains("window")
            || lower.contains("pane")
            || lower.contains("glass")
            || lower.contains("backdrop")
        {
            sky_candidates.push((r.base_form_id, r.position, model.clone()));
        }
    }

    println!(
        "\n Base-form resolution: {}/{} ({} unknown)",
        resolved,
        cell.references.len(),
        unknown
    );

    if !sky_candidates.is_empty() {
        println!(
            "\n{} sky/window/glass candidate references:",
            sky_candidates.len()
        );
        for (fid, pos, model) in &sky_candidates {
            println!(
                "  {:08X}  pos=({:+8.0}, {:+8.0}, {:+8.0})  {}",
                fid, pos[0], pos[1], pos[2], model
            );
        }
    }

    Ok(())
}
