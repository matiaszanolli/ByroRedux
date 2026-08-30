//! TEMP scratch (audit 2026-08-30): FO3 ESM coverage census.
//! Diffs the parser's indexed record population against the raw
//! on-disk record-type census of Fallout3.esm.
use byroredux_plugin::esm::parse_esm;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout3.esm".into()
    });
    let bytes = std::fs::read(&path).expect("read esm");
    let index = parse_esm(&bytes).expect("parse esm");

    println!("{}", index.category_breakdown());
    println!("index.total() = {}", index.total());

    let c = &index.cells;
    let mut int_refs = 0usize;
    for cell in c.cells.values() {
        int_refs += cell.references.len();
    }
    let mut ext_cells = 0usize;
    let mut ext_refs = 0usize;
    for (_w, grid) in c.exterior_cells.iter() {
        ext_cells += grid.len();
        for cell in grid.values() {
            ext_refs += cell.references.len();
        }
    }
    let mut pers_refs = 0usize;
    for cell in c.worldspace_persistent_cells.values() {
        pers_refs += cell.references.len();
    }
    let mut land_cells = 0usize;
    for (_w, grid) in c.exterior_cells.iter() {
        for cell in grid.values() {
            if cell.landscape.is_some() {
                land_cells += 1;
            }
        }
    }
    println!(
        "interior cells={} refs={} | exterior cells={} refs={} (worldspaces={}) | persistent cells={} refs={} | LAND-bearing ext cells={}",
        c.cells.len(),
        int_refs,
        ext_cells,
        ext_refs,
        c.exterior_cells.len(),
        c.worldspace_persistent_cells.len(),
        pers_refs,
        land_cells,
    );
    println!(
        "TOTAL placed refs indexed = {}",
        int_refs + ext_refs + pers_refs
    );
    println!(
        "statics={} texture_sets={} scols={} landscape_textures={} worldspaces={}",
        c.statics.len(),
        c.texture_sets.len(),
        c.scols.len(),
        c.landscape_textures.len(),
        c.worldspaces.len(),
    );
    probe_proj(&index);
    {
        use std::collections::HashSet;
        let have: HashSet<u32> = c.texture_sets.keys().copied().collect();
        let raw: Vec<u32> = std::fs::read_to_string("/tmp/audit/fo3/txst_ids.txt")
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.trim().parse::<u32>().ok())
            .collect();
        let missing: Vec<String> = raw
            .iter()
            .filter(|f| !have.contains(f))
            .map(|f| format!("{f:#x}"))
            .collect();
        println!(
            "TXST raw={} indexed={} missing={:?}",
            raw.len(),
            have.len(),
            missing
        );
    }
    // Worldspace listing — the Capital Wasteland identity check.
    let mut ws: Vec<_> = c.worldspaces.keys().cloned().collect();
    ws.sort();
    println!("worldspace EDIDs ({}): {:?}", ws.len(), ws);
    for key in ["megatonplayerhouse", "megatoncommonhouse", "megatonchurch"] {
        if let Some(cell) = c.cells.get(key) {
            println!(
                "  INTERIOR {key}: refs={} lighting={} form={:#x}",
                cell.references.len(),
                cell.lighting.is_some(),
                cell.form_id
            );
        } else {
            println!("  INTERIOR {key}: not indexed");
        }
    }
    // WNAM parent chains + ext-cell population per worldspace.
    let by_fid: std::collections::HashMap<u32, &str> = c
        .worldspaces
        .values()
        .map(|w| (w.form_id, w.editor_id.as_str()))
        .collect();
    let mut rows: Vec<String> = Vec::new();
    for w in c.worldspaces.values() {
        let n = c
            .exterior_cells
            .get(&w.editor_id.to_ascii_lowercase())
            .map(|m| m.len())
            .unwrap_or(0);
        let par = w.parent_worldspace.map(|f| {
            by_fid
                .get(&f)
                .map(|s| s.to_string())
                .unwrap_or(format!("{f:#x}?"))
        });
        rows.push(format!(
            "{:<24} cells={:<6} parent={:?} parent_flags={:#06x} climate_key={}",
            w.editor_id,
            n,
            par,
            w.parent_flags,
            w.editor_id.to_ascii_lowercase()
        ));
    }
    rows.sort();
    for r in rows {
        println!("  {r}");
    }
}
// appended probe: are the PGRE base PROJ forms resolvable to a mesh?
fn probe_proj(index: &byroredux_plugin::esm::EsmIndex) {
    for fid in [17402u32, 263128, 212043, 365641] {
        let s = index.cells.statics.get(&fid).map(|s| s.model_path.clone());
        let p = index.projectiles.get(&fid).is_some();
        println!("form {fid:#x}: in statics={s:?} in projectiles={p}");
    }
}
