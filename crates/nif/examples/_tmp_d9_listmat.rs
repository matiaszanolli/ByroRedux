use byroredux_bsa::Ba2Archive;
fn main() {
    for path in std::env::args().skip(1) {
        let Ok(a) = Ba2Archive::open(&path) else { eprintln!("skip {path}"); continue };
        let files = a.list_files();
        let mut hist: std::collections::BTreeMap<String, usize> = Default::default();
        for f in &files {
            let l = f.to_ascii_lowercase();
            let ext = l.rsplit('.').next().unwrap_or("").to_string();
            *hist.entry(ext).or_insert(0) += 1;
        }
        println!("=== {path} ({} files)", files.len());
        println!("{hist:?}");
        for f in files.iter().take(10) { println!("  {f}"); }
        // sample bgsm
        for f in files.iter().filter(|f| f.to_ascii_lowercase().ends_with(".bgsm")).take(10) { println!("  BGSM {f}"); }
        for f in files.iter().filter(|f| f.to_ascii_lowercase().ends_with(".mat")).take(5) { println!("  MAT {f}"); }
    }
}
