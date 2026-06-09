//! Quick BA2 path-grep helper. Scratch example for the F1 (FO4 NPC
//! skeleton not in archives) investigation — list all files in a BA2
//! whose lowercased path contains a substring.

use byroredux_bsa::Ba2Archive;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: ba2_grep <path.ba2> <needle>");
    let needle = std::env::args()
        .nth(2)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let archive = Ba2Archive::open(&path).unwrap();
    eprintln!(
        "BA2 v{} {:?}, {} files; searching for: {:?}",
        archive.version(),
        archive.variant(),
        archive.file_count(),
        needle
    );
    let mut count = 0;
    for f in archive.list_files() {
        let lower = f.to_ascii_lowercase();
        if needle.is_empty() || lower.contains(&needle) {
            println!("{}", f);
            count += 1;
        }
    }
    eprintln!("(matched {} files)", count);
}
