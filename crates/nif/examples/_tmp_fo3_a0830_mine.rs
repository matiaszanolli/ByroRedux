//! TEMP scratch (audit 2026-08-30): confirm the FO3 PGRE mine meshes exist
//! and parse from Fallout - Meshes.bsa.
use byroredux_bsa::BsaArchive;

fn main() {
    let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout - Meshes.bsa";
    let arc = BsaArchive::open(path).expect("open");
    let files = arc.list_files();
    for want in [
        "weapons\\1handminedrop\\minefrag.nif",
        "weapons\\1handminedrop\\mineplasma.nif",
        "weapons\\1handminedrop\\minepulse.nif",
        "weapons\\1handminedrop\\minebottlecap.nif",
    ] {
        let hit = files
            .iter()
            .find(|f| f.to_ascii_lowercase().ends_with(want));
        match hit {
            Some(name) => {
                let bytes = arc.extract(name).expect("extract");
                let ok = byroredux_nif::parse_nif(&bytes).is_ok();
                println!("{name}: {} bytes, parse_ok={ok}", bytes.len());
            }
            None => println!("{want}: MISSING"),
        }
    }
}
