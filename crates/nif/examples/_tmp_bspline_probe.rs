use byroredux_bsa::BsaArchive;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let bsa = std::env::args().nth(1).unwrap();
    let a = BsaArchive::open(&bsa).unwrap();
    let files: Vec<String> = a
        .list_files()
        .iter()
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            l.ends_with(".kf") || l.ends_with(".nif")
        })
        .map(|s| s.to_string())
        .collect();
    let mut hist: BTreeMap<String, usize> = BTreeMap::new();
    for p in &files {
        let Ok(bytes) = a.extract(p) else { continue };
        let Ok(scene) = parse_nif(&bytes) else { continue };
        for b in &scene.blocks {
            let n = b.block_type_name();
            if n.contains("BSpline") || n.contains("Interpolator") {
                *hist.entry(n.to_string()).or_insert(0) += 1;
            }
        }
    }
    for (k, v) in hist {
        println!("{v}\t{k}");
    }
}
