//! TEMP scratch (audit 2026-08-16): how many Oblivion.esm REFR placements
//! point at a base record whose model is one of the BSXFlags-bit-5 NIFs the
//! cell loader drops wholesale?
//!
//! argv[1] = Oblivion.esm, argv[2] = file with one dropped mesh path per line
//! (as printed by `_tmp_obl_bsx`, i.e. `meshes\...nif`).
use byroredux_plugin::esm::records::parse_esm;
use std::collections::{HashMap, HashSet};

fn norm(p: &str) -> String {
    let p = p.replace('/', "\\").to_ascii_lowercase();
    p.strip_prefix("meshes\\").unwrap_or(&p).to_string()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let esm = args.next().unwrap();
    let list = args.next().unwrap();
    let dropped: HashSet<String> = std::fs::read_to_string(&list)
        .unwrap()
        .lines()
        .map(|l| norm(l.trim()))
        .filter(|l| !l.is_empty())
        .collect();
    eprintln!("dropped models: {}", dropped.len());

    let bytes = std::fs::read(&esm).unwrap();
    let index = parse_esm(&bytes).unwrap();
    let cells = &index.cells;

    // base form id -> model path, for the dropped set only
    let mut hit_bases: HashMap<u32, String> = HashMap::new();
    for (&fid, s) in &cells.statics {
        if dropped.contains(&norm(&s.model_path)) {
            hit_bases.insert(fid, norm(&s.model_path));
        }
    }
    eprintln!(
        "base records referencing a dropped model: {}",
        hit_bases.len()
    );

    let mut per_model: HashMap<String, usize> = HashMap::new();
    let mut interior_cells = HashSet::new();
    let mut exterior_cells = HashSet::new();
    let mut total = 0usize;

    let mut count = |cd: &byroredux_plugin::esm::cell::CellData,
                     key: String,
                     interior: bool,
                     per_model: &mut HashMap<String, usize>,
                     total: &mut usize,
                     ic: &mut HashSet<String>,
                     ec: &mut HashSet<String>| {
        for r in &cd.references {
            if let Some(m) = hit_bases.get(&r.base_form_id) {
                *per_model.entry(m.clone()).or_default() += 1;
                *total += 1;
                if interior {
                    ic.insert(key.clone());
                } else {
                    ec.insert(key.clone());
                }
            }
        }
    };

    for (name, cd) in &cells.cells {
        count(
            cd,
            name.clone(),
            true,
            &mut per_model,
            &mut total,
            &mut interior_cells,
            &mut exterior_cells,
        );
    }
    for (ws, grid) in &cells.exterior_cells {
        for (xy, cd) in grid {
            count(
                cd,
                format!("{ws}{xy:?}"),
                false,
                &mut per_model,
                &mut total,
                &mut interior_cells,
                &mut exterior_cells,
            );
        }
    }

    println!("TOTAL dropped-model REFR placements: {total}");
    println!(
        "  distinct interior cells affected: {}",
        interior_cells.len()
    );
    println!(
        "  distinct exterior cells affected: {}",
        exterior_cells.len()
    );
    let mut v: Vec<_> = per_model.into_iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (m, c) in v {
        println!("  {c:6}  {m}");
    }
}
