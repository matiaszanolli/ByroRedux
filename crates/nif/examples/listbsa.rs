use byroredux_bsa::BsaArchive;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let archive = BsaArchive::open(&args[1]).expect("open");
    let pat = args.get(2).map(|s| s.to_lowercase()).unwrap_or_default();
    for f in archive.list_files() {
        let lower = f.to_lowercase();
        if pat.is_empty() || lower.contains(&pat) {
            println!("{}", f);
        }
    }
}
