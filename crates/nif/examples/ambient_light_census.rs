//! Full-archive `NiAmbientLight` census, grounding the W2.9 redesign
//! decision (light & shadow correctness campaign): does vanilla content
//! use subtree-scoped `NiAmbientLight`s at scale, do they carry
//! non-default colours, and do they actually author
//! `affected_node_names` scoping — or would folding them into the XCLL
//! ambient term lose nothing?
//!
//! Walks every `.nif` in one or more BSA/BA2 archives, imports each
//! mesh's light chain, and reports per-game:
//!   * files with ≥1 ambient light / total ambient lights
//!   * color buckets (black, near-black, saturated, other)
//!   * how many carry non-empty `affected_node_names`
//!   * co-occurrence with point/spot/directional lights in the same file
//!
//! Usage:
//!   cargo run --release -p byroredux-nif --example ambient_light_census -- <archive>...

use std::collections::BTreeMap;

use byroredux_core::ecs::components::light::LightKind;

fn main() {
    let archives: Vec<String> = std::env::args().skip(1).collect();
    assert!(
        !archives.is_empty(),
        "usage: ambient_light_census <archive>..."
    );

    let mut files = 0usize;
    let mut files_with_ambient = 0usize;
    let mut ambient_total = 0usize;
    let mut color_buckets: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut with_affected_nodes = 0usize;
    let mut with_radius = 0usize;
    // co-occurrence: files that have ambient AND at least one other kind
    let mut files_ambient_plus_other = 0usize;
    let mut per_file_ambient_hist: BTreeMap<usize, usize> = BTreeMap::new();
    let mut parse_failures = 0usize;

    for path in &archives {
        let entries: Vec<(String, Vec<u8>)> = if path.to_ascii_lowercase().ends_with(".ba2") {
            let a = byroredux_bsa::Ba2Archive::open(path).expect("open ba2");
            a.list_files()
                .iter()
                .filter(|f| f.to_ascii_lowercase().ends_with(".nif"))
                .map(|f| (f.to_string(), a.extract(f).unwrap_or_default()))
                .collect()
        } else {
            let a = byroredux_bsa::BsaArchive::open(path).expect("open bsa");
            a.list_files()
                .iter()
                .filter(|f| f.to_ascii_lowercase().ends_with(".nif"))
                .map(|f| (f.to_string(), a.extract(f).unwrap_or_default()))
                .collect()
        };
        for (_name, bytes) in entries {
            if bytes.is_empty() {
                continue;
            }
            files += 1;
            let Some(scene) = byroredux_nif::parse_nif(&bytes).ok() else {
                parse_failures += 1;
                continue;
            };
            let lights = byroredux_nif::import::import_nif_lights(&scene);
            let ambient: Vec<_> = lights
                .iter()
                .filter(|l| l.kind == LightKind::Ambient)
                .collect();
            if ambient.is_empty() {
                continue;
            }
            files_with_ambient += 1;
            ambient_total += ambient.len();
            *per_file_ambient_hist
                .entry(ambient.len())
                .or_insert(0) += 1;
            if lights.iter().any(|l| l.kind != LightKind::Ambient) {
                files_ambient_plus_other += 1;
            }
            for l in &ambient {
                if !l.affected_node_names.is_empty() {
                    with_affected_nodes += 1;
                }
                if l.radius > 0.0 {
                    with_radius += 1;
                }
                let [r, g, b] = l.color;
                let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                let bucket = if lum < 1.0 / 255.0 {
                    "black"
                } else if lum < 0.1 {
                    "near_black"
                } else if (r - g).abs() > 0.15 || (g - b).abs() > 0.15 {
                    "saturated"
                } else {
                    "white_or_grey"
                };
                *color_buckets.entry(bucket).or_insert(0) += 1;
            }
        }
    }

    println!("files scanned:        {files}");
    println!("parse failures:       {parse_failures}");
    println!("files w/ ambient:     {files_with_ambient}");
    println!("ambient lights total: {ambient_total}");
    println!(
        "  w/ affected_nodes:  {with_affected_nodes}",
    );
    println!("  w/ radius > 0:      {with_radius}");
    println!(
        "files ambient+other:  {files_ambient_plus_other}"
    );
    println!("color buckets:");
    for (k, v) in &color_buckets {
        println!("  {k:14} {v}");
    }
    let mut hist: Vec<_> = per_file_ambient_hist.into_iter().collect();
    hist.sort();
    println!("per-file ambient count histogram (count: files):");
    for (k, v) in hist.iter().take(12) {
        println!("  {k}: {v}");
    }
    if hist.len() > 12 {
        println!("  … ({} more buckets)", hist.len() - 12);
    }
}
