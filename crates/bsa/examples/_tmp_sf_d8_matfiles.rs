use byroredux_bsa::Ba2Archive;
fn main() {
    for path in std::env::args().skip(1) {
        let Ok(a) = Ba2Archive::open(&path) else { eprintln!("skip {path}"); continue };
        let mut n_bgsm = 0; let mut n_mat = 0; let mut n_cdb = 0; let mut other = 0;
        let mut samples: Vec<String> = Vec::new();
        for f in a.list_files() {
            let l = f.to_ascii_lowercase();
            if l.ends_with(".bgsm") || l.ends_with(".bgem") { n_bgsm += 1; if samples.len()<3 {samples.push(f.to_string());} }
            else if l.ends_with(".mat") { n_mat += 1; }
            else if l.ends_with(".cdb") { n_cdb += 1; }
            else { other += 1; }
        }
        println!("{path}: total={} bgsm/bgem={n_bgsm} mat={n_mat} cdb={n_cdb} other={other} samples={samples:?}", a.file_count());
    }
}
