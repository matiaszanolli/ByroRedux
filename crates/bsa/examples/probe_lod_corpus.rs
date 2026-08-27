fn main() {
    let mut total = 0usize;
    for path in std::env::args().skip(1) {
        let Ok(archive) = byroredux_bsa::BsaArchive::open(&path) else {
            eprintln!("skip {path}");
            continue;
        };
        let mut lod = 0usize;
        let mut blocks: std::collections::BTreeMap<String, std::collections::BTreeMap<String, usize>> =
            Default::default();
        let mut terrain: std::collections::BTreeMap<String, std::collections::BTreeMap<String, usize>> =
            Default::default();
        let mut far_nif = 0usize;
        let mut distantlod = 0usize;
        for f in archive.list_files() {
            let l = f.to_ascii_lowercase();
            if l.ends_with("_far.nif") { far_nif += 1; }
            if l.contains("distantlod\\") { distantlod += 1; }
            if !l.contains("landscape\\lod\\") { continue; }
            lod += 1;
            let rest = &l[l.find("landscape\\lod\\").unwrap() + "landscape\\lod\\".len()..];
            let world = rest.split('\\').next().unwrap_or("?").to_string();
            let level = rest
                .rsplit('.')
                .find(|s| s.starts_with("level"))
                .unwrap_or_else(|| {
                    rest.split('.').find(|s| s.starts_with("level")).unwrap_or("?")
                })
                .to_string();
            let bucket = if rest.contains("\\blocks\\") { &mut blocks } else { &mut terrain };
            *bucket.entry(world).or_default().entry(level).or_default() += 1;
        }
        println!("{path}\n  landscape\\lod entries={lod}  _far.nif={far_nif}  distantlod={distantlod}");
        for (label, map) in [("terrain", &terrain), ("blocks ", &blocks)] {
            for (world, levels) in map {
                let n: usize = levels.values().sum();
                println!("    {label} {world}: {n}  {levels:?}");
            }
        }
        total += lod;
    }
    println!("TOTAL landscape\\lod entries across archives: {total}");
}
