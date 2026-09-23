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

    let npc_refs = cell
        .references
        .iter()
        .filter_map(|reference| {
            let npc = index.npcs.get(&reference.base_form_id)?;
            let hair = npc
                .runtime_facegen
                .as_ref()
                .and_then(|face| face.hair_form_id)
                .and_then(|form_id| index.hair.get(&form_id))
                .map(|hair| hair.model_path.as_str());
            Some((reference, npc, hair))
        })
        .collect::<Vec<_>>();
    if !npc_refs.is_empty() {
        println!("\n{} NPC references:", npc_refs.len());
        for (reference, npc, hair) in npc_refs {
            let hair_color = npc
                .runtime_facegen
                .as_ref()
                .and_then(|face| face.hair_color_rgb);
            println!(
                "  REFR {:08X} base={:08X} npc={} hair={hair:?} hclr={hair_color:?}",
                reference.form_id, reference.base_form_id, npc.editor_id,
            );
        }
    }

    let light_refs = cell
        .references
        .iter()
        .filter_map(|reference| {
            let base = index.cells.statics.get(&reference.base_form_id)?;
            let light = base.light_data.as_ref()?;
            Some((reference, base, light))
        })
        .collect::<Vec<_>>();
    if !light_refs.is_empty() {
        println!("\n{} placed LIGH references:", light_refs.len());
        for (reference, base, light) in light_refs {
            println!(
                "  REFR {:08X} base={:08X} {} pos=({:+.0}, {:+.0}, {:+.0}) \\
                 rot={:?} radius={:.0} color=({:.2}, {:.2}, {:.2}) flags={:#010X} \\
                 sf_type={} fov={} model={:?}",
                reference.form_id,
                reference.base_form_id,
                base.editor_id,
                reference.position[0],
                reference.position[1],
                reference.position[2],
                reference.rotation,
                light.radius,
                light.color[0],
                light.color[1],
                light.color[2],
                light.flags,
                light.starfield_light_type,
                light.fov_degrees,
                base.model_path,
            );
        }
    }

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
