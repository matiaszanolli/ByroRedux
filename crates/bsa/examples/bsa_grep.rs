use byroredux_bsa::BsaArchive;
fn main() {
    let bsa = std::env::args().nth(1).expect("bsa path");
    let needle = std::env::args()
        .nth(2)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let archive = BsaArchive::open(&bsa).unwrap();
    let mut count = 0;
    for f in archive.list_files() {
        let lower = f.to_ascii_lowercase();
        if needle.is_empty() || lower.contains(&needle) {
            println!("{}", f);
            count += 1;
            if count >= 50 {
                println!("... (truncated)");
                break;
            }
        }
    }
    eprintln!("(matched {} total before truncation)", count);
}
