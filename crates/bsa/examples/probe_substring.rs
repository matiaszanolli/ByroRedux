//! List BSA files whose path contains a case-insensitive substring,
//! with no .nif filter (unlike `bsa_list`). Throwaway diagnostic for
//! the M44 footstep-sound survey.

use byroredux_bsa::BsaArchive;

fn main() {
    let path = std::env::args().nth(1).expect("usage: probe_substring <bsa> <substr>");
    let needle = std::env::args()
        .nth(2)
        .expect("missing substring")
        .to_ascii_lowercase();
    let bsa = BsaArchive::open(&path).expect("open BSA");
    for f in bsa.list_files() {
        if f.to_ascii_lowercase().contains(&needle) {
            println!("{f}");
        }
    }
}
