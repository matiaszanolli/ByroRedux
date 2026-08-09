use byroredux_bsa::Ba2Archive;
use std::collections::BTreeMap;

fn main() {
    let dir = std::path::PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Starfield/Data");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|x| x.eq_ignore_ascii_case("ba2"))
                .unwrap_or(false)
        })
        .collect();
    entries.sort();
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    let mut per_archive: Vec<(String, usize, usize, usize)> = Vec::new();
    for p in entries {
        let Ok(a) = Ba2Archive::open(&p) else {
            continue;
        };
        let (mut bgsm, mut bgem, mut mat) = (0usize, 0usize, 0usize);
        for f in a.list_files() {
            let l = f.to_ascii_lowercase();
            if let Some(dot) = l.rfind('.') {
                *tally.entry(l[dot..].to_string()).or_default() += 1;
            }
            if l.ends_with(".bgsm") {
                bgsm += 1
            } else if l.ends_with(".bgem") {
                bgem += 1
            } else if l.ends_with(".mat") {
                mat += 1
            }
        }
        if bgsm + bgem + mat > 0 {
            per_archive.push((
                p.file_name().unwrap().to_string_lossy().into_owned(),
                bgsm,
                bgem,
                mat,
            ));
        }
    }
    println!("--- archives containing loose material files (bgsm/bgem/mat)");
    for (n, a, b, c) in &per_archive {
        println!("  {n}: bgsm={a} bgem={b} mat={c}");
    }
    println!("--- top extensions across all Starfield BA2s");
    let mut v: Vec<_> = tally.into_iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (e, c) in v.iter().take(25) {
        println!("  {c:>9}  {e}");
    }
}
