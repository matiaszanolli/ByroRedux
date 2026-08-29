//! Full-archive `emissive_mult` census, per `EmissiveSource`.
//!
//! Grounds §4 of `docs/engine/nifal.md` (#3337): the original comparison
//! sampled four values per source, which is not enough to characterise a
//! distribution. This walks every `.nif` in one or more BSA/BA2 archives,
//! imports each mesh, and histograms the authored multiplier per source.
//!
//! Usage:
//!   cargo run --release -p byroredux-nif --example emissive_census -- <archive>...

use std::collections::BTreeMap;

use byroredux_core::ecs::components::material::{
    emissive_contribution_is_authored, EmissiveSource,
};

fn src_name(s: EmissiveSource) -> &'static str {
    match s {
        EmissiveSource::None => "none",
        EmissiveSource::Material => "mat",
        EmissiveSource::Lighting => "lit",
        EmissiveSource::Effect => "fx",
    }
}

fn main() {
    let archives: Vec<String> = std::env::args().skip(1).collect();
    assert!(!archives.is_empty(), "usage: emissive_census <archive>...");
    // source -> quantised mult (×100) -> count
    let mut hist: BTreeMap<&'static str, BTreeMap<i64, usize>> = BTreeMap::new();
    let mut meshes = 0usize;
    let mut files = 0usize;
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
                continue;
            };
            let mut pool = byroredux_core::string::StringPool::new();
            for m in byroredux_nif::import::import_nif(&scene, &mut pool) {
                let mat = &m.material;
                if mat.emissive_source == EmissiveSource::None {
                    continue;
                }
                if !emissive_contribution_is_authored(mat.emissive_color, mat.emissive_mult) {
                    continue;
                }
                meshes += 1;
                let bucket = (mat.emissive_mult * 100.0).round() as i64;
                *hist
                    .entry(src_name(mat.emissive_source))
                    .or_default()
                    .entry(bucket)
                    .or_insert(0) += 1;
            }
        }
    }
    println!("# {files} NIFs, {meshes} authored-emissive meshes");
    for (src, buckets) in &hist {
        let total: usize = buckets.values().sum();
        let ge10: usize = buckets
            .iter()
            .filter(|(k, _)| **k >= 1000)
            .map(|(_, v)| *v)
            .sum();
        println!(
            "\n== {src}: {total} meshes, {ge10} ({:.1}%) with mult >= 10",
            100.0 * ge10 as f64 / total.max(1) as f64
        );
        let mut rows: Vec<(i64, usize)> = buckets.iter().map(|(k, v)| (*k, *v)).collect();
        rows.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        for (k, c) in rows.iter().take(24) {
            println!("   {:>8.2}: {c}", *k as f64 / 100.0);
        }
    }
}
