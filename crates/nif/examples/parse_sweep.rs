//! Count NIF parse failures across a BSA — regression guard for parser changes.
use byroredux_bsa::BsaArchive;

fn main() {
    let bsa = std::env::args().nth(1).expect("usage: <archive>");
    let archive = BsaArchive::open(&bsa).unwrap();
    let (mut ok, mut err) = (0usize, 0usize);
    for f in archive.list_files() {
        if !f.to_ascii_lowercase().ends_with(".nif") {
            continue;
        }
        let Ok(bytes) = archive.extract(f) else {
            continue;
        };
        match byroredux_nif::parse_nif(&bytes) {
            Ok(_) => ok += 1,
            Err(_) => {
                err += 1;
                if err <= 10 {
                    println!("  ERR {f}");
                }
            }
        }
    }
    println!("{bsa}: ok={ok} err={err}");
}
