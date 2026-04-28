//! D5 audit helper: list NIFs in a BA2 matching a pattern.
use byroredux_bsa::Ba2Archive;

fn main() {
    let path = std::env::args().nth(1).expect("usage: <archive> [pattern] [limit]");
    let pat = std::env::args().nth(2).unwrap_or_default().to_lowercase();
    let limit: usize = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let arc = Ba2Archive::open(&path).expect("open BA2");
    let mut emitted = 0usize;
    for f in arc.list_files() {
        let lower = f.to_lowercase();
        if !lower.ends_with(".nif") {
            continue;
        }
        if pat.is_empty() || lower.contains(&pat) {
            println!("{}", f);
            emitted += 1;
            if emitted >= limit {
                break;
            }
        }
    }
}
