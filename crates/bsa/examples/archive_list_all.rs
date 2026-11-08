//! List every path of a BSA or BA2 (chosen by extension), optionally
//! filtered by a case-insensitive substring, one per line, no truncation.
//! M48 Phase 0 (#6643, GUI-REG-006 inventory): menu documents / movies.
//!
//! Usage: cargo run -p byroredux-bsa --example archive_list_all -- <archive> [needle]

use byroredux_bsa::{Ba2Archive, BsaArchive};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: archive_list_all <archive> [needle]");
    let needle = std::env::args().nth(2).unwrap_or_default().to_ascii_lowercase();
    let files: Vec<String> = if path.to_ascii_lowercase().ends_with(".ba2") {
        let a = Ba2Archive::open(&path).unwrap();
        a.list_files().into_iter().map(str::to_owned).collect()
    } else {
        let a = BsaArchive::open(&path).unwrap();
        a.list_files().into_iter().map(str::to_owned).collect()
    };
    for f in files {
        if needle.is_empty() || f.to_ascii_lowercase().contains(&needle) {
            println!("{f}");
        }
    }
}
