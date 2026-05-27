//! Validate cell EditorIDs against an ESM without launching the engine.
//! Used to fix `assets/debug_profiles.toml` sample_cells (F6 of the
//! 2026-05-26 Fallout symptom sweep).
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example probe_cells -- <ESM> <CELL1> <CELL2>…

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: probe_cells ESM CELL1 [CELL2 …]"))?;
    let candidates: Vec<String> = args.collect();
    if candidates.is_empty() {
        anyhow::bail!("supply at least one cell EditorID");
    }

    let bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes)?;
    let total = index.cells.cells.len();
    eprintln!(
        "[probe_cells] {} interior cells in {} ({} candidates)",
        total,
        esm_path,
        candidates.len()
    );

    for cand in &candidates {
        let key = cand.to_ascii_lowercase();
        if let Some(cell) = index.cells.cells.get(&key) {
            println!(
                "OK   {} ({} REFRs, form {:08X})",
                cand,
                cell.references.len(),
                cell.form_id
            );
        } else {
            // Provide a 5-best-match substring suggestion to make it
            // obvious what the right EditorID is.
            let needle = key.as_str();
            let mut matches: Vec<&str> = index
                .cells
                .cells
                .values()
                .filter(|c| c.editor_id.to_ascii_lowercase().contains(needle))
                .map(|c| c.editor_id.as_str())
                .collect();
            matches.sort();
            matches.truncate(5);
            println!("MISS {}  (close: {:?})", cand, matches);
        }
    }
    Ok(())
}
