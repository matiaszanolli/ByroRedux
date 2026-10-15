//! Tally the object classes of every behavior graph in a Fallout 4 animation archive:
//! how many graphs use each class and how many objects of it there are in all.
//!
//! ```text
//! cargo run --release -p byroredux-hkx --example class_census_ba2 -- \
//!     "<Fallout 4>/Data/Fallout4 - Animations.ba2" [path-substring]
//! ```
use byroredux_bsa::Ba2Archive;
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let archive = Ba2Archive::open(args.next().expect("archive path")).expect("open");
    let filter = args.next().map(|f| f.to_ascii_lowercase());
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut graphs = 0;
    for path in archive.list_files() {
        let lower = path.to_ascii_lowercase();
        if !lower.contains("\\behaviors\\") || !lower.ends_with(".hkx") || filter.as_ref().is_some_and(|f| !lower.contains(f)) {
            continue;
        }
        let Ok(bytes) = archive.extract(path) else { continue };
        let Ok(census) = byroredux_hkx::class_census(&bytes) else { continue };
        graphs += 1;
        for (class, count) in census {
            let entry = tally.entry(class).or_default();
            entry.0 += 1;
            entry.1 += count;
        }
    }
    println!("{graphs} graphs");
    let mut rows: Vec<_> = tally.into_iter().collect();
    rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(b.1 .1.cmp(&a.1 .1)));
    for (class, (files, objects)) in rows {
        println!("{files:5} graphs {objects:7} objects  {class}");
    }
}
