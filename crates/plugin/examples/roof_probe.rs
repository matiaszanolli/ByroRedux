//! Throwaway #floating-roof probe: scan loaded exterior cells in a grid
//! window, surface REFRs whose base mesh path contains "roof" and report
//! their Z + whether the base form resolves to a STAT model, plus the
//! overall top-Z REFRs per cell.
//!
//! Usage: cargo run -p byroredux-plugin --example roof_probe -- <ESM> <WORLD_SUBSTR> <gx_lo> <gx_hi> <gy_lo> <gy_hi>

use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm = args.next().expect("usage: ESM WORLD gx_lo gx_hi gy_lo gy_hi");
    let world_sub = args.next().unwrap_or_default().to_ascii_lowercase();
    let gx_lo: i32 = args.next().unwrap().parse()?;
    let gx_hi: i32 = args.next().unwrap().parse()?;
    let gy_lo: i32 = args.next().unwrap().parse()?;
    let gy_hi: i32 = args.next().unwrap().parse()?;

    let bytes = std::fs::read(&esm)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes)?;

    let mut form_to_model: HashMap<u32, String> = HashMap::new();
    for (fid, stat) in index.cells.statics.iter() {
        form_to_model.insert(*fid, stat.model_path.clone());
    }

    for (wkey, cells) in &index.cells.exterior_cells {
        if !world_sub.is_empty() && !wkey.to_ascii_lowercase().contains(&world_sub) {
            continue;
        }
        let mut grids: Vec<&(i32, i32)> = cells
            .keys()
            .filter(|(gx, gy)| *gx >= gx_lo && *gx <= gx_hi && *gy >= gy_lo && *gy <= gy_hi)
            .collect();
        grids.sort_by_key(|(gx, gy)| (*gy, *gx));

        // Roof-mesh REFRs across the whole window, sorted by Z.
        let mut roofs: Vec<(i32, i32, u32, u32, [f32; 3], String, bool)> = Vec::new();
        // Global top-Z (any mesh) across the window.
        let mut all_z: Vec<(i32, i32, u32, u32, f32, String, bool)> = Vec::new();

        for g in &grids {
            let cell = &cells[*g];
            for r in &cell.references {
                let known = form_to_model.contains_key(&r.base_form_id);
                let mesh = form_to_model
                    .get(&r.base_form_id)
                    .cloned()
                    .unwrap_or_else(|| "<no STAT model>".to_string());
                let ml = mesh.to_ascii_lowercase();
                if ml.contains("roof") || ml.contains("thatch")
                    || ml.contains("architecture") || ml.contains("riverwood")
                    || ml.contains("house") || ml.contains("\\rw")
                    || ml.contains("farm") || ml.contains("mill") {
                    roofs.push((g.0, g.1, r.form_id, r.base_form_id, r.position, mesh.clone(), known));
                }
                all_z.push((g.0, g.1, r.form_id, r.base_form_id, r.position[2], mesh, known));
            }
        }

        roofs.sort_by(|a, b| b.4[2].partial_cmp(&a.4[2]).unwrap());
        all_z.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap());

        println!("=== worldspace '{}' window x[{}..{}] y[{}..{}] : {} cells ===", wkey, gx_lo, gx_hi, gy_lo, gy_hi, grids.len());

        println!("\n--- ROOF/THATCH-mesh REFRs (sorted by Z desc) ---");
        println!("{:>5} {:>5} {:>10} {:>10} {:>10} {:>10} {:>10}  base_mesh", "gx","gy","form","base","x","y","z");
        for (gx, gy, fid, base, p, mesh, known) in roofs.iter().take(40) {
            println!("{:>5} {:>5} {:>10X} {:>10X} {:>10.0} {:>10.0} {:>10.0}  {}{}", gx, gy, fid, base, p[0], p[1], p[2], mesh, if *known {""} else {" [MISSING-STAT]"});
        }

        println!("\n--- TOP 25 highest-Z REFRs (any mesh) ---");
        println!("{:>5} {:>5} {:>10} {:>10} {:>10}  base_mesh", "gx","gy","form","base","z");
        for (gx, gy, fid, base, z, mesh, known) in all_z.iter().take(25) {
            println!("{:>5} {:>5} {:>10X} {:>10X} {:>10.0}  {}{}", gx, gy, fid, base, z, mesh, if *known {""} else {" [MISSING-STAT]"});
        }

        // Distinct architecture/farmhouse meshes with count + max Z + any-missing flag.
        println!("\n--- DISTINCT Architecture\\Farmhouse meshes (count, max Z) ---");
        let mut by_mesh: HashMap<String, (usize, f32, bool)> = HashMap::new();
        for (_, _, _, _, z, mesh, known) in all_z.iter() {
            let ml = mesh.to_ascii_lowercase();
            if ml.contains("architecture\\farmhouse") || ml.contains("riverwood") {
                let e = by_mesh.entry(mesh.clone()).or_insert((0, f32::MIN, true));
                e.0 += 1;
                e.1 = e.1.max(*z);
                e.2 = e.2 && *known;
            }
        }
        let mut v: Vec<(&String,&(usize,f32,bool))> = by_mesh.iter().collect();
        v.sort_by(|a,b| b.1.1.partial_cmp(&a.1.1).unwrap());
        for (mesh,(cnt,maxz,known)) in v {
            println!("  cnt={:<4} maxZ={:>8.0} {}{}", cnt, maxz, mesh, if *known {""} else {" [MISSING-STAT]"});
        }
    }
    Ok(())
}
