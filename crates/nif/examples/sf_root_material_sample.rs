//! One-shot empirical sampler for #1183 — counts how often Starfield
//! `BSLightingShaderProperty` blocks carry a non-empty `root_material_path`
//! while `net.name` is NOT a material reference (the case where the
//! importer fallback added in #1183 actually fires).
//!
//! Usage:
//!   cargo run --release -p byroredux-nif --example sf_root_material_sample -- <ba2>
//!
//! Output: histogram of (name-is-material × root-is-material) per archive.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::parse_nif;
use std::path::PathBuf;

fn is_material_reference(name: &str) -> bool {
    let trimmed = name.trim_end_matches(|c: char| c == '\0' || c.is_whitespace());
    let n = trimmed.len();
    if n < 4 {
        return false;
    }
    let tail4 = &trimmed.as_bytes()[n - 4..];
    if tail4.eq_ignore_ascii_case(b".mat") {
        return true;
    }
    if n < 5 {
        return false;
    }
    let tail5 = &trimmed.as_bytes()[n - 5..];
    tail5.eq_ignore_ascii_case(b".bgsm") || tail5.eq_ignore_ascii_case(b".bgem")
}

fn main() {
    let archive_path = PathBuf::from(std::env::args().nth(1).expect("usage: <ba2>"));
    let archive = Ba2Archive::open(&archive_path).expect("open BA2");
    let mut total_nifs: usize = 0;
    let mut parsed_nifs: usize = 0;
    let mut total_lsps: usize = 0;
    let mut name_is_mat: usize = 0;
    let mut name_is_label_root_is_mat: usize = 0;
    let mut name_is_label_root_is_label: usize = 0;
    let mut name_is_label_root_is_empty: usize = 0;
    let mut both_mat: usize = 0;

    let entries: Vec<String> = archive.list_files().iter().map(|s| s.to_string()).collect();
    for entry in &entries {
        if !entry.to_ascii_lowercase().ends_with(".nif") {
            continue;
        }
        total_nifs += 1;
        let Ok(bytes) = archive.extract(entry) else {
            continue;
        };
        let Ok(scene) = parse_nif(&bytes) else {
            continue;
        };
        parsed_nifs += 1;
        for block in &scene.blocks {
            let Some(lsp) = block
                .as_any()
                .downcast_ref::<byroredux_nif::blocks::shader::BSLightingShaderProperty>()
            else {
                continue;
            };
            total_lsps += 1;
            let name_is_material = lsp
                .net
                .name
                .as_deref()
                .map(is_material_reference)
                .unwrap_or(false);
            let root_is_material = lsp
                .root_material_path
                .as_deref()
                .map(is_material_reference)
                .unwrap_or(false);
            let root_is_empty = lsp.root_material_path.is_none();
            match (name_is_material, root_is_material, root_is_empty) {
                (true, true, _) => both_mat += 1,
                (true, false, _) => name_is_mat += 1,
                (false, true, _) => name_is_label_root_is_mat += 1,
                (false, false, true) => name_is_label_root_is_empty += 1,
                (false, false, false) => name_is_label_root_is_label += 1,
            }
        }
    }

    println!("=== {} ===", archive_path.display());
    println!("NIFs total:  {}", total_nifs);
    println!("NIFs parsed: {}", parsed_nifs);
    println!("LSP blocks:  {}", total_lsps);
    println!();
    println!("name=material  root=material  → {}", both_mat);
    println!("name=material  root=label     → {}", name_is_mat);
    println!(
        "name=label     root=material  → {} ← FALLBACK FIRES (#1183 captures these)",
        name_is_label_root_is_mat
    );
    println!(
        "name=label     root=label     → {}",
        name_is_label_root_is_label
    );
    println!(
        "name=label     root=empty     → {}",
        name_is_label_root_is_empty
    );
}
