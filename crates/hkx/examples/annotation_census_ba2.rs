//! Tally the annotation labels of the clips of a Fallout 4 animation archive whose path
//! contains a substring, and how many sync points (`Sync*`) each clip carries.
//!
//! ```text
//! cargo run --release -p byroredux-hkx --example annotation_census_ba2 -- \
//!     "<Fallout 4>/Data/Fallout4 - Animations.ba2" <path-substring>
//! ```
use byroredux_bsa::Ba2Archive;
use byroredux_hkx::decode_spline_animation;
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let archive = Ba2Archive::open(args.next().expect("archive path")).expect("open");
    let filter = args.next().expect("a substring").to_ascii_lowercase();
    let mut labels: BTreeMap<String, usize> = BTreeMap::new();
    let mut sync_counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut order: BTreeMap<String, usize> = BTreeMap::new();
    let mut clips = 0;
    for path in archive.list_files() {
        let lower = path.to_ascii_lowercase();
        if !lower.ends_with(".hkx") || lower.contains("\\behaviors\\") || !lower.contains(&filter) {
            continue;
        }
        let Ok(bytes) = archive.extract(path) else { continue };
        let Ok(animation) = decode_spline_animation(&bytes) else { continue };
        clips += 1;
        let mut syncs: Vec<(f32, &str)> = Vec::new();
        for annotation in &animation.annotations {
            *labels.entry(annotation.text.clone()).or_default() += 1;
            if annotation.text.to_ascii_lowercase().starts_with("sync") {
                syncs.push((annotation.time, annotation.text.as_str()));
            }
        }
        syncs.sort_by(|a, b| a.0.total_cmp(&b.0));
        *sync_counts.entry(syncs.len()).or_default() += 1;
        let pattern: Vec<&str> = syncs.iter().map(|s| s.1).collect();
        *order.entry(pattern.join(">")).or_default() += 1;
    }
    println!("{clips} clips");
    println!("sync points per clip: {sync_counts:?}");
    println!("sync label orders: {order:?}");
    let mut by_count: Vec<_> = labels.into_iter().collect();
    by_count.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (label, count) in by_count.iter().take(25) {
        println!("{count:6} {label}");
    }
}
