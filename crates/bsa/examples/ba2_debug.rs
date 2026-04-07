//! Quick BA2 sanity probe — open an archive and extract the first 3 files.
//! Used during M26 development; can be removed later.

// Despite the name, this debug example also handles BSA archives so we can
// poke at Oblivion v103 files alongside BA2 v1+. Use the file extension to
// route between BsaArchive and Ba2Archive.

use byroredux_bsa::{Ba2Archive, BsaArchive};

fn main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Trace)
        .try_init();

    let path = std::env::args().nth(1).expect("usage: ba2_debug <path.ba2>");
    let archive = Ba2Archive::open(&path).unwrap();
    println!(
        "BA2 v{} {:?}, {} files",
        archive.version(),
        archive.variant(),
        archive.file_count()
    );
    let want_ext = match archive.variant() {
        byroredux_bsa::Ba2Variant::General => ".nif",
        byroredux_bsa::Ba2Variant::Dx10 => ".dds",
    };
    let mut files: Vec<&str> = archive
        .list_files()
        .into_iter()
        .filter(|p| p.ends_with(want_ext))
        .collect();
    files.sort();
    for p in files.iter().take(3) {
        match archive.extract(p) {
            Ok(d) => {
                let head: Vec<u8> = d.iter().take(20).copied().collect();
                println!("OK   {} → {} bytes, head: {:02x?}", p, d.len(), head);
            }
            Err(e) => println!("FAIL {} → {}", p, e),
        }
    }

    // Specifically probe a known failure from the parse_real_nifs run, and
    // pipe it through the NIF parser so we see which side is at fault.
    let target = "meshes\\actors\\radtoad\\cookedleg.nif";
    println!("\nTargeted probe: {}", target);
    match archive.extract(target) {
        Ok(d) => {
            let head: Vec<u8> = d.iter().take(20).copied().collect();
            println!("extract OK → {} bytes, head: {:02x?}", d.len(), head);
            // First 80 bytes as ASCII for the header sanity check
            let preview: String = d
                .iter()
                .take(80)
                .map(|b| if (32..=126).contains(b) { *b as char } else { '.' })
                .collect();
            println!("  preview: {}", preview);
            std::fs::write("/tmp/ba2_probe.nif", &d).unwrap();
            println!("  wrote /tmp/ba2_probe.nif for offline inspection");
        }
        Err(e) => println!("extract FAIL → {}", e),
    }
}
