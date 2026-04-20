//! List .nif files inside a BSA matching a case-insensitive substring.
//! Short-lived probe helper for the FO3 audit (dim_5).

use byroredux_bsa::BsaArchive;

fn main() {
    let bsa = std::env::args().nth(1).expect("usage: bsa_list <bsa> <substr>");
    let needle = std::env::args()
        .nth(2)
        .expect("missing substring")
        .to_ascii_lowercase();
    let archive = BsaArchive::open(&bsa).unwrap();
    for f in archive.list_files() {
        let lower = f.to_ascii_lowercase();
        if lower.contains(&needle) && lower.ends_with(".nif") {
            println!("{}", f);
        }
    }
}
