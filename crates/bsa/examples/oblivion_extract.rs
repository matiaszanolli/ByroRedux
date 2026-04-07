//! Pull a specific file out of Oblivion's mesh BSA and write it to disk.
//! Used during the BSA v103 / NIF Oblivion follow-up (M26+) to investigate
//! parser failures on real Oblivion NIFs.

use byroredux_bsa::BsaArchive;

fn main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .try_init();

    let bsa_path = std::env::args()
        .nth(1)
        .expect("usage: oblivion_extract <bsa> <internal-path> [out.nif]");
    let internal = std::env::args().nth(2).expect("missing internal path");
    let out = std::env::args()
        .nth(3)
        .unwrap_or_else(|| "/tmp/obl_probe.nif".to_string());

    let archive = BsaArchive::open(&bsa_path).unwrap();
    println!("opened {} ({} files)", bsa_path, archive.file_count());

    let bytes = match archive.extract(&internal) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("extract failed: {}", e);
            std::process::exit(1);
        }
    };
    println!(
        "extracted {} bytes, first 20: {:02x?}",
        bytes.len(),
        &bytes[..20.min(bytes.len())]
    );
    std::fs::write(&out, &bytes).unwrap();
    println!("wrote {}", out);
}
