use byroredux_bsa::Ba2Archive;

fn main() {
    let dir = std::path::PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Starfield/Data");
    let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("ba2")).unwrap_or(false))
        .collect();
    entries.sort();
    for p in entries {
        let a = match Ba2Archive::open(&p) { Ok(a) => a, Err(e) => { println!("OPENFAIL {} {}", p.display(), e); continue } };
        let files = a.list_files();
        let cdbs: Vec<&str> = files.iter().filter(|f| {
            let l = f.replace('/', "\\").to_ascii_lowercase();
            l.ends_with("materialsbeta.cdb") || l.ends_with(".cdb")
        }).copied().collect();
        if !cdbs.is_empty() {
            println!("== {} ({} files)", p.file_name().unwrap().to_string_lossy(), files.len());
            for c in cdbs { println!("     {}", c); }
        }
    }
}
